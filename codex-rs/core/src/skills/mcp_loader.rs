//! MCP-served skills loader.
//!
//! Implements the host side of the Skills-over-MCP extension
//! (`io.modelcontextprotocol/skills`). For each connected MCP server whose
//! config has `mcp_skills = true`, the loader reads `skill://index.json`,
//! fetches each `skill-md` entry's `SKILL.md`, and produces
//! [`SkillMetadata`] entries tagged with `SkillScope::Mcp`.
//!
//! The loader is intentionally permissive:
//!
//! - An absent or malformed `skill://index.json` is not fatal — the server
//!   may simply not enumerate its skills (per the SEP, hosts MUST NOT treat
//!   an empty index as proof the server has no skills).
//! - Unrecognized `type` values and `mcp-resource-template` entries are
//!   skipped (per the SEP's "clients SHOULD skip entries with an unrecognized
//!   type"). Template support is deliberately deferred.
//! - A server-level capability declaration is surfaced for observability but
//!   discovery is not gated on it, again per SEP guidance.

use std::collections::HashMap;
use std::time::Duration;

use codex_protocol::protocol::SkillScope;
use mcp_types::ReadResourceRequestParams;
use mcp_types::ReadResourceResult;
use mcp_types::ReadResourceResultContents;
use serde::Deserialize;
use tokio::time::timeout;
use tracing::debug;
use tracing::info;
use tracing::warn;

use crate::config::types::McpServerConfig;
use crate::mcp_connection_manager::McpConnectionManager;
use crate::skills::loader::parse_skill_frontmatter_text;
use crate::skills::model::SkillError;
use crate::skills::model::SkillMetadata;

/// Well-known URI for the Skills-over-MCP index resource.
pub(crate) const SKILL_INDEX_URI: &str = "skill://index.json";

/// MCP extension identifier declared by capability-supporting servers.
pub(crate) const SKILLS_EXTENSION_ID: &str = "io.modelcontextprotocol/skills";

/// Per-request timeout for `resources/read` calls issued during skill
/// discovery. Keeps a slow/hung MCP server from blocking session start.
const SKILL_READ_TIMEOUT: Duration = Duration::from_secs(10);

/// Maximum accepted length for a server-supplied `url` field. URLs from MCP
/// servers are pasted verbatim into `user_instructions` (rendered as
/// `(uri: ..., server: ...)`), so an unbounded value would let a malicious
/// or buggy server bloat prompt context.
const MAX_URI_LEN: usize = 2048;

#[derive(Debug, Default)]
pub(crate) struct McpSkillsOutcome {
    pub(crate) skills: Vec<SkillMetadata>,
    pub(crate) errors: Vec<SkillError>,
}

/// Minimum-viable schema for `skill://index.json`. Mirrors the Agent Skills
/// well-known URI index shape, with `url` holding a full MCP resource URI.
#[derive(Debug, Deserialize)]
struct IndexDocument {
    #[serde(default)]
    skills: Vec<IndexEntry>,
}

#[derive(Debug, Deserialize)]
struct IndexEntry {
    #[serde(default)]
    r#type: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    url: Option<String>,
}

/// Fetch skill manifests from every server in `mcp_configs` whose
/// configuration opts into skill discovery. The returned outcome aggregates
/// successes and per-skill errors across all servers. Servers and their
/// per-skill reads run concurrently.
pub(crate) async fn load_mcp_skill_manifests(
    conn: &McpConnectionManager,
    mcp_configs: &HashMap<String, McpServerConfig>,
) -> McpSkillsOutcome {
    let server_futs = mcp_configs
        .iter()
        .filter(|(_, cfg)| cfg.enabled && cfg.mcp_skills)
        .map(|(name, _)| load_from_server(conn, name.as_str()));

    let mut outcome = McpSkillsOutcome::default();
    for server_outcome in futures::future::join_all(server_futs).await {
        outcome.skills.extend(server_outcome.skills);
        outcome.errors.extend(server_outcome.errors);
    }
    // Stable ordering across sessions: HashMap iteration above is
    // nondeterministic, and filesystem skills are already sorted by
    // `load_skills_from_roots`. Match that behavior so `user_instructions`
    // diffs cleanly across runs.
    outcome.skills.sort_by(|a, b| {
        a.server_name
            .cmp(&b.server_name)
            .then_with(|| a.name.cmp(&b.name))
    });
    outcome
}

async fn load_from_server(conn: &McpConnectionManager, server_name: &str) -> McpSkillsOutcome {
    let index_params = ReadResourceRequestParams {
        uri: SKILL_INDEX_URI.to_owned(),
    };
    let index_result = match timeout(
        SKILL_READ_TIMEOUT,
        conn.read_resource(server_name, index_params),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(err)) => {
            // Per SEP: "Hosts MUST NOT treat an absent or empty index as
            // proof that a server has no skills." So this is informational.
            debug!("skills: server `{server_name}` did not return {SKILL_INDEX_URI}: {err:#}");
            return McpSkillsOutcome::default();
        }
        Err(_) => {
            warn!(
                "skills: server `{server_name}` timed out after {:?} reading {SKILL_INDEX_URI}",
                SKILL_READ_TIMEOUT
            );
            return McpSkillsOutcome::default();
        }
    };

    let Some(index_text) = first_text(&index_result) else {
        warn!("skills: server `{server_name}` returned non-text contents for {SKILL_INDEX_URI}");
        return McpSkillsOutcome::default();
    };

    let index: IndexDocument = match serde_json::from_str(index_text) {
        Ok(doc) => doc,
        Err(err) => {
            warn!("skills: server `{server_name}` returned malformed {SKILL_INDEX_URI}: {err:#}");
            return McpSkillsOutcome::default();
        }
    };

    info!(
        "skills: server `{server_name}` published index with {} entries",
        index.skills.len()
    );

    let entry_futs = index
        .skills
        .into_iter()
        .map(|entry| load_entry(conn, server_name, entry));

    let mut outcome = McpSkillsOutcome::default();
    for entry_result in futures::future::join_all(entry_futs).await {
        match entry_result {
            EntryOutcome::Skill(skill) => outcome.skills.push(skill),
            EntryOutcome::Error(err) => outcome.errors.push(err),
            EntryOutcome::Skipped => {}
        }
    }
    outcome
}

enum EntryOutcome {
    Skill(SkillMetadata),
    Error(SkillError),
    Skipped,
}

async fn load_entry(
    conn: &McpConnectionManager,
    server_name: &str,
    entry: IndexEntry,
) -> EntryOutcome {
    match entry.r#type.as_deref().unwrap_or("") {
        "skill-md" => {}
        "mcp-resource-template" => {
            debug!(
                "skills: server `{server_name}` advertised resource template entry (deferred): {entry:?}"
            );
            return EntryOutcome::Skipped;
        }
        other => {
            debug!(
                "skills: server `{server_name}` entry has unrecognized type `{other}`; skipping"
            );
            return EntryOutcome::Skipped;
        }
    }

    let Some(uri) = entry.url.clone() else {
        return EntryOutcome::Error(SkillError {
            path: std::path::PathBuf::from(format!("mcp://{server_name}/<index>")),
            message: "skill-md index entry missing required `url` field".to_owned(),
        });
    };

    if let Err(reason) = validate_entry_uri(&uri) {
        return EntryOutcome::Error(SkillError {
            path: std::path::PathBuf::from(format!("mcp://{server_name}/<index>")),
            message: format!("`{server_name}` advertised invalid skill-md url: {reason}"),
        });
    }

    let read_params = ReadResourceRequestParams { uri: uri.clone() };
    let read_result = match timeout(
        SKILL_READ_TIMEOUT,
        conn.read_resource(server_name, read_params),
    )
    .await
    {
        Ok(Ok(result)) => result,
        Ok(Err(err)) => {
            return EntryOutcome::Error(SkillError {
                path: std::path::PathBuf::from(&uri),
                message: format!("resources/read failed for `{server_name}` ({uri}): {err:#}"),
            });
        }
        Err(_) => {
            return EntryOutcome::Error(SkillError {
                path: std::path::PathBuf::from(&uri),
                message: format!(
                    "resources/read timed out after {SKILL_READ_TIMEOUT:?} for `{server_name}` ({uri})"
                ),
            });
        }
    };

    let Some(body) = first_text(&read_result) else {
        return EntryOutcome::Error(SkillError {
            path: std::path::PathBuf::from(&uri),
            message: format!("`{server_name}` returned non-text content for `{uri}`"),
        });
    };

    match parse_skill_frontmatter_text(body) {
        Ok(parsed) => {
            // The SEP requires the SKILL.md path's final segment to equal the
            // frontmatter `name`; the index entry `name` is advisory. We log
            // mismatches so skill authors notice but still load the skill.
            if let Some(entry_name) = entry.name.as_deref()
                && entry_name != parsed.name
            {
                warn!(
                    "skills: `{server_name}` index entry name `{entry_name}` differs from SKILL.md frontmatter `{}`",
                    parsed.name
                );
            }

            // The SKILL.md frontmatter `description` is the authoritative,
            // length-validated value. The index entry's `description` is
            // advisory only (like its `name`) and is deliberately NOT used —
            // preferring it would bypass `MAX_DESCRIPTION_LEN` validation and
            // admit unbounded server-controlled text into `user_instructions`.
            EntryOutcome::Skill(SkillMetadata {
                name: parsed.name,
                description: parsed.description,
                short_description: parsed.short_description,
                path: std::path::PathBuf::from(&uri),
                scope: SkillScope::Mcp,
                uri: Some(uri),
                server_name: Some(server_name.to_owned()),
            })
        }
        Err(message) => EntryOutcome::Error(SkillError {
            path: std::path::PathBuf::from(&uri),
            message,
        }),
    }
}

/// Validate a server-supplied skill-md URL before we echo it into
/// `user_instructions` or hand it to `resources/read`. Keeps us defensive
/// against untrusted MCP servers without being strict about URI schemes:
/// the SEP allows domain-native schemes (e.g. `github://`), so we only
/// enforce a plausible `scheme:` prefix, bounded length, and no control
/// characters.
fn validate_entry_uri(uri: &str) -> Result<(), String> {
    if uri.is_empty() {
        return Err("url is empty".to_owned());
    }
    if uri.len() > MAX_URI_LEN {
        return Err(format!(
            "url is {} bytes; limit is {MAX_URI_LEN}",
            uri.len()
        ));
    }
    if uri.chars().any(char::is_control) {
        return Err("url contains control characters".to_owned());
    }
    let Some(colon) = uri.find(':') else {
        return Err("url is missing a scheme (expected e.g. `skill://...`)".to_owned());
    };
    let scheme = &uri[..colon];
    if scheme.is_empty() {
        return Err("url scheme is empty".to_owned());
    }
    let mut scheme_chars = scheme.chars();
    let first_ok = scheme_chars.next().is_some_and(|c| c.is_ascii_alphabetic());
    let rest_ok = scheme_chars
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !first_ok || !rest_ok {
        return Err(format!("url scheme `{scheme}` is not a valid URI scheme"));
    }
    Ok(())
}

fn first_text(result: &ReadResourceResult) -> Option<&str> {
    result.contents.iter().find_map(|content| match content {
        ReadResourceResultContents::TextResourceContents(text) => Some(text.text.as_str()),
        ReadResourceResultContents::BlobResourceContents(_) => None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_document_parses_canonical_example() {
        let src = r#"{
            "$schema": "https://schemas.agentskills.io/discovery/0.2.0/schema.json",
            "skills": [
                {
                    "name": "pull-requests",
                    "type": "skill-md",
                    "description": "Review GitHub PRs",
                    "url": "skill://pull-requests/SKILL.md"
                },
                {
                    "type": "mcp-resource-template",
                    "description": "Per-product docs",
                    "url": "skill://docs/{product}/SKILL.md"
                }
            ]
        }"#;
        let doc: IndexDocument = serde_json::from_str(src).expect("parses");
        assert_eq!(doc.skills.len(), 2);
        assert_eq!(doc.skills[0].r#type.as_deref(), Some("skill-md"));
        assert_eq!(doc.skills[0].name.as_deref(), Some("pull-requests"));
        assert_eq!(
            doc.skills[0].url.as_deref(),
            Some("skill://pull-requests/SKILL.md")
        );
        assert_eq!(
            doc.skills[1].r#type.as_deref(),
            Some("mcp-resource-template")
        );
    }

    #[test]
    fn index_document_tolerates_unknown_fields() {
        let src = r#"{
            "$schema": "x",
            "extra": 42,
            "skills": [
                {
                    "type": "skill-md",
                    "name": "pr",
                    "description": "d",
                    "url": "skill://pr/SKILL.md",
                    "unknown_field": true
                }
            ]
        }"#;
        let doc: IndexDocument = serde_json::from_str(src).expect("parses");
        assert_eq!(doc.skills.len(), 1);
    }

    #[test]
    fn index_document_tolerates_empty_skills() {
        let src = r#"{ "$schema": "x", "skills": [] }"#;
        let doc: IndexDocument = serde_json::from_str(src).expect("parses");
        assert!(doc.skills.is_empty());
    }

    #[test]
    fn validate_entry_uri_accepts_canonical_skill_scheme() {
        assert!(validate_entry_uri("skill://pull-requests/SKILL.md").is_ok());
    }

    #[test]
    fn validate_entry_uri_accepts_domain_native_scheme() {
        assert!(validate_entry_uri("github://skills/review/SKILL.md").is_ok());
    }

    #[test]
    fn validate_entry_uri_rejects_empty() {
        assert!(validate_entry_uri("").is_err());
    }

    #[test]
    fn validate_entry_uri_rejects_missing_scheme() {
        assert!(validate_entry_uri("pull-requests/SKILL.md").is_err());
    }

    #[test]
    fn validate_entry_uri_rejects_control_chars() {
        assert!(validate_entry_uri("skill://has\nnewline").is_err());
        assert!(validate_entry_uri("skill://has\ttab").is_err());
        assert!(validate_entry_uri("skill://has\0null").is_err());
    }

    #[test]
    fn validate_entry_uri_rejects_invalid_scheme() {
        assert!(validate_entry_uri("1bad://x").is_err());
        assert!(validate_entry_uri("://x").is_err());
        assert!(validate_entry_uri("bad scheme://x").is_err());
    }

    #[test]
    fn validate_entry_uri_rejects_too_long() {
        let long = format!("skill://{}", "a".repeat(MAX_URI_LEN));
        assert!(validate_entry_uri(&long).is_err());
    }

    #[test]
    fn first_text_prefers_text_over_blob() {
        use mcp_types::BlobResourceContents;
        use mcp_types::TextResourceContents;

        let result = ReadResourceResult {
            contents: vec![
                ReadResourceResultContents::BlobResourceContents(BlobResourceContents {
                    blob: "abc".to_owned(),
                    mime_type: Some("application/octet-stream".to_owned()),
                    uri: "x".to_owned(),
                }),
                ReadResourceResultContents::TextResourceContents(TextResourceContents {
                    mime_type: Some("text/markdown".to_owned()),
                    text: "---\nname: x\n---\n".to_owned(),
                    uri: "x".to_owned(),
                }),
            ],
        };
        assert_eq!(first_text(&result), Some("---\nname: x\n---\n"));
    }
}

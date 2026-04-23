use crate::skills::model::SkillMetadata;

pub fn render_skills_section(skills: &[SkillMetadata]) -> Option<String> {
    if skills.is_empty() {
        return None;
    }

    let has_mcp_skill = skills.iter().any(|skill| skill.uri.is_some());

    let mut lines: Vec<String> = Vec::new();
    lines.push("## Skills".to_string());
    lines.push("These skills are discovered at startup from multiple local sources. Each entry includes a name, description, and file path so you can open the source for full instructions.".to_string());

    if has_mcp_skill {
        lines.push(
            concat!(
                "Some skills are served by connected MCP servers rather than the filesystem. ",
                "Those entries show `(uri: <scheme>://..., server: <name>)` instead of ",
                "`(file: ...)`; the scheme is usually `skill://` but servers MAY use a ",
                "domain-native scheme (for example `github://`). To read such a skill, call ",
                "the `read_mcp_resource` tool with the exact `server` and `uri` from the ",
                "catalog entry — do not rewrite the URI. Relative references inside a ",
                "URI-backed SKILL.md (e.g. `references/GUIDE.md`) resolve against the ",
                "skill's directory: strip the trailing `SKILL.md` and append the relative ",
                "path.",
            )
            .to_string(),
        );
    }

    for skill in skills {
        let name = skill.name.as_str();
        let description = skill.description.as_str();
        if let Some((uri, server)) = skill.uri.as_deref().zip(skill.server_name.as_deref()) {
            lines.push(format!(
                "- {name}: {description} (uri: {uri}, server: {server})"
            ));
        } else {
            let path_str = skill.path.to_string_lossy().replace('\\', "/");
            lines.push(format!("- {name}: {description} (file: {path_str})"));
        }
    }

    lines.push(
        r###"- Discovery: Available skills are listed in project docs and may also appear in a runtime "## Skills" section (name + description + file path). These are the sources of truth; skill bodies live on disk at the listed paths.
- Trigger rules: If the user names a skill (with `$SkillName` or plain text) OR the task clearly matches a skill's description, you must use that skill for that turn. Multiple mentions mean use them all. Do not carry skills across turns unless re-mentioned.
- Missing/blocked: If a named skill isn't in the list or the path can't be read, say so briefly and continue with the best fallback.
- How to use a skill (progressive disclosure):
  1) After deciding to use a skill, open its `SKILL.md`. Read only enough to follow the workflow.
  2) If `SKILL.md` points to extra folders such as `references/`, load only the specific files needed for the request; don't bulk-load everything.
  3) If `scripts/` exist, prefer running or patching them instead of retyping large code blocks.
  4) If `assets/` or templates exist, reuse them instead of recreating from scratch.
- Description as trigger: The YAML `description` in `SKILL.md` is the primary trigger signal; rely on it to decide applicability. If unsure, ask a brief clarification before proceeding.
- Coordination and sequencing:
  - If multiple skills apply, choose the minimal set that covers the request and state the order you'll use them.
  - Announce which skill(s) you're using and why (one short line). If you skip an obvious skill, say why.
- Context hygiene:
  - Keep context small: summarize long sections instead of pasting them; only load extra files when needed.
  - Avoid deeply nested references; prefer one-hop files explicitly linked from `SKILL.md`.
  - When variants exist (frameworks, providers, domains), pick only the relevant reference file(s) and note that choice.
- Safety and fallback: If a skill can't be applied cleanly (missing files, unclear instructions), state the issue, pick the next-best approach, and continue."###
            .to_string(),
    );

    Some(lines.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_protocol::protocol::SkillScope;
    use std::path::PathBuf;

    fn fs_skill(name: &str, description: &str, path: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: description.to_string(),
            short_description: None,
            path: PathBuf::from(path),
            scope: SkillScope::User,
            uri: None,
            server_name: None,
        }
    }

    fn mcp_skill(name: &str, description: &str, uri: &str, server: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: description.to_string(),
            short_description: None,
            path: PathBuf::from(uri),
            scope: SkillScope::Mcp,
            uri: Some(uri.to_string()),
            server_name: Some(server.to_string()),
        }
    }

    #[test]
    fn returns_none_when_empty() {
        assert!(render_skills_section(&[]).is_none());
    }

    #[test]
    fn filesystem_only_does_not_emit_mcp_preamble() {
        let skills = vec![fs_skill(
            "git-flow",
            "manage commits",
            "/skills/git/SKILL.md",
        )];
        let rendered = render_skills_section(&skills).expect("non-empty");
        assert!(rendered.contains("- git-flow: manage commits (file: /skills/git/SKILL.md)"));
        assert!(!rendered.contains("read_mcp_resource"));
        assert!(!rendered.contains("uri:"));
    }

    #[test]
    fn mcp_only_emits_uri_line_and_preamble() {
        let skills = vec![mcp_skill(
            "pull-requests",
            "review PRs",
            "skill://pull-requests/SKILL.md",
            "github-skills",
        )];
        let rendered = render_skills_section(&skills).expect("non-empty");
        assert!(rendered.contains("read_mcp_resource"));
        assert!(rendered.contains(
            "- pull-requests: review PRs (uri: skill://pull-requests/SKILL.md, server: github-skills)"
        ));
    }

    #[test]
    fn mixed_sources_render_each_kind_with_matching_suffix() {
        let skills = vec![
            fs_skill("local", "local skill", "/skills/local/SKILL.md"),
            mcp_skill("remote", "remote skill", "skill://remote/SKILL.md", "srv"),
        ];
        let rendered = render_skills_section(&skills).expect("non-empty");
        assert!(rendered.contains("- local: local skill (file: /skills/local/SKILL.md)"));
        assert!(
            rendered.contains("- remote: remote skill (uri: skill://remote/SKILL.md, server: srv)")
        );
        assert!(rendered.contains("read_mcp_resource"));
    }
}

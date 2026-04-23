use std::collections::HashSet;
use std::path::PathBuf;

use codex_protocol::protocol::SkillScope;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillMetadata {
    pub name: String,
    pub description: String,
    pub short_description: Option<String>,
    /// For filesystem skills, the absolute path to SKILL.md. For MCP-served
    /// skills, the `skill://` URI of SKILL.md, stored here so existing
    /// display paths continue to work.
    pub path: PathBuf,
    pub scope: SkillScope,
    /// `Some` for MCP-served skills. Equal to the `skill://` URI of the
    /// skill's SKILL.md resource. `None` for filesystem skills.
    pub uri: Option<String>,
    /// `Some` for MCP-served skills. The name of the MCP server from which
    /// this skill's index entry was fetched. `None` for filesystem skills.
    pub server_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkillError {
    pub path: PathBuf,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct SkillLoadOutcome {
    pub skills: Vec<SkillMetadata>,
    pub errors: Vec<SkillError>,
}

impl SkillLoadOutcome {
    /// Append MCP-served skills into this outcome with filesystem-wins
    /// precedence on name collision. Returns the names of MCP skills that
    /// were masked by an existing entry so callers can log them.
    pub fn add_mcp_skills<I>(&mut self, mcp_skills: I) -> Vec<String>
    where
        I: IntoIterator<Item = SkillMetadata>,
    {
        let existing: HashSet<String> =
            self.skills.iter().map(|skill| skill.name.clone()).collect();
        let mut masked = Vec::new();
        for skill in mcp_skills {
            if existing.contains(&skill.name) {
                masked.push(skill.name.clone());
            } else {
                self.skills.push(skill);
            }
        }
        masked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fs_skill(name: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: String::new(),
            short_description: None,
            path: PathBuf::from(format!("/skills/{name}/SKILL.md")),
            scope: SkillScope::Repo,
            uri: None,
            server_name: None,
        }
    }

    fn mcp_skill(name: &str, server: &str) -> SkillMetadata {
        SkillMetadata {
            name: name.to_string(),
            description: String::new(),
            short_description: None,
            path: PathBuf::from(format!("skill://{name}/SKILL.md")),
            scope: SkillScope::Mcp,
            uri: Some(format!("skill://{name}/SKILL.md")),
            server_name: Some(server.to_string()),
        }
    }

    #[test]
    fn add_mcp_skills_filesystem_wins_and_reports_masked() {
        let mut outcome = SkillLoadOutcome {
            skills: vec![fs_skill("alpha"), fs_skill("beta")],
            errors: Vec::new(),
        };

        let masked = outcome.add_mcp_skills(vec![
            mcp_skill("alpha", "srv"),
            mcp_skill("gamma", "srv"),
        ]);

        assert_eq!(masked, vec!["alpha".to_string()]);
        let names: Vec<&str> = outcome.skills.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(names, vec!["alpha", "beta", "gamma"]);
        let alpha = outcome
            .skills
            .iter()
            .find(|s| s.name == "alpha")
            .expect("alpha present");
        assert!(alpha.uri.is_none(), "filesystem alpha must survive merge");
    }

    #[test]
    fn add_mcp_skills_with_empty_input_is_noop() {
        let mut outcome = SkillLoadOutcome {
            skills: vec![fs_skill("alpha")],
            errors: Vec::new(),
        };
        let masked = outcome.add_mcp_skills(std::iter::empty());
        assert!(masked.is_empty());
        assert_eq!(outcome.skills.len(), 1);
    }
}

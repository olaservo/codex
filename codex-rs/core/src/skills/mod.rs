pub mod injection;
pub mod loader;
pub mod manager;
pub mod mcp_loader;
pub mod model;
pub mod render;
pub mod system;

pub(crate) use injection::SkillInjections;
pub(crate) use injection::build_skill_injections;
pub use loader::load_skills;
pub use manager::SkillsManager;
pub(crate) use mcp_loader::SKILLS_EXTENSION_ID;
pub(crate) use mcp_loader::load_mcp_skill_manifests;
pub use model::SkillError;
pub use model::SkillLoadOutcome;
pub use model::SkillMetadata;
pub use render::render_skills_section;

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillManifest {
    pub name: String,
    pub description: String,
    pub version: String,
    pub triggers: Vec<String>,
    pub parameters: Vec<SkillParameter>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillParameter {
    pub name: String,
    pub description: String,
    pub required: bool,
    #[serde(default)]
    pub default: Option<String>,
}

#[derive(Debug, Clone)]
pub struct Skill {
    pub manifest: SkillManifest,
    pub path: PathBuf,
}

impl Skill {
    pub fn matches(&self, prompt: &str) -> bool {
        let prompt_lower = prompt.to_lowercase();
        self.manifest
            .triggers
            .iter()
            .any(|trigger| prompt_lower.contains(&trigger.to_lowercase()))
    }
}

pub struct SkillLoader;

impl SkillLoader {
    pub fn global_skills_dir() -> PathBuf {
        dirs::data_local_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("multilink")
            .join("skills")
            .join("global")
    }

    pub fn project_skills_dir(project_root: &PathBuf) -> PathBuf {
        project_root.join(".multilink").join("skills")
    }

    pub fn load_skills_from_dir(dir: &PathBuf) -> Vec<Skill> {
        if !dir.exists() {
            return Vec::new();
        }

        let mut skills = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.extension().map_or(false, |e| e == "toml") {
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(manifest) = toml::from_str::<SkillManifest>(&content) {
                            skills.push(Skill { manifest, path });
                        }
                    }
                }
            }
        }
        skills
    }

    pub fn load_all(project_root: Option<&PathBuf>) -> Vec<Skill> {
        let mut skills = Vec::new();

        let global_dir = Self::global_skills_dir();
        skills.extend(Self::load_skills_from_dir(&global_dir));

        if let Some(root) = project_root {
            let project_dir = Self::project_skills_dir(root);
            skills.extend(Self::load_skills_from_dir(&project_dir));
        }

        skills
    }
}

pub struct SkillOrchestrator {
    skills: Vec<Skill>,
}

impl SkillOrchestrator {
    pub fn new(skills: Vec<Skill>) -> Self {
        Self { skills }
    }

    pub fn find_matching_skill(&self, prompt: &str) -> Option<&Skill> {
        self.skills.iter().find(|skill| skill.matches(prompt))
    }

    pub fn all_skills(&self) -> &[Skill] {
        &self.skills
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn skill_matches_trigger() {
        let skill = Skill {
            manifest: SkillManifest {
                name: "test".to_string(),
                description: "test skill".to_string(),
                version: "1.0".to_string(),
                triggers: vec!["test".to_string()],
                parameters: vec![],
            },
            path: PathBuf::from("/test.toml"),
        };

        assert!(skill.matches("this is a test prompt"));
        assert!(skill.matches("TEST prompt"));
        assert!(!skill.matches("hello world"));
    }
}

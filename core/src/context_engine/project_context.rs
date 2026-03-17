use serde::{Deserialize, Serialize};
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ProjectMeta {
    pub stack: Vec<String>,
    #[serde(rename = "type")]
    pub project_type: String,
    pub has_tests: bool,
    pub has_docs: bool,
    pub has_docker: bool,
    pub has_linting: bool,
    pub complexity: String,
    pub paths: Vec<String>,
}

pub fn load_project_context(root: &Path) -> Option<String> {
    let path = root.join("MULTILINK.md");
    std::fs::read_to_string(&path).ok()
}

pub fn parse_project_meta(content: &str) -> Option<ProjectMeta> {
    let json_start = content.find("```json")?;
    let json_end = content[json_start..].find("```")?;
    let json_str = &content[json_start + 7..json_start + json_end];

    serde_json::from_str(json_str).ok()
}

pub fn get_project_context_chunk(root: &Path) -> Option<String> {
    let content = load_project_context(root)?;
    let meta = parse_project_meta(&content)?;

    let summary = format!(
        "Project type: {}, Complexity: {}, Stack: {}",
        meta.project_type,
        meta.complexity,
        meta.stack.join(", ")
    );

    Some(summary)
}

pub fn inject_project_context(context: &str, project_root: &Path, boost: bool) -> String {
    let project_chunk = get_project_context_chunk(project_root);

    let header = if boost {
        "## 🏗️ Project Context (High Priority)\n"
    } else {
        "## 🏗️ Project Context\n"
    };

    match project_chunk {
        Some(chunk) => {
            format!("{}{}\n\n{}", header, chunk, context)
        }
        None => context.to_string(),
    }
}

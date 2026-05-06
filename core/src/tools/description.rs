use std::collections::HashMap;
use std::sync::OnceLock;

use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct ToolDescription {
    pub name: String,
    pub description: String,
    pub when_to_use: String,
    pub when_not_to_use: String,
    pub examples: Vec<String>,
    pub schema: serde_json::Value,
}

impl ToolDescription {
    pub fn to_full_prompt(&self) -> String {
        format!(
            r"### Tool: {}
**Description:** {}
**Use when:** {}
**DO NOT use when:** {}
**Examples:**
{}
**Schema:**
{}",
            self.name,
            self.description,
            self.when_to_use,
            self.when_not_to_use,
            self.examples.join("\n"),
            serde_json::to_string_pretty(&self.schema).unwrap_or_default()
        )
    }
}

static TOOL_DESCRIPTIONS: OnceLock<HashMap<String, ToolDescription>> = OnceLock::new();

fn prompts_dir() -> String {
    format!(
        "{}/src/tools/prompts",
        env!("CARGO_MANIFEST_DIR")
    )
}

pub fn load_all_descriptions() -> &'static HashMap<String, ToolDescription> {
    TOOL_DESCRIPTIONS.get_or_init(|| {
        let base = prompts_dir();
        let mut map = HashMap::new();
        for name in &[
            "search_code",
            "open_file",
            "search_and_open",
            "fs_ls",
            "fs_cat",
            "fs_grep",
            "write_file",
            "apply_patch",
            "run_command",
            "git_status",
            "git_diff",
            "system_version",
        ] {
            let path = format!("{}/{}.txt", base, name);
            let content = std::fs::read_to_string(&path)
                .unwrap_or_else(|_| panic!("Tool prompt file not found: {}", path));
            match parse_tool_description(name, &content) {
                Ok(desc) => {
                    map.insert(name.to_string(), desc);
                }
                Err(e) => {
                    eprintln!("[tool_descriptions] warning: failed to parse {}: {}", name, e);
                }
            }
        }
        map
    })
}

pub fn get_description(name: &str) -> Option<&'static ToolDescription> {
    load_all_descriptions().get(name)
}

pub fn list_available_tools() -> Vec<String> {
    load_all_descriptions().keys().cloned().collect()
}

fn parse_tool_description(name: &str, content: &str) -> Result<ToolDescription, String> {
    let mut description = String::new();
    let mut when_to_use = String::new();
    let mut when_not_to_use = String::new();
    let mut examples = Vec::new();
    let mut schema = serde_json::Value::Null;
    let mut current_section = String::new();
    let mut in_schema = false;
    let mut schema_text = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("TOOL:") {
            continue;
        }
        if trimmed.starts_with("DESCRIPTION:") {
            current_section = "description".to_string();
            in_schema = false;
            continue;
        }
        if trimmed.starts_with("WHEN TO USE:") {
            current_section = "when_to_use".to_string();
            in_schema = false;
            continue;
        }
        if trimmed.starts_with("DO NOT USE WHEN:") {
            current_section = "when_not_to_use".to_string();
            in_schema = false;
            continue;
        }
        if trimmed.starts_with("EXAMPLES:") {
            current_section = "examples".to_string();
            in_schema = false;
            continue;
        }
        if trimmed.starts_with("SCHEMA:") {
            current_section = "schema".to_string();
            in_schema = true;
            continue;
        }
        if in_schema {
            if trimmed.starts_with("VALIDATION:")
                || trimmed.starts_with("VALIDATION:")
            {
                in_schema = false;
                current_section = String::new();
            } else {
                schema_text.push_str(line);
                schema_text.push('\n');
                continue;
            }
        }
        match current_section.as_str() {
            "description" if !trimmed.is_empty() => {
                if !description.is_empty() {
                    description.push(' ');
                }
                description.push_str(trimmed);
            }
            "when_to_use" => {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    when_to_use.push_str(item);
                    when_to_use.push('\n');
                }
            }
            "when_not_to_use" => {
                if let Some(item) = trimmed.strip_prefix("- ") {
                    when_not_to_use.push_str(item);
                    when_not_to_use.push('\n');
                }
            }
            "examples" if trimmed.starts_with("-> ") => {
                examples.push(trimmed.to_string());
            }
            _ => {}
        }
    }

    if !schema_text.is_empty() {
        schema = serde_json::from_str(&schema_text)
            .map_err(|e| format!("invalid JSON schema for {}: {}", name, e))?;
    }

    Ok(ToolDescription {
        name: name.to_string(),
        description: description.trim().to_string(),
        when_to_use: when_to_use.trim().to_string(),
        when_not_to_use: when_not_to_use.trim().to_string(),
        examples,
        schema,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_search_code_description() {
        let descriptions = load_all_descriptions();
        let desc = descriptions.get("search_code").expect("search_code should be loaded");
        assert!(!desc.description.is_empty());
        assert!(!desc.when_to_use.is_empty());
        assert!(!desc.when_not_to_use.is_empty());
        assert!(!desc.examples.is_empty());
        assert!(desc.schema.is_object());
    }

    #[test]
    fn test_all_tools_loadable() {
        let descriptions = load_all_descriptions();
        for name in &[
            "search_code", "open_file", "write_file", "apply_patch",
            "run_command", "fs_grep", "fs_ls", "fs_cat",
            "git_status", "git_diff", "system_version", "search_and_open",
        ] {
            assert!(
                descriptions.contains_key(*name),
                "tool description missing: {}",
                name
            );
        }
    }

    #[test]
    fn test_tool_description_to_full_prompt() {
        let descriptions = load_all_descriptions();
        let desc = descriptions.get("search_code").unwrap();
        let prompt = desc.to_full_prompt();
        assert!(prompt.contains("Tool: search_code"));
        assert!(prompt.contains("Use when:"));
        assert!(prompt.contains("DO NOT use when:"));
    }

    #[test]
    fn test_list_available_tools() {
        let tools = list_available_tools();
        assert!(tools.len() >= 12);
        assert!(tools.contains(&"search_code".to_string()));
    }
}

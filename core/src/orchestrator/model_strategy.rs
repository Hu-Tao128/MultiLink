use crate::tools::description::ToolDescription;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelSize {
    Small,
    Medium,
    Large,
}

impl ModelSize {
    pub fn from_string(s: &str) -> Self {
        match s.trim().to_lowercase().as_str() {
            "small" => ModelSize::Small,
            "medium" => ModelSize::Medium,
            "large" => ModelSize::Large,
            _ => ModelSize::Medium,
        }
    }

    pub fn from_model_name(name: &str) -> Self {
        let lower = name.to_ascii_lowercase();

        if lower.contains("embed")
            || lower.contains("nomic")
            || lower.contains("mxbai")
            || lower.contains("bge-")
        {
            return ModelSize::Small;
        }

        if let Some(size) = extract_size_billions(&lower) {
            if size < 4.0 {
                ModelSize::Small
            } else if size <= 14.0 {
                ModelSize::Medium
            } else {
                ModelSize::Large
            }
        } else {
            ModelSize::Medium
        }
    }

    pub fn allowed_tools(&self) -> Vec<&'static str> {
        match self {
            ModelSize::Small => vec![
                "search_code",
                "open_file",
                "fs_ls",
                "fs_cat",
                "fs_grep",
                "write_file",
            ],
            ModelSize::Medium => vec![
                "search_code",
                "open_file",
                "fs_ls",
                "fs_cat",
                "fs_grep",
                "write_file",
                "apply_patch",
                "run_command",
            ],
            ModelSize::Large => vec![
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
            ],
        }
    }

    pub fn execution_config(&self) -> ExecutionConfig {
        match self {
            ModelSize::Small => ExecutionConfig {
                max_steps: 5,
                max_retries: 1,
                require_validation: false,
                tool_timeout_secs: 30,
            },
            ModelSize::Medium => ExecutionConfig {
                max_steps: 10,
                max_retries: 2,
                require_validation: true,
                tool_timeout_secs: 60,
            },
            ModelSize::Large => ExecutionConfig {
                max_steps: 15,
                max_retries: 3,
                require_validation: true,
                tool_timeout_secs: 120,
            },
        }
    }

    pub fn build_tool_prompt(&self, tool: &ToolDescription) -> String {
        match self {
            ModelSize::Small => {
                format!(
                    r"Tool: {}
Use when: {}
Never use when: {}
Args: {}",
                    tool.name,
                    tool.when_to_use,
                    tool.when_not_to_use,
                    serde_json::to_string(&tool.schema).unwrap_or_default()
                )
            }
            ModelSize::Medium => {
                format!(
                    r"### {}
{}

**Use when:** {}
**Avoid when:** {}

**Schema:** {}

**Examples:** {}",
                    tool.name,
                    tool.description,
                    tool.when_to_use,
                    tool.when_not_to_use,
                    serde_json::to_string_pretty(&tool.schema).unwrap_or_default(),
                    tool.examples.join("\n")
                )
            }
            ModelSize::Large => tool.to_full_prompt(),
        }
    }
}

fn extract_size_billions(name: &str) -> Option<f32> {
    let bytes = name.as_bytes();
    let len = bytes.len();
    let mut i = 0;
    while i < len {
        if bytes[i].is_ascii_digit() {
            let start = i;
            i += 1;
            while i < len && (bytes[i].is_ascii_digit() || bytes[i] == b'.') {
                i += 1;
            }
            if i < len && bytes[i] == b'b' {
                return name[start..i].parse::<f32>().ok();
            }
        } else {
            i += 1;
        }
    }
    None
}

#[derive(Debug, Clone)]
pub struct ExecutionConfig {
    pub max_steps: usize,
    pub max_retries: u32,
    pub require_validation: bool,
    pub tool_timeout_secs: u64,
}

impl Default for ExecutionConfig {
    fn default() -> Self {
        ModelSize::Medium.execution_config()
    }
}

pub struct ModelStrategy;

impl ModelStrategy {
    pub fn get_allowed_tools(model_size: ModelSize) -> Vec<&'static str> {
        model_size.allowed_tools()
    }

    pub fn get_execution_config(model_size: ModelSize) -> ExecutionConfig {
        model_size.execution_config()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_size_from_string() {
        assert_eq!(ModelSize::from_string("small"), ModelSize::Small);
        assert_eq!(ModelSize::from_string("medium"), ModelSize::Medium);
        assert_eq!(ModelSize::from_string("large"), ModelSize::Large);
        assert_eq!(ModelSize::from_string("SMALL"), ModelSize::Small);
    }

    #[test]
    fn test_from_string_unknown_defaults_to_medium() {
        assert_eq!(ModelSize::from_string("unknown"), ModelSize::Medium);
        assert_eq!(ModelSize::from_string(""), ModelSize::Medium);
    }

    #[test]
    fn test_from_model_name_by_parameter_count() {
        assert_eq!(
            ModelSize::from_model_name("qwen2.5-coder:3b"),
            ModelSize::Small
        );
        assert_eq!(
            ModelSize::from_model_name("qwen2.5-coder:7b"),
            ModelSize::Medium
        );
        assert_eq!(
            ModelSize::from_model_name("qwen2.5-coder:14b"),
            ModelSize::Medium
        );
        assert_eq!(
            ModelSize::from_model_name("llama3.1:70b"),
            ModelSize::Large
        );
    }

    #[test]
    fn test_from_model_name_embed_models_are_small() {
        assert_eq!(
            ModelSize::from_model_name("nomic-embed-text:v1.5"),
            ModelSize::Small
        );
        assert_eq!(
            ModelSize::from_model_name("mxbai-embed-large:latest"),
            ModelSize::Small
        );
        assert_eq!(
            ModelSize::from_model_name("bge-m3:latest"),
            ModelSize::Small
        );
    }

    #[test]
    fn test_from_model_name_no_number_defaults_medium() {
        assert_eq!(
            ModelSize::from_model_name("gemma4:latest"),
            ModelSize::Medium
        );
        assert_eq!(
            ModelSize::from_model_name("mistral:latest"),
            ModelSize::Medium
        );
    }

    #[test]
    fn test_small_model_has_write_file_no_run_command() {
        let tools = ModelSize::Small.allowed_tools();
        assert!(tools.contains(&"search_code"));
        assert!(tools.contains(&"open_file"));
        assert!(tools.contains(&"write_file"));
        assert!(!tools.contains(&"run_command"));
        assert!(!tools.contains(&"apply_patch"));
        assert!(!tools.contains(&"git_status"));
    }

    #[test]
    fn test_medium_model_has_write_tools() {
        let tools = ModelSize::Medium.allowed_tools();
        assert!(tools.contains(&"write_file"));
        assert!(tools.contains(&"apply_patch"));
        assert!(tools.contains(&"run_command"));
        assert!(!tools.contains(&"git_diff"));
    }

    #[test]
    fn test_large_model_has_all_tools() {
        let tools = ModelSize::Large.allowed_tools();
        assert!(tools.contains(&"git_status"));
        assert!(tools.contains(&"git_diff"));
        assert!(tools.contains(&"system_version"));
        assert!(tools.contains(&"search_and_open"));
    }

    #[test]
    fn test_execution_config_differs_by_size() {
        let small = ModelSize::Small.execution_config();
        let large = ModelSize::Large.execution_config();
        assert!(small.max_steps < large.max_steps);
        assert!(small.max_retries < large.max_retries);
    }

    #[test]
    fn test_extract_size_billions() {
        assert_eq!(extract_size_billions("qwen2.5-coder:3b"), Some(3.0));
        assert_eq!(extract_size_billions("qwen2.5-coder:14b"), Some(14.0));
        assert_eq!(extract_size_billions("llama3.1:70b"), Some(70.0));
        assert_eq!(extract_size_billions("gemma4:latest"), None);
        assert_eq!(extract_size_billions("nomic-embed-text:v1.5"), None);
    }
}

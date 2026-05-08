use async_trait::async_trait;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub mod backup;
pub mod command;
pub mod description;
pub mod filesystem;
pub mod git;
pub mod hybrid;
pub mod patch;
pub mod permissions;
pub mod system;
pub mod write_file;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct ToolInput {
    pub path: Option<String>,
    pub pattern: Option<String>,
    pub args: Option<HashMap<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult {
    pub success: bool,
    pub output: serde_json::Value,
    pub error: Option<String>,
}

impl ToolResult {
    pub fn ok(output: impl Into<serde_json::Value>) -> Self {
        Self {
            success: true,
            output: output.into(),
            error: None,
        }
    }

    pub fn err(error: impl Into<String>) -> Self {
        Self {
            success: false,
            output: serde_json::Value::Null,
            error: Some(error.into()),
        }
    }
}

#[async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn input_schema(&self) -> serde_json::Value;
    async fn execute(&self, input: ToolInput, project_root: &Path) -> ToolResult;
}

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    project_root: PathBuf,
}

impl ToolRegistry {
    pub fn new(project_root: PathBuf) -> Self {
        Self {
            tools: HashMap::new(),
            project_root,
        }
    }

    pub fn register(&mut self, tool: Arc<dyn Tool>) {
        self.tools.insert(tool.name().to_string(), tool);
    }

    pub fn get(&self, name: &str) -> Option<Arc<dyn Tool>> {
        self.tools
            .get(name)
            .cloned()
            .or_else(|| self.tools.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| v.clone()))
    }

    pub fn list(&self) -> Vec<(String, String)> {
        self.tools
            .iter()
            .map(|(name, tool)| (name.clone(), tool.description().to_string()))
            .collect()
    }

    pub fn tool_schemas(&self) -> Vec<serde_json::Value> {
        self.tools
            .values()
            .map(|tool| {
                serde_json::json!({
                    "name": tool.name(),
                    "description": tool.description(),
                    "input_schema": tool.input_schema()
                })
            })
            .collect()
    }
}

pub struct ToolExecutor {
    registry: Arc<ToolRegistry>,
}

impl ToolExecutor {
    pub fn new(registry: Arc<ToolRegistry>) -> Self {
        Self { registry }
    }

    pub async fn execute(&self, tool_name: &str, input: ToolInput) -> ToolResult {
        self.execute_with_root(tool_name, input, &self.registry.project_root)
            .await
    }

    pub async fn execute_with_root(
        &self,
        tool_name: &str,
        input: ToolInput,
        project_root: &Path,
    ) -> ToolResult {
        let check = permissions::check_tool_allowed(
            tool_name,
            &input,
            project_root,
        );
        if !check.allowed {
            let reason = check.reason.unwrap_or_else(|| "Blocked by permissions".to_string());
            eprintln!("[permissions] DENIED: tool={} input={:?} reason={}", tool_name, input, reason);
            return ToolResult::err(reason);
        }

        match self.registry.get(tool_name) {
            Some(tool) => tool.execute(input, project_root).await,
            None => ToolResult::err(format!("Tool not found: {}", tool_name)),
        }
    }

    pub fn list_tools(&self) -> Vec<(String, String)> {
        self.registry.list()
    }

    pub fn list_tools_with_schemas(&self) -> Vec<(String, String, serde_json::Value)> {
        self.registry
            .tool_schemas()
            .into_iter()
            .filter_map(|s| {
                let name = s.get("name").and_then(|v| v.as_str())?.to_string();
                let desc = s.get("description").and_then(|v| v.as_str())?.to_string();
                let schema = s.get("input_schema").cloned().unwrap_or_default();
                Some((name, desc, schema))
            })
            .collect()
    }

    pub fn schemas(&self) -> Vec<serde_json::Value> {
        self.registry.tool_schemas()
    }
}

pub fn create_default_registry(project_root: PathBuf) -> ToolRegistry {
    create_default_registry_with_engine(
        project_root,
        Arc::new(crate::context_engine::ContextEngineV1)
            as Arc<dyn crate::context_engine::ContextEngine>,
    )
}

pub fn create_default_registry_with_engine(
    project_root: PathBuf,
    context_engine: Arc<dyn crate::context_engine::ContextEngine>,
) -> ToolRegistry {
    let mut registry = ToolRegistry::new(project_root);
    registry.register(Arc::new(hybrid::SearchCode::new(context_engine.clone())));
    registry.register(Arc::new(hybrid::OpenFile));
    registry.register(Arc::new(hybrid::SearchAndOpen::new(context_engine)));
    registry.register(Arc::new(filesystem::FsLs));
    registry.register(Arc::new(filesystem::FsCat));
    registry.register(Arc::new(filesystem::FsGrep));
    registry.register(Arc::new(git::GitStatus));
    registry.register(Arc::new(git::GitDiff));
    registry.register(Arc::new(system::SystemVersion));
    registry.register(Arc::new(write_file::WriteFile));
    registry.register(Arc::new(patch::ApplyPatch));
    registry.register(Arc::new(command::RunCommand));
    registry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tool_input_default() {
        let input = ToolInput::default();
        assert!(input.path.is_none());
        assert!(input.pattern.is_none());
        assert!(input.args.is_none());
    }

    #[test]
    fn test_tool_result_ok() {
        let result = ToolResult::ok(serde_json::json!({"key": "value"}));
        assert!(result.success);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_tool_result_err() {
        let result = ToolResult::err("error message");
        assert!(!result.success);
        assert!(result.error.is_some());
    }
}

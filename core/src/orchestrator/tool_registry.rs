use std::collections::HashMap;
use std::sync::Arc;
use serde_json::Value;

use super::tool::{Tool, ToolResult};

pub struct ToolRegistry {
    tools: HashMap<String, Arc<dyn Tool>>,
    project_root: std::path::PathBuf,
}

impl ToolRegistry {
    pub fn new(project_root: std::path::PathBuf) -> Self { Self { tools: HashMap::new(), project_root } }
    pub fn register(&mut self, tool: Arc<dyn Tool>) { self.tools.insert(tool.name().to_string(), tool); }
    pub fn lookup(&self, name: &str) -> Option<Arc<dyn Tool>> { self.tools.get(name).cloned() }
    pub fn list_tools(&self) -> Vec<(String, String)> { self.tools.iter().map(|(n,t)|(n.clone(),t.description().to_string())).collect() }
    pub fn project_root(&self) -> &std::path::Path { &self.project_root }
    pub fn is_path_allowed(&self, path: &std::path::Path) -> bool {
        let cr = match self.project_root.canonicalize() { Ok(p) => p, Err(_) => return false };
        let cp = match path.canonicalize() { Ok(p) => p, Err(_) => return false };
        cp.starts_with(&cr)
    }
}

pub struct ToolExecutor { registry: Arc<ToolRegistry> }
impl ToolExecutor {
    pub fn new(registry: Arc<ToolRegistry>) -> Self { Self { registry } }
    pub async fn execute(&self, name: &str, arguments: Value) -> ToolResult {
        match self.registry.lookup(name) {
            Some(tool) => match tool.execute(arguments).await {
                Ok(result) => ToolResult { name: name.to_string(), result, error: None },
                Err(e) => ToolResult { name: name.to_string(), result: Value::Null, error: Some(e) },
            },
            None => ToolResult { name: name.to_string(), result: Value::Null, error: Some(format!("Tool '{}' not found", name)) },
        }
    }
    pub fn list_tools(&self) -> Vec<(String, String)> { self.registry.list_tools() }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    struct MockTool;
    #[async_trait::async_trait]
    impl Tool for MockTool {
        fn name(&self) -> &str { "mock" }
        fn description(&self) -> &str { "mock desc" }
        fn input_schema(&self) -> Value { json!({}) }
        async fn execute(&self, input: Value) -> Result<Value, String> { Ok(input) }
    }
    #[test] fn test_lookup() { let mut r = ToolRegistry::new(std::env::current_dir().unwrap()); r.register(Arc::new(MockTool)); assert!(r.lookup("mock").is_some()); }
    #[tokio::test] async fn test_execute() { let mut r = ToolRegistry::new(std::env::current_dir().unwrap()); r.register(Arc::new(MockTool)); let e = ToolExecutor::new(Arc::new(r)); let res = e.execute("mock", json!({"a":1})).await; assert!(res.error.is_none()); }
}
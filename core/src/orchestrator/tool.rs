use serde::{Deserialize, Serialize};
use serde_json::Value;

#[async_trait::async_trait]
pub trait Tool: Send + Sync {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn input_schema(&self) -> Value;
    async fn execute(&self, input: Value) -> Result<Value, String>;
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolCall { pub name: String, pub arguments: Value }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolResult { pub name: String, pub result: Value, pub error: Option<String> }
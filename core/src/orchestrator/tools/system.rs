use std::process::Command;
use async_trait::async_trait;
use serde_json::{json, Value};
use crate::orchestrator::tool::Tool;
use crate::orchestrator::tools::ToolInput;

pub struct SystemVersion;

#[async_trait]
impl Tool for SystemVersion {
    fn name(&self) -> &str { "system_version" }
    fn description(&self) -> &str { "Get version of Node.js, Java, Python" }
    
    fn input_schema(&self) -> Value {
        json!({
            "type": "object",
            "properties": {
                "tools": {
                    "type": "array",
                    "items": {"type": "string", "enum": ["node", "java", "python"]}
                }
            },
            "required": []
        })
    }
    
    async fn execute(&self, input: Value) -> Result<Value, String> {
        let input: ToolInput = serde_json::from_value(input).map_err(|e| e.to_string())?;
        
        let tools = input.args.as_ref()
            .and_then(|a| a.get("tools"))
            .and_then(|v| v.as_array())
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect::<Vec<_>>())
            .unwrap_or_else(|| vec!["node".to_string(), "java".to_string(), "python".to_string()]);
        
        let mut results = serde_json::Map::new();
        for tool in tools {
            results.insert(tool.clone(), json!(get_version(&tool)));
        }
        Ok(json!(results))
    }
}

fn get_version(tool: &str) -> String {
    let cmd = match tool { "node"=>"node","java"=>"java","python"=>"python3",_=>return "not found".into() };
    match Command::new(cmd).arg("--version").output() {
        Ok(o) if o.status.success()=>{let v=String::from_utf8_lossy(&o.stdout).trim().to_string();if v.is_empty(){String::from_utf8_lossy(&o.stderr).trim().to_string()}else{v}},_=>"not found".into()
    }
}
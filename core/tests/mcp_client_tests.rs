use std::collections::HashMap;
use std::path::Path;
use std::process::Command;
use std::sync::Arc;

use multilink_core::config::McpServerConfig;
use multilink_core::tools::{create_default_registry, ToolExecutor};
use multilink_core::McpRuntime;
use serde_json::json;
use tempfile::tempdir;

const FAKE_SERVER: &str = r#"
import json
import sys

for line in sys.stdin:
    line = line.strip()
    if not line:
        continue
    try:
        message = json.loads(line)
    except Exception:
        continue
    if "id" not in message:
        continue
    method = message.get("method")
    request_id = message.get("id")
    if method == "initialize":
        result = {
            "protocolVersion": "2025-06-18",
            "capabilities": {"tools": {}},
            "serverInfo": {"name": "fake-users", "version": "1.0"}
        }
    elif method == "tools/list":
        result = {
            "tools": [{
                "name": "create_users",
                "description": "Create N users",
                "inputSchema": {
                    "type": "object",
                    "properties": {"count": {"type": "integer"}},
                    "required": ["count"]
                }
            }]
        }
    elif method == "tools/call":
        arguments = message.get("params", {}).get("arguments", {})
        result = {
            "content": [{
                "type": "text",
                "text": "created {} users".format(arguments.get("count"))
            }]
        }
    else:
        result = {}
    sys.stdout.write(json.dumps({"jsonrpc": "2.0", "id": request_id, "result": result}) + "\n")
    sys.stdout.flush()
"#;

fn python_available() -> bool {
    Command::new("python3")
        .arg("--version")
        .output()
        .map(|output| output.status.success())
        .unwrap_or(false)
}

fn fake_server_config(dir: &Path, enabled: bool) -> McpServerConfig {
    let script_path = dir.join("fake_mcp.py");
    std::fs::write(&script_path, FAKE_SERVER).expect("write fake server");
    McpServerConfig {
        name: "users".to_string(),
        command: "python3".to_string(),
        args: vec![script_path.to_string_lossy().to_string()],
        env: HashMap::new(),
        enabled,
    }
}

#[tokio::test]
async fn discovers_and_executes_stdio_mcp_tools() {
    if !python_available() {
        eprintln!("python3 not available; skipping MCP stdio integration test");
        return;
    }

    let dir = tempdir().expect("tempdir");
    let runtime = McpRuntime::new(vec![fake_server_config(dir.path(), true)]);

    let catalog = runtime.ensure_catalog().await;
    assert_eq!(catalog.len(), 1);
    assert_eq!(catalog[0].exposed_name, "mcp__users__create_users");

    let result = runtime
        .call_tool("mcp__users__create_users", json!({"count": 5}))
        .await;
    assert!(result.success, "call failed: {:?}", result.error);
    let serialized = serde_json::to_string(&result.output).expect("serialize output");
    assert!(serialized.contains("created 5 users"), "{serialized}");
}

#[tokio::test]
async fn tool_executor_routes_mcp_tools_and_schemas() {
    if !python_available() {
        return;
    }

    let dir = tempdir().expect("tempdir");
    let mcp = Arc::new(McpRuntime::new(vec![fake_server_config(dir.path(), true)]));
    let executor = ToolExecutor::new(Arc::new(create_default_registry(dir.path().to_path_buf())))
        .with_mcp_runtime(mcp);

    let catalog = executor.ensure_mcp_catalog().await;
    assert_eq!(catalog.len(), 1);
    assert!(executor
        .mcp_tool_names()
        .contains(&"mcp__users__create_users".to_string()));
    assert!(executor
        .mcp_tool_descriptions()
        .iter()
        .any(|description| description.name == "mcp__users__create_users"));
    assert!(executor
        .list_tools_with_schemas()
        .iter()
        .any(|(name, _, _)| name == "mcp__users__create_users"));

    let result = executor
        .execute_json("mcp__users__create_users", json!({"count": 3}))
        .await;
    assert!(result.success, "call failed: {:?}", result.error);
    let serialized = serde_json::to_string(&result.output).expect("serialize output");
    assert!(serialized.contains("created 3 users"), "{serialized}");
}

#[test]
fn disabled_servers_are_not_discovered() {
    if !python_available() {
        return;
    }

    let dir = tempdir().expect("tempdir");
    let runtime = McpRuntime::new(vec![fake_server_config(dir.path(), false)]);

    let catalog = tokio::runtime::Runtime::new()
        .expect("runtime")
        .block_on(runtime.ensure_catalog());
    assert!(catalog.is_empty());
}

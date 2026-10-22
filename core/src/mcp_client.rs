use std::collections::HashSet;
use std::process::Stdio;
use std::sync::Mutex;
use std::time::Duration;

use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdin, ChildStdout, Command};
use tokio::time::timeout;

use crate::config::McpServerConfig;
use crate::tools::description::ToolDescription;
use crate::tools::ToolResult;

const REQUEST_TIMEOUT: Duration = Duration::from_secs(60);
const DISCOVERY_TIMEOUT: Duration = Duration::from_secs(45);
const TOOL_CALL_TIMEOUT: Duration = Duration::from_secs(180);
const MAX_CATALOG_PAGES: usize = 50;

#[derive(Debug, Clone)]
pub struct McpCatalogEntry {
    pub server: String,
    pub tool: String,
    pub exposed_name: String,
    pub description: String,
    pub input_schema: Value,
}

impl McpCatalogEntry {
    pub fn to_tool_description(&self) -> ToolDescription {
        ToolDescription {
            name: self.exposed_name.clone(),
            description: format!("[MCP: {}] {}", self.server, self.description),
            when_to_use: format!(
                "Use this tool when the user asks to interact with the '{}' MCP server.",
                self.server
            ),
            when_not_to_use: "Do not use this tool for local filesystem or shell tasks unless the MCP server is explicitly meant for that.".to_string(),
            examples: Vec::new(),
            schema: self.input_schema.clone(),
        }
    }
}

#[derive(Debug, Clone)]
struct RawMcpTool {
    name: String,
    description: String,
    input_schema: Value,
}

struct McpState {
    servers: Vec<McpServerConfig>,
    catalog: Vec<McpCatalogEntry>,
    initialized: bool,
}

pub struct McpRuntime {
    state: Mutex<McpState>,
}

impl McpRuntime {
    pub fn new(servers: Vec<McpServerConfig>) -> Self {
        Self {
            state: Mutex::new(McpState {
                servers,
                catalog: Vec::new(),
                initialized: false,
            }),
        }
    }

    pub fn set_servers(&self, servers: Vec<McpServerConfig>) {
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.servers = servers;
        state.catalog.clear();
        state.initialized = false;
    }

    pub fn catalog(&self) -> Vec<McpCatalogEntry> {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .catalog
            .clone()
    }

    pub fn configured_server_count(&self) -> usize {
        self.state
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .servers
            .iter()
            .filter(|server| server.enabled)
            .count()
    }

    pub fn is_mcp_tool(&self, name: &str) -> bool {
        name.starts_with("mcp__")
    }

    pub async fn ensure_catalog(&self) -> Vec<McpCatalogEntry> {
        let (servers, initialized, cached) = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            (
                state.servers.clone(),
                state.initialized,
                state.catalog.clone(),
            )
        };

        if initialized {
            return cached;
        }

        let mut catalog = Vec::new();
        let mut used_names = HashSet::new();

        for server in servers.iter().filter(|server| server.enabled) {
            match discover_server(server).await {
                Ok(tools) => {
                    for tool in tools {
                        let base = format!(
                            "mcp__{}__{}",
                            sanitize_component(&server.name),
                            sanitize_component(&tool.name)
                        );
                        let mut exposed_name = base.clone();
                        let mut suffix = 2;
                        while used_names.contains(&exposed_name) {
                            exposed_name = format!("{}_{}", base, suffix);
                            suffix += 1;
                        }
                        used_names.insert(exposed_name.clone());
                        catalog.push(McpCatalogEntry {
                            server: server.name.clone(),
                            tool: tool.name,
                            exposed_name,
                            description: tool.description,
                            input_schema: tool.input_schema,
                        });
                    }
                }
                Err(err) => {
                    eprintln!(
                        "[mcp] no se pudieron listar herramientas de '{}': {}",
                        server.name, err
                    );
                }
            }
        }

        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.catalog = catalog.clone();
        state.initialized = true;
        catalog
    }

    pub async fn call_tool(&self, exposed_name: &str, arguments: Value) -> ToolResult {
        let mut catalog = self.catalog();
        if catalog.is_empty() {
            catalog = self.ensure_catalog().await;
        }

        let Some(entry) = catalog
            .iter()
            .find(|entry| entry.exposed_name == exposed_name)
            .cloned()
        else {
            return ToolResult::err(format!("MCP tool not found: {}", exposed_name));
        };

        let server = {
            let state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state
                .servers
                .iter()
                .find(|server| server.name == entry.server)
                .cloned()
        };
        let Some(server) = server else {
            return ToolResult::err(format!("MCP server not found: {}", entry.server));
        };

        match call_server_tool(&server, &entry.tool, arguments).await {
            Ok(result) => result,
            Err(err) => ToolResult::err(format!(
                "MCP tool '{}' failed on server '{}': {}",
                entry.tool, entry.server, err
            )),
        }
    }
}

fn sanitize_component(value: &str) -> String {
    let mut sanitized = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            sanitized.push(ch.to_ascii_lowercase());
        } else {
            sanitized.push('_');
        }
    }
    let trimmed = sanitized.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "unnamed".to_string()
    } else {
        trimmed
    }
}

async fn discover_server(server: &McpServerConfig) -> Result<Vec<RawMcpTool>, String> {
    let mut client = McpStdioClient::spawn(server).await?;
    let result = async {
        client.initialize().await?;
        let mut tools = Vec::new();
        let mut cursor: Option<String> = None;

        for _ in 0..MAX_CATALOG_PAGES {
            let params = cursor
                .as_ref()
                .map(|cursor| json!({ "cursor": cursor }))
                .unwrap_or_else(|| json!({}));
            let result = client
                .request("tools/list", params, DISCOVERY_TIMEOUT)
                .await?;

            if let Some(items) = result.get("tools").and_then(Value::as_array) {
                for item in items {
                    let Some(name) = item.get("name").and_then(Value::as_str) else {
                        continue;
                    };
                    tools.push(RawMcpTool {
                        name: name.to_string(),
                        description: item
                            .get("description")
                            .and_then(Value::as_str)
                            .unwrap_or("MCP tool")
                            .to_string(),
                        input_schema: item
                            .get("inputSchema")
                            .cloned()
                            .unwrap_or_else(|| json!({ "type": "object" })),
                    });
                }
            }

            cursor = result
                .get("nextCursor")
                .and_then(Value::as_str)
                .map(str::to_string);
            if cursor.is_none() {
                break;
            }
        }

        Ok(tools)
    }
    .await;

    client.close().await;
    result
}

async fn call_server_tool(
    server: &McpServerConfig,
    tool_name: &str,
    arguments: Value,
) -> Result<ToolResult, String> {
    let mut client = McpStdioClient::spawn(server).await?;
    let result = async {
        client.initialize().await?;
        let arguments = if arguments.is_object() {
            arguments
        } else {
            json!({})
        };
        let result = client
            .request(
                "tools/call",
                json!({ "name": tool_name, "arguments": arguments }),
                TOOL_CALL_TIMEOUT,
            )
            .await?;

        let is_error = result
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let text = extract_content_text(&result);

        if is_error {
            return Ok(ToolResult::err(
                text.unwrap_or_else(|| "MCP tool reported an error".to_string()),
            ));
        }

        Ok(match text {
            Some(text) => ToolResult::ok(json!({
                "content": text,
                "structured": result
            })),
            None => ToolResult::ok(result),
        })
    }
    .await;

    client.close().await;
    result
}

fn extract_content_text(result: &Value) -> Option<String> {
    let items = result.get("content").and_then(Value::as_array)?;
    let parts: Vec<String> = items
        .iter()
        .filter_map(|item| item.get("text").and_then(Value::as_str))
        .map(str::to_string)
        .collect();
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("\n"))
    }
}

struct McpStdioClient {
    child: Child,
    stdin: Option<ChildStdin>,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
}

impl McpStdioClient {
    async fn spawn(server: &McpServerConfig) -> Result<Self, String> {
        let mut command = Command::new(&server.command);
        command
            .args(&server.args)
            .envs(&server.env)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .kill_on_drop(true);

        let mut child = command
            .spawn()
            .map_err(|err| format!("no se pudo iniciar '{}': {}", server.command, err))?;
        let stdin = child
            .stdin
            .take()
            .ok_or_else(|| "MCP stdin unavailable".to_string())?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| "MCP stdout unavailable".to_string())?;

        Ok(Self {
            child,
            stdin: Some(stdin),
            stdout: BufReader::new(stdout),
            next_id: 0,
        })
    }

    async fn initialize(&mut self) -> Result<(), String> {
        let result = self
            .request(
                "initialize",
                json!({
                    "protocolVersion": "2025-06-18",
                    "capabilities": {},
                    "clientInfo": {
                        "name": "MultiLink",
                        "version": env!("CARGO_PKG_VERSION")
                    }
                }),
                REQUEST_TIMEOUT,
            )
            .await?;

        if result.get("protocolVersion").is_none() && result.get("serverInfo").is_none() {
            return Err("invalid initialize response from MCP server".to_string());
        }

        self.notify("notifications/initialized", json!({})).await
    }

    async fn notify(&mut self, method: &str, params: Value) -> Result<(), String> {
        let payload = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params
        });
        self.write_message(&payload).await
    }

    async fn request(
        &mut self,
        method: &str,
        params: Value,
        request_timeout: Duration,
    ) -> Result<Value, String> {
        self.next_id += 1;
        let id = self.next_id;
        let payload = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params
        });
        self.write_message(&payload).await?;

        loop {
            let mut line = String::new();
            let read = timeout(request_timeout, self.stdout.read_line(&mut line))
                .await
                .map_err(|_| format!("MCP request '{}' timed out", method))?
                .map_err(|err| format!("failed to read from MCP server: {}", err))?;
            if read == 0 {
                return Err(format!(
                    "MCP server closed stdout while waiting for '{}'",
                    method
                ));
            }

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            let message: Value = match serde_json::from_str(trimmed) {
                Ok(message) => message,
                Err(_) => continue,
            };

            if message.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = message.get("error") {
                    let code = error.get("code").and_then(Value::as_i64).unwrap_or(0);
                    let text = error
                        .get("message")
                        .and_then(Value::as_str)
                        .unwrap_or("MCP error");
                    return Err(format!("{} (code {})", text, code));
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }

            if message.get("method").is_some() {
                if let Some(request_id) = message.get("id").cloned() {
                    self.respond_to_server_request(request_id, &message).await?;
                }
            }
        }
    }

    async fn respond_to_server_request(
        &mut self,
        id: Value,
        request: &Value,
    ) -> Result<(), String> {
        let method = request
            .get("method")
            .and_then(Value::as_str)
            .unwrap_or_default();
        let response = match method {
            "ping" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {}
            }),
            "roots/list" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": { "roots": [] }
            }),
            _ => json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {
                    "code": -32601,
                    "message": format!("Method not found: {}", method)
                }
            }),
        };
        self.write_message(&response).await
    }

    async fn write_message(&mut self, message: &Value) -> Result<(), String> {
        let mut line = serde_json::to_string(message)
            .map_err(|err| format!("failed to serialize MCP message: {}", err))?;
        line.push('\n');
        timeout(REQUEST_TIMEOUT, async {
            let stdin = self
                .stdin
                .as_mut()
                .ok_or_else(|| "MCP stdin is closed".to_string())?;
            stdin
                .write_all(line.as_bytes())
                .await
                .map_err(|err| format!("failed to write to MCP server: {}", err))?;
            stdin
                .flush()
                .await
                .map_err(|err| format!("failed to flush MCP server stdin: {}", err))
        })
        .await
        .map_err(|_| "timed out writing to MCP server".to_string())?
    }

    async fn close(mut self) {
        if let Some(mut stdin) = self.stdin.take() {
            let _ = stdin.shutdown().await;
        }
        if timeout(Duration::from_millis(500), self.child.wait())
            .await
            .is_err()
        {
            let _ = self.child.start_kill();
            let _ = timeout(Duration::from_millis(250), self.child.wait()).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitizes_mcp_names_for_tool_selection() {
        assert_eq!(sanitize_component("Filesystem Server"), "filesystem_server");
        assert_eq!(sanitize_component("user-tools.v2"), "user_tools_v2");
        assert_eq!(sanitize_component("@@@"), "unnamed");
    }

    #[test]
    fn empty_runtime_has_no_catalog() {
        let runtime = McpRuntime::new(Vec::new());
        assert!(runtime.catalog().is_empty());
        assert!(runtime.is_mcp_tool("mcp__server__tool"));
        assert!(!runtime.is_mcp_tool("write_file"));
    }

    #[test]
    fn extracts_text_content_from_mcp_result() {
        let result = json!({
            "content": [
                {"type": "text", "text": "hola"},
                {"type": "text", "text": "mundo"}
            ]
        });
        assert_eq!(
            extract_content_text(&result),
            Some("hola\nmundo".to_string())
        );
    }
}

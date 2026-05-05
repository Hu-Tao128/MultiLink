use serde::{Deserialize, Serialize};

use crate::lan_agent::{LanEnvelope, LanPayload};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolResponse {
    pub id: String,
    pub result: Option<McpResult>,
    pub error: Option<McpError>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpResult {
    pub content: Vec<McpContent>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum McpContent {
    Text { text: String },
    Error { text: String },
    Json { json: serde_json::Value },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub code: i32,
    pub message: String,
}

pub struct ThinMcpAdapter;

impl ThinMcpAdapter {
    pub fn to_lan_envelope(call: McpToolCall, timestamp_ms: u64) -> LanEnvelope {
        let payload = match call.tool.as_str() {
            "chat.dispatch" => {
                let session_id = call
                    .arguments
                    .get("session_id")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let prompt = call
                    .arguments
                    .get("prompt")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let provider = call
                    .arguments
                    .get("provider")
                    .and_then(|v| v.as_str())
                    .map(|v| v.to_string());
                LanPayload::Dispatch {
                    session_id,
                    prompt,
                    provider,
                }
            }
            "chat.ping" => LanPayload::Ping,
            "tools.list" => LanPayload::ToolList,
            "tools.execute" => {
                let tool_name = call
                    .arguments
                    .get("tool_name")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string();
                let arguments = call
                    .arguments
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                LanPayload::ToolExecute {
                    tool_name,
                    arguments,
                }
            }
            _ => LanPayload::Error {
                message: format!("unsupported MCP tool: {}", call.tool),
            },
        };

        LanEnvelope {
            protocol_version: 1,
            request_id: call.id,
            timestamp_ms,
            hmac_signature: String::new(),
            payload,
        }
    }

    pub fn from_lan_envelope(envelope: LanEnvelope) -> McpToolResponse {
        match envelope.payload {
            LanPayload::DispatchResponse { ok, text, error } => {
                if ok {
                    McpToolResponse {
                        id: envelope.request_id,
                        result: Some(McpResult {
                            content: vec![McpContent::Text {
                                text: text.unwrap_or_default(),
                            }],
                        }),
                        error: None,
                    }
                } else {
                    McpToolResponse {
                        id: envelope.request_id,
                        result: None,
                        error: Some(McpError {
                            code: 500,
                            message: error.unwrap_or_else(|| "unknown error".to_string()),
                        }),
                    }
                }
            }
            LanPayload::ToolListResponse { tools } => {
                let json = serde_json::to_value(tools).unwrap_or_default();
                McpToolResponse {
                    id: envelope.request_id,
                    result: Some(McpResult {
                        content: vec![McpContent::Json { json }],
                    }),
                    error: None,
                }
            }
            LanPayload::ToolExecuteResponse {
                success,
                output,
                error,
            } => {
                if success {
                    McpToolResponse {
                        id: envelope.request_id,
                        result: Some(McpResult {
                            content: vec![McpContent::Json {
                                json: output.unwrap_or(serde_json::Value::Null),
                            }],
                        }),
                        error: None,
                    }
                } else {
                    McpToolResponse {
                        id: envelope.request_id,
                        result: None,
                        error: Some(McpError {
                            code: 500,
                            message: error.unwrap_or_else(|| "tool execution failed".to_string()),
                        }),
                    }
                }
            }
            LanPayload::Error { message } => McpToolResponse {
                id: envelope.request_id,
                result: None,
                error: Some(McpError { code: 400, message }),
            },
            LanPayload::Ping => McpToolResponse {
                id: envelope.request_id,
                result: Some(McpResult {
                    content: vec![McpContent::Text {
                        text: "pong".to_string(),
                    }],
                }),
                error: None,
            },
            LanPayload::Dispatch { .. } => McpToolResponse {
                id: envelope.request_id,
                result: None,
                error: Some(McpError {
                    code: 400,
                    message: "unexpected dispatch request in response".to_string(),
                }),
            },
            LanPayload::ToolList => McpToolResponse {
                id: envelope.request_id,
                result: None,
                error: Some(McpError {
                    code: 400,
                    message: "unexpected tool list request in response".to_string(),
                }),
            },
            LanPayload::ToolExecute { .. } => McpToolResponse {
                id: envelope.request_id,
                result: None,
                error: Some(McpError {
                    code: 400,
                    message: "unexpected tool execute request in response".to_string(),
                }),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lan_agent::ToolDescriptor;

    #[test]
    fn test_tools_list_mcp_to_lan() {
        let call = McpToolCall {
            id: "req-1".to_string(),
            tool: "tools.list".to_string(),
            arguments: serde_json::Value::Null,
        };
        let env = ThinMcpAdapter::to_lan_envelope(call, 1000);
        assert!(matches!(env.payload, LanPayload::ToolList));
    }

    #[test]
    fn test_tools_execute_mcp_to_lan() {
        let call = McpToolCall {
            id: "req-2".to_string(),
            tool: "tools.execute".to_string(),
            arguments: serde_json::json!({
                "tool_name": "fs_ls",
                "arguments": {"path": "."}
            }),
        };
        let env = ThinMcpAdapter::to_lan_envelope(call, 1000);
        match env.payload {
            LanPayload::ToolExecute { tool_name, .. } => {
                assert_eq!(tool_name, "fs_ls");
            }
            _ => panic!("expected ToolExecute"),
        }
    }

    #[test]
    fn test_tools_list_response_from_lan() {
        let envelope = LanEnvelope {
            protocol_version: 1,
            request_id: "req-1".to_string(),
            timestamp_ms: 1000,
            hmac_signature: String::new(),
            payload: LanPayload::ToolListResponse {
                tools: vec![ToolDescriptor {
                    name: "fs_ls".to_string(),
                    description: "list files".to_string(),
                    input_schema: serde_json::Value::Null,
                }],
            },
        };
        let response = ThinMcpAdapter::from_lan_envelope(envelope);
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }

    #[test]
    fn test_tools_execute_response_from_lan() {
        let envelope = LanEnvelope {
            protocol_version: 1,
            request_id: "req-2".to_string(),
            timestamp_ms: 1000,
            hmac_signature: String::new(),
            payload: LanPayload::ToolExecuteResponse {
                success: true,
                output: Some(serde_json::json!({"entries": []})),
                error: None,
            },
        };
        let response = ThinMcpAdapter::from_lan_envelope(envelope);
        assert!(response.result.is_some());
        assert!(response.error.is_none());
    }
}

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
        }
    }
}

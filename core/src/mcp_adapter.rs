use serde::{Deserialize, Serialize};

use crate::lan_agent::{LanEnvelope, LanPayload};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpToolCall {
    pub id: String,
    pub tool: String,
    #[serde(default)]
    pub arguments: serde_json::Value,
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
            _ => LanPayload::Error {
                message: format!("unsupported MCP tool: {}", call.tool),
            },
        };

        LanEnvelope {
            protocol_version: 1,
            request_id: call.id,
            timestamp_ms,
            payload,
        }
    }
}

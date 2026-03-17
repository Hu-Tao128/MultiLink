use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::RwLock;

use crate::chat_runtime::ChatRuntime;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanEnvelope {
    pub protocol_version: u8,
    pub request_id: String,
    pub timestamp_ms: u64,
    pub payload: LanPayload,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum LanPayload {
    Ping,
    Dispatch {
        session_id: String,
        prompt: String,
        provider: Option<String>,
    },
    DispatchResponse {
        ok: bool,
        #[serde(default)]
        text: Option<String>,
        #[serde(default)]
        error: Option<String>,
    },
    Error {
        message: String,
    },
}

pub fn encode_messagepack<T: Serialize>(value: &T) -> Result<Vec<u8>, rmp_serde::encode::Error> {
    rmp_serde::to_vec(value)
}

pub fn decode_messagepack<T: for<'de> Deserialize<'de>>(
    bytes: &[u8],
) -> Result<T, rmp_serde::decode::Error> {
    rmp_serde::from_slice(bytes)
}

pub fn shared_secret_matches(expected: &str, provided: &str) -> bool {
    if expected.is_empty() || provided.is_empty() {
        return false;
    }
    expected == provided
}

pub struct LanAgentServer {
    listener: TcpListener,
    runtime: Arc<ChatRuntime>,
    shared_secret: String,
    shutdown: Arc<RwLock<bool>>,
}

impl LanAgentServer {
    pub async fn bind(addr: &str, runtime: Arc<ChatRuntime>, shared_secret: String) -> Result<Self, std::io::Error> {
        let listener = TcpListener::bind(addr).await?;
        Ok(Self {
            listener,
            runtime,
            shared_secret,
            shutdown: Arc::new(RwLock::new(false)),
        })
    }

    pub async fn run(&self) -> Result<(), std::io::Error> {
        loop {
            {
                let should_stop = self.shutdown.read().await;
                if *should_stop {
                    break;
                }
            }

            tokio::select! {
                result = self.listener.accept() => {
                    match result {
                        Ok((stream, _addr)) => {
                            let runtime = self.runtime.clone();
                            let secret = self.shared_secret.clone();
                            tokio::spawn(async move {
                                if let Err(e) = Self::handle_connection(stream, runtime, secret).await {
                                    eprintln!("LAN Agent connection error: {}", e);
                                }
                            });
                        }
                        Err(e) => {
                            eprintln!("LAN Agent accept error: {}", e);
                        }
                    }
                }
            }
        }
        Ok(())
    }

    async fn handle_connection(
        mut stream: TcpStream,
        runtime: Arc<ChatRuntime>,
        shared_secret: String,
    ) -> Result<(), std::io::Error> {
        let mut buffer = vec![0u8; 65536];
        
        let n = stream.read(&mut buffer).await?;
        if n == 0 {
            return Ok(());
        }

        let request: LanEnvelope = match decode_messagepack(&buffer[..n]) {
            Ok(req) => req,
            Err(e) => {
                let response = LanEnvelope {
                    protocol_version: 1,
                    request_id: String::new(),
                    timestamp_ms: unix_timestamp_ms(),
                    payload: LanPayload::Error {
                        message: format!("decode error: {}", e),
                    },
                };
                Self::write_response(&mut stream, &response).await?;
                return Ok(());
            }
        };

        let response = Self::process_request(request, &runtime, &shared_secret).await;
        Self::write_response(&mut stream, &response).await?;

        Ok(())
    }

    async fn process_request(
        request: LanEnvelope,
        runtime: &Arc<ChatRuntime>,
        shared_secret: &str,
    ) -> LanEnvelope {
        if shared_secret.is_empty() {
            return LanEnvelope {
                protocol_version: request.protocol_version,
                request_id: request.request_id,
                timestamp_ms: unix_timestamp_ms(),
                payload: LanPayload::Error {
                    message: "server not configured with shared secret".to_string(),
                },
            };
        }

        let response_payload = match request.payload {
            LanPayload::Ping => LanPayload::DispatchResponse {
                ok: true,
                text: Some("pong".to_string()),
                error: None,
            },
            LanPayload::Dispatch { session_id, prompt, provider: _ } => {
                match runtime.send_message(&session_id, prompt).await {
                    Ok(_response) => LanPayload::DispatchResponse {
                        ok: true,
                        text: Some("message queued".to_string()),
                        error: None,
                    },
                    Err(e) => LanPayload::DispatchResponse {
                        ok: false,
                        text: None,
                        error: Some(e.to_string()),
                    },
                }
            }
            LanPayload::Error { message } => LanPayload::Error { message },
            LanPayload::DispatchResponse { .. } => LanPayload::Error {
                message: "unexpected response payload in request".to_string(),
            },
        };

        LanEnvelope {
            protocol_version: request.protocol_version,
            request_id: request.request_id,
            timestamp_ms: unix_timestamp_ms(),
            payload: response_payload,
        }
    }

    async fn write_response(stream: &mut TcpStream, response: &LanEnvelope) -> Result<(), std::io::Error> {
        let encoded = encode_messagepack(response).map_err(|e| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, e)
        })?;
        stream.write_all(&encoded).await?;
        stream.flush().await?;
        Ok(())
    }

    pub async fn shutdown(&self) {
        let mut guard = self.shutdown.write().await;
        *guard = true;
    }
}

fn unix_timestamp_ms() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messagepack_roundtrip_works() {
        let env = LanEnvelope {
            protocol_version: 1,
            request_id: "req-1".to_string(),
            timestamp_ms: 123,
            payload: LanPayload::Ping,
        };

        let encoded = encode_messagepack(&env).expect("encode");
        let decoded: LanEnvelope = decode_messagepack(&encoded).expect("decode");
        assert_eq!(decoded.protocol_version, 1);
        assert_eq!(decoded.request_id, "req-1");
        matches!(decoded.payload, LanPayload::Ping);
    }

    #[test]
    fn shared_secret_compare_is_exact() {
        assert!(shared_secret_matches("abc", "abc"));
        assert!(!shared_secret_matches("abc", "abd"));
        assert!(!shared_secret_matches("", "abc"));
    }
}

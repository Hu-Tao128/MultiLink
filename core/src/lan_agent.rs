use serde::{Deserialize, Serialize};

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

use crate::providers::{ProviderCapabilities, ProviderId, ProviderInfo};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct RequiredCapabilities {
    pub reasoning: bool,
    pub coding: bool,
    pub vision: bool,
    pub embeddings: bool,
    pub low_latency: bool,
    pub long_context: bool,
}

impl RequiredCapabilities {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_reasoning(mut self) -> Self {
        self.reasoning = true;
        self
    }

    pub fn with_coding(mut self) -> Self {
        self.coding = true;
        self
    }

    pub fn with_vision(mut self) -> Self {
        self.vision = true;
        self
    }

    pub fn with_embeddings(mut self) -> Self {
        self.embeddings = true;
        self
    }

    pub fn with_low_latency(mut self) -> Self {
        self.low_latency = true;
        self
    }

    pub fn with_long_context(mut self) -> Self {
        self.long_context = true;
        self
    }

    pub fn min_context_tokens(&self) -> usize {
        if self.long_context {
            32768
        } else {
            4096
        }
    }
}

pub struct ProviderSelector;

impl ProviderSelector {
    pub fn select(
        required: &RequiredCapabilities,
        available_providers: &[ProviderInfo],
    ) -> Option<ProviderId> {
        let mut scored: Vec<(i32, ProviderId)> = available_providers
            .iter()
            .filter_map(|p| {
                if !p.is_available {
                    return None;
                }
                let caps = &p.capabilities;
                if !Self::meets_requirements(required, caps) {
                    return None;
                }
                let score = Self::compute_score(required, caps);
                Some((score, p.id))
            })
            .collect();

        scored.sort_by_key(|score| std::cmp::Reverse(score.0));
        scored.first().map(|(_, id)| *id)
    }

    fn meets_requirements(required: &RequiredCapabilities, caps: &ProviderCapabilities) -> bool {
        if required.vision && !caps.supports_vision && !caps.vision {
            return false;
        }
        if required.embeddings && !caps.supports_embedding {
            return false;
        }
        if (caps.context_length as usize) < required.min_context_tokens() {
            return false;
        }
        true
    }

    fn compute_score(required: &RequiredCapabilities, caps: &ProviderCapabilities) -> i32 {
        let mut score = 0i32;

        if caps.is_local {
            score += 50;
        }

        if required.low_latency {
            let latency_bonus = 30 - (caps.latency_estimate_ms.unwrap_or(100).min(30) as i32);
            score += latency_bonus.max(0);
        }

        if required.long_context {
            if caps.context_length >= 32768 {
                score += 20;
            } else if caps.context_length >= 16384 {
                score += 10;
            }
        }

        if required.reasoning && caps.supports_thinking {
            score += 15;
        }

        if required.coding && caps.capability_tags.contains(&"coding".to_string()) {
            score += 10;
        }

        if required.embeddings && caps.supports_embedding {
            score += 5;
        }

        score
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps(
        vision: bool,
        embedding: bool,
        thinking: bool,
        context: u32,
        is_local: bool,
        latency_ms: Option<u32>,
    ) -> ProviderCapabilities {
        ProviderCapabilities {
            supports_vision: vision,
            vision,
            supports_embedding: embedding,
            context_length: context,
            supports_thinking: thinking,
            is_local,
            latency_estimate_ms: latency_ms,
            ..ProviderCapabilities::default()
        }
    }

    fn info(id: ProviderId, caps: ProviderCapabilities) -> ProviderInfo {
        ProviderInfo {
            id,
            capabilities: caps,
            is_available: true,
        }
    }

    #[test]
    fn test_vision_required_filters_non_vision() {
        let req = RequiredCapabilities {
            vision: true,
            ..Default::default()
        };
        let providers = vec![
            info(
                ProviderId::Ollama,
                caps(false, false, false, 4096, false, None),
            ),
            info(
                ProviderId::Gemini,
                caps(true, false, false, 128000, false, None),
            ),
        ];
        assert_eq!(
            ProviderSelector::select(&req, &providers),
            Some(ProviderId::Gemini)
        );
    }

    #[test]
    fn test_local_provider_preferred_when_low_latency() {
        let req = RequiredCapabilities {
            low_latency: true,
            ..Default::default()
        };
        let providers = vec![
            info(
                ProviderId::Gemini,
                caps(false, false, false, 4096, false, Some(500)),
            ),
            info(
                ProviderId::Ollama,
                caps(false, false, false, 4096, true, Some(50)),
            ),
        ];
        assert_eq!(
            ProviderSelector::select(&req, &providers),
            Some(ProviderId::Ollama)
        );
    }

    #[test]
    fn test_long_context_requires_sufficient_tokens() {
        let req = RequiredCapabilities {
            long_context: true,
            ..Default::default()
        };
        let providers = vec![
            info(
                ProviderId::Ollama,
                caps(false, false, false, 8192, false, None),
            ),
            info(
                ProviderId::Gemini,
                caps(false, false, false, 128000, false, None),
            ),
        ];
        assert_eq!(
            ProviderSelector::select(&req, &providers),
            Some(ProviderId::Gemini)
        );
    }

    #[test]
    fn test_empty_providers_returns_none() {
        let req = RequiredCapabilities::default();
        assert_eq!(ProviderSelector::select(&req, &[]), None);
    }

    #[test]
    fn test_unavailable_provider_filtered() {
        let mut p = info(
            ProviderId::Ollama,
            caps(false, false, false, 4096, false, None),
        );
        p.is_available = false;
        let req = RequiredCapabilities::default();
        assert_eq!(ProviderSelector::select(&req, &[p]), None);
    }

    #[test]
    fn test_infer_capabilities_reasoning() {
        let caps = super::super::planner::infer_capabilities("think step by step about this");
        assert!(caps.reasoning);
    }

    #[test]
    fn test_infer_capabilities_coding() {
        let caps = super::super::planner::infer_capabilities("write a function to parse JSON");
        assert!(caps.coding);
    }

    #[test]
    fn test_infer_capabilities_embeddings() {
        let caps = super::super::planner::infer_capabilities("find similar files to this one");
        assert!(caps.embeddings);
    }
}

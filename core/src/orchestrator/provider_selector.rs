use crate::providers::{ProviderCapabilities, ProviderId};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct RequiredCapabilities {
    pub vision: bool,
    pub audio: bool,
    pub thinking: bool,
    pub tools: bool,
    pub fim: bool,
    pub min_context_length: u32,
}

pub struct ProviderSelector;

impl ProviderSelector {
    /// Selects the best provider based on requirements and priority.
    /// 
    /// Returns the ProviderId with the lowest priority value (highest priority)
    /// that meets all required capabilities and is enabled.
    pub fn select(
        required: &RequiredCapabilities,
        available_providers: &[(ProviderId, ProviderCapabilities, bool, u32)], // (Id, Caps, Enabled, Priority)
    ) -> Option<ProviderId> {
        let mut candidates: Vec<&(ProviderId, ProviderCapabilities, bool, u32)> = available_providers
            .iter()
            .filter(|(_, caps, enabled, _)| {
                // 1. Must be enabled
                if !*enabled {
                    return false;
                }

                // 2. Check capabilities
                if required.vision && !caps.supports_vision && !caps.vision {
                    return false;
                }
                if required.audio && !caps.capability_tags.contains(&"audio".to_string()) {
                    return false;
                }
                if required.thinking && !caps.supports_thinking {
                    return false;
                }
                if required.tools && !caps.tools {
                    return false;
                }
                if required.fim && !caps.fim {
                    return false;
                }
                if caps.context_length < required.min_context_length {
                    return false;
                }

                true
            })
            .collect();

        // 3. Sort by priority (lower number = higher priority)
        candidates.sort_by_key(|(_, _, _, priority)| *priority);

        candidates.first().map(|(id, _, _, _)| *id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mock_caps(vision: bool, thinking: bool, ctx: u32) -> ProviderCapabilities {
        ProviderCapabilities {
            vision,
            supports_vision: vision,
            supports_thinking: thinking,
            context_length: ctx,
            ..ProviderCapabilities::default_with_context(ctx as usize)
        }
    }

    #[test]
    fn test_filters_disabled_providers() {
        let required = RequiredCapabilities::default();
        let providers = vec![
            (ProviderId::Ollama, mock_caps(false, false, 4096), false, 1),
        ];
        
        let selected = ProviderSelector::select(&required, &providers);
        assert!(selected.is_none());
    }

    #[test]
    fn test_filters_by_capabilities() {
        let required = RequiredCapabilities {
            vision: true,
            ..Default::default()
        };
        let providers = vec![
            (ProviderId::Ollama, mock_caps(false, false, 4096), true, 1),
            (ProviderId::Gemini, mock_caps(true, false, 4096), true, 2),
        ];
        
        let selected = ProviderSelector::select(&required, &providers);
        assert_eq!(selected, Some(ProviderId::Gemini));
    }

    #[test]
    fn test_respects_priority() {
        let required = RequiredCapabilities::default();
        let providers = vec![
            (ProviderId::Gemini, mock_caps(true, false, 4096), true, 2),
            (ProviderId::Ollama, mock_caps(true, false, 4096), true, 1),
        ];
        
        let selected = ProviderSelector::select(&required, &providers);
        assert_eq!(selected, Some(ProviderId::Ollama));
    }
}

use crate::providers::ProviderCapabilities;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelClass {
    Tiny,
    Small,
    Medium,
    Large,
}

#[derive(Debug, Clone)]
pub struct ModelProfile {
    pub model_name: String,
    pub class: ModelClass,
    pub parameter_count: Option<u64>,
    pub context_length: usize,
    pub quantization_level: Option<String>,
    pub embedding_length: Option<usize>,
}

#[derive(Debug, Clone, Copy)]
pub struct RetrievalBudget {
    pub safe_budget: usize,
    pub conversation_budget: usize,
    pub project_budget: usize,
    pub generation_margin: usize,
    pub project_top_k: usize,
}

impl ModelProfile {
    pub fn from_capabilities(model_name: String, caps: &ProviderCapabilities) -> Self {
        let context_length = caps.max_context_tokens.max(2048);
        let class = classify_model(caps.parameter_count);
        Self {
            model_name,
            class,
            parameter_count: caps.parameter_count,
            context_length,
            quantization_level: caps.quantization_level.clone(),
            embedding_length: caps.embedding_length,
        }
    }

    pub fn retrieval_budget(&self) -> RetrievalBudget {
        let safe_budget = self.context_length.saturating_mul(70) / 100;
        let mut conversation_budget = safe_budget.saturating_mul(40) / 100;
        let mut project_budget = safe_budget.saturating_mul(40) / 100;
        let generation_margin = safe_budget.saturating_sub(conversation_budget + project_budget);

        let project_top_k = match self.class {
            ModelClass::Tiny => {
                project_budget = safe_budget / 3;
                conversation_budget = safe_budget / 3;
                4
            }
            ModelClass::Small => 6,
            ModelClass::Medium => 8,
            ModelClass::Large => 12,
        };

        RetrievalBudget {
            safe_budget,
            conversation_budget,
            project_budget,
            generation_margin,
            project_top_k,
        }
    }
}

pub fn classify_model(parameter_count: Option<u64>) -> ModelClass {
    match parameter_count.unwrap_or(0) {
        0 => ModelClass::Small,
        v if v < 2_000_000_000 => ModelClass::Tiny,
        v if v < 7_000_000_000 => ModelClass::Small,
        v if v < 13_000_000_000 => ModelClass::Medium,
        _ => ModelClass::Large,
    }
}

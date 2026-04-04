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
        let context_length = (caps.context_length as usize)
            .max(caps.max_context_tokens)
            .max(2048);
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

    pub fn quantization_factor(&self) -> f32 {
        let q = self
            .quantization_level
            .as_deref()
            .unwrap_or("F16")
            .to_ascii_uppercase();
        if q.contains("Q2") || q.contains("2BIT") {
            0.6
        } else if q.contains("Q3") || q.contains("3BIT") {
            0.7
        } else if q.contains("Q4") || q.contains("4BIT") {
            0.85
        } else if q.contains("Q5") || q.contains("5BIT") {
            0.9
        } else if q.contains("Q6") || q.contains("6BIT") {
            0.95
        } else if q.contains("Q8") || q.contains("8BIT") {
            1.0
        } else if q.contains("F16") || q.contains("BF16") {
            1.1
        } else if q.contains("F32") {
            1.2
        } else {
            1.0
        }
    }

    pub fn retrieval_budget(&self) -> RetrievalBudget {
        let factor = self.quantization_factor();
        let base_safe_pct = (70.0 * factor).clamp(50.0, 85.0);
        let safe_budget = (self.context_length as f32 * base_safe_pct / 100.0) as usize;

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

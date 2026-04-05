use std::sync::Arc;
use crate::context_engine::{ContextEngine, ContextRetrievalConfig, RetrievalResult};
use crate::router::ProviderRouter;
use crate::providers::{ProviderId, PromptOptions, LLMResponse};
use crate::skills::{SkillOrchestrator, Skill};

pub async fn retrieve_context(
    engine: &Arc<dyn ContextEngine>,
    project_context: &str,
    query: &str,
    token_budget: usize,
) -> Result<RetrievalResult, String> {
    let config = ContextRetrievalConfig::default();
    Ok(engine.retrieve(project_context, query, token_budget, None, &config).await)
}

pub async fn execute_skill(
    orchestrator: &Arc<SkillOrchestrator>,
    prompt: &str,
) -> Result<Option<Skill>, String> {
    // For now, we just find the skill. Real execution would happen here.
    Ok(orchestrator.find_matching_skill(prompt).cloned())
}

pub async fn call_llm(
    router: &Arc<ProviderRouter>,
    provider: ProviderId,
    prompt: String,
) -> Result<LLMResponse, String> {
    let options = PromptOptions::default();
    router.send(provider, prompt, options).await.map_err(|e| e.to_string())
}

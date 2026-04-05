use std::sync::Arc;
use crate::context_engine::parser::chunk_extractor::CodeChunk;
use crate::orchestrator::planner::{Action, Plan};
use crate::router::ProviderRouter;
use crate::providers::{ProviderId, ProviderCapabilities, PromptOptions};
use crate::skills::SkillOrchestrator;
use crate::context_engine::ContextEngine;
use crate::orchestrator::tools;
use crate::orchestrator::provider_selector::{ProviderSelector, RequiredCapabilities};

pub struct ExecutionContext {
    pub context: Option<Vec<CodeChunk>>,
    pub intermediate_results: Vec<String>,
}

impl ExecutionContext {
    pub fn new() -> Self {
        Self {
            context: None,
            intermediate_results: Vec::new(),
        }
    }
}

impl Default for ExecutionContext {
    fn default() -> Self {
        Self::new()
    }
}

pub struct Executor {
    router: Arc<ProviderRouter>,
    skill_orchestrator: Arc<SkillOrchestrator>,
    context_engine: Arc<dyn ContextEngine>,
}

impl Executor {
    pub fn new(
        router: Arc<ProviderRouter>,
        skill_orchestrator: Arc<SkillOrchestrator>,
        context_engine: Arc<dyn ContextEngine>,
    ) -> Self {
        Self {
            router,
            skill_orchestrator,
            context_engine,
        }
    }

    pub async fn execute(&self, plan: Plan) -> Result<String, String> {
        let mut exec_context = ExecutionContext::new();
        let mut last_output = String::new();

        for step in plan.steps {
            match step.action {
                Action::RetrieveContext => {
                    let result = tools::retrieve_context(
                        &self.context_engine,
                        &plan.goal,
                        &plan.goal,
                        2048
                    ).await?;
                    
                    exec_context.intermediate_results.push(format!("Retrieved context from {} files", result.selected_files.len()));
                    last_output = result.context;
                }
                Action::ExecuteSkill => {
                    if let Some(skill) = tools::execute_skill(&self.skill_orchestrator, &plan.goal).await? {
                        exec_context.intermediate_results.push(format!("Executed skill: {}", skill.manifest.name));
                        last_output = format!("Skill {} analysis results placeholder", skill.manifest.name);
                    } else {
                        exec_context.intermediate_results.push("No matching skill found".to_string());
                    }
                }
                Action::GenerateResponse => {
                    // Provider selection
                    let required = RequiredCapabilities {
                        vision: false,
                        audio: false,
                        thinking: false,
                        tools: false,
                        fim: false,
                        min_context_length: 4096,
                    };

                    // In a real implementation, we would get this from the router/registry.
                    // For now, we simulate the available providers with their capabilities.
                    let available_providers = vec![
                        (
                            ProviderId::Ollama, 
                            ProviderCapabilities::default_with_context(4096), 
                            true, 
                            1
                        ),
                        (
                            ProviderId::Gemini, 
                            ProviderCapabilities::default_with_context(128000), 
                            true, 
                            2
                        ),
                    ];

                    let selected_provider = ProviderSelector::select(&required, &available_providers)
                        .unwrap_or(ProviderId::Ollama);

                    let prompt = format!("Goal: {}\n\nContext:\n{}\n\nIntermediate results:\n{}", 
                        plan.goal, 
                        last_output,
                        exec_context.intermediate_results.join("\n")
                    );
                    
                    let options = PromptOptions::default();
                    let response = self.router.send(selected_provider, prompt, options).await
                        .map_err(|e| format!("LLM error: {:?}", e))?;
                    
                    last_output = response.text;
                }
            }
        }

        Ok(last_output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::ProviderRouter;
    use crate::skills::SkillOrchestrator;
    use crate::context_engine::ContextEngineV1;
    use crate::orchestrator::planner::MinimalPlanner;

    #[tokio::test]
    async fn test_executor_basic() {
        let router = Arc::new(ProviderRouter::new());
        let skill_orchestrator = Arc::new(SkillOrchestrator::new(Vec::new()));
        let context_engine = Arc::new(ContextEngineV1);
        
        let executor = Executor::new(router, skill_orchestrator, context_engine);
        let plan = MinimalPlanner::plan("test goal");
        
        // This will fail because router has no providers registered, but it verifies the logic flow
        let result = executor.execute(plan).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("LLM error"));
    }
}

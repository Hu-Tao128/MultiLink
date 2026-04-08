use std::sync::Arc;
use crate::context_engine::parser::chunk_extractor::CodeChunk;
use crate::orchestrator::planner::{Action, Plan};
use crate::router::ProviderRouter;
use crate::providers::{ProviderId, PromptOptions};
use crate::skills::SkillOrchestrator;
use crate::context_engine::ContextEngine;
use crate::orchestrator::old_tools;
use crate::orchestrator::provider_selector::ProviderSelector;
use crate::tools::ToolExecutor;

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
    tool_executor: Arc<ToolExecutor>,
}

impl Executor {
    pub fn new(
        router: Arc<ProviderRouter>,
        skill_orchestrator: Arc<SkillOrchestrator>,
        context_engine: Arc<dyn ContextEngine>,
        tool_executor: Arc<ToolExecutor>,
    ) -> Self {
        Self {
            router,
            skill_orchestrator,
            context_engine,
            tool_executor,
        }
    }

    pub async fn execute(&self, plan: Plan) -> Result<String, String> {
        let mut exec_context = ExecutionContext::new();
        let mut last_output = String::new();

        for step in plan.steps {
            match step.action {
                Action::RetrieveContext => {
                    let result = old_tools::retrieve_context(
                        &self.context_engine,
                        &plan.goal,
                        &plan.goal,
                        2048
                    ).await?;
                    
                    exec_context.intermediate_results.push(format!("Retrieved context from {} files", result.selected_files.len()));
                    last_output = result.context;
                }
                Action::ExecuteSkill => {
                    if let Some(skill) = old_tools::execute_skill(&self.skill_orchestrator, &plan.goal).await? {
                        exec_context.intermediate_results.push(format!("Executed skill: {}", skill.manifest.name));
                        last_output = format!("Skill {} analysis results placeholder", skill.manifest.name);
                    } else {
                        exec_context.intermediate_results.push("No matching skill found".to_string());
                    }
                }
                Action::GenerateResponse { capabilities: required } => {
                    let available_providers = self.router.get_available_providers().await;
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
                Action::ToolCall { name, input } => {
                    let result = self.tool_executor.execute(&name, input).await;
                    let json_output = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
                    exec_context.intermediate_results.push(format!("Tool {} result: {}", name, json_output));
                    last_output = json_output;
                }
            }
        }

        Ok(last_output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;
    use crate::router::ProviderRouter;
    use crate::skills::SkillOrchestrator;
    use crate::context_engine::ContextEngineV1;
    use crate::orchestrator::planner::MinimalPlanner;
    use crate::tools::{ToolExecutor, ToolRegistry};

    #[tokio::test]
    async fn test_executor_basic() {
        let router = Arc::new(ProviderRouter::new());
        let skill_orchestrator = Arc::new(SkillOrchestrator::new(Vec::new()));
        let context_engine = Arc::new(ContextEngineV1);
        
        let registry = Arc::new(ToolRegistry::new(PathBuf::from(".")));
        let tool_executor = Arc::new(ToolExecutor::new(registry));
        
        let executor = Executor::new(router, skill_orchestrator, context_engine, tool_executor);
        let plan = MinimalPlanner::plan("test goal");
        
        let result = executor.execute(plan).await;
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("LLM error"));
    }
}
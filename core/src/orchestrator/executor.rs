use std::sync::Arc;
use crate::context_engine::parser::chunk_extractor::CodeChunk;
use crate::orchestrator::planner::{Action, MultiStepPlan, Plan, Step};
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

    pub async fn execute_multi_step(&self, plan: MultiStepPlan) -> Result<String, String> {
        self.execute_multi_step_with_limit(plan, crate::orchestrator::planner::DEFAULT_MAX_STEPS).await
    }

    pub async fn execute_multi_step_with_limit(&self, plan: MultiStepPlan, max_steps: usize) -> Result<String, String> {
        let mut last_output = String::new();
        let mut context = Vec::new();
        let mut steps = plan.steps;
        let mut step_index = 0;

        while step_index < steps.len() && context.len() < max_steps {
            let step = steps.remove(0);
            
            match step {
                Step::ToolCall { name, input } => {
                    let input_json = serde_json::to_string(&input).unwrap_or_default();
                    let result = self.tool_executor.execute(&name, input.clone()).await;
                    let json_output = serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
                    
                    let metadata = Some(crate::orchestrator::planner::StepMetadata::new(
                        context.len(),
                        Some(&name),
                        Some(input_json.clone()),
                    ));
                    
                    context.push(crate::orchestrator::planner::StepResult {
                        step_index: context.len(),
                        step_id: format!("step_{}", context.len()),
                        output: json_output.clone(),
                        tool_name: Some(name.clone()),
                        metadata,
                    });
                    
                    if name == "search_code" || name == "search_and_open" {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_output) {
                            let results_count = parsed.get("results")
                                .or_else(|| parsed.get("files"))
                                .and_then(|v| v.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);
                            
                            if results_count == 1 && name == "search_code" {
                                if let Some(first_result) = parsed.get("results")
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.first())
                                {
                                    if let Some(path) = first_result.get("path").and_then(|v| v.as_str()) {
                                        steps.insert(0, Step::ToolCall {
                                            name: "open_file".to_string(),
                                            input: crate::tools::ToolInput {
                                                path: Some(path.to_string()),
                                                pattern: None,
                                                args: None,
                                            },
                                        });
                                    }
                                }
                            }
                        }
                    }
                    
                    last_output = json_output;
                    step_index += 1;
                }
                Step::LLMCall { prompt } => {
                    last_output = self.execute_llm_step(context.len(), &prompt, &context).await?;
                    
                    let metadata = Some(crate::orchestrator::planner::StepMetadata::for_llm(context.len()));
                    context.push(crate::orchestrator::planner::StepResult {
                        step_index: context.len(),
                        step_id: format!("step_{}", context.len()),
                        output: last_output.clone(),
                        tool_name: None,
                        metadata,
                    });
                    step_index += 1;
                }
                Step::DecideNext => {
                    let decision = self.execute_decision_step(&plan.goal, &context).await?;
                    let decision_json = serde_json::to_string(&decision).unwrap_or_default();
                    
                    match decision.next_action.as_str() {
                        "done" => {
                            context.push(crate::orchestrator::planner::StepResult {
                                step_index: context.len(),
                                step_id: format!("step_{}", context.len()),
                                output: decision_json,
                                tool_name: Some("decide_next".to_string()),
                                metadata: None,
                            });
                            break;
                        }
                        "tool" => {
                            if let (Some(tool_name), Some(tool_input)) = (decision.tool.clone(), decision.input.clone()) {
                                steps.insert(0, Step::ToolCall {
                                    name: tool_name,
                                    input: tool_input,
                                });
                            }
                        }
                        "llm" => {
                            if let Some(input) = decision.input.clone() {
                                if let Some(prompt) = input.pattern {
                                    steps.insert(0, Step::LLMCall { prompt });
                                }
                            }
                        }
                        _ => {}
                    }
                    step_index += 1;
                }
            }
        }

        Ok(last_output)
    }

    async fn execute_decision_step(&self, goal: &str, context: &[crate::orchestrator::planner::StepResult]) -> Result<crate::orchestrator::planner::DecisionResult, String> {
        let available_providers = self.router.get_available_providers().await;
        let selected_provider = ProviderSelector::select(
            &crate::orchestrator::provider_selector::RequiredCapabilities::new(),
            &available_providers
        ).unwrap_or(ProviderId::Ollama);

        let structured_context = if !context.is_empty() {
            let ctx_items: Vec<serde_json::Value> = context.iter().map(|r| {
                serde_json::json!({
                    "step_id": r.step_id,
                    "tool_name": r.tool_name,
                    "output": r.output
                })
            }).collect();
            serde_json::to_string_pretty(&ctx_items).unwrap_or_default()
        } else {
            String::new()
        };

        let decision_prompt = format!(
            r#"You are a decision engine. Based on the goal and previous tool results, decide what to do next.

Goal: {}

Previous tool results (JSON):
{}

Your response must be a JSON object with this structure:
{{
    "next_action": "tool" | "llm" | "done",
    "tool": "tool_name_if_tool" (optional),
    "input": {{"path": "...", "pattern": "...", "args": ...}} (optional),
    "reason": "why you made this decision"
}}

- If task is complete, set next_action to "done"
- If need more info, set next_action to "tool" and specify tool name and input
- If need analysis, set next_action to "llm" and provide prompt

Respond ONLY with valid JSON, no other text."#,
            goal,
            structured_context
        );

        let options = PromptOptions::default();
        let response = self.router.send(selected_provider, decision_prompt, options).await
            .map_err(|e| format!("LLM decision error: {:?}", e))?;

        let decision = crate::orchestrator::planner::DecisionResult::parse_from_json(&response.text)
            .unwrap_or_else(|| crate::orchestrator::planner::DecisionResult::done(Some("Failed to parse decision".to_string())));

        Ok(decision)
    }

    async fn execute_llm_step(&self, _step_index: usize, prompt: &str, context: &[crate::orchestrator::planner::StepResult]) -> Result<String, String> {
        let available_providers = self.router.get_available_providers().await;
        let selected_provider = ProviderSelector::select(
            &crate::orchestrator::provider_selector::RequiredCapabilities::new(),
            &available_providers
        ).unwrap_or(ProviderId::Ollama);

        let structured_context = if !context.is_empty() {
            let ctx_items: Vec<serde_json::Value> = context.iter().map(|r| {
                serde_json::json!({
                    "step_id": r.step_id,
                    "tool_name": r.tool_name,
                    "output": r.output
                })
            }).collect();
            serde_json::to_string_pretty(&ctx_items).unwrap_or_default()
        } else {
            String::new()
        };

        let reasoning_instruction = "Use previous tool results to understand what was done and build upon them.";
        
        let full_prompt = if structured_context.is_empty() {
            format!("{}\n\n{}", reasoning_instruction, prompt)
        } else {
            format!(
                "{}\n\nTask: {}\n\nPrevious tool results (structured JSON):\n{}\n\nProvide a response that builds on the tool results.",
                reasoning_instruction,
                prompt,
                structured_context
            )
        };

        let options = PromptOptions::default();
        let response = self.router.send(selected_provider, full_prompt, options).await
            .map_err(|e| format!("LLM error: {:?}", e))?;

        Ok(response.text)
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
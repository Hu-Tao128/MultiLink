use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use crate::context_engine::parser::chunk_extractor::CodeChunk;
use crate::context_engine::ContextEngine;
use crate::orchestrator::llm_tool_selector::LlmToolSelector;
use crate::orchestrator::model_strategy::{ExecutionConfig, ModelSize};
use crate::orchestrator::old_tools;
use crate::orchestrator::planner::{Action, MinimalPlanner, MultiStepPlan, Plan, Step};
use crate::orchestrator::provider_selector::ProviderSelector;
use crate::providers::{PromptOptions, ProviderId};
use crate::router::ProviderRouter;
use crate::skills::SkillOrchestrator;
use crate::tools::description::ToolDescription;
use crate::tools::{ToolExecutor, ToolInput};

const MAX_TOOL_RESULT_LINES: usize = 500;

pub struct ExecutionContext {
    pub context: Option<Vec<CodeChunk>>,
    pub intermediate_results: Vec<String>,
    pub max_steps: usize,
    pub model_size: ModelSize,
    pub available_tools: Vec<String>,
    pub tool_selector: Option<Arc<LlmToolSelector>>,
}

impl ExecutionContext {
    pub fn new() -> Self {
        Self {
            context: None,
            intermediate_results: Vec::new(),
            max_steps: 10,
            model_size: ModelSize::Medium,
            available_tools: ModelSize::Medium
                .allowed_tools()
                .into_iter()
                .map(String::from)
                .collect(),
            tool_selector: None,
        }
    }

    pub fn with_config(model_size: ModelSize, config: &ExecutionConfig) -> Self {
        Self {
            context: None,
            intermediate_results: Vec::new(),
            max_steps: config.max_steps,
            model_size,
            available_tools: model_size
                .allowed_tools()
                .into_iter()
                .map(String::from)
                .collect(),
            tool_selector: None,
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
    tool_selector: Option<Arc<LlmToolSelector>>,
    tool_descriptions: Vec<ToolDescription>,
    working_dir: Option<std::path::PathBuf>,
}

impl Executor {
    pub fn new(
        router: Arc<ProviderRouter>,
        skill_orchestrator: Arc<SkillOrchestrator>,
        context_engine: Arc<dyn ContextEngine>,
        tool_executor: Arc<ToolExecutor>,
    ) -> Self {
        let tool_descriptions = crate::tools::description::load_all_descriptions()
            .values()
            .cloned()
            .collect();
        Self {
            router,
            skill_orchestrator,
            context_engine,
            tool_executor,
            tool_selector: None,
            tool_descriptions,
            working_dir: None,
        }
    }

    pub fn with_tool_selector(
        mut self,
        router: Arc<ProviderRouter>,
        model_size: ModelSize,
        model_name: Option<String>,
    ) -> Self {
        let tool_selector = Arc::new(LlmToolSelector::new(
            router,
            self.tool_descriptions.clone(),
            model_size,
            model_name,
        ));
        self.tool_selector = Some(tool_selector);
        self
    }

    pub fn with_working_dir(mut self, dir: std::path::PathBuf) -> Self {
        self.working_dir = Some(dir);
        self
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
                        2048,
                    )
                    .await?;

                    exec_context.intermediate_results.push(format!(
                        "Retrieved context from {} files",
                        result.selected_files.len()
                    ));
                    last_output = result.context;
                }
                Action::ExecuteSkill => {
                    if let Some(skill) =
                        old_tools::execute_skill(&self.skill_orchestrator, &plan.goal).await?
                    {
                        exec_context
                            .intermediate_results
                            .push(format!("Executed skill: {}", skill.manifest.name));
                        last_output =
                            format!("Skill {} analysis results placeholder", skill.manifest.name);
                    } else {
                        exec_context
                            .intermediate_results
                            .push("No matching skill found".to_string());
                    }
                }
                Action::GenerateResponse {
                    capabilities: required,
                } => {
                    let available_providers = self.router.get_available_providers().await;
                    let selected_provider =
                        ProviderSelector::select(&required, &available_providers)
                            .unwrap_or(ProviderId::Ollama);

                    let prompt = format!(
                        "Goal: {}\n\nContext:\n{}\n\nIntermediate results:\n{}",
                        plan.goal,
                        last_output,
                        exec_context.intermediate_results.join("\n")
                    );

                    let options = PromptOptions::default();
                    let response = self
                        .router
                        .send(selected_provider, prompt, options)
                        .await
                        .map_err(|e| format!("LLM error: {:?}", e))?;

                    last_output = response.text;
                }
                Action::ToolCall { name, input } => {
                    let result = self.tool_executor.execute(&name, input).await;
                    let json_output =
                        serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());
                    exec_context
                        .intermediate_results
                        .push(format!("Tool {} result: {}", name, json_output));
                    last_output = json_output;
                }
            }
        }

        Ok(last_output)
    }

    pub async fn execute_multi_step(&self, plan: MultiStepPlan) -> Result<String, String> {
        self.execute_multi_step_with_limit(plan, crate::orchestrator::planner::DEFAULT_MAX_STEPS)
            .await
    }

    pub async fn execute_multi_step_with_limit(
        &self,
        plan: MultiStepPlan,
        max_steps: usize,
    ) -> Result<String, String> {
        let plan_intent = crate::orchestrator::planner::classify_intent(&plan.goal);
        let raw_mode = plan.raw_mode;
        let mut last_output = String::new();
        let mut context = Vec::new();
        let mut steps = plan.steps;
        let mut step_index = 0;

        eprintln!(
            "[executor] starting raw_mode={} intent={:?} goal={}",
            raw_mode, plan_intent, plan.goal
        );

        while step_index < steps.len() && context.len() < max_steps {
            let step = steps.remove(0);

            match step {
                Step::ToolCall { name, input } => {
                    eprintln!(
                        "[planner] tool_step intent={:?} selected_tool={} executed=true",
                        plan_intent, name
                    );
                    let input_json = serde_json::to_string(&input).unwrap_or_default();
                    let result = self.tool_executor.execute(&name, input.clone()).await;
                    let tool_failed = !result.success;
                    let json_output =
                        serde_json::to_string(&result).unwrap_or_else(|_| "{}".to_string());

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

                    // Auto-validate after write/edit operations
                    if !tool_failed && (name == "write_file" || name == "apply_patch") {
                        let val_cmds =
                            load_validation_commands_from_multilink(&self.tool_executor).await;
                        if let Some(first_cmd) = val_cmds.first().cloned() {
                            eprintln!(
                                "[executor] auto-validate after {}: running '{}'",
                                name, first_cmd
                            );
                            steps.insert(
                                0,
                                Step::ToolCall {
                                    name: "run_command".to_string(),
                                    input: crate::tools::ToolInput {
                                        path: None,
                                        pattern: None,
                                        args: Some(
                                            vec![(
                                                "command".to_string(),
                                                serde_json::Value::String(first_cmd),
                                            )]
                                            .into_iter()
                                            .collect(),
                                        ),
                                    },
                                },
                            );
                        }
                    }

                    if tool_failed {
                        eprintln!(
                            "[planner] tool_failure intent={:?} selected_tool={} fallback=context_engine",
                            plan_intent, name
                        );

                        let fallback = old_tools::retrieve_context(
                            &self.context_engine,
                            &plan.goal,
                            &plan.goal,
                            2048,
                        )
                        .await;

                        match fallback {
                            Ok(result) => {
                                let fallback_output = serde_json::json!({
                                    "success": true,
                                    "fallback": "context_engine",
                                    "selected_files": result.selected_files,
                                    "context": result.context
                                })
                                .to_string();

                                context.push(crate::orchestrator::planner::StepResult {
                                    step_index: context.len(),
                                    step_id: format!("step_{}", context.len()),
                                    output: fallback_output.clone(),
                                    tool_name: Some("context_engine_fallback".to_string()),
                                    metadata: None,
                                });
                                last_output = fallback_output;
                            }
                            Err(err) => {
                                return Err(format!(
                                    "Tool {} failed and context fallback also failed: {}",
                                    name, err
                                ));
                            }
                        }

                        step_index += 1;
                        continue;
                    }

                    if name == "search_code" || name == "search_and_open" {
                        if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(&json_output)
                        {
                            let results_count = parsed
                                .get("results")
                                .or_else(|| parsed.get("files"))
                                .and_then(|v| v.as_array())
                                .map(|a| a.len())
                                .unwrap_or(0);

                            if results_count == 1 && name == "search_code" {
                                if let Some(first_result) = parsed
                                    .get("results")
                                    .and_then(|v| v.as_array())
                                    .and_then(|a| a.first())
                                {
                                    if let Some(path) =
                                        first_result.get("path").and_then(|v| v.as_str())
                                    {
                                        steps.insert(
                                            0,
                                            Step::ToolCall {
                                                name: "open_file".to_string(),
                                                input: crate::tools::ToolInput {
                                                    path: Some(path.to_string()),
                                                    pattern: None,
                                                    args: None,
                                                },
                                            },
                                        );
                                    }
                                }
                            }
                        }
                    }

                    last_output = json_output;
                    step_index += 1;
                }
                Step::LLMCall { prompt } => {
                    eprintln!(
                        "[executor] llm_step intent={:?} raw_mode={} executed=true",
                        plan_intent, raw_mode
                    );
                    last_output = self
                        .execute_llm_step(context.len(), &prompt, &context)
                        .await?;

                    let metadata = Some(crate::orchestrator::planner::StepMetadata::for_llm(
                        context.len(),
                    ));
                    context.push(crate::orchestrator::planner::StepResult {
                        step_index: context.len(),
                        step_id: format!("step_{}", context.len()),
                        output: last_output.clone(),
                        tool_name: None,
                        metadata,
                    });
                    step_index += 1;
                }
                Step::DecideNext {
                    ref goal,
                    ref available_tools,
                    ..
                } => {
                    let decision = self
                        .execute_decision_step(goal, &context, available_tools)
                        .await?;
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
                            if let (Some(tool_name), Some(tool_input)) =
                                (decision.tool.clone(), decision.input.clone())
                            {
                                steps.insert(
                                    0,
                                    Step::ToolCall {
                                        name: tool_name,
                                        input: tool_input,
                                    },
                                );
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

    pub async fn execute_with_dynamic_loop(
        &self,
        initial_goal: &str,
        exec_context: &mut ExecutionContext,
    ) -> Result<String, String> {
        let mut step_results: Vec<crate::orchestrator::planner::StepResult> = vec![];
        let mut iterations = 0;
        let max_steps = exec_context.max_steps;
        let available_tools = exec_context.available_tools.clone();
        let doom_window: usize = 3;
        let mut successful_write_paths: Vec<String> = Vec::new();
        let mut successful_write_signatures: HashSet<String> = HashSet::new();
        let mut written_contents: HashMap<String, String> = HashMap::new();
        let mut duplicate_write_skips = 0usize;

        let tool_selector = exec_context
            .tool_selector
            .as_ref()
            .cloned()
            .or_else(|| self.tool_selector.clone());

        loop {
            if iterations >= max_steps {
                return Err(format!("Max steps ({}) reached", max_steps));
            }

            if let Some(ref selector) = tool_selector {
                let tool_call = selector
                    .select_next_tool(initial_goal, &step_results, &available_tools)
                    .await?;

                if tool_call.tool.is_empty() {
                    if !successful_write_paths.is_empty()
                        && missing_required_assets(initial_goal, &successful_write_paths).is_empty()
                    {
                        return Ok(tools_complete_summary(
                            "files written",
                            &successful_write_paths,
                        ));
                    }

                    let missing = missing_required_assets(initial_goal, &successful_write_paths);
                    if !missing.is_empty() && iterations + 1 < max_steps {
                        step_results.push(crate::orchestrator::planner::StepResult {
                            step_index: iterations,
                            step_id: format!("step_{}", iterations),
                            output: format!(
                                "Task is not complete yet. Missing required files: {}. Continue with write_file for the missing files.",
                                missing.join(", ")
                            ),
                            tool_name: None,
                            metadata: None,
                        });
                        iterations += 1;
                        continue;
                    }

                    return Ok(format!("Goal achieved: {}", tool_call.reasoning));
                }

                let tool_signature = write_tool_signature(&tool_call.tool, &tool_call.args);
                if let Some(signature) = tool_signature.as_ref() {
                    if successful_write_signatures.contains(signature) {
                        duplicate_write_skips += 1;
                        let path =
                            tool_path(&tool_call.args).unwrap_or_else(|| "unknown".to_string());
                        step_results.push(crate::orchestrator::planner::StepResult {
                            step_index: iterations,
                            step_id: format!("step_{}", iterations),
                            output: format!(
                                "Skipped duplicate {} for path '{}'. Do not write this path again; choose a missing file or return null if complete.",
                                tool_call.tool, path
                            ),
                            tool_name: Some(format!("Tool: {}", tool_call.tool)),
                            metadata: Some(crate::orchestrator::planner::StepMetadata::new(
                                iterations,
                                Some(&tool_call.tool),
                                Some(serde_json::to_string(&tool_call.args).unwrap_or_default()),
                            )),
                        });

                        if duplicate_write_skips >= 2 {
                            if self
                                .auto_write_missing_web_assets(
                                    initial_goal,
                                    &mut successful_write_paths,
                                    &mut successful_write_signatures,
                                    &mut written_contents,
                                )
                                .await?
                            {
                                return Ok(tools_complete_summary(
                                    "files written",
                                    &successful_write_paths,
                                ));
                            }
                            return Ok(tools_complete_summary(
                                "stopped after duplicate write selections",
                                &successful_write_paths,
                            ));
                        }

                        iterations += 1;
                        continue;
                    }
                }

                let recent_tools: Vec<&str> = step_results
                    .iter()
                    .rev()
                    .take(doom_window)
                    .filter_map(|r| {
                        r.tool_name
                            .as_deref()
                            .and_then(|n| n.strip_prefix("Tool: "))
                    })
                    .collect();
                let recent_errors: Vec<&str> = step_results
                    .iter()
                    .rev()
                    .take(doom_window)
                    .filter_map(|r| {
                        if r.output.contains("TOOL FAILED:") {
                            Some(r.output.as_str())
                        } else {
                            None
                        }
                    })
                    .collect();

                if recent_tools.len() >= doom_window
                    && recent_tools.iter().all(|&t| t == tool_call.tool)
                {
                    eprintln!(
                        "[executor] DOOM LOOP detected: tool={} called {} times consecutively, breaking",
                        tool_call.tool, doom_window
                    );
                    return Ok(format!(
                        "Doom loop detected: {} called {} times. Goal may be blocked or ambiguous.",
                        tool_call.tool, doom_window
                    ));
                }
                if recent_errors.len() >= 2 && recent_errors.iter().all(|&e| e == recent_errors[0])
                {
                    eprintln!(
                        "[executor] DOOM LOOP detected: same error repeated twice, breaking. error={}",
                        recent_errors[0]
                    );
                    return Ok(format!(
                        "Doom loop detected: same error repeated. {}",
                        recent_errors[0]
                    ));
                }

                let tool_input = self.json_to_tool_input(&tool_call.args);
                let result = if let Some(ref dir) = self.working_dir {
                    self.tool_executor
                        .execute_with_root(&tool_call.tool, tool_input, dir)
                        .await
                } else {
                    self.tool_executor
                        .execute(&tool_call.tool, tool_input)
                        .await
                };
                let raw_output = serde_json::to_string(&result).unwrap_or_default();

                let is_search = matches!(
                    tool_call.tool.as_str(),
                    "search_code" | "search_and_open" | "fs_grep"
                );
                let is_empty = raw_output.contains("\"results\":[]")
                    || raw_output.contains("\"files\":[]")
                    || raw_output.trim().is_empty()
                    || raw_output.contains("No results");

                let augmented_output = if !result.success {
                    let err_msg = result.error.as_deref().unwrap_or("unknown error");
                    if err_msg.contains("Absolute paths") {
                        format!(
                            "TOOL FAILED: {} — Use ONLY relative paths like 'index.html' or 'src/main.rs'. Never use paths starting with '/'.",
                            err_msg
                        )
                    } else if err_msg.contains("not found") || err_msg.contains("does not exist") {
                        format!(
                            "TOOL FAILED: {} — Try a different file path or search for the correct path first.",
                            err_msg
                        )
                    } else {
                        format!("TOOL FAILED: {} — Try a different approach.", err_msg)
                    }
                } else if is_search && is_empty {
                    format!(
                        "Tool {} returned NO RESULTS. Do not call this tool again with similar args. Proceed to a different tool.",
                        tool_call.tool
                    )
                } else {
                    raw_output.clone()
                };

                step_results.push(crate::orchestrator::planner::StepResult {
                    step_index: iterations,
                    step_id: format!("step_{}", iterations),
                    output: augmented_output.clone(),
                    tool_name: Some(format!("Tool: {}", tool_call.tool)),
                    metadata: Some(crate::orchestrator::planner::StepMetadata::new(
                        iterations,
                        Some(&tool_call.tool),
                        Some(serde_json::to_string(&tool_call.args).unwrap_or_default()),
                    )),
                });

                if result.success {
                    eprintln!(
                        "[executor] dynamic_step tool={} success=true reasoning={}",
                        tool_call.tool, tool_call.reasoning
                    );
                } else {
                    eprintln!(
                        "[executor] dynamic_step tool={} success=false error={:?} — added corrective message to context",
                        tool_call.tool, result.error
                    );
                    if is_write_tool(&tool_call.tool)
                        && self
                            .auto_write_missing_web_assets(
                                initial_goal,
                                &mut successful_write_paths,
                                &mut successful_write_signatures,
                                &mut written_contents,
                            )
                            .await?
                    {
                        return Ok(tools_complete_summary(
                            "files written",
                            &successful_write_paths,
                        ));
                    }
                    iterations += 1;
                    continue;
                }

                if is_write_tool(&tool_call.tool) {
                    if let Some(path) = tool_path(&tool_call.args) {
                        if !successful_write_paths.contains(&path) {
                            successful_write_paths.push(path.clone());
                        }
                        if let Some(content) = tool_content(&tool_call.args) {
                            written_contents.insert(path, content);
                        }
                    }
                    if let Some(signature) = tool_signature {
                        successful_write_signatures.insert(signature);
                    }
                    duplicate_write_skips = 0;

                    if goal_requires_web_triplet(initial_goal)
                        && missing_required_assets(initial_goal, &successful_write_paths).is_empty()
                    {
                        return Ok(tools_complete_summary(
                            "files written",
                            &successful_write_paths,
                        ));
                    }

                    if self
                        .auto_write_missing_web_assets(
                            initial_goal,
                            &mut successful_write_paths,
                            &mut successful_write_signatures,
                            &mut written_contents,
                        )
                        .await?
                    {
                        return Ok(tools_complete_summary(
                            "files written",
                            &successful_write_paths,
                        ));
                    }
                }

                iterations += 1;

                if iterations >= max_steps {
                    let last_output = step_results
                        .last()
                        .map(|r| r.output.clone())
                        .unwrap_or_default();
                    return Ok(last_output);
                }
            } else {
                return Err("No LlmToolSelector configured; cannot run dynamic loop".to_string());
            }
        }
    }

    pub async fn execute_hybrid(
        &self,
        prompt: &str,
        model_size: ModelSize,
    ) -> Result<String, String> {
        let features = crate::orchestrator::planner::IntentFeatures::extract(prompt);
        let confidence = features.tool_confidence();
        let available_tools = model_size
            .allowed_tools()
            .into_iter()
            .map(String::from)
            .collect::<Vec<_>>();

        eprintln!(
            "[executor] execute_hybrid model_size={:?} confidence={:.2} is_read={} is_write={} needs_llm={} tools={}",
            model_size,
            confidence,
            features.is_read_operation,
            features.is_write_operation,
            features.requires_llm(),
            available_tools.len()
        );

        if confidence > 0.7 && !features.requires_llm() {
            let initial =
                crate::orchestrator::planner::create_initial_steps(prompt, available_tools.clone());
            eprintln!(
                "[executor] fast_path: heuristic match with confidence={:.2} steps={}",
                confidence,
                initial.len()
            );
            let plan =
                crate::orchestrator::planner::MultiStepPlan::new(initial, prompt.to_string(), true);
            return self.execute_multi_step(plan).await;
        }

        eprintln!(
            "[executor] dynamic_path: low confidence={:.2}, using LLM tool selection",
            confidence
        );

        let tool_selector = self.tool_selector.clone();
        if let Some(selector) = tool_selector {
            let mut exec_ctx = crate::orchestrator::executor::ExecutionContext::new();
            exec_ctx.tool_selector = Some(selector);
            exec_ctx.available_tools = available_tools;
            self.execute_with_dynamic_loop(prompt, &mut exec_ctx).await
        } else {
            let plan = MinimalPlanner::plan_multi_step(prompt);
            self.execute_multi_step(plan).await
        }
    }

    fn json_to_tool_input(&self, args: &serde_json::Value) -> crate::tools::ToolInput {
        use std::collections::HashMap;
        let path = args
            .get("path")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let pattern = args
            .get("pattern")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let args_map: Option<HashMap<String, serde_json::Value>> = args.as_object().map(|obj| {
            obj.iter()
                .filter(|(k, _)| *k != "path" && *k != "pattern")
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        });
        crate::tools::ToolInput {
            path,
            pattern,
            args: args_map.filter(|m| !m.is_empty()),
        }
    }

    async fn auto_write_missing_web_assets(
        &self,
        goal: &str,
        successful_write_paths: &mut Vec<String>,
        successful_write_signatures: &mut HashSet<String>,
        written_contents: &mut HashMap<String, String>,
    ) -> Result<bool, String> {
        if !goal_requires_web_triplet(goal)
            || missing_required_assets(goal, successful_write_paths).is_empty()
        {
            return Ok(false);
        }

        let Some(html_path) = successful_write_paths
            .iter()
            .find(|path| path.ends_with(".html"))
            .cloned()
        else {
            return Ok(false);
        };
        let Some(html_content) = written_contents.get(&html_path).cloned() else {
            return Ok(false);
        };

        let companions = infer_web_companion_paths(&html_path, &html_content);
        let mut wrote_any = false;
        for (path, content) in [
            (companions.css, default_css_asset(goal)),
            (companions.js, default_js_asset(goal)),
        ] {
            if successful_write_paths.contains(&path) {
                continue;
            }

            let mut args = HashMap::new();
            args.insert(
                "content".to_string(),
                serde_json::Value::String(content.clone()),
            );
            let input = ToolInput {
                path: Some(path.clone()),
                pattern: None,
                args: Some(args),
            };

            let result = if let Some(ref dir) = self.working_dir {
                self.tool_executor
                    .execute_with_root("write_file", input, dir)
                    .await
            } else {
                self.tool_executor.execute("write_file", input).await
            };

            if !result.success {
                return Err(result
                    .error
                    .unwrap_or_else(|| format!("failed to auto-write {}", path)));
            }

            eprintln!(
                "[executor] dynamic_step tool=write_file success=true reasoning=auto-created missing web companion asset path={}",
                path
            );
            successful_write_paths.push(path.clone());
            successful_write_signatures.insert(format!("write_file:{}", path));
            written_contents.insert(path, content);
            wrote_any = true;
        }

        Ok(wrote_any && missing_required_assets(goal, successful_write_paths).is_empty())
    }

    async fn execute_decision_step(
        &self,
        goal: &str,
        context: &[crate::orchestrator::planner::StepResult],
        available_tools: &[String],
    ) -> Result<crate::orchestrator::planner::DecisionResult, String> {
        let available_providers = self.router.get_available_providers().await;
        let selected_provider = ProviderSelector::select(
            &crate::orchestrator::provider_selector::RequiredCapabilities::new(),
            &available_providers,
        )
        .unwrap_or(ProviderId::Ollama);

        let structured_context = if !context.is_empty() {
            let ctx_items: Vec<serde_json::Value> = context
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "step_id": r.step_id,
                        "tool_name": r.tool_name,
                        "output": r.output
                    })
                })
                .collect();
            serde_json::to_string_pretty(&ctx_items).unwrap_or_default()
        } else {
            String::new()
        };

        let tools_list = if available_tools.is_empty() {
            "No tools available.".to_string()
        } else {
            available_tools.join(", ")
        };

        let decision_prompt = format!(
            r#"You are a decision engine. Based on the goal and previous tool results, decide what to do next.

Goal: {}

AVAILABLE TOOLS: {}

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
- If need more info, and the needed tool is in the AVAILABLE TOOLS list, set next_action to "tool" and specify the tool name and input
- If need analysis, set next_action to "llm" and provide prompt
- DO NOT invent tool names — only use tools from the AVAILABLE TOOLS list

Respond ONLY with valid JSON, no other text."#,
            goal, tools_list, structured_context
        );

        let options = PromptOptions::default();
        let response = self
            .router
            .send(selected_provider, decision_prompt, options)
            .await
            .map_err(|e| format!("LLM decision error: {:?}", e))?;

        let decision =
            crate::orchestrator::planner::DecisionResult::parse_from_json(&response.text)
                .unwrap_or_else(|| {
                    crate::orchestrator::planner::DecisionResult::done(Some(
                        "Failed to parse decision".to_string(),
                    ))
                });

        Ok(decision)
    }

    async fn execute_llm_step(
        &self,
        _step_index: usize,
        prompt: &str,
        context: &[crate::orchestrator::planner::StepResult],
    ) -> Result<String, String> {
        let available_providers = self.router.get_available_providers().await;
        let selected_provider = ProviderSelector::select(
            &crate::orchestrator::provider_selector::RequiredCapabilities::new(),
            &available_providers,
        )
        .unwrap_or(ProviderId::Ollama);

        let injected_tool_context = build_tool_context_message(context);
        let structured_context = if !context.is_empty() {
            let ctx_items: Vec<serde_json::Value> = context
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "step_id": r.step_id,
                        "tool_name": r.tool_name,
                        "output": r.output
                    })
                })
                .collect();
            serde_json::to_string_pretty(&ctx_items).unwrap_or_default()
        } else {
            String::new()
        };

        let system_instruction = "You are a constrained executor. You MUST answer ONLY using the provided tool results as ground truth. Do NOT explain unrelated concepts. Do NOT suggest improvements. Do NOT hallucinate code.".to_string();

        let full_prompt = if injected_tool_context.is_empty() && structured_context.is_empty() {
            prompt.to_string()
        } else {
            let mut assembled = String::new();

            if !injected_tool_context.is_empty() {
                assembled.push_str("=== TOOL RESULTS (GROUND TRUTH) ===\n");
                assembled.push_str(&injected_tool_context);
                assembled.push_str("\n=== END TOOL RESULTS ===\n\n");
            }

            assembled.push_str("=== INSTRUCTIONS ===\n");
            assembled.push_str("You MUST answer ONLY using the tool results above.\n\n");
            assembled.push_str("If the answer is not explicitly present in the tool results, respond EXACTLY with:\n");
            assembled.push_str("[Not found in provided context]\n\n");
            assembled.push_str("Do NOT:\n");
            assembled.push_str("- explain unrelated concepts\n");
            assembled.push_str("- suggest improvements\n");
            assembled.push_str("- hallucinate code\n");
            assembled.push_str("- add commentary beyond what was asked\n\n");

            use std::fmt::Write;

            assembled.push_str("=== TASK ===\n");
            write!(assembled, "{}", prompt).ok();
            assembled
        };

        eprintln!(
            "[executor] full_prompt_chars={} tool_context_chars={}",
            full_prompt.len(),
            injected_tool_context.len()
        );

        let options = PromptOptions {
            system_prompt: Some(system_instruction),
            ..PromptOptions::default()
        };
        let response = self
            .router
            .send(selected_provider, full_prompt, options)
            .await
            .map_err(|e| format!("LLM error: {:?}", e))?;

        Ok(response.text)
    }
}

fn build_tool_context_message(context: &[crate::orchestrator::planner::StepResult]) -> String {
    let blocks: Vec<String> = context
        .iter()
        .filter_map(render_tool_result_block)
        .inspect(|block| {
            eprintln!(
                "[executor] injecting_tool_result size={} chars",
                block.chars().count()
            );
        })
        .collect();

    if blocks.is_empty() {
        String::new()
    } else {
        let result = format!(
            "You have access to the following real tool results. Treat them as ground truth.\n\n{}",
            blocks.join("\n\n")
        );
        eprintln!(
            "[executor] TOOL_CONTEXT_PREVIEW: {}",
            &result[..result.len().min(500)]
        );
        result
    }
}

fn render_tool_result_block(step: &crate::orchestrator::planner::StepResult) -> Option<String> {
    let tool_name = step.tool_name.as_deref()?;
    if tool_name == "decide_next" {
        return None;
    }

    let body = match serde_json::from_str::<crate::tools::ToolResult>(&step.output) {
        Ok(parsed_result) => render_tool_result_body(tool_name, &parsed_result),
        Err(_) => limit_tool_text(&step.output),
    };

    Some(format!("---\n[TOOL RESULT - {}]\n{}\n---", tool_name, body))
}

fn render_tool_result_body(tool_name: &str, result: &crate::tools::ToolResult) -> String {
    if !result.success {
        return format!(
            "Status: error\n\n{}",
            result
                .error
                .as_deref()
                .unwrap_or("Tool failed without an error message.")
        );
    }

    match tool_name {
        "open_file" => render_open_file_result(&result.output),
        _ => limit_tool_text(
            &serde_json::to_string_pretty(&result.output)
                .unwrap_or_else(|_| result.output.to_string()),
        ),
    }
}

fn render_open_file_result(output: &serde_json::Value) -> String {
    let path = output
        .get("path")
        .and_then(|value| value.as_str())
        .unwrap_or("<unknown>");
    let content = output
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("");

    if content.is_empty() {
        return format!(
            "Path: {}\n\n{}",
            path,
            limit_tool_text(
                &serde_json::to_string_pretty(output).unwrap_or_else(|_| output.to_string())
            )
        );
    }

    format!("Path: {}\n\n{}", path, limit_tool_text(content))
}

fn is_write_tool(tool: &str) -> bool {
    matches!(tool, "write_file" | "apply_patch")
}

fn tool_path(args: &serde_json::Value) -> Option<String> {
    args.get("path")
        .and_then(|v| v.as_str())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

fn tool_content(args: &serde_json::Value) -> Option<String> {
    args.get("content")
        .and_then(|v| v.as_str())
        .map(String::from)
}

fn write_tool_signature(tool: &str, args: &serde_json::Value) -> Option<String> {
    if !is_write_tool(tool) {
        return None;
    }

    tool_path(args).map(|path| format!("{}:{}", tool, path))
}

fn missing_required_assets(goal: &str, written_paths: &[String]) -> Vec<&'static str> {
    if !goal_requires_web_triplet(goal) {
        return Vec::new();
    }

    let has_html = written_paths.iter().any(|p| p.ends_with(".html"));
    let has_css = written_paths.iter().any(|p| p.ends_with(".css"));
    let has_js = written_paths.iter().any(|p| p.ends_with(".js"));

    let mut missing = Vec::new();
    if !has_html {
        missing.push("html");
    }
    if !has_css {
        missing.push("css");
    }
    if !has_js {
        missing.push("js");
    }
    missing
}

fn goal_requires_web_triplet(goal: &str) -> bool {
    let lower = goal.to_ascii_lowercase();
    lower.contains("html")
        && lower.contains("css")
        && (lower.contains("js") || lower.contains("javascript"))
}

fn tools_complete_summary(reason: &str, written_paths: &[String]) -> String {
    let files = if written_paths.is_empty() {
        "none".to_string()
    } else {
        written_paths.join(", ")
    };
    format!("[tools_complete]\nreason: {}\nfiles: {}", reason, files)
}

struct WebCompanionPaths {
    css: String,
    js: String,
}

fn infer_web_companion_paths(html_path: &str, html_content: &str) -> WebCompanionPaths {
    let css = crate::web_assets::extract_attr_value(html_content, "href")
        .into_iter()
        .find(|value| value.ends_with(".css"))
        .unwrap_or_else(|| "styles.css".to_string());
    let js = crate::web_assets::extract_attr_value(html_content, "src")
        .into_iter()
        .find(|value| value.ends_with(".js"))
        .unwrap_or_else(|| "scripts.js".to_string());

    WebCompanionPaths {
        css: crate::web_assets::resolve_html_asset_path(html_path, &css),
        js: crate::web_assets::resolve_html_asset_path(html_path, &js),
    }
}

fn default_css_asset(goal: &str) -> String {
    let specialty = landing_specialty(goal);
    format!(
        r#":root {{
    --ink: #17313b;
    --muted: #5d7280;
    --surface: #f4fbfa;
    --accent: #0f8b8d;
    --accent-strong: #0a5f64;
    --warm: #f7c873;
}}

* {{
    box-sizing: border-box;
}}

body {{
    margin: 0;
    font-family: Georgia, "Times New Roman", serif;
    color: var(--ink);
    background: linear-gradient(180deg, #ffffff 0%, var(--surface) 100%);
}}

header, main, footer {{
    width: min(1120px, calc(100% - 32px));
    margin: 0 auto;
}}

header {{
    padding: 28px 0;
}}

.hero, header {{
    min-height: 52vh;
    display: grid;
    align-content: center;
    gap: 18px;
}}

h1 {{
    max-width: 760px;
    margin: 0;
    font-size: clamp(2.2rem, 6vw, 5.5rem);
    line-height: 0.95;
}}

h2 {{
    font-size: 2rem;
    margin: 0 0 18px;
}}

p, li {{
    color: var(--muted);
    font-size: 1.05rem;
    line-height: 1.7;
}}

a, button {{
    background: var(--accent);
    color: white;
    border: 0;
    border-radius: 8px;
    padding: 12px 18px;
    text-decoration: none;
    cursor: pointer;
}}

a:hover, button:hover {{
    background: var(--accent-strong);
}}

section {{
    padding: 48px 0;
    border-top: 1px solid rgba(15, 139, 141, 0.18);
}}

.services, .cards {{
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(220px, 1fr));
    gap: 16px;
}}

.service-item, .card {{
    background: white;
    border: 1px solid rgba(23, 49, 59, 0.12);
    border-radius: 8px;
    padding: 20px;
    box-shadow: 0 12px 30px rgba(23, 49, 59, 0.08);
}}

form {{
    display: grid;
    gap: 12px;
    max-width: 560px;
}}

input, textarea {{
    width: 100%;
    border: 1px solid rgba(23, 49, 59, 0.2);
    border-radius: 8px;
    padding: 12px 14px;
    font: inherit;
}}

footer {{
    padding: 32px 0;
    color: var(--muted);
}}

.is-visible {{
    animation: rise 420ms ease both;
}}

@keyframes rise {{
    from {{ opacity: 0; transform: translateY(14px); }}
    to {{ opacity: 1; transform: translateY(0); }}
}}

/* Auto-generated companion stylesheet for a {specialty} landing page. */
"#
    )
}

fn default_js_asset(goal: &str) -> String {
    let specialty = landing_specialty(goal);
    format!(
        r#"const sections = document.querySelectorAll('section, .service-item, .card');

const reveal = new IntersectionObserver((entries) => {{
    entries.forEach((entry) => {{
        if (entry.isIntersecting) {{
            entry.target.classList.add('is-visible');
            reveal.unobserve(entry.target);
        }}
    }});
}}, {{ threshold: 0.16 }});

sections.forEach((section) => reveal.observe(section));

const form = document.querySelector('form');
if (form) {{
    form.addEventListener('submit', (event) => {{
        event.preventDefault();
        const name = form.querySelector('[name="name"], #name')?.value || 'paciente';
        alert(`Gracias, ${{name}}. Hemos recibido tu solicitud de {specialty}.`);
        form.reset();
    }});
}}
"#
    )
}

fn landing_specialty(goal: &str) -> &'static str {
    let lower = goal.to_ascii_lowercase();
    if lower.contains("medicina") {
        "medicina"
    } else if lower.contains("nutricion") || lower.contains("nutrición") {
        "nutricion"
    } else {
        "salud"
    }
}

async fn load_validation_commands_from_multilink(
    _tool_executor: &Arc<ToolExecutor>,
) -> Vec<String> {
    // Try loading MULTILINK.md validation commands via the command tool's built-in
    // parsing logic, but executed from the project root.
    let project_root = std::env::current_dir().unwrap_or_default();
    let multilink_path = project_root.join("MULTILINK.md");
    if !multilink_path.exists() {
        return Vec::new();
    }
    let content = std::fs::read_to_string(multilink_path).ok();
    let Some(content) = content else {
        return Vec::new();
    };

    // Re-use the same extraction logic from the command tool
    crate::tools::command::extract_json_block_commands(&content).unwrap_or_default()
}

fn limit_tool_text(text: &str) -> String {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= MAX_TOOL_RESULT_LINES {
        return text.to_string();
    }

    let truncated = lines
        .iter()
        .take(MAX_TOOL_RESULT_LINES)
        .copied()
        .collect::<Vec<_>>()
        .join("\n");

    format!(
        "{}\n\n[truncated to first {} lines out of {} total lines]",
        truncated,
        MAX_TOOL_RESULT_LINES,
        lines.len()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context_engine::ContextEngineV1;
    use crate::orchestrator::planner::MinimalPlanner;
    use crate::router::ProviderRouter;
    use crate::skills::SkillOrchestrator;
    use crate::tools::{ToolExecutor, ToolRegistry, ToolResult};
    use std::path::PathBuf;

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

    #[test]
    fn render_open_file_tool_result_includes_real_content_and_path() {
        let step = crate::orchestrator::planner::StepResult {
            step_index: 0,
            step_id: "step_0".to_string(),
            output: serde_json::to_string(&ToolResult::ok(serde_json::json!({
                "path": "core/src/router.rs",
                "content": "fn alpha() {}\nfn beta() {}",
                "size": 27
            })))
            .unwrap(),
            tool_name: Some("open_file".to_string()),
            metadata: None,
        };

        let rendered = render_tool_result_block(&step).expect("tool result should render");

        assert!(rendered.contains("[TOOL RESULT - open_file]"));
        assert!(rendered.contains("Path: core/src/router.rs"));
        assert!(rendered.contains("fn alpha() {}"));
        assert!(rendered.contains("fn beta() {}"));
    }

    #[test]
    fn limit_tool_text_keeps_partial_content_with_explicit_truncation_notice() {
        let long_text = (0..=MAX_TOOL_RESULT_LINES)
            .map(|idx| format!("line {}", idx))
            .collect::<Vec<_>>()
            .join("\n");

        let limited = limit_tool_text(&long_text);

        assert!(limited.contains("line 0"));
        assert!(limited.contains(&format!("line {}", MAX_TOOL_RESULT_LINES - 1)));
        assert!(!limited.contains(&format!("line {}", MAX_TOOL_RESULT_LINES)));
        assert!(limited.contains("truncated to first"));
    }

    #[test]
    fn write_tool_signature_tracks_duplicate_write_paths() {
        let args = serde_json::json!({
            "path": "index.html",
            "content": "<!doctype html>"
        });

        assert_eq!(
            write_tool_signature("write_file", &args),
            Some("write_file:index.html".to_string())
        );
        assert_eq!(write_tool_signature("fs_ls", &args), None);
    }

    #[test]
    fn missing_required_assets_detects_html_css_js_triplet() {
        let goal = "Crea una landing page usando html, css y js";
        let written = vec!["index.html".to_string()];

        assert_eq!(missing_required_assets(goal, &written), vec!["css", "js"]);

        let complete = vec![
            "index.html".to_string(),
            "styles.css".to_string(),
            "scripts.js".to_string(),
        ];
        assert!(missing_required_assets(goal, &complete).is_empty());
    }

    #[test]
    fn infer_web_companion_paths_uses_html_references() {
        let html = r#"
            <link rel="stylesheet" href="assets/site.css">
            <script src="js/app.js"></script>
        "#;

        let paths = infer_web_companion_paths("pages/index.html", html);

        assert_eq!(paths.css, "pages/assets/site.css");
        assert_eq!(paths.js, "pages/js/app.js");
    }
}

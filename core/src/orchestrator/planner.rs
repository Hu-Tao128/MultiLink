use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::orchestrator::provider_selector::RequiredCapabilities;
use crate::tools::ToolInput;

#[derive(Debug, Clone, Default)]
pub struct IntentFeatures {
    pub has_file_path: bool,
    pub file_path: Option<String>,
    pub asks_for_content: bool,
    pub asks_for_analysis: bool,
    pub asks_for_search: bool,
    pub asks_for_creation: bool,
    pub asks_for_modification: bool,
    pub is_read_operation: bool,
    pub is_write_operation: bool,
    pub wants_explanation: bool,
    pub wants_summary: bool,
    pub is_system_query: bool,
    pub is_general_conversation: bool,
}

impl IntentFeatures {
    #[allow(clippy::field_reassign_with_default)]
    pub fn extract(prompt: &str) -> Self {
        let p = prompt.to_lowercase();
        let mut features = IntentFeatures::default();

        features.has_file_path = has_file_path_pattern(prompt);
        features.file_path = extract_file_path(prompt);

        let content_keywords = [
            "abre",
            "open",
            "copia",
            "copy",
            "cat",
            "lee",
            "display",
            "view",
            "show file",
            "lee archivo",
            "read file",
            "muestrame",
            "dame",
            "contenido",
            "mostrar",
        ];
        features.asks_for_content = content_keywords.iter().any(|k| p.contains(k));

        let search_keywords = [
            "busca",
            "search",
            "grep",
            "find",
            "donde",
            "encontrar",
            "localizar",
            "search for",
        ];
        features.asks_for_search = search_keywords.iter().any(|k| p.contains(k));

        let analysis_keywords = [
            "analiza", "analyze", "explica", "explain", "por que", "entiende", "revisar", "review",
            "debug", "bug", "error", "problema", "issue", "fix", "corregir", "optimize", "audit",
            "evalua", "evaluate", "compara", "compare",
        ];
        features.asks_for_analysis = analysis_keywords.iter().any(|k| p.contains(k));

        let creation_keywords = [
            "crea", "create", "nuevo", "new", "escribe", "write", "genera", "generate", "add",
        ];
        features.asks_for_creation = creation_keywords.iter().any(|k| p.contains(k));

        let modification_keywords = [
            "modifica",
            "modify",
            "cambia",
            "change",
            "actualiza",
            "update",
            "edita",
            "edit",
        ];
        features.asks_for_modification = modification_keywords.iter().any(|k| p.contains(k));

        let explanation_keywords = [
            "explica",
            "explain",
            "que es",
            "what is",
            "como funciona",
            "how does",
            "por que",
            "why",
        ];
        features.wants_explanation = explanation_keywords.iter().any(|k| p.contains(k));

        let summary_keywords = ["resume", "summary", "resumen", "resume"];
        features.wants_summary = summary_keywords.iter().any(|k| p.contains(k));

        let system_keywords = [
            "version",
            "node",
            "java",
            "python",
            "cargo",
            "git",
            "system",
            " installed",
        ];
        features.is_system_query = system_keywords.iter().any(|k| p.contains(k));

        let read_ops = [
            "abre",
            "open",
            "lee",
            "read",
            "muestrame",
            "show",
            "dame",
            "ver",
        ];
        features.is_read_operation = read_ops.iter().any(|k| p.contains(k));

        let write_ops = [
            "crea",
            "create",
            "escribe",
            "write",
            "modifica",
            "modify",
            "actualiza",
            "update",
            "add",
            "append",
        ];
        features.is_write_operation = write_ops.iter().any(|k| p.contains(k));

        features.is_general_conversation = !features.has_file_path
            && !features.asks_for_content
            && !features.asks_for_search
            && !features.asks_for_analysis
            && !features.is_system_query;

        features
    }

    pub fn tool_confidence(&self) -> f32 {
        let mut score: f32 = 0.0;

        if self.has_file_path && self.is_read_operation && !self.wants_explanation {
            score += 0.95;
        }
        if self.has_file_path && self.asks_for_content && !self.asks_for_analysis {
            score += 0.90;
        }
        if self.asks_for_search && !self.asks_for_analysis {
            score += 0.85;
        }
        if self.is_system_query && !self.wants_explanation {
            score += 0.90;
        }
        if self.is_read_operation && self.asks_for_content {
            score += 0.80;
        }

        score.min(1.0)
    }

    pub fn llm_confidence(&self) -> f32 {
        let mut score: f32 = 0.0;

        if self.is_general_conversation {
            score += 0.95;
        }
        if self.wants_explanation {
            score += 0.90;
        }
        if self.asks_for_analysis {
            score += 0.85;
        }
        if self.wants_summary {
            score += 0.80;
        }
        if self.asks_for_creation {
            score += 0.75;
        }
        if self.asks_for_modification {
            score += 0.70;
        }
        if self.has_file_path && self.wants_explanation {
            score += 0.65;
        }

        score.min(1.0)
    }

    pub fn tool_vs_llm_score(&self) -> (f32, f32, ExecutionMode) {
        let tool_score = self.tool_confidence();
        let llm_score = self.llm_confidence();

        let mode = if tool_score > llm_score + 0.15 {
            ExecutionMode::ToolOnly
        } else if llm_score > tool_score + 0.10 {
            ExecutionMode::LLMOnly
        } else {
            ExecutionMode::ToolThenLLM
        };

        (tool_score, llm_score, mode)
    }

    pub fn requires_llm(&self) -> bool {
        self.asks_for_analysis
            || self.wants_explanation
            || self.wants_summary
            || self.is_general_conversation
            || self.asks_for_creation
            || self.asks_for_modification
    }

    pub fn requires_file_access(&self) -> bool {
        self.has_file_path || self.asks_for_search
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    ToolOnly,
    ToolThenLLM,
    LLMOnly,
}

impl ExecutionMode {
    pub fn classify(prompt: &str) -> Self {
        let features = IntentFeatures::extract(prompt);

        let (tool_score, llm_score, mode) = features.tool_vs_llm_score();

        eprintln!(
            "[planner] tool_vs_llm: tool={:.2} llm={:.2} mode={:?} goal={}",
            tool_score, llm_score, mode, prompt
        );

        mode
    }
}

pub fn is_raw_file_request(prompt: &str) -> bool {
    let lower = prompt.to_lowercase();
    lower.contains("copia")
        || lower.contains("copy")
        || lower.contains("completo")
        || lower.contains("full file")
        || lower.contains("todo el archivo")
        || lower.contains("entire file")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueryIntent {
    Search,
    ReadFile,
    WriteFile,
    ApplyPatch,
    RunCommand,
    GitStatus,
    GitDiff,
    SystemInfo,
    General,
}

pub fn classify_intent(prompt: &str) -> QueryIntent {
    let lower = prompt.to_lowercase();

    let read_keywords = [
        "abre",
        "open",
        "lee archivo",
        "read file",
        "show file",
        "show content",
        "cat",
        "display",
        "view",
    ];
    let search_keywords = ["busca", "find", "where is", "donde está", "donde esta"];
    let system_keywords = ["version", "node", "java", "python"];

    let intent = if lower.contains("git diff") || lower.contains("diff git") {
        QueryIntent::GitDiff
    } else if lower.contains("git status")
        || lower.contains("estado git")
        || lower.contains("estado de git")
    {
        QueryIntent::GitStatus
    } else if lower.contains("/write-file")
        || lower.contains("write_file")
        || (lower.contains("crea archivo") || lower.contains("create file")
            || lower.contains("genera archivo") || lower.contains("write file")
            || lower.contains("escribe archivo"))
            && has_file_path_pattern(prompt)
    {
        QueryIntent::WriteFile
    } else if lower.contains("apply_patch") || lower.contains("apply patch")
        || lower.contains("patch")
            && (lower.contains("modifica") || lower.contains("modify")
                || lower.contains("cambia") || lower.contains("change")
                || lower.contains("edit") || lower.contains("edita"))
    {
        QueryIntent::ApplyPatch
    } else if has_file_path_pattern(prompt) || read_keywords.iter().any(|k| lower.contains(k)) {
        QueryIntent::ReadFile
    } else if search_keywords.iter().any(|k| lower.contains(k)) {
        QueryIntent::Search
    } else if system_keywords.iter().any(|k| lower.contains(k)) {
        QueryIntent::SystemInfo
    } else if lower.contains("run_command") || lower.contains("run command")
        || lower.contains("validar") || lower.contains("validate")
        || lower.contains("ejecuta test") || lower.contains("run test")
        || lower.contains("cargo test") || lower.contains("npm test")
    {
        QueryIntent::RunCommand
    } else {
        QueryIntent::General
    };

    eprintln!("[planner] selected_intent={:?} prompt={}", intent, prompt);
    intent
}

fn select_tool_for_intent(intent: &QueryIntent, prompt: &str) -> Option<(String, ToolInput)> {
    let selection = match intent {
        QueryIntent::Search => Some((
            "search_code".to_string(),
            ToolInput {
                path: None,
                pattern: None,
                args: Some(HashMap::from([
                    (
                        "query".to_string(),
                        serde_json::Value::String(extract_search_query(prompt)),
                    ),
                    ("top_k".to_string(), serde_json::Value::Number(5.into())),
                ])),
            },
        )),
        QueryIntent::ReadFile => extract_file_path(prompt).map(|path| {
            (
                "open_file".to_string(),
                ToolInput {
                    path: Some(path),
                    pattern: None,
                    args: None,
                },
            )
        }),
        QueryIntent::WriteFile => extract_file_path(prompt).map(|path| {
            (
                "write_file".to_string(),
                ToolInput {
                    path: Some(path),
                    pattern: None,
                    args: None,
                },
            )
        }),
        QueryIntent::ApplyPatch => extract_file_path(prompt).map(|path| {
            (
                "apply_patch".to_string(),
                ToolInput {
                    path: Some(path),
                    pattern: None,
                    args: None,
                },
            )
        }),
        QueryIntent::RunCommand => Some((
            "run_command".to_string(),
            ToolInput {
                path: None,
                pattern: None,
                args: Some(HashMap::from([(
                    "command".to_string(),
                    serde_json::Value::String(extract_command_name(prompt)),
                )])),
            },
        )),
        QueryIntent::GitStatus => Some((
            "git_status".to_string(),
            ToolInput {
                path: None,
                pattern: None,
                args: None,
            },
        )),
        QueryIntent::GitDiff => Some((
            "git_diff".to_string(),
            ToolInput {
                path: extract_file_path(prompt),
                pattern: None,
                args: None,
            },
        )),
        QueryIntent::SystemInfo => Some((
            "system_version".to_string(),
            ToolInput {
                path: None,
                pattern: None,
                args: Some(HashMap::from([(
                    "tools".to_string(),
                    serde_json::Value::Array(
                        extract_requested_system_tools(prompt)
                            .into_iter()
                            .map(serde_json::Value::String)
                            .collect(),
                    ),
                )])),
            },
        )),
        QueryIntent::General => None,
    };

    if let Some((tool_name, _)) = selection.as_ref() {
        eprintln!(
            "[planner] selected_tool={} intent={:?} skipped=false",
            tool_name, intent
        );
    } else {
        eprintln!(
            "[planner] selected_tool=none intent={:?} skipped=true",
            intent
        );
    }

    selection
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Step {
    ToolCall { name: String, input: ToolInput },
    LLMCall { prompt: String },
    DecideNext,
}

pub const MAX_STEPS: usize = 5;
pub const DEFAULT_MAX_STEPS: usize = 5;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DecisionResult {
    pub next_action: String,
    pub tool: Option<String>,
    pub input: Option<ToolInput>,
    pub reason: Option<String>,
}

impl DecisionResult {
    pub fn done(reason: Option<String>) -> Self {
        Self {
            next_action: "done".to_string(),
            tool: None,
            input: None,
            reason,
        }
    }

    pub fn tool(name: String, input: ToolInput, reason: Option<String>) -> Self {
        Self {
            next_action: "tool".to_string(),
            tool: Some(name),
            input: Some(input),
            reason,
        }
    }

    pub fn llm(prompt: String, reason: Option<String>) -> Self {
        Self {
            next_action: "llm".to_string(),
            tool: None,
            input: Some(ToolInput {
                path: None,
                pattern: Some(prompt),
                args: None,
            }),
            reason,
        }
    }

    pub fn parse_from_json(json_str: &str) -> Option<Self> {
        serde_json::from_str(json_str).ok()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepMetadata {
    pub step_id: String,
    pub tool_name: Option<String>,
    pub input: Option<String>,
    pub timestamp: Option<String>,
}

impl StepMetadata {
    pub fn new(step_id: usize, tool_name: Option<&str>, input: Option<String>) -> Self {
        Self {
            step_id: format!("step_{}", step_id),
            tool_name: tool_name.map(String::from),
            input,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }

    pub fn for_llm(step_id: usize) -> Self {
        Self {
            step_id: format!("step_{}", step_id),
            tool_name: None,
            input: None,
            timestamp: Some(chrono::Utc::now().to_rfc3339()),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MultiStepPlan {
    pub steps: Vec<Step>,
    pub goal: String,
    pub context: Vec<StepResult>,
    pub raw_mode: bool,
}

impl MultiStepPlan {
    pub fn new(steps: Vec<Step>, goal: String, raw_mode: bool) -> Self {
        Self {
            steps,
            goal,
            context: Vec::new(),
            raw_mode,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StepResult {
    pub step_index: usize,
    pub step_id: String,
    pub output: String,
    pub tool_name: Option<String>,
    pub metadata: Option<StepMetadata>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Action {
    RetrieveContext,
    ExecuteSkill,
    GenerateResponse { capabilities: RequiredCapabilities },
    ToolCall { name: String, input: ToolInput },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PlanStep {
    pub action: Action,
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Plan {
    pub steps: Vec<PlanStep>,
    pub goal: String,
}

pub struct MinimalPlanner;

impl MinimalPlanner {
    pub fn plan(input: &str) -> Plan {
        let mut steps = Vec::new();
        let capabilities = infer_capabilities(input);
        let lower = input.to_lowercase();
        let intent = classify_intent(input);

        if matches!(
            intent,
            QueryIntent::Search
                | QueryIntent::ReadFile
                | QueryIntent::GitStatus
                | QueryIntent::GitDiff
                | QueryIntent::SystemInfo
        ) {
            if let Some((tool_name, tool_input)) = select_tool_for_intent(&intent, input) {
                let tool_desc = tool_name.clone();
                steps.push(PlanStep {
                    action: Action::ToolCall {
                        name: tool_name,
                        input: tool_input,
                    },
                    description: format!("Using tool: {}", tool_desc),
                });
            }
        } else {
            steps.push(PlanStep {
                action: Action::RetrieveContext,
                description: "Gathering relevant project context and files.".to_string(),
            });
        }

        if lower.contains("analyze")
            || lower.contains("review")
            || lower.contains("optimize")
            || lower.contains("audit")
        {
            steps.push(PlanStep {
                action: Action::ExecuteSkill,
                description: "Running specialized analysis skill.".to_string(),
            });
        }

        steps.push(PlanStep {
            action: Action::GenerateResponse {
                capabilities: capabilities.clone(),
            },
            description: "Synthesizing final response based on gathered info.".to_string(),
        });

        Plan {
            steps,
            goal: input.to_string(),
        }
    }

    pub fn plan_multi_step(input: &str) -> MultiStepPlan {
        let intent = classify_intent(input);
        let mode = ExecutionMode::classify(input);
        let raw_mode = matches!(mode, ExecutionMode::ToolOnly);
        let mut steps = Vec::new();

        eprintln!(
            "[planner] mode={:?} intent={:?} raw_mode={} goal={}",
            mode, intent, raw_mode, input
        );

        match mode {
            ExecutionMode::ToolOnly => {
                if let Some((tool_name, tool_input)) = select_tool_for_intent(&intent, input) {
                    steps.push(Step::ToolCall {
                        name: tool_name,
                        input: tool_input,
                    });
                }
            }
            ExecutionMode::ToolThenLLM => {
                if let Some((tool_name, tool_input)) = select_tool_for_intent(&intent, input) {
                    steps.push(Step::ToolCall {
                        name: tool_name,
                        input: tool_input,
                    });
                    steps.push(Step::LLMCall {
                        prompt: format!(
                            "Usa exclusivamente el resultado real de la tool como fuente de verdad. \
Analiza y responde la solicitud del usuario.\n\nSolicitud original: {}",
                            input
                        ),
                    });
                } else {
                    steps.push(Step::LLMCall {
                        prompt: input.to_string(),
                    });
                }
            }
            ExecutionMode::LLMOnly => {
                steps.push(Step::LLMCall {
                    prompt: input.to_string(),
                });
            }
        }

        if steps.is_empty() {
            steps.push(Step::LLMCall {
                prompt: input.to_string(),
            });
        }

        MultiStepPlan::new(steps, input.to_string(), raw_mode)
    }
}

fn has_file_path_pattern(prompt: &str) -> bool {
    prompt
        .split_whitespace()
        .any(|token| extract_path_candidate(token).is_some())
}

fn extract_file_path(prompt: &str) -> Option<String> {
    prompt.split_whitespace().find_map(extract_path_candidate)
}

fn extract_path_candidate(token: &str) -> Option<String> {
    let cleaned = token
        .trim_matches(|c: char| {
            matches!(
                c,
                '"' | '\'' | '`' | ',' | ':' | ';' | '(' | ')' | '[' | ']' | '{' | '}'
            )
        })
        .trim();

    let has_known_extension = [
        ".rs", ".ts", ".tsx", ".js", ".jsx", ".md", ".toml", ".json", ".yaml", ".yml", ".cpp",
        ".h", ".hpp", ".py", ".java", ".qml",
    ]
    .iter()
    .any(|ext| cleaned.ends_with(ext));

    if cleaned.is_empty()
        || !cleaned.contains('.')
        || !(cleaned.contains('/') || has_known_extension)
        || cleaned.starts_with("http://")
        || cleaned.starts_with("https://")
    {
        return None;
    }

    Some(cleaned.to_string())
}

fn extract_search_query(prompt: &str) -> String {
    let lower = prompt.to_lowercase();
    let phrases = ["busca", "find", "where is", "donde está", "donde esta"];

    for phrase in phrases {
        if let Some(index) = lower.find(phrase) {
            let query = prompt[index + phrase.len()..].trim();
            if !query.is_empty() {
                return query.to_string();
            }
        }
    }

    prompt.trim().to_string()
}

fn extract_requested_system_tools(prompt: &str) -> Vec<String> {
    let lower = prompt.to_lowercase();
    let mut tools = Vec::new();

    for tool in ["node", "java", "python"] {
        if lower.contains(tool) {
            tools.push(tool.to_string());
        }
    }

    if lower.contains("version") && tools.is_empty() {
        tools = vec!["node".to_string(), "java".to_string(), "python".to_string()];
    }

    if tools.is_empty() {
        tools = vec!["node".to_string(), "java".to_string(), "python".to_string()];
    }

    tools
}

fn extract_command_name(prompt: &str) -> String {
    let lower = prompt.to_lowercase();
    let known = [
        "cargo test",
        "cargo build",
        "cargo clippy -- -D warnings",
        "cargo fmt --all",
        "cargo check",
        "npm test",
        "npm run build",
        "npm run lint",
        "npm install",
        "pytest",
        "make",
        "flutter test",
        "flutter analyze",
        "go test",
        "go build",
        "python -m pytest",
    ];
    for cmd in &known {
        if lower.contains(cmd) {
            return cmd.to_string();
        }
    }
    "cargo test".to_string()
}

pub fn infer_capabilities(prompt: &str) -> RequiredCapabilities {
    let lower = prompt.to_lowercase();

    let mut caps = RequiredCapabilities::new();

    let reasoning_keywords = [
        "think",
        "reason",
        "analyze",
        "explain why",
        "reasoning",
        "step by step",
        "consider",
        "evaluate",
        "compare",
        "assess",
        "determine",
        "conclude",
        "implications",
        "trade-off",
    ];
    if reasoning_keywords.iter().any(|kw| lower.contains(kw)) {
        caps.reasoning = true;
    }

    let coding_keywords = [
        "write code",
        "implement",
        "function",
        "refactor",
        "debug",
        "fix bug",
        "test",
        "code",
        "class",
        "algorithm",
        "api endpoint",
        "rust",
        "python",
        "javascript",
        "typescript",
        "sql",
        "query",
        "parse",
        "serialize",
        "async",
        "concurrent",
    ];
    if coding_keywords.iter().any(|kw| lower.contains(kw)) {
        caps.coding = true;
    }

    let vision_keywords = [
        "image",
        "screenshot",
        "photo",
        "visual",
        "picture",
        "diagram",
        "chart",
        "graph",
        "what do you see",
        "describe this",
        "look at",
        "OCR",
        "recognize",
    ];
    if vision_keywords.iter().any(|kw| lower.contains(kw)) {
        caps.vision = true;
    }

    let embedding_keywords = [
        "similar",
        "semantic search",
        "embed",
        "find related",
        "relevant context",
        "RAG",
        "knowledge base",
    ];
    if embedding_keywords.iter().any(|kw| lower.contains(kw)) {
        caps.embeddings = true;
    }

    let latency_keywords = [
        "quick",
        "fast",
        "immediate",
        "real-time",
        "low latency",
        "streaming",
        "live",
        "interactive",
    ];
    if latency_keywords.iter().any(|kw| lower.contains(kw)) {
        caps.low_latency = true;
    }

    let long_context_keywords = [
        "large codebase",
        "entire file",
        "whole project",
        "full context",
        "this repository",
        "monolith",
        "big refactor",
    ];
    if long_context_keywords.iter().any(|kw| lower.contains(kw)) {
        caps.long_context = true;
    }

    caps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_plan() {
        let plan = MinimalPlanner::plan("hello there");
        assert_eq!(plan.steps.len(), 2);
        assert!(matches!(plan.steps[0].action, Action::RetrieveContext));
        assert!(matches!(
            plan.steps[1].action,
            Action::GenerateResponse { .. }
        ));
    }

    #[test]
    fn test_analysis_plan() {
        let plan = MinimalPlanner::plan("analyze my project");
        assert_eq!(plan.steps.len(), 3);
        assert!(matches!(plan.steps[1].action, Action::ExecuteSkill));
    }

    #[test]
    fn classify_read_file_intent_from_spanish_and_path() {
        assert_eq!(
            classify_intent("Abre core/src/router.rs"),
            QueryIntent::ReadFile
        );
        assert_eq!(
            classify_intent("show file docs/README.md"),
            QueryIntent::ReadFile
        );
    }

    #[test]
    fn classify_search_and_system_intents() {
        assert_eq!(
            classify_intent("busca donde está ProviderRouter"),
            QueryIntent::Search
        );
        assert_eq!(classify_intent("python version"), QueryIntent::SystemInfo);
        assert_eq!(classify_intent("git status"), QueryIntent::GitStatus);
        assert_eq!(
            classify_intent("git diff core/src/lib.rs"),
            QueryIntent::GitDiff
        );
    }

    #[test]
    fn multi_step_plan_uses_tool_first_for_file_queries() {
        let plan = MinimalPlanner::plan_multi_step("Abre core/src/router.rs y copia el código");
        assert_eq!(plan.steps.len(), 1);
        assert!(plan.raw_mode, "raw_mode should be true for copy requests");
        match &plan.steps[0] {
            Step::ToolCall { name, input } => {
                assert_eq!(name, "open_file");
                assert_eq!(input.path.as_deref(), Some("core/src/router.rs"));
            }
            other => panic!("expected tool call, got {:?}", other),
        }
    }

    #[test]
    fn multi_step_plan_uses_tool_first_for_search_queries() {
        let plan = MinimalPlanner::plan_multi_step("busca donde está ProviderRouter");
        assert_eq!(plan.steps.len(), 1, "search is now ToolOnly, no LLM call");
        assert!(plan.raw_mode, "raw_mode should be true for search");
        match &plan.steps[0] {
            Step::ToolCall { name, .. } => assert_eq!(name, "search_code"),
            other => panic!("expected tool call, got {:?}", other),
        }
    }

    #[test]
    fn multi_step_plan_uses_git_tools_for_git_queries() {
        let status_plan = MinimalPlanner::plan_multi_step("git status");
        match &status_plan.steps[0] {
            Step::ToolCall { name, .. } => assert_eq!(name, "git_status"),
            other => panic!("expected git_status tool call, got {:?}", other),
        }

        let diff_plan = MinimalPlanner::plan_multi_step("git diff core/src/lib.rs");
        match &diff_plan.steps[0] {
            Step::ToolCall { name, input } => {
                assert_eq!(name, "git_diff");
                assert_eq!(input.path.as_deref(), Some("core/src/lib.rs"));
            }
            other => panic!("expected git_diff tool call, got {:?}", other),
        }
    }

    #[test]
    fn test_infer_reasoning() {
        let caps = infer_capabilities("think about this problem step by step");
        assert!(caps.reasoning);
    }

    #[test]
    fn test_infer_coding() {
        let caps = infer_capabilities("write a function to parse JSON");
        assert!(caps.coding);
    }

    #[test]
    fn test_infer_vision() {
        let caps = infer_capabilities("describe what you see in this screenshot");
        assert!(caps.vision);
    }

    #[test]
    fn test_infer_embeddings() {
        let caps = infer_capabilities("find similar files to this one");
        assert!(caps.embeddings);
    }

    #[test]
    fn test_infer_low_latency() {
        let caps = infer_capabilities("give me a quick answer");
        assert!(caps.low_latency);
    }

    #[test]
    fn test_infer_long_context() {
        let caps = infer_capabilities("analyze the entire large codebase");
        assert!(caps.long_context);
    }

    #[test]
    fn test_infer_complex_prompt() {
        let caps =
            infer_capabilities("analyze this large codebase and write a function to fix the bug");
        assert!(caps.reasoning);
        assert!(caps.coding);
        assert!(caps.long_context);
    }
}

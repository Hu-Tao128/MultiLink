use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::orchestrator::provider_selector::RequiredCapabilities;
use crate::tools::ToolInput;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum QueryIntent {
    Search,
    ReadFile,
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

    let intent = if has_file_path_pattern(prompt) || read_keywords.iter().any(|k| lower.contains(k))
    {
        QueryIntent::ReadFile
    } else if search_keywords.iter().any(|k| lower.contains(k)) {
        QueryIntent::Search
    } else if system_keywords.iter().any(|k| lower.contains(k)) {
        QueryIntent::SystemInfo
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
            QueryIntent::Search | QueryIntent::ReadFile | QueryIntent::SystemInfo
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
        let mut steps = Vec::new();
        let context = Vec::new();

        match intent {
            QueryIntent::ReadFile => {
                if let Some((tool_name, tool_input)) = select_tool_for_intent(&intent, input) {
                    steps.push(Step::ToolCall {
                        name: tool_name,
                        input: tool_input,
                    });
                    steps.push(Step::LLMCall {
                        prompt: format!(
                            "Usa exclusivamente el resultado real de open_file como fuente de verdad. \
Si el usuario pidió copiar el código, reproduce el contenido real del archivo. \
Luego explica brevemente si hace falta.\n\nSolicitud original: {}",
                            input
                        ),
                    });
                }
            }
            QueryIntent::Search => {
                if let Some((tool_name, tool_input)) = select_tool_for_intent(&intent, input) {
                    steps.push(Step::ToolCall {
                        name: tool_name,
                        input: tool_input,
                    });
                    steps.push(Step::LLMCall {
                        prompt: format!(
                            "Resume los resultados reales de search_code y responde la solicitud del usuario sin inventar coincidencias.\n\nSolicitud original: {}",
                            input
                        ),
                    });
                }
            }
            QueryIntent::SystemInfo => {
                if let Some((tool_name, tool_input)) = select_tool_for_intent(&intent, input) {
                    steps.push(Step::ToolCall {
                        name: tool_name,
                        input: tool_input,
                    });
                    steps.push(Step::LLMCall {
                        prompt: format!(
                            "Resume la informacion real devuelta por system_version y contesta la solicitud del usuario.\n\nSolicitud original: {}",
                            input
                        ),
                    });
                }
            }
            QueryIntent::General => {
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

        MultiStepPlan {
            steps,
            goal: input.to_string(),
            context,
        }
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
    }

    #[test]
    fn multi_step_plan_uses_tool_first_for_file_queries() {
        let plan = MinimalPlanner::plan_multi_step("Abre core/src/router.rs y copia el código");
        assert_eq!(plan.steps.len(), 2);
        match &plan.steps[0] {
            Step::ToolCall { name, input } => {
                assert_eq!(name, "open_file");
                assert_eq!(input.path.as_deref(), Some("core/src/router.rs"));
            }
            other => panic!("expected tool call, got {:?}", other),
        }
        match &plan.steps[1] {
            Step::LLMCall { prompt } => {
                assert!(prompt.contains("open_file"));
            }
            other => panic!("expected llm call, got {:?}", other),
        }
    }

    #[test]
    fn multi_step_plan_uses_tool_first_for_search_queries() {
        let plan = MinimalPlanner::plan_multi_step("busca donde está ProviderRouter");
        assert_eq!(plan.steps.len(), 2);
        match &plan.steps[0] {
            Step::ToolCall { name, .. } => assert_eq!(name, "search_code"),
            other => panic!("expected tool call, got {:?}", other),
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

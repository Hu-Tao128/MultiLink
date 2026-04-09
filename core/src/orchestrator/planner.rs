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

    let search_keywords = [
        "find",
        "search",
        "locate",
        "where is",
        "look for",
        "grep",
        "search for",
        "look up",
        "get info",
        "show me",
        "list",
    ];

    let read_keywords = ["open", "read", "show content", "cat", "display", "view"];

    let system_keywords = ["system", "version", "info", "config", "settings", "status"];

    if search_keywords.iter().any(|k| lower.contains(k)) {
        QueryIntent::Search
    } else if read_keywords.iter().any(|k| lower.contains(k)) {
        QueryIntent::ReadFile
    } else if system_keywords.iter().any(|k| lower.contains(k)) {
        QueryIntent::SystemInfo
    } else {
        QueryIntent::General
    }
}

fn select_tool_for_intent(intent: &QueryIntent, prompt: &str) -> Option<(String, ToolInput)> {
    let lower = prompt.to_lowercase();

    match intent {
        QueryIntent::Search => {
            let query = prompt
                .split(|c| c == ':' || c == '?' || c == ' ')
                .skip_while(|s| s.len() < 4 || !s.chars().any(|c| c.is_alphabetic()))
                .collect::<Vec<_>>()
                .join(" ");

            if lower.contains("find") && lower.contains("bug")
                || lower.contains("analyze")
                || lower.contains("review")
            {
                Some((
                    "search_and_open".to_string(),
                    ToolInput {
                        path: None,
                        pattern: None,
                        args: Some(HashMap::from([
                            (
                                "query".to_string(),
                                serde_json::Value::String(query.clone()),
                            ),
                            ("top_k".to_string(), serde_json::Value::Number(3.into())),
                        ])),
                    },
                ))
            } else {
                Some((
                    "search_code".to_string(),
                    ToolInput {
                        path: None,
                        pattern: None,
                        args: Some(HashMap::from([
                            ("query".to_string(), serde_json::Value::String(query)),
                            ("top_k".to_string(), serde_json::Value::Number(5.into())),
                        ])),
                    },
                ))
            }
        }
        QueryIntent::ReadFile => {
            let path = lower
                .split_whitespace()
                .find(|w| w.contains('.') && !w.contains("file"))
                .map(|s| s.to_string())
                .unwrap_or_else(|| ".".to_string());

            Some((
                "open_file".to_string(),
                ToolInput {
                    path: Some(path),
                    pattern: None,
                    args: None,
                },
            ))
        }
        QueryIntent::SystemInfo => Some((
            "system_version".to_string(),
            ToolInput {
                path: None,
                pattern: None,
                args: None,
            },
        )),
        QueryIntent::General => None,
    }
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

        if matches!(intent, QueryIntent::Search) || matches!(intent, QueryIntent::ReadFile) {
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

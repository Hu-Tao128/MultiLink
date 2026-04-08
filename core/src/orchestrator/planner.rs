use serde::{Deserialize, Serialize};

use crate::orchestrator::provider_selector::RequiredCapabilities;
use crate::tools::ToolInput;

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

        steps.push(PlanStep {
            action: Action::RetrieveContext,
            description: "Gathering relevant project context and files.".to_string(),
        });

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

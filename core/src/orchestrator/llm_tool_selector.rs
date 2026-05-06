use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::orchestrator::model_strategy::ModelSize;
use crate::orchestrator::planner::StepResult;
use crate::providers::{PromptOptions, ProviderId};
use crate::router::ProviderRouter;
use crate::tools::description::ToolDescription;

const TARGET_PROMPT_TOKENS: usize = 2048;
const ESTIMATED_TOKENS_PER_CHAR: f64 = 0.25;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolSelectionResponse {
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub args: Option<Value>,
    #[serde(default)]
    pub reasoning: Option<String>,
}

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub tool: String,
    pub args: Value,
    pub reasoning: String,
}

pub struct LlmToolSelector {
    router: Arc<ProviderRouter>,
    tool_descriptions: Vec<ToolDescription>,
    model_size: ModelSize,
}

impl LlmToolSelector {
    pub fn new(
        router: Arc<ProviderRouter>,
        tool_descriptions: Vec<ToolDescription>,
        model_size: ModelSize,
    ) -> Self {
        Self {
            router,
            tool_descriptions,
            model_size,
        }
    }

    pub async fn select_next_tool(
        &self,
        goal: &str,
        context: &[StepResult],
        available_tools: &[String],
    ) -> Result<ToolCall, String> {
        let prompt = self.build_tool_selection_prompt(goal, context, available_tools);
        let system = self.system_prompt();

        let provider = self.select_provider().await?;

        let decision = self
            .send_and_parse(&provider, &prompt, &system, 0.1)
            .await;

        match decision {
            Ok(call) => Ok(call),
            Err(_) => {
                eprintln!(
                    "[llm_tool_selector] first parse failed, retrying with lower temperature"
                );
                let decision = self
                    .send_and_parse(&provider, &prompt, &system, 0.01)
                    .await;
                match decision {
                    Ok(call) => Ok(call),
                    Err(_e) => {
                        eprintln!(
                            "[llm_tool_selector] retry also failed, falling back to heuristic"
                        );
                        self.heuristic_fallback(goal)
                    }
                }
            }
        }
    }

    async fn send_and_parse(
        &self,
        provider: &ProviderId,
        prompt: &str,
        system: &str,
        temperature: f32,
    ) -> Result<ToolCall, String> {
        let options = PromptOptions {
            system_prompt: Some(system.to_string()),
            temperature: Some(temperature),
            ..PromptOptions::default()
        };

        let response = self
            .router
            .send(*provider, prompt.to_string(), options)
            .await
            .map_err(|e| format!("LLM error: {:?}", e))?;

        self.parse_response(&response.text)
    }

    fn parse_response(&self, text: &str) -> Result<ToolCall, String> {
        let text = text.trim();

        let cleaned = text
            .trim_start_matches("```json")
            .trim_start_matches("```")
            .trim_end_matches("```")
            .trim();

        if let Ok(decision) = serde_json::from_str::<ToolSelectionResponse>(cleaned) {
            return self.decision_to_call(decision);
        }

        if let Ok(decision) = serde_json::from_str::<ToolSelectionResponse>(text) {
            return self.decision_to_call(decision);
        }

        if let Some(extracted) = extract_json_object(text) {
            if let Ok(decision) = serde_json::from_str::<ToolSelectionResponse>(&extracted) {
                return self.decision_to_call(decision);
            }
        }

        Err(format!("Could not parse tool selection from: {}", truncate(text, 300)))
    }

    fn decision_to_call(&self, decision: ToolSelectionResponse) -> Result<ToolCall, String> {
        match decision.tool {
            Some(tool_name) if !tool_name.is_empty() => {
                let args = decision.args.unwrap_or_default();
                let reasoning = decision.reasoning.unwrap_or_default();
                Ok(ToolCall {
                    tool: tool_name,
                    args,
                    reasoning,
                })
            }
            _ => {
                let reasoning = decision
                    .reasoning
                    .unwrap_or_else(|| "Goal achieved or blocked".to_string());
                Ok(ToolCall {
                    tool: String::new(),
                    args: Value::Null,
                    reasoning,
                })
            }
        }
    }

    fn heuristic_fallback(&self, goal: &str) -> Result<ToolCall, String> {
        let lower = goal.to_lowercase();
        if lower.contains("search")
            || lower.contains("find")
            || lower.contains("busca")
            || lower.contains("where")
            || lower.contains("donde")
        {
            Ok(ToolCall {
                tool: "search_code".to_string(),
                args: serde_json::json!({"query": goal}),
                reasoning: "heuristic fallback: search intent detected".to_string(),
            })
        } else if lower.contains("read")
            || lower.contains("open")
            || lower.contains("abre")
            || lower.contains("show")
            || lower.contains("cat")
            || lower.contains("lee")
        {
            Ok(ToolCall {
                tool: "open_file".to_string(),
                args: serde_json::json!({"path": "."}),
                reasoning: "heuristic fallback: read intent detected".to_string(),
            })
        } else {
            Ok(ToolCall {
                tool: String::new(),
                args: Value::Null,
                reasoning: "heuristic fallback: no clear intent, marking done".to_string(),
            })
        }
    }

    fn build_tool_selection_prompt(
        &self,
        goal: &str,
        context: &[StepResult],
        available_tools: &[String],
    ) -> String {
        let filtered_descriptions: Vec<&ToolDescription> = self
            .tool_descriptions
            .iter()
            .filter(|t| available_tools.contains(&t.name))
            .collect();

        let tools_desc: String = filtered_descriptions
            .iter()
            .map(|t| self.model_size.build_tool_prompt(t))
            .collect::<Vec<_>>()
            .join("\n\n");

        let estimated_tokens = (tools_desc.len() as f64 * ESTIMATED_TOKENS_PER_CHAR) as usize;
        let tools_desc = if estimated_tokens > TARGET_PROMPT_TOKENS {
            self.truncate_tool_descriptions(&filtered_descriptions)
        } else {
            tools_desc
        };

        let context_str = if context.is_empty() {
            "No previous steps executed yet.".to_string()
        } else {
            let max_context_chars = (500.0 / ESTIMATED_TOKENS_PER_CHAR) as usize;
            let mut parts: Vec<String> = context
                .iter()
                .map(|r| {
                    format!(
                        "- {} -> {}: {}",
                        r.step_id,
                        r.tool_name.as_deref().unwrap_or("llm"),
                        truncate(&r.output, 200)
                    )
                })
                .collect();

            let mut total = parts.join("\n");
            if total.len() > max_context_chars {
                total = truncate(&total, max_context_chars);
                parts = vec![total];
            }
            parts.join("\n")
        };

        format!(
            "GOAL: {}\n\n\
CURRENT STATE:\n{}\n\n\
AVAILABLE TOOLS:\n{}\n\n\
INSTRUCTIONS:\n\
1. Analyze the goal and current state\n\
2. Choose the NEXT tool to call (only ONE)\n\
3. Provide valid arguments matching the tool's schema\n\
4. Explain your reasoning briefly\n\n\
RESPOND IN JSON FORMAT:\n\
{{\n  \"tool\": \"tool_name\",\n  \"args\": {{ ... }},\n  \"reasoning\": \"why this tool next\"\n}}\n\n\
IF NO MORE TOOLS NEEDED (goal achieved or blocked):\n\
{{\n  \"tool\": null,\n  \"args\": null,\n  \"reasoning\": \"goal achieved\" or \"blocked because...\"\n}}",
            goal, context_str, tools_desc
        )
    }

    fn truncate_tool_descriptions(
        &self,
        descriptions: &[&ToolDescription],
    ) -> String {
        let max_per_tool_chars =
            (TARGET_PROMPT_TOKENS as f64 / ESTIMATED_TOKENS_PER_CHAR / descriptions.len().max(1) as f64)
                as usize;
        descriptions
            .iter()
            .map(|t| {
                let prompt = self.model_size.build_tool_prompt(t);
                if prompt.len() > max_per_tool_chars {
                    let truncated = truncate(&prompt, max_per_tool_chars);
                    format!("Tool: {}\n{}", t.name, truncated)
                } else {
                    prompt
                }
            })
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    fn system_prompt(&self) -> String {
        r"You are a coding assistant that selects tools to accomplish tasks.

RULES:
- Always use search_code before write_file to understand context
- Never call the same tool twice with identical arguments
- Prefer read operations before write operations
- If a tool fails, try a different approach or ask for clarification
- Use run_command after write_file to validate changes (cargo check, npm test, etc.)

THINKING PROCESS:
1. What is the current state?
2. What tools are available?
3. Which tool gets us closest to the goal?
4. What are the arguments?

Respond ONLY with valid JSON."
            .to_string()
    }

    async fn select_provider(&self) -> Result<ProviderId, String> {
        let available = self.router.get_available_providers().await;
        if available.is_empty() {
            return Err("No providers available for tool selection".to_string());
        }
        Ok(available[0].id)
    }
}

fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] == b'{' {
            let start = i;
            let mut depth = 1u32;
            i += 1;
            while i < len && depth > 0 {
                if bytes[i] == b'{' {
                    depth += 1;
                } else if bytes[i] == b'}' {
                    depth -= 1;
                }
                i += 1;
            }
            if depth == 0 {
                let candidate = &text[start..i];
                if candidate.len() > 2
                    && serde_json::from_str::<serde_json::Value>(candidate).is_ok()
                {
                    return Some(candidate.to_string());
                }
            }
        } else {
            i += 1;
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max])
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_json_object_simple() {
        let text = "Here is the result: {\"tool\": \"search_code\", \"args\": {\"query\": \"test\"}} and more text";
        let extracted = extract_json_object(text);
        assert!(extracted.is_some());
        let parsed: ToolSelectionResponse =
            serde_json::from_str(&extracted.unwrap()).expect("should parse");
        assert_eq!(parsed.tool, Some("search_code".to_string()));
    }

    #[test]
    fn test_parse_response_with_code_fences() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
        };
        let text = "```json\n{\"tool\": \"open_file\", \"args\": {\"path\": \"main.rs\"}, \"reasoning\": \"test\"}\n```";
        let result = selector.parse_response(text);
        assert!(result.is_ok());
        let call = result.unwrap();
        assert_eq!(call.tool, "open_file");
    }

    #[test]
    fn test_parse_response_plain_json() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
        };
        let text = r#"{"tool": "search_code", "args": {"query": "test"}, "reasoning": "need to search"}"#;
        let result = selector.parse_response(text);
        assert!(result.is_ok());
        let call = result.unwrap();
        assert_eq!(call.tool, "search_code");
    }

    #[test]
    fn test_parse_response_embedded_json() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
        };
        let text = "I think we should search first: {\"tool\": \"search_code\", \"args\": {\"query\": \"test\"}, \"reasoning\": \"need context\"}";
        let result = selector.parse_response(text);
        assert!(result.is_ok());
    }

    #[test]
    fn test_none_tool_means_done() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
        };
        let text = r#"{"tool": null, "args": null, "reasoning": "goal achieved"}"#;
        let result = selector.parse_response(text);
        assert!(result.is_ok());
        assert!(result.unwrap().tool.is_empty());
    }

    #[test]
    fn test_heuristic_fallback_search() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
        };
        let result = selector.heuristic_fallback("find the router implementation").unwrap();
        assert_eq!(result.tool, "search_code");
    }

    #[test]
    fn test_heuristic_fallback_read() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
        };
        let result = selector.heuristic_fallback("open main.rs").unwrap();
        assert_eq!(result.tool, "open_file");
    }

    #[test]
    fn test_extract_json_with_nested_braces() {
        let text = r#"Some text {"tool": "write_file", "args": {"content": "fn main() {}"}} trailing"#;
        let result = extract_json_object(text);
        assert!(result.is_some());
        let parsed: ToolSelectionResponse =
            serde_json::from_str(&result.unwrap()).expect("should parse nested json");
        assert_eq!(parsed.tool, Some("write_file".to_string()));
    }
}

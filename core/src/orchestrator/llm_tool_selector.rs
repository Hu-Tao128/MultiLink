use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::orchestrator::executor::{mcp_prompt_terms, mcp_tool_score, prompt_mentions_mcp};
use crate::orchestrator::model_strategy::ModelSize;
use crate::orchestrator::planner::StepResult;
use crate::providers::{PromptOptions, ProviderId};
use crate::router::ProviderRouter;
use crate::tools::description::ToolDescription;

const SMALL_TARGET_PROMPT_TOKENS: usize = 700;
const MEDIUM_TARGET_PROMPT_TOKENS: usize = 1400;
const LARGE_TARGET_PROMPT_TOKENS: usize = 3072;
const SMALL_CONTEXT_TOKENS: usize = 300;
const MEDIUM_CONTEXT_TOKENS: usize = 600;
const LARGE_CONTEXT_TOKENS: usize = 1500;
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
    current_model: Option<String>,
    num_ctx: Option<usize>,
}

impl LlmToolSelector {
    pub fn new(
        router: Arc<ProviderRouter>,
        tool_descriptions: Vec<ToolDescription>,
        model_size: ModelSize,
        current_model: Option<String>,
        num_ctx: Option<usize>,
    ) -> Self {
        Self {
            router,
            tool_descriptions,
            model_size,
            current_model,
            num_ctx,
        }
    }

    /// Sets the Ollama context window used for tool-selection calls. Keeping it
    /// explicit prevents the server from silently truncating to its default
    /// (2048/4096) while the prompt budget assumes a larger window.
    pub fn with_num_ctx(mut self, num_ctx: Option<usize>) -> Self {
        self.num_ctx = num_ctx;
        self
    }

    pub async fn select_next_tool(
        &self,
        goal: &str,
        context: &[StepResult],
        available_tools: &[String],
    ) -> Result<ToolCall, String> {
        let prompt = self.build_tool_selection_prompt(goal, context, available_tools);
        let system = self.system_prompt();

        eprintln!("[llm_tool_selector] selecting_provider...");
        let provider = self.select_provider(goal).await?;

        eprintln!("[llm_tool_selector] provider_selected={:?}", provider);
        eprintln!("[llm_tool_selector] sending_prompt chars={}", prompt.len());

        let mut attempt_prompt = prompt.clone();
        let mut last_error = String::new();

        for (attempt, temperature) in [(0u8, 0.1f32), (1, 0.01), (2, 0.0)] {
            let decision = self
                .send_and_parse(&provider, &attempt_prompt, &system, goal, temperature)
                .await
                .and_then(|call| self.accept_tool_call(call, available_tools));

            match decision {
                Ok(call) => return Ok(call),
                Err(err) => {
                    eprintln!(
                        "[llm_tool_selector] attempt {} failed: {}",
                        attempt + 1,
                        err
                    );
                    last_error = err.clone();
                    attempt_prompt = format!(
                        "{prompt}\n\nYOUR PREVIOUS RESPONSE WAS INVALID: {err}\n\
                         Return ONLY the tool call. For write_file use a \n\
                         [[WRITE_FILE path=\"relative/path.ext\"]] block, never JSON-escaped content."
                    );
                }
            }
        }

        eprintln!(
            "[llm_tool_selector] all attempts failed (last error: {}); falling back to heuristic",
            last_error
        );
        self.heuristic_fallback(goal, available_tools)
    }

    fn accept_tool_call(
        &self,
        call: ToolCall,
        available_tools: &[String],
    ) -> Result<ToolCall, String> {
        let call = self.resolve_tool_name(call, available_tools);
        if call.tool.is_empty()
            || available_tools.is_empty()
            || available_tools.iter().any(|tool| tool == &call.tool)
        {
            return Ok(call);
        }

        Err(format!(
            "Tool '{}' is not in the available tool list",
            call.tool
        ))
    }

    fn resolve_tool_name(&self, mut call: ToolCall, available_tools: &[String]) -> ToolCall {
        if call.tool.is_empty() {
            return call;
        }

        let normalized = call.tool.trim().to_ascii_lowercase().replace(' ', "_");
        if let Some(exact) = available_tools
            .iter()
            .find(|tool| tool.eq_ignore_ascii_case(&normalized))
        {
            call.tool = exact.clone();
            return call;
        }

        let suffix = format!("__{}", normalized);
        if let Some(matched) = available_tools
            .iter()
            .find(|tool| tool.to_ascii_lowercase().ends_with(&suffix))
        {
            call.tool = matched.clone();
            return call;
        }

        call.tool = normalized;
        call
    }

    async fn send_and_parse(
        &self,
        provider: &ProviderId,
        prompt: &str,
        system: &str,
        goal: &str,
        temperature: f32,
    ) -> Result<ToolCall, String> {
        let options = PromptOptions {
            system_prompt: Some(system.to_string()),
            temperature: Some(temperature),
            model: self.current_model.clone(),
            num_ctx: self.num_ctx,
            num_predict: Some(2048),
            think: Some(false),
            ..PromptOptions::default()
        };

        let response = self
            .router
            .send(*provider, prompt.to_string(), options)
            .await
            .map_err(|e| format!("LLM error: {:?}", e))?;

        self.parse_response(&response.text, goal)
    }

    fn parse_response(&self, text: &str, goal: &str) -> Result<ToolCall, String> {
        let text = text.trim();

        // Structured write blocks are preferred: they avoid escaping the file
        // body inside a JSON string (the main failure mode of small models).
        if let Some(block) = parse_write_blocks(text).into_iter().next() {
            return Ok(ToolCall {
                tool: "write_file".to_string(),
                args: serde_json::json!({
                    "path": block.path,
                    "content": block.content,
                }),
                reasoning: "structured [[WRITE_FILE]] block".to_string(),
            });
        }

        // An opening block without its closing marker means the selector was
        // truncated. Keep the path and let the executor generate the payload.
        if !contains_marker(text, WRITE_BLOCK_CLOSE) {
            if let Some(path) = parse_write_header_path(text) {
                return Ok(ToolCall {
                    tool: "write_file".to_string(),
                    args: serde_json::json!({"path": path, "content": ""}),
                    reasoning: "truncated [[WRITE_FILE]] block".to_string(),
                });
            }
        }

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

        // Last resort for small models: prose plus a single code fence and a
        // recognizable path. Keep only the fenced payload.
        if goal_has_write_intent(goal) {
            if let Some(block) = extract_fenced_write_payload(text, goal) {
                return Ok(ToolCall {
                    tool: "write_file".to_string(),
                    args: serde_json::json!({
                        "path": block.path,
                        "content": block.content,
                    }),
                    reasoning: "fenced write payload fallback".to_string(),
                });
            }
        }

        Err(format!(
            "Could not parse tool selection from: {}",
            truncate(text, 300)
        ))
    }

    fn decision_to_call(&self, decision: ToolSelectionResponse) -> Result<ToolCall, String> {
        match decision.tool {
            Some(tool_name) if !tool_name.is_empty() => {
                let args = decision.args.unwrap_or_default();
                let reasoning = decision.reasoning.unwrap_or_default();
                let normalized = tool_name.to_lowercase().replace(" ", "_");
                Ok(ToolCall {
                    tool: normalized,
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

    fn required_fields(schema: &Value) -> Vec<String> {
        schema
            .get("required")
            .and_then(Value::as_array)
            .map(|fields| {
                fields
                    .iter()
                    .filter_map(|field| field.as_str().map(ToString::to_string))
                    .collect()
            })
            .unwrap_or_default()
    }

    fn heuristic_fallback(
        &self,
        goal: &str,
        available_tools: &[String],
    ) -> Result<ToolCall, String> {
        let lower = goal.to_lowercase();
        let mcp_only = !available_tools.is_empty()
            && available_tools.iter().all(|tool| tool.starts_with("mcp__"));
        if mcp_only || prompt_mentions_mcp(goal) {
            let terms = mcp_prompt_terms(goal);
            let mut candidates: Vec<&ToolDescription> = available_tools
                .iter()
                .filter(|name| name.starts_with("mcp__"))
                .filter_map(|name| {
                    self.tool_descriptions
                        .iter()
                        .find(|tool| &tool.name == name)
                })
                .collect();

            let best = if terms.is_empty() {
                candidates
                    .iter()
                    .find(|tool| Self::required_fields(&tool.schema).is_empty())
                    .copied()
            } else {
                candidates
                    .sort_by_key(|candidate| std::cmp::Reverse(mcp_tool_score(candidate, &terms)));
                candidates.first().copied()
            };

            if let Some(tool) = best {
                let required = Self::required_fields(&tool.schema);
                if required.is_empty() {
                    return Ok(ToolCall {
                        tool: tool.name.clone(),
                        args: serde_json::json!({}),
                        reasoning: "heuristic fallback: explicit MCP request".to_string(),
                    });
                }
                return Ok(ToolCall {
                    tool: String::new(),
                    args: Value::Null,
                    reasoning: format!(
                        "MCP tool {} requires arguments ({}). Provide them and retry.",
                        tool.name,
                        required.join(", ")
                    ),
                });
            }
        }

        if goal_has_write_intent(goal) {
            if let Some(path) = detect_write_path(goal) {
                return Ok(ToolCall {
                    tool: "write_file".to_string(),
                    // Content is filled by the executor's dedicated payload
                    // generation pass; never ask the selector to inline it.
                    args: serde_json::json!({"path": path, "content": ""}),
                    reasoning: "heuristic fallback: write intent detected".to_string(),
                });
            }
        }

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

        let target_prompt_tokens = self.target_prompt_tokens();
        let estimated_tokens = (tools_desc.len() as f64 * ESTIMATED_TOKENS_PER_CHAR) as usize;
        let tools_desc = if estimated_tokens > target_prompt_tokens {
            self.truncate_tool_descriptions(&filtered_descriptions)
        } else {
            tools_desc
        };

        let context_str = if context.is_empty() {
            "No previous steps executed yet.".to_string()
        } else {
            let max_context_chars =
                (self.context_token_budget() as f64 / ESTIMATED_TOKENS_PER_CHAR) as usize;
            let mut parts: Vec<String> = context
                .iter()
                .map(|r| {
                    let input = r
                        .metadata
                        .as_ref()
                        .and_then(|m| m.input.as_deref())
                        .map(|i| format!(" input={}", truncate(i, 220)))
                        .unwrap_or_default();
                    format!(
                        "- {} -> {}{}: {}",
                        r.step_id,
                        r.tool_name.as_deref().unwrap_or("llm"),
                        input,
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

        let mcp_only = !available_tools.is_empty()
            && available_tools.iter().all(|tool| tool.starts_with("mcp__"));
        let mcp_rules = if mcp_only {
            "\nMCP MODE RULES:\n\
- The user explicitly asked to use MCP. You MUST choose exactly one MCP tool from the list.\n\
- Use the exact tool name shown (it starts with mcp__).\n\
- Arguments MUST include every property listed in the schema's \"required\" array, with the exact field names. Never send empty args when the schema requires fields.\n\
- Do not invent fields that are not in the schema.\n\
- Never return null unless no MCP tool can possibly satisfy the goal.\n\
- If the goal asks for N items (for example 5 users), call the tool once per item with different unique values (username/email) before returning null.\n"
        } else {
            ""
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
STRATEGY:\n{}\n\
{}\n\
COMPLETION RULES:\n\
- Do not repeat write_file/apply_patch for the same path.\n\
- For requests that ask for HTML, CSS and JS, create the missing files before returning null.\n\
- If the needed files were already written successfully, return null.\n\n\
WRITE TOOL FORMAT (use this instead of JSON for write_file):\n\
[[WRITE_FILE path=\"relative/path.ext\"]]\n\
```\n\
<full file content, unescaped>\n\
```\n\
[[/WRITE_FILE]]\n\
Do NOT put the file content inside JSON. Do not escape newlines.\n\n\
RESPOND IN JSON FORMAT for every other tool:\n\
{{\n  \"tool\": \"tool_name\",\n  \"args\": {{ ... }},\n  \"reasoning\": \"why this tool next\"\n}}\n\n\
IF NO MORE TOOLS NEEDED (goal achieved or blocked):\n\
{{\n  \"tool\": null,\n  \"args\": null,\n  \"reasoning\": \"goal achieved\" or \"blocked because...\"\n}}",
            goal,
            context_str,
            tools_desc,
            self.strategy_guidance(),
            mcp_rules
        )
    }

    /// Extra behavioural guidance injected into the tool-selection prompt.
    /// Large models get more freedom (multi-step evidence gathering) while
    /// small models are constrained to a single, low-risk action.
    fn strategy_guidance(&self) -> &'static str {
        match self.model_size {
            ModelSize::Small => {
                "- Choose exactly ONE tool.\n\
- Use the simplest tool that can answer the goal.\n\
- Keep arguments minimal and valid.\n\
- If you are not confident, return null with a clear reason instead of guessing."
            }
            ModelSize::Medium => {
                "- Read before you write: inspect existing files before editing them.\n\
- After a successful write, validate it with run_command when a project command exists.\n\
- Stop and return null once the goal is satisfied."
            }
            ModelSize::Large => {
                "- Plan: you may chain several read-only tools (search_code, open_file, git_diff) to gather evidence BEFORE editing.\n\
- For bug fixes, locate the cause with search_code/open_file first, then apply a surgical apply_patch.\n\
- Prefer apply_patch for small edits and write_file for new files.\n\
- After editing, run the project's validation command (cargo check/test, npm test, ...) before finishing.\n\
- Do not repeat an identical tool call with identical arguments; if it failed, change approach.\n\
- Return null when the goal is complete, with a short summary of what changed."
            }
        }
    }

    fn context_token_budget(&self) -> usize {
        match self.model_size {
            ModelSize::Small => SMALL_CONTEXT_TOKENS,
            ModelSize::Medium => MEDIUM_CONTEXT_TOKENS,
            ModelSize::Large => LARGE_CONTEXT_TOKENS,
        }
    }

    fn target_prompt_tokens(&self) -> usize {
        match self.model_size {
            ModelSize::Small => SMALL_TARGET_PROMPT_TOKENS,
            ModelSize::Medium => MEDIUM_TARGET_PROMPT_TOKENS,
            ModelSize::Large => LARGE_TARGET_PROMPT_TOKENS,
        }
    }

    fn truncate_tool_descriptions(&self, descriptions: &[&ToolDescription]) -> String {
        let target_prompt_tokens = self.target_prompt_tokens();
        let max_per_tool_chars = (target_prompt_tokens as f64
            / ESTIMATED_TOKENS_PER_CHAR
            / descriptions.len().max(1) as f64) as usize;
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
        r#"You are a coding assistant that selects tools to accomplish tasks.

RULES:
- Tools whose names start with mcp__ are live MCP tools. When the goal asks to use MCP, you MUST select one of those tools.
- For MCP tools, copy every required field from the schema into args with the exact name; never call an MCP tool with an empty args object if it has required fields.
- Use search_code before write_file ONLY if the project has existing files to examine
- If search_code returns empty results, proceed directly to write_file
- Never call the same tool more than once with similar arguments
- Prefer read operations before write operations
- If a tool fails, try a different approach or ask for clarification
- Use run_command after write_file to validate changes (cargo check, npm test, etc.)
- For write_file, prefer the [[WRITE_FILE path="..."]] block format with the raw file body; JSON-escaped multiline content is error-prone.

THINKING PROCESS:
1. What is the current state?
2. What tools are available?
3. Which tool gets us closest to the goal?
4. What are the arguments?

Respond ONLY with valid JSON."#
            .to_string()
    }

    async fn select_provider(&self, goal: &str) -> Result<ProviderId, String> {
        let available = self.router.available();
        eprintln!("[llm_tool_selector] available_providers={:?}", available);
        if available.is_empty() {
            return Err("No providers available for tool selection".to_string());
        }

        // Prefer the provider whose capabilities best match the goal. This is
        // what lets a reasoning-heavy task route to a large hosted reasoner
        // instead of always defaulting to the first local provider.
        let required = crate::orchestrator::planner::infer_capabilities(goal);
        let provider_infos = self.router.get_available_providers().await;
        if let Some(id) = crate::orchestrator::provider_selector::ProviderSelector::select(
            &required,
            &provider_infos,
        ) {
            eprintln!(
                "[llm_tool_selector] capability_selected_provider={:?} required={:?}",
                id, required
            );
            return Ok(id);
        }

        Ok(available[0])
    }
}

const WRITE_BLOCK_OPEN: &str = "[[WRITE_FILE";
const WRITE_BLOCK_CLOSE: &str = "[[/WRITE_FILE]]";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WriteBlock {
    pub path: String,
    pub content: String,
}

fn goal_has_write_intent(goal: &str) -> bool {
    const WRITE_VERBS: &[&str] = &[
        "crea",
        "crear",
        "create",
        "write",
        "escribe",
        "escribir",
        "genera",
        "generar",
        "generate",
        "save",
        "guarda",
        "guardar",
        "anade",
        "anadir",
        "añade",
        "añadir",
        "update",
        "modify",
        "modifica",
        "actualiza",
    ];

    goal.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .any(|token| WRITE_VERBS.contains(&token))
}

const WRITE_EXTENSIONS: &[&str] = &[
    "html", "htm", "css", "scss", "js", "mjs", "cjs", "jsx", "ts", "tsx", "vue", "svelte", "rs",
    "py", "go", "java", "kt", "kts", "rb", "php", "json", "toml", "yaml", "yml", "md", "txt", "sh",
    "sql", "xml", "svg", "dart", "c", "cc", "cpp", "h", "hpp", "cs", "lua", "qml", "zig",
];

fn detect_write_path(text: &str) -> Option<String> {
    for raw in text.split_whitespace() {
        let token = raw
            .trim_matches(|c: char| {
                matches!(
                    c,
                    '`' | '"' | '\'' | ',' | ';' | ':' | '.' | ')' | '(' | '[' | ']' | '{' | '}'
                )
            })
            .trim_start_matches("./")
            .trim_start_matches('/');

        if token.is_empty() || token.contains("..") {
            continue;
        }

        let Some((_, ext)) = token.rsplit_once('.') else {
            continue;
        };
        if ext.is_empty() || ext.len() > 8 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
            continue;
        }

        let known = WRITE_EXTENSIONS
            .iter()
            .any(|candidate| candidate.eq_ignore_ascii_case(ext));
        if token.contains('/') || known {
            return Some(token.to_string());
        }
    }
    None
}

fn extract_block_path(attrs: &str) -> Option<String> {
    let lower = attrs.to_ascii_lowercase();

    if let Some(key_pos) = lower.find("path") {
        let rest = attrs[key_pos + "path".len()..].trim_start();
        if let Some(rest) = rest.strip_prefix('=') {
            let rest = rest.trim_start();
            if let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') {
                if let Some(end) = rest[1..].find(quote) {
                    let value = rest[1..1 + end].trim();
                    if !value.is_empty() {
                        return Some(value.to_string());
                    }
                }
            } else {
                let end = rest
                    .find(|c: char| c.is_whitespace() || c == ']')
                    .unwrap_or(rest.len());
                let value = rest[..end].trim().trim_matches('"').trim_matches('\'');
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    // Fallback: first quoted string in the tag.
    for quote in ['"', '\''] {
        if let Some(start) = attrs.find(quote) {
            if let Some(end_rel) = attrs[start + 1..].find(quote) {
                let value = attrs[start + 1..start + 1 + end_rel].trim();
                if !value.is_empty() {
                    return Some(value.to_string());
                }
            }
        }
    }

    None
}

fn find_marker(text: &str, marker: &str) -> Option<usize> {
    text.to_ascii_lowercase().find(&marker.to_ascii_lowercase())
}

fn contains_marker(text: &str, marker: &str) -> bool {
    find_marker(text, marker).is_some()
}

fn parse_write_header_path(text: &str) -> Option<String> {
    let open = find_marker(text, WRITE_BLOCK_OPEN)?;
    let attrs_start = open + WRITE_BLOCK_OPEN.len();
    let attrs_end = attrs_start + text[attrs_start..].find("]]")?;
    extract_block_path(&text[attrs_start..attrs_end])
}

fn clean_block_content(raw: &str) -> String {
    let trimmed = raw.trim();

    let without_open = match trimmed.strip_prefix("```") {
        Some(rest) => match rest.find('\n') {
            Some(line_end) => &rest[line_end + 1..],
            None => "",
        },
        None => trimmed,
    };

    let without_close = match without_open.rfind("```") {
        Some(index) if without_open[index..].trim() == "```" => &without_open[..index],
        _ => without_open,
    };

    without_close
        .trim_matches(|c| c == '\n' || c == '\r')
        .to_string()
}

/// Parses every `[[WRITE_FILE path="..."]] ... [[/WRITE_FILE]]` block. The
/// payload is delimited by markers and never goes through JSON parsing, so raw
/// braces, quotes and even internal ``` fences survive intact.
pub(crate) fn parse_write_blocks(text: &str) -> Vec<WriteBlock> {
    let mut blocks = Vec::new();
    let mut cursor = 0usize;

    while let Some(open_rel) = find_marker(&text[cursor..], WRITE_BLOCK_OPEN) {
        let open = cursor + open_rel;
        let attrs_start = open + WRITE_BLOCK_OPEN.len();
        let Some(attrs_end_rel) = text[attrs_start..].find("]]") else {
            break;
        };
        let attrs_end = attrs_start + attrs_end_rel;
        let body_start = attrs_end + 2;

        let Some(close_rel) = find_marker(&text[body_start..], WRITE_BLOCK_CLOSE) else {
            break;
        };
        let close = body_start + close_rel;

        let path = extract_block_path(&text[attrs_start..attrs_end]);
        if let Some(path) = path {
            let content = clean_block_content(&text[body_start..close]);
            blocks.push(WriteBlock { path, content });
        }

        cursor = close + WRITE_BLOCK_CLOSE.len();
    }

    blocks
}

/// Strips one leading/trailing code fence from a model response, keeping the
/// body untouched. Also used by the executor's two-phase generation.
pub(crate) fn strip_fenced_payload(text: &str) -> String {
    let trimmed = text.trim();
    let Some(fence_start) = trimmed.find("```") else {
        return trimmed.to_string();
    };

    let after_open = &trimmed[fence_start + 3..];
    let Some(line_end) = after_open.find('\n') else {
        return String::new();
    };
    let body_start = fence_start + 3 + line_end + 1;
    let body = &trimmed[body_start..];
    let body_end = body.rfind("```").unwrap_or(body.len());

    body[..body_end]
        .trim_matches(|c| c == '\n' || c == '\r')
        .to_string()
}

fn extract_fenced_write_payload(text: &str, goal: &str) -> Option<WriteBlock> {
    let path = detect_write_path(goal).or_else(|| detect_write_path(text))?;
    let content = strip_fenced_payload(text);
    if content.trim().is_empty() {
        return None;
    }
    Some(WriteBlock { path, content })
}

fn extract_json_object(text: &str) -> Option<String> {
    let bytes = text.as_bytes();
    let len = bytes.len();
    let mut i = 0;

    while i < len {
        if bytes[i] != b'{' {
            i += 1;
            continue;
        }

        let start = i;
        let mut depth = 0u32;
        let mut in_string = false;
        let mut escaped = false;
        let mut found_end = false;

        // Brace counting must ignore braces that live inside JSON strings.
        // Tool arguments routinely embed source code (`fn f() { ... }`,
        // `body { }`, unbalanced braces in comments), which used to terminate
        // the candidate early and made every multiline write fail to parse.
        while i < len {
            let byte = bytes[i];
            i += 1;

            if in_string {
                if escaped {
                    escaped = false;
                } else if byte == b'\\' {
                    escaped = true;
                } else if byte == b'"' {
                    in_string = false;
                }
                continue;
            }

            match byte {
                b'"' => in_string = true,
                b'{' => depth += 1,
                b'}' => {
                    depth = depth.saturating_sub(1);
                    if depth == 0 {
                        found_end = true;
                        break;
                    }
                }
                _ => {}
            }
        }

        if found_end {
            let candidate = &text[start..i];
            if candidate.len() > 2 && serde_json::from_str::<serde_json::Value>(candidate).is_ok() {
                return Some(candidate.to_string());
            }
        }
    }
    None
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        return s.to_string();
    }

    // Truncate on a UTF-8 character boundary. Provider responses frequently
    // contain multi-byte characters (em dashes, accents, emoji); slicing at an
    // arbitrary byte offset panics inside the tool-selection path.
    let mut end = max;
    while end > 0 && !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}...", &s[..end])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_respects_utf8_char_boundaries() {
        let text = "respuesta con un caracter multibyte — y texto adicional";
        let truncated = truncate(text, 37);
        assert!(truncated.ends_with("..."));
        assert!(text.starts_with(truncated.trim_end_matches('.')));
    }

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
            current_model: None,
            num_ctx: None,
        };
        let text = "```json\n{\"tool\": \"open_file\", \"args\": {\"path\": \"main.rs\"}, \"reasoning\": \"test\"}\n```";
        let result = selector.parse_response(text, "");
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
            current_model: None,
            num_ctx: None,
        };
        let text =
            r#"{"tool": "search_code", "args": {"query": "test"}, "reasoning": "need to search"}"#;
        let result = selector.parse_response(text, "");
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
            current_model: None,
            num_ctx: None,
        };
        let text = "I think we should search first: {\"tool\": \"search_code\", \"args\": {\"query\": \"test\"}, \"reasoning\": \"need context\"}";
        let result = selector.parse_response(text, "");
        assert!(result.is_ok());
    }

    #[test]
    fn test_none_tool_means_done() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
            current_model: None,
            num_ctx: None,
        };
        let text = r#"{"tool": null, "args": null, "reasoning": "goal achieved"}"#;
        let result = selector.parse_response(text, "");
        assert!(result.is_ok());
        assert!(result.unwrap().tool.is_empty());
    }

    #[test]
    fn test_heuristic_fallback_search() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
            current_model: None,
            num_ctx: None,
        };
        let result = selector
            .heuristic_fallback("find the router implementation", &[])
            .unwrap();
        assert_eq!(result.tool, "search_code");
    }

    #[test]
    fn test_heuristic_fallback_read() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
            current_model: None,
            num_ctx: None,
        };
        let result = selector.heuristic_fallback("open main.rs", &[]).unwrap();
        assert_eq!(result.tool, "open_file");
    }

    #[test]
    fn test_resolve_mcp_tool_without_prefix() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Small,
            current_model: None,
            num_ctx: None,
        };
        let available = vec!["mcp__fastapi_users__create_user".to_string()];
        let call = selector.resolve_tool_name(
            ToolCall {
                tool: "create_user".to_string(),
                args: serde_json::json!({}),
                reasoning: String::new(),
            },
            &available,
        );
        assert_eq!(call.tool, "mcp__fastapi_users__create_user");
    }

    #[test]
    fn test_heuristic_fallback_prefers_mcp_when_explicit() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![ToolDescription {
                name: "mcp__fastapi_users__health_check".to_string(),
                description: "Comprueba la salud del servidor".to_string(),
                when_to_use: String::new(),
                when_not_to_use: String::new(),
                examples: vec![],
                schema: serde_json::json!({"type": "object", "properties": {}}),
            }],
            model_size: ModelSize::Small,
            current_model: None,
            num_ctx: None,
        };
        let available = vec!["mcp__fastapi_users__health_check".to_string()];
        let result = selector
            .heuristic_fallback("Usando el MCP revisa la salud del servidor", &available)
            .unwrap();
        assert_eq!(result.tool, "mcp__fastapi_users__health_check");
    }

    #[test]
    fn test_heuristic_fallback_does_not_call_mcp_without_required_args() {
        let selector = LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![ToolDescription {
                name: "mcp__fastapi_users__create_user".to_string(),
                description: "Crea un usuario".to_string(),
                when_to_use: String::new(),
                when_not_to_use: String::new(),
                examples: vec![],
                schema: serde_json::json!({
                    "type": "object",
                    "required": ["username", "email", "password"]
                }),
            }],
            model_size: ModelSize::Small,
            current_model: None,
            num_ctx: None,
        };
        let available = vec!["mcp__fastapi_users__create_user".to_string()];
        let result = selector
            .heuristic_fallback("Usando el MCP crea 5 usuarios", &available)
            .unwrap();
        assert!(result.tool.is_empty());
        assert!(result.reasoning.contains("requires arguments"));
    }

    #[test]
    fn test_extract_json_with_nested_braces() {
        let text =
            r#"Some text {"tool": "write_file", "args": {"content": "fn main() {}"}} trailing"#;
        let result = extract_json_object(text);
        assert!(result.is_some());
        let parsed: ToolSelectionResponse =
            serde_json::from_str(&result.unwrap()).expect("should parse nested json");
        assert_eq!(parsed.tool, Some("write_file".to_string()));
    }

    #[test]
    fn test_extract_json_object_ignores_braces_and_quotes_inside_content() {
        let content = r#"body {
  color: red;
}
@media screen {
  .a { content: "{not json}"; }
}
// unbalanced brace in comment: {"#;
        let payload = serde_json::json!({
            "tool": "write_file",
            "args": { "path": "styles.css", "content": content },
            "reasoning": "write the stylesheet"
        });
        let text = format!("Sure, here is the call:\n{}\nDone.", payload);

        let extracted = extract_json_object(&text).expect("should extract object");
        let parsed: ToolSelectionResponse =
            serde_json::from_str(&extracted).expect("should parse extracted object");
        assert_eq!(parsed.tool, Some("write_file".to_string()));
        let parsed_content = parsed
            .args
            .as_ref()
            .and_then(|args| args.get("content"))
            .and_then(|value| value.as_str())
            .expect("content should roundtrip");
        assert_eq!(parsed_content, content);
    }

    #[test]
    fn test_extract_json_object_handles_escaped_quotes_and_backslashes() {
        let content = "let s = \"a \\\"quoted\\\" value\";\nif (x) { y(); }";
        let payload = serde_json::json!({
            "tool": "write_file",
            "args": { "path": "src/main.js", "content": content }
        });
        let text = format!("prefix {} suffix", payload);
        let extracted = extract_json_object(&text).expect("should extract object");
        let parsed: ToolSelectionResponse =
            serde_json::from_str(&extracted).expect("should parse escaped content");
        assert_eq!(parsed.args.unwrap()["content"].as_str(), Some(content));
    }

    fn test_selector() -> LlmToolSelector {
        LlmToolSelector {
            router: Arc::new(ProviderRouter::new()),
            tool_descriptions: vec![],
            model_size: ModelSize::Medium,
            current_model: None,
            num_ctx: None,
        }
    }

    #[test]
    fn test_parse_write_block_simple() {
        let text =
            "[[WRITE_FILE path=\"src/main.js\"]]\n```js\nconsole.log('hi');\n```\n[[/WRITE_FILE]]";
        let blocks = parse_write_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].path, "src/main.js");
        assert_eq!(blocks[0].content, "console.log('hi');");
    }

    #[test]
    fn test_parse_write_block_preserves_internal_fences() {
        let text = r#"[[WRITE_FILE path="docs/example.md"]]
```md
# Example
```rust
fn main() {}
```
done
```
[[/WRITE_FILE]]"#;
        let blocks = parse_write_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].path, "docs/example.md");
        assert!(blocks[0].content.contains("```rust"));
        assert!(blocks[0].content.contains("fn main() {}"));
        assert!(blocks[0].content.trim_end().ends_with("done"));
    }

    #[test]
    fn test_parse_write_block_keeps_braces_and_quotes() {
        let text = r#"[[WRITE_FILE path='styles.css']]
```css
body { color: "red"; }
.card { content: "{not json}"; }
```
[[/WRITE_FILE]]"#;
        let blocks = parse_write_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].path, "styles.css");
        assert_eq!(
            blocks[0].content,
            "body { color: \"red\"; }\n.card { content: \"{not json}\"; }"
        );
    }

    #[test]
    fn test_parse_response_prioritizes_write_block_over_json() {
        let selector = test_selector();
        let text = "[[WRITE_FILE path=\"index.html\"]]\n```html\n<h1>ok</h1>\n```\n[[/WRITE_FILE]]";
        let call = selector
            .parse_response(text, "crea index.html")
            .expect("call");
        assert_eq!(call.tool, "write_file");
        assert_eq!(call.args["path"], "index.html");
        assert_eq!(call.args["content"], "<h1>ok</h1>");
    }

    #[test]
    fn test_parse_response_truncated_block_requests_payload_generation() {
        let selector = test_selector();
        let text = "[[WRITE_FILE path=\"src/big.ts\"]]\n```ts\nexport const value = 1;";
        let call = selector
            .parse_response(text, "create src/big.ts")
            .expect("call");
        assert_eq!(call.tool, "write_file");
        assert_eq!(call.args["path"], "src/big.ts");
        assert_eq!(call.args["content"], "");
    }

    #[test]
    fn test_parse_response_extracts_fence_with_surrounding_prose() {
        let selector = test_selector();
        let text = "Sure! Here is the file:\n\n```js\nconst answer = 42;\n```\n\nLet me know if you need changes.";
        let call = selector
            .parse_response(text, "crea src/app.js")
            .expect("call");
        assert_eq!(call.tool, "write_file");
        assert_eq!(call.args["path"], "src/app.js");
        assert_eq!(call.args["content"], "const answer = 42;");
    }

    #[test]
    fn test_strip_fenced_payload_ignores_prose_after_fence() {
        let text = "```python\nprint('hi')\n```\ntrailing prose";
        assert_eq!(strip_fenced_payload(text), "print('hi')");
    }

    #[test]
    fn test_parse_write_blocks_supports_multiple_blocks() {
        let text = "[[WRITE_FILE path=\"a.js\"]]\n```\nA\n```\n[[/WRITE_FILE]]\n[[WRITE_FILE path=\"b.js\"]]\n```\nB\n```\n[[/WRITE_FILE]]";
        let blocks = parse_write_blocks(text);
        assert_eq!(blocks.len(), 2);
        assert_eq!(blocks[0].path, "a.js");
        assert_eq!(blocks[1].path, "b.js");
        assert_eq!(blocks[1].content, "B");
    }

    #[test]
    fn test_parse_write_block_is_case_insensitive() {
        let text = "[[write_file path=\"a.txt\"]]\n```\nhello\n```\n[[/write_file]]";
        let blocks = parse_write_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert_eq!(blocks[0].path, "a.txt");
        assert_eq!(blocks[0].content, "hello");
    }

    #[test]
    fn test_parse_write_block_keeps_unbalanced_braces() {
        let text = "[[WRITE_FILE path=\"src/broken.rs\"]]\n```rs\nfn broken() {\n    // unmatched brace stays literal\n```\n[[/WRITE_FILE]]";
        let blocks = parse_write_blocks(text);
        assert_eq!(blocks.len(), 1);
        assert!(blocks[0].content.contains("fn broken() {"));
        assert!(blocks[0]
            .content
            .contains("// unmatched brace stays literal"));
    }
}

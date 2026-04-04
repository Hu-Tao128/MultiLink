use crate::config::TaskWeight;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QueryIntent {
    ProjectWide,
    FileScoped,
    SymbolScoped,
    Conversational,
}

#[derive(Debug, Clone, Copy)]
pub struct IntentBudget {
    pub project_budget_ratio: u8,
    pub top_k_cap: usize,
}

pub fn detect_query_intent(prompt: &str) -> QueryIntent {
    let p = prompt.to_ascii_lowercase();

    let project_wide_markers = [
        "de que trata",
        "qué trata",
        "que hace este proyecto",
        "resumen del proyecto",
        "arquitectura",
        "overview",
        "whole project",
    ];
    if project_wide_markers.iter().any(|m| p.contains(m)) {
        return QueryIntent::ProjectWide;
    }

    let file_markers = [
        "archivo", "file", "analiza", "revisa", "review", "improve", "mejorar", ".py", ".rs",
        ".ts", ".tsx", ".js", ".dart", ".md", ".json", ".yml", ".yaml", ".toml",
    ];
    if file_markers.iter().any(|m| p.contains(m)) {
        return QueryIntent::FileScoped;
    }

    for token in prompt.split_whitespace() {
        let t = token
            .trim_matches(|c: char| {
                c == '`'
                    || c == '"'
                    || c == '\''
                    || c == ','
                    || c == ';'
                    || c == ':'
                    || c == '('
                    || c == ')'
            })
            .trim_start_matches("./")
            .trim_start_matches('/');
        if t.contains('/') && t.contains('.') && !t.contains("..") {
            return QueryIntent::FileScoped;
        }
    }

    let symbol_markers = [
        "funcion", "function", "method", "clase", "class", "struct", "trait", "fn ",
    ];
    if symbol_markers.iter().any(|m| p.contains(m)) {
        return QueryIntent::SymbolScoped;
    }

    QueryIntent::Conversational
}

pub fn budget_for_intent(intent: QueryIntent) -> IntentBudget {
    match intent {
        QueryIntent::ProjectWide => IntentBudget {
            project_budget_ratio: 55,
            top_k_cap: 12,
        },
        QueryIntent::FileScoped => IntentBudget {
            project_budget_ratio: 45,
            top_k_cap: 7,
        },
        QueryIntent::SymbolScoped => IntentBudget {
            project_budget_ratio: 35,
            top_k_cap: 5,
        },
        QueryIntent::Conversational => IntentBudget {
            project_budget_ratio: 20,
            top_k_cap: 4,
        },
    }
}

pub fn task_weight_for_prompt(prompt: &str, intent: QueryIntent) -> TaskWeight {
    let p = prompt.to_ascii_lowercase();
    let token_count = p.split_whitespace().count();

    let heavy_markers = [
        "arquitectura",
        "architecture",
        "distributed",
        "orchestrator",
        "deep analysis",
        "critical",
        "seguridad",
    ];
    if heavy_markers.iter().any(|m| p.contains(m)) || token_count > 180 {
        return if token_count > 300 {
            TaskWeight::Critical
        } else {
            TaskWeight::Heavy
        };
    }

    match intent {
        QueryIntent::Conversational => {
            if token_count < 20 {
                TaskWeight::Light
            } else {
                TaskWeight::Medium
            }
        }
        QueryIntent::FileScoped | QueryIntent::SymbolScoped => {
            if token_count < 35 {
                TaskWeight::Light
            } else {
                TaskWeight::Medium
            }
        }
        QueryIntent::ProjectWide => TaskWeight::Medium,
    }
}

use multilink_core::{
    budget_for_intent, detect_query_intent, task_weight_for_prompt, QueryIntent, TaskWeight,
};

#[test]
fn detects_project_wide_queries() {
    let intent = detect_query_intent("De que trata este proyecto?");
    assert_eq!(intent, QueryIntent::ProjectWide);
}

#[test]
fn detects_symbol_scoped_queries() {
    let intent = detect_query_intent("Explica la funcion parseMessageSegments");
    assert_eq!(intent, QueryIntent::SymbolScoped);
}

#[test]
fn project_wide_has_higher_topk_than_symbol_scope() {
    let wide = budget_for_intent(QueryIntent::ProjectWide);
    let symbol = budget_for_intent(QueryIntent::SymbolScoped);
    assert!(wide.top_k_cap > symbol.top_k_cap);
    assert!(wide.project_budget_ratio > symbol.project_budget_ratio);
}

#[test]
fn task_weight_marks_architecture_prompts_as_heavy() {
    let prompt = "Please provide a deep architecture analysis for this distributed project";
    let weight = task_weight_for_prompt(prompt, QueryIntent::ProjectWide);
    assert_eq!(weight, TaskWeight::Heavy);
}

#[test]
fn task_weight_keeps_short_conversation_prompts_light() {
    let prompt = "hello, can you summarize this?";
    let weight = task_weight_for_prompt(prompt, QueryIntent::Conversational);
    assert_eq!(weight, TaskWeight::Light);
}

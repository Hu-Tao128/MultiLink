use multilink_core::{budget_for_intent, detect_query_intent, QueryIntent};

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

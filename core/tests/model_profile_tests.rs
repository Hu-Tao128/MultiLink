use multilink_core::{ModelClass, ModelProfile, ProviderCapabilities};

#[test]
fn tiny_models_get_reduced_project_budget() {
    let caps = ProviderCapabilities {
        max_context_tokens: 8192,
        parameter_count: Some(1_300_000_000),
        ..ProviderCapabilities::default_with_context(8192)
    };

    let profile = ModelProfile::from_capabilities("deepseek-coder:1.3b".to_string(), &caps);
    let budget = profile.retrieval_budget();

    assert_eq!(profile.class, ModelClass::Tiny);
    assert!(budget.project_top_k <= 4);
    assert!(budget.project_budget <= budget.safe_budget / 3 + 16);
}

#[test]
fn medium_models_get_higher_topk() {
    let caps = ProviderCapabilities {
        max_context_tokens: 16384,
        parameter_count: Some(7_000_000_000),
        ..ProviderCapabilities::default_with_context(16384)
    };

    let profile = ModelProfile::from_capabilities("qwen2.5-coder:7b".to_string(), &caps);
    let budget = profile.retrieval_budget();

    assert!(matches!(
        profile.class,
        ModelClass::Medium | ModelClass::Large
    ));
    assert!(budget.project_top_k >= 8);
    assert!(budget.project_budget > 0);
}

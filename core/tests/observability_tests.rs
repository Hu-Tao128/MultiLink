use multilink_core::ExecutionMetrics;

#[test]
fn execution_metrics_serializes_to_json() {
    let metrics = ExecutionMetrics {
        timestamp_ms: 1,
        model: "qwen2.5-coder:3b".to_string(),
        server: "http://127.0.0.1:11434".to_string(),
        tokens_in: 120,
        tokens_out: 64,
        context_tokens: 340,
        top_k_applied: 8,
        latency_ms: 512,
        fallback_used: false,
        retries: 0,
    };

    let json = serde_json::to_string(&metrics).expect("serialize metrics");
    assert!(json.contains("qwen2.5-coder:3b"));
    assert!(json.contains("\"top_k_applied\":8"));
}

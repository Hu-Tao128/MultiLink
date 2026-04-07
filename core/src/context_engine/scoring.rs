use crate::context_engine::tokenizer::Tokenizer;

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() || a.len() != b.len() {
        return 0.0;
    }

    let mut dot = 0f32;
    let mut na = 0f32;
    let mut nb = 0f32;
    for i in 0..a.len() {
        dot += a[i] * b[i];
        na += a[i] * a[i];
        nb += b[i] * b[i];
    }
    if na <= f32::EPSILON || nb <= f32::EPSILON {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

pub fn estimate_tokens_for_model(s: &str, _model: Option<&str>) -> usize {
    let tokens = Tokenizer::tokenize(s);
    (tokens.len() as f64 * 1.3) as usize
}

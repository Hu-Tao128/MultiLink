use serde::{Deserialize, Serialize};
use std::fs::{OpenOptions, create_dir_all};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

/// Registro de uso de tokens para una sola respuesta.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageRecord {
    pub session_id: String,
    pub model: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub is_estimated: bool,
    pub timestamp: String, // ISO 8601
}

/// Presupuesto de tokens definido por el usuario.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TokenBudget {
    /// Pool total de tokens (0 = sin límite)
    pub total_pool: usize,
    /// Tokens consumidos (calculado)
    pub consumed: usize,
    /// Fecha de inicio del período (ISO 8601)
    pub period_start: String,
    /// Fecha de fin del período (ISO 8601, vacío = sin fin)
    pub period_end: Option<String>,
}

impl TokenBudget {
    pub fn new(total_pool: usize) -> Self {
        let now = chrono::Utc::now().to_rfc3339();
        Self {
            total_pool,
            consumed: 0,
            period_start: now,
            period_end: None,
        }
    }

    pub fn remaining(&self) -> usize {
        if self.total_pool == 0 {
            return usize::MAX; // Sin límite
        }
        self.total_pool.saturating_sub(self.consumed)
    }

    pub fn usage_percent(&self) -> f64 {
        if self.total_pool == 0 {
            return 0.0;
        }
        (self.consumed as f64 / self.total_pool as f64) * 100.0
    }

    pub fn is_exceeded(&self) -> bool {
        self.total_pool > 0 && self.consumed >= self.total_pool
    }
}

/// Resumen agregado de uso.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UsageSummary {
    pub total_prompt_tokens: usize,
    pub total_completion_tokens: usize,
    pub total_tokens: usize,
    pub session_count: usize,
    pub model_breakdown: Vec<ModelUsage>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelUsage {
    pub model: String,
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
    pub request_count: usize,
}

/// Persiste registros de uso en un archivo JSONL.
pub struct UsageStore {
    path: PathBuf,
}

impl UsageStore {
    pub fn new(base_dir: &Path) -> Self {
        let path = base_dir.join("usage.jsonl");
        if let Some(parent) = path.parent() {
            let _ = create_dir_all(parent);
        }
        Self { path }
    }

    /// Agregar un registro de uso.
    pub fn append(&self, record: &UsageRecord) -> std::io::Result<()> {
        let line = serde_json::to_string(record)?;
        let mut file = OpenOptions::new()
            .create(true)
            .append(true)
            .open(&self.path)?;
        writeln!(file, "{}", line)?;
        Ok(())
    }

    /// Leer todos los registros (puede ser lento para archivos grandes).
    pub fn load_all(&self) -> std::io::Result<Vec<UsageRecord>> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let file = std::fs::File::open(&self.path)?;
        let reader = BufReader::new(file);
        let mut records = Vec::new();
        for line in reader.lines() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }
            if let Ok(record) = serde_json::from_str::<UsageRecord>(&line) {
                records.push(record);
            }
        }
        Ok(records)
    }

    /// Resumen global de uso.
    pub fn summary(&self) -> std::io::Result<UsageSummary> {
        let records = self.load_all()?;
        let mut total_prompt = 0usize;
        let mut total_completion = 0usize;
        let mut total = 0usize;
        let mut models: std::collections::HashMap<String, ModelUsage> = std::collections::HashMap::new();
        let mut sessions = std::collections::HashSet::new();

        for r in &records {
            total_prompt += r.prompt_tokens;
            total_completion += r.completion_tokens;
            total += r.total_tokens;
            sessions.insert(&r.session_id);

            let entry = models
                .entry(r.model.clone())
                .or_insert_with(|| ModelUsage {
                    model: r.model.clone(),
                    prompt_tokens: 0,
                    completion_tokens: 0,
                    total_tokens: 0,
                    request_count: 0,
                });
            entry.prompt_tokens += r.prompt_tokens;
            entry.completion_tokens += r.completion_tokens;
            entry.total_tokens += r.total_tokens;
            entry.request_count += 1;
        }

        let mut model_breakdown: Vec<ModelUsage> = models.into_values().collect();
        model_breakdown.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

        Ok(UsageSummary {
            total_prompt_tokens: total_prompt,
            total_completion_tokens: total_completion,
            total_tokens: total,
            session_count: sessions.len(),
            model_breakdown,
        })
    }

    /// Resumen por sesión.
    pub fn summary_by_session(&self) -> std::io::Result<Vec<(String, UsageSummary)>> {
        let records = self.load_all()?;
        let mut by_session: std::collections::HashMap<String, Vec<UsageRecord>> = std::collections::HashMap::new();

        for r in records {
            by_session.entry(r.session_id.clone()).or_default().push(r);
        }

        let mut result = Vec::new();
        for (session_id, recs) in by_session {
            let mut total_prompt = 0usize;
            let mut total_completion = 0usize;
            let mut total = 0usize;
            let mut models: std::collections::HashMap<String, ModelUsage> = std::collections::HashMap::new();

            for r in &recs {
                total_prompt += r.prompt_tokens;
                total_completion += r.completion_tokens;
                total += r.total_tokens;

                let entry = models
                    .entry(r.model.clone())
                    .or_insert_with(|| ModelUsage {
                        model: r.model.clone(),
                        prompt_tokens: 0,
                        completion_tokens: 0,
                        total_tokens: 0,
                        request_count: 0,
                    });
                entry.prompt_tokens += r.prompt_tokens;
                entry.completion_tokens += r.completion_tokens;
                entry.total_tokens += r.total_tokens;
                entry.request_count += 1;
            }

            let mut model_breakdown: Vec<ModelUsage> = models.into_values().collect();
            model_breakdown.sort_by(|a, b| b.total_tokens.cmp(&a.total_tokens));

            result.push((
                session_id,
                UsageSummary {
                    total_prompt_tokens: total_prompt,
                    total_completion_tokens: total_completion,
                    total_tokens: total,
                    session_count: 1,
                    model_breakdown,
                },
            ));
        }

        Ok(result)
    }
}

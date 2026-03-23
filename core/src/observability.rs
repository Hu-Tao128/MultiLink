use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionMetrics {
    pub timestamp_ms: u128,
    pub model: String,
    pub server: String,
    pub tokens_in: usize,
    pub tokens_out: usize,
    pub context_tokens: usize,
    pub top_k_applied: usize,
    pub latency_ms: u128,
    pub fallback_used: bool,
    pub retries: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ContextRetrievalMetrics {
    pub session_id: String,
    pub engine: String,
    pub context_latency_ms: u64,
    pub embedding_latency_ms: u64,
    pub index_refresh_ms: Option<u64>,
    pub retrieval_hit_rate: f32,
    pub truncation_rate: f32,
    pub error_rate: f32,
    pub selected_files: usize,
    pub used_tokens: usize,
    pub budget_used: usize,
    pub embedding_used: bool,
    pub top_k: usize,
}

impl ContextRetrievalMetrics {
    pub fn new(engine: &str, session_id: &str) -> Self {
        Self {
            engine: engine.to_string(),
            session_id: session_id.to_string(),
            ..Default::default()
        }
    }

    pub fn emit(&self, json_logs: bool) {
        if json_logs {
            if let Ok(line) = serde_json::to_string(self) {
                eprintln!("[context.metrics] {}", line);
                return;
            }
        }

        eprintln!(
            "[context.metrics] session={} engine={} latency_ms={} embed_latency_ms={} hit_rate={:.2} truncation_rate={:.2} error_rate={:.2} selected={} tokens={}",
            self.session_id,
            self.engine,
            self.context_latency_ms,
            self.embedding_latency_ms,
            self.retrieval_hit_rate,
            self.truncation_rate,
            self.error_rate,
            self.selected_files,
            self.used_tokens,
        );
    }
}

impl ExecutionMetrics {
    pub fn now_timestamp_ms() -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
    }

    pub fn emit(&self, json_logs: bool) {
        if json_logs {
            if let Ok(line) = serde_json::to_string(self) {
                eprintln!("[metrics] {}", line);
                return;
            }
        }

        eprintln!(
            "[metrics] model={} server={} tokens_in={} tokens_out={} context_tokens={} top_k={} latency_ms={} fallback={} retries={}",
            self.model,
            self.server,
            self.tokens_in,
            self.tokens_out,
            self.context_tokens,
            self.top_k_applied,
            self.latency_ms,
            self.fallback_used,
            self.retries,
        );
    }
}

/// Métricas de latencia por ruta con percentiles p50/p95/p99.
/// Usar un `LatencyTracker` por ruta (LSP handler, Context Engine, router).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RouteLatencyMetrics {
    pub route: String,
    pub samples: u64,
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub p99_ms: u64,
    pub min_ms: u64,
    pub max_ms: u64,
    pub mean_ms: f64,
}

/// Acumula muestras de latencia y calcula percentiles al emitir.
/// Diseñado para ser barato de actualizar — sin heap allocation por muestra.
///
/// Uso:
/// ```ignore
/// let mut tracker = LatencyTracker::new("lsp.hover");
/// let t = Instant::now();
/// // ... operación ...
/// tracker.record(t.elapsed().as_millis() as u64);
/// tracker.emit(json_logs);
/// ```
pub struct LatencyTracker {
    route: String,
    samples: Vec<u64>,
}

impl LatencyTracker {
    pub fn new(route: &str) -> Self {
        Self {
            route: route.to_string(),
            samples: Vec::new(),
        }
    }

    /// Registra una muestra de latencia en milisegundos.
    pub fn record(&mut self, latency_ms: u64) {
        self.samples.push(latency_ms);
    }

    /// Calcula percentiles y retorna las métricas sin limpiar el buffer.
    pub fn metrics(&self) -> RouteLatencyMetrics {
        if self.samples.is_empty() {
            return RouteLatencyMetrics {
                route: self.route.clone(),
                ..Default::default()
            };
        }

        let mut sorted = self.samples.clone();
        sorted.sort_unstable();

        let n = sorted.len();
        // Índices basados en (n-1) para alinear p50/p95/p99 con datos 1..=n.
        let p50 = sorted[(n - 1) * 50 / 100];
        let p95 = sorted[(n - 1) * 95 / 100];
        let p99 = sorted[(n - 1) * 99 / 100];
        let min = sorted[0];
        let max = sorted[n - 1];
        let mean = sorted.iter().sum::<u64>() as f64 / n as f64;

        RouteLatencyMetrics {
            route: self.route.clone(),
            samples: n as u64,
            p50_ms: p50,
            p95_ms: p95,
            p99_ms: p99,
            min_ms: min,
            max_ms: max,
            mean_ms: mean,
        }
    }

    /// Emite las métricas y limpia el buffer para el siguiente intervalo.
    pub fn emit_and_reset(&mut self, json_logs: bool) {
        let m = self.metrics();
        if m.samples == 0 {
            return;
        }
        if json_logs {
            if let Ok(line) = serde_json::to_string(&m) {
                eprintln!("[latency] {}", line);
            }
        } else {
            eprintln!(
                "[latency] route={} n={} p50={}ms p95={}ms p99={}ms min={}ms max={}ms mean={:.1}ms",
                m.route, m.samples, m.p50_ms, m.p95_ms, m.p99_ms, m.min_ms, m.max_ms, m.mean_ms
            );
        }
        self.samples.clear();
    }
}

impl RouteLatencyMetrics {
    pub fn emit(&self, json_logs: bool) {
        if json_logs {
            if let Ok(line) = serde_json::to_string(self) {
                eprintln!("[latency] {}", line);
                return;
            }
        }
        eprintln!(
            "[latency] route={} n={} p50={}ms p95={}ms p99={}ms mean={:.1}ms",
            self.route, self.samples, self.p50_ms, self.p95_ms, self.p99_ms, self.mean_ms
        );
    }
}

#[cfg(test)]
mod latency_tests {
    use super::*;

    #[test]
    fn percentiles_calculados_correctamente() {
        let mut tracker = LatencyTracker::new("test.route");
        // 100 muestras: 1..=100 ms
        for i in 1u64..=100 {
            tracker.record(i);
        }
        let m = tracker.metrics();
        assert_eq!(m.samples, 100);
        assert_eq!(m.p50_ms, 50);
        assert_eq!(m.p95_ms, 95);
        assert_eq!(m.p99_ms, 99);
        assert_eq!(m.min_ms, 1);
        assert_eq!(m.max_ms, 100);
    }

    #[test]
    fn emit_and_reset_limpia_buffer() {
        let mut tracker = LatencyTracker::new("test.reset");
        tracker.record(10);
        tracker.record(20);
        tracker.emit_and_reset(false);
        // Después del reset, métricas vacías
        let m = tracker.metrics();
        assert_eq!(m.samples, 0);
    }

    #[test]
    fn tracker_vacio_no_paniquea() {
        let tracker = LatencyTracker::new("empty");
        let m = tracker.metrics();
        assert_eq!(m.samples, 0);
        assert_eq!(m.p95_ms, 0);
    }

    #[test]
    fn una_muestra_todos_percentiles_iguales() {
        let mut tracker = LatencyTracker::new("single");
        tracker.record(42);
        let m = tracker.metrics();
        assert_eq!(m.p50_ms, 42);
        assert_eq!(m.p95_ms, 42);
        assert_eq!(m.p99_ms, 42);
    }
}

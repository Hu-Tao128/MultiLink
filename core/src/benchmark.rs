use std::time::{Duration, Instant};

#[derive(Debug, Clone)]
pub struct BenchmarkResult {
    pub name: String,
    pub iterations: u32,
    pub total_duration_ms: u64,
    pub avg_latency_ms: f64,
    pub throughput: f64,
}

impl BenchmarkResult {
    pub fn new(name: &str, iterations: u32, total_duration: Duration) -> Self {
        let total_duration_ms = total_duration.as_millis() as u64;
        let avg_latency_ms = total_duration_ms as f64 / iterations as f64;
        let throughput = (iterations as f64) / (total_duration_ms as f64 / 1000.0);

        Self {
            name: name.to_string(),
            iterations,
            total_duration_ms,
            avg_latency_ms,
            throughput,
        }
    }
}

pub struct BenchmarkRunner {
    warmup_iterations: u32,
    measured_iterations: u32,
}

impl Default for BenchmarkRunner {
    fn default() -> Self {
        Self::new()
    }
}

impl BenchmarkRunner {
    pub fn new() -> Self {
        Self {
            warmup_iterations: 3,
            measured_iterations: 100,
        }
    }

    pub fn with_warmup(mut self, n: u32) -> Self {
        self.warmup_iterations = n;
        self
    }

    pub fn with_iterations(mut self, n: u32) -> Self {
        self.measured_iterations = n;
        self
    }

    pub fn run<F, T>(&self, name: &str, mut f: F) -> BenchmarkResult
    where
        F: FnMut() -> T,
    {
        for _ in 0..self.warmup_iterations {
            let _ = f();
        }

        let start = Instant::now();
        for _ in 0..self.measured_iterations {
            let _ = f();
        }
        let duration = start.elapsed();

        BenchmarkResult::new(name, self.measured_iterations, duration)
    }

    pub async fn run_async<F, T, Fut>(&self, name: &str, mut f: F) -> BenchmarkResult
    where
        F: FnMut() -> Fut,
        Fut: std::future::Future<Output = T>,
    {
        for _ in 0..self.warmup_iterations {
            let _ = f().await;
        }

        let start = Instant::now();
        for _ in 0..self.measured_iterations {
            let _ = f().await;
        }
        let duration = start.elapsed();

        BenchmarkResult::new(name, self.measured_iterations, duration)
    }
}

#[derive(Debug, Clone)]
pub struct ReleaseCriteria {
    pub max_avg_latency_ms: f64,
    pub min_throughput: f64,
    pub max_memory_mb: usize,
    pub test_timeout_secs: u64,
}

impl Default for ReleaseCriteria {
    fn default() -> Self {
        Self {
            max_avg_latency_ms: 500.0,
            min_throughput: 10.0,
            max_memory_mb: 512,
            test_timeout_secs: 30,
        }
    }
}

impl ReleaseCriteria {
    pub fn check(&self, result: &BenchmarkResult) -> bool {
        result.avg_latency_ms <= self.max_avg_latency_ms
            && result.throughput >= self.min_throughput
    }

    pub fn report(&self, result: &BenchmarkResult) -> Vec<String> {
        let mut issues = Vec::new();

        if result.avg_latency_ms > self.max_avg_latency_ms {
            issues.push(format!(
                "Latency {}ms exceeds threshold {}ms",
                result.avg_latency_ms, self.max_avg_latency_ms
            ));
        }

        if result.throughput < self.min_throughput {
            issues.push(format!(
                "Throughput {} ops/s below threshold {} ops/s",
                result.throughput, self.min_throughput
            ));
        }

        issues
    }
}

pub struct ReleaseChecklist {
    pub core_tests_pass: bool,
    pub gui_builds: Vec<String>,
    pub benchmarks_pass: bool,
    pub documentation_updated: bool,
}

impl Default for ReleaseChecklist {
    fn default() -> Self {
        Self {
            core_tests_pass: false,
            gui_builds: Vec::new(),
            benchmarks_pass: false,
            documentation_updated: false,
        }
    }
}

impl ReleaseChecklist {
    pub fn is_ready(&self) -> bool {
        self.core_tests_pass
            && !self.gui_builds.is_empty()
            && self.benchmarks_pass
            && self.documentation_updated
    }

    pub fn summary(&self) -> String {
        format!(
            "Core tests: {}, GUI builds: {}, Benchmarks: {}, Docs: {}",
            if self.core_tests_pass { "PASS" } else { "FAIL" },
            self.gui_builds.len(),
            if self.benchmarks_pass { "PASS" } else { "FAIL" },
            if self.documentation_updated { "PASS" } else { "FAIL" }
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_result_calculates_correctly() {
        let result = BenchmarkResult::new("test", 100, Duration::from_millis(500));
        assert_eq!(result.avg_latency_ms, 5.0);
        assert_eq!(result.throughput, 200.0);
    }

    #[test]
    fn release_criteria_check_passes() {
        let criteria = ReleaseCriteria::default();
        let result = BenchmarkResult::new("test", 100, Duration::from_millis(100));
        assert!(criteria.check(&result));
    }

    #[test]
    fn release_criteria_check_fails_latency() {
        let criteria = ReleaseCriteria::default();
        let result = BenchmarkResult::new("test", 10, Duration::from_millis(10000));
        assert!(!criteria.check(&result));
    }

    #[test]
    fn release_checklist_is_ready() {
        let mut checklist = ReleaseChecklist::default();
        checklist.core_tests_pass = true;
        checklist.gui_builds = vec!["linux".to_string()];
        checklist.benchmarks_pass = true;
        checklist.documentation_updated = true;

        assert!(checklist.is_ready());
    }
}

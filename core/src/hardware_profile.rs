use std::process::Command;

use crate::model_profile::ModelClass;

#[derive(Debug, Clone, Copy)]
pub struct HardwareProfile {
    pub ram_gb: usize,
    pub cpu_cores: usize,
    pub gpu_count: usize,
    pub max_gpu_vram_gb: usize,
}

#[derive(Debug, Clone, Copy)]
pub struct HardwareCaps {
    pub max_parallel_streams: usize,
    pub max_context_tokens: usize,
    pub max_project_tokens: usize,
    pub max_project_top_k: usize,
    pub max_model_class: ModelClass,
}

impl HardwareProfile {
    pub fn detect() -> Self {
        let mut sys = sysinfo::System::new_all();
        sys.refresh_memory();
        sys.refresh_cpu_all();

        let ram_gb = (sys.total_memory() / 1024 / 1024 / 1024) as usize;
        let cpu_cores = sys.cpus().len().max(1);
        let (gpu_count, max_gpu_vram_gb) = detect_nvidia_gpu();

        Self {
            ram_gb,
            cpu_cores,
            gpu_count,
            max_gpu_vram_gb,
        }
    }

    pub fn derive_caps(&self) -> HardwareCaps {
        if self.gpu_count == 0 {
            if self.ram_gb <= 8 {
                return HardwareCaps {
                    max_parallel_streams: 1,
                    max_context_tokens: 3072,
                    max_project_tokens: 900,
                    max_project_top_k: 4,
                    max_model_class: ModelClass::Small,
                };
            }

            if self.ram_gb <= 16 {
                return HardwareCaps {
                    max_parallel_streams: self.cpu_cores.clamp(1, 2),
                    max_context_tokens: 4096,
                    max_project_tokens: 1200,
                    max_project_top_k: 5,
                    max_model_class: ModelClass::Small,
                };
            }

            if self.ram_gb <= 32 {
                return HardwareCaps {
                    max_parallel_streams: self.cpu_cores.clamp(1, 2),
                    max_context_tokens: 5120,
                    max_project_tokens: 1500,
                    max_project_top_k: 5,
                    max_model_class: ModelClass::Medium,
                };
            }

            return HardwareCaps {
                max_parallel_streams: self.cpu_cores.clamp(1, 3),
                max_context_tokens: 6144,
                max_project_tokens: 1800,
                max_project_top_k: 6,
                max_model_class: ModelClass::Medium,
            };
        }

        if self.gpu_count > 0 && self.max_gpu_vram_gb >= 20 {
            return HardwareCaps {
                max_parallel_streams: self.cpu_cores.clamp(2, 6),
                max_context_tokens: 16384,
                max_project_tokens: 4800,
                max_project_top_k: 14,
                max_model_class: ModelClass::Large,
            };
        }

        HardwareCaps {
            max_parallel_streams: self.cpu_cores.clamp(1, 4),
            max_context_tokens: 8192,
            max_project_tokens: 2500,
            max_project_top_k: 8,
            max_model_class: ModelClass::Medium,
        }
    }
}

fn detect_nvidia_gpu() -> (usize, usize) {
    let output = Command::new("nvidia-smi")
        .arg("--query-gpu=memory.total")
        .arg("--format=csv,noheader,nounits")
        .output();

    let Ok(out) = output else {
        return (0, 0);
    };
    if !out.status.success() {
        return (0, 0);
    }

    let text = String::from_utf8_lossy(&out.stdout);
    let mut values = Vec::new();
    for line in text.lines() {
        if let Ok(mb) = line.trim().parse::<usize>() {
            values.push(mb);
        }
    }
    if values.is_empty() {
        return (0, 0);
    }

    let max_mb = *values.iter().max().unwrap_or(&0);
    (values.len(), max_mb / 1024)
}

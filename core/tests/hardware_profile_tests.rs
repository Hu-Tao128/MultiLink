use multilink_core::{HardwareProfile, ModelClass};

#[test]
fn derive_caps_for_low_resource_machine() {
    let profile = HardwareProfile {
        ram_gb: 8,
        cpu_cores: 4,
        gpu_count: 0,
        max_gpu_vram_gb: 0,
    };

    let caps = profile.derive_caps();
    assert_eq!(caps.max_parallel_streams, 1);
    assert!(caps.max_context_tokens <= 3072);
    assert!(matches!(
        caps.max_model_class,
        ModelClass::Small | ModelClass::Tiny
    ));
}

#[test]
fn derive_caps_for_cpu_only_mid_ram_machine() {
    let profile = HardwareProfile {
        ram_gb: 24,
        cpu_cores: 8,
        gpu_count: 0,
        max_gpu_vram_gb: 0,
    };

    let caps = profile.derive_caps();
    assert!(caps.max_parallel_streams <= 2);
    assert!(caps.max_context_tokens <= 5120);
    assert!(caps.max_project_tokens <= 1500);
    assert!(caps.max_project_top_k <= 5);
    assert!(matches!(
        caps.max_model_class,
        ModelClass::Small | ModelClass::Medium
    ));
}

#[test]
fn derive_caps_for_gpu_machine() {
    let profile = HardwareProfile {
        ram_gb: 64,
        cpu_cores: 16,
        gpu_count: 2,
        max_gpu_vram_gb: 24,
    };

    let caps = profile.derive_caps();
    assert!(caps.max_parallel_streams >= 2);
    assert!(caps.max_context_tokens >= 8192);
    assert_eq!(caps.max_model_class, ModelClass::Large);
}

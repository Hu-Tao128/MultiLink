use std::process::Command;

pub struct SystemService;

pub struct OllamaInstallPlan {
    pub command: String,
    pub requires_sudo: bool,
}

#[derive(Debug, thiserror::Error)]
pub enum SystemError {
    #[error("command execution failed: {0}")]
    Command(String),
    #[error("user consent required for privileged command")]
    ConsentRequired,
    #[error("ollama is not installed")]
    OllamaNotInstalled,
}

impl SystemService {
    pub fn detect_ollama_installed() -> bool {
        Command::new("ollama")
            .arg("--version")
            .output()
            .map(|out| out.status.success())
            .unwrap_or(false)
    }

    pub fn linux_install_plan() -> OllamaInstallPlan {
        OllamaInstallPlan {
            command: "curl -fsSL https://ollama.com/install.sh | sh".to_string(),
            requires_sudo: true,
        }
    }

    pub fn run_linux_install(consent_for_privileged: bool) -> Result<(), SystemError> {
        let plan = Self::linux_install_plan();
        if plan.requires_sudo && !consent_for_privileged {
            return Err(SystemError::ConsentRequired);
        }

        let output = Command::new("sh")
            .arg("-c")
            .arg(plan.command)
            .output()
            .map_err(|e| SystemError::Command(e.to_string()))?;

        if !output.status.success() {
            return Err(SystemError::Command(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        Self::verify_ollama_installation()
    }

    pub fn verify_ollama_installation() -> Result<(), SystemError> {
        if Self::detect_ollama_installed() {
            Ok(())
        } else {
            Err(SystemError::OllamaNotInstalled)
        }
    }
}

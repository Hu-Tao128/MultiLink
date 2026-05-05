use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

const BACKUP_DIR: &str = ".multilink/backups";

pub fn save_backup(project_root: &Path, relative_path: &str) -> Result<PathBuf, String> {
    let full_path = project_root.join(relative_path);
    if !full_path.exists() {
        return Ok(PathBuf::new());
    }

    let content = std::fs::read_to_string(&full_path)
        .map_err(|e| format!("Failed to read original file for backup: {}", e))?;

    let backup_dir = project_root.join(BACKUP_DIR);
    std::fs::create_dir_all(&backup_dir)
        .map_err(|e| format!("Failed to create backup directory: {}", e))?;

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_millis();

    let safe_name = relative_path.replace('/', "__");
    let backup_name = format!("{}_{}.bak", safe_name, timestamp);
    let backup_path = backup_dir.join(&backup_name);

    std::fs::write(&backup_path, &content)
        .map_err(|e| format!("Failed to write backup: {}", e))?;

    Ok(backup_path)
}

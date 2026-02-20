use std::path::PathBuf;

pub struct NpmModelManager {
    pub cache_dir: PathBuf,
}

impl NpmModelManager {
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }
}

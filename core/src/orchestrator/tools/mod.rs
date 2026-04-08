pub mod filesystem;
pub mod system;

use std::collections::HashMap;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ToolInput {
    pub path: Option<String>,
    pub pattern: Option<String>,
    pub args: Option<HashMap<String, serde_json::Value>>,
}
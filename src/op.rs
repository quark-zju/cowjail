use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Operation {
    WriteFile { path: PathBuf, data: Vec<u8> },
}

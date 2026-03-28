use std::path::PathBuf;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum FileState {
    Deleted,
    Regular { data: Vec<u8>, mode: u32 },
    Symlink(PathBuf),
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Operation {
    WriteFile { path: PathBuf, state: FileState },
    CreateDir { path: PathBuf, mode: u32 },
    RemoveDir { path: PathBuf },
    Rename { from: PathBuf, to: PathBuf },
    Truncate { path: PathBuf, size: u64 },
}

use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::cli::RunCommand;

/// If `argv[0]` basename is not "leash" (e.g. invoked via a symlink like
/// `/usr/local/bin/codex` -> `leash`), search PATH for the real binary and
/// run it under leash's sandbox.
///
/// Returns `Ok(Some(exit_code))` when the symlink case was handled,
/// `Ok(None)` when `argv[0]` is "leash" and normal CLI parsing should
/// proceed.
pub fn try_handle_arg0(args: &[OsString]) -> Result<Option<i32>> {
    let argv0 = match args.first() {
        Some(a) => a,
        None => return Ok(None),
    };

    let basename = std::path::Path::new(argv0)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("leash");

    if basename == "leash" {
        return Ok(None);
    }

    let found =
        search_path_for(basename).with_context(|| format!("{basename}: command not found"))?;

    let remaining: Vec<OsString> = args[1..].to_vec();
    crate::cmd_run::run_command(RunCommand {
        verbose: false,
        program: found.into(),
        args: remaining,
    })
    .map(Some)
}

/// Search PATH for a binary named `name`, skipping the current executable
/// (to avoid matching a symlink back to ourselves).
fn search_path_for(name: &str) -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok()?;
    let current = current_exe
        .canonicalize()
        .unwrap_or_else(|_| current_exe.clone());
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = std::path::Path::new(dir).join(name);
        if candidate.is_file() {
            match candidate.canonicalize() {
                Ok(c) if c == current => continue,
                Ok(c) => return Some(c),
                Err(_) => return Some(candidate),
            }
        }
    }
    None
}

use std::path::{Path, PathBuf};

/// Resolve a bare executable name from PATH, returning the first file match.
pub fn find_in_path(name: &str) -> Option<PathBuf> {
    find_in_path_with(name, |_| false)
}

/// Resolve a bare executable name from PATH while skipping candidates that
/// point to the currently-running executable.
pub fn find_in_path_excluding_current_exe(name: &str) -> Option<PathBuf> {
    let current_exe = std::env::current_exe().ok();
    find_in_path_with(name, |candidate| {
        is_same_executable(candidate, current_exe.as_deref())
    })
}

fn find_in_path_with(name: &str, skip: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let path_var = std::env::var("PATH").ok()?;
    for dir in path_var.split(':') {
        let candidate = Path::new(dir).join(name);
        if candidate.is_file() && !skip(&candidate) {
            return Some(candidate.canonicalize().unwrap_or(candidate));
        }
    }
    None
}

fn is_same_executable(candidate: &Path, current_exe: Option<&Path>) -> bool {
    let Some(current_exe) = current_exe else {
        return false;
    };
    let current = current_exe
        .canonicalize()
        .unwrap_or_else(|_| current_exe.to_path_buf());
    let candidate = candidate
        .canonicalize()
        .unwrap_or_else(|_| candidate.to_path_buf());
    candidate == current
}

use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

static CURRENT_EXE_CANONICAL: LazyLock<Option<PathBuf>> = LazyLock::new(|| {
    let current_exe = std::env::current_exe().ok()?;
    Some(current_exe.canonicalize().unwrap_or(current_exe))
});

/// Resolve a bare executable name from PATH, returning the first file match.
pub fn find_in_path(name: &OsStr) -> Option<PathBuf> {
    find_in_path_with(name, |_| false)
}

/// Resolve a bare executable name from PATH while skipping candidates that
/// point to the currently-running executable.
pub fn find_in_path_excluding_current_exe(name: &OsStr) -> Option<PathBuf> {
    find_in_path_with(name, |candidate| {
        is_same_executable(candidate, CURRENT_EXE_CANONICAL.as_deref())
    })
}

fn find_in_path_with(name: &OsStr, skip: impl Fn(&Path) -> bool) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(name);
        if candidate.is_file() && !skip(&candidate) {
            return Some(candidate.canonicalize().unwrap_or(candidate));
        }
    }
    None
}

fn is_same_executable(candidate: &Path, current_exe_canonical: Option<&Path>) -> bool {
    let Some(current_exe_canonical) = current_exe_canonical else {
        return false;
    };

    if candidate == current_exe_canonical {
        return true;
    }

    let Ok(candidate_canonical) = candidate.canonicalize() else {
        return false;
    };
    candidate_canonical == current_exe_canonical
}

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fs_err as fs;

const RUNTIME_DIR_NAME: &str = "leash2";
const MOUNT_DIR_NAME: &str = "mount";

pub fn ensure_global_mountpoint() -> Result<PathBuf> {
    let runtime_dir = xdg_runtime_dir_from_env()?;
    ensure_global_mountpoint_under(&runtime_dir)
}

pub fn ensure_global_mountpoint_under(runtime_dir: &Path) -> Result<PathBuf> {
    ensure_private_dir(runtime_dir)?;
    let leash_dir = runtime_dir.join(RUNTIME_DIR_NAME);
    ensure_private_dir(&leash_dir)?;
    let mount_dir = leash_dir.join(MOUNT_DIR_NAME);
    ensure_private_dir(&mount_dir)?;
    Ok(mount_dir)
}

fn xdg_runtime_dir_from_env() -> Result<PathBuf> {
    let Some(path) = std::env::var_os("XDG_RUNTIME_DIR") else {
        bail!("XDG_RUNTIME_DIR is not set");
    };
    Ok(PathBuf::from(path))
}

fn ensure_private_dir(path: &Path) -> Result<()> {
    if path.exists() {
        if !path.is_dir() {
            bail!("{} exists but is not a directory", path.display());
        }
    } else {
        fs::create_dir_all(path)
            .with_context(|| format!("failed to create {}", path.display()))?;
    }
    fs::set_permissions(path, std::fs::Permissions::from_mode(0o700))
        .with_context(|| format!("failed to chmod 0700 {}", path.display()))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::MetadataExt;
    use tempfile::tempdir;

    #[test]
    fn ensure_global_mountpoint_under_creates_private_directories() {
        let tempdir = tempdir().expect("tempdir");
        let runtime_dir = tempdir.path().join("xdg-runtime");

        let mount_dir = ensure_global_mountpoint_under(&runtime_dir).expect("mountpoint");

        assert_eq!(mount_dir, runtime_dir.join("leash2/mount"));
        assert!(mount_dir.is_dir());
        assert_eq!(
            fs::metadata(&runtime_dir).expect("runtime metadata").mode() & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(runtime_dir.join("leash2"))
                .expect("leash2 metadata")
                .mode()
                & 0o777,
            0o700
        );
        assert_eq!(
            fs::metadata(&mount_dir).expect("mount metadata").mode() & 0o777,
            0o700
        );
    }

    #[test]
    fn ensure_global_mountpoint_under_rejects_non_directory_runtime_path() {
        let tempdir = tempdir().expect("tempdir");
        let runtime_path = tempdir.path().join("xdg-runtime");
        fs::write(&runtime_path, b"file").expect("write runtime file");

        let err = ensure_global_mountpoint_under(&runtime_path).expect_err("must fail");

        assert!(err.to_string().contains("not a directory"), "{err:#}");
    }
}

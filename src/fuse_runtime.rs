use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use fs_err as fs;

const RUNTIME_DIR_NAME: &str = "leash2";
const MOUNT_DIR_NAME: &str = "mount";
const MOUNTINFO_PATH: &str = "/proc/self/mountinfo";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MountState {
    Unmounted,
    Fuse { fs_type: String },
    Other { fs_type: String },
}

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

pub fn read_global_mount_state(mountpoint: &Path) -> Result<MountState> {
    read_mount_state_from(mountpoint, Path::new(MOUNTINFO_PATH))
}

fn read_mount_state_from(mountpoint: &Path, mountinfo: &Path) -> Result<MountState> {
    let content = fs::read_to_string(mountinfo)
        .with_context(|| format!("failed to read {}", mountinfo.display()))?;
    for line in content.lines() {
        let Some((parsed_mountpoint, fs_type)) = parse_mountinfo_line(line)? else {
            continue;
        };
        if parsed_mountpoint != mountpoint {
            continue;
        }
        if fs_type.starts_with("fuse") {
            return Ok(MountState::Fuse { fs_type });
        }
        return Ok(MountState::Other { fs_type });
    }
    Ok(MountState::Unmounted)
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

fn parse_mountinfo_line(line: &str) -> Result<Option<(PathBuf, String)>> {
    if line.trim().is_empty() {
        return Ok(None);
    }

    let fields: Vec<&str> = line.split_whitespace().collect();
    let Some(sep) = fields.iter().position(|field| *field == "-") else {
        bail!("mountinfo line missing separator: {line}");
    };
    if sep < 5 || fields.len() <= sep + 1 {
        bail!("mountinfo line is malformed: {line}");
    }
    Ok(Some((
        PathBuf::from(unescape_mount_field(fields[4])),
        fields[sep + 1].to_owned(),
    )))
}

fn unescape_mount_field(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut idx = 0usize;
    while idx < bytes.len() {
        if bytes[idx] == b'\\'
            && idx + 3 < bytes.len()
            && bytes[idx + 1..idx + 4]
                .iter()
                .all(|byte| matches!(byte, b'0'..=b'7'))
            && let Ok(value) = u8::from_str_radix(&input[idx + 1..idx + 4], 8)
        {
            out.push(value as char);
            idx += 4;
            continue;
        }
        out.push(bytes[idx] as char);
        idx += 1;
    }
    out
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

    #[test]
    fn read_mount_state_from_detects_matching_fuse_mount() {
        let tempdir = tempdir().expect("tempdir");
        let mountinfo = tempdir.path().join("mountinfo");
        fs::write(
            &mountinfo,
            "41 29 0:45 / /run/user/1000/leash2/mount rw,nosuid,nodev - fuse.leash leash rw\n",
        )
        .expect("write mountinfo");

        assert_eq!(
            read_mount_state_from(Path::new("/run/user/1000/leash2/mount"), &mountinfo)
                .expect("read mount state"),
            MountState::Fuse {
                fs_type: "fuse.leash".to_owned(),
            }
        );
    }

    #[test]
    fn read_mount_state_from_reports_non_fuse_mount_and_unescapes_spaces() {
        let tempdir = tempdir().expect("tempdir");
        let mountinfo = tempdir.path().join("mountinfo");
        fs::write(
            &mountinfo,
            "41 29 0:45 / /tmp/My\\040Mount rw,nosuid,nodev - tmpfs tmpfs rw\n",
        )
        .expect("write mountinfo");

        assert_eq!(
            read_mount_state_from(Path::new("/tmp/My Mount"), &mountinfo)
                .expect("read mount state"),
            MountState::Other {
                fs_type: "tmpfs".to_owned(),
            }
        );
        assert_eq!(
            read_mount_state_from(Path::new("/tmp/other"), &mountinfo).expect("read mount state"),
            MountState::Unmounted
        );
    }
}

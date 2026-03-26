use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::process::Command as ProcessCommand;

use anyhow::{Context, Result, bail};
use fs_err as fs;

use crate::cli::LowLevelSuidCommand;
use crate::privileges;
use crate::vlog;

const SUDO_REEXEC_ENV: &str = "COWJAIL_SUID_REEXEC";

pub(crate) fn suid_command(cmd: LowLevelSuidCommand) -> Result<()> {
    let exe = std::env::current_exe().context("failed to resolve current executable path")?;
    let meta = fs::symlink_metadata(&exe)
        .with_context(|| format!("failed to stat current executable {}", exe.display()))?;
    if is_suid_root(&meta) {
        vlog(
            cmd.verbose,
            format!("_suid: {} is already setuid-root, skipping", exe.display()),
        );
        return Ok(());
    }

    let mut euid = unsafe { libc::geteuid() };
    let ruid = unsafe { libc::getuid() };
    if euid != 0 && ruid == 0 {
        // `sudo` may invoke a setuid binary that changed euid away from 0.
        // Real uid 0 can still restore effective uid 0 for privileged file ops.
        if unsafe { libc::seteuid(0) } != 0 {
            let err = std::io::Error::last_os_error();
            return Err(anyhow::anyhow!("failed to restore effective uid 0: {err}"));
        }
        euid = unsafe { libc::geteuid() };
    }

    if euid != 0 {
        if std::env::var_os(SUDO_REEXEC_ENV).is_some() {
            bail!("_suid could not obtain root privileges even after sudo re-exec");
        }
        let mut sudo = ProcessCommand::new("sudo");
        sudo.env(SUDO_REEXEC_ENV, "1").arg(&exe).arg("_suid");
        if cmd.verbose {
            sudo.arg("--verbose");
        }
        vlog(
            cmd.verbose,
            format!("_suid: reinvoking via sudo for {}", exe.display()),
        );
        let status = sudo
            .status()
            .context("failed to start sudo for _suid self-reexec")?;
        if !status.success() {
            bail!("sudo _suid failed with status {status}");
        }

        let meta = fs::symlink_metadata(&exe).with_context(|| {
            format!("failed to stat current executable after sudo {}", exe.display())
        })?;
        if is_suid_root(&meta) {
            return Ok(());
        }
        bail!(
            "_suid completed via sudo but binary is still not setuid-root: {}",
            exe.display()
        );
    }

    if !privileges::in_initial_user_namespace()? {
        bail!(
            "_suid needs root in the initial user namespace; current root is namespaced and cannot provision required mount capabilities"
        );
    }

    apply_setuid_root(&exe)?;
    let meta = fs::symlink_metadata(&exe)
        .with_context(|| format!("failed to stat executable after _suid {}", exe.display()))?;
    if !is_suid_root(&meta) {
        bail!(
            "_suid attempted updates but binary is still not setuid-root: {}",
            exe.display()
        );
    }

    vlog(
        cmd.verbose,
        format!("_suid: setuid-root ready for {}", exe.display()),
    );
    Ok(())
}

fn apply_setuid_root(path: &std::path::Path) -> Result<()> {
    let path_c = CString::new(path.as_os_str().as_bytes())
        .context("executable path contains interior NUL byte")?;

    if unsafe { libc::chown(path_c.as_ptr(), 0, 0) } != 0 {
        let err = std::io::Error::last_os_error();
        return Err(anyhow::anyhow!(
            "chown(root:root) failed for {}: {}",
            path.display(),
            err
        ));
    }

    let meta = fs::symlink_metadata(path)
        .with_context(|| format!("failed to stat executable {}", path.display()))?;
    let current_mode = meta.permissions().mode();
    let target_mode = current_mode | libc::S_ISUID as u32;
    if unsafe { libc::chmod(path_c.as_ptr(), target_mode as libc::mode_t) } != 0 {
        let err = std::io::Error::last_os_error();
        return Err(anyhow::anyhow!(
            "chmod(u+s) failed for {}: {}",
            path.display(),
            err
        ));
    }

    Ok(())
}

fn is_suid_root(meta: &std::fs::Metadata) -> bool {
    meta.uid() == 0 && (meta.mode() & libc::S_ISUID as u32) != 0
}

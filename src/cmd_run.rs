use anyhow::{Context, Result, bail};
use std::path::Path;
use std::process::Command as ProcessCommand;
use std::process::Stdio;
use std::time::Duration;

use crate::cli::RunCommand;
use crate::jail;
use crate::mount_plan;
use crate::ns_runtime;
use crate::privileges;
use crate::run_env;
use crate::run_with_log;

pub(crate) fn run_command(run: RunCommand) -> Result<i32> {
    privileges::require_root_euid("leash run")?;
    run_env::set_process_name(c"leash-run")?;

    let cwd = run_with_log(jail::current_pwd, || {
        "resolve current working directory".to_string()
    })?;
    let resolved = run_with_log(
        || {
            jail::resolve(
                None,
                run.profile.as_deref(),
                jail::ResolveMode::EnsureExists,
            )
        },
        || "resolve run jail".to_string(),
    )?;
    let runtime = run_with_log(
        || ns_runtime::ensure_runtime_for_exec(&resolved.paths),
        || "ensure runtime".to_string(),
    )?;
    let mount_plan = run_with_log(
        || mount_plan::build_mount_plan(&resolved.normalized_profile),
        || "build run mount plan".to_string(),
    )?;
    crate::vlog!(
        "run: runtime={} state_before={:?} state_after={:?} rebuilt={}",
        runtime.ensured.paths.runtime_dir.display(),
        runtime.ensured.state_before,
        runtime.ensured.state_after,
        runtime.ensured.rebuilt
    );
    ensure_fuse_server(
        &resolved.paths,
        &runtime.ensured.paths,
        &resolved.paths.profile_path,
        run.verbose,
    )?;

    crate::vlog!(
        "run: preparing child pivot_root into {} then chdir to {}",
        runtime.ensured.paths.mount_dir.display(),
        cwd.display()
    );
    run_with_log(run_env::setup_run_namespaces, || {
        "unshare run namespaces".to_string()
    })?;
    let status = run_with_log(
        || {
            run_env::run_child_in_jail(
                &run,
                &runtime.ensured.paths.mount_dir,
                &cwd,
                mount_plan.clone(),
            )
        },
        || format!("execute jailed command {:?}", run.program),
    );

    let status = status?;
    Ok(exit_code_from_status(status))
}

fn ensure_fuse_server(
    jail_paths: &crate::jail::JailPaths,
    runtime_paths: &ns_runtime::NsRuntimePaths,
    profile_path: &Path,
    verbose: bool,
) -> Result<()> {
    let _lock = ns_runtime::open_lock(jail_paths)?;
    if let Some(pid) = ns_runtime::read_fuse_pid(runtime_paths)?
        && ns_runtime::process_has_mount(pid, &runtime_paths.mount_dir)?
    {
        crate::vlog!(
            "run: reusing fuse server pid={} mount={}",
            pid,
            runtime_paths.mount_dir.display()
        );
        return Ok(());
    }

    crate::vlog!(
        "run: starting fuse server for mount {}",
        runtime_paths.mount_dir.display()
    );
    run_with_log(
        || ns_runtime::cleanup_before_fuse_start(runtime_paths),
        || {
            format!(
                "cleanup stale fuse runtime before start at {}",
                runtime_paths.mount_dir.display()
            )
        },
    )?;
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let mut cmd = ProcessCommand::new(exe);
    let fuse_log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&runtime_paths.fuse_log_path)
        .with_context(|| {
            format!(
                "failed to open fuse log file {}",
                runtime_paths.fuse_log_path.display()
            )
        })?;
    let fuse_log_err = fuse_log.try_clone().with_context(|| {
        format!(
            "failed to clone fuse log handle {}",
            runtime_paths.fuse_log_path.display()
        )
    })?;
    cmd.arg("_fuse")
        .arg("--profile")
        .arg(profile_path)
        .arg("--mountpoint")
        .arg(&runtime_paths.mount_dir)
        .arg("--pid-path")
        .arg(&runtime_paths.fuse_pid_path)
        // Keep _fuse detached from caller stdio while preserving diagnostics in
        // per-runtime logs.
        .stdin(Stdio::null())
        .stdout(Stdio::from(fuse_log))
        .stderr(Stdio::from(fuse_log_err));
    if verbose {
        cmd.arg("-v");
    }

    let child = cmd
        .spawn()
        .context("failed to spawn _fuse server process")?;
    let pid = child.id();
    let ok =
        ns_runtime::wait_for_process_mount(pid, &runtime_paths.mount_dir, Duration::from_secs(5))?;
    if !ok {
        bail!(
            "fuse server pid={} did not mount {} within timeout",
            pid,
            runtime_paths.mount_dir.display()
        );
    }
    Ok(())
}

fn exit_code_from_status(status: std::process::ExitStatus) -> i32 {
    if let Some(code) = status.code() {
        return code;
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        if let Some(sig) = status.signal() {
            return 128 + sig;
        }
    }
    1
}

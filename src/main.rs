mod cli;
mod profile;

use anyhow::{Context, Result, bail};
use cli::{Command, FlushCommand, MountCommand, RunCommand};
use std::path::Path;

fn main() {
    if let Err(err) = try_main() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    match cli::parse_env()? {
        Command::Run(run) => run_command(run),
        Command::Mount(mount) => mount_command(mount),
        Command::Flush(flush) => flush_command(flush),
    }
}

fn run_command(_run: RunCommand) -> Result<()> {
    let euid = unsafe { libc::geteuid() };
    if euid != 0 {
        bail!(
            "cowjail run requires root euid (current euid={euid}).\n\
             Example setuid setup:\n\
             sudo chown root:root $(command -v cowjail)\n\
             sudo chmod u+s $(command -v cowjail)"
        );
    }

    let _profile = load_profile(Path::new(&_run.profile))?;

    bail!("run is not implemented yet")
}

fn mount_command(mount: MountCommand) -> Result<()> {
    let _profile = load_profile(Path::new(&mount.profile))?;
    bail!("mount is not implemented yet")
}

fn flush_command(_flush: FlushCommand) -> Result<()> {
    bail!("flush is not implemented yet")
}

fn load_profile(profile_path: &Path) -> Result<profile::Profile> {
    let source = fs_err::read_to_string(profile_path)
        .with_context(|| format!("failed to read profile file: {}", profile_path.display()))?;
    let cwd = std::env::current_dir().context("failed to get current directory")?;
    profile::Profile::parse(&source, &cwd)
        .with_context(|| format!("failed to parse profile file: {}", profile_path.display()))
}

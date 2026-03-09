mod cli;
mod profile;

use anyhow::{Result, bail};
use cli::{Command, FlushCommand, MountCommand, RunCommand};

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

    bail!("run is not implemented yet")
}

fn mount_command(_mount: MountCommand) -> Result<()> {
    bail!("mount is not implemented yet")
}

fn flush_command(_flush: FlushCommand) -> Result<()> {
    bail!("flush is not implemented yet")
}

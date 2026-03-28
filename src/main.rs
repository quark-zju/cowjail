mod cli;
mod cmd_help;
mod cmd_flush;
mod cmd_fuse;
mod cmd_jail;
mod cmd_mount;
mod cmd_run;
mod cmd_show;
mod cmd_suid;
mod cowfs;
mod jail;
mod ns_runtime;
mod op;
mod privileges;
mod profile;
mod profile_loader;
mod record;

use anyhow::{Context, Result};
use cli::Command;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicBool, Ordering};

static VERBOSE_LOG: AtomicBool = AtomicBool::new(false);

pub(crate) fn is_verbose() -> bool {
    VERBOSE_LOG.load(Ordering::Relaxed)
}

macro_rules! vlog {
    ($($arg:tt)*) => {{
        if $crate::is_verbose() {
            eprintln!($($arg)*);
        }
    }};
}
pub(crate) use vlog;

fn main() {
    match try_main() {
        Ok(code) => std::process::exit(code),
        Err(err) => {
            eprintln!("error: {err:#}");
            std::process::exit(1);
        }
    }
}

fn try_main() -> Result<i32> {
    match cli::parse_env()? {
        Command::Help { topic, verbose } => {
            set_verbose(verbose);
            drop_privileges_for_unprivileged_command("help")?;
            cmd_help::print_help(topic, verbose);
            Ok(0)
        }
        Command::Add(add) => {
            set_verbose(false);
            drop_privileges_for_unprivileged_command("add")?;
            cmd_jail::add_command(add).context("add subcommand failed")?;
            Ok(0)
        }
        Command::List(list) => {
            set_verbose(false);
            drop_privileges_for_unprivileged_command("list")?;
            cmd_jail::list_command(list).context("list subcommand failed")?;
            Ok(0)
        }
        Command::Show(show) => {
            set_verbose(show.verbose);
            drop_privileges_for_unprivileged_command("show")?;
            cmd_show::show_command(show).context("show subcommand failed")?;
            Ok(0)
        }
        Command::Rm(rm) => {
            set_verbose(rm.verbose);
            cmd_jail::rm_command(rm).context("rm subcommand failed")?;
            Ok(0)
        }
        Command::Run(run) => {
            set_verbose(run.verbose);
            cmd_run::run_command(run).context("run subcommand failed")
        }
        Command::LowLevelMount(mount) => {
            set_verbose(mount.verbose);
            drop_privileges_for_unprivileged_command("_mount")?;
            cmd_mount::mount_command(mount).context("_mount subcommand failed")?;
            Ok(0)
        }
        Command::Flush(flush) => {
            set_verbose(flush.verbose);
            drop_privileges_for_unprivileged_command("flush")?;
            cmd_flush::flush_command(flush).context("flush subcommand failed")?;
            Ok(0)
        }
        Command::LowLevelFlush(flush) => {
            set_verbose(flush.verbose);
            drop_privileges_for_unprivileged_command("_flush")?;
            cmd_flush::low_level_flush_command(flush).context("_flush subcommand failed")?;
            Ok(0)
        }
        Command::LowLevelFuse(fuse) => {
            set_verbose(fuse.verbose);
            cmd_fuse::fuse_command(fuse).context("_fuse subcommand failed")?;
            Ok(0)
        }
        Command::LowLevelSuid(suid) => {
            set_verbose(suid.verbose);
            cmd_suid::suid_command(suid).context("_suid subcommand failed")?;
            Ok(0)
        }
    }
}

fn drop_privileges_for_unprivileged_command(command: &str) -> Result<()> {
    run_with_log(
        privileges::drop_root_euid_if_needed,
        || format!("drop elevated privileges before '{command}'"),
    )?;
    Ok(())
}

pub(crate) fn set_verbose(enabled: bool) {
    VERBOSE_LOG.store(enabled, Ordering::Relaxed);
}

pub(crate) fn run_with_log<T, F, D>(func: F, desc: D) -> Result<T>
where
    F: FnOnce() -> Result<T>,
    D: Fn() -> String,
{
    let verbose = VERBOSE_LOG.load(Ordering::Relaxed);
    let label = LazyLock::new(desc);
    let get_label = || label.as_str();

    if verbose {
        eprintln!("begin {}", get_label());
    }
    match func() {
        Ok(v) => {
            if verbose {
                eprintln!("ok {}", get_label());
            }
            Ok(v)
        }
        Err(err) => {
            if verbose {
                eprintln!("err {}: {err:#}", get_label());
            }
            Err(err).with_context(|| label.to_string())
        }
    }
}

#[cfg(test)]
mod tests;

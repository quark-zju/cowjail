use std::convert::Infallible;
use std::ffi::OsString;
use std::path::PathBuf;

use anyhow::{Context, Result, bail};
use pico_args::Arguments;

pub const DEFAULT_PROFILE: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Run(RunCommand),
    Mount(MountCommand),
    Flush(FlushCommand),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunCommand {
    pub profile: String,
    pub record: Option<PathBuf>,
    pub program: OsString,
    pub args: Vec<OsString>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountCommand {
    pub profile: String,
    pub record: PathBuf,
    pub path: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FlushCommand {
    pub record: Option<PathBuf>,
    pub dry_run: bool,
}

pub fn parse_from<I>(argv: I) -> Result<Command>
where
    I: IntoIterator<Item = OsString>,
{
    let mut args = Arguments::from_vec(argv.into_iter().collect());
    let subcmd = args
        .subcommand()?
        .ok_or_else(|| anyhow::anyhow!("missing subcommand (expected: run, mount, flush)"))?;

    let command = match subcmd.as_str() {
        "run" => parse_run(args)?,
        "mount" => parse_mount(args)?,
        "flush" => parse_flush(args)?,
        other => bail!("unknown subcommand: {other}"),
    };

    Ok(command)
}

pub fn parse_env() -> Result<Command> {
    let argv: Vec<OsString> = std::env::args_os().skip(1).collect();
    parse_from(argv)
}

fn parse_run(mut args: Arguments) -> Result<Command> {
    let profile = args
        .opt_value_from_str("--profile")?
        .unwrap_or_else(|| DEFAULT_PROFILE.to_string());
    let record = args.opt_value_from_os_str("--record", parse_pathbuf)?;

    let mut trailing = args.finish();
    if trailing.is_empty() {
        bail!("run requires a command to execute");
    }

    let program = trailing.remove(0);
    Ok(Command::Run(RunCommand {
        profile,
        record,
        program,
        args: trailing,
    }))
}

fn parse_mount(mut args: Arguments) -> Result<Command> {
    let profile = args
        .value_from_str("--profile")
        .context("mount requires --profile <profile>")?;
    let record = args
        .value_from_os_str("--record", parse_pathbuf)
        .context("mount requires --record <record_path>")?;
    let path = args
        .free_from_os_str(parse_pathbuf)
        .context("mount requires <path>")?;

    let extra = args.finish();
    if !extra.is_empty() {
        bail!("mount got unexpected trailing arguments");
    }

    Ok(Command::Mount(MountCommand {
        profile,
        record,
        path,
    }))
}

fn parse_flush(mut args: Arguments) -> Result<Command> {
    let dry_run = args.contains("--dry-run");
    let record = args.opt_value_from_os_str("--record", parse_pathbuf)?;

    let extra = args.finish();
    if !extra.is_empty() {
        bail!("flush got unexpected trailing arguments");
    }

    Ok(Command::Flush(FlushCommand { record, dry_run }))
}

fn parse_pathbuf(raw: &std::ffi::OsStr) -> Result<PathBuf, Infallible> {
    Ok(PathBuf::from(raw))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn os(args: &[&str]) -> Vec<OsString> {
        args.iter().map(|s| OsString::from(*s)).collect()
    }

    #[test]
    fn parse_run_defaults_profile() {
        let cmd = parse_from(os(&["run", "echo", "hi"]))
            .expect("run command should parse with default profile");
        let run = match cmd {
            Command::Run(run) => run,
            other => panic!("expected run, got {other:?}"),
        };
        assert_eq!(run.profile, DEFAULT_PROFILE);
        assert_eq!(run.program, OsString::from("echo"));
        assert_eq!(run.args, vec![OsString::from("hi")]);
    }

    #[test]
    fn parse_mount_requires_all_flags() {
        let err = parse_from(os(&["mount", "./mnt"]))
            .expect_err("mount without required flags should fail");
        assert!(err.to_string().contains("mount requires --profile"));
    }

    #[test]
    fn parse_flush_dry_run() {
        let cmd = parse_from(os(&["flush", "--dry-run"])).expect("flush should parse with dry-run");
        let flush = match cmd {
            Command::Flush(flush) => flush,
            other => panic!("expected flush, got {other:?}"),
        };
        assert!(flush.dry_run);
        assert!(flush.record.is_none());
    }
}

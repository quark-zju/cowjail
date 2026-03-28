use anyhow::{Context, Result};
use globset::Glob;
use std::thread;
use std::time::Duration;

use crate::cli::{AddCommand, ListCommand, RmCommand};
use crate::jail;
use crate::privileges;
use crate::record;
use crate::run_with_log;

const DIRTY_RM_DELAY_SECS: u64 = 10;

pub(crate) fn add_command(add: AddCommand) -> Result<()> {
    if let Some(name) = add.name.as_deref() {
        run_with_log(
            || jail::validate_explicit_name(name),
            || format!("validate jail name '{name}'"),
        )
        .with_context(|| format!("invalid jail name '{name}'"))?;
    }
    run_with_log(
        || {
            jail::resolve(
                add.name.as_deref(),
                add.profile.as_deref(),
                jail::ResolveMode::EnsureExists,
            )
        },
        || "create or reuse explicit jail".to_string(),
    )?;
    Ok(())
}

pub(crate) fn list_command(_list: ListCommand) -> Result<()> {
    let names = run_with_log(jail::list_named_jails, || "list named jails".to_string())?;
    for name in names {
        println!("{}", name.to_string_lossy());
    }
    Ok(())
}

pub(crate) fn rm_command(rm: RmCommand) -> Result<()> {
    privileges::require_root_euid("cowjail rm")?;
    if let Some(profile) = rm.profile.as_deref() {
        return remove_one_jail(None, Some(profile), rm.allow_dirty);
    }

    let Some(name_selector) = rm.name.as_deref() else {
        unreachable!("cli parser ensures rm has at least one selector");
    };

    if !contains_glob_syntax(name_selector) {
        return remove_one_jail(Some(name_selector), None, rm.allow_dirty);
    }

    let matched_names = run_with_log(
        || expand_name_glob_selector(name_selector),
        || format!("expand jail name glob '{name_selector}'"),
    )?;
    if matched_names.is_empty() {
        anyhow::bail!("rm name glob matched no jails: {name_selector}");
    }
    for name in matched_names {
        remove_one_jail(Some(&name), None, rm.allow_dirty)?;
    }
    Ok(())
}

fn remove_one_jail(name: Option<&str>, profile: Option<&str>, allow_dirty: bool) -> Result<()> {
    let resolved = run_with_log(
        || jail::resolve(name, profile, jail::ResolveMode::MustExist),
        || match (name, profile) {
            (Some(name), None) => format!("resolve jail '{name}'"),
            (None, Some(profile)) => format!("resolve jail by profile '{profile}'"),
            _ => "resolve jail".to_string(),
        },
    )?;
    if !allow_dirty {
        protect_dirty_remove(
            &resolved.name,
            &resolved.paths.record_path,
            DIRTY_RM_DELAY_SECS,
        )?;
    }
    run_with_log(
        || jail::remove_jail(&resolved.paths),
        || format!("remove jail runtime/state artifacts '{}'", resolved.name),
    )
}

fn protect_dirty_remove(name: &str, record_path: &std::path::Path, delay_secs: u64) -> Result<()> {
    let frames = run_with_log(
        || record::read_frames_best_effort(record_path),
        || format!("read record {}", record_path.display()),
    )?;
    let pending = pending_unflushed_write_count(&frames);
    if pending == 0 {
        return Ok(());
    }
    eprintln!(
        "warning: jail '{}' has {} pending unflushed write op(s).",
        name, pending
    );
    eprintln!(
        "rm will continue in {} seconds. Press Ctrl+C now to abort this removal.",
        delay_secs
    );
    thread::sleep(Duration::from_secs(delay_secs));
    Ok(())
}

fn pending_unflushed_write_count(frames: &[record::Frame]) -> usize {
    frames
        .iter()
        .filter(|frame| frame.tag == record::TAG_WRITE_OP && !frame.flushed)
        .count()
}

fn expand_name_glob_selector(selector: &str) -> Result<Vec<String>> {
    let matcher = Glob::new(selector)
        .with_context(|| format!("invalid rm name glob pattern '{selector}'"))?
        .compile_matcher();
    let names = jail::list_named_jails()?;
    let mut matched = Vec::new();
    for name in names {
        let Some(name) = name.to_str() else {
            continue;
        };
        if matcher.is_match(name) {
            matched.push(name.to_string());
        }
    }
    Ok(matched)
}

fn contains_glob_syntax(value: &str) -> bool {
    value.contains('*') || value.contains('?') || value.contains('[')
}

#[cfg(test)]
mod tests {
    use super::{contains_glob_syntax, pending_unflushed_write_count};
    use crate::record::{self, Frame};

    #[test]
    fn glob_syntax_detection() {
        assert!(contains_glob_syntax("unnamed-*"));
        assert!(contains_glob_syntax("foo?"));
        assert!(contains_glob_syntax("name[0-9]"));
        assert!(!contains_glob_syntax("agent-prod"));
    }

    #[test]
    fn pending_unflushed_write_counter_ignores_non_write_and_flushed_frames() {
        let frames = vec![
            Frame {
                offset: 0,
                tag: record::TAG_WRITE_OP,
                flushed: false,
                payload: Vec::new(),
            },
            Frame {
                offset: 1,
                tag: record::TAG_WRITE_OP,
                flushed: true,
                payload: Vec::new(),
            },
            Frame {
                offset: 2,
                tag: record::TAG_PROFILE_HEADER,
                flushed: false,
                payload: Vec::new(),
            },
        ];
        assert_eq!(pending_unflushed_write_count(&frames), 1);
    }
}

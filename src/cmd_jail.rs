use anyhow::{Context, Result};

use crate::cli::{AddCommand, ListCommand, RmCommand};
use crate::jail;
use crate::privileges;
use crate::run_with_log;

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
    let resolved = run_with_log(
        || {
            jail::resolve(
                rm.name.as_deref(),
                rm.profile.as_deref(),
                jail::ResolveMode::MustExist,
            )
        },
        || "resolve jail".to_string(),
    )?;
    run_with_log(
        || jail::remove_jail_with_verbose(&resolved.paths, rm.verbose),
        || "remove jail runtime/state artifacts".to_string(),
    )
}

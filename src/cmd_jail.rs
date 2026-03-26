use anyhow::{Context, Result};

use crate::cli::{AddCommand, ListCommand, RmCommand};
use crate::jail;

pub(crate) fn add_command(add: AddCommand) -> Result<()> {
    jail::validate_explicit_name(&add.name)
        .with_context(|| format!("invalid jail name '{}'", add.name))?;
    jail::resolve(
        Some(&add.name),
        add.profile.as_deref(),
        jail::ResolveMode::EnsureExists,
    )
    .context("failed to create or reuse explicit jail")?;
    Ok(())
}

pub(crate) fn list_command(_list: ListCommand) -> Result<()> {
    for name in jail::list_named_jails()? {
        println!("{}", name.to_string_lossy());
    }
    Ok(())
}

pub(crate) fn rm_command(rm: RmCommand) -> Result<()> {
    let resolved = jail::resolve(
        rm.name.as_deref(),
        rm.profile.as_deref(),
        jail::ResolveMode::MustExist,
    )
    .context("failed to resolve jail to remove")?;
    jail::remove_jail(&resolved.paths)
}

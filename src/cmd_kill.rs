use anyhow::Result;

use crate::fuse_runtime;

pub(crate) fn kill_command() -> Result<()> {
    let _ = fuse_runtime::kill_global_daemon()?;
    Ok(())
}

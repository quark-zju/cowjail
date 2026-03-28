use anyhow::Result;

use crate::cli::MountCommand;
use crate::cowfs;
use crate::profile_loader::{append_profile_header, ensure_record_parent_dir, load_profile};
use crate::record;
use crate::run_with_log;

pub(crate) fn mount_command(mount: MountCommand) -> Result<()> {
    let loaded = run_with_log(
        || load_profile(std::path::Path::new(&mount.profile)),
        || format!("load mount profile '{}'", mount.profile),
    )?;
    run_with_log(
        || ensure_record_parent_dir(&mount.record),
        || format!("prepare record parent dir {}", mount.record.display()),
    )?;
    let writer = run_with_log(
        || record::Writer::open_append(&mount.record),
        || format!("open mount record writer {}", mount.record.display()),
    )?;
    run_with_log(
        || append_profile_header(&writer, &loaded.normalized_source),
        || {
            format!(
                "append mount profile header into {}",
                mount.record.display()
            )
        },
    )?;

    let fs = cowfs::CowFs::new(loaded.profile, writer);
    crate::vlog!(
        "mount: mounting fuse at {} with record {}",
        mount.path.display(),
        mount.record.display()
    );
    run_with_log(
        || fs.mount(&mount.path, false),
        || format!("mount fuse at {}", mount.path.display()),
    )
}

use anyhow::Result;

pub(crate) fn drop_to_real_user() -> Result<()> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    drop_to_ids(uid, gid)
}

fn drop_to_ids(uid: u32, gid: u32) -> Result<()> {
    if unsafe { libc::setgroups(0, std::ptr::null()) } != 0 {
        let err = std::io::Error::last_os_error();
        return Err(anyhow::anyhow!("setgroups([]) failed: {err}"));
    }
    if unsafe { libc::setgid(gid) } != 0 {
        let err = std::io::Error::last_os_error();
        return Err(anyhow::anyhow!("setgid({gid}) failed: {err}"));
    }
    if unsafe { libc::setuid(uid) } != 0 {
        let err = std::io::Error::last_os_error();
        return Err(anyhow::anyhow!("setuid({uid}) failed: {err}"));
    }
    if unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        let err = std::io::Error::last_os_error();
        return Err(anyhow::anyhow!("prctl(PR_SET_NO_NEW_PRIVS) failed: {err}"));
    }
    Ok(())
}

pub(crate) fn in_initial_user_namespace() -> Result<bool> {
    let raw = fs_err::read_to_string("/proc/self/uid_map")
        .map_err(|err| anyhow::anyhow!("failed to read /proc/self/uid_map: {err}"))?;
    let Some(first_line) = raw.lines().next() else {
        return Err(anyhow::anyhow!("/proc/self/uid_map is empty"));
    };
    let mut fields = first_line.split_whitespace();
    let inside = fields
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid uid_map line: {first_line}"))?
        .parse::<u64>()
        .map_err(|err| anyhow::anyhow!("invalid uid_map inside uid: {err}"))?;
    let outside = fields
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid uid_map line: {first_line}"))?
        .parse::<u64>()
        .map_err(|err| anyhow::anyhow!("invalid uid_map outside uid: {err}"))?;
    let length = fields
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid uid_map line: {first_line}"))?
        .parse::<u64>()
        .map_err(|err| anyhow::anyhow!("invalid uid_map length: {err}"))?;
    Ok(inside == 0 && outside == 0 && length == u32::MAX as u64)
}

use anyhow::Result;

pub(crate) fn drop_to_user(uid: u32, gid: u32, no_new_privs: bool) -> Result<()> {
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
    if no_new_privs && unsafe { libc::prctl(libc::PR_SET_NO_NEW_PRIVS, 1, 0, 0, 0) } != 0 {
        let err = std::io::Error::last_os_error();
        return Err(anyhow::anyhow!("prctl(PR_SET_NO_NEW_PRIVS) failed: {err}"));
    }
    Ok(())
}

pub(crate) fn drop_to_real_user(no_new_privs: bool) -> Result<()> {
    let uid = unsafe { libc::getuid() };
    let gid = unsafe { libc::getgid() };
    drop_to_user(uid, gid, no_new_privs)
}

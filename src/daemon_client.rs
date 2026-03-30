use anyhow::{Context, Result, bail};
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, BorrowedFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::process::{Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::cmd_daemon;

pub(crate) fn ensure_daemon_running(verbose: bool, profile_source: &str) -> Result<OwnedFd> {
    let socket_path = cmd_daemon::default_socket_path();
    if ping_daemon(&socket_path).is_ok() {
        return daemon_pidfd(&socket_path);
    }

    spawn_daemon_process(verbose)?;
    wait_for_daemon(&socket_path, Duration::from_secs(2))?;
    set_profile(profile_source)?;
    daemon_pidfd(&socket_path)
}

fn daemon_pidfd(socket_path: &std::path::Path) -> Result<OwnedFd> {
    let pid = daemon_pid_from_socket(socket_path)?;
    open_pidfd(pid)
}

fn daemon_pid_from_socket(socket_path: &std::path::Path) -> Result<libc::pid_t> {
    let mut stream = UnixStream::connect(socket_path).with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    })?;
    let creds = peer_credentials_for_stream(&stream)?;
    stream.write_all(b"ping\n").with_context(|| {
        format!(
            "failed to finalize daemon pidfd probe request to {}",
            socket_path.display()
        )
    })?;
    let mut response = String::new();
    stream.read_to_string(&mut response).with_context(|| {
        format!(
            "failed to read daemon pidfd probe response from {}",
            socket_path.display()
        )
    })?;
    if response.trim() != "pong" {
        bail!("unexpected daemon pidfd probe response: {}", response.trim());
    }
    Ok(creds.pid)
}

fn open_pidfd(pid: libc::pid_t) -> Result<OwnedFd> {
    let raw_fd = unsafe { libc::syscall(libc::SYS_pidfd_open, pid, libc::PIDFD_NONBLOCK) as libc::c_int };
    if raw_fd < 0 {
        return Err(std::io::Error::last_os_error())
            .with_context(|| format!("pidfd_open failed for daemon pid {pid}"));
    }
    let fd = unsafe { OwnedFd::from_raw_fd(raw_fd) };
    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFD) };
    if flags < 0 {
        return Err(std::io::Error::last_os_error()).context("fcntl(F_GETFD) failed for daemon pidfd");
    }
    if unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_SETFD, flags | libc::FD_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error()).context("fcntl(F_SETFD) failed for daemon pidfd");
    }
    Ok(fd)
}

fn peer_credentials_for_stream(stream: &UnixStream) -> Result<libc::ucred> {
    let fd = stream.as_raw_fd();
    let mut creds = libc::ucred {
        pid: 0,
        uid: 0,
        gid: 0,
    };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            fd,
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut creds as *mut libc::ucred as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(std::io::Error::last_os_error()).context("getsockopt(SO_PEERCRED) failed");
    }
    Ok(creds)
}

fn ping_daemon(socket_path: &std::path::Path) -> Result<()> {
    let response = send_request(socket_path, "ping")?;
    if response.trim() == "pong" {
        return Ok(());
    }
    bail!("unexpected daemon ping response: {}", response.trim())
}

pub(crate) fn set_profile(profile_source: &str) -> Result<()> {
    let request = format!("set-profile\n{profile_source}");
    let response = send_request(&cmd_daemon::default_socket_path(), &request)?;
    if response.trim() == "ok profile-updated" {
        return Ok(());
    }
    bail!("daemon refused profile update: {}", response.trim())
}

pub(crate) fn shutdown_daemon() -> Result<()> {
    let response = send_request(&cmd_daemon::default_socket_path(), "shutdown")?;
    if response.trim() == "ok shutting-down" {
        return Ok(());
    }
    bail!("daemon refused shutdown request: {}", response.trim())
}

pub(crate) fn get_profile_if_running() -> Result<Option<String>> {
    let Some(response) = try_send_request(&cmd_daemon::default_socket_path(), "get-profile")?
    else {
        return Ok(None);
    };
    let Some(body) = response.strip_prefix("ok\n") else {
        bail!("daemon returned unexpected get-profile response: {}", response.trim());
    };
    Ok(Some(body.strip_suffix('\n').unwrap_or(body).to_string()))
}

pub(crate) fn subscribe_tail_fd(fd: BorrowedFd<'_>) -> Result<()> {
    let mut stream = UnixStream::connect(cmd_daemon::default_socket_path()).with_context(|| {
        format!(
            "failed to connect to daemon socket {}",
            cmd_daemon::default_socket_path().display()
        )
    })?;
    send_request_with_fd(&stream, "tail", fd)?;
    let mut response = String::new();
    stream.read_to_string(&mut response).with_context(|| {
        format!(
            "failed to read daemon response from {}",
            cmd_daemon::default_socket_path().display()
        )
    })?;
    if response.trim() == "ok tailing" {
        return Ok(());
    }
    bail!("daemon refused tail request: {}", response.trim())
}

fn send_request(socket_path: &std::path::Path, request: &str) -> Result<String> {
    let Some(response) = try_send_request(socket_path, request)? else {
        bail!(
            "failed to connect to daemon socket {}",
            socket_path.display()
        )
    };
    Ok(response)
}

fn try_send_request(socket_path: &std::path::Path, request: &str) -> Result<Option<String>> {
    let mut stream = match UnixStream::connect(socket_path) {
        Ok(stream) => stream,
        Err(err) if daemon_not_running_error(&err) => return Ok(None),
        Err(err) => {
            return Err(err).with_context(|| {
                format!(
                    "failed to connect to daemon socket {}",
                    socket_path.display()
                )
            });
        }
    };
    stream.write_all(request.as_bytes()).with_context(|| {
        format!(
            "failed to write daemon request to {}",
            socket_path.display()
        )
    })?;
    stream.write_all(b"\n").with_context(|| {
        format!(
            "failed to finalize daemon request to {}",
            socket_path.display()
        )
    })?;

    let mut response = String::new();
    stream.read_to_string(&mut response).with_context(|| {
        format!(
            "failed to read daemon response from {}",
            socket_path.display()
        )
    })?;
    Ok(Some(response))
}

fn send_request_with_fd(stream: &UnixStream, request: &str, fd: BorrowedFd<'_>) -> Result<()> {
    let message = format!("{request}\n");
    let mut iov = libc::iovec {
        iov_base: message.as_ptr() as *mut libc::c_void,
        iov_len: message.len(),
    };
    let mut control = vec![0u8; unsafe {
        libc::CMSG_SPACE(std::mem::size_of::<libc::c_int>() as libc::c_uint) as usize
    }];
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr() as *mut libc::c_void;
    msg.msg_controllen = control.len();

    unsafe {
        let cmsg = libc::CMSG_FIRSTHDR(&msg);
        if cmsg.is_null() {
            bail!("failed to construct daemon control message for fd passing");
        }
        (*cmsg).cmsg_level = libc::SOL_SOCKET;
        (*cmsg).cmsg_type = libc::SCM_RIGHTS;
        (*cmsg).cmsg_len = libc::CMSG_LEN(std::mem::size_of::<libc::c_int>() as libc::c_uint) as _;
        std::ptr::write(libc::CMSG_DATA(cmsg) as *mut libc::c_int, fd.as_raw_fd());
        msg.msg_controllen = (*cmsg).cmsg_len;
    }

    let sent = unsafe { libc::sendmsg(stream.as_raw_fd(), &msg, 0) };
    if sent < 0 {
        return Err(std::io::Error::last_os_error()).context("sendmsg failed for daemon request");
    }
    if sent as usize != message.len() {
        bail!("short sendmsg while sending daemon request with fd");
    }
    Ok(())
}

fn daemon_not_running_error(err: &std::io::Error) -> bool {
    matches!(
        err.raw_os_error(),
        Some(libc::ENOENT | libc::ECONNREFUSED)
    )
}

fn spawn_daemon_process(verbose: bool) -> Result<()> {
    let exe = std::env::current_exe().context("failed to locate current executable")?;
    let mut cmd = Command::new(exe);
    cmd.arg("_daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    if verbose {
        cmd.arg("-v");
    }
    cmd.spawn().context("failed to spawn daemon process")?;
    Ok(())
}

fn wait_for_daemon(socket_path: &std::path::Path, timeout: Duration) -> Result<()> {
    let deadline = Instant::now() + timeout;
    let mut last_err = None;
    while Instant::now() < deadline {
        match ping_daemon(socket_path) {
            Ok(()) => return Ok(()),
            Err(err) => last_err = Some(err),
        }
        thread::sleep(Duration::from_millis(50));
    }
    if let Some(err) = last_err {
        return Err(err).context("daemon did not become ready before timeout");
    }
    bail!("daemon did not become ready before timeout")
}

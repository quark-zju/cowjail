use anyhow::{Context, Result};
use std::io::{Read, Write};
use std::os::fd::{AsFd, FromRawFd, OwnedFd};
use std::thread;

use crate::cli::TailCommand;
use crate::daemon_client;

pub(crate) fn tail_command(_cmd: TailCommand) -> Result<()> {
    let (read_fd, write_fd) = create_pipe()?;
    daemon_client::subscribe_tail_fd(write_fd.as_fd())?;
    drop(write_fd);

    let mut read_file = std::fs::File::from(read_fd);
    let mut stdout = std::io::stdout().lock();
    let mut buffer = [0u8; 8192];
    loop {
        let count = read_file
            .read(&mut buffer)
            .context("failed to read daemon tail stream")?;
        if count == 0 {
            break;
        }
        stdout
            .write_all(&buffer[..count])
            .context("failed to write tail output to stdout")?;
        stdout.flush().context("failed to flush tail stdout")?;
        thread::yield_now();
    }
    Ok(())
}

fn create_pipe() -> Result<(OwnedFd, OwnedFd)> {
    let mut fds = [0; 2];
    if unsafe { libc::pipe2(fds.as_mut_ptr(), libc::O_CLOEXEC) } != 0 {
        return Err(std::io::Error::last_os_error()).context("pipe2 failed for tail command");
    }
    let read_fd = unsafe { OwnedFd::from_raw_fd(fds[0]) };
    let write_fd = unsafe { OwnedFd::from_raw_fd(fds[1]) };
    Ok((read_fd, write_fd))
}

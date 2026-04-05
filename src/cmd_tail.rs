use std::io::{Read, Write};
use std::os::unix::net::UnixStream;

use anyhow::{Context, Result};

use crate::cli::TailCommand;
use crate::fuse_runtime;
use crate::tail_ipc::EventKind;

pub(crate) fn tail_command(command: TailCommand) -> Result<()> {
    let socket_path = fuse_runtime::global_tail_socket_path()?;
    let mut stream = UnixStream::connect(&socket_path)
        .with_context(|| format!("failed to connect {}", socket_path.display()))?;
    let filter_line = format_filter_line(&command.kinds);
    stream
        .write_all(filter_line.as_bytes())
        .context("failed to send tail filter")?;
    stream.flush().context("failed to flush tail filter")?;

    let mut stdout = std::io::stdout().lock();
    let mut buffer = [0u8; 8192];
    loop {
        let count = stream
            .read(&mut buffer)
            .context("failed to read tail stream")?;
        if count == 0 {
            break;
        }
        stdout
            .write_all(&buffer[..count])
            .context("failed to write tail output")?;
        stdout.flush().context("failed to flush tail output")?;
    }
    Ok(())
}

fn format_filter_line(kinds: &[EventKind]) -> String {
    if kinds.is_empty() {
        return "\n".to_owned();
    }
    let kinds = kinds
        .iter()
        .map(|kind| kind.as_token())
        .collect::<Vec<_>>()
        .join(",");
    format!("kinds={kinds}\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_filter_line_supports_empty_and_non_empty_lists() {
        assert_eq!(format_filter_line(&[]), "\n");
        assert_eq!(
            format_filter_line(&[EventKind::LookupMiss, EventKind::Lock]),
            "kinds=lookup-miss,lock\n"
        );
    }
}

mod cli;
mod profile;

use anyhow::{Result, bail};

fn main() {
    if let Err(err) = try_main() {
        eprintln!("error: {err:#}");
        std::process::exit(1);
    }
}

fn try_main() -> Result<()> {
    match cli::parse_env()? {
        cli::Command::Run(_) => bail!("run is not implemented yet"),
        cli::Command::Mount(_) => bail!("mount is not implemented yet"),
        cli::Command::Flush(_) => bail!("flush is not implemented yet"),
    }
}

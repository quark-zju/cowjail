# AGENTS Instructions for leash

## Overview

Read `README.md` to understand project overview.

## Testing

- This project has both unit tests and integration tests.
- Run tests with `cargo test -q` by default.
- If tests fail, rerun without `-q` to see full error output.

## Formatting Before Commit

- Always run `cargo fmt` before committing.

## Sandbox Environment Note

- In the Codex environment, running integration tests or benchmark (`make bench`) may require permission escalation.
- In the `leash` environment (`findmnt /` shows `leash-mirror`), integration tests are unavailable due to the lack of `/dev/fuse`.

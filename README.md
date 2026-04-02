# leash2

Experimental rewrite of `leash` focused on a single global FUSE mirror filesystem with pluggable access control.

## Current Scope

- `src/mirrorfs.rs`: mirror-style FUSE filesystem backed by the real host filesystem via `fs_err`
- `src/access.rs`: standalone access-control trait and operation model; FUSE does not depend directly on `profile`
- `src/profile.rs`: conditional profile rule engine for future policy integration
- `tests/integration.rs`: custom integration harness that mounts one FUSE instance and runs filesystem-facing tests against the real mount

The intended direction is:

- one global FUSE mount
- one global policy/profile in the future
- per-process-name access decisions instead of multiple mounts with separate profiles

## Implemented In MirrorFs

- mirror read/write behavior on top of a backing directory
- per-request access checks through `AccessController`
- process-name lookup from `/proc/<pid>/comm`
- stable handle behavior across rename
- hardlink support
- `mmap`-relevant file semantics coverage
- POSIX byte-range locks (`fcntl`) and `flock` test coverage
- FUSE passthrough attempts for `open` and `create`, with fallback to normal FUSE handles when `open_backing()` is unavailable
- zero TTL for FUSE entry/attr replies to reduce stale kernel-side metadata caching

## Current Limitations

- `src/main.rs` is still a stub; there is no real mount CLI yet
- passthrough is wired up in code, but on the current test machine `open_backing()` returns `EPERM`, so mounted tests still exercise the fallback path for passthrough-sensitive cases
- some lock behavior is still partly kernel-local when mounted, so a few integration checks intentionally downgrade to skip-style passes with an explanation
- the codebase still has some `unused`/`dead_code` allowances while the binary entrypoint is unfinished

## Testing

Run everything:

```bash
cargo test -q
```

Run the mounted integration harness only:

```bash
cargo test --test integration -- --nocapture
```

The integration harness:

- creates one top-level temp directory
- creates `backing/` and `mount/` under it
- mounts one background FUSE instance on `mount/`
- creates one subdirectory per logical subtest under both trees
- runs subtests sequentially

## Integration Logging

The integration harness uses `env_logger` and reads `RUST_LOG`.

Common settings:

```bash
RUST_LOG=integration=debug cargo test --test integration -- --nocapture
RUST_LOG=integration=debug,fuser=off cargo test --test integration -- --nocapture
RUST_LOG=debug cargo test --test integration -- --nocapture
```

These are useful when investigating passthrough activation, lock behavior, and FUSE request flow.

## Notes On Passthrough

With `fuser 0.17`, both `open` and `create` attempt FUSE passthrough:

- `ReplyOpen::open_backing()` / `opened_passthrough()`
- `ReplyCreate::open_backing()` / `created_passthrough()`

If the kernel or environment rejects backing registration, the filesystem falls back to normal FUSE replies and logs the reason at debug level.

## Dependencies Used Intentionally

- `fs_err` for host filesystem access
- `fuser` for FUSE
- `anyhow` and `thiserror` for errors
- `log` and `env_logger` for logging
- `tempfile` and `memmap2` for tests

## Near-Term Next Steps

- replace the stub `main` with a real mount CLI
- connect `profile` to `AccessController`
- investigate why `open_backing()` currently fails with `EPERM` in the local environment
- tighten mounted tests around true passthrough activation once the environment issue is resolved

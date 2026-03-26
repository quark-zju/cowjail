# cowjail

`cowjail` is a copy-on-write FUSE filesystem jail for running untrusted programs or coding agents with reduced filesystem blast radius.

`cowjail` 是一个基于 FUSE 的 copy-on-write 文件系统隔离层，主要用于运行不可信程序或 coding agent，降低文件系统破坏风险。

## Status

This project is under active development.

当前项目仍在开发中，接口和行为可能继续调整。

## What It Does

- Filters filesystem visibility with profile rules: `ro`, `rw`, `deny`
- Uses an in-memory overlay for writes during `run` or `mount`
- Records write operations into a CBOR-framed record log
- Replays those writes later with `cowjail flush`
- Supports stricter replay with `cowjail flush --profile <profile>`

## What It Does Not Do

`cowjail` is not a full sandbox.

- It does not restrict network access
- It does not enforce CPU or memory limits
- It does not try to contain kernel or FUSE-level escapes
- It is primarily a filesystem safety layer, not a complete isolation boundary

如果你需要的是完整沙箱，`cowjail` 目前并不够。它更适合“限制可见文件 + 延迟写回 + 审计/回放”的场景。

## Requirements

- Linux
- FUSE support available on the host
- `fusermount` available for `mount`-mode smoke tests and manual unmount
- Rust toolchain for building and running via `cargo`

Additional requirement for `run`:

- `cowjail run` requires `euid == 0` because it mounts a temporary FUSE root and then `chroot`s into it
- In practice this means running as root or configuring the binary as setuid root

## Quick Start

Create a profile:

```text
/bin ro
/lib ro
/lib64 ro
/usr ro
/etc ro
/tmp rw
. rw
```

Debug with `mount` first:

```bash
cargo run -- mount --profile ./default --record ./session.cjr /tmp/cowjail-mnt
```

In another shell:

```bash
echo hi > /tmp/cowjail-mnt/tmp/example.txt
```

At this point, the host filesystem is still unchanged. The write lives only in the in-memory overlay and in the record file.

Replay later:

```bash
cargo run -- flush --record ./session.cjr --dry-run
cargo run -- flush --record ./session.cjr
```

## Commands

```bash
cowjail run [--profile <profile>] [--record <record_path>] [-v|--verbose] command ...
cowjail mount --profile <profile> --record <record_path> [-v|--verbose] <path>
cowjail flush [--record <record_path>] [--profile <profile>] [--dry-run] [-v|--verbose]
```

- `run`: mounts a temporary FUSE root, `chroot`s into it, `chdir`s back to the original cwd, drops to real uid/gid, then executes the target command
- `mount`: debug mode without `chroot` and without command execution
- `flush`: replays unflushed operations from the record into the host filesystem

## Typical Workflow

1. Write a profile that exposes only the paths the target program should see.
2. Start with `cowjail mount` to debug visibility and write behavior without root or `chroot`.
3. Use `cowjail run` once the profile is correct and you want the full jailed execution path.
4. Inspect pending writes with `cowjail flush --dry-run`.
5. Apply writes with `cowjail flush`.
6. If needed, replay under a stricter policy with `cowjail flush --profile <other_profile>`.

## Profile Format

Each non-empty line is:

```text
<pattern> <action>
```

Actions:

- `ro`: visible and readable, but write-like operations are denied
- `rw`: visible and writable in the in-memory overlay
- `deny`: hidden and inaccessible

Rules are evaluated in order. The first matching rule wins.

Unmatched paths are hidden.

Special pattern:

- `.` means the current working directory when the profile is loaded

Example:

```text
/bin ro
/lib ro
/lib64 ro
/tmp rw
/etc ro
/var ro
/home/*/.ssh deny
. rw
```

Important semantics:

- If `/foo/bar` is allowed, its parent directories become implicitly visible so traversal still works
- Rule order matters; a broad earlier rule can shadow a narrower later rule
- `.` is normalized into an absolute path before it is written into the record header

## Write and Replay Model

During `run` or `mount`:

- Reads come from the host filesystem, filtered by the profile
- Writes go into an in-memory overlay
- The host filesystem is not modified immediately
- Each write-like operation is appended to the record file

During `flush`:

- Previously unflushed operations are replayed onto the host filesystem
- Operations already marked as flushed are skipped
- Replay is idempotent across repeated `flush` runs
- Partial or corrupt record tails are ignored

## Flush Replay Policy

If `flush --profile` is provided, that profile is used as the replay policy.

Otherwise, `flush` uses the latest normalized profile header stored in the record.

If no profile header exists, replay falls back to permissive behavior for backward compatibility with older records.

Replay applies an operation only when all relevant paths are `rw` under the effective replay profile:

- `WriteFile { path, .. }`: `path` must be `rw`
- `CreateDir { path }`: `path` must be `rw`
- `RemoveDir { path }`: `path` must be `rw`
- `Truncate { path, .. }`: `path` must be `rw`
- `Rename { from, to }`: both `from` and `to` must be `rw`

Blocked operations are not marked flushed, so they can be retried later with a broader profile.

## Record Format

Each frame is:

- `tag: u8`
- `len: u64` in little-endian
- `checksum: u64` using `xxhash64` over the payload
- `payload: [u8; len]` encoded as CBOR

The high bit of the tag byte is reserved as the flushed marker.

Current tags:

- `0x01`: write operation
- `0x02`: normalized profile header

The reader stops at the first incomplete or checksum-invalid tail frame and ignores the remaining bytes.

## Build

```bash
cargo build
```

## Test

Unit tests:

```bash
cargo test
```

Manual end-to-end smoke test:

```bash
./docs/e2e_smoke.py
```

## License

MIT. See `LICENSE`.

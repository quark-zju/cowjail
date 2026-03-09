# cowjail Implementation Plan (Atomic Commits)

This document breaks down the `cowjail` project into small, reviewable, and runnable commits.

## Scope Recap

- Command interface:
  - `cowjail run [--profile <profile>] [--record <record_path>] command ...`
  - `cowjail flush [--record <record_path>] [--dry-run]`
- Profile rules:
  - First matched rule wins.
  - If `/foo/bar` is allowed, parent `/foo` is visible.
  - Unmatched paths are invisible.
  - `.` means current working directory at launch.
- IO model:
  - `ro` read-only pass-through.
  - `rw` write buffered in memory (copy-on-write), not persisted immediately.
  - `deny` hidden/inaccessible.
- Record model:
  - Append framed CBOR records.
  - Supports partial trailing record ignore and checksum validation.
  - Supports marking entries as flushed to avoid double-apply.
  - `run` periodically flushes record buffer to disk.
- Runtime flow:
  1) Start FUSE mount with filtered view.
  2) `chroot` into mount.
  3) Execute command.
  4) Record write operations for later `flush`.

## Commit-by-Commit Plan

1. `build: add foundational crates and feature flags`
- Add CLI, logging, path and serialization crates (`clap`, `tracing`, `thiserror`, `camino`, optional `tokio` if needed).
- Keep compile green with no behavior changes.

2. `cli: scaffold run/flush subcommands and option parsing`
- Implement `clap` schema and argument validation.
- Add typed command enum and placeholders.

3. `profile: define rule grammar and parser`
- Implement `RuleAction` (`ro/rw/deny`) and parse profile file line format.
- Add syntax error reporting with line numbers.

4. `profile: implement matcher with first-hit semantics`
- Use glob-based matching and canonical absolute path handling.
- Resolve `.` to launch cwd at load time.

5. `profile: add parent-visibility expansion logic`
- Ensure allowing `/a/b` also implies visibility for `/a`.
- Keep deny semantics intact for explicitly denied deeper paths.

6. `profile: add table-driven tests for precedence and edge cases`
- Cases: first match wins, unmatched hidden, dot expansion, overlapping rules.

7. `record: design framed binary format and constants`
- Define tag byte, length (u64), checksum, payload layout.
- Document forward compatibility and version byte strategy.

8. `record: implement writer with atomic append discipline`
- Append records with checksum.
- `fsync` strategy and temp-write behavior where needed.

9. `record: implement tolerant reader for partial/corrupt tail`
- Stop at first incomplete/tail-corrupt frame; ignore remaining bytes.
- Expose iterator-style API for replay.

10. `record: add flushed-bit/tag mutation support`
- Add in-place mark for flushed record entries.
- Prevent reapply on repeated `flush`.

11. `record: add tests for crash-recovery scenarios`
- Simulate truncation, bad checksum, duplicate flush execution.

12. `cow: implement in-memory copy-on-write inode/file buffer`
- Create overlay map for created/modified/deleted paths.
- Support read-after-write behavior within same session.

13. `cow: map VFS write operations to record operations`
- Define operation schema (create/write/truncate/unlink/rename/mkdir/rmdir/chmod/chown/utimens if supported).
- Serialize via CBOR payload types.

14. `fuse: scaffold filesystem with lookup/readdir/open/read`
- Minimal read-only view following profile visibility.
- No mutation yet; ensure mount/unmount lifecycle works.

15. `fuse: enforce ro/rw/deny permissions on operation paths`
- Read path policy at syscall boundary and return proper errno.
- Verify denied files are non-discoverable where required.

16. `fuse: implement rw write path via in-memory cow layer`
- Route write-like operations into overlay map, never host FS.
- Reflect metadata updates in overlay.

17. `run: mount, chroot, and child command execution`
- Prepare mountpoint, `chroot`, `chdir("/")`, exec command.
- Capture exit status and signal mapping.

18. `run: periodic record flush and graceful shutdown handling`
- Background periodic flush of buffered records.
- Final sync on process exit/signals.

19. `flush: implement replay engine with profile-aware safety checks`
- Pick latest record by mtime when unspecified.
- Apply only unflushed entries; support idempotent reruns.

20. `flush: add --dry-run output and diff-like summary`
- Print intended mutations without disk writes.
- Mark nothing as flushed in dry-run mode.

21. `git: repository integration for audit trail`
- Detect repo root and append commit/log metadata per flush batch.
- Keep failure non-fatal if not inside git repo.

22. `ops: add lock strategy for concurrent run/flush access`
- File lock for record writing and flush replay coordination.
- Ensure flush can proceed while run is active without corruption.

23. `docs: write profile/reference docs and threat model`
- Clarify guarantees, non-goals (not a full sandbox), known escapes.
- Provide sample profiles and operational playbook.

24. `test: integration tests for run+flush end-to-end`
- Use temp dirs to validate no host writes during run.
- Validate flushed results and idempotency.

25. `release: polish errors, logs, and default paths`
- Default record path under `.cache/cowjail/<timestamp>.cjr`.
- Improve actionable error messages and CLI UX.

## Suggested Milestones

- M1 (commits 1-6): CLI + profile engine complete.
- M2 (commits 7-11): durable record format complete.
- M3 (commits 12-18): FUSE runtime and run command complete.
- M4 (commits 19-22): flush workflow + concurrency complete.
- M5 (commits 23-25): docs, tests, release hardening.

## Risks and Early Decisions

- FUSE crate stability: vendor currently exists; verify required ops coverage early (commit 14) to avoid late redesign.
- `chroot` requires privilege/capability in many environments; if unavailable, add fallback strategy (e.g., `pivot_root`/namespace mode later).
- Glob matching correctness with symlink/canonical paths must be deterministic to avoid policy bypass.
- Record mutation-in-place for flushed mark needs careful crash consistency; consider single-byte state field designed for atomic write.

## Definition of Done (v0)

- `run` can safely execute untrusted command with profile-constrained visible tree.
- Any writes are isolated in memory and recorded to record file.
- `flush` can replay reliably, resume after interruption, and avoid duplicate application.
- Core integration tests pass on a Linux environment with FUSE support.

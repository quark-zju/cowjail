# Troubleshooting

## `run` fails with FUSE or permission errors

### `fusermount: option allow_other only allowed if 'user_allow_other' is set in /etc/fuse.conf`

`cowjail` may need `allow_other` for high-level `run` mounts.

Fix:

1. Edit `/etc/fuse.conf` as root.
2. Ensure this line is present and uncommented:

```text
user_allow_other
```

### `failed to spawn child command in jail: Invalid argument (os error 22)`

This is often a follow-on symptom when mount/chroot access failed earlier.

Run with verbose logging to inspect the step that failed:

```bash
cowjail run -v --name <name> -- <command>
```

If `_fuse` starts but fails later, inspect the runtime log:

- `${XDG_RUNTIME_DIR}/cowjail/<name>/fuse.log`
- fallback: `/run/user/<uid>/cowjail/<name>/fuse.log`

## `_suid` and setuid behavior

### `_suid` appears successful but privileged operations still fail

The binary may be on a `nosuid` mount.

Check owner/mode:

```bash
ls -l ./target/debug/cowjail
```

Expected: owner `root`, and mode containing `s` on user execute bit (for example `-rwsr-xr-x`).

If your workspace filesystem is mounted with `nosuid`, place the binary on a mount where setuid is honored.

## `rm` fails with mount-related errors

### `Device or resource busy (os error 16)`

A FUSE mount may still be active. Retry with verbose logs to see unmount/cleanup steps:

```bash
cowjail rm -v <name>
```

### `Transport endpoint is not connected (os error 107)`

This indicates a stale/disconnected FUSE mountpoint. `cowjail rm` includes recovery logic; rerun with `-v` for details.

## E2E smoke notes

`docs/e2e_smoke.py` uses `_suid` for high-level tests. If the test reports setuid-related skip/failures, verify:

1. The built binary can become setuid-root.
2. FUSE config (`user_allow_other`) is set when needed.

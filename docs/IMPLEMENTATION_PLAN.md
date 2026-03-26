# cowjail Implementation Plan

This document replaces the old single-process `run + temporary chroot` plan with a named-jail design.

The new direction is:

- jails have stable names
- a jail can outlive one process
- entering an existing jail should feel closer to `ip netns exec <name> ...`
- named jails should imply stable record file naming
- record state should survive host reboot
- replay should gradually move from "host-side flush only" toward "filesystem can materialize state from record itself"

## Design Judgment

This direction is feasible and materially better for usability.

The strongest part of the proposal is the shift from an almost-anonymous one-shot jail to a named object with lifecycle:

- easier to inspect
- easier to re-enter
- easier to associate with a record file
- easier to debug
- easier to recover after reboot

The main architectural consequence is that `cowjail` stops being only a command runner and becomes a jail manager.

That means the implementation should explicitly manage:

- jail identity
- jail metadata
- jail lifecycle
- namespace entry/exit
- persistent backing state

## Core Model

Each jail has a globally unique name, for example:

- `cowjail create --name agent-a --profile default`
- `cowjail exec agent-a command ...`
- `cowjail mount agent-a <path>`
- `cowjail flush agent-a`
- `cowjail rm agent-a`

Each named jail should have durable metadata under a stable runtime path plus a durable record path.

Suggested split:

- runtime namespace handle and mount wiring under `/run/cowjail/<name>/`
- durable record and metadata under `~/.local/state/cowjail/` or `~/.cache/cowjail/`

The runtime path can disappear on reboot; the durable state must not.

## Namespace Design

The proposal to use a mount namespace is correct.

Recommended behavior:

1. Create a new mount namespace for the jail.
2. Bind-mount a namespace handle into `/run/cowjail/<name>/mntns` so it can be reopened later.
3. Mount the jail FUSE filesystem inside that namespace.
4. Enter the namespace when running commands or attaching debug mounts.

This is analogous to `ip netns`, but for mount namespaces.

### IPC Isolation

Adding IPC isolation is worth doing.

Reason:

- some launchers or helpers may use IPC channels that should not bleed across jail boundaries
- it reduces accidental interaction with host session services
- it makes the jail concept more coherent when reused by multiple commands

Suggested sequence:

- start with mount namespace only
- then add IPC namespace
- evaluate whether PID namespace is needed later

PID namespace is valuable, but it is a larger behavioral change than mount+IPC and does not need to block the named-jail design.

## Record and State Design

Named jails imply stable records.

Instead of "new implicit record for each run", use:

- one stable record file per jail name
- one metadata file per jail name

Suggested durable files:

- `state/<name>/record.cjr`
- `state/<name>/profile`
- `state/<name>/meta.json`

Suggested metadata contents:

- jail name
- normalized profile source
- creation time
- last attach time
- record path
- runtime namespace handle path
- status flags

## Reboot Survival

If you want the jail to survive reboot conceptually, split "jail identity" from "live namespace instance".

After reboot:

- the mount namespace handle under `/run` is gone
- the record and profile remain
- `cowjail revive <name>` or `cowjail start <name>` should recreate the namespace and remount the filesystem from durable state

This is where FUSE-internal replay becomes important.

## FUSE Internal Replay

This is the right long-term move.

Today the model is:

- host filesystem visible through profile
- overlay in memory
- write operations appended to record
- separate `flush` replays record to host

For reboot survival, the jail filesystem itself should be able to reconstruct overlay state from record on mount.

That means:

- open record at mount time
- scan valid frames
- build overlay state from unflushed operations
- present reconstructed state immediately inside the FUSE filesystem

This should be treated as "overlay replay", distinct from "host flush replay".

Two replay layers:

1. `record -> overlay`
   Used when mounting or reviving a jail. Does not mutate host filesystem.

2. `record -> host`
   Used by `cowjail flush`. Mutates host filesystem and marks frames flushed.

That split gives you reboot persistence without forcing immediate host writes.

## CLI Direction

The old commands are too tied to ephemeral execution.

Suggested new top-level shape:

```text
cowjail create --name <name> [--profile <profile>]
cowjail start <name>
cowjail exec <name> command ...
cowjail mount <name> <path>
cowjail flush <name> [--dry-run] [--profile <profile>]
cowjail status [<name>]
cowjail rm <name>
```

Possible compatibility bridge:

- keep `cowjail run` as sugar for `create + exec + optional auto-cleanup`

That avoids breaking the current UX immediately while moving the internals to named jails.

## Recommended Implementation Order

### Phase 1: Persistent jail identity

1. `state: add named jail metadata model`
- Introduce jail name validation and on-disk metadata layout.
- Make jail names globally unique.

2. `cli: add create/status/rm commands`
- Start managing named jails even before namespace reuse exists.
- Keep runtime behavior simple.

3. `record: bind jail name to stable record path`
- Replace implicit per-run record with a stable record path derived from jail name.
- Keep existing frame format.

4. `profile: persist normalized profile into jail metadata`
- Remove ambiguity between "profile path" and "actual resolved profile content".

### Phase 2: Named mount namespace lifecycle

5. `ns: create named mount namespace handles under /run/cowjail`
- Add namespace creation and runtime directory conventions.

6. `ns: add enter logic for existing named jail`
- Implement the equivalent of "open handle and setns into it".

7. `mount: move fuse mount lifecycle into named namespace`
- Make mount placement part of jail start rather than part of a single process execution.

8. `cmd: add start and exec commands`
- `start` creates or restores runtime namespace state.
- `exec` enters the jail and runs a command inside it.

### Phase 3: Isolation refinement

9. `ns: add ipc namespace isolation`
- Keep this separate from mount namespace work.
- Add behavioral tests for isolated IPC.

10. `run: preserve privilege dropping inside named exec path`
- Keep `setgroups([])`, `setgid`, `setuid`, `PR_SET_NO_NEW_PRIVS`.

11. `security: document current isolation boundary`
- Explicitly state that network and broader sandboxing are still out of scope.

### Phase 4: Overlay replay from record

12. `record: define overlay replay pass`
- Formalize record-to-overlay replay semantics.

13. `fuse: reconstruct overlay state from record at mount time`
- Make a newly started jail show prior unflushed writes.

14. `fuse: separate overlay replay from host flush replay`
- Do not mark frames flushed just because overlay replay consumed them.

15. `test: reboot-style recovery scenarios`
- Simulate "write in jail -> process exits -> remount jail -> state still visible".

### Phase 5: Operational polish

16. `flush: switch to jail-name based UX`
- `cowjail flush <name>` should discover profile and record from metadata.

17. `status: show live namespace status and pending record info`
- Include pending frame count and whether runtime namespace exists.

18. `docs: update README around named jail workflow`
- Replace temporary run-focused examples with `create/start/exec/flush`.

## Key Risks

### 1. Mount namespace handle management

The `/run` handle approach is sound, but cleanup must be explicit.

Questions to settle:

- what command owns namespace creation
- what command remounts after reboot
- how to detect stale runtime handles

### 2. Stable single record per jail

One stable record file is good for ergonomics, but it increases pressure on:

- compaction
- recovery time on mount
- concurrent writer/flush coordination

Long term, you may want:

- a compacted snapshot file
- plus an append-only active log

But this does not need to block the first named-jail version.

### 3. Overlay replay correctness

Once the filesystem rehydrates from record, replay bugs become mount-time bugs, not just flush-time bugs.

That raises the bar for:

- ordering
- rename boundaries
- symlink behavior
- type transitions

### 4. Root and setns behavior

Entering existing mount namespaces and mounting FUSE in them will likely tighten privilege requirements further.

This should be designed deliberately instead of accreting special cases.

## Definition of Done for This New Direction

Version 1 of the named-jail design is done when:

- a jail has a stable name and stable metadata
- a jail can be started, re-entered, and flushed by name
- the default record file is derived from jail identity, not per-run randomness
- mount namespace state can be recreated after reboot
- unflushed record state can be reconstructed into the FUSE overlay on jail start
- `flush` remains explicit for host filesystem mutation

## Non-Goals for This Plan

These are intentionally out of scope for now:

- network namespace support
- non-Linux platforms
- full process sandboxing
- seccomp/capability micro-hardening beyond the existing exec path

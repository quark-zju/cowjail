# Technical Overview

This document keeps implementation-level details that are intentionally minimized in `README.md`.

## Scope and Boundary

`cowjail` focuses on filesystem-risk reduction:

- path visibility filtering via profile rules
- copy-on-write overlay for write operations
- record-based replay (`flush`) with selective policy checks
- IPC namespace isolation to reduce IPC-based bypass paths

Out of scope:

- network namespace isolation
- complete process sandboxing (seccomp/capability hardening is limited)
- non-Linux platforms

## High-Level Architecture

Main components:

- `run`: resolve/select jail, ensure runtime namespace handles, ensure per-jail FUSE server, execute command in jail
- `_fuse` (hidden): internal long-lived FUSE server entrypoint for a jail runtime
- `flush`: replay pending record operations onto host filesystem
- `add` / `rm` / `list`: named jail lifecycle

State layout:

- persistent state: `~/.local/state/cowjail/<NAME>/...`
- runtime state: `/run/cowjail/<NAME>/...`

## Record Model

Record is CBOR-framed append-only log with:

- tag byte
- payload length
- checksum
- payload

Write operations are appended during FUSE activity and marked flushed when host replay succeeds.
Reader uses best-effort behavior for incomplete/corrupt tail fragments.

## Replay Layers

There are two replay paths:

1. `record -> overlay` (mount-time)
- used by `_fuse` startup
- reconstructs in-memory overlay from unflushed write frames
- does not mutate host filesystem

2. `record -> host` (flush-time)
- used by public `flush` / hidden `_flush`
- applies operations to host filesystem if allowed by replay policy
- marks successfully handled frames as flushed

## Profiles

Profile lines are `pattern action` with first-match-wins.

Actions:

- `ro`
- `rw`
- `deny`

`.` resolves relative to current working directory at profile load time.

## Internal vs Public CLI

Public:

- `run`
- `flush`
- `add`
- `rm`
- `list`

Hidden low-level (debug/recovery):

- `_fuse`
- `_mount`
- `_flush`

These low-level commands are intentionally separate from normal workflow docs.

# cowjail

`cowjail` is a Linux filesystem safety layer for untrusted programs and coding agents.

`cowjail` 是一个 Linux 上的文件系统防护层，面向不可信程序和 coding agent。

It combines:

- profile-based filesystem visibility and write policy (`ro` / `rw` / `deny`)
- copy-on-write behavior (writes stay in overlay + record first)
- selective replay (`flush`) to apply only pending writes you accept
- IPC namespace isolation to reduce escapes via host IPC services (for example `systemd-run`)

Out of scope:

- network isolation
- full process/container sandboxing
- cross-platform support (Linux only)

## Quick Usage (Named Jail Workflow)

Create or pin a named jail:

```bash
cowjail add --name agent --profile default
```

Run command inside jail (auto-create if missing):

```bash
cowjail run --name agent -- your-command arg1 arg2
```

Or run by profile-derived jail identity:

```bash
cowjail run --profile default -- your-command
```

Inspect/apply pending writes:

```bash
cowjail flush --name agent --dry-run
cowjail flush --name agent
```

List and remove:

```bash
cowjail list
cowjail rm --name agent
```

## More Docs

- Technical overview: [`docs/TECHNICAL_OVERVIEW.md`](docs/TECHNICAL_OVERVIEW.md)
- Implementation plan and progress: [`docs/IMPLEMENTATION_PLAN.md`](docs/IMPLEMENTATION_PLAN.md)
- E2E smoke test: [`docs/e2e_smoke.py`](docs/e2e_smoke.py)

## License

MIT. See `LICENSE`.

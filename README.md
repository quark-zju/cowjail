# leash

`leash` is a Linux filesystem safety layer for coding agents - keep your AI on a leash.

What it does:

- blocks reads of sensitive files (like `~/.ssh`, browser profiles, system secrets)
- controls what can be written (`/tmp`, agent state, git working copies)
- protects `.git` metadata so only trusted `git` commands can write it

Out of scope:

- network/container isolation
- non-Linux support

`leash` 是一个 Linux 文件系统防护层，主要面向 AI 编码工具。

能做什么：

- 阻止读取敏感文件（如 `~/.ssh`、浏览器配置、系统机密）
- 控制可写路径（`/tmp`、agent 状态目录、git 工作区）
- 保护 `.git` 元数据，只允许可信 `git` 命令写入

不包含：

- 网络/容器隔离
- 非 Linux 系统支持

## Install & Quick start

```bash
cargo install --git https://github.com/quark-zju/leash leash
leash _suid
leash run codex # or opencode, bash, ...
```

Shell completion (optional): put this line in your shell rc file:

```bash
source <(leash completion)
```

## Profiles

Check which paths are read-only or writable:

```bash
leash profile show
```

Modify your local override (applies on the next `leash run`):

```bash
leash profile edit
```

Profile syntax and details: [`docs/PROFILE.md`](docs/PROFILE.md)

## More Docs

- Agent compatibility notes: [`docs/AGENT_COMPAT.md`](docs/AGENT_COMPAT.md)
- Technical overview: [`docs/TECHNICAL_OVERVIEW.md`](docs/TECHNICAL_OVERVIEW.md)
- Profile guide (syntax, actions, default profile): [`docs/PROFILE.md`](docs/PROFILE.md)
- Runtime layout: [`docs/RUNTIME_LAYOUT.md`](docs/RUNTIME_LAYOUT.md)
- Privilege model: [`docs/PRIVILEGE_MODEL.md`](docs/PRIVILEGE_MODEL.md)
- Troubleshooting: [`docs/TROUBLESHOOTING.md`](docs/TROUBLESHOOTING.md)
- Semantic E2E script: [`docs/e2e_semantics.py`](docs/e2e_semantics.py)

## License

MIT. See `LICENSE`.

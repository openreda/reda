# Every action is a command; AI drives REDA through MCP and the CLI, not an in-app assistant

Every user-visible action in REDA is a registered command: a stable id plus parameters described by a machine-readable schema. Menus, toolbars, shortcuts, scripts, the CLI and MCP all invoke the same command execution path, and a command called with parameters never opens a dialog. The engine runs headless with no UI dependency; MCP is served by a separate CLI binary that either runs a Project in-process or bridges to a running REDA instance over an authenticated loopback control port. We do not build an in-app chat assistant: users bring whatever model they prefer through MCP/CLI, which keeps REDA model-agnostic and makes automation a first-class consequence of the architecture rather than a bolt-on.

## Considered Options

- **Free-text parameter descriptions** (the simpler, common approach): rejected in favour of schemas from day one, so MCP tool definitions and validation can be generated rather than hand-maintained.
- **In-app AI chat/agent**: rejected for now; revisit only if far-future evidence shows users want to prompt inside the app.

## Consequences

Undo/redo, journaling and enable/disable state live in the command execution path, so every entry point (GUI or agent) gets them for free. Features that cannot be expressed as commands are a design smell.

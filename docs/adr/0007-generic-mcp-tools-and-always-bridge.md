# MCP exposes a few generic tools over the command registry, and the CLI always bridges to an instance that has the Project open

REDA's MCP server offers a small fixed set of tools (`search_commands`, `describe_command`, `run_command`, plus a handful of frequently needed query commands such as `objects.query`, `object.get` and `document.render`) rather than one MCP tool per command. Agents discover commands progressively, so the tool list costs little context and does not change as the registry grows into hundreds of commands. Separately, whenever a running REDA instance has the target Project open, the `reda` CLI/MCP binary always bridges to it over the control port and never opens the same files in-process, so there is only ever one writer per Project and an agent's edits land in the user's undo stacks.

## Considered Options

- **One MCP tool per command**: rejected. Hundreds of tool definitions would bloat every agent's context, and the tool list would churn with every new command.
- **Per-command tools filtered by user-enabled categories**: rejected as extra configuration for the same result that progressive discovery gives for free.
- **Letting the CLI open a Project in-process while the app has it open**: rejected. Two writers on the same files lose edits, and the user could not undo what the agent did.

## Consequences

- Command descriptions and schemas must be good enough for an agent to pick the right command from a `search_commands` hit. They are written for agents as well as tooltips.
- Several steps an agent wants as one undo entry go through a `batch` edit command, since the generic `run_command` runs one command per call.

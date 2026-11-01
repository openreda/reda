# REDA

Rust monorepo: a Cargo virtual workspace. Each crate lives in `crates/<name>/` and is picked up automatically by `members = ["crates/*"]`. Shared metadata, dependency versions and lints are set at the workspace level, and crates opt in with `edition.workspace = true`, `foo.workspace = true` and `[lints] workspace = true`. The toolchain is pinned in `rust-toolchain.toml`.

Keep third-party dependencies few: depend on crates for substantial pieces that are hard to write well (polygon ops, GUI, MCP, file dialogs), and write small utilities in-house rather than pulling in a long tail of tiny crates.

Everything for REDA lives in this repo (see `docs/adr/0003-monorepo.md`); only forks of upstream dependencies may live elsewhere. Architecture decisions are in `docs/adr/`, domain vocabulary in `GLOSSARY.md`.

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues for openreda/reda, using the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

# REDA

Rust monorepo: a Cargo virtual workspace. Each crate lives in `crates/<name>/` and is picked up automatically by `members = ["crates/*"]`. Shared metadata, dependency versions and lints are set at the workspace level, and crates opt in with `edition.workspace = true`, `foo.workspace = true` and `[lints] workspace = true`. The toolchain is pinned in `rust-toolchain.toml`.

## Agent skills

### Issue tracker

Issues are tracked in GitHub Issues for mnj/REDA, using the `gh` CLI. See `docs/agents/issue-tracker.md`.

### Triage labels

Uses the five default labels: `needs-triage`, `needs-info`, `ready-for-agent`, `ready-for-human`, `wontfix`. See `docs/agents/triage-labels.md`.

### Domain docs

Single-context: one `GLOSSARY.md` and `docs/adr/` at the repo root. See `docs/agents/domain.md`.

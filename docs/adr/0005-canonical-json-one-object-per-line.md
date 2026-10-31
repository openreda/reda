# Documents are canonical JSON, one object per line, sorted by UUID

Every Project file and Document is UTF-8 JSON written in a single canonical form: fixed key order, integers only (nm, millidegrees), tables grouped by object kind and sorted by UUID, each small object (Wire, Track, Via, Pad, label…) compacted onto one line and only container objects spread over several. The same data always serialises to the same bytes. We chose this so that git diffs and merges work at object granularity, as they do with S-expression formats, while every language, script and AI agent can read the files with a stock parser, and the same Rust types yield both the files and a published JSON Schema, as commands already do (ADR-0001).

## Considered Options

- **S-expressions (as KiCad uses):** compact, comma-free, one object per line by habit; rejected because we would own the parser and nothing standard reads it. The one-object-per-line canonical form recovers its diff behaviour.
- **Pretty-printed JSON, one field per line:** rejected; files several times longer and noisier diffs.
- **RON / TOML:** RON lacks tooling outside Rust; TOML handles deep nesting poorly.
- **Binary containers:** rejected for Documents; binary is allowed only in rebuildable caches under `.reda/`.

## Consequences

- Insertion order is not preserved; sorting by UUID scatters new objects so parallel branches rarely touch the same lines.
- Hand edits are accepted on read but normalised on the next save.
- Every file carries a `format` integer; older formats are migrated on load through a chain of raw-JSON steps that is never pruned, and files from a newer REDA or with unknown fields are refused.

# v0.1 testing, CI and release

## Testing

Source: [How is REDA tested?](https://github.com/openreda/reda/issues/29).

- **Command-level scenario tests:**
  - Most behaviour tests run on a headless engine. They drive the Command registry the way MCP and the CLI do, and assert on Command results and saved Documents, not on model internals.
  - The model is never mocked.
- **Unit tests** live inside crates, for `reda-geom`, `reda-text`, parsers and output writers.
- **Deterministic engine:**
  - The engine takes an injected `IdSource` and `Clock`, never globals.
  - Tests use a seeded counter and a fixed epoch.
- **Scenarios:**
  - **Rust scenarios** use a small helper API, for example `engine.run("pcb.track.add", json!{…})`.
  - **Data-driven scenarios** are `.jsonl` scripts in the Journal format under `tests/scenarios/`, each with golden copies of the resulting Documents beside it. One harness runs every script, and replay remaps UUIDs.
  - A bug report can attach an exported Journal, and the fix promotes it into `tests/scenarios/`.
- **`reda-testkit`:**
  - A dev-only crate holding `assert_golden(path, bytes)` (rewritten with `REDA_BLESS=1`), the test-engine builder and the scenario harness.
  - It does not use `insta`.
- **Manufacturing outputs:**
  - Byte-exact goldens of the Gerber, Excellon, BOM/CPL and PDF for each reference Project, with a fixed `Clock`.
  - A Gerber read-back test parses REDA's output with a permissive reader (`gerber_parser` if it fits, otherwise a minimal in-house reader for the subset REDA writes) and compares the geometry with the model.
  - No GPL tools, even in CI.
- **Rendering:**
  - **Scene snapshots:** the main snapshot is the Scene serialised as text, run on every platform.
  - **PNG snapshots:**
    - A small set of `document.render` PNG snapshots runs on Linux CI only, using Mesa's software renderer, with a per-pixel tolerance.
    - A failure uploads the image diff as an artifact.
- **UI tests (`egui_kittest`):**
  - Scope:
    - the shell: docking and Layout Commands, and menus and toolbars that follow the active Document
    - Tool state machines driven by synthetic pointer and key input
  - Assertions:
    - They assert on the Commands that land in the Journal (a drag gives exactly one `pcb.track.add {…}`) and on the AccessKit tree.
    - Image snapshots are only for the theme and icons, and run on Linux only.
- **Property tests and fuzzing:**
  - `proptest` covers:
    - geometry (clearance, polygon ops)
    - save → load → save identity of every Document type
    - format migrations
  - `cargo-fuzz` covers file loading and KiCad import. It runs as a weekly scheduled job once import lands, and never gates a merge.
- **Reference Projects** under `tests/projects/`:
  - a minimal board
  - a 555 blinky
  - a USB-C MCU board with Zones and Footprints on both sides

  They drive the ERC, DRC, Sync and output goldens end to end. Each one is first built by a script in `tests/scenarios/`, then kept as files. On every `format` bump, an old-format copy of each is frozen to test the migration chain.
- **Generated artefacts checked in and kept fresh:**
  - `docs/commands.json`
  - the JSON Schema per file kind
  - the dependency-edge allow-list
- **Performance:** no gates in v0.1. A few `#[ignore]` timing tests on the reference Projects can be run by hand.

## CI and merge gate

- **CI jobs:**
  - **Ubuntu, Windows, macOS:** `cargo fmt --check`, `cargo clippy -D warnings` and `cargo test --workspace`, on the pinned toolchain only.
  - **Linux only:**
    - the wasm32 build of the engine crates
    - freshness of `docs/commands.json` and the format JSON Schema
    - `cargo-deny` for licences and advisories, which enforces ADR-0002
    - the PNG snapshots
- **Merge gate:**
  - Agents work on branches and open PRs. Nothing is pushed straight to `main`.
  - Every CI job must be green on all three OSes.
  - Every blessed golden diff is explained in the PR body and reviewed by the maintainer.
  - A **Command coverage test** fails if any Command id in `docs/commands.json`, or any Command the app registers itself, is not run by at least one test. There is no line-coverage threshold.

## Packaging

Sources: [How is REDA packaged, distributed and updated?](https://github.com/openreda/reda/issues/30), [packaging research](https://github.com/openreda/reda/issues/24).

- **Unsigned in v0.1.** It is a tester release. The pipeline leaves slots for signing and notarization, which come for v1.0.
- **Formats, built by `cargo-packager` in CI:**
  - **macOS:** a `.dmg` with `REDA.app`, arm64 only. Nothing should prevent an Intel build.
  - **Windows:** a per-user NSIS `.exe` with no admin prompt, x86_64.
  - **Linux:** an AppImage and a `.deb`, for x86_64 and aarch64. They are built on Ubuntu 22.04 runners, which keeps the glibc floor low enough for Debian 12 and Ubuntu 22.04.
  - A plain `.tar.gz` or `.zip` archive per OS.
- **Distribution:**
  - GitHub Releases on `openreda/reda` only, with `SHA256SUMS` attached.
  - A rolling `latest` release, with version-less asset names, gives `openreda.com` a stable link.
  - 0.x releases are normal releases, not pre-releases.
  - The README and release notes say the builds are early and unsigned, and explain how to get past Gatekeeper and SmartScreen.
- **Updates:**
  - There is no self-updater.
  - `app.update.check` (a query Command, under Help → Check for Updates) asks the GitHub Releases API only when the user runs it, and links to any newer release.
  - REDA makes no network calls on its own: no check at startup and no telemetry.
- **File types:**
  - `.reda`, `.redasch` and `.redapcb` are registered with their own icons: macOS UTIs, Windows ProgIDs, and freedesktop MIME XML that REDA ships.
  - Opening a `.redasch` or `.redapcb` opens its Project and focuses that Document's tab. If a running instance has that Project open, the file is passed to it over the bridge. Otherwise a new instance starts.
- **The `reda` CLI** ships in the same package:
  - `.deb`: installed to `/usr/bin`.
  - NSIS: an optional "Add to PATH" checkbox.
  - macOS: kept inside the `.app`, with an `app.cli.install` Command that symlinks it into `/usr/local/bin`.
  - AppImage and archives: the binary sits beside the app.

## Releases

- **Releases are started by hand for now:** merges to `main` don't publish. Run the Release workflow with the chosen bump when a release is ready. Restoring the workflow's `push` trigger makes every merge to `main` release again, and the rules below for labels and markers then apply.
- **A release:**
  - The run does CI, builds every package, and publishes an immutable `vX.Y.Z` release (marked Latest) plus the rolling `latest`.
  - Release notes come from PR titles. There is no CHANGELOG.
- **Skipping and forcing, once automatic:**
  - `release:skip` or `[skip release]` skips a release.
  - `workflow_dispatch` can force a bump.
- **Version bumps while the version is 0.x:**
  - Every release bumps the patch version.
  - `release:minor` is applied by hand when a milestone is done, so finishing v0.1 releases `0.1.0`.
  - `release:major` means 1.0.
- **Version numbers:**
  - The release tag is the source of truth. CI stamps it into the workspace `version` before building, without committing it.
  - Local builds report the Cargo version plus `-dev` and the git hash, in About, `app.update.check` and `reda --version`.
- The release workflow lands together with the first `reda-app` that opens a window.

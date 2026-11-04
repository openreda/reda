# REDA

REDA is an MIT-licensed electronic design automation (EDA) tool written in Rust. In one window you capture a circuit on Schematic Sheets, lay out the PCB that realises it, and produce the files a fab needs to make and assemble the board. The same Commands drive the GUI, the `reda` command-line tool and MCP, so AI agents can work on a design too.

REDA is at an early stage. Expect missing features, rough edges and file-format changes between releases.

Website: **<https://openreda.github.io/reda/>**

## Screenshots

The app draws these itself from the newest `main`, so they always show REDA as it is today. More, in the light theme too, and the screenshots of every release are on the [website](https://openreda.github.io/reda/).

![A 555 LED blinker on a Schematic Sheet](https://openreda.github.io/reda/screenshots/latest/schematic-dark.png)

![The 555 blinker's 2-layer board, with its ground Zone filled](https://openreda.github.io/reda/screenshots/latest/pcb-dark.png)

![An Output Job that wrote Gerber, drill, BOM and CPL files](https://openreda.github.io/reda/screenshots/latest/output-job-dark.png)

## Download

Releases are published by hand on [GitHub Releases](https://github.com/openreda/reda/releases). The [`latest`](https://github.com/openreda/reda/releases/tag/latest) release always holds the newest build, with the version left out of the file names.

| System | File |
|---|---|
| macOS (Apple Silicon) | `REDA-<version>-macos-arm64.dmg` |
| Windows (x86_64) | `REDA-<version>-windows-x86_64-setup.exe`, a per-user installer that needs no administrator rights |
| Linux (x86_64, aarch64) | `.deb` for Debian 12, Ubuntu 22.04 and newer, or an `.AppImage` |
| Any of the above | a `.tar.gz` or `.zip` archive with the app and the `reda` CLI |

`SHA256SUMS` lists the SHA-256 checksum of every file. To check a download, run `sha256sum --check --ignore-missing SHA256SUMS` (or `shasum -a 256 -c --ignore-missing SHA256SUMS` on macOS) in the folder that holds both.

REDA makes no network calls on its own: there is no telemetry and no automatic update check, and there is no self-updater. Download a new release to update.

## Installing an unsigned build

The early builds are **not signed** yet. Code signing and notarization come with v1.0. Until then, macOS and Windows warn you before the first launch. Only bypass the warning for files you downloaded from this repository's releases, and compare their checksums with `SHA256SUMS` if in doubt.

**macOS (Gatekeeper).** Open the `.dmg` and drag REDA to Applications. The first time you open it, macOS says it cannot verify that REDA is free of malware. Then:

1. Click **Done** (not Move to Trash).
2. Open **System Settings → Privacy & Security**, scroll to the message about REDA, and click **Open Anyway**.
3. Confirm with your password. After this, REDA opens normally.

On the command line you can instead remove the quarantine flag: `xattr -dr com.apple.quarantine /Applications/REDA.app`.

**Windows (SmartScreen).** When you run the installer, Microsoft Defender SmartScreen may say "Windows protected your PC". Click **More info**, then **Run anyway**. The installer puts REDA in your user folder, so it needs no administrator rights. Tick **Add the reda command to PATH** on the Components page to use the CLI from any terminal.

**Linux.** No warning appears. Install the `.deb` with `sudo apt install ./REDA-<version>-linux-<arch>.deb`, or make the AppImage executable (`chmod +x`) and run it.

## The `reda` CLI

Every package ships the `reda` command-line tool beside the app:

- `.deb`: installed to `/usr/bin/reda`.
- Windows: in the install folder (`%LOCALAPPDATA%\REDA`), and on your PATH if you ticked the option.
- macOS: inside the app, at `/Applications/REDA.app/Contents/MacOS/reda`.
- AppImage and archives: next to the app.

Run `reda` with no arguments to list its commands; `reda mcp` serves REDA to MCP clients.

## Building from source

Install Rust with [rustup](https://rustup.rs); `rust-toolchain.toml` pins the toolchain. Then:

```sh
cargo run -p reda-app              # the app
cargo run -p reda-cli -- commands  # the CLI: list every Command
cargo test --workspace
```

Local builds report their version as `<version>-dev+<git hash>`.

## Documentation

- [The website](https://openreda.github.io/reda/): what REDA does, with screenshots of today's app and of every release. Its source is [`site/`](site/), built by `cargo xtask showcase` and `cargo xtask site`.
- [`docs/spec/`](docs/spec/README.md): the product spec, and what each milestone builds.
- [`docs/adr/`](docs/adr/): architecture decisions.
- [`GLOSSARY.md`](GLOSSARY.md): the domain vocabulary.
- [`AGENTS.md`](AGENTS.md): how the repository is organised.

## License

REDA is MIT licensed; see [LICENSE](LICENSE).

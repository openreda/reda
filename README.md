# REDA

REDA is a free and open-source electronic design automation (EDA) tool written in Rust. In one window you capture a circuit on Schematic Sheets, lay out the PCB that realises it, and produce the files a fab needs to make and assemble the board. The same Commands drive the GUI, the `reda` command-line tool and MCP, so AI agents can work on a design too.

REDA is at an early stage. Expect missing features, rough edges and file-format changes between releases.

The public source code and releases live at **<https://github.com/openreda/openreda>**, under the GNU General Public License, version 2 or later. See [Licence and repositories](#licence-and-repositories) for how development works until v1.0.

Website: **<https://openreda.github.io/reda/>**

## Screenshots

The app draws these itself from the newest `main`, so they always show REDA as it is today. More, in the light theme too, and the screenshots of every release are on the [website](https://openreda.github.io/reda/).

![A 555 LED blinker on a Schematic Sheet](https://openreda.github.io/reda/screenshots/latest/schematic-dark.png)

![The 555 blinker's 2-layer board, with its ground Zone filled](https://openreda.github.io/reda/screenshots/latest/pcb-dark.png)

![An Output Job that wrote Gerber, drill, BOM and CPL files](https://openreda.github.io/reda/screenshots/latest/output-job-dark.png)

## Download

Releases are published by hand on [GitHub Releases](https://github.com/openreda/openreda/releases). The [`latest`](https://github.com/openreda/openreda/releases/tag/latest) release always holds the newest build, with the version left out of the file names.

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

## Licence and repositories

**Public releases are GPL.** The source code published at [openreda/openreda](https://github.com/openreda/openreda), and every release built from it, is licensed under the GNU General Public License, version 2 or (at your option) any later version (`GPL-2.0-or-later`). The full text is in [LICENSE](LICENSE). The Starter Library in [`library/`](library/), the symbols, footprints, parts and board templates that ship with REDA, stays under the MIT License (see [library/LICENSE](library/LICENSE)), so you can use and adapt its parts without the GPL applying. The GPL covers REDA itself, not your work: schematics, boards and fab outputs you make with REDA are yours, under whatever terms you choose.

**Development happens in a private repository.** Milestones are planned and built in `openreda/reda`, which is not public. Until REDA is ready for general use at v1.0, we publish source releases from it to [openreda/openreda](https://github.com/openreda/openreda) as development goes along. The public repository's history may be squashed or rewritten between source releases.

**Contributions are accepted under MIT.** Pull requests to openreda/openreda are welcome, but by opening one you agree to license your contribution under the [MIT License](CONTRIBUTING.md#licence-of-contributions), not the GPL. We keep the right to maintain our own internal version of REDA that is not under the GPL, and MIT-licensed contributions let us carry your change into it as well as into the GPL releases. See [CONTRIBUTING.md](CONTRIBUTING.md) for the full terms.

### Why GPL now, and MIT later

Our goal is for REDA to be MIT licensed. We are not there yet because of when REDA is being built. With AI tools, a fork of an open project can be taken, extended and sold as a paid product very quickly. During early, active development, before REDA has established itself, an MIT release would let anyone do that with the work done so far. The GPL is our protection in the meantime: anyone may use, study, change and share REDA, but anyone who distributes a version of it must publish their source under the same terms.

The split is not a plan for a closed edition. We do not intend to release a closed-source REDA with extra features. The GPL release is where our work goes. We keep the non-GPL internal version, and take contributions under MIT, so that we can move the whole project, contributions included, to MIT without having to track down every contributor. When REDA has been around long enough and is established, we intend to switch the public releases to the MIT License.

Third-party code, fonts and data that ship with REDA keep their own licences; they are listed in the `THIRD_PARTY_NOTICES` file in every release and in the app's About box.

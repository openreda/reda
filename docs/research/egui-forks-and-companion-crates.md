# Do we need forks of egui/winit, and which egui companion crates should we depend on?

Research for issue #2. Researched 2026-10-31 against egui 0.37.0 and winit 0.30.13 (the current
releases; egui 0.37 depends on winit `^0.30.13`).

## Answer

**Stay on upstream egui and winit. Do not fork now.** Upstream already has the features REDA
needs first: native multi-window viewports, menu bars, modals, keyboard shortcuts with a fallback
for non-Latin layouts, and AccessKit. A production Rust/egui app family that does carry egui and
winit forks patches **text input and internationalisation** (IME, bidirectional/RTL text,
shortcuts on non-Latin layouts, Wayland clipboard) and some platform quirks. It does **not** patch
multi-viewport tear-off or rendering for CAD-style canvases. REDA can wait until one of those gaps
actually affects it.

**For companion crates the answer is (c) with selective (a): write REDA's own keymap, menu
model, settings and window-management layer on upstream egui, and depend on small, published,
permissively licensed leaf crates** (`rfd`, `muda`, `directories`, `toml_edit`, a docking crate,
`egui_kittest`). Do not take git dependencies on the existing app family's shared UI library
crates. Use them as reference designs, and copy small pieces with attribution only when one
clearly saves work (option b, case by case).

Revisit if any trigger in [When to fork](#when-to-fork) fires.

## 1. What upstream egui/winit already gives us

| Need | Upstream status (egui 0.37.0) | Source |
|---|---|---|
| Multiple native windows (tear-off panels, second monitor) | Supported by native `eframe`: `Context::show_viewport_deferred` / `show_viewport_immediate`, a viewport tree, and `ViewportCommand` to move, resize and retitle windows. Not supported on web, where viewports fall back to embedded `egui::Window`s. | [`crates/egui/src/viewport.rs` module docs @0.37.0](https://github.com/emilk/egui/blob/0.37.0/crates/egui/src/viewport.rs); [multiple_viewports example](https://github.com/emilk/egui/tree/0.37.0/examples/multiple_viewports) |
| Menu bar, context menus | `egui::MenuBar`, `egui::menu` | [`crates/egui/src/lib.rs` re-exports @0.37.0](https://github.com/emilk/egui/blob/0.37.0/crates/egui/src/lib.rs) |
| Modal dialogs | `egui::Modal`, `ModalResponse` | same |
| Keyboard shortcuts | `KeyboardShortcut`, `ModifierNames`, `InputState::consume_shortcut` | same |
| Shortcuts on non-Latin layouts | egui-winit uses the logical key and falls back to the physical key ("a fallback mechanism for keyboard layouts without Latin characters"), and `Event::Key` carries `physical_key` | [`crates/egui-winit/src/lib.rs` `on_keyboard_input` @0.37.0](https://github.com/emilk/egui/blob/0.37.0/crates/egui-winit/src/lib.rs); [emilk/egui#3653](https://github.com/emilk/egui/issues/3653) |
| Accessibility | AccessKit is re-exported and wired in | `lib.rs` above; [accesskit](https://crates.io/crates/accesskit) |
| Headless UI tests / snapshots | `egui_kittest` 0.37 | [crates.io](https://crates.io/crates/egui_kittest) |

Platform limits that a fork would not remove: on Wayland a client cannot set its own window
position, and egui's web backend has no real multi-viewport support.

## 2. What an existing production egui app family patches

An app family of about a dozen MIT OR Apache-2.0 Rust/egui desktop apps (one of them a CAD
drafting app) maintains forks of egui and winit and pins every egui-workspace crate and winit to
them with `[patch.crates-io]`. Each fork keeps a `PATCHES.md` that lists every patch, its upstream
PR status and when to drop it. Both forks were inspected at the pinned revisions with
`gh api repos/<fork>/compare/...` against upstream. The repositories are not named here, per
project policy.

**egui fork**: based on upstream tag `0.37.0` (`841a328b`). At the pinned revision it is 90
commits ahead of and 8 behind upstream `main`, with 21 numbered patches (E1–E21) plus uncommitted
"local follow-ups":

| Area | Patches | Upstream status (per its PATCHES.md) |
|---|---|---|
| Bidi / RTL text shaping, caret, selection, mirrored widget chrome, `LayoutJob::direction`, pluggable line breaker, grapheme-cluster editing, per-`Ui` language, AccessKit language | E5, E8, E9, E12–E17, E20 | E5 ports open PR [emilk/egui#8577](https://github.com/emilk/egui/pull/8577); the rest "not proposed" / "fork-only" |
| IME: focus not interrupting composition, delete exact replaced bytes, surrounding text, reset | E6, E21, local P7b | not proposed |
| Shortcuts on non-Latin layouts (Windows VK codes, first Latin group on Linux/macOS, web `KeyboardEvent.code`) | E7, E10, E19 | not proposed / fork-only until winit has the APIs |
| Wayland clipboard through winit's data device (avoids a second data device under Hyprland), PRIMARY selection, bounded async paste | E4, local P2b | draft [emilk/egui#8657](https://github.com/emilk/egui/pull/8657) |
| wgpu: fit oversized surfaces to `max_texture_dimension_2d`, return setup errors | E2+E3 | not proposed; would close [emilk/egui#8361](https://github.com/emilk/egui/issues/8361) |
| `ViewportHooks`: access to each viewport's native window and raw input (used for native Wayland pen input) | E1 | not proposed |
| vello_cpu glyph rasterisation with `multithreading` | E11 | draft [emilk/egui#8641](https://github.com/emilk/egui/pull/8641) |
| ScrollArea explicit-offset vs kinetic scroll after zoom; `TexturesDelta` drop while panicking | local | not proposed |

**winit fork**: based on upstream tag `v0.30.13` (`e9809ef5`), 37 commits ahead, with patches
W1–W27:

| Area | Patches | Upstream status |
|---|---|---|
| Windows 11 mixed-DPI drag (`WM_DPICHANGED` rect) | W1 | merged on master ([#4341](https://github.com/rust-windowing/winit/pull/4341)), not yet in a 0.30.x release |
| X11 scale factor from server DPI | W11 | to be proposed |
| Wayland smooth/tilt scroll reporting (Ctrl-zoom latching) | W13 | to be proposed |
| macOS title-bar drags, cancelable termination → close request | W12, W14 | to be proposed |
| Windows hidden maximized windows | W4 | open [#4587](https://github.com/rust-windowing/winit/pull/4587) |
| Wayland file drop and clipboard on a per-seat data device; PRIMARY selection; bounded transfers | W15, W15P, W15I | [#4729](https://github.com/rust-windowing/winit/pull/4729) open for 0.30; master has [#4571](https://github.com/rust-windowing/winit/pull/4571) |
| IME on every platform (Korean, Vietnamese, press-and-hold, dictation, emoji picker, `reset_ime`, surrounding text) | W2, W3, W5–W10, W18–W24, W26, W27 | several open upstream PRs; surrounding text and delete-surrounding are core API on winit master / 0.31 |
| Latin-layout key for shortcuts on non-Latin layouts; macOS ⌘-table key names | W23, W25 | to be proposed |

What this tells REDA:

- **No patch concerns multi-viewport tear-off or canvas rendering.** The family's shared UI crate
  moves floating panels into their own OS windows using upstream viewports (a `native-windows`
  feature). E1 adds native-window *hooks* for pen input. It does not change how viewports work.
- **Most of the patch volume is text-input and i18n work**, the kind needed by word processors,
  layout tools and multilingual UIs. REDA mostly edits short Latin-script fields (designators,
  net names, property values) on a shortcut-heavy canvas, so it is far less exposed to these gaps.
- **The patches that also apply to REDA** are platform quirks: mixed-DPI drags on Windows 11 (W1,
  already merged upstream), X11 DPI (W11), Wayland Ctrl-zoom latching (W13, which matters for an
  EDA canvas zoom), the Wayland clipboard on some compositors (E4/W15), and oversized wgpu
  surfaces on large or high-DPI monitors (E2+E3).
- **The cost is large.** The egui fork changes about 130 files, including about 10k lines in
  `egui-winit` and a 9.5k-line Wayland clipboard module in the winit fork. Its `PATCHES.md`
  records ongoing qualification work, a dedicated migration branch for each egui release, and a
  public API change (`egui_winit::process_viewport_commands` signature). Every crate in the
  dependency graph must patch the same egui revision, or two eguis resolve.

## 3. The app family's shared UI library crates

The family publishes its shared crates from one Git monorepo, licensed `MIT OR Apache-2.0`
(workspace `license` field; `LICENSE-MIT` and `LICENSE-APACHE` at the root). They are **not on
crates.io**: apps consume them as `git` + `rev` dependencies. The workspace pins egui `0.37` and
patches all egui crates and winit to the forks above.

| Crate (generic name) | What it does | Dependencies | Fit for REDA |
|---|---|---|---|
| keymap | Chords, bindings to command ids with JSON params, scopes, conflicts, user overrides, named sets, importing other apps' shortcut sets, an editor model, an optional Shortcuts dialog | serde, serde_json; optional egui and the shared UI/i18n crates | Close to what ADR-0001 needs. Its command reference (`id` + JSON params) resembles REDA's commands, but REDA's command registry and schema must be the source of truth. Compiles on crates.io egui, where it falls back to `None` for fork-only layout info (it uses an inherent-method-vs-trait trick). Good reference design. |
| menu | Platform-specific menu layouts (macOS/Windows/Linux/web), a native macOS backend on `muda`, key equivalents fed back to egui | egui, keymap, quit crate, muda, objc2 | Useful design, but coupled to its keymap and quit crates and their command type. |
| window | Decides who draws the window frame (desktop or app) per platform and compositor (Wayland `zxdg_decoration_manager_v1` probe), app-drawn title-bar behaviour, window list / cascade / Window menu | egui (optional), wayland-client, objc2 | Small and self-contained. A candidate to copy and adapt if REDA draws its own title bar. |
| settings | `config.toml` in platform folders, never-failing load, debounced atomic saves, backup, recovery, schema migration | its atomic-write crate, serde, toml_edit, dirs | Generic, good design. Easy to rewrite or copy. |
| dialogs | Non-blocking native file and message dialogs: queued, shown at end of frame, answered on a later frame (works around a macOS modal-loop re-entrancy abort in winit) | rfd, raw-window-handle, objc2 | The pattern matters for REDA, since ADR-0001 says commands with parameters never open dialogs, but GUI flows still will. Small. Copy or reimplement the pattern on `rfd`. |
| ui | Icon catalogs, tabs, toolbar fitting, split/resize, docking with transactional panel moves and native-window floating, fonts, printing | egui, epaint, skrifa, fontique, egui_kittest | Large and tied to its own design language and i18n crate. Reference only. |

Costs of depending on them directly:

- **Git-only dependencies** pinned by `rev`, with no semver. `cargo-deny` needs a git-source
  allowance (ADR-0002).
- **egui lock-step.** All of them track the family's egui version. Upstream egui ships a breaking
  minor release roughly every 1–2 months (0.34.3 2026-05-27, 0.35.0 06-25, 0.36.0 08-05, 0.37.0
  10-15), so REDA could not upgrade egui until they do.
- **Pressure to adopt the forks.** Their tests and some behaviour assume fork-only egui APIs, so
  REDA would get the upstream-fallback behaviour unless it also patched to the forks.
- **Coupled cluster.** keymap → ui → i18n → fonts-build, and menu → keymap + quit. Taking one
  pulls in the others.
- **Licence notices.** Fine under ADR-0002, but REDA's distribution must carry their MIT notice
  (or Apache-2.0 NOTICE).

## 4. Published companion crates on crates.io

Licence, latest version and the egui version required, from the crates.io API on 2026-10-31.

| Crate | Version | Licence | egui req | Use for REDA |
|---|---|---|---|---|
| [`rfd`](https://crates.io/crates/rfd) | 0.17.2 | MIT | none | **Depend.** Native file and message dialogs (also used by the shared dialogs crate). Wrap it in REDA's own non-blocking queue. |
| [`muda`](https://crates.io/crates/muda) | 0.21.2 | Apache-2.0 OR MIT | none | **Depend (macOS)** for a native global menu bar. Draw egui `MenuBar` elsewhere. |
| [`directories`](https://crates.io/crates/directories) | 6.0.0 | MIT OR Apache-2.0 | none | **Depend** for config/data folders. |
| `toml_edit` + `serde` | – | MIT OR Apache-2.0 | none | **Depend** for format-preserving settings files. |
| [`egui_tiles`](https://crates.io/crates/egui_tiles) | 0.18.0 | MIT OR Apache-2.0 | ^0.37.0 | **Candidate** for docking (tracks egui releases quickly). Evaluate against `egui_dock` in a prototype. |
| [`egui_dock`](https://crates.io/crates/egui_dock) | 0.21.1 | MIT | ^0.36 | **Candidate.** Still on egui 0.36; native multi-viewport support is an open issue since 2023 ([#202](https://github.com/anhosh/egui_dock/issues/202)). |
| [`egui_extras`](https://crates.io/crates/egui_extras), [`egui_kittest`](https://crates.io/crates/egui_kittest), [`egui_plot`](https://crates.io/crates/egui_plot) | 0.37 / 0.38 | MIT OR Apache-2.0 | ^0.37.0 | **Depend** as needed (tables/images, UI tests, plots). These are first-party and released with egui. |
| [`egui_ltreeview`](https://crates.io/crates/egui_ltreeview) | 0.10.0 | MIT | ^0.37 | Candidate for the Project tree. |
| [`egui-notify`](https://crates.io/crates/egui-notify) | 0.24.0 | MIT | ^0.37 | Optional toasts. Trivial to write. |
| [`keyboard-types`](https://crates.io/crates/keyboard-types) | 0.8.3 | MIT OR Apache-2.0 | none | Optional serialisable key vocabulary for the keymap. |
| [`egui-file-dialog`](https://crates.io/crates/egui-file-dialog), [`egui_file`](https://crates.io/crates/egui_file) | 0.15 / 0.28 | MIT | ^0.36 | Only for an in-egui dialog (e.g. web). Prefer `rfd` natively. |
| [`egui-keybind`](https://crates.io/crates/egui-keybind) | 0.8.0 | Unlicense OR MIT OR BSL-1.0 | ^0.33 | **No.** Stale, and only a capture widget. |
| [`egui_keybinds`](https://crates.io/crates/egui_keybinds) | 3.3.8 | **GPL-3.0** | ^0.21 | **No.** GPL is barred by ADR-0002. |
| [`egui-modal`](https://crates.io/crates/egui-modal) | 0.6.0 | MIT | ^0.30 | **No.** Superseded by `egui::Modal`. |

No published crate gives a full keymap, menu model, settings store and window manager tied to a
command registry. Every option in that space is either an app's private code or a narrow widget.
REDA's keymap and menus derive from its own command registry (ADR-0001), so these are REDA's to
write.

Note: `winit` itself is licensed **Apache-2.0** only (crates.io metadata), and egui crates are
MIT OR Apache-2.0. Both are allowed by ADR-0002, but REDA's notices file must include Apache-2.0
attribution.

## 5. Recommendation and costs

| Option | Cost | Verdict |
|---|---|---|
| (a) Depend on the shared library crates and their forks | Git-only deps, egui lock-step with another project's cadence, inheriting about 50 fork patches mostly irrelevant to EDA, a coupled crate cluster, and their command type versus REDA's | Reject |
| (b) Copy and adapt selected pieces | One-off effort; keep the `MIT OR Apache-2.0` notice on copied files; no ongoing coupling | Use case by case: the window-frame decision rule, the non-blocking dialog queue, and settings recovery/migration are the best candidates |
| (c) Write our own on upstream egui, with published leaf crates | Write keymap, menu model, settings and window bookkeeping (moderate; they must follow ADR-0001's command registry anyway) | **Default** |

Concretely:

1. Workspace deps: `egui`/`eframe` 0.37 from crates.io, no `[patch]`.
2. Build `keymap` and `menu` on the command registry: bindings map chords to command ids and
   parameters, egui `MenuBar` draws menus in the window, and `muda` provides the macOS menu bar.
   Match shortcuts on the logical key with the physical key as fallback, as upstream egui-winit
   does.
3. Settings: `serde` + `toml_edit` + `directories`, with atomic writes (write temp, fsync,
   rename).
4. Dialogs: `rfd` behind a small queue that never blocks inside a winit callback.
5. Docking and tear-off: prototype `egui_tiles` versus `egui_dock` on upstream viewports before
   choosing.

## When to fork

Fork (or carry a `[patch]` to a branch) only when **all** of these hold:

1. REDA hits a concrete bug that users notice.
2. An upstream fix exists or is drafted, or a small patch is clearly upstreamable.
3. It cannot be worked around in REDA's code.

Likely first candidates, in order:

- Wayland Ctrl-zoom latching (winit W13)
- Windows 11 mixed-DPI window growth (winit W1; fixed on master, so wait for 0.31)
- Oversized wgpu surfaces on large or HiDPI multi-monitor setups (egui E2+E3)
- Wayland clipboard on wlroots/Hyprland (E4/W15)

If REDA forks, follow the reference family's discipline: one branch per patch off a release tag,
a `PATCHES.md` listing upstream status and a "remove when" condition per patch, and patch every
egui-workspace crate to one revision. ADR-0003 already allows forks to live outside the monorepo.

winit 0.31 is in beta (`v0.31.0-beta.4`, 2026-10-24). It brings IME surrounding text,
delete-surrounding and Wayland data-device changes into core, and will remove several of the
reference fork's reasons to exist once egui moves to it.

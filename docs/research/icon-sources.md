# Icon sources and icon rendering for REDA's toolbars and Active Bar

Research for issue #20. Surveyed on 2026-11-01 against the icon sets' own licence files and published packages (npm tarballs, unpacked and counted locally), the GitHub API (licence, activity), crates.io, the egui 0.37 / egui_extras 0.37 / usvg 0.45 source in the Cargo registry, and a size probe built locally. Checked against ADR-0002 (MIT, Apache-2.0, BSD, zlib, ISC allowed; MPL-2.0 case by case; no GPL/LGPL, no copyleft data bundled) and the AGENTS.md dependency policy.

## Summary

| Question | Recommendation |
|---|---|
| General UI icons | **Lucide** (ISC, plus MIT for the Feather-derived subset). Vendor only the SVGs REDA uses into the repo. |
| Fallback set | **Tabler Icons** (MIT): same 24 px grid, 2 px stroke and round caps as Lucide, so single icons can fill gaps. |
| EDA icons (Wire, Net Label, Port, Power Port, Track, Via, Zone, Keepout, Footprint, ...) | **REDA draws its own** SVGs, MIT, in the repo, following Lucide's published design rules. No permissive set covers them, and the open EDA tools all draw their own. |
| Rendering | **SVG through `egui_extras`' `svg` feature** (resvg), embedded with `include_image!`. Rewrite `currentColor` to white when vendoring, so egui's tint can recolour icons for theme, hover and selection. Use the painter only for the few icons that show live state. |
| Icon font | Not recommended: custom EDA glyphs would need a font build step, and the egui icon-font crates lag egui releases or ship whole fonts. |

## 1. General icon sets

Counts are SVG files in the published npm package, in one style (Lucide `icons/`, Tabler `icons/outline`, Phosphor `assets/regular`, Material Symbols `outlined`).

| Set | Licence (primary source) | Icons (one style) | Latest release | Style | Avg SVG |
|---|---|---|---|---|---|
| Lucide | ISC; the Feather-derived icons are MIT ([LICENSE](https://github.com/lucide-icons/lucide/blob/main/LICENSE)) | 2,292 (`lucide-static` 1.66.0) | 1.66.0, 2026-10-31 | 24×24, 2 px stroke, round caps/joins, `stroke="currentColor"` | 481 B |
| Tabler Icons | MIT (GitHub licence: MIT; `package.json`) | 5,203 outline + 1,054 filled (`@tabler/icons` 3.50.0) | 3.50.0, 2026-10-27 | 24×24, 2 px stroke, round caps/joins, `currentColor` | 559 B |
| Phosphor | MIT ([phosphor-icons/core](https://github.com/phosphor-icons/core)) | 1,512 × 6 weights (`@phosphor-icons/core` 2.1.1) | v2.0.8 tag 2024-02-01 (npm 2.1.1 newer) | 256×256 viewBox, filled outlines, `fill="currentColor"` | 505 B |
| Material Symbols | Apache-2.0 ([google/material-design-icons](https://github.com/google/material-design-icons)) | 7,874 per style (npm `@material-symbols/svg-400` 0.48.2, a third-party repackaging) | Repo pushed 2026-10-23 | 960 viewBox, filled shapes, no colour attribute (renders black) | 489 B |

Excluded under ADR-0002:

- **Remix Icon**: since January 2026 under its own "Remix Icon License v1.0", which permits distribution only "as part of a larger product" where the icons "are not marketed or sold as the primary value" ([License](https://github.com/Remix-Design/RemixIcon/blob/master/License)). Not one of ADR-0002's licences.
- **Pictogrammers Material Design Icons** (`@mdi/svg`): "Some of the icons are redistributed under the Apache 2.0 license. All other icons are either redistributed under their respective licenses or are distributed under the Apache 2.0 license" ([LICENSE](https://github.com/Templarian/MaterialDesign/blob/master/LICENSE)). Per-icon provenance is not stated, so it cannot be cleared wholesale. It does have `resistor`, `integrated-circuit-chip`, `vector-polyline` and `led-*`.
- **VS Code Codicons**: CC-BY-4.0 (GitHub licence field). Not on ADR-0002's list.

Coverage of general actions (each column is the closest name in that set; all four cover everything REDA needs):

| Action | Lucide | Tabler | Phosphor | Material Symbols |
|---|---|---|---|---|
| Save / Undo / Redo | save, undo, redo | device-floppy, arrow-back-up, arrow-forward-up | floppy-disk, arrow-counter-clockwise, arrow-clockwise | save, undo, redo |
| Select | mouse-pointer | pointer | cursor | arrow_selector_tool |
| Layers | layers | stack | stack | layers |
| Panel toggle | panel-left | layout-sidebar | sidebar | left_panel_open |
| Zoom in / Zoom to fit | zoom-in, scan | zoom-in, scan | magnifying-glass-plus, scan | zoom_in, scan |
| Copy / Paste / Cut / Delete | copy, clipboard-paste, scissors, trash-2 | copy, clipboard, scissors, trash | copy, clipboard, scissors, trash | content_copy, content_paste, content_cut, delete |
| Rotate / Flip / Align | rotate-cw, flip-horizontal, align-start-vertical | rotate-clockwise, flip-horizontal, layout-align-left | arrow-clockwise, flip-horizontal, align-left | rotate_right, flip, align_horizontal_left |
| Grid / Snap / Measure | grid-3x3, magnet, ruler | grid-3x3, magnet, ruler | grid-four, magnet, ruler | grid_on, (none), straighten |
| Show / Hide / Lock | eye, eye-off, lock | eye, eye-off, lock | eye, eye-slash, lock | visibility, visibility_off, lock |
| Run check / OK / Warning | play, circle-check, triangle-alert | player-play, circle-check, alert-triangle | play, check-circle, warning | play_arrow, check_circle, warning |
| Text / Polyline | type, spline | typography, vector-spline | text-t, bezier-curve | title, polyline |

Why Lucide:

- Its licence is ISC, and the Feather-derived icons it lists in its LICENSE are MIT. Both are on ADR-0002's list; REDA must ship both notices.
- It is the most active of the four (release 1.66.0 the day before this survey; 25k GitHub stars).
- Its stroke style suits a CAD toolbar: thin, even line weight that stays legible at 16–20 px, and is easy to match when REDA draws its own EDA icons. The rules are published: 24×24 canvas, 1 px padding, uniform 2 px stroke, round caps and joins, 2 px corner radius on shapes of 8 px or more, at least 2 px between elements ([icon design guide](https://lucide.dev/contribute/icon-design-guide)).
- Tabler uses the same canvas, stroke and caps (visible in its SVG attributes), so a Tabler icon can fill a gap without looking foreign. Tabler has more than twice as many icons, which is why it is the fallback rather than a second primary set.
- Phosphor (filled outlines on a 256 grid) and Material Symbols (filled shapes) are a different visual language; mixing either with self-drawn stroke icons would show.

## 2. EDA-specific icons

No permissive set covers EDA editing actions. Matches found by name search in the four sets above:

- Tabler has 19 `circuit-*` icons that are schematic *component* symbols: resistor, capacitor (plain and polarised), inductor, diode, Zener diode, ground, digital ground, battery, cell, switch (open, closed, changeover), push-button, bulb, motor, ammeter, voltmeter. They could serve for placing common Parts, but not for Wire, Net Label, Port, Track, Via, Zone, Keepout or Footprint.
- Lucide has `circuit-board`, `microchip`, `cable`, `plug`, `route`, `waypoints`, `spline`, `ruler-dimension-line` and `square-dashed`, which are good starting points (Board, IC/Part, routing, dimension, selection box), but nothing for Net Label, Power Port, Via, Zone or Keepout.
- Phosphor and Material Symbols have only generic electronics (cpu, memory, plug, battery). Neither has "pcb", "via" or "track" icons.

How the open EDA tools source theirs (all of them draw their own; none of the results can be copied into REDA):

- **KiCad** keeps its own icon SVG sources in two hand-drawn variants, `resources/bitmaps_png/sources/light` and `.../dark`, rather than recolouring one set. The CREDITS file there gives "License: CC-BY-SA 4.0" ([CREDITS](https://gitlab.com/kicad/code/kicad/-/raw/master/resources/bitmaps_png/sources/CREDITS)). That is copyleft, so excluded by ADR-0002.
- **Horizon EDA** (GPL-3.0) draws its own GNOME-style "symbolic" SVGs: `action-draw-net-symbolic.svg`, `action-place-net-label-symbolic.svg`, `action-place-power-symbol-symbolic.svg`, `action-place-via-symbolic.svg`, `action-route-track-symbolic.svg` and similar ([src/icons/scalable/actions](https://github.com/horizon-eda/horizon/tree/master/src/icons/scalable/actions)). Symbolic icons are single-colour and recoloured by the toolkit, which is the same approach recommended here.
- **LibrePCB** (GPL-3.0) takes generic icons from Font Awesome and Bootstrap Icons and self-draws the EDA ones (`add_smt_pad`, `add_tht_pad`, `draw_bus`, `wire_45_90`, `draw-trace.svg`, ...). Its image rules ask for monochrome black icons that are "recolored dynamically in the UI" ([img/README.md](https://github.com/LibrePCB/LibrePCB/blob/master/img/README.md)).

Recommendation: REDA draws its own EDA icons as SVGs that follow Lucide's rules, licensed MIT with the rest of the repo. The set for the first Active Bars is small (about 15 to 25: Wire, Bus, Net Label, Port, Power Port, Place Part, Junction, No-Connect, Track, Via, Zone, Keepout, Footprint, Pad, Dimension). Most are simple geometry on a 24 px grid. Lucide and Tabler icons may be modified (ISC and MIT allow it), so a REDA icon may start from one, keeping its notice.

## 3. Rendering in egui 0.37

### 3.1 SVG through `egui_extras` (resvg)

How it works, from the 0.37 source:

- `egui_extras::install_image_loaders(ctx)` registers an `SvgLoader` when the `svg` feature is on (`egui_extras/src/loaders.rs`). `svg = ["resvg"]`; text in SVGs (`svg_text`) and embedded rasters (`svg_images`) are separate features REDA does not need.
- `egui::include_image!("icons/wire.svg")` embeds the bytes in the binary and gives them a `bytes://` URI (`egui/src/lib.rs`).
- **Crisp at any scale.** `Image` computes a `SizeHint` from the widget's point size multiplied by `ctx.pixels_per_point()` (`egui/src/widgets/image.rs`, `ImageSize::hint`). The SVG loader rasterises at exactly that pixel size (`egui_extras/src/image.rs`, `load_svg_bytes_with_size`) and caches one texture per URI and `SizeHint` (`svg_loader.rs`). A DPI change or a different icon size produces a fresh raster, not a scaled bitmap.
- **Tinting.** `Image::tint` multiplies the texture colour (`image.rs`: "Multiply image color with this. Default is WHITE"). In 0.37, `Button::image_tint_follows_text_color(true)` tints an untinted image with the widget text colour, so the icon follows the theme and the hover/active state (`egui/src/widgets/button.rs`).
- **Disabled state** is free: `Ui::disable()` multiplies the painter's opacity by `visuals.disabled_alpha()` for everything in that Ui, images included (`egui/src/ui.rs`).
- **The pitfall: `currentColor`.** Lucide and Tabler SVGs draw with `stroke="currentColor"`. usvg resolves a missing colour to black (`usvg/src/parser/style.rs`, `Color::black()`), and black multiplied by any tint stays black. egui_extras builds its `usvg::Options` with `Default::default()` and offers no way to change them (`svg_loader.rs`). So REDA must make icons white before embedding: replace `currentColor` with `#fff` when an icon is vendored into the repo (a one-line `sed` in the vendoring step), and draw REDA's own icons white. Then the tint gives the exact colour.
- **Binary size** (measured locally, release, LTO, `codegen-units = 1`, stripped, x86_64 Linux): a probe calling only `egui::Context` was 3.49 MB; adding `egui_extras` 0.37 (`default-features = false, features = ["svg"]`) and rasterising one Lucide SVG made it 4.41 MB, so about **+0.9 MB**. Against a full eframe/wgpu binary the share will be smaller. Each embedded icon adds about 0.5 KB.
- **Dependencies:** 30 more crates in the normal build (`resvg`, `usvg`, `tiny-skia`, `tiny-skia-path`, `roxmltree`, `svgtypes`, `simplecss`, `png`, `flate2`/`miniz_oxide`, `kurbo` 0.11 alongside egui's 0.13, `enum-map`, and small helpers). All MIT/Apache-2.0, except `tiny-skia` (BSD-3-Clause). All are permitted by ADR-0002 and all are pure Rust.
- **wasm32:** all of these are pure Rust with no C build. egui_extras compiles its file loader out on `wasm32` (`#[cfg(all(not(target_arch = "wasm32"), feature = "file"))]` in `loaders.rs`), and `include_image!` needs no file system. *Not built for wasm32 in this survey (target not installed).*
- **Dependency policy:** `egui_extras` belongs to the egui project and is released with each egui version, and resvg is the standard Rust SVG renderer. A correct SVG rasteriser is a substantial piece that is hard to write well, which is the kind of dependency AGENTS.md allows. Writing one in-house is not justified.

### 3.2 Icon font (`egui::FontDefinitions`)

- How it works: put a TTF in `FontDefinitions::font_data` and add it as a fallback to `FontFamily::Proportional`; icons are then characters in any label or button. This is how `egui-phosphor` and `egui_material_icons` work.
- Strengths: glyphs are rasterised at the current `pixels_per_point`, so they are crisp; they take the text colour, so theme, hover and disabled colours come for free; no SVG dependency.
- Costs:
  - REDA's own EDA icons would need a font build step (SVG to TTF via external tooling such as FontForge or fantasticon), and every new icon means rebuilding the font. Stroke icons must be outlined first.
  - Whole fonts are large unless subset: Lucide's TTF is 977 KB, `egui-phosphor` 0.14 ships 0.45–0.54 MB per weight, `egui_material_icons` 0.9 ships 1.2 MB (outlined) and 1.8 MB (filled).
  - Glyphs sit on the text baseline, so icons need alignment tuning (`egui-phosphor` ships a `debug_alignment` example for this).
  - Crate lag: `egui-phosphor` 0.14.0 (2026-09-10) still depends on egui 0.36, one release behind, like `egui_dock` in #17. `egui_material_icons` 0.9.0 is on egui 0.37 but is filled Material style. The crates.io Lucide crates for egui (`egui-lucide` 0.2.0, `shadcn-egui-icons-lucide` 0.7.0) are days to weeks old with hundreds of downloads.
- Verdict: fine for an app that only needs a stock set; a poor fit once REDA adds its own icons.

### 3.3 Drawn in code with the painter

- How it works: each icon is a function that issues `Painter` shapes (`line_segment`, `circle_stroke`, `rect_stroke`, `PathShape`) into the icon's rect.
- Strengths: epaint tessellates at the current scale, so icons are perfectly crisp; colour is just a parameter; no dependencies and no assets; works on wasm32 as it is plain egui.
- Costs: every icon is code (dozens of lines for a non-trivial shape), the design cannot be previewed or edited in a vector editor, and imported Lucide/Tabler icons would need converting by hand.
- Verdict: use for the few icons that show **live state**, which a static SVG cannot: the current layer's colour swatch on the PCB Active Bar, or a routing-mode glyph that changes with the corner style.

## 4. Proposed shape (for the implementing ticket)

- An in-repo `icons/` folder (or an icons module in the UI crate): vendored Lucide SVGs (and occasional Tabler ones) with `currentColor` already replaced by `#fff`, REDA's own EDA SVGs, and a `LICENSE` holding Lucide's ISC and MIT notices and Tabler's MIT notice.
- One Rust module exposing each icon as `pub const WIRE: egui::ImageSource = egui::include_image!(...)`. Commands and Tools refer to icons by these constants.
- Buttons use `Button::image(icon).image_tint_follows_text_color(true)` (or an explicit tint from REDA's theme); disabled Commands go through `ui.add_enabled`.
- `egui_extras` with `default-features = false, features = ["svg"]`, plus `install_image_loaders` at startup.
- REDA's third-party-notices output (whenever it exists) must include the icon licences, as ISC and MIT require.

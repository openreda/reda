# PROTOTYPE: component recipes for the Paper direction (issue #276)

Throwaway. Never merged. One question: which recipe does each chrome component lock, in
light and dark and both Densities?

## Run it

    cargo run -p reda-app                       # F10 swaps the window for the gallery
    REDA_PROTO_GALLERY=1 cargo run -p reda-app  # opens on the gallery

A bar at the bottom drives it: F9 next page, Shift+F9 light/dark, Ctrl+F9 Density,
F7 / Shift+F7 text size (11 to 16), F6 typeface (Inter, Inter regular-only, DejaVu Sans
regular-only). The "Live" card on page 1 follows the pointer and keyboard.

Redraw the screenshots (llvmpipe):

    LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl REDA_GALLERY_OUT=docs/prototype/component-recipes \
      cargo test -p reda-ui --test component_gallery -- --nocapture

The same test run walks the contrast table (129 rules per theme, none fail).

## What is here

- `crates/reda-ui/src/proto.rs`: `Tokens`, in the families ADR-0015 sets out, with the
  values proposed for the spec, and the contrast rules.
- `crates/reda-ui/src/proto/widgets.rs`: the recipes. They read `Tokens` only.
- `crates/reda-ui/src/proto/pages*.rs`: the gallery, seven pages.
- `crates/reda-ui/src/proto/tabular.rs`: bakes Inter's `tnum` digits into its character map.
- Screenshots: `NN-page-{light,dark}-{comfortable,compact}.png`; `text11-*`, `text16-*`
  (the ends of the text-size range); `face-*` (regular-only faces).

## Token values

Colour (light / dark):

| Token | Light | Dark |
|---|---|---|
| surface.base / card / floating / well | `#F0EEE6` `#FAF9F5` `#FDFCFA` `#FFFFFF` | `#1F1E1D` `#262624` `#2C2B29` `#1A1918` |
| surface.scrim | black 35% | black 55% |
| text.primary / muted | `#1F1E1C` `#59574F` | `#F3F1EA` `#C2C0B7` |
| stroke.hairline / strong / focus_ring | `#DEDBD0` `#C4C0B3` `#1F5FBF` | `#3B3A37` `#55534E` `#8FBAF8` |
| fill.control / hover / press / selected_unfocused (text colour at) | 7% 5.5% 11% 8% | 8% 7% 13% 10% |
| accent text / subtle / solid (on) | `#1B55AD` / `#1F5FBF` 14% / `#1F5FBF` (white) | `#8FBAF8` / `#4F8FE8` 22% / `#2458B0` (white) |
| error text / subtle / solid (on) | `#A8231B` / `#C8372D` 13% / `#B3261E` (white) | `#F5998C` / `#E5534A` 20% / `#AD2F27` (white) |
| warning text / subtle / solid (on) | `#7A5200` / `#D99A00` 20% / `#F0B429` (`#2A1F00`) | `#E5B04A` / `#E5A530` 16% / `#E5B04A` (`#231A05`) |
| success text / subtle / solid (on) | `#1F6139` / `#2E8B57` 15% / `#26734A` (white) | `#7CC792` / `#4FAF6E` 18% / `#25683F` (white) |
| touch, close_hover | `#2F7D4F`, `#C4392F` | `#6FBF8A`, `#C4392F` |

Changed from the direction ticket to pass the contrast rules on hovered and tinted
grounds: muted text (light `#65635B` to `#59574F`, dark `#B0AEA5` to `#C2C0B7`), accent
as text (light `#1F5FBF` to `#1B55AD`, dark `#86B4F7` to `#8FBAF8`), and the dark accent
solid (`#2F6FD0` to `#2458B0`, so white holds 4.5:1 on it hovered and pressed).

Space (Comfortable / Compact), the only tokens that follow Density:

| Token | C | K | Gives, at 13px text |
|---|---|---|---|
| control_x, control_y | 12, 5 | 10, 3 | control height 30 / 26 |
| row_x, row_y | 8, 4 | 6, 2 | list, tree, table and menu rows 28 / 24 |
| tab_x, tab_y | 12, 7 | 10, 5 | tab strip 34 / 30 |
| gap (between controls) | 8 | 6 | |
| stack (between form rows) | 8 | 4 | |
| section | 16 | 12 | |
| panel (inside a card) | 12 | 8 | |
| dialog | 20 | 16 | |
| menu (around a popup's items) | 6 | 4 | |
| bar_y (around the Title bar's controls) | 5 | 5 | Title bar 40 / 36 |

Fixed: radius mark 4, row 6, control 8, card 12, popup 12, window 16; hairline 1, accent
line 2, focus ring 2; icons 16 (tabs, rows, buttons, menus) and 20 (quick strip, Tool
bars); disabled opacity 45%; motion fast 90 ms (hover, press), base 140 ms (toggles,
tabs), slow 200 ms (popups, flyouts, Dock collapse); status bar 24 and a floating
window's bar 32, whatever the Density.

Type (body size B, default 13; line height `2 * floor(0.75 * size + 0.5)`):

| Style | Size | Weight | Line at B=13 |
|---|---|---|---|
| small | B − 2 (floor 10) | regular | 16 |
| body | B | regular | 20 |
| body_strong | B | medium | 20 |
| title | B | semibold | 20 |
| heading | B + 3 | semibold | 24 |
| mono | B − 1 | regular | 20 |
| numeric, numeric_small | B, B − 2 | regular, tabular digits | 20, 16 |

Elevation: card 0/1/4 black 9% (27% dark); popup 0/6/24 black 10% (39%); window 0/12/40
black 12% (47%).

## Findings

- **Line height as a token works.** `TextFormat::line_height` / `RichText::line_height`
  fix the row height, and epaint centres the glyphs in it. Inter, DejaVu Sans and Hack
  give the same 30px (26px Compact) control. A plain string handed to a stock widget
  takes the face's own row height (22, 21 and 21px for the same three), so every REDA
  widget builds its text from a text-style token. A stock `TextEdit` takes the line
  height through its layouter, and also needs `interact_size.y` set, or it grows.
- **Tabular digits can be baked in-house.** `tabular.rs` reads the `tnum` single
  substitutions with skrifa and writes a new `cmap`; about 140 lines, no new dependency.
- **Regular-only faces hold.** Nothing relies on weight: see `face-*`.
- **Text size 11 and 16 hold** for the recipes; widths set in the gallery by hand do not.
- **Derived states that needed care**: on a solid, the hover and press overlays lighten
  it in dark, which is why the dark solids are darker than the direction ticket's. No
  state needed an explicit token.
- **Monospace face: not picked.** Only Hack (egui's) and DejaVu Sans Mono were on the
  machine; JetBrains Mono, Geist Mono and IBM Plex Mono need a download.

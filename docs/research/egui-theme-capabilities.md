# What can an egui 0.37 theme control, and where does it fall short of a SwiftUI-like look?

Research for issue #267 (child of the map #266). Researched 2026-11-14 against the published
crate sources of **egui 0.37.0, epaint 0.37.0, epaint_default_fonts 0.37.0, emath 0.37.0,
eframe 0.37.0, egui-wgpu 0.37.0 and harfrust 0.12.0**, the versions the workspace `Cargo.toml`
resolves to. Every claim below was read from those sources; citations are
`crate/path:line` in the published crate (the same files live under `crates/<crate>/src/` at
the [`0.37.0` tag](https://github.com/emilk/egui/tree/0.37.0)). Nothing was run: statements
about how text or shadows *look* are marked as unverified and belong to the prototype tickets.

## Answer

A theme in egui 0.37 is a `Style` value per light/dark mode. It directly controls colours per
interaction state, spacing, text sizes and families, one shadow for windows and one for
popups/menus, corner radii for three groups (windows, menus/popups, "all widgets" per state),
scroll-bar behaviour and one global animation time. That covers neutral surfaces, larger
radii, soft shadows, a bundled typeface with real weights, and live light/dark and Density
switching. All of this needs no custom widgets.

It falls short of a SwiftUI-like look in five places:

1. **No per-widget-kind tokens in stable API.** One `WidgetVisuals` set styles buttons, check
   boxes, sliders, combo boxes and text fields alike. Different radii or fills per kind need
   per-call-site builders, REDA wrapper widgets, or the feature-gated `StyleProvider` system
   (new in this line, covers four widgets, marked experimental).
2. **No animated state changes on widgets.** Hover, press and selection switch instantly.
   Animating them means REDA-owned widgets that interpolate colours themselves.
3. **No OpenType feature selection**, so tabular figures cannot be turned on for a font that
   has them as the `tnum` feature. Workaround: a font file whose default digits are tabular.
4. **No backdrop blur or vibrancy.** Translucent fills work; blurring what is behind a surface
   does not exist and would need a render-pipeline change (fork or custom wgpu pass).
5. **Simple elevation only.** One shadow per frame, built from widened edge feathering; no
   layered or inner shadows, and no gradient fills on frames. Extra layers are hand-painted.

Text is greyscale-antialiased only (no LCD sub-pixel rendering), with hinting and sub-pixel
positioning on by default.

## 1. What `Style` holds

`egui::Style` (`egui/src/style.rs:239`) has these themable parts:

| Field | Controls | Source |
|---|---|---|
| `text_styles: BTreeMap<TextStyle, FontId>` | Size and family for `Small`, `Body`, `Monospace`, `Button`, `Heading` and any `TextStyle::Name(..)` you add | `style.rs:285`, `style.rs:176` |
| `override_text_style`, `override_font_id`, `override_text_valign`, `wrap_mode` | Scope-wide text defaults | `style.rs:243-303` |
| `drag_value_text_style` (default `Button`) | The text style of every `DragValue` | `style.rs:292`, `style.rs:1544` |
| `number_formatter` | How numbers become strings (thousands separators and so on) | `style.rs:296` |
| `spacing: Spacing` | See below | `style.rs:306` |
| `interaction: Interaction` | Hit radius, resize grab radii, tooltip delay and grace time, selectable labels | `style.rs:862` |
| `visuals: Visuals` | See below | `style.rs:312` |
| `animation_time: f32` (default 0.2 s) | Duration of every built-in animation | `style.rs:318`, `style.rs:1550` |
| `scroll_animation` | Speed and duration range of programmatic scrolling | `style.rs:333` |
| `compact_menu_style`, `explanation_tooltips`, `url_in_tooltip`, `always_scroll_the_only_direction` | Behaviour switches | `style.rs:325-338` |

**`Spacing`** (`style.rs:343`): `item_spacing`, `window_margin`, `button_padding`,
`menu_margin`, `indent`, `interact_size` (the default widget height), `slider_width`,
`slider_rail_height`, `combo_width`, `combo_height`, `text_edit_width`,
`extra_text_line_spacing`, `icon_width`, `icon_width_inner`, `icon_spacing`,
`default_area_size`, `tooltip_width`, `menu_width`, `menu_spacing`,
`indent_ends_with_horizontal_line`, and `scroll: ScrollStyle`.

`ScrollStyle` (`style.rs` struct `ScrollStyle`) is rich: floating or solid bars, `bar_width`,
`floating_width`, margins, minimum handle length, six opacity values for the dormant, active
and hovered states, an edge fade (`ScrollFadeStyle { strength, size }`) and kinetic scrolling
with optional rubber-banding (`KineticScrollStyle { decay_time, stop_distance, rubber_band }`).
Thin overlay scroll bars that widen on hover, as on macOS, are a theme setting.

**`Visuals`** (`style.rs` struct `Visuals`):

- Text: `override_text_color`, `weak_text_alpha`, `weak_text_color`, `hyperlink_color`,
  `warn_fg_color`, `error_fg_color`, `text_cursor` (stroke, blink timing), `text_options`
  (hinting, sub-pixel binning, coverage transfer function; see section 4).
- Surfaces: `panel_fill`, `window_fill`, `window_stroke`, `faint_bg_color`,
  `extreme_bg_color`, `text_edit_bg_color`, `code_bg_color`.
- Selection: `selection.bg_fill`, `selection.stroke`.
- Shape: `window_corner_radius`, `menu_corner_radius`, `window_shadow`, `popup_shadow`,
  `resize_corner_size`, `handle_shape` (slider handle: circle or rectangle).
- Switches: `button_frame`, `collapsing_header_frame`, `indent_has_left_vline`, `striped`,
  `slider_trailing_fill`, `window_highlight_topmost`, `interact_cursor`,
  `image_loading_spinners`, `disabled_alpha`.
- `widgets: Widgets`: five `WidgetVisuals` (`noninteractive`, `inactive`, `hovered`, `active`,
  `open`), each with `bg_fill`, `weak_bg_fill`, `bg_stroke`, `corner_radius`, `fg_stroke`
  (which is also the text colour) and `expansion`.

Things a design system usually has that `Visuals` does **not**: a focus-ring token (a focused
widget simply takes the `active` visuals, `egui/src/widget_style/mod.rs` `Response::widget_state`;
a focused `TextEdit` is outlined with `selection.stroke`, `egui/src/theme/default_style.rs`
`StyleProvider<TextEditStyle>`), separate tokens for primary, secondary and destructive
buttons, a tooltip fill separate from the popup fill, and per-kind radii.

Units are coarse: `CornerRadius` fields are `u8` (`epaint/src/corner_radius.rs`), `Margin`
fields are `i8` (`epaint/src/margin.rs`), and `Shadow` uses `[i8; 2]` offset and `u8` blur and
spread (`epaint/src/shadow.rs`). Whole points only, so no 2.5 pt radius or 0.5 pt margin.
`Spacing` vectors and strokes are `f32`.

## 2. Shadows and elevation

- `Shadow { offset: [i8; 2], blur: u8, spread: u8, color }` (`epaint/src/shadow.rs`). The
  defaults are `[10, 20]` blur 15 for windows and `[6, 10]` blur 8 for popups
  (`style.rs:1630-1649`); REDA's current theme sets both to `Shadow::NONE`.
- A theme has exactly **two** shadow tokens. `Frame::window` uses `window_shadow`;
  `Frame::menu` and `Frame::popup` both use `popup_shadow` and `menu_corner_radius`
  (`egui/src/containers/frame.rs:196-222`). Menus, combo-box lists, popups and tooltips
  therefore share one elevation.
- `Frame` has a single `shadow` field (`frame.rs` struct `Frame`), and any `Frame` can carry
  one, so a card or Dock can be given a shadow at its call site with `Frame::shadow(..)`.
- The blur is not a Gaussian blur. `Shadow::as_shape` emits one filled rounded rectangle with
  `blur_width`, and the tessellator renders that by widening the edge feathering
  (`epaint/src/shadow.rs:48-66`, `epaint/src/tessellator.rs:2098-2110`). The blur is clamped
  to the rectangle's smallest side. Whether this reads as "soft" at large blur values was not
  verified visually; the prototype should check a 24-32 pt blur at low alpha.
- A SwiftUI-style elevation usually stacks two shadows (a tight contact shadow and a wide
  ambient one). egui has no API for that. Workaround: paint a second `Shadow::as_shape`
  yourself before showing the frame. Cost: a small helper in REDA for the surfaces REDA
  creates itself; surfaces egui creates internally (menus, tooltips, combo lists) take a
  `Frame` through `Popup::frame` or the menu config and get only the one shadow.
- Shadows are painted in the same layer as their frame, so a shadow on a panel-embedded frame
  is cut off by the enclosing clip rectangle. Floating areas (windows, popups) are not affected.
- No inner shadows and no gradient fills on a `Frame`. `Shape::gradient_rect`
  (`epaint/src/shapes/shape.rs:329`) and meshes exist for hand painting.
- OS-level window shadow is separate: `ViewportBuilder::with_has_shadow`
  (`egui/src/viewport.rs:518`).

## 3. Corner radii per widget kind

Stable API gives three groups:

| Token | Applies to | Source |
|---|---|---|
| `visuals.window_corner_radius` | `Window`, `Frame::window` | `frame.rs:196` |
| `visuals.menu_corner_radius` | Menus, popups, combo lists, tooltips | `frame.rs:205-222` |
| `visuals.widgets.<state>.corner_radius` | Buttons, text fields, check boxes, slider rail and handle, collapsing headers, `Frame::group`, colour swatches | `theme/default_style.rs`, `widgets/slider_core.rs:389-439`, `containers/collapsing_header.rs:604`, `frame.rs:178` |

Built-in exceptions: the check box caps its radius at 30 % of `icon_width`
(`theme/default_style.rs`, `StyleProvider<CheckboxStyle>`); a progress bar defaults to a
fully rounded pill (`widgets/progress_bar.rs:138`).

So "buttons 6, text fields 6, check boxes 4, sliders pill" works only by luck of those
exceptions. A different radius for buttons than for text fields needs one of:

- **Per call site**: `Button::corner_radius`, `Button::fill`, `Button::stroke`,
  `Button::min_size`, `Button::frame` (`widgets/button.rs:156-224`), `ProgressBar::corner_radius`,
  `Image::corner_radius`, `Frame::corner_radius`.
- **Per scope**: change `ui.visuals_mut().widgets.*` inside `ui.scope(..)` (section 5).
- **REDA wrapper widgets** that build the egui widget with the right tokens. This is the
  approach that fits the map's rule that widgets take values only from theme tokens.
- **The experimental `StyleProvider`** (section 6).

## 4. Fonts and text

**Bundled fonts** (`epaint_default_fonts/fonts/`, wired in
`epaint/src/text/font_definitions.rs:121-190`): `Ubuntu-Light` (proportional), `Hack-Regular`
(monospace), `egui-icons`, and, behind the `monochrome_emoji_fonts` feature,
`NotoEmoji-Regular` and `emoji-icon-font`. One weight each. REDA currently installs no fonts
of its own, so its UI text is Ubuntu Light.

**Own fonts**: `Context::set_fonts(FontDefinitions)` or `Context::add_font(FontInsert)`
(`egui/src/context.rs:2505`, `:2532`) take `.ttf`/`.otf` bytes and assign them to
`FontFamily::Proportional`, `Monospace` or any `FontFamily::Name(..)`, with a fallback chain
per family. A `FontProvider` can supply fallbacks on demand; eframe's `system_fonts` feature
installs a system-font provider (`epaint/src/text/font_provider.rs`), which REDA's eframe
feature list does not enable.

**Variable fonts are supported.** This is new relative to older egui and matters for the
typeface choice:

- `FontTweak::coords: VariationCoords` sets axis values for a whole installed font
  (`epaint/src/text/font_tweak.rs:47`).
- `TextFormat::coords` sets them per text run, and `RichText::variation(tag, value)` /
  `RichText::variations(..)` expose that (`epaint/src/text/text_layout_types.rs` struct
  `TextFormat`; `egui/src/widget_text.rs:202-214`).
- `FontData::variation_axes()` lists a font's axes (`epaint/src/text/font_data.rs:66`).
- Both are fed to the shaper and the rasteriser (`epaint/src/text/text_layout.rs:1583-1599`,
  `epaint/src/text/font_face.rs:614`).

**Weights in a text style are indirect.** `FontId` is only `{ size, family }`, with a
`TODO: weight (bold), italics` (`epaint/src/text/font_id.rs:12`), and `Style::text_styles`
maps to `FontId`. A text style therefore cannot say "Body at weight 600". Two workarounds,
both cheap:

- Install the same variable font file several times under different names, each with
  `FontTweak::coords` set to one weight, and give each its own `FontFamily::Name` (for example
  `"ui-medium"`, `"ui-semibold"`). Fonts are stored by name (`epaint/src/text/face_store.rs:58-77`),
  and the bytes are shared (`FontData::font` is a shared `Blob`). Then
  `TextStyle::Heading -> FontId::new(16.0, FontFamily::Name("ui-semibold"))`. The same works
  with static per-weight font files.
- Or set the weight per call with `RichText::variation("wght", 600.0)`.

`RichText::strong()` is **not** bold: it only switches to a stronger colour
(`egui/src/widget_text.rs:250-254`, `style.rs:1257`). `italics` is a synthetic slant made by
shearing the glyph quad, not an italic face (`epaint/src/text/text_layout.rs:1304-1310`).
There is no synthetic bold.

**Tabular figures are not selectable.** Text is shaped with harfrust, but egui passes no
OpenType features: `shaper.shape(buffer, harfrust::ShapeOptions::new())`
(`epaint/src/text/text_layout.rs:1605`), and no `features`/`tnum` API exists anywhere in
epaint, although `harfrust::ShapeOptions::features(..)` exists (`harfrust/src/hb/face.rs:324`).
Default-on features of the font (kerning, standard ligatures) still apply. Options:

- Use a UI font whose **default** digits are tabular, or a copy of the font with `tnum` frozen
  in by a font tool at build time. Install it as a named family and point
  `Style::drag_value_text_style` and numeric labels at it. Cost: a font-preparation step, no
  code. Which candidate typefaces have tabular default digits was not checked here.
- Use the monospace family for numbers. Works today; looks like code.
- Patch epaint to pass features (a few lines at `text_layout.rs:1605` plus plumbing a feature
  list through `TextFormat` or `FontTweak`). Cost: a fork, which the earlier research
  (`research/egui-forks-and-companion-crates`) recommends against until a trigger fires.

**Rendering quality at 12-14 px.** What the source says:

- Outlines come from skrifa and are rasterised by `vello_cpu` into an alpha atlas
  (`epaint/src/text/font_face.rs:155-160`). Antialiasing is greyscale. There is no LCD
  (per-channel sub-pixel) antialiasing anywhere in epaint.
- Hinting is on by default (`TextOptions::font_hinting`, `epaint/src/text/mod.rs:55`),
  including auto-hinting (`skrifa` is built with `autohint_shaping`). It can be set per font
  with `FontTweak::hinting` and tuned with `HintingTarget::Smooth { light, symmetric_rendering,
  preserve_linear_metrics }` (`font_tweak.rs`).
- Sub-pixel **positioning** is on by default: each glyph is rasterised at up to four
  horizontal offsets for more even spacing; the doc comment says it "also lead[s] to text
  looking more blurry" (`mod.rs:57-69`). Per-font override: `FontTweak::subpixel_binning`.
- Coverage is remapped per theme: `FontColorTransferFunction::Off` for light mode and
  `TwoCoverageMinusCoverageSq` for dark, or a custom `Gamma(f32)` (`epaint/src/image.rs:370-398`).
  These sit in `Visuals::text_options`, so `Visuals::dark()`/`light()` already pick them.
- Other knobs: `FontTweak::scale`, `y_offset_factor`, `y_offset` for optical alignment;
  `TextFormat::extra_letter_spacing` and `line_height`; `Spacing::extra_text_line_spacing`;
  `TessellationOptions::round_text_to_pixels`.

Not verified: how a given typeface actually looks at 12-14 px at 1x and 2x scale with these
settings. That needs screenshots, with hinting and sub-pixel binning toggled, in the prototype.

## 5. Overrides per widget and per scope

- **Per scope**: `ui.style_mut()`, `ui.visuals_mut()`, `ui.spacing_mut()` change the style of
  that `Ui` and its later children (clone-on-write); `ui.scope(|ui| ..)` limits the change;
  `ui.set_style(..)` and `ui.reset_style()` replace or restore it (`egui/src/ui.rs:391-460`,
  `:2311`). A toolbar, a Dock header or the status bar can each run under a variant style.
- **Popups and menus**: `Popup::style(StyleModifier)`, `ComboBox::popup_style(..)` and the
  menu config's `style` take a closure that edits the style inside the popup
  (`containers/popup.rs:456`, `combo_box.rs:229`, `menu.rs:130`, `:345`). `Popup::frame(..)`
  replaces its frame (`popup.rs:395`).
- **Containers**: `Panel::frame`, `CentralPanel::frame`, `Window::frame`, `Modal::frame`,
  `Modal::backdrop_color` take explicit frames and colours.
- **Per widget**: the builders in section 3, `RichText` (size, family, colour, variation,
  line height, letter spacing), `Slider::handle_shape`, `Slider::trailing_fill`.
- **Opacity**: `ui.set_opacity(..)` / `multiply_opacity(..)` fade everything painted by a `Ui`
  (`ui.rs:587-594`).

## 6. The experimental per-widget theme system

egui 0.37 contains a class-based styling layer that is **not** in older versions:

- Widgets carry `Classes` (`egui/src/class/`), for example `Button::CLASS_SELECTED`,
  `CLASS_SMALL`, `CLASS_NO_FRAME`, `ComboBox::CLASS_OPEN`, `DragValue::CLASS`,
  `TextEdit::CLASS_READ_ONLY`. A `Ui` can carry classes too (`UiBuilder::classes`), and a
  provider can ask whether a widget or any ancestor has a class (`StyleArgs::has_class`,
  `widget_style/mod.rs`).
- A `StyleProvider<S>` computes a typed style from `StyleArgs { classes, state, stack, style,
  ctx }` (`theme/style_provider.rs`, `widget_style/mod.rs`). Register one with
  `Context::add_widget_theme` / `replace_widget_theme` (`context.rs:2458-2480`).
- Style types exist for **four** widgets: `ButtonStyle`, `TextEditStyle`, `CheckboxStyle`,
  `SeparatorStyle` (`widget_style/mod.rs`); `ComboBox` and `DragValue` go through the button
  and text-edit styles. Each gives a full `Frame` (fill, stroke, radius, margin, shadow),
  minimum size, gap, font and text colour, so a REDA provider could give buttons and text
  fields different radii, paddings and even shadows without touching call sites.
- Radio buttons, sliders, progress bars, collapsing headers, scroll bars, windows and menus
  still read `Visuals` directly.

Limits and risk:

- It is gated: the `theme` module and the types are public only with egui's `experimental`
  feature, documented as "might have massive breaking changes or be removed entirely in
  future updates" (`egui/Cargo.toml.orig:51-55`, `egui/src/lib.rs:424-427`). REDA does not
  enable it today. (Source comments call the feature `experimental_theme`; the Cargo feature
  is named `experimental`.)
- `StyleArgs` carries no widget id, so a provider cannot run a per-widget animation; it sees
  only the current discrete `WidgetState` (`Noninteractive`, `Inactive`, `Hovered`, `Active`).
- The state is read from the previous pass, and the first frame requests a repaint
  (`theme/mod.rs`, `Ui::widget_style`).

Whether to adopt it is a decision for the map: it is the cleanest way to get per-kind tokens
for the four covered widgets, at the price of an unstable API on every egui upgrade. The
stable alternative is REDA wrapper widgets.

## 7. Switching the whole style at run time

- egui keeps one `Style` per theme. `Context::set_style_of(Theme, Style)`,
  `style_mut_of`, `all_styles_mut` and `set_theme(ThemePreference)` change them, and
  `ThemePreference::System` follows the OS (`context.rs:2576-2680`). REDA already does this in
  `theme::install`.
- Styles are plain values behind an `Arc`. Changing Density means building the two styles
  again from a different spacing/size table and calling `set_style_of` twice. It takes effect
  for every `Ui` created afterwards, which in an immediate-mode UI is the next frame. Nothing
  needs restarting.
- Expected caveat (inferred, not tested): sizes egui keeps in memory (panel widths, window
  sizes, scroll offsets) are stored in points and nothing rescales them when the style
  changes, so a Density change may leave a Dock sized for the old Density until it is resized. `Context::set_fonts` takes
  effect at the start of the next pass (`context.rs:2500-2505`).
- `Context::set_zoom_factor` scales everything uniformly (`context.rs:2761`). It is a
  different thing from Density (it scales text too) but composes with it.
- Hard-coded sizes at REDA call sites do not follow any of this, which is why the map already
  plans to migrate them to tokens.

## 8. Animation

API (`context.rs:3709-3790`, `emath/src/easing.rs`):

- `animate_bool(id, bool) -> f32` and variants `_responsive` (cubic-out), `_with_easing`,
  `_with_time`, `_with_time_and_easing`; `animate_value_with_time(id, target, time)` for an
  arbitrary `f32` (linear only). They request repaints while running.
- Easing functions: linear, quadratic, cubic, sine, circular, exponential, back and bounce,
  each in/out/in-out. No spring or physics-based animation.
- One global duration token, `Style::animation_time`.

What egui animates by itself: fade-in of every `Area` (windows, popups, menus, tooltips),
window fade-out on close, collapsing-header open/close, panel expand/collapse
(`Panel::show_animated_inside`, `show_collapsible`, `show_animated_between_inside`), scroll-bar
show/hide and widen-on-hover, programmatic scrolling, kinetic scrolling
(`containers/area.rs:698-708`, `window.rs:639-712`, `collapsing_header.rs:78`, `panel.rs:27`,
`scroll_area.rs`).

What it does not animate: hover, press and selected colours of buttons, check boxes and tabs
(no `animate_*` call in `widgets/button.rs`, `widgets/checkbox.rs` or `atomics/`); popup and
menu fade-**out** (only `Window` has one); scale or slide transitions of popups; layout
changes; the check mark.

What can be animated by hand: anything that is a number. Opacity (`ui.set_opacity`), colours
(interpolate two `Color32` with the `f32` from `animate_bool`), sizes and offsets. The cost is
that the widget must be painted by REDA code: an animated hover on a button means a REDA
button widget (allocate a rect, read the response, interpolate, paint a `Frame` and the
content), since neither `Visuals` nor a `StyleProvider` can express a blend between states.
For the handful of chrome widgets REDA already draws itself (tabs, Dock headers, tool
buttons) this is a small addition; for stock egui widgets it means replacing them.

## 9. Translucency and blur

- **Translucent fills work.** Every colour is premultiplied RGBA; `window_fill`, `panel_fill`,
  frame fills and `Modal::backdrop_color` can be translucent, and surfaces blend over what is
  under them inside the egui frame.
- **Transparent OS window**: `ViewportBuilder::with_transparent(true)` plus
  `eframe::App::clear_color` (`viewport.rs:415-430`, `eframe/src/epi.rs:251`), with
  `with_decorations(false)` for a custom title bar and `ViewportCommand::StartDrag` /
  `BeginResize` to move and resize it (`viewport.rs:372`, `:1108`, `:1129`). macOS extras:
  `with_fullsize_content_view`, `with_title_shown`, `with_titlebar_buttons_shown`,
  `with_titlebar_shown` (`viewport.rs:476-505`).
- **Backdrop blur does not exist.** No shape or frame option blurs what is behind it, and
  `ViewportBuilder` has no blur or vibrancy option (no `blur` in `egui/src/viewport.rs` or
  `egui-winit/src/`). egui paints in one pass to the surface, so a blurred popover background
  would need the content below rendered to a texture first: a custom `egui_wgpu` paint
  callback (`egui-wgpu/src/renderer.rs:33`, `CallbackTrait` at `:88`) can draw arbitrary wgpu
  content inside a rect, but it cannot read the frame egui is currently drawing. That means
  restructuring rendering (an offscreen pass) or forking egui-wgpu. Not practical for chrome
  polish. OS-compositor blur behind a transparent window would have to go around eframe to
  the windowing layer and is platform-specific; not investigated further.
- Practical substitute: opaque or nearly opaque surfaces with a hairline stroke and a soft
  shadow, which is also what keeps WCAG contrast predictable.

## 10. Gap list: what a SwiftUI / Codex / Claude desktop look needs

| Need | In the theme? | Workaround | Cost |
|---|---|---|---|
| Neutral surfaces, hairline strokes, accent selection | Yes (`Visuals`) | none | none |
| Larger radii on windows, menus, widgets | Yes, three groups, whole points | none | none |
| Different radius or padding per widget kind | No | wrapper widgets, or experimental `StyleProvider` (4 widgets) | custom widgets, or unstable API |
| Soft single shadow on windows, popups, menus | Yes (two tokens) | none | none; look unverified |
| Layered shadows, more than two elevation levels | No | paint extra `Shadow::as_shape` before the frame | small helper; not for egui-made popups |
| Bundled typeface, several weights | Yes via `set_fonts` + variation coords | one named family per weight | setup code only |
| Weight as part of a text style | Not directly | named family per weight | setup code only |
| True italic | No (synthetic slant) | install the italic face as a named family | setup code only |
| Tabular figures | No feature API | font with tabular default digits, or `tnum` frozen in | font preparation; else fork |
| Crisp 12-14 px text | Partly: hinting, sub-pixel positioning, greyscale AA | tune `FontTweak` and `TextOptions` per font | prototype needed; LCD AA would be a fork |
| Primary / secondary / destructive button variants | No tokens | wrapper widgets with `Button::fill/stroke`, or classes + `StyleProvider` | custom widgets |
| Focus ring distinct from pressed state | No token | paint a ring from `response.has_focus()` | custom painting per widget |
| Segmented controls, pill toggles, switch | No such widgets | REDA widgets | custom widgets |
| Quiet tabs with animated indicator | Not an egui widget | REDA already draws its tabs | custom painting |
| Animated hover and press | No | REDA widgets interpolating with `animate_bool` | custom widgets |
| Popup scale/slide in, fade out | Fade-in only | own popup wrapper driving opacity and offset | custom container; fiddly |
| Light/dark and Density switching live | Yes (`set_style_of`, `set_theme`) | none | none |
| Thin overlay scroll bars | Yes (`ScrollStyle`) | none | none |
| Translucent surfaces | Yes | none | none |
| Backdrop blur / vibrancy | No | offscreen pass or fork | not practical |
| Gradient fills on surfaces | No | `Shape::gradient_rect` or a mesh behind the content | custom painting |
| Custom title bar | Building blocks exist (`with_decorations(false)`, `StartDrag`, `BeginResize`) | REDA draws the bar | custom widget; platform behaviour is a separate ticket |

## Surprises

- egui 0.37 is a different text stack from the egui most write-ups describe: skrifa + harfrust
  + vello_cpu, with variable-font axes, hinting control and sub-pixel positioning. Advice
  that "egui cannot do bold" or "has no font weights" is out of date for this version.
- The class-based `StyleProvider` layer exists but is feature-gated and covers four widgets.
- `ScrollStyle` has grown kinetic scrolling, rubber-banding and edge fades.
- `Shadow` and `CornerRadius` are integer-valued.

## Not verified

- Any visual judgement: shadow softness, text crispness at 12-14 px, how the light-mode and
  dark-mode coverage functions look with a specific typeface.
- Which candidate UI typefaces ship tabular digits by default.
- Whether installing one variable font under several names costs noticeable atlas space or
  start-up time (each name is a separate face with its own glyph cache).
- How a Density switch interacts with sizes REDA persists in its Layout.
- OS-compositor blur behind a transparent window on Wayland, Windows and macOS.

//! The value tables of docs/spec/v0.1/design-language.md, generated from [`Tokens`].
//!
//! The spec keeps them between `<!-- tokens:begin -->` and `<!-- tokens:end -->`. The test
//! here fails when that region is stale, and `cargo xtask tokens` (which runs it with
//! `REDA_BLESS=1`) rewrites it. A token's job stays in the spec's prose and in the labels
//! below; its value comes from `Tokens`.

use std::fmt::Write as _;
use std::path::PathBuf;

use egui::epaint::Shadow;
use egui::{Color32, Theme};

use super::palette::Tint;
use super::tokens::{palette, rgb, tint};
use super::{Density, Easing, Settings, Text, Tokens, Tone, Transition};

const BEGIN: &str = "<!-- tokens:begin -->";
const END: &str = "<!-- tokens:end -->";
const NOTE: &str = "<!-- Generated from `Tokens` (crates/reda-ui/src/theme) by `cargo xtask tokens`. Do not edit between the markers. -->";

/// One table row. An empty cell is written without padding.
fn row(out: &mut String, cells: &[&str]) {
    out.push('|');
    for cell in cells {
        if !cell.is_empty() {
            out.push(' ');
            out.push_str(cell);
        }
        out.push_str(" |");
    }
    out.push('\n');
}

fn head(out: &mut String, cells: &[&str]) {
    row(out, cells);
    out.push('|');
    for _ in cells {
        out.push_str("---|");
    }
    out.push('\n');
}

/// A number as the tables write it: `8`, `1.5`.
fn num(v: f32) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v}")
    }
}

fn hex_of(c: Color32) -> u32 {
    assert!(c.is_opaque(), "{c:?} has no plain hex");
    (u32::from(c.r()) << 16) | (u32::from(c.g()) << 8) | u32::from(c.b())
}

/// An opaque colour: `` `#F0EEE6` ``.
fn hex(c: Color32) -> String {
    format!("`#{:06X}`", hex_of(c))
}

/// An icon role colour, in the lower case the SVG files use.
fn hex_lower(c: Color32) -> String {
    format!("`#{:06x}`", hex_of(c))
}

/// The alpha of a translucent black, in percent.
fn black_percent(c: Color32) -> String {
    assert_eq!((c.r(), c.g(), c.b()), (0, 0, 0), "{c:?} is not black");
    format!("{:.0} %", f32::from(c.a()) / 255.0 * 100.0)
}

/// A tint as the palette holds it, checked against the token it became.
fn tint_of(name: &str, source: Tint, token: Color32) -> String {
    assert_eq!(tint(source), token, "the token is not the palette's tint");
    format!("{name} at {} %", num(source.percent))
}

fn shadow(s: Shadow) -> String {
    assert_eq!(s.spread, 0);
    format!(
        "{} / {}, blur {}, black at {}",
        s.offset[0],
        s.offset[1],
        s.blur,
        black_percent(s.color)
    )
}

/// Seconds as whole milliseconds.
fn ms(seconds: f32) -> String {
    format!("{:.0}", seconds * 1000.0)
}

fn size_line(t: &Text) -> String {
    format!("{} / {}", num(t.size), num(t.line))
}

fn tokens(theme: Theme, density: Density, text_size: u8) -> Tokens {
    let settings = Settings {
        density,
        text_size,
        ..Settings::default()
    };
    Tokens::new(theme, &settings)
}

fn colour_values(out: &mut String, light: &Tokens, dark: &Tokens) {
    out.push_str("### Colour values\n\n");
    head(out, &["Token", "Light", "Dark"]);
    let (l, d) = (&light.color, &dark.color);
    let (pl, pd) = (palette(Theme::Light), palette(Theme::Dark));
    for (name, a, b) in [
        ("`surface.base`", l.surface.base, d.surface.base),
        ("`surface.card`", l.surface.card, d.surface.card),
        ("`surface.floating`", l.surface.floating, d.surface.floating),
        ("`surface.well`", l.surface.well, d.surface.well),
    ] {
        row(out, &[name, &hex(a), &hex(b)]);
    }
    let scrim = |c: Color32| format!("black at {}", black_percent(c));
    row(
        out,
        &[
            "`surface.scrim`",
            &scrim(l.surface.scrim),
            &scrim(d.surface.scrim),
        ],
    );
    for (name, a, b) in [
        ("`text.primary`", l.text.primary, d.text.primary),
        ("`text.muted`", l.text.muted, d.text.muted),
        ("`stroke.hairline`", l.stroke.hairline, d.stroke.hairline),
        ("`stroke.strong`", l.stroke.strong, d.stroke.strong),
        (
            "`stroke.focus_ring`",
            l.stroke.focus_ring,
            d.stroke.focus_ring,
        ),
    ] {
        row(out, &[name, &hex(a), &hex(b)]);
    }
    // The fills are the primary text colour at a low opacity.
    assert_eq!(rgb(pl.ink), l.text.primary);
    assert_eq!(rgb(pd.ink), d.text.primary);
    let fills = [
        ("`fill.control`", l.fill.control, d.fill.control),
        ("`fill.hover`", l.fill.hover, d.fill.hover),
        ("`fill.press`", l.fill.press, d.fill.press),
        (
            "`fill.selected_unfocused`",
            l.fill.selected_unfocused,
            d.fill.selected_unfocused,
        ),
    ];
    for (i, (name, a, b)) in fills.into_iter().enumerate() {
        let of = |ink: u32, percent: f32, token: Color32| {
            tint_of("`text.primary`", Tint { hex: ink, percent }, token)
        };
        row(
            out,
            &[
                name,
                &of(pl.ink, pl.ink_fills[i], a),
                &of(pd.ink, pd.ink_fills[i], b),
            ],
        );
    }
    row(out, &["`touch`", &hex(l.touch), &hex(d.touch)]);
    row(
        out,
        &["`close_hover`", &hex(l.close_hover), &hex(d.close_hover)],
    );
    out.push('\n');

    head(out, &["Tone", "Strength", "Light", "Dark"]);
    let tones: [(&str, Tone, Tone, Tint, Tint); 4] = [
        ("`accent`", l.accent, d.accent, pl.blue.tint, pd.blue.tint),
        ("`error`", l.error, d.error, pl.red.tint, pd.red.tint),
        (
            "`warning`",
            l.warning,
            d.warning,
            pl.amber.tint,
            pd.amber.tint,
        ),
        (
            "`success`",
            l.success,
            d.success,
            pl.green.tint,
            pd.green.tint,
        ),
    ];
    for (name, a, b, ta, tb) in tones {
        let subtle = |source: Tint, token: Color32| tint_of(&hex(rgb(source.hex)), source, token);
        row(out, &[name, "text", &hex(a.text), &hex(b.text)]);
        row(
            out,
            &["", "subtle", &subtle(ta, a.subtle), &subtle(tb, b.subtle)],
        );
        row(out, &["", "solid", &hex(a.solid), &hex(b.solid)]);
        row(out, &["", "on_solid", &hex(a.on_solid), &hex(b.on_solid)]);
    }
    out.push('\n');
}

fn icon_roles(out: &mut String, light: &Tokens, dark: &Tokens) {
    out.push_str("### Icon role colours\n\n");
    head(out, &["Role", "Key in the SVG", "Light", "Dark"]);
    let roles = [
        "Context (what already exists)",
        "Schematic",
        "Copper",
        "General",
        "Picked point",
        "Board edge and Keepout",
    ];
    let (l, d) = (light.color.icon.roles(), dark.color.icon.roles());
    for (i, role) in roles.into_iter().enumerate() {
        let key = hex_lower(rgb(crate::icons::ROLE_KEYS[i]));
        row(out, &[role, &key, &hex_lower(l[i]), &hex_lower(d[i])]);
    }
    out.push_str(
        "\nThe dark picked-point value is the one closest to `#ec4f7a` that the contrast test \
         accepts; `#ef5a82` was computed, not run.\n\n",
    );
}

fn space_values(out: &mut String, comfortable: &Tokens, compact: &Tokens) {
    out.push_str(
        "### Space values\n\n\
         The only tokens that follow Density. Pixels at UI scale 100 %.\n\n",
    );
    head(out, &["Token", "Job", "Comfortable", "Compact"]);
    let (a, b) = (&comfortable.space, &compact.space);
    for (name, job, a, b) in [
        (
            "`control_x`",
            "Inside buttons, fields and combo boxes, sideways",
            a.control_x,
            b.control_x,
        ),
        (
            "`control_y`",
            "The same, vertically",
            a.control_y,
            b.control_y,
        ),
        (
            "`row_x`",
            "Inside rows of lists, trees, tables and menus, sideways",
            a.row_x,
            b.row_x,
        ),
        ("`row_y`", "The same, vertically", a.row_y, b.row_y),
        ("`tab_x`", "Inside a tab, sideways", a.tab_x, b.tab_x),
        ("`tab_y`", "The same, vertically", a.tab_y, b.tab_y),
        ("`gap`", "Between controls side by side", a.gap, b.gap),
        (
            "`stack`",
            "Between stacked rows of a form",
            a.stack,
            b.stack,
        ),
        ("`section`", "Between sections", a.section, b.section),
        ("`panel`", "Inside a Panel's card", a.panel, b.panel),
        ("`dialog`", "Inside a dialog", a.dialog, b.dialog),
        (
            "`menu`",
            "Inside a menu or popup, around its items",
            a.menu,
            b.menu,
        ),
        (
            "`bar_y`",
            "Above and below the Title bar's controls",
            a.bar_y,
            b.bar_y,
        ),
    ] {
        row(out, &[name, job, &num(a), &num(b)]);
    }
    out.push('\n');
}

fn type_values(out: &mut String) {
    out.push_str(
        "### Type values\n\n\
         B is the body size from `general.text_size` (default 13). Line height is \
         `2 * floor(0.75 * size + 0.5)`.\n\n",
    );
    let default = Settings::DEFAULT_TEXT_SIZE;
    let at = format!("Size / line at B = {default}");
    head(out, &["Style", "Size", "Weight", &at]);
    let t = tokens(Theme::Dark, Density::Comfortable, default).ty;
    for (name, size, weight, style) in [
        ("`small`", "B − 2, never below 10", "regular", &t.small),
        ("`body`", "B", "regular", &t.body),
        ("`body_strong`", "B", "medium", &t.body_strong),
        ("`title`", "B", "semibold", &t.title),
        ("`heading`", "B + 3", "semibold", &t.heading),
        (
            "`mono`",
            "B − 1, on the body's line height",
            "regular",
            &t.mono,
        ),
        ("`numeric`", "B", "regular, tabular digits", &t.numeric),
        (
            "`numeric_small`",
            "B − 2, never below 10",
            "regular, tabular digits",
            &t.numeric_small,
        ),
    ] {
        row(out, &[name, size, weight, &size_line(style)]);
    }
    out.push('\n');
    head(
        out,
        &[
            "B",
            "`small`",
            "`body`",
            "`heading`",
            "`mono`",
            "Control height (Comfortable / Compact)",
        ],
    );
    for b in Settings::TEXT_SIZES {
        let comfortable = tokens(Theme::Dark, Density::Comfortable, b);
        let compact = tokens(Theme::Dark, Density::Compact, b);
        let ty = &comfortable.ty;
        let heights = format!(
            "{} / {}",
            num(comfortable.control_height()),
            num(compact.control_height())
        );
        row(
            out,
            &[
                &b.to_string(),
                &size_line(&ty.small),
                &size_line(&ty.body),
                &size_line(&ty.heading),
                &size_line(&ty.mono),
                &heights,
            ],
        );
    }
    out.push('\n');
}

fn fixed_values(out: &mut String, t: &Tokens) {
    out.push_str("### Fixed values\n\n");
    head(out, &["Token", "Value"]);
    let m = &t.metric;
    let pair = |a: f32, b: f32| format!("{}, {}", num(a), num(b));
    let rows: [(&str, String); 23] = [
        ("`radius.mark`", num(t.radius.mark)),
        ("`radius.row`", num(t.radius.row)),
        ("`radius.control`", num(t.radius.control)),
        ("`radius.card`", num(t.radius.card)),
        ("`radius.popup`", num(t.radius.popup)),
        ("`radius.window`", num(t.radius.window)),
        ("Hairline width", num(t.stroke.hairline)),
        ("Accent line width", num(t.stroke.accent_line)),
        ("Focus ring width", num(t.stroke.focus_ring)),
        ("Icon size: tabs, rows, buttons, menus", num(t.icon.small)),
        (
            "Icon size: quick strip, Tool bars, Active Bar, Dock strips",
            num(t.icon.bar),
        ),
        ("Glyph box and stroke", pair(m.glyph, m.glyph_stroke)),
        (
            "`opacity.disabled`",
            format!("{:.0} %", t.opacity.disabled * 100.0),
        ),
        (
            "Gap between cards, and from cards to the window edge",
            num(m.card_gap),
        ),
        ("Check box and radio mark", num(m.mark)),
        (
            "Switch track and knob",
            format!(
                "{} × {}, {}",
                num(m.switch_track.x),
                num(m.switch_track.y),
                num(m.switch_knob)
            ),
        ),
        ("Swatch", num(m.swatch)),
        ("Tree indent per level", num(m.tree_indent)),
        ("Window control width", num(m.window_control)),
        ("Drag gap kept beside the window controls", num(m.drag_gap)),
        (
            "Command field width",
            format!(
                "{} to {}",
                num(m.command_field_min),
                num(m.command_field_max)
            ),
        ),
        (
            "Scrollbar thumb, at rest and hovered",
            pair(m.scrollbar, m.scrollbar_hover),
        ),
        (
            "Progress rail and slider rail",
            pair(m.progress_rail, m.slider_rail),
        ),
    ];
    for (name, value) in &rows {
        row(out, &[name, value]);
    }
    out.push_str(
        "\nThe rows without a token name are dimensions the gallery drew as plain numbers. \
         The first build slice gives each a token, since a recipe holds no numbers of its \
         own.\n\n",
    );
}

fn elevation_values(out: &mut String, light: &Tokens, dark: &Tokens) {
    out.push_str(
        "### Elevation values\n\n\
         Offset x / y, blur, colour. Spread is zero.\n\n",
    );
    head(out, &["Token", "Light", "Dark"]);
    let (l, d) = (&light.elevation, &dark.elevation);
    for (name, a, b) in [
        ("`elevation.card`", l.card, d.card),
        ("`elevation.popup`", l.popup, d.popup),
        ("`elevation.window`", l.window, d.window),
    ] {
        row(out, &[name, &shadow(a), &shadow(b)]);
    }
    out.push('\n');
}

fn motion_values(out: &mut String, t: &Tokens) {
    let m = &t.motion;
    let _ = write!(
        out,
        "### Motion values\n\n\
         `motion.fast` is {} ms and `motion.base` is {} ms. Milliseconds in / out; ease-out \
         unless noted.\n\n",
        ms(m.fast),
        ms(m.base)
    );
    head(out, &["Transition", "In", "Out", "What moves"]);
    let time = |seconds: f32, easing: Easing| match (seconds, easing) {
        (0.0, _) => "at once".to_owned(),
        (s, Easing::Out) => ms(s),
        (s, Easing::Linear) => format!("{}, linear", ms(s)),
    };
    let rows: [(&str, Transition, &str); 15] = [
        (
            "Hover fill, scrollbar widening",
            m.hover,
            "the fill fades out; the thumb narrows",
        ),
        ("Press fill", m.press, "fades out"),
        (
            "Check box, radio, toggle on",
            m.toggle,
            "the fill fades, the mark grows",
        ),
        (
            "Switch",
            m.switch,
            "the knob slides, the track colour mixes",
        ),
        ("Tab accent line", m.tab_line, "fades in on the new tab"),
        (
            "Menu, context menu, combo list",
            m.menu,
            "fade + 4 px from the anchor",
        ),
        ("Tooltip", m.tooltip, "fade + 4 px, after a 500 ms wait"),
        ("Flyout", m.flyout, "fade + 8 px from the strip"),
        (
            "Dock collapse",
            m.dock_collapse,
            "width; contents clipped and cross-faded",
        ),
        (
            "Panel section",
            m.section,
            "rows revealed from the top, the chevron turns",
        ),
        (
            "Dialog and scrim",
            m.dialog,
            "fade, grows from 97 %; the scrim fades with it",
        ),
        (
            "Command Palette",
            m.palette,
            "fade + 4 px drop from the command field",
        ),
        ("Selected row", m.selected_row, ""),
        (
            "Bars on the canvas",
            m.canvas_bar,
            "a Tool's bar: fade + 4 px; an on state fades",
        ),
        ("Progress bar", m.progress, "catches up with its value"),
    ];
    for (name, t, what) in rows {
        let leave = t.leave.map(|s| time(s, t.easing)).unwrap_or_default();
        row(out, &[name, &time(t.enter, Easing::Out), &leave, what]);
    }
    out.push('\n');
    head(out, &["Other", "Value"]);
    let travel = format!(
        "{} px (a flyout: {} px)",
        num(m.travel),
        num(m.flyout_travel)
    );
    row(out, &["Travel", &travel]);
    row(
        out,
        &[
            "Dialog scale at the start",
            &format!("{:.0} %", m.dialog_scale * 100.0),
        ],
    );
    row(
        out,
        &["Tooltip wait", &format!("{} ms", ms(m.tooltip_wait))],
    );
    row(
        out,
        &[
            "Tooltip shown at once after another",
            &format!("within {} ms", ms(m.tooltip_chain)),
        ],
    );
}

/// Everything between the two markers, each on its own line.
fn region() -> String {
    let at = |theme, density| tokens(theme, density, Settings::DEFAULT_TEXT_SIZE);
    let light = at(Theme::Light, Density::Comfortable);
    let dark = at(Theme::Dark, Density::Comfortable);
    let compact = at(Theme::Dark, Density::Compact);
    let mut out = format!("{NOTE}\n\n");
    colour_values(&mut out, &light, &dark);
    icon_roles(&mut out, &light, &dark);
    space_values(&mut out, &dark, &compact);
    type_values(&mut out);
    fixed_values(&mut out, &dark);
    elevation_values(&mut out, &light, &dark);
    motion_values(&mut out, &dark);
    out
}

/// `doc` with what lies between the markers replaced by `region`.
fn with_region(doc: &str, region: &str) -> String {
    let begin = doc.find(BEGIN).expect("the spec has the begin marker") + BEGIN.len();
    let end = doc.find(END).expect("the spec has the end marker");
    assert!(begin < end, "the markers are out of order");
    format!("{}\n{region}\n{}", &doc[..begin], &doc[end..])
}

#[test]
fn the_specs_value_tables_are_the_tokens() {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../docs/spec/v0.1/design-language.md");
    let doc = std::fs::read_to_string(&path).unwrap();
    let fresh = with_region(&doc, &region());
    if std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1") {
        std::fs::write(&path, &fresh).unwrap();
        return;
    }
    assert!(
        doc == fresh,
        "the tables between the tokens markers in {} are stale: run `cargo xtask tokens`",
        path.display()
    );
}

#[test]
fn only_the_region_between_the_markers_is_rewritten() {
    let doc = format!("# Before\n\n{BEGIN}\nold\n{END}\n\n## After\n");
    let fresh = with_region(&doc, "new\n");
    assert_eq!(
        fresh,
        format!("# Before\n\n{BEGIN}\nnew\n\n{END}\n\n## After\n")
    );
    // Regenerating what is fresh changes nothing.
    assert_eq!(with_region(&fresh, "new\n"), fresh);
}

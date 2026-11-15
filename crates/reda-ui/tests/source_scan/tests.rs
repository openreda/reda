//! The scan's own rules, checked on small pieces of source.

use crate::rules::{Finding, StockWidget, scan};
use crate::{Lists, check};

/// The patterns the scan reports in `source`, in line order.
fn banned(source: &str) -> Vec<String> {
    scan(source, &[])
        .findings
        .into_iter()
        .map(|finding| finding.pattern)
        .collect()
}

fn clean(source: &str) -> bool {
    banned(source).is_empty()
}

fn files(files: &[(&str, &str)]) -> Vec<(String, String)> {
    files
        .iter()
        .map(|(path, text)| ((*path).to_owned(), (*text).to_owned()))
        .collect()
}

const NO_LISTS: Lists = Lists {
    exempt: &[],
    unconverted: &[],
    stock_widgets: &[],
};

#[test]
fn a_colour_constructor_is_banned() {
    assert_eq!(
        banned("let c = Color32::from_rgb(1, 2, 3);"),
        ["`Color32::from_*`"]
    );
    assert_eq!(
        banned("let c = egui::Color32::from_black_alpha(96);"),
        ["`Color32::from_*`"]
    );
    assert_eq!(
        banned("let c = egui::Rgba::from_gray(0.5);"),
        ["`Rgba::from_*`"]
    );
    assert_eq!(
        banned("let c = egui::hex_color!(\"fff\");"),
        ["`hex_color!`"]
    );
}

#[test]
fn a_named_colour_constant_is_banned_but_the_transparent_zero_is_not() {
    assert_eq!(
        banned("let c = Color32::WHITE;"),
        ["named colour `Color32::WHITE`"]
    );
    assert_eq!(
        banned("let c = egui::Color32::DARK_RED;"),
        ["named colour `Color32::DARK_RED`"]
    );
    assert!(clean("let c = Color32::TRANSPARENT;"));
    assert!(clean("fn paint(colour: Color32) -> Color32 { colour }"));
}

#[test]
fn blending_a_colour_by_hand_is_banned() {
    assert_eq!(
        banned("let c = colour.gamma_multiply(0.5);"),
        ["`gamma_multiply`"]
    );
    assert_eq!(
        banned("let c = colour.linear_multiply(0.5);"),
        ["`linear_multiply`"]
    );
}

#[test]
fn a_hex_colour_is_banned_in_code_and_in_a_string() {
    assert_eq!(banned("const KEY: u32 = 0x8a93a0;"), ["hex colour literal"]);
    assert_eq!(
        banned("const KEY: u32 = 0x8A_93_A0_FF;"),
        ["hex colour literal"]
    );
    assert_eq!(
        banned("let palette = [\"#e6194b\", \"#fff\"];"),
        ["hex colour literal"]
    );
    assert_eq!(
        banned("let css = r#\"fill: #1f5fbfcc;\"#;"),
        ["hex colour literal"]
    );
}

#[test]
fn a_hex_number_that_is_not_a_colour_is_allowed() {
    assert!(clean("let low = value & 0xff;"));
    assert!(clean(
        "const ID: u128 = 0x0000_0000_0000_4000_8000_7072_6576_6965;"
    ));
    assert!(clean("let text = \"see #294 and #12\";"));
    assert_eq!(banned("let text = \"#123456\";"), ["hex colour literal"]);
    assert!(clean("let text = \"a#abcdefg\";"));
}

#[test]
fn reading_or_setting_the_egui_style_is_banned() {
    for (source, pattern) in [
        ("let v = ui.visuals();", "`.visuals()`"),
        ("ui.visuals_mut().dark_mode = true;", "`.visuals_mut()`"),
        ("let s = ui.spacing().item_spacing;", "`.spacing()`"),
        ("ui.spacing_mut().item_spacing.x = gap;", "`.spacing_mut()`"),
        ("let frame = egui::Frame::popup(ui.style());", "`.style()`"),
        ("ui.style_mut().wrap_mode = None;", "`.style_mut()`"),
        (
            "let s = ctx.style_of(egui::Theme::Dark);",
            "`.style_of(..)`",
        ),
        (
            "ctx.all_styles_mut(|style| change(style));",
            "`all_styles_mut`",
        ),
        ("ctx.set_style(style);", "`set_style*`"),
        (
            "ctx.set_style_of(egui::Theme::Dark, style);",
            "`set_style*`",
        ),
        ("ctx.set_visuals(visuals);", "`set_visuals*`"),
        (
            "ctx.set_visuals_of(egui::Theme::Dark, visuals);",
            "`set_visuals*`",
        ),
    ] {
        assert_eq!(banned(source), [pattern], "{source}");
    }
}

#[test]
fn a_method_chain_broken_over_lines_is_still_caught() {
    let source = "let colour = ui\n    .visuals()\n    .text_color();";
    assert_eq!(
        scan(source, &[]).findings,
        [Finding {
            line: 2,
            pattern: "`.visuals()`".to_owned()
        }]
    );
}

#[test]
fn a_name_that_only_ends_like_a_banned_one_is_allowed() {
    assert!(clean("self.reset_style_cache();"));
    assert!(clean(
        "let grid = egui::Grid::new(id).spacing(t.space.grid);"
    ));
    assert!(clean("let size = galley.size();"));
}

#[test]
fn a_literal_size_is_banned() {
    for (source, pattern) in [
        ("ui.add_space(8.0);", "`add_space(<n>)`"),
        ("ui.add_space(-4.0);", "`add_space(<n>)`"),
        ("ui.add_space(4.0 * 2.0);", "`add_space(<n>)`"),
        (
            "let s = Stroke::new(1.5, colour);",
            "`Stroke::new(<n>, ..)`",
        ),
        (
            "let s = egui::epaint::PathStroke::new(2.0_f32, colour);",
            "`Stroke::new(<n>, ..)`",
        ),
        (
            "let text = RichText::new(label).size(11.5);",
            "`.size(<n>)`",
        ),
        (
            "let r = CornerRadius::same(4);",
            "`CornerRadius` from a literal",
        ),
        (
            "let r = egui::CornerRadius { nw: 4, ne: 4, sw: 0, se: 0 };",
            "`CornerRadius` from a literal",
        ),
        (
            "let frame = frame.corner_radius(4.0);",
            "`CornerRadius` from a literal",
        ),
        (
            "painter.rect_filled(rect, 3.0, fill);",
            "`CornerRadius` from a literal",
        ),
        (
            "painter.rect_stroke(rect, 3.0, stroke, kind);",
            "`CornerRadius` from a literal",
        ),
        (
            "let m = egui::Margin::symmetric(6, 0);",
            "`Margin` from a literal",
        ),
        (
            "let frame = frame.inner_margin(6.0);",
            "`Margin` from a literal",
        ),
        (
            "let frame = frame.outer_margin(egui::vec2(0.0, 2.0));",
            "`Margin` from a literal",
        ),
        (
            "let s = Shadow { offset: [0, 2], blur: 8, spread: 0, color };",
            "`Shadow` from a literal",
        ),
        (
            "let grid = egui::Grid::new(id).spacing([12.0, 6.0]);",
            "literal `vec2` in `.spacing(..)`",
        ),
        (
            "let b = Button::new(text).min_size(egui::vec2(80.0, 0.0));",
            "literal `vec2` in `.min_size(..)`",
        ),
        (
            "ui.set_min_size(Vec2::splat(24.0));",
            "literal `vec2` in `set_min_size(..)`",
        ),
    ] {
        assert_eq!(banned(source), [pattern], "{source}");
    }
}

#[test]
fn a_literal_size_split_over_lines_is_caught_on_the_line_it_starts() {
    let source = "painter.line_segment(\n    [a, b],\n    Stroke::new(\n        1.5,\n        colour,\n    ),\n);";
    assert_eq!(
        scan(source, &[]).findings,
        [Finding {
            line: 3,
            pattern: "`Stroke::new(<n>, ..)`".to_owned()
        }]
    );
}

#[test]
fn building_a_font_from_a_size_is_banned() {
    assert_eq!(
        banned("let font = FontId::proportional(12.0);"),
        ["`FontId::*`"]
    );
    assert_eq!(
        banned("let font = egui::FontId::new(size, family);"),
        ["`FontId::*`"]
    );
    assert_eq!(
        banned("let font = FontId { size, family };"),
        ["`FontId { .. }`"]
    );
    assert!(clean("fn draw(font: FontId) {}"));
}

#[test]
fn zero_is_allowed_everywhere() {
    assert!(clean("ui.add_space(0.0);"));
    assert!(clean("let s = Stroke::new(0.0, colour);"));
    assert!(clean("let r = CornerRadius::same(0);"));
    assert!(clean("let r = CornerRadius::ZERO;"));
    assert!(clean("let m = egui::Margin::symmetric(0, 0);"));
    assert!(clean(
        "let frame = frame.inner_margin(0).corner_radius(0.0_f32);"
    ));
    assert!(clean("painter.rect_filled(rect, 0.0, fill);"));
    assert!(clean("let grid = egui::Grid::new(id).spacing([0.0, 0.0]);"));
    assert!(clean("let s = Shadow::NONE;"));
}

#[test]
fn arithmetic_on_token_values_is_allowed_everywhere() {
    assert!(clean("ui.add_space(t.space.gap);"));
    assert!(clean("ui.add_space(t.space.gap * 2.0);"));
    assert!(clean("ui.add_space(t.space.gap + 4.0);"));
    assert!(clean(
        "let s = Stroke::new(t.stroke.hairline, t.color.stroke.hairline);"
    ));
    assert!(clean(
        "let s = Stroke::new(t.stroke.hairline * 2.0, colour);"
    ));
    assert!(clean("let r = CornerRadius::same(t.radius.control - 2);"));
    assert!(clean(
        "let r = CornerRadius { nw: t.radius.card, ne: t.radius.card, sw: 0, se: 0 };"
    ));
    assert!(clean(
        "let m = egui::Margin::symmetric(t.space.pad_x, t.space.pad_y / 2);"
    ));
    assert!(clean(
        "let b = button.min_size(egui::vec2(t.control_height() * 2.0, 0.0));"
    ));
    assert!(clean("painter.rect_filled(rect, t.radius.control, fill);"));
    assert!(clean("let text = t.ty.small.rich(label);"));
}

#[test]
fn a_type_named_in_a_signature_is_not_a_literal_size() {
    assert!(clean(
        "fn margin(t: &Tokens) -> Margin {\n    t.space.card.into()\n}"
    ));
    assert!(clean(
        "impl From<Pad> for Margin {\n    fn from(pad: Pad) -> Self { pad.0 }\n}"
    ));
    assert!(clean("fn frame(radius: CornerRadius, shadow: Shadow) {}"));
}

#[test]
fn a_pattern_in_a_comment_or_in_text_is_not_a_use() {
    assert!(clean(
        "// The old code read ui.visuals() and Color32::WHITE here.\nlet x = 1;"
    ));
    assert!(clean(
        "/// `ui.add_space(8.0)` is what this replaces.\nfn gap() {}"
    ));
    assert!(clean(
        "/* Stroke::new(1.0, c) /* nested */ Color32::RED */ let x = 1;"
    ));
    assert!(clean(
        "let message = \"call ui.visuals() or Color32::from_rgb(1, 2, 3)\";"
    ));
    assert!(clean(
        "let message = r#\"a \"quoted\" ui.spacing() here\"#;"
    ));
}

#[test]
fn code_after_a_string_a_character_or_a_lifetime_is_still_scanned() {
    assert_eq!(
        banned("let a = \"x \\\" y\"; let v = ui.visuals();"),
        ["`.visuals()`"]
    );
    assert_eq!(
        banned("let quote = '\"'; let v = ui.visuals();"),
        ["`.visuals()`"]
    );
    assert_eq!(
        banned("let tick = '\\''; let v = ui.visuals();"),
        ["`.visuals()`"]
    );
    assert_eq!(
        banned("fn f<'a>(ui: &'a Ui) -> &'a Visuals { ui.visuals() }"),
        ["`.visuals()`"]
    );
}

#[test]
fn the_stock_widget_table_bans_what_it_lists_and_nothing_before_it_is_listed() {
    let source = "if ui.button(label).clicked() {}\nlet b = egui::Button::new(label);";
    assert!(scan(source, &[]).findings.is_empty());

    let table = [
        StockWidget {
            pattern: "ui.button(",
            instead: "`widgets::button`",
        },
        StockWidget {
            pattern: "Button::new",
            instead: "`widgets::button`",
        },
    ];
    assert_eq!(
        scan(source, &table).findings,
        [
            Finding {
                line: 1,
                pattern: "raw stock widget `ui.button(` (use `widgets::button`)".to_owned()
            },
            Finding {
                line: 2,
                pattern: "raw stock widget `Button::new` (use `widgets::button`)".to_owned()
            },
        ]
    );
    assert!(
        scan("let b = ImageButton::new(image); gui.button(x);", &table)
            .findings
            .is_empty()
    );
}

#[test]
fn the_stock_widget_table_starts_empty() {
    // Each widget slice adds its patterns when its REDA widget lands; this one adds none.
    // Delete this test with the first entry.
    assert!(crate::lists::STOCK_WIDGETS.is_empty());
}

#[test]
fn a_marker_with_a_reason_exempts_the_next_line_only() {
    let source = "// theme-exempt: a Layer's own colour is data\nlet a = Color32::from_rgb(r, g, b);\nlet b = Color32::from_rgb(r, g, b);";
    let scan = scan(source, &[]);
    assert_eq!(
        scan.findings,
        [Finding {
            line: 3,
            pattern: "`Color32::from_*`".to_owned()
        }]
    );
    assert_eq!(scan.markers, 1);
}

#[test]
fn a_marker_with_no_reason_fails_and_exempts_nothing() {
    for marker in [
        "// theme-exempt",
        "// theme-exempt:",
        "    // theme-exempt:   ",
    ] {
        let scan = scan(&format!("{marker}\nlet a = Color32::WHITE;"), &[]);
        assert_eq!(
            scan.findings,
            [
                Finding {
                    line: 1,
                    pattern: "`// theme-exempt` with no reason".to_owned()
                },
                Finding {
                    line: 2,
                    pattern: "named colour `Color32::WHITE`".to_owned()
                },
            ],
            "{marker:?}"
        );
        assert_eq!(scan.markers, 0);
    }
}

#[test]
fn the_markers_are_totalled_over_every_file_read() {
    let marked = "// theme-exempt: data colour\nlet a = Color32::WHITE;\n// theme-exempt: data colour\nlet b = Color32::BLACK;";
    let report = check(
        &files(&[("a.rs", marked), ("b/c.rs", marked), ("d.rs", "let x = 1;")]),
        &NO_LISTS,
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.markers, 4);
}

#[test]
fn a_failure_names_the_file_the_line_and_the_pattern() {
    let report = check(
        &files(&[(
            "panels/find.rs",
            "fn draw(ui: &mut Ui) {\n    ui.add_space(8.0);\n    let v = ui.visuals();\n}",
        )]),
        &NO_LISTS,
    );
    assert_eq!(
        report.failures,
        [
            "crates/reda-ui/src/panels/find.rs:2: `add_space(<n>)`",
            "crates/reda-ui/src/panels/find.rs:3: `.visuals()`"
        ]
    );
}

#[test]
fn an_exempt_module_is_not_read() {
    let dirty = "let c = Color32::WHITE;";
    let lists = Lists {
        exempt: &["theme.rs", "theme/", "canvas/markers.rs"],
        ..NO_LISTS
    };
    let report = check(
        &files(&[
            ("theme.rs", dirty),
            ("theme/style.rs", dirty),
            ("canvas/markers.rs", dirty),
            ("theme_picker.rs", dirty),
            ("canvas/markers/extra.rs", dirty),
        ]),
        &lists,
    );
    assert_eq!(
        report.failures,
        [
            "crates/reda-ui/src/theme_picker.rs:1: named colour `Color32::WHITE`",
            "crates/reda-ui/src/canvas/markers/extra.rs:1: named colour `Color32::WHITE`"
        ]
    );
}

#[test]
fn a_listed_file_is_skipped_and_an_unlisted_file_must_be_clean() {
    let dirty = "let c = Color32::WHITE;";
    let lists = Lists {
        unconverted: &["old.rs"],
        ..NO_LISTS
    };
    let report = check(&files(&[("new.rs", dirty), ("old.rs", dirty)]), &lists);
    assert_eq!(
        report.failures,
        ["crates/reda-ui/src/new.rs:1: named colour `Color32::WHITE`"]
    );
}

#[test]
fn a_clean_file_still_on_the_list_fails() {
    let lists = Lists {
        unconverted: &["done.rs"],
        ..NO_LISTS
    };
    let report = check(&files(&[("done.rs", "let gap = t.space.gap;")]), &lists);
    assert_eq!(
        report.failures,
        ["crates/reda-ui/src/done.rs: is clean, so take it off the unconverted list"]
    );
}

#[test]
fn the_unconverted_list_names_only_files_that_exist_and_are_scanned() {
    let dirty = "let c = Color32::WHITE;";
    let lists = Lists {
        exempt: &["theme/"],
        unconverted: &["gone.rs", "theme/style.rs"],
        ..NO_LISTS
    };
    let report = check(&files(&[("theme/style.rs", dirty)]), &lists);
    assert_eq!(
        report.failures,
        [
            "crates/reda-ui/src/gone.rs: is on the unconverted list, but there is no such file",
            "crates/reda-ui/src/theme/style.rs: is exempt, so it does not belong on the unconverted list",
        ]
    );
}

#[test]
fn the_unconverted_list_is_sorted_with_no_file_twice() {
    let dirty = "let c = Color32::WHITE;";
    let sorted = "the unconverted list must be sorted, with no file twice";
    for unconverted in [&["b.rs", "a.rs"], &["a.rs", "a.rs"]] {
        let report = check(
            &files(&[("a.rs", dirty), ("b.rs", dirty)]),
            &Lists {
                unconverted,
                ..NO_LISTS
            },
        );
        assert_eq!(
            report.failures.first().map(String::as_str),
            Some(sorted),
            "{unconverted:?}"
        );
    }
    let report = check(
        &files(&[("a.rs", dirty), ("b.rs", dirty)]),
        &Lists {
            unconverted: &["a.rs", "b.rs"],
            ..NO_LISTS
        },
    );
    assert!(report.failures.is_empty(), "{:?}", report.failures);
}

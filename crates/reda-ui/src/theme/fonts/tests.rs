use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use egui::{FontFamily, FontId};

use super::*;

fn face(file: &str, weight: u16) -> Face {
    Face {
        file: PathBuf::from(file),
        index: 0,
        weight,
        weight_axis: None,
        width: 5,
        italic: false,
        fixed_pitch: false,
        latin: true,
    }
}

fn family(name: &str, faces: impl IntoIterator<Item = Face>) -> Family {
    Family {
        name: name.to_owned(),
        faces: faces.into_iter().collect(),
    }
}

/// The weights the three styles are drawn at, and the file each comes from.
fn picks(family: &Family) -> [(u16, &str); 3] {
    Weight::ALL.map(|w| {
        let pick = family.pick(w).unwrap();
        (pick.weight, pick.face.file.to_str().unwrap())
    })
}

/// Installed fonts held in memory: families by lower-case name, files by path.
#[derive(Default)]
struct Fake {
    families: BTreeMap<String, Family>,
    files: BTreeMap<PathBuf, Arc<[u8]>>,
}

impl Fake {
    fn with(mut self, name: &str, faces: impl IntoIterator<Item = (Face, Vec<u8>)>) -> Fake {
        let mut listed = Vec::new();
        for (face, bytes) in faces {
            self.files.insert(face.file.clone(), bytes.into());
            listed.push(face);
        }
        self.families
            .insert(name.to_lowercase(), family(name, listed));
        self
    }
}

impl Source for Fake {
    fn family(&self, name: &str) -> Option<Family> {
        self.families.get(&name.to_lowercase()).cloned()
    }

    fn families(&self) -> Vec<Family> {
        self.families.values().cloned().collect()
    }

    fn read(&self, face: &Face) -> Option<Arc<[u8]>> {
        self.files.get(&face.file).cloned()
    }
}

fn ui(name: &str) -> Choice {
    Choice {
        ui: Some(name.to_owned()),
        mono: None,
    }
}

fn chain(resolved: &Resolved, family: &str) -> Vec<String> {
    resolved.definitions.families[&FontFamily::Name(family.into())].clone()
}

/// Lays `text` out in every REDA family on a fresh context with `resolved` installed, the
/// way the window does. Panics if egui cannot use a font.
fn lay_out(resolved: &Resolved, text: &str) {
    let ctx = egui::Context::default();
    ctx.set_fonts(resolved.definitions.clone());
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        for family in [REGULAR, MEDIUM, SEMIBOLD, NUMERIC, MONO] {
            let font = FontId::new(13.0, FontFamily::Name(family.into()));
            ui.label(egui::RichText::new(text).font(font));
        }
    });
    output.textures_delta.clear();
}

/// The advance of each character of `text` in `family`, in a context with `resolved`.
fn advances(resolved: &Resolved, family: &FontFamily, text: &str) -> Vec<f32> {
    let ctx = egui::Context::default();
    ctx.set_fonts(resolved.definitions.clone());
    let mut out = Vec::new();
    let mut output = ctx.run_ui(egui::RawInput::default(), |ui| {
        let font = FontId::new(13.0, family.clone());
        out = text
            .chars()
            .map(|ch| ui.fonts_mut(|f| f.glyph_width(&font, ch)))
            .collect();
    });
    output.textures_delta.clear();
    out
}

fn one_width(widths: &[f32]) -> bool {
    widths.iter().all(|w| *w == widths[0])
}

// ---- Weight resolution ----

#[test]
fn a_weight_the_family_has_is_used_as_it_is() {
    let f = family(
        "Full",
        [
            face("light", 300),
            face("regular", 400),
            face("medium", 500),
            face("semibold", 600),
            face("bold", 700),
        ],
    );
    assert_eq!(
        picks(&f),
        [(400, "regular"), (500, "medium"), (600, "semibold")]
    );
    assert_eq!(f.weights(), [300, 400, 500, 600, 700]);
}

#[test]
fn a_missing_weight_takes_the_nearest_static_weight() {
    // Regular and bold only: medium is nearer regular; semibold is as near to bold as to
    // medium-less regular... 600 is 200 from 400 and 100 from 700.
    let f = family("Two", [face("regular", 400), face("bold", 700)]);
    assert_eq!(
        picks(&f),
        [(400, "regular"), (400, "regular"), (700, "bold")]
    );
    // Of two equally near, medium takes the lighter and semibold the heavier.
    let f = family("Gaps", [face("regular", 400), face("semibold", 600)]);
    assert_eq!(f.pick(Weight::Medium).unwrap().weight, 400);
    let f = family("Gaps", [face("medium", 500), face("bold", 700)]);
    assert_eq!(f.pick(Weight::Semibold).unwrap().weight, 700);
    // No regular at all: the nearest to it.
    let f = family("Heavy", [face("bold", 700), face("black", 900)]);
    assert_eq!(picks(&f), [(700, "bold"), (700, "bold"), (700, "bold")]);
}

#[test]
fn a_regular_only_family_is_never_emboldened() {
    let f = family("One", [face("regular", 400)]);
    assert_eq!(
        picks(&f),
        [(400, "regular"), (400, "regular"), (400, "regular")]
    );
    assert!(Weight::ALL.iter().all(|w| !f.pick(*w).unwrap().on_axis));
    // What egui is handed for medium and semibold is the regular file, untouched: no
    // weight coordinate, and egui has no synthetic bold.
    let source = Fake::default().with("One", [(face("regular", 400), INTER_REGULAR.to_vec())]);
    let resolved = resolve(&ui("One"), &source);
    assert_eq!(resolved.report.weights, [400, 400, 400]);
    for key in ["reda-chosen-ui-500", "reda-chosen-ui-600"] {
        let data = &resolved.definitions.font_data[key];
        assert!(data.bytes() == INTER_REGULAR);
        assert_eq!(data.tweak, egui::epaint::text::FontTweak::default());
    }
}

#[test]
fn a_variable_font_sets_each_weight_on_its_axis() {
    let variable = Face {
        weight_axis: Some((100, 900)),
        ..face("variable", 400)
    };
    let f = family("Variable", [variable.clone(), face("static-bold", 700)]);
    for weight in Weight::ALL {
        let pick = f.pick(weight).unwrap();
        assert_eq!((pick.weight, pick.on_axis), (weight.class(), true));
        assert_eq!(pick.face.file, PathBuf::from("variable"));
    }
    assert_eq!(f.weights(), [100, 700, 900]);
    // An axis that stops short gives its nearest end.
    let narrow = Face {
        weight_axis: Some((300, 450)),
        ..face("narrow", 400)
    };
    let f = family("Narrow", [narrow]);
    assert_eq!(f.pick(Weight::Semibold).unwrap().weight, 450);

    // egui is handed the weight as a variation coordinate.
    let source = Fake::default().with("Variable", [(variable, INTER_REGULAR.to_vec())]);
    let resolved = resolve(&ui("Variable"), &source);
    assert_eq!(resolved.report.weights, [400, 500, 600]);
    let coords = &resolved.definitions.font_data["reda-chosen-ui-600"]
        .tweak
        .coords;
    assert_eq!(
        *coords,
        egui::epaint::text::VariationCoords::new([(b"wght", 600.0)])
    );
}

#[test]
fn upright_faces_of_normal_width_come_before_italic_and_condensed_ones() {
    let italic = |file, weight| Face {
        italic: true,
        ..face(file, weight)
    };
    let condensed = |file, weight| Face {
        width: 3,
        ..face(file, weight)
    };
    let f = family(
        "Mixed",
        [
            italic("medium-italic", 500),
            condensed("condensed-semibold", 600),
            face("regular", 400),
        ],
    );
    assert_eq!(
        picks(&f),
        [(400, "regular"), (400, "regular"), (400, "regular")]
    );
    assert_eq!(f.weights(), [400, 600]);
    // An italic-only family is still usable.
    let f = family("Slanted", [italic("italic", 400)]);
    assert_eq!(
        f.pick(Weight::Regular).unwrap().face.file,
        PathBuf::from("italic")
    );
    assert!(!f.has_latin_regular());
    assert!(family("Empty", []).pick(Weight::Regular).is_none());
}

// ---- What the font lists hold ----

#[test]
fn a_family_is_fixed_pitch_when_its_regular_face_is() {
    let fixed = |file, weight| Face {
        fixed_pitch: true,
        ..face(file, weight)
    };
    assert!(family("Mono", [fixed("regular", 400), fixed("bold", 700)]).is_fixed_pitch());
    assert!(!family("Sans", [face("regular", 400)]).is_fixed_pitch());
    // A stray fixed-pitch bold does not make the family monospace.
    assert!(!family("Odd", [face("regular", 400), fixed("bold", 700)]).is_fixed_pitch());
}

#[test]
fn the_ui_list_wants_a_regular_face_that_covers_latin() {
    assert!(family("Sans", [face("regular", 400)]).has_latin_regular());
    assert!(!family("Display", [face("black", 900)]).has_latin_regular());
    let symbols = Face {
        latin: false,
        ..face("symbols", 400)
    };
    assert!(!family("Symbols", [symbols]).has_latin_regular());
}

// ---- The bundled faces ----

#[test]
fn every_bundled_face_passes_validation() {
    for (key, bytes) in BUNDLED {
        assert_eq!(validate(bytes, 0), Ok(()), "{key}");
    }
}

#[test]
fn with_nothing_chosen_every_family_is_a_bundled_face_then_eguis_fonts() {
    let resolved = resolve(&Choice::default(), &NoFonts);
    assert_eq!(
        resolved.report,
        Report {
            ui: Outcome::Bundled,
            mono: Outcome::Bundled,
            weights: [400, 500, 600],
            numeric_is_chosen: false,
        }
    );
    let own = egui::FontDefinitions::default();
    let proportional = &own.families[&FontFamily::Proportional];
    let monospace = &own.families[&FontFamily::Monospace];
    for (family, first, rest) in [
        (REGULAR, "reda-inter-regular", proportional),
        (MEDIUM, "reda-inter-medium", proportional),
        (SEMIBOLD, "reda-inter-semibold", proportional),
        (NUMERIC, "reda-inter-tabular", proportional),
        (MONO, "reda-jetbrains-mono", monospace),
    ] {
        let chain = chain(&resolved, family);
        assert_eq!(chain[0], first, "{family}");
        assert_eq!(&chain[1..], &rest[..], "{family}");
    }
    // Text that names no REDA family is in the same faces.
    let families = &resolved.definitions.families;
    assert_eq!(
        families[&FontFamily::Proportional],
        chain(&resolved, REGULAR)
    );
    assert_eq!(families[&FontFamily::Monospace], chain(&resolved, MONO));
    // Each weight is its own file, not one file three times.
    let data = &resolved.definitions.font_data;
    assert!(data["reda-inter-regular"].bytes() != data["reda-inter-medium"].bytes());
    assert!(data["reda-inter-medium"].bytes() != data["reda-inter-semibold"].bytes());
    lay_out(&resolved, "Net 0123456789 -> ≥ µ");
}

#[test]
fn the_named_families_of_the_type_scale_are_the_ones_bound() {
    let resolved = resolve(&Choice::default(), &NoFonts);
    let named = crate::theme::Families::named();
    for family in [
        named.regular,
        named.medium,
        named.semibold,
        named.numeric,
        named.mono,
    ] {
        assert!(
            resolved.definitions.families.contains_key(&family),
            "{family:?}"
        );
    }
}

#[test]
fn numeric_text_has_digits_of_one_width_and_running_text_does_not() {
    let resolved = resolve(&Choice::default(), &NoFonts);
    let digits = "0123456789";
    let regular = advances(&resolved, &FontFamily::Name(REGULAR.into()), digits);
    let numeric = advances(&resolved, &FontFamily::Name(NUMERIC.into()), digits);
    assert!(!one_width(&regular), "Inter's own digits: {regular:?}");
    assert!(one_width(&numeric), "the baked digits: {numeric:?}");
    // The monospace face is one width throughout.
    let mono = advances(&resolved, &FontFamily::Name(MONO.into()), "iW0.m");
    assert!(one_width(&mono), "{mono:?}");
}

// ---- A chosen family ----

/// "Fancy" has a regular and a semibold (Inter's files under another name); "Fixed" is
/// JetBrains Mono under another name.
fn installed() -> Fake {
    let fixed = Face {
        fixed_pitch: true,
        ..face("/fonts/Fixed-Regular.ttf", 400)
    };
    Fake::default()
        .with(
            "Fancy",
            [
                (
                    face("/fonts/Fancy-Regular.ttf", 400),
                    INTER_REGULAR.to_vec(),
                ),
                (
                    face("/fonts/Fancy-SemiBold.ttf", 600),
                    INTER_SEMIBOLD.to_vec(),
                ),
            ],
        )
        .with("Fixed", [(fixed, JETBRAINS_MONO.to_vec())])
}

#[test]
fn a_chosen_family_goes_first_with_the_bundled_face_behind_it() {
    let choice = Choice {
        ui: Some("fancy".to_owned()),
        mono: Some("FIXED".to_owned()),
    };
    let resolved = resolve(&choice, &installed());
    assert_eq!(resolved.report.ui, Outcome::Installed);
    assert_eq!(resolved.report.mono, Outcome::Installed);
    // Medium has no face of its own: it takes the regular, and semibold its own.
    assert_eq!(resolved.report.weights, [400, 400, 600]);
    let data = &resolved.definitions.font_data;
    assert!(data["reda-chosen-ui-400"].bytes() == INTER_REGULAR);
    assert!(data["reda-chosen-ui-500"].bytes() == INTER_REGULAR);
    assert!(data["reda-chosen-ui-600"].bytes() == INTER_SEMIBOLD);
    assert!(data["reda-chosen-mono"].bytes() == JETBRAINS_MONO);
    // Missing glyphs: the chosen face, then the bundled face, then egui's fonts.
    assert_eq!(
        chain(&resolved, MEDIUM)[..3],
        ["reda-chosen-ui-500", "reda-inter-medium", "Ubuntu-Light"]
    );
    assert_eq!(
        chain(&resolved, MONO)[..3],
        ["reda-chosen-mono", "reda-jetbrains-mono", "Hack"]
    );
    assert_eq!(
        resolved.definitions.families[&FontFamily::Proportional][0],
        "reda-chosen-ui-400"
    );
    lay_out(&resolved, "Net 0123456789");
}

#[test]
fn numeric_recipes_keep_the_bundled_tabular_inter_unless_the_chosen_digits_line_up() {
    // Fancy's digits are Inter's proportional ones.
    let resolved = resolve(&ui("Fancy"), &installed());
    assert!(!resolved.report.numeric_is_chosen);
    assert_eq!(chain(&resolved, NUMERIC)[0], "reda-inter-tabular");
    let numeric = advances(&resolved, &FontFamily::Name(NUMERIC.into()), "0123456789");
    assert!(one_width(&numeric));
    // A face whose digits are one width is used for numbers too.
    let resolved = resolve(&ui("Fixed"), &installed());
    assert!(resolved.report.numeric_is_chosen);
    assert_eq!(
        chain(&resolved, NUMERIC)[..2],
        ["reda-chosen-ui-400", "reda-inter-tabular"]
    );
}

#[test]
fn a_family_that_is_not_installed_falls_back_to_the_bundled_face() {
    let choice = Choice {
        ui: Some("Comic Sans".to_owned()),
        mono: Some("Courier Nowhere".to_owned()),
    };
    let resolved = resolve(&choice, &installed());
    assert_eq!(resolved.report.ui, Outcome::NotInstalled);
    assert_eq!(resolved.report.mono, Outcome::NotInstalled);
    assert!(resolved.report.ui.fell_back());
    assert_eq!(resolved.report.weights, [400, 500, 600]);
    // Exactly what nothing chosen gives.
    let bundled = resolve(&Choice::default(), &NoFonts);
    assert_eq!(resolved.definitions.families, bundled.definitions.families);
    // An empty name is no choice.
    let resolved = resolve(&ui("  "), &installed());
    assert_eq!(resolved.report.ui, Outcome::Bundled);
    // An indexed file that has since gone is "not installed" too.
    let gone = Fake {
        files: BTreeMap::new(),
        ..installed()
    };
    assert_eq!(
        resolve(&ui("Fancy"), &gone).report.ui,
        Outcome::NotInstalled
    );
}

// ---- Validation ----

#[test]
fn a_file_that_is_not_a_font_is_rejected() {
    assert_eq!(validate(b"", 0), Err(Invalid::NotAFont));
    assert_eq!(validate(b"hello, not a font", 0), Err(Invalid::NotAFont));
    assert_eq!(validate(&[0u8; 4096], 0), Err(Invalid::NotAFont));
    // A real font has no second face.
    assert_eq!(validate(INTER_REGULAR, 1), Err(Invalid::NotAFont));
    // Web fonts are not OpenType files.
    assert_eq!(validate(b"wOF2\0\x01\0\0", 0), Err(Invalid::NotAFont));
}

/// `font` with table `tag` renamed, so readers do not find it.
fn without_table(font: &[u8], tag: &[u8; 4]) -> Vec<u8> {
    let mut out = font.to_vec();
    let tables = usize::from(u16::from_be_bytes([font[4], font[5]]));
    for i in 0..tables {
        let at = 12 + 16 * i;
        if &out[at..at + 4] == tag {
            out[at..at + 4].copy_from_slice(b"zzzz");
        }
    }
    out
}

#[test]
fn a_font_without_the_tables_layout_needs_is_rejected() {
    for tag in [b"head", b"hhea", b"hmtx", b"maxp", b"cmap"] {
        let broken = without_table(INTER_REGULAR, tag);
        assert_eq!(
            validate(&broken, 0),
            Err(Invalid::BrokenTables),
            "{}",
            String::from_utf8_lossy(tag)
        );
    }
    assert_eq!(
        validate(&without_table(INTER_REGULAR, b"glyf"), 0),
        Err(Invalid::NoOutlines)
    );
}

#[test]
fn a_rejected_font_file_is_skipped_and_the_bundled_face_used() {
    let broken = without_table(INTER_REGULAR, b"cmap");
    let source = Fake::default()
        .with("Broken", [(face("/fonts/Broken.ttf", 400), broken)])
        .with(
            "Junk",
            [(face("/fonts/Junk.ttf", 400), b"not a font".to_vec())],
        );
    let choice = Choice {
        ui: Some("Broken".to_owned()),
        mono: Some("Junk".to_owned()),
    };
    let resolved = resolve(&choice, &source);
    assert_eq!(resolved.report.ui, Outcome::Rejected(Invalid::BrokenTables));
    assert_eq!(resolved.report.mono, Outcome::Rejected(Invalid::NotAFont));
    assert!(resolved.report.mono.fell_back());
    // The file never reaches egui.
    assert!(
        resolved
            .definitions
            .font_data
            .keys()
            .all(|key| !key.starts_with("reda-chosen"))
    );
    assert_eq!(chain(&resolved, REGULAR)[0], "reda-inter-regular");
    assert_eq!(chain(&resolved, MONO)[0], "reda-jetbrains-mono");
    lay_out(&resolved, "Still drawn 123");
}

#[test]
fn a_broken_medium_or_semibold_face_leaves_the_regular_one() {
    let source = Fake::default().with(
        "Half",
        [
            (face("/fonts/Half-Regular.ttf", 400), INTER_REGULAR.to_vec()),
            (
                face("/fonts/Half-SemiBold.ttf", 600),
                b"damaged on disk".to_vec(),
            ),
        ],
    );
    let resolved = resolve(&ui("Half"), &source);
    assert_eq!(resolved.report.ui, Outcome::Installed);
    assert_eq!(resolved.report.weights, [400, 400, 400]);
    assert!(resolved.definitions.font_data["reda-chosen-ui-600"].bytes() == INTER_REGULAR);
    lay_out(&resolved, "Title");
}

#[test]
fn damaged_fonts_never_panic_the_window() {
    // Cut a real font short at many lengths, and stamp garbage over parts of it. Each
    // either fails validation (and is skipped) or passes and must then lay out.
    let whole = JETBRAINS_MONO;
    let mut damaged: Vec<Vec<u8>> = (1..40)
        .map(|i| whole[..whole.len() * i / 40].to_vec())
        .collect();
    for at in (0..whole.len()).step_by(whole.len() / 40) {
        let mut bytes = whole.to_vec();
        let end = (at + 2048).min(bytes.len());
        bytes[at..end].fill(0xff);
        damaged.push(bytes);
    }
    let (mut rejected, mut accepted) = (0, 0);
    for bytes in damaged {
        let source = Fake::default().with("Damaged", [(face("/fonts/Damaged.ttf", 400), bytes)]);
        let resolved = resolve(&ui("Damaged"), &source);
        match resolved.report.ui {
            Outcome::Rejected(_) => rejected += 1,
            Outcome::Installed => accepted += 1,
            other => panic!("{other:?}"),
        }
        lay_out(&resolved, "Aag0189 The quick brown fox, 0123456789 {}[]");
    }
    assert!(
        rejected > 0 && accepted > 0,
        "{rejected} rejected, {accepted} accepted"
    );
}

// ---- On a context ----

#[test]
fn fonts_are_installed_once_per_choice_and_report_what_happened() {
    let ctx = egui::Context::default();
    assert_eq!(report(&ctx), None);
    set_source(&ctx, Arc::new(installed()));
    let first = apply(&ctx, &ui("Fancy"));
    assert_eq!(first.ui, Outcome::Installed);
    assert_eq!(report(&ctx), Some(first));
    // The same choice again changes nothing; another one does.
    assert_eq!(apply(&ctx, &ui("Fancy")), first);
    assert_eq!(apply(&ctx, &ui("Missing")).ui, Outcome::NotInstalled);
    // A new source resolves the standing choice again.
    set_source(
        &ctx,
        Arc::new(Fake::default().with(
            "Missing",
            [(face("/fonts/Found.ttf", 400), INTER_REGULAR.to_vec())],
        )),
    );
    assert_eq!(report(&ctx).unwrap().ui, Outcome::Installed);
}

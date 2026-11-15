//! The Appearance page of the Preferences window, driven through AccessKit with
//! `egui_kittest` (docs/spec/v0.1/app.md#preferences): its groups and controls, each
//! setting switching live, the text size's range, the font lists, and a chosen font that
//! is not installed.

use std::path::Path;
use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::{FontFace, UserDirs};
use reda_ui::engine::preferences;
use reda_ui::preferences_dialog::PageId;
use reda_ui::shell::{InstalledFonts, Shell};
use reda_ui::theme::fonts::{self, Face, Family, Outcome};
use reda_ui::theme::{Density, TokensExt as _};
use serde_json::json;

/// A machine with four families: REDA's own font files under other names. "Fixed" is the
/// only fixed-pitch one, and "Plain Medium" has no regular face.
struct Machine;

fn family(name: &str) -> Option<Family> {
    let face = |file: &str, weight, fixed_pitch| Face {
        file: Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fonts")
            .join(file),
        index: 0,
        weight,
        weight_axis: None,
        width: 5,
        italic: false,
        fixed_pitch,
        latin: true,
    };
    let (name, faces) = match name.to_lowercase().as_str() {
        "broken" => ("Broken", face("inter/OFL.txt", 400, false)),
        "fixed" => (
            "Fixed",
            face("jetbrains-mono/JetBrainsMonoNL-Regular.ttf", 400, true),
        ),
        "plain" => ("Plain", face("inter/Inter-Regular.ttf", 400, false)),
        "plain medium" => ("Plain Medium", face("inter/Inter-Medium.ttf", 500, false)),
        _ => return None,
    };
    Some(Family {
        name: name.to_owned(),
        faces: vec![faces],
    })
}

impl fonts::Source for Machine {
    fn family(&self, name: &str) -> Option<Family> {
        family(name)
    }

    fn families(&self) -> Vec<Family> {
        ["Broken", "Fixed", "Plain", "Plain Medium"]
            .into_iter()
            .filter_map(family)
            .collect()
    }

    fn read(&self, face: &Face) -> Option<Arc<[u8]>> {
        std::fs::read(&face.file).ok().map(Arc::from)
    }
}

impl InstalledFonts for Machine {
    fn load(&self, _family: &str) -> Option<FontFace> {
        None
    }
}

/// The window with the Preferences open on Appearance.
fn appearance(home: &Path) -> Harness<'static, App> {
    appearance_on(home, true)
}

/// The same, on an OS whose reduced-motion setting is `!os_allows_motion`.
fn appearance_on(home: &Path, os_allows_motion: bool) -> Harness<'static, App> {
    let engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    let shell = Shell::new(engine).with_fonts(Arc::new(Machine));
    let mut h = Harness::builder()
        .with_size([1400.0, 900.0])
        // A dropdown's list arrives in 90 ms: six steps of a 60 Hz frame.
        .with_max_steps(8)
        .build_ui_state(
            |ui, app: &mut App| app.ui(ui),
            App::new(shell).with_os_motion(os_allows_motion),
        );
    h.run();
    h.state_mut().open_preferences(Some(PageId::APPEARANCE));
    h.run();
    h
}

fn stored(home: &Path, key: &str, value: serde_json::Value) {
    preferences::set(&UserDirs::under(home), key, value).unwrap();
}

fn general(h: &Harness<'static, App>) -> preferences::General {
    h.state().shell().preferences().general.clone()
}

/// The top of the text `label`.
fn top(h: &Harness<'static, App>, label: &str) -> f32 {
    h.get_by_role_and_label(Role::Label, label).rect().top()
}

fn segment(h: &mut Harness<'static, App>, label: &str) {
    h.get_by_role_and_label(Role::RadioButton, label).click();
    h.run();
}

/// Opens a font's list. The page is a little longer than the window, so it is scrolled
/// to the font first.
fn open_list(h: &mut Harness<'static, App>, font: &str) {
    h.get_by_role_and_label(Role::ComboBox, font).scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::ComboBox, font).click();
    h.run();
}

const DENSITY_CAPTION: &str =
    "How much room controls and rows get. Text and icons stay the same size.";
const TEXT_SIZE_CAPTION: &str = "The size of text only. Icons stay the same size.";
const UI_SCALE_CAPTION: &str = "Magnifies everything: text, icons and spacing.";

#[test]
fn appearance_follows_general_and_takes_theme_and_ui_scale_from_it() {
    let titles: Vec<String> = PageId::tree().iter().map(|p| p.title()).collect();
    assert_eq!(&titles[..2], ["General", "Appearance"]);
    let on = |page: PageId| -> Vec<String> { page.settings().into_iter().map(|s| s.key).collect() };
    assert_eq!(
        on(PageId::APPEARANCE),
        [
            "general.theme",
            "general.density",
            "general.text_size",
            "general.ui_scale",
            "general.ui_font",
            "general.mono_font",
            "general.animate",
            "general.system_title_bar",
        ]
    );
    let general = on(PageId::GENERAL);
    assert!(
        general
            .iter()
            .all(|k| k != "general.theme" && k != "general.ui_scale")
    );
    assert!(general.iter().any(|k| k == "general.reopen_last_project"));
}

#[test]
fn the_page_has_its_groups_in_order_with_each_setting_under_its_own() {
    let home = TempDir::new("appearance-groups");
    let h = appearance(home.path());
    // Top to bottom: each heading, then its settings. "Theme" is a heading and a label.
    let themes: Vec<f32> = h
        .get_all_by_label("Theme")
        .filter(|n| n.accesskit_node().role() == Role::Label)
        .map(|n| n.rect().top())
        .collect();
    assert_eq!(themes.len(), 2, "the group and its one setting");
    let order = [
        themes[0].min(themes[1]),
        themes[0].max(themes[1]),
        top(&h, "Size"),
        top(&h, "Density"),
        top(&h, "Text size"),
        top(&h, "UI scale"),
        top(&h, "Fonts"),
        top(&h, "UI font"),
        top(&h, "Monospace font"),
        top(&h, "Window"),
        top(&h, "Animate"),
        top(&h, SYSTEM_TITLE_BAR),
    ];
    assert!(order.is_sorted_by(|a, b| a < b), "{order:?}");
}

fn animate_switch(h: &Harness<'static, App>) -> bool {
    h.get_by_role_and_label(Role::Switch, "Animate")
        .accesskit_node()
        .toggled()
        .map(|t| t == egui::accesskit::Toggled::True)
        .expect("a switch says whether it is on")
}

#[test]
fn animate_shows_the_os_setting_while_unset_and_a_flip_stores_it() {
    let home = TempDir::new("appearance-animate");
    // The OS asks for reduced motion: unset, the switch is off and nothing animates.
    let mut h = appearance_on(home.path(), false);
    assert_eq!(general(&h).animate, None);
    assert!(!animate_switch(&h));
    assert!(!h.ctx.tokens().motion.animate);
    h.get_by_role_and_label(Role::Switch, "Animate")
        .scroll_to_me();
    h.run();
    h.get_by_role_and_label(Role::Switch, "Animate").click();
    h.run();
    assert_eq!(general(&h).animate, Some(true));
    assert!(animate_switch(&h));
    assert!(h.ctx.tokens().motion.animate, "it applies live");
    h.get_by_role_and_label(Role::Switch, "Animate").click();
    h.run();
    assert_eq!(general(&h).animate, Some(false));
    assert!(!h.ctx.tokens().motion.animate);
}

#[test]
fn animate_unset_on_an_os_that_allows_motion_is_on() {
    let home = TempDir::new("appearance-animate-os");
    let h = appearance_on(home.path(), true);
    assert!(animate_switch(&h));
    assert!(h.ctx.tokens().motion.animate);
}

#[test]
fn each_of_the_three_sizes_has_its_caption_under_it() {
    let home = TempDir::new("appearance-captions");
    let h = appearance(home.path());
    let order = [
        top(&h, "Density"),
        top(&h, DENSITY_CAPTION),
        top(&h, "Text size"),
        top(&h, TEXT_SIZE_CAPTION),
        top(&h, "UI scale"),
        top(&h, UI_SCALE_CAPTION),
        top(&h, "Monospace font"),
        top(&h, "Used for the Journal, file names and command text."),
    ];
    assert!(order.is_sorted_by(|a, b| a < b), "{order:?}");
    // A caption starts where its setting's label does.
    let left = |label: &str| h.get_by_label(label).rect().left();
    assert_eq!(left(DENSITY_CAPTION), left("Density"));
}

#[test]
fn the_theme_is_a_segmented_control_that_switches_live() {
    let home = TempDir::new("appearance-theme");
    let mut h = appearance(home.path());
    // System · Light · Dark, left to right, System chosen.
    let x = |label: &str| {
        h.get_by_role_and_label(Role::RadioButton, label)
            .rect()
            .left()
    };
    assert!(x("System") < x("Light") && x("Light") < x("Dark"));
    segment(&mut h, "Dark");
    assert_eq!(general(&h).theme, preferences::Theme::Dark);
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::Dark
    );
    segment(&mut h, "Light");
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::Light
    );
    segment(&mut h, "System");
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::System
    );
}

#[test]
fn density_is_a_segmented_control_that_switches_live() {
    let home = TempDir::new("appearance-density");
    let mut h = appearance(home.path());
    assert_eq!(h.ctx.tokens().density, Density::Comfortable);
    let roomy = h.ctx.tokens().control_height();
    let text = h.ctx.tokens().ty.body.size;
    segment(&mut h, "Compact");
    assert_eq!(general(&h).density, preferences::Density::Compact);
    // The frame after the click is drawn with the new tokens, in both themes.
    let t = h.ctx.tokens();
    assert_eq!(t.density, Density::Compact);
    assert!(t.control_height() < roomy);
    assert_eq!(t.ty.body.size, text, "text keeps its size");
    assert_eq!(
        h.ctx.global_style().spacing.interact_size.y,
        t.control_height()
    );
    segment(&mut h, "Comfortable");
    assert_eq!(h.ctx.tokens().density, Density::Comfortable);
}

#[test]
fn the_text_size_steps_and_applies_when_entered_not_while_typing() {
    let home = TempDir::new("appearance-text-size");
    let mut h = appearance(home.path());
    assert_eq!(h.ctx.tokens().ty.body.size, 13.0);
    let field = || Role::TextInput;
    // Up steps a pixel and applies at once.
    h.get_by_role_and_label(field(), "Text size").click();
    h.run();
    h.key_press(egui::Key::ArrowUp);
    h.run();
    assert_eq!(general(&h).text_size, 14);
    h.run();
    assert_eq!(h.ctx.tokens().ty.body.size, 14.0);
    let small = h.ctx.tokens().ty.small.size;

    // Typing changes nothing until Enter.
    h.key_combination_modifiers(egui::Modifiers::COMMAND, &[egui::Key::A]);
    h.get_by_role_and_label(field(), "Text size")
        .type_text("16");
    h.run();
    assert_eq!(general(&h).text_size, 14);
    assert_eq!(h.ctx.tokens().ty.body.size, 14.0);
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert_eq!(general(&h).text_size, 16);
    assert_eq!(h.ctx.tokens().ty.body.size, 16.0);
    assert_eq!(
        h.ctx.tokens().ty.small.size,
        small + 2.0,
        "the scale moves with it"
    );
}

#[test]
fn the_text_size_field_stays_within_11_and_16() {
    let home = TempDir::new("appearance-text-range");
    let mut h = appearance(home.path());
    for (typed, size) in [("99", 16), ("3", 11)] {
        h.get_by_role_and_label(Role::TextInput, "Text size")
            .click();
        h.run();
        h.key_combination_modifiers(egui::Modifiers::COMMAND, &[egui::Key::A]);
        h.get_by_role_and_label(Role::TextInput, "Text size")
            .type_text(typed);
        h.run();
        h.key_press(egui::Key::Enter);
        h.run();
        h.run();
        assert_eq!(general(&h).text_size, size, "typed {typed}");
        assert_eq!(h.ctx.tokens().ty.body.size, f32::from(size));
    }
    // At the bottom of the range Down goes no further.
    h.key_press(egui::Key::ArrowDown);
    h.run();
    assert_eq!(general(&h).text_size, 11);
}

#[test]
fn the_ui_scale_keeps_its_dropdown_and_switches_live() {
    let home = TempDir::new("appearance-scale");
    let mut h = appearance(home.path());
    h.get_by_role_and_label(Role::ComboBox, "UI scale").click();
    h.run();
    h.get_by_label("125 %").click();
    h.run();
    h.run();
    assert_eq!(general(&h).ui_scale, 1.25);
    assert_eq!(h.ctx.zoom_factor(), 1.25);
}

#[test]
fn the_font_lists_start_with_the_default_and_the_monospace_one_is_fixed_pitch_only() {
    let home = TempDir::new("appearance-font-lists");
    let mut h = appearance(home.path());
    open_list(&mut h, "UI font");
    // Every family with a regular face; "Plain Medium" has none.
    for family in ["Default (Inter)", "Broken", "Fixed", "Plain"] {
        assert!(
            h.query_by_role_and_label(Role::Button, family).is_some(),
            "{family}"
        );
    }
    assert!(h.query_by_label("Plain Medium").is_none());
    let y = |label: &str| h.get_by_role_and_label(Role::Button, label).rect().top();
    assert!(
        y("Default (Inter)") < y("Broken") && y("Broken") < y("Fixed") && y("Fixed") < y("Plain")
    );
    h.key_press(egui::Key::Escape);
    h.run();

    open_list(&mut h, "Monospace font");
    assert!(
        h.query_by_role_and_label(Role::Button, "Default (JetBrains Mono)")
            .is_some()
    );
    assert!(h.query_by_role_and_label(Role::Button, "Fixed").is_some());
    for family in ["Broken", "Plain"] {
        assert!(
            h.query_by_role_and_label(Role::Button, family).is_none(),
            "{family}"
        );
    }
}

#[test]
fn a_font_applies_when_chosen_not_while_moving_through_the_list() {
    let home = TempDir::new("appearance-font-live");
    let mut h = appearance(home.path());
    let body_width = |h: &Harness<'static, App>| {
        let t = h.ctx.tokens();
        h.ctx.fonts_mut(|f| {
            f.layout_no_wrap(
                "Minimum clearance".to_owned(),
                t.ty.body.font_id(),
                egui::Color32::WHITE,
            )
            .size()
            .x
        })
    };
    let inter = body_width(&h);
    open_list(&mut h, "UI font");
    // Down to "Fixed": nothing is set or drawn differently on the way.
    for _ in 0..2 {
        h.key_press(egui::Key::ArrowDown);
        h.run();
    }
    assert_eq!(general(&h).ui_font, None);
    assert_eq!(fonts::report(&h.ctx).unwrap().ui, Outcome::Bundled);
    h.key_press(egui::Key::Enter);
    h.run();
    h.run();
    assert_eq!(general(&h).ui_font.as_deref(), Some("Fixed"));
    assert_eq!(fonts::report(&h.ctx).unwrap().ui, Outcome::Installed);
    assert_ne!(
        body_width(&h),
        inter,
        "the chrome's text is in the chosen face"
    );
    // The field shows the family.
    assert_eq!(
        h.get_by_role_and_label(Role::ComboBox, "UI font")
            .accesskit_node()
            .value()
            .as_deref(),
        Some("Fixed")
    );

    // The monospace font, by a click; then back to the default.
    open_list(&mut h, "Monospace font");
    h.get_by_role_and_label(Role::Button, "Fixed").click();
    h.run();
    h.run();
    assert_eq!(general(&h).mono_font.as_deref(), Some("Fixed"));
    assert_eq!(fonts::report(&h.ctx).unwrap().mono, Outcome::Installed);
    open_list(&mut h, "UI font");
    h.get_by_role_and_label(Role::Button, "Default (Inter)")
        .click();
    h.run();
    h.run();
    assert_eq!(general(&h).ui_font, None);
    assert_eq!(fonts::report(&h.ctx).unwrap().ui, Outcome::Bundled);
    assert_eq!(body_width(&h), inter);
}

#[test]
fn a_chosen_font_that_is_not_installed_falls_back_and_the_list_says_so() {
    let home = TempDir::new("appearance-font-missing");
    stored(home.path(), "general.ui_font", json!("Nowhere Sans"));
    // Installed, but its file is not a font.
    stored(home.path(), "general.mono_font", json!("Broken"));
    let mut h = appearance(home.path());
    let report = fonts::report(&h.ctx).unwrap();
    assert_eq!(report.ui, Outcome::NotInstalled);
    assert!(matches!(report.mono, Outcome::Rejected(_)));
    // The stored names are kept and shown, each with the warning under it.
    assert_eq!(general(&h).ui_font.as_deref(), Some("Nowhere Sans"));
    h.get_by_label("“Nowhere Sans” is not installed. REDA is using the default.");
    h.get_by_label("“Broken” is not installed. REDA is using the default.");
    // Only the fonts are warned about.
    assert_eq!(h.query_all_by_label_contains("is not installed").count(), 2);
    assert_eq!(
        h.get_by_role_and_label(Role::ComboBox, "UI font")
            .accesskit_node()
            .value()
            .as_deref(),
        Some("Nowhere Sans")
    );

    open_list(&mut h, "UI font");
    let row = h.get_by_role_and_label(Role::Button, "Nowhere Sans");
    assert_eq!(
        row.accesskit_node().description().as_deref(),
        Some("not installed"),
        "the row is marked"
    );
    let plain = h.get_by_role_and_label(Role::Button, "Plain");
    assert_eq!(plain.accesskit_node().description(), None);
    // Picking an installed family clears the warning.
    h.get_by_role_and_label(Role::Button, "Plain").click();
    h.run();
    h.run();
    assert_eq!(fonts::report(&h.ctx).unwrap().ui, Outcome::Installed);
    assert!(h.query_by_label_contains("“Nowhere Sans”").is_none());
    open_list(&mut h, "UI font");
    assert!(
        h.query_by_label("Nowhere Sans").is_none(),
        "it is offered no longer"
    );
}

#[test]
fn the_stored_appearance_is_in_force_from_the_first_frame() {
    let home = TempDir::new("appearance-stored");
    stored(home.path(), "general.density", json!("compact"));
    stored(home.path(), "general.text_size", json!(15));
    stored(home.path(), "general.ui_font", json!("Plain"));
    let h = appearance(home.path());
    let t = h.ctx.tokens();
    assert_eq!(t.density, Density::Compact);
    assert_eq!(t.ty.body.size, 15.0);
    assert_eq!(fonts::report(&h.ctx).unwrap().ui, Outcome::Installed);
}

#[test]
fn another_instance_changing_the_appearance_applies_here() {
    let home = TempDir::new("appearance-watch");
    let mut h = appearance(home.path());
    stored(home.path(), "general.density", json!("compact"));
    stored(home.path(), "general.text_size", json!(12));
    h.run();
    h.run();
    let t = h.ctx.tokens();
    assert_eq!((t.density, t.ty.body.size), (Density::Compact, 12.0));
}

#[test]
fn resetting_the_page_puts_every_setting_back_live() {
    let home = TempDir::new("appearance-reset");
    stored(home.path(), "general.density", json!("compact"));
    stored(home.path(), "general.text_size", json!(16));
    stored(home.path(), "general.theme", json!("dark"));
    let mut h = appearance(home.path());
    h.get_by_role_and_label(Role::Button, "Reset page").click();
    h.run();
    h.run();
    let t = h.ctx.tokens();
    assert_eq!((t.density, t.ty.body.size), (Density::Comfortable, 13.0));
    assert_eq!(
        h.ctx.options(|o| o.theme_preference),
        egui::ThemePreference::System
    );
}

#[test]
fn the_search_shows_an_appearance_setting_with_its_own_control() {
    let home = TempDir::new("appearance-search");
    let mut h = appearance(home.path());
    h.get_by_role_and_label(Role::SearchInput, "Search settings")
        .type_text("density");
    h.run();
    segment(&mut h, "Compact");
    assert_eq!(h.ctx.tokens().density, Density::Compact);
    h.get_by_label(DENSITY_CAPTION);
}

const SYSTEM_TITLE_BAR: &str = "Use the system title bar";
const RESTART_NOTE: &str = "Applies the next time REDA starts.";

/// Scrolls the system title bar's check box into view.
fn title_bar_box(h: &mut Harness<'static, App>) {
    h.get_by_role_and_label(Role::CheckBox, SYSTEM_TITLE_BAR)
        .scroll_to_me();
    h.run();
}

#[test]
fn the_system_title_bar_is_a_check_box_that_is_stored_and_notes_the_restart() {
    let home = TempDir::new("appearance-title-bar");
    let mut h = appearance(home.path());
    title_bar_box(&mut h);
    assert!(!general(&h).system_title_bar, "off by default");
    // Nothing changed yet: no note.
    assert!(h.query_by_label(RESTART_NOTE).is_none());
    h.get_by_role_and_label(Role::CheckBox, SYSTEM_TITLE_BAR)
        .click();
    h.run();
    assert!(general(&h).system_title_bar);
    let stored = preferences::load(&UserDirs::under(home.path())).values;
    assert!(stored.general.system_title_bar);
    // It applies on restart, so the row says so, under the check box.
    title_bar_box(&mut h);
    assert!(top(&h, SYSTEM_TITLE_BAR) < top(&h, RESTART_NOTE));
    // Back to what REDA started with: nothing is waiting for a restart.
    h.get_by_role_and_label(Role::CheckBox, SYSTEM_TITLE_BAR)
        .click();
    h.run();
    assert!(!general(&h).system_title_bar);
    assert!(h.query_by_label(RESTART_NOTE).is_none());
}

#[test]
fn a_title_bar_stored_before_the_start_is_in_force_without_a_note() {
    let home = TempDir::new("appearance-title-bar-stored");
    stored(home.path(), "general.system_title_bar", json!(true));
    let mut h = appearance(home.path());
    title_bar_box(&mut h);
    let node = h.get_by_role_and_label(Role::CheckBox, SYSTEM_TITLE_BAR);
    assert_eq!(
        node.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    assert!(h.query_by_label(RESTART_NOTE).is_none());
    assert!(h.state().title_bar_startup().system_title_bar());
}

#[test]
fn a_forced_system_title_bar_shows_ticked_and_disabled_with_its_reason() {
    let home = TempDir::new("appearance-title-bar-forced");
    let mut h = appearance(home.path());
    // What the Wayland slice detects before the window is created (sway, Hyprland).
    reda_ui::title_bar::Startup {
        preference: false,
        forced: true,
    }
    .remember(&h.ctx);
    h.run();
    title_bar_box(&mut h);
    let node = h.get_by_role_and_label(Role::CheckBox, SYSTEM_TITLE_BAR);
    assert_eq!(
        node.accesskit_node().toggled(),
        Some(egui::accesskit::Toggled::True)
    );
    assert!(node.accesskit_node().is_disabled());
    h.get_by_label("Your desktop draws the title bar for every window.");
    // The stored value is left alone.
    assert!(!general(&h).system_title_bar);
}

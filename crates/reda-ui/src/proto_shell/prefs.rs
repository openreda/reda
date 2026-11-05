//! PROTOTYPE. The Preferences tree, with the settings `app.md` lists placed on it. Values
//! live in memory, except the theme, which shows what "live" means.

use std::collections::{BTreeMap, BTreeSet};

use egui::{Button, Color32, RichText, Ui};
use serde_json::json;

use crate::shell::Shell;

use super::with;

#[derive(Clone, Copy)]
enum K {
    On,
    Off,
    /// Choices, the first being the default.
    Pick(&'static [&'static str]),
    Num(&'static str),
    Note,
}

struct Page {
    /// Top-level category.
    group: &'static str,
    /// Child page; empty for the category's own page.
    page: &'static str,
    rows: &'static [(&'static str, K)],
}

const YES_NO_FOLLOW: &[&str] = &["Follow OS", "Dark", "Light"];

#[rustfmt::skip]
const TREE: &[Page] = &[
    Page { group: "General", page: "", rows: &[
        ("Theme", K::Pick(YES_NO_FOLLOW)), ("UI scale", K::Pick(&["100 %", "75 %", "85 %", "115 %", "125 %", "150 %", "200 %"])),
        ("Reopen the last Project on start", K::On), ("Undo depth", K::Pick(&["Unlimited", "Cap"])),
        ("Author of Waivers", K::Num("OS user name")),
    ]},
    Page { group: "Window & Layouts", page: "", rows: &[
        ("Default Saved Layout", K::Pick(&["Shipped default"])),
        ("Reset Layout, manage Saved Layouts", K::Note),
    ]},
    Page { group: "Editing", page: "", rows: &[
        ("Default units", K::Pick(&["mm", "mil"])), ("Display precision, mm", K::Num("3")),
        ("Display precision, mil", K::Num("1")), ("Repeat placement", K::On),
        ("Click Clears Selection", K::On), ("Directional Rectangle", K::On),
        ("Rotation Step", K::Num("90 deg")), ("Zoom, flip and Layer-switch animation", K::On),
    ]},
    Page { group: "Editing", page: "Snap", rows: &[
        ("Object's Electrical Hot Spot", K::On), ("Center of Object", K::Off),
        ("Default object snap, PCB", K::Pick(&["Active Layer", "All Layers", "Off"])),
        ("Default object snap, schematic", K::On), ("Snap Distance", K::Num("8 mil")),
        ("Hotspot checklist, per object kind", K::Note),
    ]},
    Page { group: "Editing", page: "Grid", rows: &[
        ("Visible style", K::Pick(&["Dots", "Lines"])), ("Coarse multiplier", K::Num("5")),
        ("Preset tables and default preset per Document type", K::Note),
    ]},
    Page { group: "Editing", page: "Auto-pan", rows: &[
        ("Style", K::Pick(&["Ballistic", "Fixed", "Off"])), ("Speed", K::Num("1200")),
    ]},
    Page { group: "Editing", page: "Highlight", rows: &[
        ("Select", K::On), ("Zoom", K::On), ("Zoom closeness", K::Num("0.6")),
        ("Dim", K::On), ("Dim strength", K::Num("0.6")), ("Mask strength", K::Num("0.75")),
    ]},
    Page { group: "Editing", page: "HUD", rows: &[
        ("Show the HUD", K::On), ("Follow cursor", K::On), ("Item checklist", K::Note),
    ]},
    Page { group: "Schematic", page: "", rows: &[
        ("In-Place Editing", K::On), ("Clipboard Reference", K::Off),
        ("Auto-increment, primary", K::Num("1")), ("Auto-increment, secondary", K::Num("1")),
        ("Display When Dragging", K::On),
    ]},
    Page { group: "Schematic", page: "Wires", rows: &[
        ("Optimize Wires", K::On), ("Components Cut Wires", K::On),
        ("Break Wires At Autojunctions", K::On), ("Convert Cross-Junctions", K::On),
        ("Display Cross-Overs", K::Off), ("Always Drag", K::On), ("Drag Orthogonal", K::On),
        ("Drag Step", K::Num("Grid")), ("Cutting Length", K::Num("Grid")),
        ("Show Cutter Box", K::On), ("Show Extremity Markers", K::On),
    ]},
    Page { group: "Schematic", page: "Names & Units", rows: &[
        ("Unit Suffix", K::Pick(&["Alpha", "Numeric"])), ("Single '\\' Negation", K::Off),
        ("Port Cross References, Sheet Style", K::Pick(&["Name", "Number"])),
        ("Port Cross References, Location Style", K::Pick(&["Zone", "Location"])),
    ]},
    Page { group: "PCB", page: "", rows: &[
        ("Smart Track Ends", K::Off), ("Protect Locked Objects", K::On),
        ("Protect Locked Primitives In Component", K::On),
        ("Live hover", K::Pick(&["Shift only", "On", "Off"])),
        ("Net Info delta-T", K::Num("10  degC")),
    ]},
    Page { group: "PCB", page: "Interactive Routing", rows: &[
        ("Obstacle mode", K::Pick(&["Stop", "Ignore"])),
        ("Clearance display", K::Pick(&["Near cursor", "All", "Off"])),
        ("Pick up width from the Track", K::On),
        ("Width mode", K::Pick(&["Net Class", "Minimum", "Favourite"])),
        ("Favourite widths", K::Num("0.15, 0.25, 0.5 mm")), ("Loop removal", K::On),
        ("Remove antennas", K::On), ("Remove loops through Vias", K::Off),
        ("Keep angle when dragging", K::On),
    ]},
    Page { group: "PCB", page: "Placement", rows: &[
        ("Obstacle mode", K::Pick(&["Ignore", "Stop", "Avoid"])), ("Alignment guides", K::On),
        ("Ratsnest in a move", K::Pick(&["All", "Selected", "Off"])),
    ]},
    Page { group: "PCB", page: "Zones", rows: &[
        ("Auto-refill", K::Pick(&["After edit", "On save", "Off"])),
        ("Zone Tool remembers last-used values", K::On),
    ]},
    Page { group: "PCB", page: "View", rows: &[
        ("Net colour pattern", K::Pick(&["Solid", "Stripes"])),
        ("Net colours when zoomed out", K::Pick(&["Keep", "Fade"])),
        ("User Layer sets", K::Note),
    ]},
    Page { group: "Cross-select", page: "", rows: &[
        ("Dim", K::On), ("Zoom", K::Off), ("Focus", K::On), ("Reposition", K::Off),
    ]},
    Page { group: "Checks", page: "", rows: &[
        ("Overlay style", K::Pick(&["Cross pattern", "Warning pattern", "Solid", "Layer colour"])),
        ("Overlay when zoomed out", K::Pick(&["Override colour dominates", "Layer colour dominates", "Pattern scales"])),
        ("Most markers drawn", K::Num("500")), ("Per-check Details and Overlay grid", K::Note),
    ]},
    Page { group: "Display", page: "", rows: &[
        ("Layer colour overrides per theme, by Layer kind", K::Note),
        ("Layer control", K::Pick(&["Swatch, top right of the canvas", "Strip along the canvas bottom"])),
        ("Group Tools on the Active Bar", K::Off),
        ("Cursor", K::Pick(&["Small crosshair", "Full-screen crosshair"])),
        ("Default Inactive-Layer mode", K::Pick(&["Normal", "Dimmed", "Hidden"])),
    ]},
    Page { group: "Libraries", page: "", rows: &[
        ("User Library paths, Active checkboxes", K::Note), ("Library check severities", K::Note),
    ]},
    Page { group: "Fab Presets", page: "", rows: &[("The user's presets", K::Note)]},
    Page { group: "Agents", page: "", rows: &[("Bridge", K::On), ("Allowed agents", K::Note)]},
    Page { group: "Keyboard Shortcuts", page: "", rows: &[("The built page goes here, unchanged", K::Note)]},
];

/// The UI scale steps, smallest first.
pub const SCALES: [(f64, &str); 7] = [
    (0.75, "75 %"),
    (0.85, "85 %"),
    (1.0, "100 %"),
    (1.15, "115 %"),
    (1.25, "125 %"),
    (1.5, "150 %"),
    (2.0, "200 %"),
];

const ALL_SETTINGS: usize = usize::MAX;

#[derive(Default)]
pub struct PrefsState {
    page: usize,
    query: String,
    modified_only: bool,
    collapsed: BTreeSet<&'static str>,
    /// Values that differ from the default, by slug.
    values: BTreeMap<String, Value>,
}

#[derive(Clone, PartialEq)]
enum Value {
    Bool(bool),
    Text(String),
}

fn slug(page: &Page, label: &str) -> String {
    let path = if page.page.is_empty() {
        page.group.to_owned()
    } else {
        format!("{}.{}", page.group, page.page)
    };
    format!("{path}.{label}")
        .to_lowercase()
        .replace(" & ", "_")
        .replace([' ', ',', '\''], "_")
        .replace('-', "_")
}

fn default_of(kind: K) -> Option<Value> {
    match kind {
        K::On => Some(Value::Bool(true)),
        K::Off => Some(Value::Bool(false)),
        K::Pick(choices) => Some(Value::Text(choices[0].to_owned())),
        K::Num(text) => Some(Value::Text(text.to_owned())),
        K::Note => None,
    }
}

fn modified_on(state: &PrefsState, page: &Page) -> usize {
    page.rows
        .iter()
        .filter(|(label, _)| state.values.contains_key(&slug(page, label)))
        .count()
}

/// The whole surface, on whichever kind of window holds it.
pub fn show(ui: &mut Ui, shell: &mut Shell) {
    let mut state = with(|s| std::mem::take(&mut s.prefs));
    ui.horizontal(|ui| {
        ui.heading("Preferences");
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.add(
                egui::TextEdit::singleline(&mut state.query)
                    .hint_text("Search every setting")
                    .desired_width(240.0),
            );
            ui.checkbox(&mut state.modified_only, "Modified only");
        });
    });
    ui.separator();
    let height = ui.available_height() - 34.0;
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            ui.set_width(190.0);
            ui.set_height(height);
            egui::ScrollArea::vertical()
                .id_salt("proto-pref-tree")
                .auto_shrink(false)
                .show(ui, |ui| tree(ui, &mut state));
        });
        ui.separator();
        ui.vertical(|ui| {
            ui.set_height(height);
            egui::ScrollArea::vertical()
                .id_salt("proto-pref-page")
                .auto_shrink(false)
                .show(ui, |ui| content(ui, &mut state, shell));
        });
    });
    with(|s| s.prefs = state);
}

fn tree(ui: &mut Ui, state: &mut PrefsState) {
    ui.spacing_mut().item_spacing.y = 1.0;
    let mut i = 0;
    while i < TREE.len() {
        let group = TREE[i].group;
        let end = i + TREE[i..].iter().take_while(|p| p.group == group).count();
        let children = end - i > 1;
        let modified: usize = TREE[i..end].iter().map(|p| modified_on(state, p)).sum();
        ui.horizontal(|ui| {
            if children {
                let open = !state.collapsed.contains(group);
                if ui
                    .add(Button::new(if open { "v" } else { ">" }).frame(false))
                    .clicked()
                {
                    if open {
                        state.collapsed.insert(group);
                    } else {
                        state.collapsed.remove(group);
                    }
                }
            } else {
                ui.add_space(14.0);
            }
            row(ui, state, group, i, modified);
        });
        if children && !state.collapsed.contains(group) {
            for (j, page) in TREE.iter().enumerate().take(end).skip(i + 1) {
                let modified = modified_on(state, page);
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    row(ui, state, page.page, j, modified);
                });
            }
        }
        i = end;
    }
    ui.add_space(6.0);
    ui.separator();
    ui.horizontal(|ui| {
        ui.add_space(14.0);
        let all = state.values.len();
        row(ui, state, "All settings", ALL_SETTINGS, all);
    });
}

fn row(ui: &mut Ui, state: &mut PrefsState, title: &str, page: usize, modified: usize) {
    let mut text = RichText::new(title);
    if modified > 0 {
        text = text.strong();
    }
    let response = super::tree_row(ui, state.page == page, text);
    if modified > 0 {
        ui.painter().circle_filled(
            response.rect.right_center() - egui::vec2(8.0, 0.0),
            3.0,
            Color32::from_rgb(90, 160, 230),
        );
    }
    if response.clicked() {
        state.page = page;
        state.query.clear();
    }
}

fn content(ui: &mut Ui, state: &mut PrefsState, shell: &mut Shell) {
    let query = state.query.trim().to_lowercase();
    if !query.is_empty() || state.modified_only {
        for page in TREE {
            let rows: Vec<_> = page
                .rows
                .iter()
                .filter(|(label, _)| {
                    (query.is_empty() || label.to_lowercase().contains(&query))
                        && (!state.modified_only || state.values.contains_key(&slug(page, label)))
                })
                .collect();
            if rows.is_empty() {
                continue;
            }
            ui.add_space(4.0);
            ui.strong(heading(page));
            for (label, kind) in rows {
                setting(ui, state, page, label, *kind, shell);
            }
        }
        return;
    }
    if state.page == ALL_SETTINGS {
        all_settings(ui, state);
        return;
    }
    let page = &TREE[state.page.min(TREE.len() - 1)];
    ui.horizontal(|ui| {
        ui.strong(heading(page));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if ui.small_button("Reset all").clicked() {
                state.values.clear();
            }
            if ui.small_button("Reset branch").clicked() {
                for p in TREE.iter().filter(|p| p.group == page.group) {
                    for (label, _) in p.rows {
                        state.values.remove(&slug(p, label));
                    }
                }
            }
            if ui.small_button("Reset page").clicked() {
                for (label, _) in page.rows {
                    state.values.remove(&slug(page, label));
                }
            }
        });
    });
    ui.add_space(6.0);
    for (label, kind) in page.rows {
        setting(ui, state, page, label, *kind, shell);
    }
}

fn heading(page: &Page) -> String {
    if page.page.is_empty() {
        page.group.to_owned()
    } else {
        format!("{} > {}", page.group, page.page)
    }
}

/// A row: modified dot, label, editor, reset.
fn setting(
    ui: &mut Ui,
    state: &mut PrefsState,
    page: &Page,
    label: &str,
    kind: K,
    shell: &mut Shell,
) {
    let key = slug(page, label);
    let Some(default) = default_of(kind) else {
        ui.horizontal(|ui| {
            ui.add_space(12.0);
            ui.label(label);
            let _ = ui.small_button("Edit...");
        });
        return;
    };
    let mut value = state.values.get(&key).cloned().unwrap_or(default.clone());
    if label == "UI scale" {
        // The real value: the keys and the View menu change it too.
        value = Value::Text(format!(
            "{:.0} %",
            shell.preferences().general.ui_scale * 100.0
        ));
    }
    let mut next = value.clone();
    ui.horizontal(|ui| {
        let modified = value != default;
        let (dot, _) = ui.allocate_exact_size(egui::vec2(8.0, 16.0), egui::Sense::hover());
        if modified {
            ui.painter()
                .circle_filled(dot.center(), 3.0, Color32::from_rgb(90, 160, 230));
        }
        let (rect, _) = ui.allocate_exact_size(egui::vec2(290.0, 18.0), egui::Sense::hover());
        ui.painter().text(
            rect.left_center(),
            egui::Align2::LEFT_CENTER,
            label,
            egui::TextStyle::Body.resolve(ui.style()),
            ui.visuals().text_color(),
        );
        match (&mut next, kind) {
            (Value::Bool(on), _) => {
                ui.checkbox(on, "");
            }
            (Value::Text(text), K::Pick(choices)) => {
                egui::ComboBox::from_id_salt(&key)
                    .selected_text(text.as_str())
                    .width(190.0)
                    .show_ui(ui, |ui| {
                        for choice in choices {
                            ui.selectable_value(text, (*choice).to_owned(), *choice);
                        }
                    });
            }
            (Value::Text(text), _) => {
                ui.add(egui::TextEdit::singleline(text).desired_width(190.0));
            }
        }
        if ui
            .add_enabled(modified, Button::new("Reset").small())
            .on_hover_text("Reset to default")
            .clicked()
        {
            next = default.clone();
        }
    });
    if next != value {
        if label == "Theme"
            && let Value::Text(theme) = &next
        {
            let choice = match theme.as_str() {
                "Dark" => "dark",
                "Light" => "light",
                _ => "follow_os",
            };
            let _ = shell.run_reporting("view.theme.set", json!({ "theme": choice }));
        }
        if label == "Group Tools on the Active Bar"
            && let Value::Bool(on) = &next
        {
            let on = *on;
            with(|s| {
                s.axes.bar = if on {
                    super::ActiveBar::Grouped
                } else {
                    super::ActiveBar::Flat
                };
            });
        }
        if let Value::Text(picked) = &next {
            match label {
                "UI scale" => {
                    let scale = SCALES
                        .iter()
                        .find(|(_, name)| name == picked)
                        .map_or(1.0, |(v, _)| *v);
                    let _ = shell.run_reporting(
                        "app.preferences.set",
                        json!({"key": "general.ui_scale", "value": scale}),
                    );
                }
                "Layer control" => with(|s| {
                    s.axes.layers = if picked.starts_with("Strip") {
                        super::LayerUi::Strip
                    } else {
                        super::LayerUi::Swatch
                    };
                }),
                _ => {}
            }
        }
        if next == default {
            state.values.remove(&key);
        } else {
            state.values.insert(key, next);
        }
    }
}

/// Generated from the schema: every key in one grid.
fn all_settings(ui: &mut Ui, state: &mut PrefsState) {
    ui.strong("All settings");
    ui.weak("Generated from the schema, one row per key. Edits are the same as on the pages.");
    ui.add_space(4.0);
    egui::Grid::new("proto-all-settings")
        .striped(true)
        .num_columns(3)
        .show(ui, |ui| {
            ui.weak("Key");
            ui.weak("Value");
            ui.weak("Default");
            ui.end_row();
            for page in TREE {
                for (label, kind) in page.rows {
                    let Some(default) = default_of(*kind) else {
                        continue;
                    };
                    let key = slug(page, label);
                    let show = |v: &Value| match v {
                        Value::Bool(b) => b.to_string(),
                        Value::Text(t) => t.clone(),
                    };
                    let value = state.values.get(&key).unwrap_or(&default);
                    ui.monospace(&key);
                    if *value == default {
                        ui.label(show(value));
                    } else {
                        ui.strong(show(value));
                    }
                    ui.weak(show(&default));
                    ui.end_row();
                }
            }
        });
}

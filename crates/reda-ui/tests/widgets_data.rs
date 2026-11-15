//! The data widgets, driven through AccessKit: list and tree rows, the table, the
//! collapsible section, form-field rows, the status mark and the swatch. How they look is
//! the gallery's job (`tests/widget_gallery`).

use egui::accesskit::Role;
use egui::{Color32, Theme, Ui};
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT as _, Queryable as _};
use reda_ui::theme::{self, Density, Settings};
use reda_ui::widgets::{self, Column, RowLook, Selection, Sorted, Status};

/// What the page below shows and edits.
struct Page {
    layer: usize,
    open: bool,
    chosen: Option<&'static str>,
    net: Option<usize>,
    sorts: u32,
    ascending: bool,
    width: String,
    recolours: u32,
    libraries: u32,
}

impl Default for Page {
    fn default() -> Self {
        Page {
            layer: 0,
            open: false,
            chosen: None,
            net: None,
            sorts: 0,
            ascending: true,
            width: "0.250".to_owned(),
            recolours: 0,
            libraries: 0,
        }
    }
}

const LAYERS: [&str; 3] = ["Top Copper", "Inner 1", "Bottom Copper"];
/// A Net, its length and its Via count. The lengths have the same number of digits but
/// different digits, so only tabular digits give them one width.
const NETS: [(&str, &str, &str); 3] = [
    ("USB_D+", "41.118", "2"),
    ("VBUS", "70.070", "11"),
    ("SWDIO", "88.890", "0"),
];

fn page(ui: &mut Ui, p: &mut Page) {
    widgets::rows(ui, |ui| {
        for (i, name) in LAYERS.into_iter().enumerate() {
            let row = widgets::list_row(name)
                .selected(p.layer == i)
                .swatch(Color32::TRANSPARENT);
            if ui.add(row).clicked() {
                p.layer = i;
            }
        }
    });
    let off = ui.add_enabled(false, widgets::list_row("Libraries"));
    if off.clicked() {
        p.libraries += 1;
    }

    let root = widgets::tree_row("Schematic", 0)
        .open(Some(p.open))
        .show(ui);
    if root.toggled {
        p.open = !p.open;
    } else if root.response.clicked() {
        p.chosen = Some("Schematic");
    }
    if p.open {
        for name in ["Sheet 1", "Power"] {
            let leaf = widgets::tree_row(name, 1).selected(p.chosen == Some(name));
            if ui.add(leaf).clicked() {
                p.chosen = Some(name);
            }
        }
    }

    widgets::section("Position")
        .count("2")
        .show(ui, |ui| ui.label("Start and end"));
    widgets::section("Track").default_open(true).show(ui, |ui| {
        widgets::form("track").show(ui, |ui| {
            widgets::form_row(ui, "Net Class", |ui| ui.label("Power"));
            widgets::form_row(ui, "Width of the Track", |ui| {
                let field = ui.add(widgets::text_field(&mut p.width));
                field.widget_info(|| egui::WidgetInfo::labeled(Role::TextInput, true, "Width"));
            });
        });
    });

    widgets::table("nets")
        .columns([
            Column::auto(),
            Column::fill(),
            Column::auto().number(),
            Column::auto().number(),
        ])
        .show(ui, |table| {
            table.header(|h| {
                h.empty();
                let sorted = Sorted {
                    ascending: p.ascending,
                    rank: None,
                };
                if h.sort("Net", Some(sorted)).clicked() {
                    p.sorts += 1;
                    p.ascending = !p.ascending;
                }
                h.text("Length");
                h.text("Vias");
            });
            for (i, (net, length, vias)) in NETS.into_iter().enumerate() {
                let selection = if p.net == Some(i) {
                    Selection::Live
                } else {
                    Selection::None
                };
                let row = table.row_with(RowLook::selected(selection), |row| {
                    row.status(Status::Warning);
                    row.text(net);
                    row.text(length);
                    row.text(vias);
                });
                row.widget_info(|| {
                    egui::WidgetInfo::selected(
                        Role::Row,
                        true,
                        p.net == Some(i),
                        format!("Net {net}"),
                    )
                });
                if row.clicked() {
                    p.net = Some(i);
                }
            }
        });

    if ui
        .add(widgets::swatch_rgb(200, 52, 52).button("Colour of Top Copper"))
        .clicked()
    {
        p.recolours += 1;
    }
    ui.add(widgets::status_mark(Status::Error));
}

fn harness(density: Density) -> Harness<'static, Page> {
    let mut installed = false;
    let mut h = Harness::builder().with_size([420.0, 720.0]).build_ui_state(
        move |ui, p: &mut Page| {
            if !installed {
                theme::install(ui.ctx(), Theme::Light.into());
                theme::set(
                    ui.ctx(),
                    &Settings {
                        density,
                        ..Settings::default()
                    },
                );
                installed = true;
            }
            page(ui, p);
        },
        Page::default(),
    );
    h.run();
    h
}

fn comfortable() -> Harness<'static, Page> {
    harness(Density::Comfortable)
}

#[test]
fn rows_are_28_px_in_comfortable_and_24_in_compact_at_13_px_text() {
    for (density, height) in [(Density::Comfortable, 28.0), (Density::Compact, 24.0)] {
        let h = harness(density);
        let rows = [
            h.get_by_role_and_label(Role::ListItem, "Inner 1"),
            h.get_by_role_and_label(Role::TreeItem, "Schematic"),
            h.get_by_role_and_label(Role::DisclosureTriangle, "Position"),
            h.get_by_role_and_label(Role::Row, "Net VBUS"),
        ];
        for row in rows {
            assert_eq!(row.rect().height(), height, "{density:?}: {row:?}");
        }
    }
}

#[test]
fn rows_stack_with_no_gap_between_them() {
    let h = comfortable();
    let first = h.get_by_role_and_label(Role::ListItem, "Top Copper").rect();
    let second = h.get_by_role_and_label(Role::ListItem, "Inner 1").rect();
    assert_eq!(first.bottom(), second.top());
    let first = h.get_by_role_and_label(Role::Row, "Net USB_D+").rect();
    let second = h.get_by_role_and_label(Role::Row, "Net VBUS").rect();
    assert_eq!(first.bottom(), second.top());
}

#[test]
fn a_list_row_is_chosen_when_clicked_and_says_it_is_selected() {
    let mut h = comfortable();
    let selected = |h: &Harness<'_, Page>, name: &str| {
        h.get_by_role_and_label(Role::ListItem, name)
            .accesskit_node()
            .is_selected()
    };
    assert_eq!(selected(&h, "Top Copper"), Some(true));
    h.get_by_role_and_label(Role::ListItem, "Inner 1").click();
    h.run();
    assert_eq!(h.state().layer, 1);
    assert_eq!(selected(&h, "Inner 1"), Some(true));
    assert_eq!(selected(&h, "Top Copper"), Some(false));
}

#[test]
fn a_disabled_row_cannot_be_chosen() {
    let mut h = comfortable();
    let row = h.get_by_role_and_label(Role::ListItem, "Libraries");
    assert!(row.accesskit_node().is_disabled());
    row.click();
    h.run();
    assert_eq!(h.state().libraries, 0);
}

#[test]
fn a_tree_rows_chevron_opens_it_without_choosing_it() {
    let mut h = comfortable();
    assert!(h.query_by_label("Sheet 1").is_none());
    let root = h.get_by_role_and_label(Role::TreeItem, "Schematic");
    assert_eq!(root.accesskit_node().data().is_expanded(), Some(false));
    // The chevron sits in the first 16 px after the row's padding.
    let at = root.rect().left_center() + egui::vec2(12.0, 0.0);
    h.drag_at(at);
    h.drop_at(at);
    h.run();
    assert!(h.state().open);
    assert_eq!(h.state().chosen, None);
    let root = h.get_by_role_and_label(Role::TreeItem, "Schematic");
    assert_eq!(root.accesskit_node().data().is_expanded(), Some(true));
    assert!(h.query_by_label("Sheet 1").is_some());
}

#[test]
fn a_click_on_a_tree_rows_text_chooses_it_and_leaves_it_closed() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::TreeItem, "Schematic").click();
    h.run();
    assert_eq!(h.state().chosen, Some("Schematic"));
    assert!(!h.state().open);
}

#[test]
fn the_arrow_keys_open_and_close_a_focused_tree_row() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::TreeItem, "Schematic").focus();
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert!(h.state().open);
    h.key_press(egui::Key::ArrowRight);
    h.run();
    assert!(h.state().open, "right does not close an open row");
    h.key_press(egui::Key::ArrowLeft);
    h.run();
    assert!(!h.state().open);
}

#[test]
fn a_tree_row_says_how_deep_it_is() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::TreeItem, "Schematic").focus();
    h.run();
    h.key_press(egui::Key::ArrowRight);
    h.run();
    let level = |name: &str| {
        h.get_by_role_and_label(Role::TreeItem, name)
            .accesskit_node()
            .level()
    };
    assert_eq!(level("Schematic"), Some(1));
    assert_eq!(level("Power"), Some(2));
}

#[test]
fn a_section_header_opens_and_closes_its_body() {
    let mut h = comfortable();
    let header = h.get_by_role_and_label(Role::DisclosureTriangle, "Position");
    assert_eq!(header.accesskit_node().data().is_expanded(), Some(false));
    assert!(h.query_by_label("Start and end").is_none());
    header.click();
    h.run();
    assert!(h.query_by_label("Start and end").is_some());
    let header = h.get_by_role_and_label(Role::DisclosureTriangle, "Position");
    assert_eq!(header.accesskit_node().data().is_expanded(), Some(true));
    header.click();
    h.run();
    assert!(h.query_by_label("Start and end").is_none());
}

#[test]
fn a_section_can_start_open() {
    let h = comfortable();
    assert!(h.query_by_label("Width of the Track").is_some());
}

#[test]
fn a_focused_section_header_toggles_on_enter() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::DisclosureTriangle, "Position")
        .focus();
    h.run();
    h.key_press(egui::Key::Enter);
    h.run();
    assert!(h.query_by_label("Start and end").is_some());
}

#[test]
fn form_values_line_up_in_one_column_whatever_the_length_of_their_names() {
    let h = comfortable();
    let short = h.get_by_label("Net Class").rect();
    let long = h.get_by_label("Width of the Track").rect();
    assert_eq!(short.left(), long.left());
    let value = h.get_by_label("Power").rect();
    let field = h.get_by_role_and_label(Role::TextInput, "Width").rect();
    assert!(value.left() > long.right());
    // The field's own rectangle is the text inside its well, which is padded.
    assert!(field.left() >= value.left());
    assert!(field.left() - value.left() <= 12.0);
}

#[test]
fn a_form_field_row_takes_what_is_typed() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::TextInput, "Width").click();
    h.run();
    h.get_by_role_and_label(Role::TextInput, "Width")
        .type_text("5");
    h.run();
    assert!(h.state().width.contains('5'));
    assert_ne!(h.state().width, "0.250");
}

#[test]
fn a_table_row_is_chosen_when_clicked() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::Row, "Net VBUS").click();
    h.run();
    assert_eq!(h.state().net, Some(1));
    let row = h.get_by_role_and_label(Role::Row, "Net VBUS");
    assert_eq!(row.accesskit_node().is_selected(), Some(true));
}

#[test]
fn a_sorting_header_cell_is_a_button() {
    let mut h = comfortable();
    h.get_by_role_and_label(Role::Button, "Net").click();
    h.run();
    assert_eq!(h.state().sorts, 1);
    assert!(!h.state().ascending);
}

#[test]
fn number_columns_are_right_aligned_in_tabular_digits() {
    let h = comfortable();
    let lengths: Vec<egui::Rect> = NETS
        .iter()
        .map(|(_, length, _)| h.get_by_label(length).rect())
        .collect();
    // Tabular digits: every figure of five digits is as wide as the next.
    assert_eq!(lengths[0].width(), lengths[1].width());
    assert_eq!(lengths[1].width(), lengths[2].width());
    // Right-aligned: figures of different lengths end on one edge, under their header.
    let vias: Vec<egui::Rect> = ["2", "11", "0"]
        .iter()
        .map(|vias| h.get_by_label(vias).rect())
        .collect();
    assert_eq!(vias[0].right(), vias[1].right());
    assert_eq!(vias[1].right(), vias[2].right());
    assert!(vias[1].width() > vias[0].width());
    assert_eq!(h.get_by_label("Vias").rect().right(), vias[0].right());
    assert_eq!(h.get_by_label("Length").rect().right(), lengths[0].right());
}

#[test]
fn text_columns_start_on_one_edge_under_their_header() {
    let h = comfortable();
    let header = h.get_by_role_and_label(Role::Button, "Net").rect();
    for (net, _, _) in NETS {
        assert_eq!(h.get_by_label(net).rect().left(), header.left());
    }
}

#[test]
fn a_swatch_can_be_a_named_button() {
    let mut h = comfortable();
    let swatch = h.get_by_role_and_label(Role::Button, "Colour of Top Copper");
    assert_eq!(swatch.rect().size(), egui::Vec2::splat(14.0));
    swatch.click();
    h.run();
    assert_eq!(h.state().recolours, 1);
}

#[test]
fn a_selection_is_live_only_where_the_focus_is() {
    let ctx = egui::Context::default();
    let seen = |focused: bool| {
        let mut out = Vec::new();
        let input = egui::RawInput {
            focused,
            ..Default::default()
        };
        let mut output = ctx.run_ui(input, |ui| {
            out = vec![
                Selection::of(ui, false, true),
                Selection::of(ui, true, true),
                Selection::of(ui, true, false),
            ];
        });
        output.textures_delta.clear();
        out
    };
    assert_eq!(
        seen(true),
        [Selection::None, Selection::Live, Selection::Unfocused]
    );
    // REDA's window in the background: the live selection goes neutral too.
    assert_eq!(
        seen(false),
        [Selection::None, Selection::Unfocused, Selection::Unfocused]
    );
}

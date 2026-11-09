//! The Preferences page tree (docs/spec/v0.1/app.md#preferences): which page each
//! Preference key is on. `app.preferences.reset {page}` and the Preferences window both
//! read it, so a page means the same keys everywhere.
//!
//! A new Preference lands on its section's page; a key that belongs on a sub-page gets a
//! line in [`page_of`]. Pages that hold no settings yet are listed so later settings have
//! their place; the window hides a page with nothing to show.

/// One page of the tree.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Page {
    /// A stable id such as `editing.snap`; the window's title comes from the catalogue.
    pub id: &'static str,
    /// The page this one is indented under, if any.
    pub parent: Option<&'static str>,
}

const fn top(id: &'static str) -> Page {
    Page { id, parent: None }
}

const fn under(parent: &'static str, id: &'static str) -> Page {
    Page {
        id,
        parent: Some(parent),
    }
}

/// Every page, in tree order: each branch's child pages follow it.
pub const PAGES: &[Page] = &[
    top("general"),
    top("window"),
    top("editing"),
    under("editing", "editing.snap"),
    under("editing", "editing.grid"),
    under("editing", "editing.auto_pan"),
    under("editing", "editing.highlight"),
    under("editing", "editing.hud"),
    top("schematic"),
    under("schematic", "schematic.wires"),
    under("schematic", "schematic.names_units"),
    top("pcb"),
    under("pcb", "pcb.routing"),
    under("pcb", "pcb.placement"),
    under("pcb", "pcb.zones"),
    under("pcb", "pcb.view"),
    top("cross_select"),
    top("checks"),
    top("display"),
    top("libraries"),
    top("fab_presets"),
    top("agents"),
    top("keyboard_shortcuts"),
];

/// The page `id`, if there is one.
pub fn page(id: &str) -> Option<&'static Page> {
    PAGES.iter().find(|p| p.id == id)
}

/// The page a dotted Preference key is on.
pub fn page_of(key: &str) -> &'static str {
    let (section, name) = key.split_once('.').unwrap_or((key, ""));
    match (section, name) {
        ("editing", _) => "editing",
        ("general", "default_layout") => "window",
        ("schematic" | "pcb", "default_units" | "repeat_placement") => "editing",
        ("schematic" | "pcb", "electrical_snap" | "hotspots_off" | "snap_distance") => {
            "editing.snap"
        }
        ("schematic" | "pcb", "default_grid") => "editing.grid",
        ("schematic" | "pcb", "auto_pan") => "editing.auto_pan",
        (
            "schematic",
            "optimize_wires"
            | "components_cut_wires"
            | "break_wires_at_autojunctions"
            | "convert_cross_junctions"
            | "display_cross_overs",
        ) => "schematic.wires",
        (
            "schematic",
            "unit_suffix"
            | "single_negation"
            | "cross_reference_sheet_style"
            | "cross_reference_location_style",
        ) => "schematic.names_units",
        ("schematic", name) if name.starts_with("break_wire.") => "schematic.wires",
        ("schematic", _) => "schematic",
        ("pcb", name) if name.starts_with("zone.") || name.starts_with("keepout.") => "pcb.zones",
        ("pcb", name) if name.starts_with("route.") || name.starts_with("drag.") => "pcb.routing",
        ("pcb", name) if name.starts_with("place.") => "pcb.placement",
        ("pcb", name) if name.starts_with("view.") || name.starts_with("ratsnest.") => "pcb.view",
        ("pcb", _) => "pcb",
        ("checks", _) => "checks",
        ("display", _) => "display",
        ("libraries", _) => "libraries",
        ("agents", _) => "agents",
        ("keymap", _) => "keyboard_shortcuts",
        _ => "general",
    }
}

/// Whether `page` is `branch` or one of its descendants.
pub fn is_in(page: &str, branch: &str) -> bool {
    let mut at = self::page(page);
    while let Some(p) = at {
        if p.id == branch {
            return true;
        }
        at = p.parent.and_then(self::page);
    }
    false
}

/// The keys on `page`, or with `branch` on it and every page under it, in key order.
pub fn keys_on(page: &str, branch: bool) -> Vec<String> {
    super::keys()
        .into_iter()
        .filter(|k| {
            let on = page_of(k);
            on == page || (branch && is_in(on, page))
        })
        .collect()
}

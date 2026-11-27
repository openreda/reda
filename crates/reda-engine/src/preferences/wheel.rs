//! The wheel mapping (docs/spec/v0.3/input-and-defaults.md#wheel-mapping): each wheel
//! action bound to one modifier combination, kept as `canvas.wheel.*`. One mapping serves
//! every editor's canvas and the 3D view.
//!
//! No two actions share a combination. A table is checked whole when the file is read (a
//! clash there falls back to the REDA table with a warning), and `app.preferences.set`
//! and `app.preferences.reset` refuse a change that would make one, naming the action
//! that holds the combination. The preset (REDA, Wheel scrolls or Custom) is read off the
//! table and never stored.

use std::borrow::Cow;
use std::fmt;

use schemars::{JsonSchema, Schema, SchemaGenerator, json_schema};
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// The wheel mapping and its touchpad handling.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(default, deny_unknown_fields)]
pub struct Wheel {
    /// Zoom at the cursor.
    pub zoom: WheelBinding,
    /// Scroll sideways.
    pub scroll_x: WheelBinding,
    /// Scroll up and down.
    pub scroll_y: WheelBinding,
    /// Step Layer: the next or previous copper Layer, in the PCB editor.
    pub step_layer: WheelBinding,
    /// Zoom speed: how far one wheel notch zooms, 1 being the shipped speed.
    pub zoom_speed: f64,
    /// Reverse zoom direction: the wheel turned away zooms out instead of in.
    pub reverse_zoom: bool,
    /// Touchpad: how a touchpad's scroll is taken.
    pub touchpad: Touchpad,
}

impl Default for Wheel {
    fn default() -> Wheel {
        WheelPreset::Reda.table()
    }
}

/// The lowest and highest `zoom_speed`.
pub const ZOOM_SPEEDS: std::ops::RangeInclusive<f64> = 0.1..=10.0;

/// How a scroll from a touchpad is taken.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub enum Touchpad {
    /// Auto: a scroll the system reports as a touchpad's pans the view and a pinch zooms;
    /// a scroll from a wheel, or of unknown source, follows the table.
    #[default]
    #[serde(rename = "auto")]
    Auto,
    /// Always pan: every scroll is taken as a touchpad's.
    #[serde(rename = "pan")]
    Pan,
    /// Same as wheel: every scroll follows the table.
    #[serde(rename = "wheel")]
    Wheel,
}

/// A wheel action of the table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum WheelAction {
    Zoom,
    ScrollX,
    ScrollY,
    StepLayer,
}

impl WheelAction {
    /// Every action, in the table's order.
    pub const ALL: [WheelAction; 4] = [
        WheelAction::Zoom,
        WheelAction::ScrollX,
        WheelAction::ScrollY,
        WheelAction::StepLayer,
    ];

    /// Its Preference key, such as `canvas.wheel.zoom`.
    pub fn key(self) -> &'static str {
        match self {
            WheelAction::Zoom => "canvas.wheel.zoom",
            WheelAction::ScrollX => "canvas.wheel.scroll_x",
            WheelAction::ScrollY => "canvas.wheel.scroll_y",
            WheelAction::StepLayer => "canvas.wheel.step_layer",
        }
    }

    /// The action of a Preference key.
    pub fn of_key(key: &str) -> Option<WheelAction> {
        WheelAction::ALL.into_iter().find(|a| a.key() == key)
    }

    /// Its title, as messages name it.
    pub fn title(self) -> &'static str {
        match self {
            WheelAction::Zoom => "Zoom at the cursor",
            WheelAction::ScrollX => "Scroll sideways",
            WheelAction::ScrollY => "Scroll up and down",
            WheelAction::StepLayer => "Step Layer",
        }
    }

    /// Whether it works only in the PCB editor.
    pub fn pcb_only(self) -> bool {
        self == WheelAction::StepLayer
    }
}

/// The modifiers held with the wheel: none, or any of Ctrl, Alt and Shift together. Ctrl
/// is Ctrl or Cmd on macOS.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct WheelModifiers {
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl WheelModifiers {
    /// The plain wheel.
    pub const NONE: WheelModifiers = WheelModifiers {
        ctrl: false,
        alt: false,
        shift: false,
    };

    /// Every combination, the plain wheel first, then by how many keys it holds.
    pub fn all() -> [WheelModifiers; 8] {
        let m = |ctrl, alt, shift| WheelModifiers { ctrl, alt, shift };
        [
            m(false, false, false),
            m(false, false, true),
            m(true, false, false),
            m(false, true, false),
            m(true, false, true),
            m(false, true, true),
            m(true, true, false),
            m(true, true, true),
        ]
    }
}

/// What an action is bound to: a modifier combination, or nothing.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WheelBinding {
    Unbound,
    With(WheelModifiers),
}

impl WheelBinding {
    /// The plain wheel.
    pub const PLAIN: WheelBinding = WheelBinding::With(WheelModifiers::NONE);
    pub const SHIFT: WheelBinding = WheelBinding::With(WheelModifiers {
        ctrl: false,
        alt: false,
        shift: true,
    });
    pub const CTRL: WheelBinding = WheelBinding::With(WheelModifiers {
        ctrl: true,
        alt: false,
        shift: false,
    });

    pub fn is_unbound(self) -> bool {
        self == WheelBinding::Unbound
    }

    /// Every binding a table row offers: each combination, then unbound.
    pub fn all() -> Vec<WheelBinding> {
        WheelModifiers::all()
            .into_iter()
            .map(WheelBinding::With)
            .chain([WheelBinding::Unbound])
            .collect()
    }

    /// Reads `""`, `"shift"`, `"ctrl+alt"` (any order and case) or `"unbound"`.
    pub fn parse(text: &str) -> Result<WheelBinding, String> {
        let text = text.trim().to_ascii_lowercase();
        if text == "unbound" {
            return Ok(WheelBinding::Unbound);
        }
        let mut mods = WheelModifiers::NONE;
        if text.is_empty() {
            return Ok(WheelBinding::With(mods));
        }
        for part in text.split('+').map(str::trim) {
            let held = match part {
                "ctrl" | "control" | "cmd" | "mod" => &mut mods.ctrl,
                "alt" | "option" => &mut mods.alt,
                "shift" => &mut mods.shift,
                _ => {
                    return Err(format!(
                        "{part:?} is not a modifier; give \"\", any of shift, ctrl and alt \
                         joined by +, or \"unbound\""
                    ));
                }
            };
            if *held {
                return Err(format!("{part} is given twice"));
            }
            *held = true;
        }
        Ok(WheelBinding::With(mods))
    }

    /// The binding as people read it: `the plain wheel`, `Ctrl+Shift`, `unbound`.
    pub fn describe(self) -> String {
        match self {
            WheelBinding::Unbound => "unbound".to_owned(),
            WheelBinding::With(m) if m == WheelModifiers::NONE => "the plain wheel".to_owned(),
            WheelBinding::With(m) => [(m.ctrl, "Ctrl"), (m.alt, "Alt"), (m.shift, "Shift")]
                .into_iter()
                .filter_map(|(on, name)| on.then_some(name))
                .collect::<Vec<_>>()
                .join("+"),
        }
    }
}

/// The file's spelling: `""`, `"shift"`, `"ctrl+alt"`, `"unbound"`, modifiers in the
/// order ctrl, alt, shift.
impl fmt::Display for WheelBinding {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            WheelBinding::Unbound => f.write_str("unbound"),
            WheelBinding::With(m) => {
                let names: Vec<&str> = [(m.ctrl, "ctrl"), (m.alt, "alt"), (m.shift, "shift")]
                    .into_iter()
                    .filter_map(|(on, name)| on.then_some(name))
                    .collect();
                f.write_str(&names.join("+"))
            }
        }
    }
}

impl Serialize for WheelBinding {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for WheelBinding {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<WheelBinding, D::Error> {
        let text = String::deserialize(d)?;
        WheelBinding::parse(&text).map_err(serde::de::Error::custom)
    }
}

impl JsonSchema for WheelBinding {
    fn schema_name() -> Cow<'static, str> {
        "WheelBinding".into()
    }

    fn json_schema(_: &mut SchemaGenerator) -> Schema {
        let variants: Vec<serde_json::Value> = WheelBinding::all()
            .into_iter()
            .map(|b| {
                let mut label = b.describe();
                if let Some(first) = label.get(..1) {
                    label = first.to_uppercase() + &label[1..];
                }
                serde_json::json!({ "const": b.to_string(), "description": label })
            })
            .collect();
        json_schema!({
            "type": "string",
            "description": "A modifier combination held with the wheel: \"\" for the plain wheel, any of ctrl, alt and shift joined by + (such as \"ctrl+alt\"), or \"unbound\".",
            "oneOf": variants
        })
    }
}

/// A shipped table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WheelPreset {
    /// The plain wheel zooms, Shift scrolls sideways, Ctrl scrolls up and down.
    Reda,
    /// The plain wheel scrolls up and down, Shift sideways, Ctrl zooms.
    WheelScrolls,
}

impl WheelPreset {
    pub const ALL: [WheelPreset; 2] = [WheelPreset::Reda, WheelPreset::WheelScrolls];

    /// The preset's table, with the shipped zoom speed, direction and touchpad handling.
    pub fn table(self) -> Wheel {
        let (zoom, scroll_y) = match self {
            WheelPreset::Reda => (WheelBinding::PLAIN, WheelBinding::CTRL),
            WheelPreset::WheelScrolls => (WheelBinding::CTRL, WheelBinding::PLAIN),
        };
        Wheel {
            zoom,
            scroll_x: WheelBinding::SHIFT,
            scroll_y,
            step_layer: WheelBinding::Unbound,
            zoom_speed: 1.0,
            reverse_zoom: false,
            touchpad: Touchpad::Auto,
        }
    }
}

impl Wheel {
    /// What `action` is bound to.
    pub fn binding(&self, action: WheelAction) -> WheelBinding {
        match action {
            WheelAction::Zoom => self.zoom,
            WheelAction::ScrollX => self.scroll_x,
            WheelAction::ScrollY => self.scroll_y,
            WheelAction::StepLayer => self.step_layer,
        }
    }

    /// Binds `action` to `binding`, without checking for a clash.
    pub fn bind(&mut self, action: WheelAction, binding: WheelBinding) {
        *match action {
            WheelAction::Zoom => &mut self.zoom,
            WheelAction::ScrollX => &mut self.scroll_x,
            WheelAction::ScrollY => &mut self.scroll_y,
            WheelAction::StepLayer => &mut self.step_layer,
        } = binding;
    }

    /// Each action with its binding, in the table's order.
    pub fn bindings(&self) -> impl Iterator<Item = (WheelAction, WheelBinding)> + '_ {
        WheelAction::ALL.into_iter().map(|a| (a, self.binding(a)))
    }

    /// The action the wheel does with `held`, if one is bound to it.
    pub fn action_for(&self, held: WheelModifiers) -> Option<WheelAction> {
        self.bindings()
            .find(|(_, b)| *b == WheelBinding::With(held))
            .map(|(a, _)| a)
    }

    /// The other action that already holds `binding`, which refuses it for `action`.
    pub fn held_by(&self, action: WheelAction, binding: WheelBinding) -> Option<WheelAction> {
        if binding.is_unbound() {
            return None;
        }
        self.bindings()
            .find(|(a, b)| *a != action && *b == binding)
            .map(|(a, _)| a)
    }

    /// The first two actions sharing a combination, if any do.
    pub fn clash(&self) -> Option<(WheelAction, WheelAction)> {
        self.bindings().find_map(|(a, b)| {
            self.held_by(a, b)
                .map(|holder| (holder.min(a), holder.max(a)))
        })
    }

    /// The preset whose table this is, or `None` for Custom. Only the bindings count.
    pub fn preset(&self) -> Option<WheelPreset> {
        WheelPreset::ALL.into_iter().find(|p| {
            let table = p.table();
            self.bindings().all(|(a, b)| table.binding(a) == b)
        })
    }

    /// The bindings to set one by one, each clash-free when it is set, to reach the
    /// bindings of `target` (itself clash-free). Where two actions trade combinations,
    /// one is unbound on the way.
    pub fn steps_to(&self, target: &Wheel) -> Vec<(WheelAction, WheelBinding)> {
        let mut now = self.clone();
        let mut steps = Vec::new();
        loop {
            let pending: Vec<WheelAction> = WheelAction::ALL
                .into_iter()
                .filter(|a| now.binding(*a) != target.binding(*a))
                .collect();
            let Some(&first) = pending.first() else {
                return steps;
            };
            let free = pending
                .iter()
                .copied()
                .find(|a| now.held_by(*a, target.binding(*a)).is_none());
            let (action, binding) = match free {
                Some(a) => (a, target.binding(a)),
                // Every pending action waits on another: free one up.
                None => (first, WheelBinding::Unbound),
            };
            now.bind(action, binding);
            steps.push((action, binding));
        }
    }

    /// Range rules: the zoom speed. Returns the offending key and why.
    pub(super) fn check(&self) -> Result<(), (&'static str, String)> {
        let speed = self.zoom_speed;
        if !ZOOM_SPEEDS.contains(&speed) {
            return Err((
                "canvas.wheel.zoom_speed",
                format!(
                    "{speed} is not a zoom speed from {} to {}",
                    ZOOM_SPEEDS.start(),
                    ZOOM_SPEEDS.end()
                ),
            ));
        }
        Ok(())
    }

    /// Why setting `action` (now in this table) clashes, naming the action that holds its
    /// combination.
    pub fn refusal(&self, action: WheelAction) -> Option<String> {
        let binding = self.binding(action);
        self.held_by(action, binding).map(|holder| {
            let mut what = binding.describe();
            if let Some(first) = what.get(..1) {
                what = first.to_uppercase() + &what[1..];
            }
            format!(
                "{what} is held by {} ({}); free it first",
                holder.title(),
                holder.key()
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_combination_reads_back_from_its_spelling() {
        for binding in WheelBinding::all() {
            assert_eq!(WheelBinding::parse(&binding.to_string()), Ok(binding));
        }
        assert_eq!(
            WheelBinding::parse("Shift + CTRL").unwrap().to_string(),
            "ctrl+shift"
        );
    }

    #[test]
    fn the_held_modifiers_pick_the_action() {
        let reda = WheelPreset::Reda.table();
        assert_eq!(
            reda.action_for(WheelModifiers::NONE),
            Some(WheelAction::Zoom)
        );
        let alt = WheelModifiers {
            alt: true,
            ..WheelModifiers::NONE
        };
        assert_eq!(reda.action_for(alt), None);
    }
}

//! The `[modifiers]` table of a Keymap file (docs/spec/v0.1/app.md#keymap): which held
//! modifier each mouse gesture role uses. Mouse gestures and held keys aren't chords, so
//! they get a table of their own:
//!
//! ```toml
//! [modifiers]
//! net_highlight_board = "Mod"   # Mod+click highlights a Net on a board
//! net_highlight_sheet = "Alt"   # Alt+click highlights a Net on a sheet
//! constrain = "Alt"             # held during a drag: horizontal or vertical only
//! detach = "Mod"                # held as a drag starts: Move rather than Drag
//! snap_suspend = "Mod"          # held in flight: no object snap (with Shift, no grid)
//! ```
//!
//! A role takes `Mod` (Cmd on macOS, Ctrl elsewhere), `Ctrl` or `Alt`. Shift is not offered:
//! it toggles picks and adds Nets on every canvas. The user's `keymap.toml` overrides
//! roles one by one; where a window manager takes Alt+click, set `net_highlight_sheet`.

use std::fmt;

/// A modifier a role can be held with.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Held {
    /// Cmd on macOS, Ctrl elsewhere.
    Mod,
    /// Ctrl, which is `Mod` except on macOS.
    Ctrl,
    Alt,
}

impl Held {
    pub const ALL: [Held; 3] = [Held::Mod, Held::Ctrl, Held::Alt];

    /// Reads `Mod`, `Ctrl` or `Alt` (also `Cmd`, `Control`, `Option`), in any case.
    pub fn parse(text: &str) -> Option<Held> {
        match text.trim().to_ascii_lowercase().as_str() {
            "mod" | "cmd" | "command" => Some(Held::Mod),
            "ctrl" | "control" => Some(Held::Ctrl),
            "alt" | "option" => Some(Held::Alt),
            _ => None,
        }
    }

    /// Whether `modifiers` hold it.
    pub fn held(self, modifiers: &egui::Modifiers) -> bool {
        match self {
            Held::Mod => modifiers.command,
            Held::Ctrl => modifiers.ctrl,
            Held::Alt => modifiers.alt,
        }
    }

    /// The portable name, as the file writes it.
    pub fn name(self) -> &'static str {
        match self {
            Held::Mod => "Mod",
            Held::Ctrl => "Ctrl",
            Held::Alt => "Alt",
        }
    }

    /// Just this modifier, as egui reports it held on this platform.
    pub fn modifiers(self) -> egui::Modifiers {
        match self {
            Held::Mod => egui::Modifiers::COMMAND,
            Held::Ctrl => egui::Modifiers::CTRL,
            Held::Alt => egui::Modifiers::ALT,
        }
    }
}

impl fmt::Display for Held {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}

/// A gesture whose modifier the `[modifiers]` table sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Role {
    NetHighlightBoard,
    NetHighlightSheet,
    Constrain,
    Detach,
    SnapSuspend,
}

impl Role {
    pub const ALL: [Role; 5] = [
        Role::NetHighlightBoard,
        Role::NetHighlightSheet,
        Role::Constrain,
        Role::Detach,
        Role::SnapSuspend,
    ];

    /// The role's name in the file.
    pub fn name(self) -> &'static str {
        match self {
            Role::NetHighlightBoard => "net_highlight_board",
            Role::NetHighlightSheet => "net_highlight_sheet",
            Role::Constrain => "constrain",
            Role::Detach => "detach",
            Role::SnapSuspend => "snap_suspend",
        }
    }

    pub fn parse(name: &str) -> Option<Role> {
        Role::ALL.into_iter().find(|r| r.name() == name)
    }
}

/// The modifier of every role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModifierRoles {
    pub net_highlight_board: Held,
    pub net_highlight_sheet: Held,
    pub constrain: Held,
    pub detach: Held,
    pub snap_suspend: Held,
}

impl Default for ModifierRoles {
    /// The spec's defaults; the default Keymap file says the same.
    fn default() -> ModifierRoles {
        ModifierRoles {
            net_highlight_board: Held::Mod,
            net_highlight_sheet: Held::Alt,
            constrain: Held::Alt,
            detach: Held::Mod,
            snap_suspend: Held::Mod,
        }
    }
}

impl ModifierRoles {
    pub fn get(&self, role: Role) -> Held {
        match role {
            Role::NetHighlightBoard => self.net_highlight_board,
            Role::NetHighlightSheet => self.net_highlight_sheet,
            Role::Constrain => self.constrain,
            Role::Detach => self.detach,
            Role::SnapSuspend => self.snap_suspend,
        }
    }

    pub fn set(&mut self, role: Role, held: Held) {
        let slot = match role {
            Role::NetHighlightBoard => &mut self.net_highlight_board,
            Role::NetHighlightSheet => &mut self.net_highlight_sheet,
            Role::Constrain => &mut self.constrain,
            Role::Detach => &mut self.detach,
            Role::SnapSuspend => &mut self.snap_suspend,
        };
        *slot = held;
    }
}

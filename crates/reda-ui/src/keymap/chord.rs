//! A key and its modifiers, written portably (`"Mod+Shift+Z"`), matched against egui key
//! events by logical key (the character the key produces), and shown the way the platform
//! writes it.

use std::fmt;

use egui::{Event, Key, ModifierNames, Modifiers};

/// The key of a [`Chord`]: one of egui's named keys, or a character egui has no key for
/// (such as `*`), which is matched against the text the key types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyName {
    Key(Key),
    Char(char),
}

/// A key with modifiers. `Mod` is Cmd on macOS and Ctrl elsewhere; on other platforms
/// `Ctrl` is the same key as `Mod`, so `"Ctrl+Q"` and `"Mod+Q"` are one chord there.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub key: KeyName,
    /// `Mod`: Cmd on macOS, Ctrl elsewhere.
    pub command: bool,
    /// `Ctrl` as a key of its own: only on macOS.
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

const MAC: bool = cfg!(target_os = "macos");

impl Chord {
    /// Reads a portable chord such as `"Mod+Shift+Z"`, `"Del"`, `"+"` or `"Mod++"`.
    /// Modifiers are `Mod`, `Ctrl`, `Alt` (or `Option`) and `Shift`, in any order and case.
    pub fn parse(text: &str) -> Result<Chord, String> {
        Chord::parse_for(text, MAC)
    }

    /// [`Chord::parse`] as on macOS (`mac`) or elsewhere.
    pub fn parse_for(text: &str, mac: bool) -> Result<Chord, String> {
        let text = text.trim();
        // The key itself may be "+": "Mod++" or "+".
        let (mods, key) = match text.strip_suffix("++") {
            Some(mods) => (mods, "+"),
            None if text == "+" => ("", "+"),
            None => match text.rsplit_once('+') {
                Some((mods, key)) => (mods, key),
                None => ("", text),
            },
        };
        if key.is_empty() {
            return Err("no key after the modifiers".to_owned());
        }
        let mut chord = Chord {
            key: key_name(key).ok_or_else(|| format!("{key:?} is not a key"))?,
            command: false,
            ctrl: false,
            alt: false,
            shift: false,
        };
        for m in mods
            .split('+')
            .filter(|m| !mods.is_empty() || !m.is_empty())
        {
            match m.to_ascii_lowercase().as_str() {
                "mod" | "cmd" | "command" => chord.command = true,
                "ctrl" | "control" if mac => chord.ctrl = true,
                "ctrl" | "control" => chord.command = true,
                "alt" | "option" => chord.alt = true,
                "shift" => chord.shift = true,
                _ => return Err(format!("{m:?} is not a modifier")),
            }
        }
        Ok(chord)
    }

    /// The portable form: modifiers in the order Mod, Ctrl, Alt, Shift, then the key.
    pub fn portable(&self) -> String {
        let mut out = String::new();
        for (on, name) in [
            (self.command, "Mod+"),
            (self.ctrl, "Ctrl+"),
            (self.alt, "Alt+"),
            (self.shift, "Shift+"),
        ] {
            if on {
                out += name;
            }
        }
        out + &self.key.to_string()
    }

    /// The chord as menus show it on macOS (`mac`) or elsewhere: `Ctrl+Shift+Z`,
    /// `Shift+Cmd+Z`.
    pub fn label(&self, mac: bool) -> String {
        let modifiers = Modifiers {
            alt: self.alt,
            ctrl: self.ctrl,
            shift: self.shift,
            mac_cmd: mac && self.command,
            command: self.command,
        };
        let mods = ModifierNames::NAMES.format(&modifiers, mac);
        if mods.is_empty() {
            self.key.to_string()
        } else {
            format!("{mods}+{}", self.key)
        }
    }

    /// The chords a pressed key event may stand for, most exact first: the key with the
    /// modifiers held, then, for a punctuation key typed with Shift (`+` and `*` need it on
    /// many layouts), the key without Shift. Released keys give none.
    pub fn from_event(event: &Event) -> Vec<Chord> {
        Chord::from_event_for(event, MAC)
    }

    /// [`Chord::from_event`] as on macOS (`mac`) or elsewhere.
    pub fn from_event_for(event: &Event, mac: bool) -> Vec<Chord> {
        let (key, modifiers) = match event {
            Event::Key {
                key,
                pressed: true,
                modifiers,
                ..
            } => (KeyName::Key(*key), *modifiers),
            // Characters egui has no key for come only as text.
            Event::Text(text) => {
                let mut chars = text.chars();
                match (chars.next(), chars.next()) {
                    (Some(c), None) if key_name(&c.to_string()) == Some(KeyName::Char(c)) => {
                        (KeyName::Char(c), Modifiers::NONE)
                    }
                    _ => return Vec::new(),
                }
            }
            // The OS turns Mod+X/C/V into clipboard events before they are keys.
            Event::Cut => (KeyName::Key(Key::X), Modifiers::COMMAND),
            Event::Copy => (KeyName::Key(Key::C), Modifiers::COMMAND),
            Event::Paste(_) => (KeyName::Key(Key::V), Modifiers::COMMAND),
            _ => return Vec::new(),
        };
        let chord = Chord {
            key,
            command: if mac {
                modifiers.mac_cmd || modifiers.command
            } else {
                modifiers.command || modifiers.ctrl
            },
            ctrl: mac && modifiers.ctrl,
            alt: modifiers.alt,
            shift: modifiers.shift,
        };
        let mut out = vec![chord];
        if chord.shift && key.is_punctuation() {
            out.push(Chord {
                shift: false,
                ..chord
            });
        }
        out
    }
}

impl KeyName {
    /// A key typed as a character other than a letter or digit.
    fn is_punctuation(self) -> bool {
        match self {
            KeyName::Char(_) => true,
            KeyName::Key(key) => {
                let name = key.symbol_or_name();
                let mut chars = name.chars();
                matches!((chars.next(), chars.next()), (Some(c), None) if !c.is_alphanumeric())
            }
        }
    }
}

impl fmt::Display for KeyName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            KeyName::Key(Key::Plus) => f.write_str("+"),
            KeyName::Key(Key::Minus) => f.write_str("-"),
            KeyName::Key(Key::Slash) => f.write_str("/"),
            KeyName::Key(key) => f.write_str(key.name()),
            KeyName::Char(c) => write!(f, "{c}"),
        }
    }
}

impl fmt::Display for Chord {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.portable())
    }
}

/// The key a name stands for: egui's names (`"Z"`, `"Delete"`, `"Del"`, `"Space"`, `"F6"`,
/// `"+"`), and any other single printable character that is not a letter or digit.
fn key_name(name: &str) -> Option<KeyName> {
    let alias = match name.to_ascii_lowercase().as_str() {
        "del" => "Delete",
        "esc" => "Escape",
        "space" => "Space",
        "backspace" => "Backspace",
        "delete" => "Delete",
        "escape" => "Escape",
        "home" => "Home",
        "end" => "End",
        "tab" => "Tab",
        "enter" | "return" => "Enter",
        _ => name,
    };
    if let Some(key) = Key::from_name(alias) {
        return Some(KeyName::Key(key));
    }
    // Function keys and letters in lower case.
    if let Some(key) = Key::from_name(&alias.to_ascii_uppercase()) {
        return Some(KeyName::Key(key));
    }
    let mut chars = name.chars();
    match (chars.next(), chars.next()) {
        (Some(c), None) if !c.is_alphanumeric() && !c.is_whitespace() && !c.is_control() => {
            Some(KeyName::Char(c))
        }
        _ => None,
    }
}

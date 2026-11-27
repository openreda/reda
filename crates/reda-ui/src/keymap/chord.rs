//! A key and its modifiers, written portably (`"Mod+Shift+Z"`), matched against egui key
//! events, and shown the way the platform writes it. Digits are matched by physical key
//! (the top-row key in the US digit's position, on every layout); every other key by
//! logical key (the character the key produces).

use std::fmt;

use egui::{Event, Key, ModifierNames, Modifiers};

/// The key of a [`Chord`]: one of egui's named keys, or a character egui has no key for
/// (such as `*`), which is matched against the text the key types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum KeyName {
    Key(Key),
    Char(char),
    Keypad(u8),
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

    /// Whether the chord reaches the Keymap while a button or other widget has keyboard
    /// focus: one with Mod, Ctrl or Alt, or a function key. Plain keys (Space, Enter, the
    /// arrows, letters) stay with the widget.
    pub fn passes_focused_widgets(&self) -> bool {
        let function_key = match self.key {
            KeyName::Key(key) => {
                let name = key.name();
                name.len() > 1
                    && name.starts_with('F')
                    && name[1..].chars().all(|c| c.is_ascii_digit())
            }
            KeyName::Char(_) | KeyName::Keypad(_) => false,
        };
        self.command || self.ctrl || self.alt || function_key
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

    /// The chords a pressed key event may stand for, most exact first. A top-row digit key
    /// is first the digit in its US position with the modifiers held, whatever the layout
    /// makes it type (docs/spec/v0.3/3d-view.md#keys). Then the key it produces with the
    /// modifiers held, and, for a punctuation key typed with Shift (`+` and `*` need it on
    /// many layouts), that key without Shift. A digit is never matched by what a key
    /// types, only by where the key is, unless the event does not say where (as with
    /// synthesized input). Released keys give none.
    pub fn from_event(event: &Event) -> Vec<Chord> {
        Chord::from_event_for(event, MAC)
    }

    /// [`Chord::from_event`] as on macOS (`mac`) or elsewhere.
    pub fn from_event_for(event: &Event, mac: bool) -> Vec<Chord> {
        let chord = |key, modifiers: Modifiers| Chord {
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
        let (key, modifiers) = match event {
            // A native '*' has its own markers. F23 is a dedicated multiply,
            // F22 a key needing Shift to produce the character. The physical key is F24,
            // or the digit when the key is on the top row.
            Event::Key {
                key: key @ (Key::F22 | Key::F23),
                physical_key: Some(physical),
                pressed: true,
                modifiers,
                ..
            } if *physical == Key::F24 || top_row_digit(*physical) => {
                let mut star = *modifiers;
                star.shift &= *key == Key::F23;
                let star = chord(KeyName::Char('*'), star);
                return if top_row_digit(*physical) {
                    vec![chord(KeyName::Key(*physical), *modifiers), star]
                } else {
                    vec![star]
                };
            }
            Event::Key {
                key,
                pressed: true,
                modifiers,
                physical_key,
                ..
            } => {
                // The native adapter preserves the numpad digit's logical key and text.
                let keypad = (physical_key == &Some(Key::F24))
                    .then(|| key.name().parse::<u8>().ok())
                    .flatten();
                if let Some(d) = keypad {
                    return vec![chord(KeyName::Keypad(d), *modifiers)];
                }
                match physical_key {
                    Some(physical) if top_row_digit(*physical) => {
                        let mut out = vec![chord(KeyName::Key(*physical), *modifiers)];
                        if !top_row_digit(*key) {
                            out.extend(Chord::typed(chord(KeyName::Key(*key), *modifiers)));
                        }
                        return out;
                    }
                    // A digit typed by a key elsewhere is not the digit's binding.
                    Some(_) if top_row_digit(*key) => return Vec::new(),
                    _ => (KeyName::Key(*key), *modifiers),
                }
            }
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
        Chord::typed(chord(key, modifiers))
    }

    /// The chords a key that typed `chord` stands for: `chord`, then, for punctuation
    /// typed with Shift, the same key without Shift.
    fn typed(chord: Chord) -> Vec<Chord> {
        let mut out = vec![chord];
        if chord.shift && chord.key.is_punctuation() {
            out.push(Chord {
                shift: false,
                ..chord
            });
        }
        out
    }

    /// Whether `event` is a key on the top digit row, which the Keymap matches by its
    /// place, so the character it types (the [`Event::Text`] after it) belongs to it too.
    pub(crate) fn on_top_row(event: &Event) -> bool {
        matches!(event, Event::Key { physical_key: Some(key), pressed: true, .. } if top_row_digit(*key))
    }
}

/// Whether `key` is a digit, which as a physical key is the top-row key in that digit's US
/// position (the native adapter marks numpad digits apart).
fn top_row_digit(key: Key) -> bool {
    matches!(
        key,
        Key::Num0
            | Key::Num1
            | Key::Num2
            | Key::Num3
            | Key::Num4
            | Key::Num5
            | Key::Num6
            | Key::Num7
            | Key::Num8
            | Key::Num9
    )
}

impl KeyName {
    /// A key typed as a character other than a letter or digit.
    fn is_punctuation(self) -> bool {
        match self {
            KeyName::Keypad(_) => false,
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
            KeyName::Keypad(d) => write!(f, "Keypad{d}"),
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
    if let Some(n) = name
        .to_ascii_lowercase()
        .strip_prefix("keypad")
        .and_then(|n| n.parse().ok())
        .filter(|n| *n < 10)
    {
        return Some(KeyName::Keypad(n));
    }
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

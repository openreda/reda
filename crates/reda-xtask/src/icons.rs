//! Vendoring the interim placeholder icons (docs/spec/v0.1/app.md#icons).
//!
//! `crates/reda-ui/icons/sources.txt` names one Lucide or Tabler icon per semantic icon
//! key. `cargo xtask icons` fetches the two pinned npm packages, copies each named SVG to
//! `crates/reda-ui/icons/<key>.svg` drawn in white (`currentColor` becomes `#fff`, the
//! dark-theme colour that reda-ui swaps on load), copies the sets' licences, and rewrites
//! the README with each icon's provenance and `crates/reda-ui/src/icons/table.rs`, the
//! key → image table.
//!
//! REDA's own icons, drawn to the style guide in issue #32, are listed with their
//! provenance in `crates/reda-ui/icons/reda.txt`. The xtask leaves their SVGs alone, puts
//! them in the table and records them in the README; `sources.txt` covers only the
//! placeholders that remain.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};
use std::process::Command;

/// The pinned version of the `lucide-static` npm package.
pub const LUCIDE_VERSION: &str = "1.66.0";
/// The pinned version of the `@tabler/icons` npm package.
pub const TABLER_VERSION: &str = "3.50.0";

/// A placeholder icon set.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IconSet {
    Lucide,
    Tabler,
}

impl IconSet {
    const ALL: [IconSet; 2] = [IconSet::Lucide, IconSet::Tabler];

    fn id(self) -> &'static str {
        match self {
            IconSet::Lucide => "lucide",
            IconSet::Tabler => "tabler",
        }
    }

    fn package(self) -> &'static str {
        match self {
            IconSet::Lucide => "lucide-static",
            IconSet::Tabler => "@tabler/icons",
        }
    }

    fn version(self) -> &'static str {
        match self {
            IconSet::Lucide => LUCIDE_VERSION,
            IconSet::Tabler => TABLER_VERSION,
        }
    }

    fn tarball(self) -> String {
        match self {
            IconSet::Lucide => format!(
                "https://registry.npmjs.org/lucide-static/-/lucide-static-{LUCIDE_VERSION}.tgz"
            ),
            IconSet::Tabler => {
                format!("https://registry.npmjs.org/@tabler/icons/-/icons-{TABLER_VERSION}.tgz")
            }
        }
    }

    /// Where an icon's SVG sits inside the unpacked package.
    fn svg_path(self, name: &str) -> String {
        match self {
            IconSet::Lucide => format!("package/icons/{name}.svg"),
            IconSet::Tabler => format!("package/icons/outline/{name}.svg"),
        }
    }

    fn describe(self, name: &str) -> String {
        match self {
            IconSet::Lucide => format!("Lucide `{name}`"),
            IconSet::Tabler => format!("Tabler `{name}` (outline)"),
        }
    }

    fn licence(self) -> &'static str {
        match self {
            IconSet::Lucide => "ISC (parts MIT, from Feather)",
            IconSet::Tabler => "MIT",
        }
    }
}

/// One line of the sources list: the icon drawn for `key`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub key: String,
    pub set: IconSet,
    pub name: String,
}

fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
}

/// Reads the sources list: `<key> <set>:<icon>` per line, `#` comments and blank lines
/// skipped. Keys are lowercase snake_case and appear once.
pub fn parse_sources(text: &str) -> Result<Vec<Source>, String> {
    let mut sources = Vec::new();
    let mut keys = BTreeSet::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = |why: &str| format!("sources line {}: {why}: {line}", i + 1);
        let mut words = line.split_whitespace();
        let (Some(key), Some(source), None) = (words.next(), words.next(), words.next()) else {
            return Err(bad("expected `<key> <set>:<icon>`"));
        };
        if !valid_key(key) {
            return Err(bad("the key is not lowercase snake_case"));
        }
        let (set, name) = source
            .split_once(':')
            .ok_or_else(|| bad("expected `<set>:<icon>`"))?;
        let set = IconSet::ALL
            .into_iter()
            .find(|s| s.id() == set)
            .ok_or_else(|| bad("the set is neither lucide nor tabler"))?;
        if name.is_empty() || !name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-') {
            return Err(bad("the icon name is not a file name"));
        }
        if !keys.insert(key.to_owned()) {
            return Err(bad(&format!("the key {key} is listed twice")));
        }
        sources.push(Source {
            key: key.to_owned(),
            set,
            name: name.to_owned(),
        });
    }
    Ok(sources)
}

/// One of REDA's own icons: its key and its provenance, as the style guide asks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Owned {
    pub key: String,
    /// Who drew it: an agent model or a person.
    pub drawn_by: String,
    /// Any reference or generator it was drawn from, with its prompt.
    pub reference: String,
    /// Who cleaned it up.
    pub cleaned_up_by: String,
}

/// Reads the list of REDA's own icons: `<key> | <drawn by> | <reference> | <cleaned up
/// by>` per line, `#` comments and blank lines skipped. Keys appear once.
pub fn parse_owned(text: &str) -> Result<Vec<Owned>, String> {
    let mut owned = Vec::new();
    let mut keys = BTreeSet::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = |why: &str| format!("reda.txt line {}: {why}: {line}", i + 1);
        let fields: Vec<&str> = line.split('|').map(str::trim).collect();
        let [key, drawn_by, reference, cleaned_up_by] = fields[..] else {
            return Err(bad(
                "expected `<key> | <drawn by> | <reference> | <cleaned up by>`",
            ));
        };
        if !valid_key(key) {
            return Err(bad("the key is not lowercase snake_case"));
        }
        if !keys.insert(key.to_owned()) {
            return Err(bad(&format!("the key {key} is listed twice")));
        }
        owned.push(Owned {
            key: key.to_owned(),
            drawn_by: drawn_by.to_owned(),
            reference: reference.to_owned(),
            cleaned_up_by: cleaned_up_by.to_owned(),
        });
    }
    Ok(owned)
}

/// Removes every `<!-- … -->` comment and every `class="…"` attribute (with the space or
/// line break before it), draws `currentColor` as white, and ends the file with one newline.
pub fn vendor_svg(svg: &str) -> String {
    let mut out = svg.to_owned();
    while let Some(start) = out.find("<!--") {
        let Some(len) = out[start..].find("-->") else {
            break;
        };
        out.replace_range(start..start + len + 3, "");
    }
    while let Some(attr) = out.find("class=\"") {
        let start = out[..attr].trim_end().len();
        let Some(len) = out[attr + 7..].find('"') else {
            break;
        };
        out.replace_range(start..attr + 7 + len + 1, "");
    }
    let out = out.replace("currentColor", "#fff");
    format!("{}\n", out.trim())
}

/// The generated `reda-ui/src/icons/table.rs` before rustfmt: every key with its SVG,
/// sorted by key.
pub fn table_rs(keys: &[String]) -> String {
    let mut keys = keys.to_vec();
    keys.sort();
    let mut out = String::from(
        "// @generated by `cargo xtask icons` from icons/sources.txt; do not edit.\n\n\
         /// Every icon key with its SVG (dark-theme colours), sorted by key.\n\
         pub(super) const ICONS: &[(&str, egui::ImageSource<'static>)] = &[\n",
    );
    for key in keys {
        let _ = writeln!(
            out,
            "    (\"{key}\", egui::include_image!(\"../../icons/{key}.svg\")),"
        );
    }
    out.push_str("];\n");
    out
}

/// The icon set's README: one provenance row per icon, REDA's own first, then where the
/// placeholders come from.
pub fn readme(owned: &[Owned], sources: &[Source]) -> String {
    let mut out = String::from(
        "# Icons\n\n\
         <!-- @generated by `cargo xtask icons` from reda.txt and sources.txt; do not edit. -->\n\n\
         One SVG per semantic icon key (docs/spec/v0.1/app.md#icons). Every key gets REDA's \
         own art, drawn to the style guide in issue #32; until then a Lucide or Tabler \
         placeholder stands in.\n\n\
         ## Role colours\n\n\
         REDA's own icons use only these colours, written as the dark-theme value; reda-ui \
         swaps each for its light-theme value on load (`ROLES` in `src/icons.rs`). Each \
         reaches at least 3:1 on its theme's panel.\n\n\
         | Role | Dark (in file) | Light |\n|---|---|---|\n\
         | Existing / context (pins, pads, frames, the thing acted on) | `#8a93a0` | `#727984` |\n\
         | Created, schematic (Wire, Net Label, Power Port…) | `#2fb36b` | `#228a51` |\n\
         | Created, PCB copper (Track, Via, Zone, Pad…) | `#e08a2e` | `#ac6921` |\n\
         | Created, general (file, edit, view, selection, checks) | `#3d8fd6` | `#347dbc` |\n\
         | Picked point | `#ec4f7a` | `#d2456c` |\n\
         | Created, board edge and keepout (Board Outline, Board Cutout, Keepout) | `#d4c03c` | `#8a7a12` |\n\n\
         ## REDA's own icons\n\n\
         MIT, like the repo. Each is listed in `reda.txt`, which `cargo xtask icons` leaves \
         alone.\n\n\
         | Key | Drawn by | Reference | Cleaned up by |\n|---|---|---|---|\n",
    );
    for o in owned {
        let _ = writeln!(
            out,
            "| `{}` | {} | {} | {} |",
            o.key, o.drawn_by, o.reference, o.cleaned_up_by
        );
    }
    out.push_str(
        "\n## Placeholder icons\n\n\
         Interim art, tracked until REDA's own replaces every one of them. Each SVG is copied \
         from the npm package below, with comments and `class` attributes removed and \
         `currentColor` rewritten to `#fff`, the dark-theme colour reda-ui swaps for the \
         light theme on load. To change one, edit `sources.txt` and run \
         `cargo xtask icons`.\n\n\
         | Set | Package | Licence |\n|---|---|---|\n",
    );
    for set in IconSet::ALL {
        let _ = writeln!(
            out,
            "| {} | `{}@{}` | {}, in `LICENSE-{}.txt` |",
            match set {
                IconSet::Lucide => "Lucide",
                IconSet::Tabler => "Tabler",
            },
            set.package(),
            set.version(),
            set.licence(),
            set.id()
        );
    }
    out.push_str(
        "\nEvery icon below was drawn by its set's authors and vendored unmodified \
         apart from the rewrite above.\n\n| Key | Source |\n|---|---|\n",
    );
    for s in sources {
        let _ = writeln!(out, "| `{}` | {} |", s.key, s.set.describe(&s.name));
    }
    out
}

/// `cargo xtask icons [--from <dir>]`: vendors the icons named in `<ui>/icons/sources.txt`
/// into `<ui>/icons`, and regenerates the README and `<ui>/src/icons/table.rs`. `from`
/// holds the unpacked packages as `<dir>/lucide/package` and `<dir>/tabler/package`;
/// without it they are downloaded with `curl` and unpacked with `tar` into `cache`.
pub fn vendor(ui: &Path, from: Option<&Path>, cache: &Path) -> Result<usize, String> {
    let icons = ui.join("icons");
    let list = read(&icons.join("sources.txt"))?;
    let sources = parse_sources(&list)?;
    let owned_list = icons.join("reda.txt");
    let owned = if owned_list.exists() {
        parse_owned(&read(&owned_list)?)?
    } else {
        Vec::new()
    };
    for o in &owned {
        if sources.iter().any(|s| s.key == o.key) {
            return Err(format!(
                "{} is REDA's own icon (reda.txt); drop it from sources.txt",
                o.key
            ));
        }
        let svg = icons.join(format!("{}.svg", o.key));
        if !svg.exists() {
            return Err(format!(
                "reda.txt lists {}, which has no SVG",
                svg.display()
            ));
        }
    }
    let root = match from {
        Some(dir) => dir.to_path_buf(),
        None => fetch(cache)?,
    };
    // Old placeholders go first, so a key dropped from the list loses its file. REDA's own
    // SVGs stay.
    for entry in std::fs::read_dir(&icons).map_err(|e| format!("{}: {e}", icons.display()))? {
        let path = entry.map_err(|e| e.to_string())?.path();
        let key = path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or_default();
        let is_owned = owned.iter().any(|o| o.key == key);
        if path.extension().is_some_and(|e| e == "svg") && !is_owned {
            std::fs::remove_file(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        }
    }
    for s in &sources {
        let path = root.join(s.set.id()).join(s.set.svg_path(&s.name));
        let svg = read(&path)?;
        write(&icons.join(format!("{}.svg", s.key)), &vendor_svg(&svg))?;
    }
    for set in IconSet::ALL {
        let licence = read(&root.join(set.id()).join("package/LICENSE"))?;
        write(&icons.join(format!("LICENSE-{}.txt", set.id())), &licence)?;
    }
    write(&icons.join("README.md"), &readme(&owned, &sources))?;
    let keys: Vec<String> = sources
        .iter()
        .map(|s| s.key.clone())
        .chain(owned.iter().map(|o| o.key.clone()))
        .collect();
    let table = ui.join("src/icons/table.rs");
    write(&table, &table_rs(&keys))?;
    // Formatted as `cargo fmt` would, so `cargo fmt --check` passes on it.
    run(Command::new("rustfmt")
        .args(["--edition", "2024"])
        .arg(&table))?;
    Ok(sources.len())
}

/// Downloads and unpacks both packages into `cache/<set>`, once per version.
fn fetch(cache: &Path) -> Result<PathBuf, String> {
    for set in IconSet::ALL {
        let dir = cache.join(set.id());
        let stamp = dir.join(format!("package/.reda-{}", set.version()));
        if stamp.exists() {
            continue;
        }
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
        let tarball = dir.join("package.tgz");
        run(Command::new("curl")
            .args(["-sfL", "-o"])
            .arg(&tarball)
            .arg(set.tarball()))?;
        run(Command::new("tar")
            .arg("xzf")
            .arg(&tarball)
            .arg("-C")
            .arg(&dir))?;
        write(&stamp, "")?;
    }
    Ok(cache.to_path_buf())
}

fn run(command: &mut Command) -> Result<(), String> {
    let status = command.status().map_err(|e| format!("{command:?}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{command:?} failed: {status}"))
    }
}

fn read(path: &Path) -> Result<String, String> {
    std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))
}

fn write(path: &Path, text: &str) -> Result<(), String> {
    std::fs::write(path, text).map_err(|e| format!("{}: {e}", path.display()))
}

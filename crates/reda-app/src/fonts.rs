//! The fonts installed on the machine, handed to the engine on demand.
//!
//! The engine reads no font files: PCB text may name any installed family, and the UI asks
//! [`InstalledFonts::load`] for the families a Project uses. Loading every installed font
//! up front would take hundreds of megabytes, so a background thread indexes the OS font
//! folders by reading only a few small tables of each file, and a family's bytes are read
//! when it is first asked for.
//!
//! The chrome's own font Preferences use the same index (app.md#preferences): it records
//! every face of a family with its weight and whether it is fixed-pitch, which is what
//! `reda-ui` needs to resolve a chosen family and to fill the two font lists.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use reda_ui::engine::command::FontFace;
use reda_ui::shell::{FontSource, InstalledFonts};
use reda_ui::theme::fonts::{Face, Family};

/// The font files the index looks at.
const EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];
/// How deep the folder walk goes (font folders nest a few levels; links could loop).
const MAX_DEPTH: usize = 8;
/// A `name` table larger than this is not read.
const MAX_NAME_TABLE: u32 = 1 << 20;

/// A `fvar` table larger than this is not read.
const MAX_FVAR_TABLE: u32 = 1 << 12;

/// One face's names, from its `name` table, and what its other tables say of it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceName {
    /// The face's index in its file (0 unless the file is a collection).
    pub index: u32,
    pub family: String,
    /// The style within the family, such as "Regular" or "Bold Italic".
    pub style: String,
    pub traits: Traits,
}

/// What a face's `OS/2`, `post`, `head` and `fvar` tables say of it. A table that is
/// missing or cut short leaves the defaults: an upright regular of normal width.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Traits {
    /// The weight class, 1 to 1000 (400 is regular).
    pub weight: u16,
    /// The ends of a variable font's weight axis.
    pub weight_axis: Option<(u16, u16)>,
    /// The width class, 1 to 9 (5 is normal).
    pub width: u16,
    pub italic: bool,
    pub fixed_pitch: bool,
    /// The face says it covers basic Latin.
    pub latin: bool,
}

impl Default for Traits {
    fn default() -> Traits {
        Traits {
            weight: 400,
            weight_axis: None,
            width: 5,
            italic: false,
            fixed_pitch: false,
            latin: false,
        }
    }
}

impl Traits {
    /// Reads the tables named in `directory` (a font's table records).
    fn read<R: Read + Seek>(file: &mut R, directory: &[u8]) -> Traits {
        let mut table = |tag: &[u8; 4], at_most: u32| -> Option<Vec<u8>> {
            let record = directory.chunks_exact(16).find(|r| &r[..4] == tag)?;
            let (start, length) = (be32(&record[8..12]), be32(&record[12..16]));
            let mut bytes = vec![0; length.min(at_most) as usize];
            file.seek(SeekFrom::Start(u64::from(start))).ok()?;
            file.read_exact(&mut bytes).ok()?;
            Some(bytes)
        };
        let mut traits = Traits::default();
        // OS/2: weight and width classes, PANOSE, the Unicode ranges and the style bits.
        let os2 = table(b"OS/2", 64).unwrap_or_default();
        if let Some(weight) = os2.get(4..6).map(be16) {
            traits.weight = match weight {
                // Some old fonts count in hundreds.
                1..=9 => weight * 100,
                10..=1000 => weight,
                _ => 400,
            };
        }
        if let Some(width @ 1..=9) = os2.get(6..8).map(be16) {
            traits.width = width;
        }
        // PANOSE: a Latin text face (family kind 2) of monospaced proportion (9).
        let panose_fixed = os2.get(32..42).is_some_and(|p| p[0] == 2 && p[3] == 9);
        traits.latin = os2.get(42..46).is_some_and(|range| be32(range) & 1 != 0);
        let head = table(b"head", 46).unwrap_or_default();
        traits.italic = match os2.get(62..64).map(be16) {
            // fsSelection: ITALIC or OBLIQUE.
            Some(selection) => selection & 0x201 != 0,
            // macStyle: italic.
            None => head.get(44..46).is_some_and(|style| be16(style) & 2 != 0),
        };
        let post = table(b"post", 16).unwrap_or_default();
        traits.fixed_pitch = panose_fixed || post.get(12..16).is_some_and(|fixed| be32(fixed) != 0);
        traits.weight_axis = table(b"fvar", MAX_FVAR_TABLE).and_then(|fvar| weight_axis(&fvar));
        traits
    }
}

/// The ends of the `wght` axis in a `fvar` table, as whole weights.
fn weight_axis(fvar: &[u8]) -> Option<(u16, u16)> {
    let axes = usize::from(be16(fvar.get(4..6)?));
    let count = usize::from(be16(fvar.get(8..10)?));
    let size = usize::from(be16(fvar.get(10..12)?)).max(20);
    (0..count).find_map(|i| {
        let axis = fvar.get(axes + i * size..axes + i * size + 20)?;
        if &axis[..4] != b"wght" {
            return None;
        }
        // 16.16 fixed point: the whole part is the weight.
        let whole = |b: &[u8]| be16(&b[..2]).clamp(1, 1000);
        let (lightest, heaviest) = (whole(&axis[4..8]), whole(&axis[12..16]));
        (lightest < heaviest).then_some((lightest, heaviest))
    })
}

/// The faces in a font file: one entry per family name each face goes by (its legacy
/// family and, when different, its typographic family).
pub fn faces<R: Read + Seek>(file: &mut R) -> io::Result<Vec<FaceName>> {
    let tag = read_u32(file, 0)?;
    let offsets = match &tag.to_be_bytes() {
        b"ttcf" => {
            let count = read_u32(file, 8)?.min(1024);
            (0..count)
                .map(|i| read_u32(file, 12 + 4 * u64::from(i)))
                .collect::<io::Result<Vec<_>>>()?
        }
        [0, 1, 0, 0] | b"OTTO" | b"true" => vec![0],
        _ => return Err(invalid("not an OpenType font")),
    };
    let mut out = Vec::new();
    for (index, offset) in offsets.into_iter().enumerate() {
        out.extend(face_names(file, u64::from(offset), index as u32)?);
    }
    Ok(out)
}

fn face_names<R: Read + Seek>(file: &mut R, offset: u64, index: u32) -> io::Result<Vec<FaceName>> {
    let tables = read_u16(file, offset + 4)?;
    let mut directory = vec![0; 16 * usize::from(tables)];
    file.seek(SeekFrom::Start(offset + 12))?;
    file.read_exact(&mut directory)?;
    let (start, length) = directory
        .chunks_exact(16)
        .find(|record| &record[..4] == b"name")
        .map(|r| (be32(&r[8..12]), be32(&r[12..16])))
        .ok_or_else(|| invalid("no name table"))?;
    if length > MAX_NAME_TABLE {
        return Err(invalid("name table too large"));
    }
    let mut table = vec![0; length as usize];
    file.seek(SeekFrom::Start(u64::from(start)))?;
    file.read_exact(&mut table)?;
    let names = NameTable::parse(&table).ok_or_else(|| invalid("bad name table"))?;
    let traits = Traits::read(file, &directory);

    let family = names.get(1).ok_or_else(|| invalid("no family name"))?;
    let style = names.get(2).unwrap_or_default();
    let mut out = vec![FaceName {
        index,
        family: family.clone(),
        style: style.clone(),
        traits,
    }];
    if let Some(typographic) = names.get(16).filter(|t| *t != family) {
        out.push(FaceName {
            index,
            family: typographic,
            style: names.get(17).unwrap_or(style),
            traits,
        });
    }
    Ok(out)
}

/// The strings of a `name` table.
struct NameTable<'a> {
    table: &'a [u8],
    count: usize,
    strings: usize,
}

impl<'a> NameTable<'a> {
    fn parse(table: &'a [u8]) -> Option<NameTable<'a>> {
        let count = usize::from(be16(table.get(2..4)?));
        let strings = usize::from(be16(table.get(4..6)?));
        table.get(6..6 + 12 * count)?;
        Some(NameTable {
            table,
            count,
            strings,
        })
    }

    /// Name `id`, preferring Windows US English, then any Unicode record, then Mac Roman.
    fn get(&self, id: u16) -> Option<String> {
        let mut best: Option<(u8, String)> = None;
        for i in 0..self.count {
            let r = &self.table[6 + 12 * i..18 + 12 * i];
            let (platform, encoding, language) = (be16(&r[0..2]), be16(&r[2..4]), be16(&r[4..6]));
            if be16(&r[6..8]) != id {
                continue;
            }
            let (len, at) = (usize::from(be16(&r[8..10])), usize::from(be16(&r[10..12])));
            let Some(bytes) = self.table.get(self.strings + at..self.strings + at + len) else {
                continue;
            };
            let (rank, text) = match (platform, encoding) {
                (3, 1 | 10) if language == 0x409 => (0, utf16(bytes)),
                (3, 1 | 10) | (0, _) => (1, utf16(bytes)),
                (1, 0) => (2, bytes.iter().map(|&b| char::from(b)).collect()),
                _ => continue,
            };
            if best.as_ref().is_none_or(|(r, _)| rank < *r) && !text.trim().is_empty() {
                best = Some((rank, text.trim().to_owned()));
            }
        }
        best.map(|(_, text)| text)
    }
}

fn utf16(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes.chunks_exact(2).map(be16).collect();
    String::from_utf16_lossy(&units)
}

fn be16(b: &[u8]) -> u16 {
    u16::from_be_bytes([b[0], b[1]])
}

fn be32(b: &[u8]) -> u32 {
    u32::from_be_bytes([b[0], b[1], b[2], b[3]])
}

fn read_u16<R: Read + Seek>(file: &mut R, at: u64) -> io::Result<u16> {
    let mut b = [0; 2];
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(&mut b)?;
    Ok(u16::from_be_bytes(b))
}

fn read_u32<R: Read + Seek>(file: &mut R, at: u64) -> io::Result<u32> {
    let mut b = [0; 4];
    file.seek(SeekFrom::Start(at))?;
    file.read_exact(&mut b)?;
    Ok(u32::from_be_bytes(b))
}

fn invalid(why: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, why)
}

/// Installed families by lower-case name: every face of each, and the file and face
/// index of its regular face (or the first face found when it has none).
#[derive(Debug, Default)]
pub struct Index {
    families: HashMap<String, Indexed>,
}

#[derive(Debug)]
struct Indexed {
    face: (PathBuf, u32),
    regular: bool,
    family: Family,
}

impl Index {
    pub fn build(faces: impl IntoIterator<Item = (FaceName, PathBuf)>) -> Index {
        let mut families: HashMap<String, Indexed> = HashMap::new();
        for (face, path) in faces {
            let regular = is_regular(&face.style);
            let traits = face.traits;
            let listed = Face {
                file: path.clone(),
                index: face.index,
                weight: traits.weight,
                weight_axis: traits.weight_axis,
                width: traits.width,
                italic: traits.italic,
                fixed_pitch: traits.fixed_pitch,
                latin: traits.latin,
            };
            match families.entry(face.family.to_lowercase()) {
                Entry::Vacant(v) => {
                    v.insert(Indexed {
                        face: (path, face.index),
                        regular,
                        family: Family {
                            name: face.family,
                            faces: vec![listed],
                        },
                    });
                }
                Entry::Occupied(mut o) => {
                    let indexed = o.get_mut();
                    // The same face installed twice (system-wide and per-user) counts once.
                    let same = |f: &Face| Face {
                        file: listed.file.clone(),
                        ..f.clone()
                    };
                    if !indexed.family.faces.iter().any(|f| same(f) == listed) {
                        indexed.family.faces.push(listed);
                    }
                    if regular && !indexed.regular {
                        indexed.face = (path, face.index);
                        indexed.regular = true;
                    }
                }
            }
        }
        Index { families }
    }

    /// The file and face index for `family`, compared case-insensitively.
    pub fn get(&self, family: &str) -> Option<&(PathBuf, u32)> {
        self.families.get(&family.to_lowercase()).map(|i| &i.face)
    }

    /// `family` with every face it has, compared case-insensitively.
    pub fn family(&self, family: &str) -> Option<&Family> {
        self.families.get(&family.to_lowercase()).map(|i| &i.family)
    }

    /// Every family, in alphabetical order.
    pub fn families(&self) -> Vec<&Family> {
        let mut all: Vec<(&String, &Family)> =
            self.families.iter().map(|(k, i)| (k, &i.family)).collect();
        all.sort_by_key(|(key, _)| *key);
        all.into_iter().map(|(_, family)| family).collect()
    }
}

fn is_regular(style: &str) -> bool {
    matches!(
        style.to_lowercase().as_str(),
        "regular" | "book" | "normal" | "roman" | ""
    )
}

/// Every font file under `dirs`, with its faces.
fn scan_dirs(dirs: &[PathBuf]) -> Index {
    let mut found = Vec::new();
    for dir in dirs {
        walk(dir, 0, &mut found);
    }
    Index::build(found)
}

fn walk(dir: &Path, depth: usize, found: &mut Vec<(FaceName, PathBuf)>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut paths: Vec<PathBuf> = entries.filter_map(|e| e.ok().map(|e| e.path())).collect();
    // A stable order, so which face wins a tie does not depend on the file system.
    paths.sort();
    for path in paths {
        if path.is_dir() {
            if depth < MAX_DEPTH {
                walk(&path, depth + 1, found);
            }
            continue;
        }
        let is_font = path
            .extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| EXTENSIONS.contains(&e.to_lowercase().as_str()));
        if !is_font {
            continue;
        }
        let Ok(mut file) = std::fs::File::open(&path) else {
            continue;
        };
        if let Ok(names) = faces(&mut io::BufReader::new(&mut file)) {
            found.extend(names.into_iter().map(|n| (n, path.clone())));
        }
    }
}

/// The fonts installed on this machine, indexed on a background thread.
pub struct SystemFonts {
    index: Arc<OnceLock<Index>>,
}

impl SystemFonts {
    /// Indexes `dirs` now.
    #[cfg(test)]
    pub fn scan(dirs: Vec<PathBuf>) -> SystemFonts {
        let index = OnceLock::new();
        let _ = index.set(scan_dirs(&dirs));
        SystemFonts {
            index: Arc::new(index),
        }
    }

    /// Starts indexing `dirs` on a background thread; [`load`](InstalledFonts::load)
    /// waits for it.
    pub fn scan_in_background(dirs: Vec<PathBuf>) -> SystemFonts {
        let index = Arc::new(OnceLock::new());
        let filled = index.clone();
        std::thread::spawn(move || {
            let _ = filled.set(scan_dirs(&dirs));
        });
        SystemFonts { index }
    }

    /// The OS font folders, system-wide and per-user.
    pub fn os_dirs() -> Vec<PathBuf> {
        let home = std::env::home_dir();
        let mut dirs = Vec::new();
        if cfg!(target_os = "windows") {
            if let Some(windir) = std::env::var_os("WINDIR") {
                dirs.push(PathBuf::from(windir).join("Fonts"));
            }
            if let Some(local) = std::env::var_os("LOCALAPPDATA") {
                dirs.push(PathBuf::from(local).join("Microsoft/Windows/Fonts"));
            }
        } else if cfg!(target_os = "macos") {
            dirs.push(PathBuf::from("/System/Library/Fonts"));
            dirs.push(PathBuf::from("/Library/Fonts"));
            dirs.extend(home.map(|h| h.join("Library/Fonts")));
        } else {
            let data_home = std::env::var_os("XDG_DATA_HOME")
                .map(PathBuf::from)
                .or_else(|| home.as_ref().map(|h| h.join(".local/share")));
            dirs.extend(data_home.map(|d| d.join("fonts")));
            dirs.extend(home.map(|h| h.join(".fonts")));
            let data_dirs = std::env::var("XDG_DATA_DIRS")
                .ok()
                .filter(|v| !v.is_empty())
                .unwrap_or_else(|| "/usr/local/share:/usr/share".to_owned());
            dirs.extend(data_dirs.split(':').map(|d| Path::new(d).join("fonts")));
        }
        dirs
    }
}

impl FontSource for SystemFonts {
    fn family(&self, name: &str) -> Option<Family> {
        self.index.wait().family(name).cloned()
    }

    fn families(&self) -> Vec<Family> {
        self.index.wait().families().into_iter().cloned().collect()
    }

    fn read(&self, face: &Face) -> Option<Arc<[u8]>> {
        std::fs::read(&face.file).ok().map(Arc::from)
    }
}

impl InstalledFonts for SystemFonts {
    fn load(&self, family: &str) -> Option<FontFace> {
        let (path, index) = self.index.wait().get(family)?.clone();
        let bytes = std::fs::read(path).ok()?;
        Some(FontFace {
            family: family.to_owned(),
            bytes: bytes.into(),
            index,
        })
    }
}

#[cfg(test)]
mod tests;

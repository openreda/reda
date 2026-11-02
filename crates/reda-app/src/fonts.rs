//! The fonts installed on the machine, handed to the engine on demand.
//!
//! The engine reads no font files: PCB text may name any installed family, and the UI asks
//! [`InstalledFonts::load`] for the families a Project uses. Loading every installed font
//! up front would take hundreds of megabytes, so a background thread indexes the OS font
//! folders by reading only each file's `name` table, and a family's bytes are read when it
//! is first asked for.

use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::io::{self, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use reda_ui::engine::command::FontFace;
use reda_ui::shell::InstalledFonts;

/// The font files the index looks at.
const EXTENSIONS: [&str; 4] = ["ttf", "otf", "ttc", "otc"];
/// How deep the folder walk goes (font folders nest a few levels; links could loop).
const MAX_DEPTH: usize = 8;
/// A `name` table larger than this is not read.
const MAX_NAME_TABLE: u32 = 1 << 20;

/// One face's names, from its `name` table.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FaceName {
    /// The face's index in its file (0 unless the file is a collection).
    pub index: u32,
    pub family: String,
    /// The style within the family, such as "Regular" or "Bold Italic".
    pub style: String,
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

    let family = names.get(1).ok_or_else(|| invalid("no family name"))?;
    let style = names.get(2).unwrap_or_default();
    let mut out = vec![FaceName {
        index,
        family: family.clone(),
        style: style.clone(),
    }];
    if let Some(typographic) = names.get(16).filter(|t| *t != family) {
        out.push(FaceName {
            index,
            family: typographic,
            style: names.get(17).unwrap_or(style),
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

/// Installed families by lower-case name: the file and face index of each family's
/// regular face (or the first face found when it has none).
#[derive(Debug, Default)]
pub struct Index {
    families: HashMap<String, Indexed>,
}

#[derive(Debug)]
struct Indexed {
    face: (PathBuf, u32),
    regular: bool,
}

impl Index {
    pub fn build(faces: impl IntoIterator<Item = (FaceName, PathBuf)>) -> Index {
        let mut families: HashMap<String, Indexed> = HashMap::new();
        for (face, path) in faces {
            let new = Indexed {
                face: (path, face.index),
                regular: is_regular(&face.style),
            };
            let slot = families.entry(face.family.to_lowercase());
            match slot {
                Entry::Vacant(v) => {
                    v.insert(new);
                }
                Entry::Occupied(mut o) if new.regular && !o.get().regular => {
                    o.insert(new);
                }
                Entry::Occupied(_) => {}
            }
        }
        Index { families }
    }

    /// The file and face index for `family`, compared case-insensitively.
    pub fn get(&self, family: &str) -> Option<&(PathBuf, u32)> {
        self.families.get(&family.to_lowercase()).map(|i| &i.face)
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

use std::io::Cursor;
use std::path::{Path, PathBuf};

use super::*;

fn noto_sans() -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-text/fonts/noto-sans/NotoSans-Regular.ttf");
    std::fs::read(path).unwrap()
}

/// Wraps single fonts into a font collection (TTC), moving their table offsets.
fn collection(fonts: &[Vec<u8>]) -> Vec<u8> {
    let header = 12 + 4 * fonts.len();
    let mut out = b"ttcf".to_vec();
    out.extend_from_slice(&[0, 1, 0, 0]);
    out.extend_from_slice(&(fonts.len() as u32).to_be_bytes());
    let mut at = header;
    let mut bodies = Vec::new();
    for font in fonts {
        out.extend_from_slice(&(at as u32).to_be_bytes());
        let mut body = font.clone();
        let tables = u16::from_be_bytes([body[4], body[5]]) as usize;
        for t in 0..tables {
            let o = 12 + 16 * t + 8;
            let offset = u32::from_be_bytes(body[o..o + 4].try_into().unwrap());
            body[o..o + 4].copy_from_slice(&(offset + at as u32).to_be_bytes());
        }
        at += body.len();
        bodies.push(body);
    }
    for body in bodies {
        out.extend_from_slice(&body);
    }
    out
}

#[test]
fn a_font_file_names_its_family_and_style() {
    let faces = faces(&mut Cursor::new(noto_sans())).unwrap();
    assert_eq!(
        faces,
        [FaceName {
            index: 0,
            family: "Noto Sans".into(),
            style: "Regular".into(),
        }]
    );
}

#[test]
fn every_face_of_a_collection_is_named_with_its_index() {
    let ttc = collection(&[noto_sans(), noto_sans()]);
    let faces = faces(&mut Cursor::new(ttc)).unwrap();
    let indices: Vec<u32> = faces.iter().map(|f| f.index).collect();
    assert_eq!(indices, [0, 1]);
    assert!(faces.iter().all(|f| f.family == "Noto Sans"));
}

#[test]
fn files_that_are_not_fonts_are_refused() {
    assert!(faces(&mut Cursor::new(b"hello, not a font".to_vec())).is_err());
    assert!(faces(&mut Cursor::new(Vec::new())).is_err());
}

fn face(family: &str, style: &str, path: &str, index: u32) -> (FaceName, PathBuf) {
    (
        FaceName {
            index,
            family: family.into(),
            style: style.into(),
        },
        PathBuf::from(path),
    )
}

#[test]
fn the_index_prefers_each_familys_regular_face() {
    let index = Index::build([
        face("Fira Code", "Bold", "/f/FiraCode-Bold.ttf", 0),
        face("Fira Code", "Regular", "/f/FiraCode-Regular.ttf", 0),
        face("Fira Code", "Light", "/f/FiraCode-Light.ttf", 0),
        face("Only Bold", "Bold", "/f/OnlyBold.ttf", 0),
        face("Only Bold", "Black", "/f/OnlyBlack.ttf", 0),
    ]);
    assert_eq!(
        index.get("fira code"),
        Some(&(PathBuf::from("/f/FiraCode-Regular.ttf"), 0))
    );
    // With no regular face, the first one found.
    assert_eq!(
        index.get("ONLY BOLD"),
        Some(&(PathBuf::from("/f/OnlyBold.ttf"), 0))
    );
    assert_eq!(index.get("missing"), None);
}

#[test]
fn installed_fonts_are_loaded_from_the_indexed_files() {
    let dir = std::env::temp_dir().join(format!("reda-app-fonts-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("sub")).unwrap();
    std::fs::write(dir.join("sub/NotoSans-Regular.ttf"), noto_sans()).unwrap();
    std::fs::write(dir.join("readme.txt"), b"not a font").unwrap();
    std::fs::write(dir.join("broken.ttf"), b"not a font either").unwrap();

    let fonts = SystemFonts::scan(vec![dir.clone(), dir.join("does-not-exist")]);
    let face = fonts.load("noto sans").expect("found");
    assert_eq!(face.family, "noto sans");
    assert_eq!(face.index, 0);
    assert_eq!(&*face.bytes, &noto_sans()[..]);
    assert!(fonts.load("Comic Sans").is_none());
    std::fs::remove_dir_all(&dir).unwrap();
}

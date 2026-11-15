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
            traits: Traits {
                latin: true,
                ..Traits::default()
            },
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
            traits: Traits::default(),
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

/// A font REDA bundles for its chrome, from `reda-ui`.
fn chrome_font(file: &str) -> Vec<u8> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../reda-ui/fonts")
        .join(file);
    std::fs::read(path).unwrap()
}

fn traits_of(font: Vec<u8>) -> Traits {
    faces(&mut Cursor::new(font)).unwrap()[0].traits
}

#[test]
fn a_face_records_its_weight() {
    assert_eq!(traits_of(noto_sans()).weight, 400);
    assert_eq!(traits_of(chrome_font("inter/Inter-Medium.ttf")).weight, 500);
    assert_eq!(
        traits_of(chrome_font("inter/Inter-SemiBold.ttf")).weight,
        600
    );
}

#[test]
fn a_face_records_whether_it_is_fixed_pitch() {
    let mono = traits_of(chrome_font("jetbrains-mono/JetBrainsMonoNL-Regular.ttf"));
    assert!(mono.fixed_pitch);
    assert_eq!((mono.weight, mono.width, mono.italic), (400, 5, false));
    assert!(mono.latin);
    assert!(!traits_of(noto_sans()).fixed_pitch);
    assert!(!traits_of(chrome_font("inter/Inter-Regular.ttf")).fixed_pitch);
}

/// A font's table directory with `tables` laid out after it.
fn font_with(tables: &[(&[u8; 4], Vec<u8>)]) -> (Vec<u8>, Vec<u8>) {
    let mut directory = Vec::new();
    let mut body = Vec::new();
    let start = 12 + 16 * tables.len();
    for (tag, data) in tables {
        directory.extend_from_slice(*tag);
        directory.extend_from_slice(&[0; 4]);
        directory.extend_from_slice(&((start + body.len()) as u32).to_be_bytes());
        directory.extend_from_slice(&(data.len() as u32).to_be_bytes());
        body.extend_from_slice(data);
    }
    let mut file = vec![0; 12];
    file.extend_from_slice(&directory);
    file.extend_from_slice(&body);
    (file, directory)
}

fn os2(weight: u16, width: u16, panose: [u8; 10], selection: u16) -> Vec<u8> {
    let mut table = vec![0; 64];
    table[4..6].copy_from_slice(&weight.to_be_bytes());
    table[6..8].copy_from_slice(&width.to_be_bytes());
    table[32..42].copy_from_slice(&panose);
    table[62..64].copy_from_slice(&selection.to_be_bytes());
    table
}

fn read_traits(tables: &[(&[u8; 4], Vec<u8>)]) -> Traits {
    let (file, directory) = font_with(tables);
    Traits::read(&mut Cursor::new(file), &directory)
}

#[test]
fn style_bits_width_and_the_fixed_pitch_flags_are_read() {
    // Italic and oblique are both "not upright"; width class 3 is condensed.
    let t = read_traits(&[(b"OS/2", os2(700, 3, [0; 10], 0x0001))]);
    assert_eq!((t.weight, t.width, t.italic), (700, 3, true));
    assert!(read_traits(&[(b"OS/2", os2(400, 5, [0; 10], 0x0200))]).italic);
    assert!(!read_traits(&[(b"OS/2", os2(400, 5, [0; 10], 0x0040))]).italic);
    // Fixed pitch: PANOSE's monospaced proportion, or the `post` flag.
    let panose = [2, 11, 5, 9, 0, 0, 0, 0, 0, 0];
    assert!(read_traits(&[(b"OS/2", os2(400, 5, panose, 0))]).fixed_pitch);
    let mut post = vec![0; 32];
    post[15] = 1;
    assert!(read_traits(&[(b"post", post)]).fixed_pitch);
    assert!(!read_traits(&[(b"post", vec![0; 32])]).fixed_pitch);
    // Old fonts that count weight in hundreds, and nonsense values.
    assert_eq!(read_traits(&[(b"OS/2", os2(7, 5, [0; 10], 0))]).weight, 700);
    assert_eq!(read_traits(&[(b"OS/2", os2(0, 0, [0; 10], 0))]).weight, 400);
    assert_eq!(read_traits(&[(b"OS/2", os2(0, 77, [0; 10], 0))]).width, 5);
    // No OS/2 table: the italic bit of `head`.
    let mut head = vec![0; 54];
    head[45] = 2;
    assert!(read_traits(&[(b"head", head)]).italic);
}

#[test]
fn missing_or_short_tables_leave_an_upright_regular() {
    assert_eq!(read_traits(&[]), Traits::default());
    assert_eq!(
        read_traits(&[(b"OS/2", vec![0; 3]), (b"post", vec![1; 5])]),
        Traits::default()
    );
    // A table the directory places past the end of the file.
    let (mut file, directory) = font_with(&[(b"OS/2", os2(700, 5, [0; 10], 0))]);
    file.truncate(40);
    assert_eq!(
        Traits::read(&mut Cursor::new(file), &directory),
        Traits::default()
    );
}

fn fvar(axes: &[(&[u8; 4], u16, u16)]) -> Vec<u8> {
    let mut table = vec![0, 1, 0, 0, 0, 16, 0, 2];
    table.extend_from_slice(&(axes.len() as u16).to_be_bytes());
    table.extend_from_slice(&[0, 20, 0, 0, 0, 4]);
    for (tag, min, max) in axes {
        table.extend_from_slice(*tag);
        table.extend_from_slice(&[min.to_be_bytes(), [0, 0]].concat());
        table.extend_from_slice(&[400u16.to_be_bytes(), [0, 0]].concat());
        table.extend_from_slice(&[max.to_be_bytes(), [0, 0]].concat());
        table.extend_from_slice(&[0; 4]);
    }
    table
}

#[test]
fn a_variable_font_records_the_ends_of_its_weight_axis() {
    let table = fvar(&[(b"opsz", 14, 32), (b"wght", 100, 900)]);
    assert_eq!(weight_axis(&table), Some((100, 900)));
    assert_eq!(
        read_traits(&[(b"fvar", table.clone())]).weight_axis,
        Some((100, 900))
    );
    // No weight axis, or a table cut short.
    assert_eq!(weight_axis(&fvar(&[(b"wdth", 75, 100)])), None);
    assert_eq!(weight_axis(&table[..30]), None);
    assert_eq!(weight_axis(&[]), None);
    // The bundled fonts are static.
    assert_eq!(traits_of(noto_sans()).weight_axis, None);
}

#[test]
fn the_index_records_each_familys_weights_and_whether_it_is_fixed_pitch() {
    let dir = std::env::temp_dir().join(format!("reda-app-families-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("inter")).unwrap();
    std::fs::create_dir_all(dir.join("again")).unwrap();
    for file in [
        "Inter-Regular.ttf",
        "Inter-Medium.ttf",
        "Inter-SemiBold.ttf",
    ] {
        let bytes = chrome_font(&format!("inter/{file}"));
        std::fs::write(dir.join("inter").join(file), bytes).unwrap();
    }
    // The same face installed a second time counts once.
    std::fs::write(
        dir.join("again/Inter-Regular.ttf"),
        chrome_font("inter/Inter-Regular.ttf"),
    )
    .unwrap();
    let mono = chrome_font("jetbrains-mono/JetBrainsMonoNL-Regular.ttf");
    std::fs::write(dir.join("Mono.ttf"), &mono).unwrap();
    std::fs::write(dir.join("broken.ttf"), b"not a font").unwrap();

    let fonts = SystemFonts::scan(vec![dir.clone()]);
    let inter = fonts.family("INTER").expect("indexed");
    assert_eq!(inter.name, "Inter");
    assert_eq!(inter.weights(), [400, 500, 600]);
    assert_eq!(inter.faces.len(), 3);
    assert!(!inter.is_fixed_pitch());
    assert!(inter.has_latin_regular());
    let mono_family = fonts.family("JetBrains Mono NL").expect("indexed");
    assert_eq!(mono_family.weights(), [400]);
    assert!(mono_family.is_fixed_pitch());
    assert!(fonts.family("Comic Sans").is_none());

    // Every family, alphabetically, for the font lists.
    let names: Vec<String> = fonts.families().into_iter().map(|f| f.name).collect();
    let mut sorted = names.clone();
    sorted.sort_by_key(|n| n.to_lowercase());
    assert_eq!(names, sorted);
    assert!(names.contains(&"Inter".to_owned()));
    assert!(names.contains(&"JetBrains Mono NL".to_owned()));

    // A face's bytes are its file's.
    let face = &mono_family.faces[0];
    assert_eq!(&*fonts.read(face).expect("readable"), &mono[..]);
    std::fs::remove_dir_all(&dir).unwrap();
    assert!(fonts.read(face).is_none());
}

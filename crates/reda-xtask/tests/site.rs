//! Assembling the website: the site source, the current showcase and the released
//! versions' showcases become one static folder.

use std::fs;
use std::path::{Path, PathBuf};

use reda_xtask::site;

/// A scratch folder under the target dir, emptied first.
fn scratch(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR"))
        .join("site")
        .join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

/// A minimal site source: two pages with their markers, a stylesheet and a sub-folder.
fn source(root: &Path) -> PathBuf {
    let src = root.join("src");
    fs::create_dir_all(src.join("img")).unwrap();
    fs::write(
        src.join("index.html"),
        "<main><!-- reda:gallery --></main>\n",
    )
    .unwrap();
    fs::write(
        src.join("versions.html"),
        "<main><!-- reda:versions --></main>\n",
    )
    .unwrap();
    fs::write(src.join("style.css"), "body{}\n").unwrap();
    fs::write(src.join("img/logo.svg"), "<svg/>").unwrap();
    src
}

/// A showcase folder holding `scenes` (id, title) in both themes.
fn showcase(dir: &Path, extra: &str, scenes: &[(&str, &str)]) {
    fs::create_dir_all(dir).unwrap();
    let mut entries = Vec::new();
    for (id, title) in scenes {
        for theme in ["dark", "light"] {
            let file = format!("{id}-{theme}.png");
            fs::write(dir.join(&file), format!("png {id} {theme}")).unwrap();
            entries.push(format!(
                r#"{{"id":"{id}","title":"{title}","caption":"About {id}.","theme":"{theme}","file":"{file}"}}"#
            ));
        }
    }
    fs::write(
        dir.join("manifest.json"),
        format!(r#"{{{extra}"scenes":[{}]}}"#, entries.join(",")),
    )
    .unwrap();
}

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path.as_ref()).unwrap_or_else(|e| panic!("{}: {e}", path.as_ref().display()))
}

#[test]
fn the_home_page_shows_the_current_showcase_in_both_themes() {
    let root = scratch("home");
    let src = source(&root);
    let shots = root.join("shots");
    showcase(
        &shots,
        r#""commit":"abc1234","#,
        &[("start", "Start page"), ("pcb", "PCB layout")],
    );
    let out = root.join("out");

    site::build(&src, &shots, None, &out).unwrap();

    let index = read(out.join("index.html"));
    assert!(!index.contains("reda:gallery"), "{index}");
    let start = index.find("Start page").expect("the start scene");
    let pcb = index.find("PCB layout").expect("the pcb scene");
    assert!(start < pcb, "scenes keep the manifest's order");
    assert!(index.contains(r#"src="screenshots/latest/start-dark.png""#));
    assert!(index.contains(r#"src="screenshots/latest/start-light.png""#));
    assert!(index.contains("About pcb."));
    assert!(index.contains("abc1234"));
    assert_eq!(
        read(out.join("screenshots/latest/pcb-light.png")),
        "png pcb light"
    );
    assert!(out.join("screenshots/latest/manifest.json").is_file());
    // The rest of the source is copied as is.
    assert_eq!(read(out.join("style.css")), "body{}\n");
    assert_eq!(read(out.join("img/logo.svg")), "<svg/>");
    assert!(out.join(".nojekyll").is_file());
}

#[test]
fn the_versions_page_lists_released_versions_newest_first() {
    let root = scratch("versions");
    let src = source(&root);
    let shots = root.join("shots");
    showcase(&shots, "", &[("start", "Start page")]);
    let versions = root.join("versions");
    for v in ["0.1.0", "0.10.0", "0.9.1"] {
        showcase(&versions.join(v), "", &[("start", "Start page")]);
    }
    let out = root.join("out");

    let listed = site::build(&src, &shots, Some(&versions), &out).unwrap();

    assert_eq!(listed, ["0.10.0", "0.9.1", "0.1.0"]);
    let page = read(out.join("versions.html"));
    let at = |v: &str| page.find(&format!(r#"id="v{v}""#)).unwrap();
    assert!(at("0.10.0") < at("0.9.1") && at("0.9.1") < at("0.1.0"));
    assert!(page.contains(r#"src="screenshots/0.9.1/start-dark.png""#));
    // Each version's figures have their own ids.
    assert!(page.contains(r#"id="v0.9.1-start""#), "{page}");
    assert!(page.contains(r#"id="v0.1.0-start""#));
    assert_eq!(
        read(out.join("screenshots/0.1.0/start-dark.png")),
        "png start dark"
    );
}

#[test]
fn the_versions_page_says_when_no_version_is_released_yet() {
    let root = scratch("no-versions");
    let src = source(&root);
    let shots = root.join("shots");
    showcase(&shots, "", &[("start", "Start page")]);
    let out = root.join("out");

    // An empty folder is the same as none.
    fs::create_dir_all(root.join("versions")).unwrap();
    let listed = site::build(&src, &shots, Some(&root.join("versions")), &out).unwrap();

    assert!(listed.is_empty());
    let page = read(out.join("versions.html"));
    assert!(page.contains("No version"), "{page}");
}

#[test]
fn text_from_a_manifest_is_escaped() {
    let root = scratch("escape");
    let src = source(&root);
    let shots = root.join("shots");
    showcase(&shots, "", &[("start", "Start <b>&</b> more")]);
    let out = root.join("out");

    site::build(&src, &shots, None, &out).unwrap();

    let index = read(out.join("index.html"));
    assert!(
        index.contains("Start &lt;b&gt;&amp;&lt;/b&gt; more"),
        "{index}"
    );
}

#[test]
fn a_manifest_may_only_name_pngs_beside_it() {
    let root = scratch("escape-path");
    let src = source(&root);
    let shots = root.join("shots");
    fs::create_dir_all(&shots).unwrap();
    for file in ["../secret.png", "/etc/passwd.png", "shot.svg"] {
        fs::write(
            shots.join("manifest.json"),
            format!(
                r#"{{"scenes":[{{"id":"a","title":"A","caption":"","theme":"dark","file":"{file}"}}]}}"#
            ),
        )
        .unwrap();
        let err = site::build(&src, &shots, None, &root.join("out")).unwrap_err();
        assert!(err.contains(file), "{file}: {err}");
    }
}

#[test]
fn a_folder_that_is_not_an_earlier_build_is_never_replaced() {
    let root = scratch("not-a-site");
    let src = source(&root);
    let shots = root.join("shots");
    showcase(&shots, "", &[("start", "Start page")]);
    let out = root.join("out");
    fs::create_dir_all(&out).unwrap();
    fs::write(out.join("precious.txt"), "keep").unwrap();

    assert!(site::build(&src, &shots, None, &out).is_err());
    assert_eq!(read(out.join("precious.txt")), "keep");

    // An earlier build is replaced.
    fs::remove_file(out.join("precious.txt")).unwrap();
    site::build(&src, &shots, None, &out).unwrap();
    site::build(&src, &shots, None, &out).unwrap();
}

#[test]
fn a_page_without_its_marker_is_refused() {
    let root = scratch("marker");
    let src = source(&root);
    fs::write(src.join("index.html"), "<main></main>").unwrap();
    let shots = root.join("shots");
    showcase(&shots, "", &[("start", "Start page")]);

    let err = site::build(&src, &shots, None, &root.join("out")).unwrap_err();
    assert!(err.contains("reda:gallery"), "{err}");
}

//! Assembling REDA's website (`cargo xtask site`).
//!
//! The source in `site/` is plain HTML, CSS and a little JavaScript. Two pages carry a
//! marker that the assembly replaces with screenshot galleries:
//!
//! - `index.html`, `<!-- reda:gallery -->`: the showcase of the current build, which
//!   `cargo xtask showcase` drew. Its images go to `screenshots/latest/`.
//! - `versions.html`, `<!-- reda:versions -->`: one gallery per released version, newest
//!   first, from `<versions>/<version>/` (each release's `screenshots-<version>.tar.gz`,
//!   unpacked). Their images go to `screenshots/<version>/`.
//!
//! Every showcase folder holds the PNGs and the `manifest.json` that lists them; see
//! [`Manifest`].

use std::cmp::Ordering;
use std::fmt::Write as _;
use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

const GALLERY_MARKER: &str = "<!-- reda:gallery -->";
const VERSIONS_MARKER: &str = "<!-- reda:versions -->";
const RELEASES: &str = "https://github.com/openreda/reda/releases/tag";

/// One image in a showcase.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shot {
    /// The scene, shared by its themes, such as `pcb`.
    pub id: String,
    pub title: String,
    pub caption: String,
    /// `dark` or `light`.
    pub theme: String,
    /// The PNG's name, beside the manifest.
    pub file: String,
}

/// A showcase folder's `manifest.json`: `{"version"?, "commit"?, "scenes": [Shot…]}`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Manifest {
    /// The release version the images show, when they show one.
    pub version: Option<String>,
    /// The commit the images were drawn from, when known.
    pub commit: Option<String>,
    pub shots: Vec<Shot>,
}

impl Manifest {
    /// Parses a manifest. File names must be plain `.png` names, so a manifest can only
    /// point at images beside it.
    pub fn parse(text: &str) -> Result<Manifest, String> {
        let json: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
        let optional = |key: &str| json.get(key).and_then(Value::as_str).map(str::to_owned);
        let scenes = json
            .get("scenes")
            .and_then(Value::as_array)
            .ok_or("no `scenes` list")?;
        let mut shots = Vec::new();
        for (i, scene) in scenes.iter().enumerate() {
            let field = |key: &str| {
                scene
                    .get(key)
                    .and_then(Value::as_str)
                    .map(str::to_owned)
                    .ok_or(format!("scene {i} has no `{key}`"))
            };
            let shot = Shot {
                id: field("id")?,
                title: field("title")?,
                caption: field("caption")?,
                theme: field("theme")?,
                file: field("file")?,
            };
            let plain = !shot.file.is_empty()
                && !shot.file.starts_with('.')
                && shot
                    .file
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c));
            if !plain || !shot.file.ends_with(".png") {
                return Err(format!(
                    "scene {i}: {:?} is not a .png file name beside the manifest",
                    shot.file
                ));
            }
            shots.push(shot);
        }
        Ok(Manifest {
            version: optional("version"),
            commit: optional("commit"),
            shots,
        })
    }

    /// Reads `<dir>/manifest.json`.
    pub fn read(dir: &Path) -> Result<Manifest, String> {
        let path = dir.join("manifest.json");
        let text = fs::read_to_string(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        Manifest::parse(&text).map_err(|e| format!("{}: {e}", path.display()))
    }
}

/// Builds the website into `out` from the site `source`, the current showcase in
/// `screenshots` and the released versions' showcases under `versions`. Returns the
/// versions shown, newest first.
///
/// `out` is replaced, so it must be absent, empty or an earlier build (one with
/// `.nojekyll`).
pub fn build(
    source: &Path,
    screenshots: &Path,
    versions: Option<&Path>,
    out: &Path,
) -> Result<Vec<String>, String> {
    // Read and check everything before touching `out`.
    let current = Manifest::read(screenshots)?;
    let released = match versions {
        Some(dir) => released_versions(dir)?,
        None => Vec::new(),
    };
    let index = page(source, "index.html", GALLERY_MARKER)?;
    let versions_page = page(source, "versions.html", VERSIONS_MARKER)?;

    clear(out)?;
    copy_tree(source, out)?;
    fs::write(out.join(".nojekyll"), "").map_err(io(out))?;

    let latest = out.join("screenshots/latest");
    copy_showcase(screenshots, &current, &latest)?;
    let mut gallery = String::new();
    if let Some(commit) = &current.commit {
        writeln!(
            gallery,
            r#"<p class="shots-from">From commit <a href="https://github.com/openreda/reda/commit/{c}"><code>{c}</code></a>.</p>"#,
            c = escape(commit)
        )
        .unwrap();
    }
    gallery.push_str(&gallery_html(
        &current,
        "screenshots/latest",
        &latest,
        "shot",
    ));
    write_page(out, "index.html", &index, GALLERY_MARKER, &gallery)?;

    let mut listing = String::new();
    if released.is_empty() {
        listing.push_str(
            "<p class=\"empty\">No version has been released with screenshots yet. \
             Each release adds its own gallery here.</p>\n",
        );
    } else {
        listing.push_str("<nav class=\"version-list\" aria-label=\"Versions\"><ul>\n");
        for (version, _, _) in &released {
            let v = escape(version);
            writeln!(listing, r##"<li><a href="#v{v}">{v}</a></li>"##).unwrap();
        }
        listing.push_str("</ul></nav>\n");
        for (version, dir, manifest) in &released {
            let target = out.join("screenshots").join(version);
            copy_showcase(dir, manifest, &target)?;
            let v = escape(version);
            writeln!(
                listing,
                r#"<section class="version" id="v{v}" aria-labelledby="v{v}-title">
<h2 id="v{v}-title">REDA {v}</h2>
<p><a href="{RELEASES}/v{v}">Release notes and downloads for {v}</a></p>"#
            )
            .unwrap();
            listing.push_str(&gallery_html(
                manifest,
                &format!("screenshots/{version}"),
                &target,
                &format!("v{version}"),
            ));
            listing.push_str("</section>\n");
        }
    }
    write_page(
        out,
        "versions.html",
        &versions_page,
        VERSIONS_MARKER,
        &listing,
    )?;
    Ok(released.into_iter().map(|(v, _, _)| v).collect())
}

/// The versions under `dir` that hold a manifest, newest first, with their folders.
fn released_versions(dir: &Path) -> Result<Vec<(String, PathBuf, Manifest)>, String> {
    let mut found = Vec::new();
    if !dir.exists() {
        return Ok(found);
    }
    for entry in fs::read_dir(dir).map_err(io(dir))? {
        let path = entry.map_err(io(dir))?.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let version = name.strip_prefix('v').unwrap_or(name).to_owned();
        if !path.is_dir() || !is_version(&version) {
            continue;
        }
        if !path.join("manifest.json").is_file() {
            eprintln!(
                "xtask: skipping {}: it has no manifest.json",
                path.display()
            );
            continue;
        }
        let manifest = Manifest::read(&path)?;
        found.push((version, path, manifest));
    }
    found.sort_by(|a, b| compare_versions(&b.0, &a.0));
    Ok(found)
}

/// A version is dot-separated numbers, optionally with a `-suffix`; nothing else may name
/// a folder in the output.
fn is_version(s: &str) -> bool {
    let core = s.split_once('-').map_or(s, |(core, _)| core);
    !core.is_empty()
        && core
            .split('.')
            .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        && s.chars()
            .all(|c| c.is_ascii_alphanumeric() || "-.".contains(c))
}

/// Orders versions by their numbers, so 0.10.0 comes after 0.9.1; a pre-release comes
/// before its release.
pub fn compare_versions(a: &str, b: &str) -> Ordering {
    let split = |s: &str| -> (Vec<u64>, Option<String>) {
        let (core, pre) = match s.split_once('-') {
            Some((core, pre)) => (core, Some(pre.to_owned())),
            None => (s, None),
        };
        (
            core.split('.').map(|n| n.parse().unwrap_or(0)).collect(),
            pre,
        )
    };
    let (a, a_pre) = split(a);
    let (b, b_pre) = split(b);
    a.cmp(&b).then_with(|| match (a_pre, b_pre) {
        (None, None) => Ordering::Equal,
        (None, Some(_)) => Ordering::Greater,
        (Some(_), None) => Ordering::Less,
        (Some(x), Some(y)) => x.cmp(&y),
    })
}

/// The gallery for a showcase whose images are served from `url` (and copied to `dir`):
/// one figure per scene, in the manifest's order, holding an image per theme. Each figure's
/// id is `<prefix>-<scene>`, so galleries on one page never share ids.
fn gallery_html(manifest: &Manifest, url: &str, dir: &Path, prefix: &str) -> String {
    let mut ids: Vec<&str> = Vec::new();
    for shot in &manifest.shots {
        if !ids.contains(&shot.id.as_str()) {
            ids.push(&shot.id);
        }
    }
    let mut html = String::from("<div class=\"gallery\">\n");
    for id in ids {
        let shots: Vec<&Shot> = manifest.shots.iter().filter(|s| s.id == id).collect();
        let first = shots[0];
        writeln!(
            html,
            r#"<figure class="shot" id="{}-{}">"#,
            escape(prefix),
            escape(id)
        )
        .unwrap();
        for shot in &shots {
            let src = format!("{url}/{}", shot.file);
            let size = png_size(&dir.join(&shot.file))
                .map(|(w, h)| format!(r#" width="{w}" height="{h}""#))
                .unwrap_or_default();
            writeln!(
                html,
                r#"<a class="theme-{theme}" href="{src}"><img src="{src}" alt="{title}, {theme} theme" loading="lazy" decoding="async"{size}></a>"#,
                theme = escape(&shot.theme),
                src = escape(&src),
                title = escape(&shot.title),
            )
            .unwrap();
        }
        writeln!(
            html,
            "<figcaption><strong>{}</strong> {}</figcaption>\n</figure>",
            escape(&first.title),
            escape(&first.caption)
        )
        .unwrap();
    }
    html.push_str("</div>\n");
    html
}

/// A PNG's width and height, from its header.
fn png_size(path: &Path) -> Option<(u32, u32)> {
    let bytes = fs::read(path).ok()?;
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        return None;
    }
    let word = |at: usize| u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap());
    Some((word(16), word(20)))
}

/// Escapes text for HTML content and attribute values.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// Reads a source page and checks it holds `marker`.
fn page(source: &Path, name: &str, marker: &str) -> Result<String, String> {
    let path = source.join(name);
    let text = fs::read_to_string(&path).map_err(io(&path))?;
    if !text.contains(marker) {
        return Err(format!("{} has no {marker}", path.display()));
    }
    Ok(text)
}

fn write_page(out: &Path, name: &str, text: &str, marker: &str, with: &str) -> Result<(), String> {
    let path = out.join(name);
    fs::write(&path, text.replace(marker, with.trim_end())).map_err(io(&path))
}

/// Copies a showcase's manifest and the images it lists into `to`.
fn copy_showcase(from: &Path, manifest: &Manifest, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(io(to))?;
    for name in std::iter::once("manifest.json").chain(manifest.shots.iter().map(|s| &*s.file)) {
        let src = from.join(name);
        fs::copy(&src, to.join(name)).map_err(io(&src))?;
    }
    Ok(())
}

/// Empties `out`, refusing a non-empty folder that is not an earlier build.
fn clear(out: &Path) -> Result<(), String> {
    if out.exists() {
        let empty = fs::read_dir(out).map_err(io(out))?.next().is_none();
        if !empty && !out.join(".nojekyll").is_file() {
            return Err(format!(
                "{} is not empty and is not an earlier site build; refusing to replace it",
                out.display()
            ));
        }
        fs::remove_dir_all(out).map_err(io(out))?;
    }
    fs::create_dir_all(out).map_err(io(out))
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(io(to))?;
    for entry in fs::read_dir(from).map_err(io(from))? {
        let path = entry.map_err(io(from))?.path();
        let target = to.join(path.file_name().unwrap());
        if path.is_dir() {
            copy_tree(&path, &target)?;
        } else {
            fs::copy(&path, &target).map_err(io(&path))?;
        }
    }
    Ok(())
}

fn io(path: &Path) -> impl Fn(std::io::Error) -> String + '_ {
    move |e| format!("{}: {e}", path.display())
}

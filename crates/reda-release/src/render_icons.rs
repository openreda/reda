//! `reda-release icons` (binary only): renders `packaging/icons/src/*.svg` into the files the packages
//! use. The outputs are committed, so this runs only when an SVG changes:
//!
//!   cargo run -p reda-release -- icons packaging/icons
//!
//! - `app.svg` → `app/<N>x<N>.png` (the Linux app icon and cargo-packager's input),
//!   `app/app.icns` (macOS) and `app/app.ico` (Windows).
//! - every other SVG, named after its file extension (`redasch.svg`) → `documents/<ext>.icns`
//!   and `documents/<ext>.ico`. Linux installs the SVGs themselves as MIME-type icons.

use std::path::Path;
use std::process::Command;

use reda_release::{Png, icns, ico};

const APP_PNG_SIZES: [u32; 4] = [32, 128, 256, 512];
const ICO_SIZES: [u32; 6] = [16, 24, 32, 48, 64, 256];
const ICNS_SIZES: [u32; 6] = [16, 32, 64, 128, 256, 512];

pub fn render_all(dir: &Path) -> Result<(), String> {
    let src = dir.join("src");
    let mut svgs: Vec<_> = std::fs::read_dir(&src)
        .map_err(|e| format!("reading {}: {e}", src.display()))?
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "svg"))
        .collect();
    svgs.sort();
    for svg in svgs {
        let stem = svg
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or(format!("{} has no usable name", svg.display()))?;
        if stem == "app" {
            let out = dir.join("app");
            for size in APP_PNG_SIZES {
                write(
                    &out.join(format!("{size}x{size}.png")),
                    &render(&svg, size)?.bytes,
                )?;
            }
            write_icons(&svg, &out, "app")?;
        } else {
            write_icons(&svg, &dir.join("documents"), stem)?;
        }
    }
    Ok(())
}

fn write_icons(svg: &Path, out: &Path, name: &str) -> Result<(), String> {
    let ico_images = ICO_SIZES
        .map(|s| render(svg, s))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    write(&out.join(format!("{name}.ico")), &ico(&ico_images))?;
    let icns_images = ICNS_SIZES
        .map(|s| render(svg, s))
        .into_iter()
        .collect::<Result<Vec<_>, _>>()?;
    write(&out.join(format!("{name}.icns")), &icns(&icns_images)?)
}

fn render(svg: &Path, size: u32) -> Result<Png, String> {
    let out = Command::new("rsvg-convert")
        .args(["--width", &size.to_string(), "--height", &size.to_string()])
        .arg(svg)
        .output()
        .map_err(|e| format!("running rsvg-convert (is librsvg installed?): {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "rsvg-convert failed on {}: {}",
            svg.display(),
            String::from_utf8_lossy(&out.stderr)
        ));
    }
    Ok(Png {
        size,
        bytes: out.stdout,
    })
}

fn write(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("creating {}: {e}", parent.display()))?;
    }
    std::fs::write(path, bytes).map_err(|e| format!("writing {}: {e}", path.display()))
}

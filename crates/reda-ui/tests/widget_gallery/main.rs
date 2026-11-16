//! The widget gallery: every REDA widget in every state and the Glyph set, snapshotted in
//! light and dark, Comfortable and Compact
//! (docs/spec/v0.1/design-language.md#enforcement-and-tests).
//!
//! The gallery exists only here. A page is a function that draws sections of widgets
//! with their states forced side by side; `kit.rs` has the section card and the state
//! grid. To add a widget, add its rows to a page (or add a page and its four tests).
//!
//! Like the whole-screen snapshots, the images are drawn by Mesa's llvmpipe and compared
//! on Linux only:
//! `LIBGL_ALWAYS_SOFTWARE=1 WGPU_BACKEND=gl REDA_PNG_SNAPSHOTS=1 cargo test -p reda-ui
//! --test widget_gallery`, and add `UPDATE_SNAPSHOTS=1` to rewrite
//! `tests/snapshots/gallery_*.png`.
//!
//! `REDA_GALLERY_OUT=<folder>` also writes every image the run draws to that folder,
//! along with close-ups of single sections at twice the size (`approval_sheets`): what a
//! reviewer is shown.

mod bars;
mod controls;
mod data;
mod kit;
mod popups;
mod sheets;
mod shell;
mod title_bar;

use egui::{Theme, Vec2};
use egui_kittest::Harness;
use reda_ui::theme::{self, Density, Settings};

/// One of the four variants every page is drawn in.
#[derive(Clone, Copy)]
pub struct Variant {
    pub theme: Theme,
    pub density: Density,
}

impl Variant {
    pub const ALL: [Variant; 4] = [
        Variant::new(Theme::Light, Density::Comfortable),
        Variant::new(Theme::Light, Density::Compact),
        Variant::new(Theme::Dark, Density::Comfortable),
        Variant::new(Theme::Dark, Density::Compact),
    ];

    pub const fn new(theme: Theme, density: Density) -> Variant {
        Variant { theme, density }
    }

    /// `light_comfortable` and so on: the end of a snapshot's name.
    pub fn name(self) -> String {
        let theme = match self.theme {
            Theme::Light => "light",
            Theme::Dark => "dark",
        };
        let density = match self.density {
            Density::Comfortable => "comfortable",
            Density::Compact => "compact",
        };
        format!("{theme}_{density}")
    }
}

/// Why snapshots cannot be compared here, if they cannot.
fn skip_reason() -> Option<String> {
    if !cfg!(target_os = "linux") {
        return Some("gallery snapshots are compared on Linux only".into());
    }
    match reda_ui::engine::render::gpu::Headless::new() {
        Ok(h) if h.adapter().name.contains("llvmpipe") => None,
        Ok(h) => Some(format!(
            "the adapter is {:?}, not Mesa's llvmpipe that drew the snapshots",
            h.adapter().name
        )),
        Err(e) => Some(e.to_string()),
    }
}

/// Whether images can be drawn here; says why not, and fails where CI asks for them.
fn can_draw() -> bool {
    let Some(why) = skip_reason() else {
        return true;
    };
    if std::env::var_os("REDA_PNG_SNAPSHOTS").is_some() {
        panic!("REDA_PNG_SNAPSHOTS is set but snapshots cannot run: {why}");
    }
    eprintln!("skipping snapshot: {why}");
    false
}

/// A harness that draws `page` in `variant` through wgpu, on the base surface, at `size`
/// points and `scale` pixels per point.
pub fn harness(
    variant: Variant,
    size: Vec2,
    scale: f32,
    mut page: impl FnMut(&mut egui::Ui) + 'static,
) -> Harness<'static> {
    let render_state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    let mut installed = false;
    Harness::builder()
        .with_size(size)
        .with_pixels_per_point(scale)
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            render_state,
        ))
        .build_ui(move |ui| {
            if !installed {
                install(ui.ctx(), variant);
                installed = true;
            }
            kit::page(ui, |ui| page(ui));
        })
}

/// Installs the theme, the Density and the icons a page needs on a test context.
pub fn install(ctx: &egui::Context, variant: Variant) {
    theme::install(ctx, variant.theme.into());
    theme::set(
        ctx,
        &Settings {
            density: variant.density,
            ..Settings::default()
        },
    );
    reda_ui::icons::install_loaders(ctx);
}

/// Writes what `h` shows to `REDA_GALLERY_OUT/<name>.png`, when that is set.
pub fn keep(h: &mut Harness<'_>, name: &str) {
    let Some(folder) = std::env::var_os("REDA_GALLERY_OUT") else {
        return;
    };
    let folder = std::path::PathBuf::from(folder);
    std::fs::create_dir_all(&folder).unwrap();
    let image = h.render().expect("the gallery renders");
    image.save(folder.join(format!("{name}.png"))).unwrap();
}

/// Draws the page `name` in `variant` and compares it with
/// `tests/snapshots/gallery_<name>_<variant>.png`.
fn snapshot(name: &str, size: Vec2, variant: Variant, page: fn(&mut egui::Ui)) {
    if !can_draw() {
        return;
    }
    let mut h = harness(variant, size, 1.0, page);
    h.run();
    let name = format!("gallery_{name}_{}", variant.name());
    keep(&mut h, &name);
    h.snapshot(name);
}

/// The four tests of one page.
macro_rules! page_snapshots {
    ($page:literal, $size:expr, $draw:path: $($test:ident => $theme:ident, $density:ident;)+) => {
        $(
            #[test]
            fn $test() {
                let variant = Variant::new(Theme::$theme, Density::$density);
                snapshot($page, $size, variant, $draw);
            }
        )+
    };
}

page_snapshots!("controls", controls::SIZE, controls::page:
    controls_light_comfortable => Light, Comfortable;
    controls_light_compact => Light, Compact;
    controls_dark_comfortable => Dark, Comfortable;
    controls_dark_compact => Dark, Compact;
);

page_snapshots!("popups", popups::SIZE, popups::page:
    popups_light_comfortable => Light, Comfortable;
    popups_light_compact => Light, Compact;
    popups_dark_comfortable => Dark, Comfortable;
    popups_dark_compact => Dark, Compact;
);

page_snapshots!("data", data::SIZE, data::page:
    data_light_comfortable => Light, Comfortable;
    data_light_compact => Light, Compact;
    data_dark_comfortable => Dark, Comfortable;
    data_dark_compact => Dark, Compact;
);

page_snapshots!("shell", shell::SIZE, shell::page:
    shell_light_comfortable => Light, Comfortable;
    shell_light_compact => Light, Compact;
    shell_dark_comfortable => Dark, Comfortable;
    shell_dark_compact => Dark, Compact;
);

page_snapshots!("title_bar", title_bar::SIZE, title_bar::page:
    title_bar_light_comfortable => Light, Comfortable;
    title_bar_light_compact => Light, Compact;
    title_bar_dark_comfortable => Dark, Comfortable;
    title_bar_dark_compact => Dark, Compact;
);

/// Close-ups for a reviewer: written only when `REDA_GALLERY_OUT` is set, compared with
/// nothing.
#[test]
fn approval_sheets() {
    if std::env::var_os("REDA_GALLERY_OUT").is_none() || !can_draw() {
        return;
    }
    sheets::write_all();
}

page_snapshots!("bars", bars::SIZE, bars::page:
    bars_light_comfortable => Light, Comfortable;
    bars_light_compact => Light, Compact;
    bars_dark_comfortable => Dark, Comfortable;
    bars_dark_compact => Dark, Compact;
);

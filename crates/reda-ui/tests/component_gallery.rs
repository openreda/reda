//! PROTOTYPE (issue #276): draws every gallery page in light/dark × Comfortable/Compact.
//! `REDA_GALLERY_OUT=<dir> cargo test -p reda-ui --test component_gallery -- --nocapture`

use std::path::PathBuf;

use egui_kittest::Harness;
use reda_ui::proto;

fn draw(page: usize, theme: egui::Theme, name: &str, out: &PathBuf) {
    let render_state = egui_kittest::wgpu::create_render_state(
        egui_kittest::wgpu::default_wgpu_setup(),
        egui_wgpu::RendererOptions::PREDICTABLE,
    );
    proto::set_page(page);
    let mut h = Harness::builder()
        .with_size([1600.0, 1000.0])
        .with_accessibility_check(false)
        .renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            render_state,
        ))
        .build_ui_state(
            move |ui, installed: &mut bool| {
                if !*installed {
                    let pref = match theme {
                        egui::Theme::Dark => egui::ThemePreference::Dark,
                        egui::Theme::Light => egui::ThemePreference::Light,
                    };
                    reda_ui::theme::install(ui.ctx(), pref);
                    reda_ui::icons::install_loaders(ui.ctx());
                    proto::install_fonts(ui.ctx());
                    *installed = true;
                }
                proto::pages::gallery(ui);
            },
            false,
        );
    h.run();
    h.run();
    let image = h.render().unwrap();
    image.save(out.join(name)).unwrap();
    eprintln!("drew {name}");
}

#[test]
fn contrast() {
    let (report, failures) = proto::contrast_report();
    println!("{report}");
    assert_eq!(failures, 0, "contrast rules fail");
}

#[test]
fn gallery() {
    let Some(out) = std::env::var_os("REDA_GALLERY_OUT").map(PathBuf::from) else {
        eprintln!("REDA_GALLERY_OUT is not set; nothing drawn");
        return;
    };
    std::fs::create_dir_all(&out).unwrap();
    let only = std::env::var("REDA_GALLERY_PAGES").ok();
    let slugs = ["controls", "popups", "window", "panels", "dialogs", "colour", "type"];
    for (page, slug) in slugs.into_iter().enumerate() {
        if only.as_ref().is_some_and(|o| !o.contains(&(page + 1).to_string())) {
            continue;
        }
        for (theme, tname) in [(egui::Theme::Light, "light"), (egui::Theme::Dark, "dark")] {
            for (compact, dname) in [(false, "comfortable"), (true, "compact")] {
                proto::set_compact(compact);
                proto::set_text_size(13);
                let name = format!("{:02}-{slug}-{tname}-{dname}.png", page + 1);
                draw(page, theme, &name, &out);
            }
        }
    }
    if only.is_none() {
        // The ends of the text-size range, and a regular-only face that is not Inter.
        proto::set_compact(false);
        for size in [11u8, 16] {
            proto::set_text_size(size);
            for (page, slug) in [(0, "controls"), (2, "window"), (3, "panels")] {
                let name = format!("text{size}-{:02}-{slug}-light-comfortable.png", page + 1);
                draw(page, egui::Theme::Light, &name, &out);
            }
        }
        proto::set_text_size(13);
        let ctx = egui::Context::default();
        for (face, fname) in [(1, "inter-regular-only"), (2, "dejavu")] {
            proto::set_face(&ctx, face);
            for (page, slug) in [(0, "controls"), (2, "window"), (3, "panels")] {
                let name = format!("face-{fname}-{:02}-{slug}-light-comfortable.png", page + 1);
                draw(page, egui::Theme::Light, &name, &out);
            }
        }
        proto::set_face(&ctx, 0);
    }
}

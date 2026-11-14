//! PROTOTYPE (issue #282): draws the Motion page at rest and partway through Play all,
//! in each profile.
//! `REDA_GALLERY_OUT=<dir> cargo test -p reda-ui --test motion_gallery -- --nocapture`

use std::path::PathBuf;

use egui_kittest::Harness;
use reda_ui::proto;

#[test]
fn motion() {
    let Some(out) = std::env::var_os("REDA_GALLERY_OUT").map(PathBuf::from) else {
        eprintln!("REDA_GALLERY_OUT is not set; nothing drawn");
        return;
    };
    std::fs::create_dir_all(&out).unwrap();
    proto::set_page(7);
    for (theme, tname) in [(egui::Theme::Light, "light"), (egui::Theme::Dark, "dark")] {
        for (profile, pname) in ["a-still", "b-crisp", "c-fluid"].into_iter().enumerate() {
            if theme == egui::Theme::Dark && profile != 1 {
                continue;
            }
            proto::anim::set_profile(profile);
            let render_state = egui_kittest::wgpu::create_render_state(
                egui_kittest::wgpu::default_wgpu_setup(),
                egui_wgpu::RendererOptions::PREDICTABLE,
            );
            let mut h = Harness::builder()
                .with_size([1600.0, 1000.0])
                .with_step_dt(1.0 / 120.0)
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
            h.run_steps(4);
            let mut save = |h: &mut Harness<'_, bool>, name: &str| {
                let name = format!("motion-{pname}-{tname}-{name}.png");
                h.render().unwrap().save(out.join(&name)).unwrap();
                eprintln!("drew {name}");
            };
            save(&mut h, "0-before");
            // Play all: one step applies it, then 60 ms and 120 ms into the transitions.
            proto::anim::play();
            h.run_steps(1);
            h.run_steps(7);
            save(&mut h, "1-at-060ms");
            h.run_steps(7);
            save(&mut h, "2-at-120ms");
            h.run_steps(120);
            save(&mut h, "3-after");
            proto::anim::play();
            h.run_steps(120);
            if profile == 1 && theme == egui::Theme::Light {
                // A tooltip: the pointer rests on the Move Tool past the wait.
                h.event(egui::Event::PointerMoved(egui::pos2(470.0, 302.0)));
                h.run_steps(90);
                save(&mut h, "4-tooltip");
                h.event(egui::Event::PointerMoved(egui::pos2(10.0, 10.0)));
                h.run_steps(30);
            }
        }
    }
}

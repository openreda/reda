//! The Start page and the window's own dialogs fit their text: in a 1280 × 800 window, in
//! Comfortable and Compact and in light and dark, no text is cut short with an ellipsis,
//! none runs out of the field, row or button that clips it, and no label breaks over
//! lines. Sentences that are meant to wrap (an intro, a note) may.
//!
//! The screens: the Start page with a long and a missing recent Project, New Project
//! (with the rectangle board), About and its Licences, both results of Check for Updates,
//! the closing prompt for a tab and the quit list, the recovery prompt, the Crash Report
//! offer, and a blocked job run's report. The job run's progress modal is up for as long
//! as a run takes, too short to look at here.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use egui::Rect;
use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable as _};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::crash::{self, Report};
use reda_ui::engine::command::UserDirs;
use reda_ui::engine::{Engine, state};
use reda_ui::help::{self, Release, ReleaseSource};
use reda_ui::shell::Shell;
use serde_json::json;

/// A laptop's window.
const WINDOW: [f32; 2] = [1280.0, 800.0];
/// A recent Project with a long name.
const LONG: &str = "Four Channel Class D Amplifier Rev C";

struct Fixed(&'static str);

impl ReleaseSource for Fixed {
    fn latest(&self) -> Result<Release, String> {
        Ok(Release {
            version: self.0.to_owned(),
            url: format!("https://github.com/openreda/reda/releases/tag/v{}", self.0),
        })
    }
}

/// The look a run is drawn in.
#[derive(Clone, Copy)]
struct Look {
    theme: &'static str,
    density: &'static str,
}

fn engine_in(home: &Path, latest: &'static str) -> Engine {
    let mut engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    help::configure(&mut engine, "0.1.3", Some(Arc::new(Fixed(latest))));
    engine
}

/// A shell in `look`, set through the Preferences as the App applies them.
fn shell_in(home: &Path, look: Look) -> Shell {
    shell_with(home, look, "0.1.3")
}

fn shell_with(home: &Path, look: Look, latest: &'static str) -> Shell {
    let mut shell = Shell::new(engine_in(home, latest));
    for (key, value) in [
        ("general.theme", look.theme),
        ("general.density", look.density),
    ] {
        shell
            .run("app.preferences.set", json!({"key": key, "value": value}))
            .unwrap();
    }
    shell
}

fn harness(shell: Shell) -> Harness<'static, App> {
    let mut builder = Harness::builder().with_size(WINDOW).with_max_steps(12);
    if pictures().is_some() {
        let render_state = egui_kittest::wgpu::create_render_state(
            egui_kittest::wgpu::default_wgpu_setup(),
            egui_wgpu::RendererOptions::PREDICTABLE,
        );
        reda_ui::install_canvas(&render_state, 1);
        builder = builder.renderer(egui_kittest::wgpu::WgpuTestRenderer::from_render_state(
            render_state,
        ));
    }
    let mut h = builder.build_ui_state(|ui, app: &mut App| app.ui(ui), App::new(shell));
    settle(&mut h);
    h
}

/// Where to write a picture of each screen: `REDA_GALLERY_OUT`, when set (with the
/// llvmpipe environment of the snapshots).
fn pictures() -> Option<PathBuf> {
    std::env::var_os("REDA_GALLERY_OUT").map(PathBuf::from)
}

/// Writes what `h` shows to `REDA_GALLERY_OUT/fit_<screen>_<theme>_<density>.png`.
fn keep(h: &mut Harness<'static, App>, look: Look, screen: &str) {
    let Some(folder) = pictures() else {
        return;
    };
    std::fs::create_dir_all(&folder).unwrap();
    let name = format!("fit_{screen}_{}_{}.png", look.theme, look.density).replace(' ', "_");
    let image = h.render().expect("the window renders");
    image.save(folder.join(name)).unwrap();
}

/// Dialogs grow in and size themselves to their content over a few frames.
fn settle(h: &mut Harness<'static, App>) {
    h.run_steps(12);
}

fn click(h: &mut Harness<'static, App>, label: &str) {
    h.get_all_by_label(label).last().unwrap().click();
    settle(h);
}

/// The rect of the dialog titled `title`.
fn dialog(h: &Harness<'static, App>, title: &str) -> Rect {
    h.get_by_role_and_label(Role::Window, title).rect()
}

/// Draws frames until `label` shows: some screens wait on another thread.
fn wait_for(h: &mut Harness<'static, App>, label: &str) {
    let start = Instant::now();
    while h.query_by_label_contains(label).is_none() {
        assert!(start.elapsed().as_secs() < 20, "{label:?} never showed");
        std::thread::sleep(Duration::from_millis(10));
        h.step();
    }
    settle(h);
}

/// What is wrong with the text drawn wholly inside one of `regions`, one line per text:
/// cut short with an ellipsis, out of its clip sideways, or a label broken over lines.
/// Text that pokes out of a region belongs to what is behind it; text a scroll area cuts
/// at its foot is scrolled, not clipped.
///
/// A file path can be any length, so no width holds every one: the Start page cuts a
/// recent Project's folder short (its full path is the name's tooltip) and the Crash
/// Report offer breaks its path over lines. Texts starting with one of `paths` may do
/// that.
fn misfits(h: &Harness<'static, App>, regions: &[Rect], paths: &[&Path]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen = 0;
    for clipped in &h.output().shapes {
        let mut shapes = vec![&clipped.shape];
        while let Some(shape) = shapes.pop() {
            let text = match shape {
                egui::Shape::Vec(inner) => {
                    shapes.extend(inner);
                    continue;
                }
                egui::Shape::Text(text) => text,
                _ => continue,
            };
            let rect = text.galley.rect.translate(text.pos.to_vec2());
            let clip: Rect = clipped.clip_rect;
            let said = text.galley.text();
            if said.trim().is_empty()
                || !clip.intersects(rect)
                || !regions.iter().any(|r| r.expand(0.5).contains_rect(rect))
            {
                continue;
            }
            seen += 1;
            // The app shows a folder as the OS resolves it, which on macOS (`/private/var/..`)
            // and Windows (`\\?\C:\..`) is not how the temporary folder was asked for.
            if paths.iter().any(|p| {
                said.starts_with(&*p.display().to_string())
                    || std::fs::canonicalize(p)
                        .is_ok_and(|real| said.starts_with(&*real.display().to_string()))
            }) {
                continue;
            }
            if text.galley.elided {
                out.push(format!("cut short: {said:?}"));
            }
            if rect.right() > clip.right() + 0.5 || rect.left() < clip.left() - 0.5 {
                out.push(format!(
                    "runs out sideways by {}: {said:?}",
                    (rect.right() - clip.right()).max(clip.left() - rect.left())
                ));
            }
            let sentence =
                said.trim_end().ends_with(['.', '?', ':', '…']) || said.split(' ').count() > 8;
            if text.galley.rows.len() > 1 && !said.contains('\n') && !sentence {
                out.push(format!("wraps: {said:?}"));
            }
        }
    }
    if seen == 0 {
        out.push("no text to look at: the screen did not open".to_owned());
    }
    out
}

fn report(out: &mut Vec<String>, look: Look, screen: &str, found: Vec<String>) {
    out.extend(
        found
            .into_iter()
            .map(|m| format!("{} {}, {screen}: {m}", look.theme, look.density)),
    );
}

/// Creates a Project named `name` in `folder`; returns its `.reda` file.
fn make_project(folder: &Path, name: &str) -> PathBuf {
    let mut engine = Engine::builder()
        .user_dirs(None)
        .starter_library(None)
        .build();
    engine
        .run("project.new", json!({"path": folder, "name": name}))
        .unwrap();
    std::fs::canonicalize(folder.join(name).join(format!("{name}.reda"))).unwrap()
}

/// The Start page, with a recent Project of a long name and one that is gone, then New
/// Project with the rectangle board.
fn start_and_new_project(look: Look, out: &mut Vec<String>) {
    let home = TempDir::new("fit-start-home");
    let work = TempDir::new("fit-start-work");
    let long = make_project(work.path(), LONG);
    let gone = work.path().join("Gone/Gone.reda");
    state::update(&UserDirs::under(home.path()), |s| {
        s.recent_projects = vec![long.clone(), gone.clone()];
    })
    .unwrap();
    let mut h = harness(shell_in(home.path(), look));
    let labels = [
        "Welcome to REDA".to_owned(),
        "Recent Projects".to_owned(),
        LONG.to_owned(),
        format!("Remove {LONG} from recent Projects"),
        "Remove Gone from recent Projects".to_owned(),
    ];
    let mut page = h.get_by_label("Welcome to REDA").rect();
    for label in &labels {
        page = page.union(h.get_all_by_label(label).last().unwrap().rect());
    }
    keep(&mut h, look, "start page");
    report(
        out,
        look,
        "start page",
        misfits(&h, &[page], &[work.path()]),
    );

    click(&mut h, "New Project…");
    click(&mut h, "Rectangle");
    keep(&mut h, look, "new project");
    report(
        out,
        look,
        "new project",
        misfits(&h, &[dialog(&h, "New Project")], &[]),
    );
}

/// About on both pages, then both answers of Check for Updates.
fn help_dialogs(look: Look, out: &mut Vec<String>) {
    for (latest, answer) in [
        ("0.2.0", "REDA 0.2.0 is available. This is 0.1.3."),
        ("0.1.3", "REDA 0.1.3 is the newest release."),
    ] {
        let home = TempDir::new("fit-help-home");
        let mut h = harness(shell_with(home.path(), look, latest));
        if latest == "0.2.0" {
            h.state_mut()
                .shell_mut()
                .run("app.dialog.open", json!({"dialog": "about"}))
                .unwrap();
            settle(&mut h);
            keep(&mut h, look, "about");
            report(
                out,
                look,
                "about",
                misfits(&h, &[dialog(&h, "About REDA")], &[]),
            );
            click(&mut h, "Licences");
            keep(&mut h, look, "licences");
            report(
                out,
                look,
                "licences",
                misfits(&h, &[dialog(&h, "About REDA")], &[]),
            );
            click(&mut h, "Close");
        }
        h.state_mut()
            .shell_mut()
            .run("app.update.check", json!({}))
            .unwrap();
        wait_for(&mut h, answer);
        let screen = format!("check for updates ({latest})");
        keep(&mut h, look, &screen);
        report(
            out,
            look,
            &screen,
            misfits(&h, &[dialog(&h, "Check for Updates")], &[]),
        );
    }
}

/// The closing prompt for one tab, then the quit list.
fn closing(look: Look, out: &mut Vec<String>) {
    let home = TempDir::new("fit-closing-home");
    let work = TempDir::new("fit-closing-work");
    let mut shell = shell_in(home.path(), look);
    shell.new_project(work.path(), LONG).unwrap();
    let sheet = shell
        .engine()
        .project()
        .unwrap()
        .documents()
        .next()
        .unwrap()
        .uuid;
    shell
        .run(
            "schematic.wire.add",
            json!({"document": sheet, "points": [{"x": "0mm", "y": "0mm"}, {"x": "5mm", "y": "0mm"}]}),
        )
        .unwrap();
    shell
        .run("project.properties.set", json!({"title": "Amp"}))
        .unwrap();
    let mut h = harness(shell);
    h.get_by_role_and_label(Role::Button, "Close Sheet 1")
        .click();
    settle(&mut h);
    keep(&mut h, look, "closing a tab");
    report(
        out,
        look,
        "closing a tab",
        misfits(&h, &[dialog(&h, "Unsaved Changes")], &[]),
    );
    h.get_all_by_role_and_label(Role::Button, "Cancel")
        .last()
        .unwrap()
        .click();
    settle(&mut h);
    click(&mut h, "File");
    click(&mut h, "Quit");
    keep(&mut h, look, "quitting");
    report(
        out,
        look,
        "quitting",
        misfits(&h, &[dialog(&h, "Unsaved Changes")], &[]),
    );
}

/// The recovery prompt after an unclean exit, then the Crash Report offer.
fn crash_dialogs(look: Look, out: &mut Vec<String>) {
    let home = TempDir::new("fit-recovery-home");
    let work = TempDir::new("fit-recovery-work");
    let mut shell = shell_in(home.path(), look);
    shell.new_project(work.path(), LONG).unwrap();
    let t0 = Instant::now();
    shell.tick_recovery(t0);
    shell
        .run("project.properties.set", json!({"title": "Amp"}))
        .unwrap();
    shell.tick_recovery(t0 + Duration::from_secs(1));
    shell.tick_recovery(t0 + Duration::from_secs(10));
    shell.flush_recovery();
    drop(shell);
    let mut shell = shell_in(home.path(), look);
    shell.open_project(&work.path().join(LONG)).unwrap();
    let mut h = harness(shell);
    keep(&mut h, look, "recovery");
    report(
        out,
        look,
        "recovery",
        misfits(&h, &[dialog(&h, "Recover Unsaved Work")], &[]),
    );

    let home = TempDir::new("fit-crash-home");
    let dirs = UserDirs::under(home.path());
    let crashed = Report {
        version: "0.1.0".into(),
        os: crash::os(),
        message: "boom".into(),
        location: "a.rs:1:1".into(),
        backtrace: String::new(),
        journal: vec![],
    };
    crash::write(
        &crash::reports_dir(&dirs),
        &crashed,
        std::time::SystemTime::now(),
    )
    .unwrap();
    let mut h = harness(shell_in(home.path(), look));
    keep(&mut h, look, "crash report");
    report(
        out,
        look,
        "crash report",
        misfits(
            &h,
            &[dialog(&h, "REDA Closed Unexpectedly")],
            &[home.path()],
        ),
    );
}

/// A job run that is blocked (the board has no outline): its report.
fn job_report(look: Look, out: &mut Vec<String>) {
    let home = TempDir::new("fit-job-home");
    let work = TempDir::new("fit-job-work");
    let file = make_project(work.path(), "Jobs");
    let mut shell = shell_in(home.path(), look);
    shell.open_project(&file).unwrap();
    let job = shell
        .engine()
        .project()
        .unwrap()
        .output_jobs()
        .next()
        .unwrap()
        .uuid;
    let mut h = harness(shell);
    h.state_mut().open_output_job(job);
    settle(&mut h);
    h.get_by_role_and_label(Role::Button, "Run").click();
    wait_for(&mut h, "did not run");
    let window = h
        .get_all_by_role(Role::Window)
        .find(|n| {
            n.accesskit_node()
                .label()
                .is_some_and(|l| l.contains("did not run"))
        })
        .expect("the run report")
        .rect();
    keep(&mut h, look, "job run report");
    report(out, look, "job run report", misfits(&h, &[window], &[]));
}

fn screens(look: Look) -> Vec<String> {
    let mut out = Vec::new();
    start_and_new_project(look, &mut out);
    help_dialogs(look, &mut out);
    closing(look, &mut out);
    crash_dialogs(look, &mut out);
    job_report(look, &mut out);
    out
}

#[test]
fn the_start_page_and_the_window_dialogs_fit_their_text_in_both_densities_and_themes() {
    let mut out = Vec::new();
    for theme in ["light", "dark"] {
        for density in ["comfortable", "compact"] {
            out.extend(screens(Look { theme, density }));
        }
    }
    assert!(out.is_empty(), "{}", out.join("\n"));
}

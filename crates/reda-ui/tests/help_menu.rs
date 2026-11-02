//! The Help menu in the window: About with the built-in Licences, and Check for Updates
//! showing what `app.update.check` found. The release source is a fake; nothing here
//! reaches the network.

use std::path::Path;
use std::sync::Arc;

use egui::accesskit::Role;
use egui_kittest::Harness;
use egui_kittest::kittest::{NodeT, Queryable};
use reda_testkit::TempDir;
use reda_ui::App;
use reda_ui::engine::Engine;
use reda_ui::engine::command::UserDirs;
use reda_ui::help::{self, Release, ReleaseSource};
use reda_ui::shell::Shell;

struct Fixed(&'static str);

impl ReleaseSource for Fixed {
    fn latest(&self) -> Result<Release, String> {
        Ok(Release {
            version: self.0.to_owned(),
            url: format!("https://github.com/openreda/reda/releases/tag/v{}", self.0),
        })
    }
}

fn harness(home: &Path, latest: &'static str) -> Harness<'static, App> {
    let mut engine = Engine::builder()
        .user_dirs(Some(UserDirs::under(home)))
        .starter_library(None)
        .headless(false)
        .build();
    help::configure(&mut engine, "0.1.3", Some(Arc::new(Fixed(latest))));
    let app = App::new(Shell::new(engine));
    Harness::builder()
        .with_size([1000.0, 640.0])
        .build_ui_state(|ui, app: &mut App| app.ui(ui), app)
}

fn open_help_item(h: &mut Harness<'_, App>, item: &str) {
    h.get_by_role_and_label(Role::Button, "Help").click();
    h.run();
    h.get_by_label(item).click();
    h.run();
}

fn journal_ids(h: &Harness<'_, App>) -> Vec<String> {
    h.state()
        .shell()
        .engine()
        .journal()
        .iter()
        .map(|e| e.id.clone())
        .collect()
}

#[test]
fn about_shows_the_version_and_the_licences_built_into_the_binary() {
    let home = TempDir::new("help-home");
    let mut h = harness(home.path(), "0.1.3");
    h.run();
    open_help_item(&mut h, "About REDA");
    assert_eq!(journal_ids(&h), ["app.dialog.open:about"]);
    h.get_by_label("Version 0.1.3");

    h.get_by_label("Licences").click();
    h.run();
    let notices = h.get_by_role_and_label(Role::MultilineTextInput, "Third-party notices");
    let shown = notices.accesskit_node().value().unwrap_or_default();
    assert_eq!(shown, help::NOTICES, "the notices come from the binary");

    h.get_by_label("Close").click();
    h.run();
    assert!(h.query_by_label("Version 0.1.3").is_none());
}

#[test]
fn check_for_updates_links_to_a_newer_release() {
    let home = TempDir::new("help-home");
    let mut h = harness(home.path(), "0.2.0");
    h.run();
    open_help_item(&mut h, "Check for Updates…");
    assert_eq!(journal_ids(&h), ["app.update.check"]);
    h.get_by_label("REDA 0.2.0 is available. This is 0.1.3.");
    let link = h.get_by_role_and_label(Role::Link, "Open the release page");
    assert!(!link.accesskit_node().is_disabled());
}

#[test]
fn check_for_updates_says_when_reda_is_up_to_date() {
    let home = TempDir::new("help-home");
    let mut h = harness(home.path(), "0.1.3");
    h.run();
    open_help_item(&mut h, "Check for Updates…");
    h.get_by_label("REDA 0.1.3 is the newest release.");
    assert!(h.query_by_role(Role::Link).is_none());
    h.get_by_label("Close").click();
    h.run();
    assert!(
        h.query_by_label("REDA 0.1.3 is the newest release.")
            .is_none()
    );
}

#[test]
fn help_stays_the_last_menu_for_every_document_type() {
    use reda_ui::bars::{self, DocumentType};
    for document in [
        None,
        Some(DocumentType::Schematic),
        Some(DocumentType::Pcb),
        Some(DocumentType::Symbol),
        Some(DocumentType::Footprint),
    ] {
        let menus = &bars::for_document(document).menus;
        assert_eq!(menus.last().unwrap().id, "help", "{document:?}");
    }
}

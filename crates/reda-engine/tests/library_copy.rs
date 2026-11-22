//! `library.element.copy`: an element copied into a writable Library gets a new UUID and,
//! when its name is taken there, `_1`, `_2`…; a copied Part keeps referencing its
//! original Symbol and Footprints, across Libraries.

use std::path::Path;

use reda_engine::Engine;
use reda_engine::command::{ErrorCode, FixedClock, SeededIds, UserDirs};
use reda_testkit::{TempDir, TestEngine, starter_library};
use serde_json::{Value, json};

const NPN: &str = "614301c4-68c2-4d22-998f-5c9f4d2dbd19";
const NPN_SYMBOL: &str = "4b09f27b-e1a9-46d0-af71-1fa9775f2c85";
const SOT23: &str = "43ca8637-7f8d-4597-866a-d91d359dd6bb";

/// An engine with the Starter Library and one empty user Library, `Mine`; returns the
/// user Library's UUID too.
fn engine(dir: &Path) -> (TestEngine, String) {
    let mut engine = TestEngine(
        Engine::builder()
            .ids(SeededIds::new(0))
            .clock(FixedClock::EPOCH)
            .user_dirs(Some(UserDirs::under(&dir.join("home"))))
            .starter_library(Some(starter_library()))
            .build(),
    );
    let out = engine.run_ok(
        "library.create",
        json!({"path": dir.join("mine"), "name": "Mine", "register": "user"}),
    );
    let library = out["library"].as_str().unwrap().to_owned();
    (engine, library)
}

fn element(engine: &mut TestEngine, uuid: &Value) -> Value {
    engine.run_ok("library.element.open", json!({"element": uuid}))
}

#[test]
fn a_copy_gets_a_new_uuid_and_a_free_name_in_the_target_library() {
    let dir = TempDir::new("element-copy");
    let (mut engine, mine) = engine(dir.path());

    let first = engine.run_ok(
        "library.element.copy",
        json!({"element": NPN_SYMBOL, "library": mine}),
    );
    assert_ne!(first["element"], NPN_SYMBOL);
    assert_eq!(first["kind"], "symbol");
    assert_eq!(first["name"], "NPN");
    assert_eq!(
        Path::new(first["path"].as_str().unwrap()),
        dir.path().join("mine/symbols/NPN.redasym")
    );
    let second = engine.run_ok(
        "library.element.copy",
        json!({"element": NPN_SYMBOL, "library": mine}),
    );
    assert_eq!(second["name"], "NPN_1");
    // Copying the copy, as Duplicate does, takes the next free suffix too.
    let third = engine.run_ok(
        "library.element.copy",
        json!({"element": first["element"], "library": mine}),
    );
    assert_eq!(third["name"], "NPN_2");
    assert!(dir.path().join("mine/symbols/NPN_2.redasym").exists());

    // The copy is the same Symbol under its new UUID and name.
    let original = element(&mut engine, &json!(NPN_SYMBOL));
    let copy = element(&mut engine, &second["element"]);
    assert_eq!(copy["library"], json!(mine));
    assert_eq!(copy["read_only"], false);
    let search = engine.run_ok("library.search", json!({}));
    assert_eq!(search["problems"], json!([]));
    assert_eq!(original["name"], "NPN");
    assert_eq!(copy["name"], "NPN_1");
}

#[test]
fn a_copied_part_still_references_its_original_symbol_and_footprints() {
    let dir = TempDir::new("element-copy-part");
    let (mut engine, mine) = engine(dir.path());
    let out = engine.run_ok(
        "library.element.copy",
        json!({"element": NPN, "library": mine}),
    );
    let search = engine.run_ok("library.search", json!({"library": mine}));
    let parts = search["parts"].as_array().unwrap();
    assert_eq!(parts.len(), 1);
    assert_eq!(parts[0]["uuid"], out["element"]);
    assert_eq!(parts[0]["symbol"], NPN_SYMBOL);
    assert_eq!(parts[0]["footprints"], json!([SOT23]));
    let used = engine.run_ok("library.where_used", json!({"element": NPN_SYMBOL}));
    let libraries: Vec<_> = used["parts"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| {
            (
                p["name"].as_str().unwrap(),
                p["library"]["name"].as_str().unwrap(),
            )
        })
        .collect();
    assert_eq!(
        libraries,
        [
            ("NPN", "Mine"),
            ("NPN", "Starter"),
            ("NPN_TO-92", "Starter")
        ]
    );
}

#[test]
fn copying_into_the_starter_library_or_from_nowhere_is_refused() {
    let dir = TempDir::new("element-copy-refused");
    let (mut engine, mine) = engine(dir.path());
    let starter = engine.run_ok("library.search", json!({}))["libraries"]
        .as_array()
        .unwrap()
        .iter()
        .find(|l| l["origin"] == "starter")
        .unwrap()["uuid"]
        .clone();
    let err = engine.run_err(
        "library.element.copy",
        json!({"element": NPN_SYMBOL, "library": starter}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    let nowhere = "00000000-0000-4000-8000-0000000000ff";
    let err = engine.run_err(
        "library.element.copy",
        json!({"element": nowhere, "library": mine}),
    );
    assert_eq!(err.code, ErrorCode::NotFound, "{err:?}");
    let err = engine.run_err(
        "library.element.copy",
        json!({"element": NPN_SYMBOL, "library": nowhere}),
    );
    assert_eq!(err.code, ErrorCode::NotFound, "{err:?}");
}

const PI_TEMPLATE: &str = "40f34815-ae6e-4549-bf0e-a54bb293ca59";

fn template_names(engine: &mut TestEngine, library: &str) -> Vec<String> {
    let out = engine.run_ok("library.board_template.list", json!({"library": library}));
    assert_eq!(out["problems"], json!([]), "{out}");
    let mut names: Vec<String> = (out["templates"].as_array().unwrap().iter())
        .map(|t| t["name"].as_str().unwrap().to_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn board_templates_copy_duplicate_and_delete_like_other_elements() {
    let dir = TempDir::new("template-copy");
    let (mut engine, mine) = engine(dir.path());
    let name = "Add-on board for Raspberry Pi";

    // Copy to Library… from the Starter Library.
    let copied = engine.run_ok(
        "library.element.copy",
        json!({"element": PI_TEMPLATE, "library": mine}),
    );
    assert_eq!(copied["kind"], "board_template");
    assert_eq!(copied["name"], name);
    assert_ne!(copied["element"], PI_TEMPLATE);
    assert_eq!(
        Path::new(copied["path"].as_str().unwrap()),
        dir.path()
            .join(format!("mine/board-templates/{name}.redaboard"))
    );
    // Duplicate: a copy into its own Library takes the next free name.
    let duplicate = engine.run_ok(
        "library.element.copy",
        json!({"element": copied["element"], "library": mine}),
    );
    assert_eq!(duplicate["name"], format!("{name}_1"));
    assert_eq!(
        template_names(&mut engine, &mine),
        [name.to_owned(), format!("{name}_1")]
    );
    // The copy evaluates as the original does.
    let preview = |engine: &mut TestEngine, element: &Value| {
        let frame = engine.run_ok(
            "library.board_template.preview",
            json!({"element": element}),
        );
        frame["outline"].clone()
    };
    assert_eq!(
        preview(&mut engine, &duplicate["element"]),
        preview(&mut engine, &json!(PI_TEMPLATE))
    );

    // Delete removes the file; the Starter Library's can't be deleted.
    let deleted = engine.run_ok(
        "library.element.delete",
        json!({"element": copied["element"]}),
    );
    assert_eq!(deleted["kind"], "board_template");
    assert!(
        !dir.path()
            .join(format!("mine/board-templates/{name}.redaboard"))
            .exists()
    );
    assert_eq!(template_names(&mut engine, &mine), [format!("{name}_1")]);
    let err = engine.run_err("library.element.delete", json!({"element": PI_TEMPLATE}));
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    assert!(err.message.contains("read-only"), "{err:?}");
    let err = engine.run_err(
        "library.element.delete",
        json!({"element": copied["element"]}),
    );
    assert_eq!(err.code, ErrorCode::NotFound, "{err:?}");
}

#[test]
fn delete_refuses_a_symbol_a_part_uses_and_an_element_open_in_its_editor() {
    let dir = TempDir::new("element-delete");
    let (mut engine, mine) = engine(dir.path());
    let symbol = engine.run_ok(
        "library.element.copy",
        json!({"element": NPN_SYMBOL, "library": mine}),
    );
    let part = engine.run_ok(
        "library.element.copy",
        json!({"element": NPN, "library": mine}),
    );
    // The copied Part uses the Starter Symbol, not the copy; point it at the copy.
    let opened = element(&mut engine, &part["element"]);
    assert_eq!(opened["kind"], "part");
    engine.run_ok(
        "library.part.symbol.set",
        json!({"part": part["element"], "symbol": symbol["element"]}),
    );
    engine.run_ok("library.element.save", json!({"element": part["element"]}));
    let err = engine.run_err(
        "library.element.delete",
        json!({"element": symbol["element"]}),
    );
    assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
    assert!(err.message.contains("used by NPN"), "{err:?}");
    // The Part is open in its editor.
    let err = engine.run_err(
        "library.element.delete",
        json!({"element": part["element"]}),
    );
    assert!(err.message.contains("open in its editor"), "{err:?}");
    engine.run_ok("library.element.close", json!({"element": part["element"]}));
    engine.run_ok(
        "library.element.delete",
        json!({"element": part["element"]}),
    );
    engine.run_ok(
        "library.element.delete",
        json!({"element": symbol["element"]}),
    );
    assert!(!dir.path().join("mine/symbols/NPN.redasym").exists());
    assert!(!dir.path().join("mine/parts/NPN.redapart").exists());
}

#[test]
fn library_element_new_refuses_a_board_template() {
    let dir = TempDir::new("element-new-template");
    let (mut engine, mine) = engine(dir.path());
    let err = engine.run_err(
        "library.element.new",
        json!({"library": mine, "kind": "board_template", "name": "Mine"}),
    );
    assert_eq!(err.code, ErrorCode::InvalidParams, "{err:?}");
}

#[test]
fn copying_into_a_read_only_library_folder_is_refused_for_every_kind() {
    // One writability rule for save and copy: the Starter Library, and a folder the
    // filesystem won't let us write.
    let dir = TempDir::new("element-copy-read-only");
    let (mut engine, mine) = engine(dir.path());
    for (element, folder) in [(PI_TEMPLATE, "board-templates"), (NPN_SYMBOL, "symbols")] {
        let folder = dir.path().join("mine").join(folder);
        std::fs::create_dir_all(&folder).unwrap();
        set_read_only(&folder, true);
        let err = engine.run_err(
            "library.element.copy",
            json!({"element": element, "library": mine}),
        );
        set_read_only(&folder, false);
        assert_eq!(err.code, ErrorCode::Rejected, "{err:?}");
        assert!(err.message.contains("read-only"), "{err:?}");
        assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 0);
    }
}

#[test]
fn a_board_template_does_not_open_in_an_element_editor() {
    let dir = TempDir::new("element-open-template");
    let (mut engine, _) = engine(dir.path());
    let err = engine.run_err("library.element.open", json!({"element": PI_TEMPLATE}));
    assert_ne!(err.code, ErrorCode::Internal, "{err:?}");
}

fn set_read_only(folder: &Path, on: bool) {
    let mut permissions = std::fs::metadata(folder).unwrap().permissions();
    permissions.set_readonly(on);
    std::fs::set_permissions(folder, permissions).unwrap();
}

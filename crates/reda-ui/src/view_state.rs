//! Per-Project view state: `.reda/view.json` holds the layout, the open tabs, and each
//! Document's camera, Layer state and working state (grid, units, snap mode, Gridless and
//! Selection Filter). The selection is not kept.
//!
//! The file is disposable. One that cannot be read, or that another version of REDA
//! wrote, is thrown away without a word in favour of the default layout. Writing it never
//! touches a Document, so viewing never makes one dirty.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{ObjectType, Uuid};
use reda_engine::pcb::RatsnestVisibility;
use reda_engine::preferences::{InactiveLayerMode, Units};
use serde::{Deserialize, Serialize};

use crate::canvas::Camera;
use crate::layout::Layout;
use crate::working_state::SnapMode;

/// How long the view must stay unchanged before it is saved.
pub const VIEW_SAVE_DELAY: Duration = Duration::from_secs(1);

/// The REDA version written into the file; a file from any other version is discarded.
const VERSION: &str = env!("CARGO_PKG_VERSION");

/// One Document's Layer state in its view.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct LayerView {
    /// The Layers hidden, by UUID.
    pub hidden: BTreeSet<Uuid>,
    /// How Layers other than the active one are drawn; unset follows the Preference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inactive_mode: Option<InactiveLayerMode>,
    /// The Layer edited; unset is the top copper.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active: Option<Uuid>,
    /// The board is seen from below: mirrored, with the physical Layer order reversed.
    #[serde(skip_serializing_if = "is_default")]
    pub from_bottom: bool,
    /// The kinds of object not drawn, such as `track`.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub hidden_types: BTreeSet<ObjectType>,
    /// Which Ratsnest lines show.
    #[serde(skip_serializing_if = "is_default")]
    pub ratsnest: RatsnestVisibility,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// What `view.json` keeps of one Document.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct DocumentView {
    /// Zoom and pan; unset fits the Document.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub camera: Option<Camera>,
    pub layers: LayerView,
    /// The board's user origin (`pcb.view.origin.set`); unset is the board's own.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub user_origin: Option<Point>,
    /// The snap grid picked; unset is the Document type's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub grid: Option<Nm>,
    /// The units lengths show in (the board's); unset follows the Preference.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub units: Option<Units>,
    /// The object snap mode; unset is the Document type's default.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub snap_mode: Option<SnapMode>,
    /// The grid level of snapping is off.
    #[serde(skip_serializing_if = "is_default")]
    pub gridless: bool,
    /// The kinds the Selection Filter turns off.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub selection_filter_off: BTreeSet<String>,
}

/// The contents of `view.json`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ViewFile {
    /// The REDA version that wrote the file.
    reda: String,
    pub layout: Layout,
    #[serde(default)]
    pub documents: BTreeMap<Uuid, DocumentView>,
}

impl ViewFile {
    pub fn new(layout: Layout, documents: BTreeMap<Uuid, DocumentView>) -> ViewFile {
        ViewFile {
            reda: VERSION.to_owned(),
            layout,
            documents,
        }
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut bytes = serde_json::to_vec_pretty(self).expect("view state serialises");
        bytes.push(b'\n');
        bytes
    }
}

/// `.reda/view.json` in the Project folder `folder`.
pub fn path(folder: &Path) -> PathBuf {
    folder.join(".reda").join("view.json")
}

/// The view state saved in `folder`, or `None` when there is none this version can use.
pub fn read(folder: &Path) -> Option<ViewFile> {
    let bytes = std::fs::read(path(folder)).ok()?;
    let file: ViewFile = serde_json::from_slice(&bytes).ok()?;
    (file.reda == VERSION).then_some(file)
}

/// Writes `bytes` as the view state of `folder`.
pub fn write(folder: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let file = path(folder);
    if let Some(dir) = file.parent() {
        std::fs::create_dir_all(dir)?;
    }
    reda_engine::fs::write_atomic(&file, bytes)
}

/// When to save: after the view has stayed unchanged for [`VIEW_SAVE_DELAY`].
#[derive(Debug, Default)]
pub struct Debounce {
    /// What the file holds, as far as this session knows.
    written: Option<Vec<u8>>,
    /// The latest unsaved state and when it was first seen.
    pending: Option<(Vec<u8>, Instant)>,
}

impl Debounce {
    /// Takes `bytes` as what the file holds.
    pub fn saved(&mut self, bytes: Vec<u8>) {
        self.written = Some(bytes);
        self.pending = None;
    }

    /// Notes the state at `now`; returns the bytes to write once they have settled.
    pub fn poll(&mut self, bytes: Vec<u8>, now: Instant) -> Option<Vec<u8>> {
        if self.written.as_ref() == Some(&bytes) {
            self.pending = None;
            return None;
        }
        match &self.pending {
            Some((pending, since)) if *pending == bytes => {
                (now.saturating_duration_since(*since) >= VIEW_SAVE_DELAY).then_some(bytes)
            }
            _ => {
                self.pending = Some((bytes, now));
                None
            }
        }
    }
}

//! The window shell's state, kept apart from drawing so it can be tested without a
//! window: the docking [`Layout`] and the open Document tabs, opening and creating
//! Projects, recent Projects, and running Commands on the engine.
//!
//! Every change to the design or the layout goes through [`Shell::run`], which runs a
//! Command and then refreshes what the tabs show (their Scenes) and the fonts the engine
//! has. The Layout lives in the engine's view state, where the layout Commands change it.
//!
//! Each Project's view (the layout and each Document's camera and Layers) is kept in its
//! `.reda/view.json`: see [`view_state`](crate::view_state).

mod checks;
mod cross_select;
mod grid_snap;
mod pcb_input;
mod prefs;
mod projects;
mod refresh;
mod route;
mod run;
mod tabs;

use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Instant;

use reda_engine::command::{CommandError, FontFace, Kind, Signals};
use reda_engine::journal::Source;
use reda_engine::library::LibraryIndex;
use reda_engine::model::Project;
use reda_engine::model::geom::{Nm, Point};
use reda_engine::model::{DocumentKind, HERSHEY_FONT, NOTO_SANS_FONT, Scope, Severity, Uuid};
use reda_engine::preferences::Preferences;
use reda_engine::sync::ChangeList;
use reda_engine::{Engine, preferences, state, store};
use serde_json::{Value, json};

use crate::canvas::{Camera, LiveHandle, Overlay, PaletteHandle, SceneHandle, SelectionHandle};
use crate::cross_probe::{self, CrossProbe};
use crate::grid::{self, Grids};
use crate::highlight::Highlight;
use crate::keymap::{self, Keymap};
use crate::layout::{self, Layout, SettingsWindow, TabKey, WindowRect, saved};
use crate::messages::{self, Marker, Messages, Note};
use crate::pcb::{self, BoardView};
use crate::recovery::{self, Recorder};
use crate::schematic_editor::{Interaction, Turn};
use crate::selection::{self, Selection};
use crate::snap::Snapped;
pub use crate::tool_preview::ToolPreview;
use crate::tools::{self, ActiveTool, PowerPortName, Tool};
use crate::tr;
use crate::user_preferences::Watched;
use crate::view_state::{self, Debounce, DocumentView, ViewFile};

use crate::working_state::{self, SelectionFilter, WorkingState};

pub use crate::view_state::{LayerView, VIEW_SAVE_DELAY};

/// Where the engine gets the bytes of fonts installed on the machine. The app scans the
/// OS font folders; tests pass a fake.
pub trait InstalledFonts: Send + Sync {
    /// The installed face for `family` (its regular style when there are several), or
    /// `None` when the machine does not have it.
    fn load(&self, family: &str) -> Option<FontFace>;
}

/// No installed fonts: only REDA's bundled ones.
struct NoFonts;

impl InstalledFonts for NoFonts {
    fn load(&self, _family: &str) -> Option<FontFace> {
        None
    }
}

/// What a canvas's masking was worked out for (Scene and marks generations), and the
/// answer.
type MaskingKey = ((u64, u64), bool);

/// A Document open in a tab.
#[derive(Clone, Debug, PartialEq)]
pub struct DocumentTab {
    pub document: Uuid,
    pub title: String,
    /// The canvas camera; `None` until the canvas first lays out, which fits the Document.
    pub camera: Option<Camera>,
    /// Which Layers show, and how inactive ones are drawn.
    pub layers: LayerView,
    /// What the canvas draws.
    pub scene: SceneHandle,
    /// A box to zoom the camera to when the canvas next lays out, in Document nm.
    pub zoom_to: Option<(Point, Point)>,
    /// A point to centre the camera on when the canvas next lays out, keeping the zoom
    /// (`view.jump_to` before the canvas has a camera).
    pub center_on: Option<Point>,
    /// The selected objects the canvas marks.
    pub selection: SelectionHandle,
    /// The Violation markers the canvas draws over the Document, at most the
    /// `checks.max_markers` Preference.
    pub markers: Arc<[Marker]>,
    /// How many of the Document's Violations have no marker drawn because of that cap.
    pub markers_not_shown: usize,
    /// The snap grid the canvas draws and Tools snap to.
    pub grid: Nm,
    /// The visible grid the canvas draws.
    pub grid_visible: grid::Visible,
    /// The Document point the visible grid runs through.
    pub grid_origin: Point,
    /// What the editor has in flight on this canvas.
    pub interaction: Interaction,
    /// A Wire run another Tool suspended, until that Tool finishes.
    pub suspended: Option<Interaction>,
    /// The hovered object and the objects in flight, as the canvas marks them.
    pub live: LiveHandle,
    /// How far (nm) the objects in flight are drawn from where they are.
    pub moving_offset: (f64, f64),
    /// What the editor draws over the canvas: Wires in flight, a selection box.
    pub overlay: Overlay,
    /// The pointer's last Document position (nm) over the canvas.
    pub cursor: Option<(f64, f64)>,
    /// The colour overrides the canvas draws with.
    pub palette: PaletteHandle,
    /// Earlier views, latest last: Alt+← (`view.previous`) goes back through them.
    pub history: Vec<Camera>,
    /// A wheel zoom or pan was under way last frame.
    pub navigating: bool,
    /// Auto-pan's state while an operation is in flight.
    pub auto_pan: crate::canvas::auto_pan::AutoPan,
}

/// How many earlier views a canvas keeps for Alt+←.
const VIEW_HISTORY: usize = 50;

impl DocumentTab {
    /// Keeps the current view so Alt+← can come back to it, before the view jumps.
    pub fn remember_view(&mut self) {
        let Some(camera) = self.camera else {
            return;
        };
        if self.history.last() != Some(&camera) {
            self.history.push(camera);
            if self.history.len() > VIEW_HISTORY {
                self.history.remove(0);
            }
        }
    }

    /// Goes back to the last remembered view; false when there is none.
    pub fn previous_view(&mut self) -> bool {
        let Some(camera) = self.history.pop() else {
            return false;
        };
        self.camera = Some(camera);
        self.zoom_to = None;
        self.center_on = None;
        true
    }

    /// The canvas shows a board from below, so Document x runs right to left on screen.
    pub fn mirrored(&self) -> bool {
        self.layers.from_bottom && self.scene.layers.is_some()
    }

    fn new(document: Uuid, title: String, scene: SceneHandle) -> DocumentTab {
        DocumentTab {
            document,
            title,
            camera: None,
            layers: LayerView::default(),
            scene,
            zoom_to: None,
            center_on: None,
            selection: SelectionHandle::default(),
            markers: Arc::from([]),
            markers_not_shown: 0,
            grid: grid::SCHEMATIC_DEFAULT,
            grid_visible: grid::Visible::default(),
            grid_origin: Point::default(),
            interaction: Interaction::Idle,
            suspended: None,
            live: LiveHandle::default(),
            moving_offset: (0.0, 0.0),
            overlay: Overlay::default(),
            cursor: None,
            palette: PaletteHandle::default(),
            history: Vec::new(),
            navigating: false,
            auto_pan: Default::default(),
        }
    }
}

/// One row of the Start page's recent Projects.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RecentProject {
    /// The `.reda` file.
    pub path: PathBuf,
    /// The Project name: the file stem.
    pub name: String,
    /// The file is gone; the row is greyed out.
    pub missing: bool,
}

/// The shell: the engine and the tabs showing it.
pub struct Shell {
    pub(crate) engine: Engine,
    /// The open Documents' tabs, wherever the Layout puts them.
    documents: Vec<DocumentTab>,
    /// The units the Properties panel shows lengths in after `Ctrl+Q` there; `None`
    /// follows the active Document's.
    properties_units: Option<preferences::Units>,
    fonts: Arc<dyn InstalledFonts>,
    /// Families already asked of [`InstalledFonts`], found or not.
    asked_fonts: BTreeSet<String>,
    generation: u64,
    message: Option<String>,
    /// A loop from graphics refused for a gap, waiting for the outer-edge retry.
    loop_retry: Option<crate::pcb::board_shapes::LoopRetry>,
    /// A status-bar note that is not an error, such as a Tool a bridged edit cancelled.
    note: Option<String>,
    /// The preview of the Tool in use.
    tool: Option<ToolPreview>,
    messages: Messages,
    /// The [`Selection`]'s objects as the canvases mark them, renewed when it changes.
    marked: SelectionHandle,
    /// Per Document: whether its canvas masks what is not highlighted, keyed by the Scene
    /// and marks generations it was worked out for.
    masking: std::cell::RefCell<HashMap<Uuid, MaskingKey>>,
    /// What the highlight marks were last worked out from: the selection, the
    /// [`Highlight`], the edit count and the switches.
    probed: Option<(Selection, Highlight, u64, CrossProbe, bool)>,
    /// Live hover: the Net under the cursor on the board, and the Net whose outline the
    /// canvas drew last frame (see [`crate::pcb::live_hover`]).
    live_hover: (Option<Uuid>, Option<Uuid>),
    /// Counts the refreshes after edits, so the cross-probe marks know when to redo.
    edits: u64,
    keymap: Keymap,
    /// The Preferences as last read (see [`Shell::poll_preferences`]).
    preferences: Watched,
    /// What `schematic.copy` or `schematic.cut` last returned, for `schematic.paste`.
    clipboard: Option<Value>,
    /// Text `schematic.copy_as_text` returned, for the window to put on the system
    /// clipboard.
    copied_text: Option<String>,
    /// The PCB canvas's select-and-place Tool.
    pcb_tool: pcb::SelectTool,
    /// The board's routing and Via Tools.
    router: pcb::Router,
    /// The PCB drawing Tools' state (see `pcb::draw`).
    pcb_draw: pcb::draw::Draw,
    /// Whether the board's Zone fills are out of date ("Zones need refilling").
    zones_stale: bool,
    /// The automatic refill in flight (see [`crate::zone_refill`]).
    pub(crate) zone_refill: Option<crate::zone_refill::ZoneRefill>,
    /// An edit was committed while a refill ran, or a drag cancelled one: refill again.
    pub(crate) zone_refill_again: bool,
    /// Whether a PCB drag is in progress (the button is down and the pointer moved).
    pub(crate) pcb_dragging: bool,
    /// The pending Sync changes (what `sync.preview` returns), renewed after every edit;
    /// `None` without a PCB Document.
    change_list: Option<ChangeList>,
    /// The colour overrides from the Preferences.
    palette: PaletteHandle,
    /// The saved view of Documents without a tab, kept for when `view.json` is next saved.
    closed_views: BTreeMap<Uuid, DocumentView>,
    view_saves: Debounce,
    /// Recovery Snapshots and the Journal mirror (see [`crate::recovery`]).
    pub(crate) recorder: Recorder,
    /// Counts the Commands that may have changed the registered Libraries, so the
    /// Library panel knows when to scan them again.
    library_generation: u64,
    /// The `view.checks.clear` count last followed.
    checks_cleared: u64,
    /// The Library panel's filters, chosen Part and scan.
    library_panel: crate::panels::library::LibraryPanel,
    /// The registered Libraries as last scanned, and the library generation of the scan.
    library_index: Option<(u64, Arc<LibraryIndex>)>,
    /// A notice for the window to show once, such as Import part's vendor-terms notice.
    notice: Option<String>,
    /// Commands to call from the GUI after the frame (asking for missing params in a
    /// dialog), such as the Extract that Edit in Library offers on Source Missing.
    calls: Vec<(String, Value)>,
    /// The titles of the open settings windows' tabs, as the window sets them.
    settings_titles: BTreeMap<SettingsWindow, String>,
}

impl Shell {
    /// A shell on `engine`, showing its open Project or else the Start page, in the
    /// Default Layout. It adds the layout Commands to the engine.
    pub fn new(mut engine: Engine) -> Shell {
        if engine.registry().get("panel.show").is_none() {
            let dirs = engine.user_dirs().cloned();
            engine.register(|r| layout::register_with_layouts(r, dirs));
        }
        if engine.registry().get("selection.set").is_none() {
            engine.register(selection::register_all);
        }
        if engine.registry().get("tool.activate").is_none() {
            engine.register(tools::register_all);
        }
        if engine.registry().get("view.highlight").is_none() {
            engine.register(crate::highlight::register_all);
            engine.register(crate::similar::register_all);
        }
        if engine.registry().get("schematic.dialog.open").is_none() {
            engine.register(crate::schematic_dialogs::register_all);
        }
        if engine
            .registry()
            .get("schematic.port.go_to_partner")
            .is_none()
        {
            engine.register(crate::port_partner::register_all);
        }
        if engine.registry().get("tool.place").is_none() {
            engine.register(crate::placement::register_all);
        }
        if engine.registry().get("view.cross_select.toggle").is_none() {
            engine.register(cross_probe::register_all);
        }
        if engine.registry().get("view.grid.set").is_none() {
            engine.register(grid::register_all);
        }
        if engine.registry().get("view.hud.toggle").is_none() {
            engine.register(crate::hud::register_all);
        }
        if engine.registry().get("selection.filter.set").is_none() {
            engine.register(working_state::register_all);
        }
        if engine.registry().get("pcb.layer.show").is_none() {
            engine.register(pcb::register_all);
        }
        if engine.registry().get("library.symbol.unit.show").is_none() {
            engine.register(crate::symbol_editor::register_all);
        }
        if engine.registry().get("view.footprint_tool.set").is_none() {
            engine.register(crate::footprint_editor::register_all);
        }
        if engine.registry().get("library.paste_array").is_none() {
            engine.register(crate::library_dialogs::paste_array::register_all);
        }
        if engine.registry().get("view.messages.show_waived").is_none() {
            engine.register(messages::register_all);
        }
        if engine.registry().get("tool.paste").is_none() {
            engine.register(crate::schematic_editor::register_all);
        }
        if engine.registry().get("view.keys_card.toggle").is_none() {
            engine.register(crate::keys_card::register_all);
        }
        if engine.registry().get("view.properties.rules").is_none() {
            engine.register(crate::properties::commands::register_all);
        }
        if engine.registry().get("view.properties.pick_net").is_none() {
            engine.register(crate::properties::track::register_all);
        }
        let units = preferences::current(engine.user_dirs()).pcb.default_units;
        engine.view_mut().set(Layout::default());
        engine.view_mut().set(Selection::default());
        engine
            .view_mut()
            .set(selection::picking::Picking::default());
        engine.view_mut().set(Highlight::default());
        engine
            .view_mut()
            .set(crate::similar::SimilarState::default());
        engine
            .view_mut()
            .set(crate::navigation::SelectionStep::default());
        engine
            .view_mut()
            .set(crate::highlight::SessionStrengths::default());
        engine
            .view_mut()
            .set(reda_engine::preferences::HighlightPreferences::default());
        engine.view_mut().set(pcb::measure::Measurements::default());
        engine.view_mut().set(pcb::delete::DeletePrompt::default());
        engine
            .view_mut()
            .set(crate::port_partner::PartnerMenu::default());
        engine.view_mut().set(crate::keys_card::KeysCard::default());
        engine
            .view_mut()
            .set(crate::properties::commands::RevealRules::default());
        engine.view_mut().set(ActiveTool::default());
        engine.view_mut().set(pcb::placing::PlaceFlight::default());
        engine.view_mut().set(pcb::CopperCycle::default());
        engine
            .view_mut()
            .set(crate::properties::track::NetPick::default());
        engine
            .view_mut()
            .set(pcb::copper_drag::DragFlight::default());
        engine.view_mut().set(PowerPortName::default());
        engine.view_mut().set(tools::RotateAboutAngle::default());
        engine.view_mut().set(tools::WireTool::default());
        engine
            .view_mut()
            .set(crate::schematic_editor::DragKeys::default());
        engine
            .view_mut()
            .set(crate::panels::find::FindPanel::default());
        engine.view_mut().set(tools::BreakWireTool::default());
        engine.view_mut().set(tools::ToolStack::default());
        engine
            .view_mut()
            .set(crate::placement::Placement::default());
        engine.view_mut().set(Grids::default());
        engine.view_mut().set(crate::hud::HudState::default());
        let working = WorkingState::following(&preferences::current(engine.user_dirs()));
        engine.view_mut().set(working);
        engine.view_mut().set(CrossProbe::default());
        engine
            .view_mut()
            .set(crate::snap::PaletteRequests::default());
        engine.view_mut().set(messages::ShowWaived::default());
        engine.view_mut().set(messages::ChecksCleared::default());
        engine.view_mut().set(messages::MessageSteps::default());
        engine
            .view_mut()
            .set(crate::footprint_editor::FootprintEditorState::default());
        engine.view_mut().set(BoardView::new(units));
        engine
            .view_mut()
            .set(crate::symbol_editor::SymbolEditorState::default());
        engine.view_mut().set(layout::focus::Focus::default());
        crate::bars::install(&mut engine);
        crate::help::install(&mut engine);
        let mut shell = Shell {
            engine,
            documents: Vec::new(),
            properties_units: None,
            fonts: Arc::new(NoFonts),
            asked_fonts: BTreeSet::new(),
            generation: 0,
            message: None,
            loop_retry: None,
            note: None,
            tool: None,
            messages: Messages::default(),
            marked: SelectionHandle::default(),
            masking: Default::default(),
            probed: None,
            live_hover: (None, None),
            edits: 0,
            keymap: Keymap::default(),
            preferences: Watched::default(),
            clipboard: None,
            copied_text: None,
            pcb_tool: pcb::SelectTool::default(),
            router: pcb::Router::default(),
            pcb_draw: pcb::draw::Draw::default(),
            zones_stale: false,
            zone_refill: None,
            zone_refill_again: false,
            pcb_dragging: false,
            change_list: None,
            palette: PaletteHandle::default(),
            closed_views: BTreeMap::new(),
            view_saves: Debounce::default(),
            recorder: Recorder::new(),
            library_generation: 0,
            checks_cleared: 0,
            library_panel: Default::default(),
            library_index: None,
            notice: None,
            calls: Vec::new(),
            settings_titles: BTreeMap::new(),
        };
        shell.load_preference_warnings();
        shell.reload_keymap();
        shell.palette = pcb::view::palette_handle(shell.preferences.values(), 0);
        if shell.engine.project().is_some() {
            shell.show_project();
            shell.recorder.opened(&shell.engine);
        } else if shell.engine.user_dirs().is_some() {
            // Library editor tabs open with no Project snapshot into the per-user folder,
            // and what an unclean exit left there is offered now.
            shell.recorder.opened(&shell.engine);
        }
        shell.note_focus();
        shell
    }

    /// Takes installed fonts from `fonts` for PCB text that names them.
    pub fn with_fonts(mut self, fonts: Arc<dyn InstalledFonts>) -> Shell {
        self.fonts = fonts;
        self.load_fonts();
        self
    }

    pub(crate) fn engine_mut(&mut self) -> &mut Engine {
        &mut self.engine
    }

    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// The docking layout: where every tab is.
    pub fn layout(&self) -> &Layout {
        self.engine
            .view()
            .get::<Layout>()
            .expect("the Shell installs its Layout")
    }

    fn layout_mut(&mut self) -> &mut Layout {
        self.engine
            .view_mut()
            .get_mut::<Layout>()
            .expect("the Shell installs its Layout")
    }

    /// The canvases' picking state (tests set its clock with `set_time`).
    pub fn selection_picking_mut(&mut self) -> &mut selection::picking::Picking {
        self.engine
            .view_mut()
            .get_mut::<selection::picking::Picking>()
            .expect("installed picker")
    }

    /// What is selected; it changes through `selection.set`.
    /// The Tracks waiting for a canvas click to pick their Net (Track Properties).
    pub fn net_pick(&self) -> &[Uuid] {
        self.engine
            .view()
            .get::<crate::properties::track::NetPick>()
            .map_or(&[], |p| p.0.as_slice())
    }

    pub fn selection(&self) -> &Selection {
        self.engine
            .view()
            .get::<Selection>()
            .expect("the Shell installs its Selection")
    }

    /// What is highlighted on every canvas; it changes through `view.highlight`.
    pub fn highlight(&self) -> &Highlight {
        self.engine
            .view()
            .get::<Highlight>()
            .expect("the Shell installs its Highlight")
    }

    /// The active canvas Tool; it changes through `tool.activate`.
    pub fn tool(&self) -> Tool {
        self.engine
            .view()
            .get::<ActiveTool>()
            .map(|t| t.0)
            .unwrap_or_default()
    }

    /// What the last copy or cut put on the clipboard, for `schematic.paste`.
    pub fn clipboard(&self) -> Option<&Value> {
        self.clipboard.as_ref()
    }

    /// Takes the text Copy As Text left for the system clipboard.
    pub fn take_copied_text(&mut self) -> Option<String> {
        self.copied_text.take()
    }

    /// The board's view: its Layer state, units and snap grid. It changes through the
    /// PCB view Commands (`pcb.layer.show`, `view.grid.set`, ...).
    pub fn board_view(&self) -> &BoardView {
        self.engine
            .view()
            .get::<BoardView>()
            .expect("the Shell installs its BoardView")
    }

    fn board_view_mut(&mut self) -> &mut BoardView {
        self.engine
            .view_mut()
            .get_mut::<BoardView>()
            .expect("the Shell installs its BoardView")
    }

    /// The units the Properties panel was switched to with `Ctrl+Q`, if it was; the panel
    /// otherwise follows the active Document's.
    pub fn properties_units(&self) -> Option<preferences::Units> {
        self.properties_units
    }

    /// Sets the Properties panel's own units (`Ctrl+Q` there), leaving the canvas's.
    pub fn set_properties_units(&mut self, units: Option<preferences::Units>) {
        self.properties_units = units;
    }

    /// The Layer colour overrides the canvases draw with.
    pub fn palette(&self) -> &PaletteHandle {
        &self.palette
    }

    /// Counts the Commands that may have changed the registered Libraries (Library
    /// element and Project Commands, Preferences): a Library panel scan made at an older
    /// count may be stale.
    pub fn library_generation(&self) -> u64 {
        self.library_generation
    }

    /// The registered Libraries, scanned again only when a Command may have changed them
    /// (see [`Shell::library_generation`]).
    pub fn library_index(&mut self) -> Arc<LibraryIndex> {
        let generation = self.library_generation;
        match &self.library_index {
            Some((g, index)) if *g == generation => index.clone(),
            _ => {
                let index = Arc::new(self.engine.libraries());
                self.library_index = Some((generation, index.clone()));
                index
            }
        }
    }

    /// The loop from graphics refused for a gap, waiting for the outer-edge retry.
    pub fn loop_retry(&self) -> Option<&crate::pcb::board_shapes::LoopRetry> {
        self.loop_retry.as_ref()
    }

    /// Answers the outer-edge retry: `retry` runs the Command again along the outer edge
    /// of the strokes; either way the question goes.
    pub fn answer_loop_retry(&mut self, retry: bool) -> Option<Result<Value, CommandError>> {
        let pending = self.loop_retry.take()?;
        retry.then(|| self.run_reporting(&pending.id, pending.params))
    }

    /// Takes the notice waiting to be shown, such as the vendor-terms notice of the first
    /// Import part.
    pub fn take_notice(&mut self) -> Option<String> {
        self.notice.take()
    }

    /// Asks the window to call `id` after the frame, opening a dialog for the params it
    /// lacks.
    pub(crate) fn queue_call(&mut self, id: &str, params: Value) {
        self.calls.push((id.to_owned(), params));
    }

    /// The calls [`Shell::queue_call`] asked for, oldest first.
    pub(crate) fn take_calls(&mut self) -> Vec<(String, Value)> {
        std::mem::take(&mut self.calls)
    }

    /// The Library panel's filters, chosen row and scan.
    pub fn library_panel_mut(&mut self) -> &mut crate::panels::library::LibraryPanel {
        &mut self.library_panel
    }

    // ---- Live hover (see `pcb::live_hover`) ----

    /// The Net under the cursor on the board, whether or not its outline shows.
    pub fn live_hover_candidate(&self) -> Option<Uuid> {
        self.live_hover.0
    }

    pub(crate) fn set_live_hover_candidate(&mut self, net: Option<Uuid>) {
        self.live_hover.0 = net;
    }

    /// The Net whose live hover outline the board canvas drew in the last frame: the
    /// Net under the cursor while the Preference (and Shift, for Shift only) lets it show.
    pub fn live_hover_shown(&self) -> Option<Uuid> {
        self.live_hover.1
    }

    pub(crate) fn set_live_hover_shown(&mut self, net: Option<Uuid>) {
        self.live_hover.1 = net;
    }

    // ---- The PCB canvas Tool (see `pcb::tool`) ----

    /// Whether the board's Zone fills are out of date: the "Zones need refilling" note.
    pub fn zones_need_refill(&self) -> bool {
        self.zones_stale
    }

    /// How many Sync changes are pending: "out of sync: N changes" on the PCB tab and in
    /// the status bar. Zero without a PCB Document.
    pub fn out_of_sync(&self) -> usize {
        self.change_list.as_ref().map_or(0, |l| l.count)
    }

    /// Whether `object` (or the child `child` of its owner) is masked on `document`'s
    /// canvas: a masking highlight is on there and leaves it out. Masked objects can't be
    /// hit.
    pub fn is_masked(&self, document: Uuid, object: Uuid, owner: Option<Uuid>) -> bool {
        let marked = &self.marked;
        if !marked.mask || marked.highlighted.is_empty() {
            return false;
        }
        let lit = |o: Uuid| marked.highlighted.binary_search(&o).is_ok();
        if lit(object) || owner.is_some_and(lit) {
            return false;
        }
        let Some(tab) = self.document_tab(document) else {
            return false;
        };
        let key = (tab.scene.generation, marked.generation);
        let mut cache = self.masking.borrow_mut();
        match cache.get(&document) {
            Some((k, masking)) if *k == key => *masking,
            _ => {
                // As on the GPU, only a highlighted object this canvas draws masks the rest.
                let masking = tab
                    .scene
                    .scene
                    .items
                    .iter()
                    .any(|i| lit(i.object) || i.child.as_ref().is_some_and(|c| lit(c.owner())));
                cache.insert(document, (key, masking));
                masking
            }
        }
    }

    /// What the canvases mark highlighted: the [`Highlight`]'s objects and what
    /// cross-select and Net cross-probing carry across (see [`cross_probe`]), sorted by
    /// UUID.
    pub fn cross_probe(&self) -> &[Uuid] {
        &self.marked.highlighted
    }

    /// Which cross-probes are on (`view.cross_select.toggle`,
    /// `view.net_cross_probe.toggle`).
    pub fn cross_probe_switches(&self) -> CrossProbe {
        self.engine
            .view()
            .get::<CrossProbe>()
            .copied()
            .unwrap_or_default()
    }

    /// Whether `document` is the open Project's PCB Document.
    pub fn is_pcb(&self, document: Uuid) -> bool {
        self.engine
            .project()
            .and_then(|p| p.pcb())
            .is_some_and(|pcb| pcb.uuid == document)
    }

    /// The pending Sync Change List, as `sync.preview` returns it; `None` without a PCB
    /// Document.
    pub fn change_list(&self) -> Option<&ChangeList> {
        self.change_list.as_ref()
    }

    // ---- The routing and Via Tools (see `pcb::route`) ----

    /// The last error to show the user, if any.
    pub fn message(&self) -> Option<&str> {
        self.message.as_deref()
    }

    /// Shows `message` in the status bar, as for a failed Command.
    pub fn report(&mut self, message: String) {
        self.message = Some(message);
    }

    pub fn dismiss_message(&mut self) {
        self.message = None;
    }

    /// The status-bar note, if any: news that is not an error.
    pub fn note(&self) -> Option<&str> {
        self.note.as_deref()
    }

    pub fn dismiss_note(&mut self) {
        self.note = None;
    }

    /// Starts a Tool's preview, replacing any other.
    pub fn begin_tool(&mut self, preview: ToolPreview) {
        self.tool = Some(preview);
    }

    /// The preview of the Tool in use; `None` once it ends or a bridged edit cancels it.
    pub fn tool_preview(&self) -> Option<&ToolPreview> {
        self.tool.as_ref()
    }

    /// Ends the Tool's preview: it committed or the user abandoned it.
    pub fn end_tool(&mut self) {
        self.tool = None;
    }
}

/// The stem and kind of `path` when it names a Document file rather than a Project.
fn document_file(path: &Path) -> Option<(String, DocumentKind)> {
    let kind = DocumentKind::from_extension(path.extension()?.to_str()?)?;
    let stem = path.file_stem()?.to_string_lossy().into_owned();
    Some((stem, kind))
}

/// The `source` of missing-font warnings.
const FONTS: &str = "fonts";
/// The `source` of the notes on Documents whose files are missing.
const MISSING_DOCUMENTS: &str = "documents";
/// The `source` of Preference warnings.
const PREFERENCES: &str = "preferences";
const KEYMAP: &str = "keymap";

/// The open Project's PCB Document; only called when there is one.
fn pcb_of(engine: &Engine) -> &reda_engine::model::PcbDocument {
    engine
        .project()
        .and_then(|p| p.pcb())
        .expect("checked by the caller")
}

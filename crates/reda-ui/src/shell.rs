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
}

impl DocumentTab {
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
    probed: Option<(Selection, Highlight, u64, CrossProbe)>,
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
        }
        if engine.registry().get("schematic.dialog.open").is_none() {
            engine.register(crate::schematic_dialogs::register_all);
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
        if engine.registry().get("view.messages.show_waived").is_none() {
            engine.register(messages::register_all);
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

    fn focus_state(&mut self) -> &mut layout::focus::Focus {
        self.engine
            .view_mut()
            .get_mut::<layout::focus::Focus>()
            .expect("the Shell installs its Focus")
    }

    /// Remembers the focused tab as the most recently used.
    fn note_focus(&mut self) {
        if let Some(tab) = self.layout().focus.clone() {
            self.focus_state().used(&tab);
        }
    }

    /// The tab switcher (Ctrl+Tab), while it is open.
    pub fn tab_switch(&self) -> Option<&layout::focus::TabSwitch> {
        self.engine
            .view()
            .get::<layout::focus::Focus>()
            .and_then(|f| f.switching.as_ref())
    }

    /// Picks the switcher's tab at `index`.
    pub fn pick_tab_switch(&mut self, index: usize) {
        if let Some(switch) = &mut self.focus_state().switching
            && index < switch.tabs.len()
        {
            switch.index = index;
        }
    }

    /// Closes the tab switcher; with `commit`, shows and focuses the tab it picked
    /// (through `layout.tab.activate`).
    pub fn end_tab_switch(&mut self, commit: bool) {
        let Some(switch) = self.focus_state().switching.take() else {
            return;
        };
        if commit && let Some(tab) = switch.picked() {
            let tab = tab.clone();
            let params = json!({"tab": tab.to_string()});
            if self.run_reporting("layout.tab.activate", params).is_ok() {
                self.focus_state().request = Some(tab);
            }
        }
    }

    /// Takes the tab whose strip button should take the keyboard focus, after a Command
    /// moved focus to it from the keyboard.
    pub fn take_focus_request(&mut self) -> Option<TabKey> {
        self.focus_state().request.take()
    }

    pub(crate) fn selection_picking_mut(&mut self) -> &mut selection::picking::Picking {
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

    /// `document`'s snap grid; it changes through `view.grid.set` and `view.grid.cycle`.
    pub fn grid(&self, document: Uuid) -> Nm {
        self.engine
            .view()
            .get::<Grids>()
            .map(|g| g.of(self.engine.elements(), document))
            .unwrap_or(grid::SCHEMATIC_DEFAULT)
    }

    /// The grids in force, their preset tables and custom values.
    pub fn grids(&self) -> Grids {
        self.engine
            .view()
            .get::<Grids>()
            .cloned()
            .unwrap_or_default()
    }

    /// The Electrical Range on a sheet or Symbol: its grid row's, else 0.8 of the step.
    pub fn electrical_range(&self, document: Uuid) -> Nm {
        self.engine
            .view()
            .get::<Grids>()
            .map(|g| g.range(self.engine.elements(), document))
            .unwrap_or_else(|| grid::default_range(self.grid(document)))
    }

    /// The origin the Document's grid runs through (the Output Origin on a board).
    pub fn grid_origin(&self, document: Uuid) -> Point {
        grid::origin(self.engine.elements(), document)
    }

    /// The open board's grid origin: its Output Origin, or zero while no board is open
    /// or none is placed. The board tools snap, and the cursor readout measures, from it.
    pub fn board_grid_origin(&self) -> Point {
        self.engine
            .project()
            .and_then(|p| p.pcb())
            .map_or_else(Point::default, |pcb| self.grid_origin(pcb.uuid))
    }

    /// A Document point on its grid level, including the board's Output Origin; left
    /// where it is when Gridless, or with the grid suspended.
    pub fn snap_to_grid(&self, document: Uuid, at: (f64, f64)) -> Point {
        crate::snap::to_grid(self, document, at)
    }

    /// Feeds this frame's keys over `document`'s canvas to the snap suspend keys
    /// ([`crate::snap::Suspend`]): `Mod` and Shift held, and the primary button down.
    pub fn hold_snap_keys(&mut self, document: Uuid, modifiers: egui::Modifiers, pressed: bool) {
        let keys = crate::snap::held(self, document, modifiers, pressed);
        self.engine.view_mut().set(keys);
    }

    /// `document`'s Selection Filter; it changes through `selection.filter.set`.
    pub fn selection_filter(&self, document: Uuid) -> SelectionFilter {
        self.engine
            .view()
            .get::<WorkingState>()
            .map(|w| w.filter(document))
            .unwrap_or_default()
    }

    /// Moves `document`'s canvas as `view.jump_to` asked, keeping the zoom.
    pub fn jump(&mut self, document: Uuid, jump: crate::navigation::Jump) {
        use crate::navigation::Jump;
        let Some(tab) = self.document_tab_mut(document) else {
            return;
        };
        let pending = tab.center_on.take();
        let center = match (tab.camera.as_ref(), pending) {
            (_, Some(p)) => (p.x.0 as f64, p.y.0 as f64),
            (Some(camera), None) => camera.center,
            (None, None) => {
                // No camera yet: the fitted canvas centres on the Document's bounds.
                let zero = Point::new(Nm(0), Nm(0));
                let (lo, hi) = tab.scene.scene.bounds().unwrap_or((zero, zero));
                (
                    (lo.x.0 + hi.x.0) as f64 / 2.0,
                    (lo.y.0 + hi.y.0) as f64 / 2.0,
                )
            }
        };
        let target = match jump {
            Jump::To(p) => (p.x.0 as f64, p.y.0 as f64),
            Jump::By(d) => (center.0 + d.x.0 as f64, center.1 + d.y.0 as f64),
        };
        match tab.camera.as_mut() {
            Some(camera) => camera.center = target,
            None => {
                tab.center_on = Some(Point::new(Nm(target.0 as i64), Nm(target.1 as i64)));
            }
        }
    }

    /// What the last copy or cut put on the clipboard, for `schematic.paste`.
    pub fn clipboard(&self) -> Option<&Value> {
        self.clipboard.as_ref()
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

    /// A button went down on the board at `at`; `toggle` is `Mod` held; `tolerance` is
    /// how near counts as on an object.
    pub fn pcb_press(&mut self, at: Point, toggle: bool, tolerance: Nm) {
        if self.drawing() {
            let mut draw = std::mem::take(&mut self.pcb_draw);
            draw.press(self, at);
            self.pcb_draw = draw;
            return;
        }
        let modifiers = egui::Modifiers {
            command: toggle,
            ..egui::Modifiers::NONE
        };
        self.with_pcb_tool(|tool, shell| tool.press(shell, at, modifiers, tolerance));
    }

    /// The canvas passes actual modifiers: Shift toggles picks, while Mod detaches drags.
    pub fn pcb_press_with_modifiers(
        &mut self,
        at: Point,
        modifiers: egui::Modifiers,
        tolerance: Nm,
    ) {
        if self.drawing() {
            self.pcb_press(at, modifiers.command, tolerance);
            return;
        }
        self.with_pcb_tool(|tool, shell| tool.press(shell, at, modifiers, tolerance));
    }

    /// The pointer moved to `at` with the button down.
    pub fn pcb_drag(&mut self, at: Point) {
        self.drag_started();
        if self.drawing() {
            let mut draw = std::mem::take(&mut self.pcb_draw);
            draw.drag(self, at);
            self.pcb_draw = draw;
            return;
        }
        self.with_pcb_tool(|tool, shell| tool.drag(shell, at));
    }

    /// The button came up: selects, or runs the move in flight as one Command.
    pub fn pcb_release(&mut self) {
        self.pcb_dragging = false;
        if self.drawing() {
            self.with_draw(|draw, shell| draw.release(shell));
            return;
        }
        self.with_pcb_tool(|tool, shell| tool.release(shell));
    }

    /// Esc: drops the move or rubber band in flight.
    pub fn pcb_cancel(&mut self) {
        self.with_pcb_tool(|tool, shell| tool.cancel(shell));
    }

    /// Space: turns the move in flight, or the selection, by 90°.
    pub(crate) fn pcb_rotate(&mut self) {
        self.with_pcb_tool(|tool, shell| tool.rotate(shell));
    }

    /// Move Board Shape: the Board Outline, with any selected Board Cutouts, follows the
    /// pointer until a click puts it down. Anything in flight is dropped first.
    /// With `whole`, Move Board: the outline and Cutouts show the move, and the click
    /// runs `pcb.board.move`.
    pub(crate) fn pcb_move_board_shape(&mut self, whole: bool) {
        selection::picking::cancel(self);
        self.pcb_cancel();
        self.with_router(|r, shell| r.cancel(shell));
        let tool = self.tool();
        self.pcb_draw.start(tool);
        self.with_pcb_tool(|tool, shell| tool.carry_board_shape(shell, whole));
    }

    /// L: flips the move in flight, or the selection, to the other side.
    pub(crate) fn pcb_flip(&mut self) {
        self.with_pcb_tool(|tool, shell| tool.flip(shell));
    }

    /// The cursor is at `at` with no button down: where it snaps.
    pub fn pcb_hover(&mut self, at: Point) -> Snapped {
        if self.drawing() {
            let mut draw = std::mem::take(&mut self.pcb_draw);
            let snapped = draw.hover(self, at);
            self.pcb_draw = draw;
            return snapped;
        }
        self.with_pcb_tool(|tool, shell| tool.hover_mut(shell, at))
    }

    /// Runs `f` on the PCB Tool, then shares what it has in flight with the placement
    /// Commands ([`pcb::placing::PlaceFlight`]).
    pub(crate) fn with_pcb_tool<T>(
        &mut self,
        f: impl FnOnce(&mut pcb::SelectTool, &mut Shell) -> T,
    ) -> T {
        let mut tool = std::mem::take(&mut self.pcb_tool);
        let out = f(&mut tool, self);
        self.pcb_tool = tool;
        let (moving, queue) = (
            self.pcb_tool.carries_footprints(self),
            self.pcb_tool.queue(),
        );
        let dragging = self.pcb_tool.copper_drag().is_some();
        if let Some(flight) = self
            .engine
            .view_mut()
            .get_mut::<pcb::copper_drag::DragFlight>()
        {
            flight.0 = dragging;
        }
        if let Some(flight) = pcb::placing::flight_mut(self) {
            flight.moving = moving;
            flight.queue = queue;
        }
        out
    }

    /// The PCB Tool, to read what it draws (snap marker, rubber band) and set its snap
    /// radius from the zoom.
    pub fn pcb_tool(&self) -> &pcb::SelectTool {
        &self.pcb_tool
    }

    pub fn pcb_tool_mut(&mut self) -> &mut pcb::SelectTool {
        &mut self.pcb_tool
    }

    /// Where the cursor last snapped.
    pub fn pcb_snap(&self) -> Option<Snapped> {
        if self.drawing() {
            return self.pcb_draw.snap();
        }
        self.pcb_tool.snap()
    }

    /// Whether a PCB drawing Tool is active (see `pcb::draw`).
    fn drawing(&self) -> bool {
        self.tool().draws_on_board()
    }

    fn with_draw(&mut self, f: impl FnOnce(&mut pcb::draw::Draw, &mut Shell)) {
        let mut draw = std::mem::take(&mut self.pcb_draw);
        f(&mut draw, self);
        self.pcb_draw = draw;
    }

    /// The PCB drawing Tools' state: the preview, a measurement, text waiting for words.
    pub fn pcb_draw(&self) -> &pcb::draw::Draw {
        &self.pcb_draw
    }

    /// Places the text the text Tool was clicked for, with these words (one Command).
    pub fn pcb_draw_text(&mut self, text: &str) {
        self.with_draw(|draw, shell| draw.place_text(shell, text));
    }

    /// Drops the text the text Tool was clicked for.
    pub fn pcb_cancel_text(&mut self) {
        self.pcb_draw.cancel_text();
    }

    /// Board point `at` as the cursor readout gives it: from the Output Origin, if the
    /// board has one.
    pub fn pcb_cursor_from_origin(&self, at: Point) -> Point {
        at - self.board_grid_origin()
    }

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

    /// The rubber band's corners while one is drawn.
    pub fn pcb_band(&self) -> Option<(Point, Point)> {
        self.pcb_tool.band()
    }

    // ---- The routing and Via Tools (see `pcb::route`) ----

    /// The cursor moved to the board point `at` while routing or placing Vias.
    pub fn route_hover(&mut self, at: Point) {
        self.with_router(|r, shell| r.hover(shell, at));
    }

    /// A click at the board point `at` while routing or placing Vias.
    pub fn route_click(&mut self, at: Point) {
        self.with_router(|r, shell| r.click(shell, at));
    }

    /// The route in flight, if any.
    pub fn route_preview(&self) -> Option<pcb::route::Preview> {
        self.router.preview()
    }

    pub fn router(&self) -> &pcb::Router {
        &self.router
    }

    pub fn router_mut(&mut self) -> &mut pcb::Router {
        &mut self.router
    }

    fn with_router(&mut self, f: impl FnOnce(&mut pcb::Router, &mut Shell)) {
        let mut router = std::mem::take(&mut self.router);
        f(&mut router, self);
        self.router = router;
    }

    /// Whether the active Document is the board, whose Tool `tool.rotate` and
    /// `tool.flip_side` turn.
    /// Swap with a pair of Footprints already selected: swaps them at once and goes back
    /// to Select. With no pair the Tool asks for two picks.
    fn swap_selected_pair(&mut self) {
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
            return;
        };
        let pair: Vec<Uuid> = self
            .selection()
            .objects
            .iter()
            .copied()
            .filter(|o| pcb.footprint(*o).is_some())
            .collect();
        if let [a, b] = pair[..]
            && self.selection().objects.len() == 2
        {
            let _ = self.run_reporting("pcb.swap", json!({"a": a, "b": b}));
            let _ = self.run_reporting("tool.activate", json!({"tool": "select"}));
        }
    }

    /// X or Y on the board: refused when they would mirror a Footprint, carried by the
    /// move in flight or else selected. The board has only sides; L flips instead.
    fn board_mirror_refused(&self) -> bool {
        if !self.board_is_active() {
            return false;
        }
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
            return false;
        };
        let objects = match self.pcb_tool.carried() {
            Some(carried) => carried,
            None => &self.selection().objects[..],
        };
        objects.iter().any(|o| pcb.footprint(*o).is_some())
    }

    pub(crate) fn board_is_active(&self) -> bool {
        let pcb = self.engine.project().and_then(|p| p.pcb()).map(|p| p.uuid);
        pcb.is_some() && self.active_document().map(|d| d.document) == pcb
    }

    /// Draws `document` from `project` instead of the open Project: a preview of an edit
    /// in flight. The next refresh (or [`Shell::redraw`]) draws the real one again.
    pub fn show_preview(&mut self, document: Uuid, project: &Project) {
        let scene = self.scene_from(project, document);
        if let Some(tab) = self.document_tab_mut(document) {
            tab.scene = scene;
        }
    }

    /// Draws `document` from the open Project again, as after a dropped preview.
    pub fn redraw(&mut self, document: Uuid) {
        let scene = self.scene_of(document);
        if let Some(tab) = self.document_tab_mut(document) {
            tab.scene = scene;
        }
    }

    /// The open Documents' tabs.
    pub fn document_tabs(&self) -> &[DocumentTab] {
        &self.documents
    }

    /// The tab of the open Document `document`.
    pub fn document_tab(&self, document: Uuid) -> Option<&DocumentTab> {
        self.documents.iter().find(|d| d.document == document)
    }

    /// The tab of an open Document, to update its camera.
    pub fn document_tab_mut(&mut self, document: Uuid) -> Option<&mut DocumentTab> {
        self.documents.iter_mut().find(|d| d.document == document)
    }

    /// The active Document's tab: the Document last interacted with.
    pub fn active_document(&self) -> Option<&DocumentTab> {
        self.document_tab(self.layout().active_document()?)
    }

    pub fn active_document_mut(&mut self) -> Option<&mut DocumentTab> {
        let document = self.layout().active_document()?;
        self.document_tab_mut(document)
    }

    /// Shows the settings window `window`: raises it where it is, or opens it in a window
    /// of its own at `rect`. Returns whether it opened just now.
    pub fn open_settings(&mut self, window: SettingsWindow, rect: Option<WindowRect>) -> bool {
        self.layout_mut().open_settings(window, rect)
    }

    /// Closes the settings window `window`'s tab, as its own buttons do: a view change
    /// with nothing to journal.
    pub fn close_settings(&mut self, window: SettingsWindow) {
        let _ = self.layout_mut().close(&TabKey::Settings(window));
    }

    /// Names the settings window `window`'s tab.
    pub fn set_settings_title(&mut self, window: SettingsWindow, title: String) {
        self.settings_titles.insert(window, title);
    }

    /// The title of the settings window `window`'s tab, once the window named it.
    pub fn settings_title(&self, window: SettingsWindow) -> Option<&str> {
        self.settings_titles.get(&window).map(String::as_str)
    }

    /// The title of `tab`'s Document, if it is an open Document.
    pub fn document_title(&self, tab: &TabKey) -> Option<&str> {
        match tab {
            TabKey::Document(uuid) => self.document_tab(*uuid).map(|d| d.title.as_str()),
            _ => None,
        }
    }

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

    /// Runs a Command an agent sent over the bridge, as [`Shell::run`] does, with its
    /// source (for the Journal and the "MCP: …" undo label) and signals. If it modified
    /// an object the Tool in use depends on, the Tool's preview is cancelled with a note.
    pub fn run_bridged(
        &mut self,
        source: Source,
        id: &str,
        params: Value,
        signals: &Signals,
    ) -> Result<Value, CommandError> {
        if crate::help::USER_ONLY.contains(&id) {
            return Err(CommandError::rejected(format!(
                "only the user can run {id}, from the app"
            )));
        }
        let out = self.engine.run_from(source, id, params, signals)?;
        self.after_run(id);
        if let Some(tool) = &self.tool
            && tool.touched_by(&out)
        {
            self.note = Some(tr!(
                "status-tool-cancelled",
                tool = tool.title.as_str(),
                source = source.label()
            ));
            self.tool = None;
        }
        Ok(out)
    }

    /// Shows the file the OS asked a second app to open, when it belongs to the open
    /// Project: a Document file focuses that Document's tab, the Project file or folder
    /// just the window. Anything else is `rejected`, so that app opens it itself.
    pub fn open_in_place(&mut self, path: &Path) -> Result<(), CommandError> {
        let document = document_file(path);
        let project = match &document {
            Some(_) => path.parent().unwrap_or(path),
            None => path,
        };
        let wanted = store::resolve_project_file(project);
        let same = |a: &Path, b: &Path| match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
            (Ok(a), Ok(b)) => a == b,
            _ => false,
        };
        if !self.project_file().is_some_and(|open| same(&open, &wanted)) {
            return Err(CommandError::rejected(format!(
                "{} is not in the open Project",
                path.display()
            )));
        }
        if let Some((stem, kind)) = document {
            let uuid = self.engine.project().and_then(|p| {
                p.documents()
                    .find(|d| d.kind == kind && d.name == stem)
                    .map(|d| d.uuid)
            });
            let Some(uuid) = uuid else {
                return Err(CommandError::not_found(format!(
                    "the open Project has no Document {}",
                    path.display()
                )));
            };
            self.run("document.open", json!({ "document": uuid }))?;
        }
        Ok(())
    }

    /// The recent Projects from `state.json`, newest first.
    pub fn recent_projects(&self) -> Vec<RecentProject> {
        let Some(dirs) = self.engine.user_dirs() else {
            return Vec::new();
        };
        state::read(dirs)
            .recent_projects
            .into_iter()
            .map(|path| RecentProject {
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                missing: !path.is_file(),
                path,
            })
            .collect()
    }

    /// Forgets a recent Project.
    pub fn remove_recent(&mut self, path: &Path) {
        if let Some(dirs) = self.engine.user_dirs()
            && let Err(e) = state::update(dirs, |s| s.recent_projects.retain(|p| p != path))
        {
            self.message = Some(tr!("error-state-file", error = e.to_string()));
        }
    }

    /// Opens the last Project, when the Preferences ask for it. A last Project that is
    /// gone is not reopened, and the status bar says so.
    pub fn reopen_last_project(&mut self) {
        let Some(dirs) = self.engine.user_dirs() else {
            return;
        };
        if !preferences::current(Some(dirs)).general.reopen_last_project {
            return;
        }
        let Some(last) = self.recent_projects().into_iter().next() else {
            return;
        };
        if last.missing {
            self.message = Some(tr!(
                "status-last-project-missing",
                project = last.name.as_str(),
                path = reda_engine::fs::display_path(&last.path)
            ));
            return;
        }
        let _ = self.open_project(&last.path);
    }

    /// Saves the open Project's view state now, as when it closes.
    pub fn save_view(&mut self) {
        let Some((folder, bytes)) = self.view_bytes() else {
            return;
        };
        // The view state is disposable: a failed write loses nothing worth reporting.
        if view_state::write(&folder, &bytes).is_ok() {
            self.view_saves.saved(bytes);
        }
    }

    /// Saves the open Project's view state once it has stayed unchanged for
    /// [`VIEW_SAVE_DELAY`]. The window calls this every frame.
    pub fn save_view_if_due(&mut self, now: Instant) {
        let Some((folder, bytes)) = self.view_bytes() else {
            return;
        };
        if let Some(bytes) = self.view_saves.poll(bytes, now)
            && view_state::write(&folder, &bytes).is_ok()
        {
            self.view_saves.saved(bytes);
        }
    }

    /// The open Project's folder and its view state as `view.json` bytes.
    fn view_bytes(&self) -> Option<(PathBuf, Vec<u8>)> {
        let folder = self.engine.location()?.to_path_buf();
        let project = self.engine.project()?;
        let mut documents = self.closed_views.clone();
        documents.retain(|uuid, _| project.documents().any(|d| d.uuid == *uuid));
        for tab in &self.documents {
            documents.insert(
                tab.document,
                DocumentView {
                    camera: tab.camera,
                    layers: tab.layers.clone(),
                    ..DocumentView::default()
                },
            );
        }
        let grids = self.engine.view().get::<Grids>();
        let working = self.engine.view().get::<WorkingState>();
        for entry in project.documents() {
            let grid = grids.and_then(|g| g.picked.get(&entry.uuid).copied());
            let snap = working.and_then(|w| w.snap.get(&entry.uuid).cloned());
            let filter = working.map(|w| w.filter(entry.uuid)).unwrap_or_default();
            let view = documents.get_mut(&entry.uuid);
            let unchanged = grid.is_none() && snap.is_none() && filter.off.is_empty();
            let view = match view {
                Some(view) => view,
                None if unchanged => continue,
                None => documents.entry(entry.uuid).or_default(),
            };
            view.grid = grid;
            view.snap_mode = snap.as_ref().map(|s| s.mode);
            view.gridless = snap.as_ref().is_some_and(|s| s.gridless);
            view.hotspots_off = snap.as_ref().map(|s| {
                (s.hotspots_off.iter())
                    .map(|k| crate::snap::hotspot_name(*k).to_owned())
                    .collect()
            });
            view.selection_filter_off = filter.off.iter().map(|k| k.name().to_owned()).collect();
        }
        if let Some(pcb) = project.pcb() {
            let view = documents.entry(pcb.uuid).or_default();
            view.layers = self.board_view().layers.clone();
            view.units = Some(self.board_view().units);
        }
        // The settings windows are not kept: their edits do not outlive the window.
        let mut layout = self.layout().clone();
        layout.retain_tabs(|t| !matches!(t, TabKey::Settings(_)));
        let file = ViewFile::new(layout, documents);
        Some((folder, file.encode()))
    }

    /// Opens the Project at `path`, replacing any open one. `path` is its `.reda` file or
    /// folder, or one of its Document files (`.redasch`, `.redapcb`), which opens the
    /// Project with that Document's tab focused.
    pub fn open_project(&mut self, path: &Path) -> Result<(), CommandError> {
        let document = document_file(path);
        let project = match &document {
            Some(_) => path.parent().unwrap_or(path),
            None => path,
        };
        if let Some(dirs) = self.engine.user_dirs()
            && let Some(pid) = recovery::live_instance(dirs, project)
        {
            // Another REDA has it open: that one keeps it, and no recovery is offered.
            let name = store::resolve_project_file(project);
            let name = name.file_stem().unwrap_or_default().to_string_lossy();
            self.message = Some(tr!(
                "error-project-held",
                project = name.as_ref(),
                pid = pid.to_string()
            ));
            return Ok(());
        }
        self.run_reporting("project.open", json!({ "path": project }))?;
        if let Some((stem, kind)) = document {
            let uuid = self.engine.project().and_then(|p| {
                p.documents()
                    .find(|d| d.kind == kind && d.name == stem)
                    .map(|d| d.uuid)
            });
            if let Some(uuid) = uuid {
                self.run_reporting("document.open", json!({ "document": uuid }))?;
            }
        }
        Ok(())
    }

    /// The Fab Presets a new Project can start from (shipped, then the user's) and the
    /// default pick: the last one used, else Generic.
    pub fn fab_preset_choices(&mut self) -> (Vec<String>, String) {
        use reda_engine::commands::fab_preset::PresetListResult;
        let list = self
            .engine
            .run("fab_preset.list", json!({}))
            .ok()
            .and_then(|v| serde_json::from_value::<PresetListResult>(v).ok());
        match list {
            Some(list) => (
                list.presets.into_iter().map(|p| p.preset.name).collect(),
                list.default,
            ),
            None => (Vec::new(), "Generic".to_owned()),
        }
    }

    /// Creates the Project `name` in a new folder inside `folder`, and opens it.
    pub fn new_project(&mut self, folder: &Path, name: &str) -> Result<(), CommandError> {
        self.run_reporting("project.new", json!({ "path": folder, "name": name }))?;
        Ok(())
    }

    /// Runs a Command, then refreshes the tabs when it may have changed the design or
    /// closed a Document's tab. Opening, creating or saving a Project puts it at the top
    /// of the recent Projects.
    pub fn run(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let base = id.split(':').next().unwrap_or(id);
        if matches!(base, "project.open" | "project.new" | "project.close") {
            self.save_view();
        }
        let kind = self.engine.registry().get(base).map(|meta| meta.kind);
        let params = self.with_wiring(base, params);
        if base == "selection.cycle"
            && let Some(document) = params
                .get("document")
                .and_then(Value::as_str)
                .and_then(|s| s.parse().ok())
        {
            selection::picking::prepare_cycle(self, document);
        }
        if base == "tool.activate" {
            // The Tool stack needs to know whether a run is in flight.
            let run = self
                .active_sheet()
                .is_some_and(|sheet| crate::schematic_editor::run_in_flight(self, sheet));
            if let Some(stack) = self.engine.view_mut().get_mut::<tools::ToolStack>() {
                stack.run = run;
            }
        }
        // Delete while a Wire vertex's handle is held removes that vertex instead.
        if base == "schematic.delete"
            && let Some(sheet) = self.active_sheet()
            && let Some(commands) = crate::schematic_editor::delete_held_vertex(self, sheet)
        {
            let mut out = json!({});
            for (id, params) in commands {
                out = self.run(&id, params)?;
            }
            return Ok(out);
        }
        if matches!(base, "tool.mirror_x" | "tool.mirror_y") && self.board_mirror_refused() {
            return Err(CommandError::rejected(
                reda_engine::pcb::place::MIRROR_REFUSED,
            ));
        }
        // Tab skips a Footprint while Reposition Selected runs, and widens otherwise.
        if base == "pcb.selection.widen" && self.pcb_tool.queue().is_some() {
            return self.run("pcb.place.skip", json!({}));
        }
        if base == "pcb.copper.delete" && self.pcb_tool.polygon_edit().is_some() {
            // Del while holding a polygon's vertex removes the vertex, not the selection.
            let held = self.with_pcb_tool(|tool, shell| tool.delete_held_vertex(shell));
            if held {
                return Ok(json!({}));
            }
        }
        let silkscreen = crate::pcb::silkscreen::COMMANDS
            .contains(&base)
            .then(|| params.clone());
        let out = self.engine.run(id, params.clone());
        if !matches!(kind, Some(Kind::View | Kind::Query)) {
            // A move put down shows until its Command has run, whatever the outcome.
            self.end_committed_moves();
        }
        if let Err(e) = &out {
            self.loop_retry = crate::pcb::board_shapes::retry_for(id, &params, e);
        }
        let out = out?;
        if base == "pcb.route.unroute" {
            pcb::placing::ask_about_locked_unroute(self, &params, &out);
        }
        self.after_edit(base, &out);
        self.after_run(id);
        if matches!(base, "library.element.open" | "library.element.new") {
            crate::element_editor::after_open(self, &out);
        }
        self.after_element_run(base, &out);
        if let Some(params) = silkscreen {
            crate::pcb::silkscreen::after_run(self, base, &params, &out);
        }
        Ok(out)
    }

    /// `params` for Command `id` with its `wiring` filled in from the wiring Preferences
    /// and the sheet's snap grid, when `id` takes one and the caller gave none. Agents'
    /// Commands keep the fixed defaults, so their results don't depend on this machine.
    fn with_wiring(&self, id: &str, mut params: Value) -> Value {
        if !reda_engine::schematic::WIRING_COMMANDS.contains(&id) {
            return params;
        }
        let document = (params.get("document"))
            .and_then(Value::as_str)
            .and_then(|s| s.parse::<Uuid>().ok())
            .or_else(|| self.active_document().map(|t| t.document));
        let Some(map) = params.as_object_mut() else {
            return params;
        };
        if map.contains_key("wiring") {
            return params;
        }
        let grid = document.map_or(grid::SCHEMATIC_DEFAULT, |d| self.grid(d));
        let wiring = self.preferences().schematic.wiring(grid);
        map.insert(
            "wiring".to_owned(),
            serde_json::to_value(wiring).expect("wiring serialises"),
        );
        params
    }

    /// What the Library editors do once `id` ran (with result `out`) and the tabs are
    /// refreshed: show the Unit picked, open Edit in Library's Symbol, check a saved
    /// element into Messages, and reopen restored elements in their tabs.
    fn after_element_run(&mut self, id: &str, out: &Value) {
        let uuid = |v: &Value| v.as_str().and_then(|s| s.parse::<Uuid>().ok());
        match id {
            "library.symbol.unit.show" => {
                if let Some(symbol) = uuid(&out["symbol"]) {
                    let scene = self.scene_of(symbol);
                    if let Some(tab) = self.document_tab_mut(symbol) {
                        tab.scene = scene;
                    }
                }
            }
            "library.element.edit_in_library" => {
                if let Some(unit) = uuid(&out["object"]) {
                    crate::symbol_editor::edit_in_library(self, unit);
                }
            }
            "library.element.save" => crate::element_editor::check_after_save(self),
            "library.element.copy" => {
                let copy = out["name"].as_str().unwrap_or_default().to_owned();
                self.set_note(tr!("to-library-copied", copy = copy));
            }
            "library.extract_from_project" => {
                let count = |key: &str| out[key].as_array().map_or(0, Vec::len);
                self.set_note(tr!(
                    "to-library-extracted",
                    written = count("written"),
                    skipped = count("skipped")
                ));
            }
            "library.import_part" => {
                if let Some(notice) = out["notice"].as_str() {
                    self.notice = Some(notice.to_owned());
                }
                crate::element_editor::report_created(self, out);
            }
            _ if reda_engine::library::GENERATORS.contains(&id) => {
                crate::element_editor::report_created(self, out);
            }
            "recovery.restore" => {
                let untabbed: Vec<Uuid> = crate::element_editor::open_elements(&self.engine)
                    .into_iter()
                    .filter(|e| self.layout().place(&TabKey::Document(*e)).is_none())
                    .collect();
                for element in untabbed {
                    let _ = self.run("document.open", json!({ "document": element }));
                }
            }
            _ => {}
        }
    }

    /// Refreshes what a successful run of `id` may have changed.
    fn after_run(&mut self, id: &str) {
        let base = id.split(':').next().unwrap_or(id);
        // First, so the Scenes the refresh below rebuilds are drawn with them.
        if matches!(
            base,
            "app.preferences.set"
                | "app.preferences.reset"
                | "view.theme.set"
                | "view.ui_scale.set"
                | "view.ui_scale.step"
        ) {
            self.reload_preferences();
        }
        match base {
            "project.open" | "project.new" => {
                self.show_project();
                self.recorder.opened(&self.engine);
                if base == "project.open" {
                    self.run_health_checks();
                }
            }
            "project.close" => {
                self.engine.view_mut().set(Selection::default());
                self.closed_views.clear();
                self.refresh();
                self.layout_mut().open_start();
                // The closed Project's recovery folder goes; open Library elements
                // snapshot into the per-user one from now on.
                self.recorder.opened(&self.engine);
            }
            _ => match self.engine.registry().get(base).map(|meta| meta.kind) {
                Some(Kind::Query) => {}
                Some(Kind::View) => self.sync_document_tabs(),
                Some(Kind::Edit) => {
                    self.refresh();
                    self.auto_refill();
                }
                _ => self.refresh(),
            },
        }
        if base.starts_with("library.")
            || base.starts_with("project.")
            || matches!(base, "app.preferences.set" | "app.preferences.reset")
        {
            self.library_generation += 1;
        }
        if base.starts_with("app.preferences.") {
            self.follow_grid_preferences();
        }
        self.sync_selection();
        self.sync_messages(base);
        self.sync_board_view();
        self.note_focus();
        if matches!(base, "project.open" | "project.new" | "project.save") {
            self.remember_project();
        }
    }

    /// Starts new views on the snap grids the Preferences now pick.
    fn follow_grid_preferences(&mut self) {
        let prefs = preferences::current(self.engine.user_dirs());
        if let Some(grids) = self.engine.view_mut().get_mut::<Grids>() {
            grids.follow(&prefs);
        }
        if let Some(working) = self.engine.view_mut().get_mut::<WorkingState>() {
            working.follow(&prefs);
        }
    }

    /// The Undo stack `Mod+Z` uses: a focused Document tab's own, else the Project's.
    /// The Projects panel (and any Panel that is not about one Document) uses the
    /// Project stack. An open dialog names the stack its changes go on (`dialog`): the
    /// Project's for Project Settings, the PCB Document's for Board Setup.
    pub fn undo_scope(&self, dialog: Option<Scope>) -> Option<Scope> {
        if self.engine.project().is_none() {
            // With no Project open, only open Library elements have stacks.
            return match &self.layout().focus {
                Some(TabKey::Document(uuid)) if self.engine.element(*uuid).is_some() => {
                    Some(Scope::Document(*uuid))
                }
                _ => self
                    .active_document()
                    .filter(|t| self.engine.element(t.document).is_some())
                    .map(|t| Scope::Document(t.document)),
            };
        }
        if dialog.is_some() {
            return dialog;
        }
        match &self.layout().focus {
            Some(TabKey::Document(uuid)) => Some(Scope::Document(*uuid)),
            // Board Setup's changes go on the PCB Document's stack.
            Some(TabKey::Settings(SettingsWindow::BoardSetup)) => {
                let pcb = self.engine.project().and_then(|p| p.pcb());
                Some(pcb.map_or(Scope::Project, |pcb| Scope::Document(pcb.uuid)))
            }
            _ => Some(Scope::Project),
        }
    }

    /// Undoes (or with `redo`, redoes) on the stack [`Shell::undo_scope`] picks.
    pub fn undo(&mut self, redo: bool, dialog: Option<Scope>) -> Result<Value, CommandError> {
        let Some(scope) = self.undo_scope(dialog) else {
            return Err(CommandError::unavailable("no Project is open"));
        };
        let document = match scope {
            Scope::Document(uuid) => json!(uuid),
            _ => json!("project"),
        };
        let id = if redo { "edit.redo" } else { "edit.undo" };
        self.run_reporting(id, json!({ "document": document }))
    }

    /// [`Shell::run`], keeping any error as the message the status bar shows.
    pub fn run_reporting(&mut self, id: &str, params: Value) -> Result<Value, CommandError> {
        let out = self.run(id, params);
        match &out {
            Ok(_) => self.message = None,
            Err(e) => self.message = Some(tr!("error-command", error = e.message.clone())),
        }
        out
    }

    /// Saves the session's Journal to `path` as a script that `reda run --script`
    /// replays. View Commands are left out: only the app can run them. A failure is also
    /// kept as the message the status bar shows.
    pub fn export_journal(&mut self, path: &Path) -> std::io::Result<()> {
        let script = crate::journal_panel::script(&self.engine);
        let out = reda_engine::fs::write_atomic(path, script.as_bytes());
        if let Err(e) = &out {
            self.message = Some(tr!("journal-export-failed", error = e.to_string()));
        }
        out
    }

    /// Shows `note` in the status bar: news, not an error.
    pub(crate) fn set_note(&mut self, note: String) {
        self.note = Some(note);
    }

    /// Journals a run detached from the engine (see [`JobRunner`](crate::job_run::JobRunner))
    /// and returns its outcome.
    pub fn finish_detached(&mut self, done: reda_engine::Finished) -> Result<Value, CommandError> {
        self.engine.finish(done)
    }

    /// The Keymap in force: the base Keymap the Preferences pick, with the user's
    /// `keymap.toml` laid over it.
    pub fn keymap(&self) -> &Keymap {
        &self.keymap
    }

    /// Reads the Keymap again (after `keymap.toml` or `keymap.base` changed) and lists its
    /// problems in Messages, replacing the earlier ones.
    pub fn reload_keymap(&mut self) {
        let dirs = self.engine.user_dirs();
        let base = preferences::current(dirs).keymap.base;
        let mut problems = Vec::new();
        let user = dirs.and_then(|d| {
            let path = d.config.join(keymap::USER_FILE);
            match std::fs::read_to_string(&path) {
                Ok(text) => Some(text),
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => {
                    problems.push(tr!(
                        "keymap-unreadable",
                        file = keymap::USER_FILE,
                        error = e.to_string()
                    ));
                    None
                }
            }
        });
        let (mut map, found) = Keymap::load(base, user.as_deref());
        problems.extend(found);
        problems.extend(map.check(self.engine.registry()));
        self.keymap = map;
        let problems = problems
            .into_iter()
            .map(|text| (Severity::Error, text))
            .collect();
        self.set_load_messages(KEYMAP, problems);
    }

    /// The Preferences as last read: after this window's own changes at once, and after
    /// another instance's once [`Shell::poll_preferences`] notices them.
    pub fn preferences(&self) -> &Preferences {
        self.preferences.values()
    }

    /// Moves every time [`Shell::preferences`] change, so the window applies the theme and
    /// UI scale once per change.
    pub fn preferences_generation(&self) -> u64 {
        self.preferences.generation()
    }

    /// Reads `preferences.toml` again if it changed on disk (another instance, or a hand
    /// edit) and applies what changed. Returns whether the Preferences changed.
    pub fn poll_preferences(&mut self) -> bool {
        match self.engine.user_dirs() {
            Some(dirs) if self.preferences.changed_on_disk(dirs) => {
                let changed = self.reload_preferences();
                if changed {
                    // Layer colours and the Inactive-Layer mode are in the Scenes.
                    self.refresh();
                    self.sync_selection();
                }
                changed
            }
            _ => false,
        }
    }

    /// Reads `preferences.toml` again and applies what changed: the undo depth, the base
    /// Keymap, the Layer colours and the warnings in Messages. Returns whether the
    /// Preferences changed.
    fn reload_preferences(&mut self) -> bool {
        let Some(dirs) = self.engine.user_dirs().cloned() else {
            return false;
        };
        let Some(reloaded) = self.preferences.reload(&dirs) else {
            return false;
        };
        let now = self.preferences.values().clone();
        self.sync_ui_scale();
        if now.general.undo_depth != reloaded.before.general.undo_depth {
            (self.engine).set_undo_depth(now.general.undo_depth.map(|d| d.get() as usize));
        }
        if now.keymap != reloaded.before.keymap {
            self.reload_keymap();
        }
        self.generation += 1;
        self.palette = pcb::view::palette_handle(&now, self.generation);
        self.set_preference_warnings(reloaded.warnings);
        if now.checks != reloaded.before.checks {
            self.update_markers();
        }
        true
    }

    /// Lists the warnings reading `preferences.toml` gave.
    fn load_preference_warnings(&mut self) {
        let (watched, warnings) = Watched::load(self.engine.user_dirs());
        self.preferences = watched;
        self.sync_ui_scale();
        if self.engine.user_dirs().is_some() {
            self.set_preference_warnings(warnings);
        }
    }

    /// Keeps the UI scale view state, which `view.ui_scale.step` steps from, equal to the
    /// Preference.
    fn sync_ui_scale(&mut self) {
        let scale = self.preferences.values().general.ui_scale;
        self.engine
            .view_mut()
            .set(crate::bars::commands::UiScale(scale));
    }

    fn set_preference_warnings(&mut self, warnings: Vec<String>) {
        let notes = warnings
            .into_iter()
            .map(|w| Note {
                source: PREFERENCES.to_owned(),
                document: None,
                severity: Severity::Warning,
                text: tr!("message-preferences", warning = w),
            })
            .collect();
        self.messages.set_notes(PREFERENCES, notes);
    }

    /// Puts the open Project at the top of the recent Projects in `state.json`.
    fn remember_project(&mut self) {
        if let (Some(dirs), Some(file)) = (self.engine.user_dirs(), self.project_file())
            && let Err(e) = state::update(dirs, |s| s.add_recent_project(&file))
        {
            self.message = Some(tr!("error-state-file", error = e.to_string()));
        }
    }

    /// The open Project's `.reda` file, canonical when it exists.
    fn project_file(&self) -> Option<PathBuf> {
        let folder = self.engine.location()?;
        let name = &self.engine.project()?.props().name;
        let file = store::project_file_path(folder, name);
        Some(std::fs::canonicalize(&file).unwrap_or(file))
    }

    /// Shows the open Project as its `view.json` left it, or else in the default layout
    /// with its first sheet in the centre.
    fn show_project(&mut self) {
        let Some(project) = self.engine.project() else {
            return;
        };
        let titles: BTreeMap<Uuid, String> = project
            .documents()
            .map(|d| (d.uuid, d.title.clone()))
            .collect();
        let first_sheet = project
            .documents()
            .find(|d| d.kind == DocumentKind::Schematic)
            .map(|d| d.uuid);
        let restored = self.engine.location().and_then(view_state::read);
        let (layout, mut views) = match restored {
            Some(file) => {
                let mut layout = file.layout;
                layout.retain_tabs(|t| match t {
                    TabKey::Document(uuid) => titles.contains_key(uuid),
                    TabKey::Start | TabKey::Settings(_) => false,
                    TabKey::Panel(_) => true,
                });
                (layout, file.documents)
            }
            None => (self.default_layout(first_sheet), BTreeMap::new()),
        };
        let pcb = self.engine.project().and_then(|p| p.pcb()).map(|p| p.uuid);
        let prefs = preferences::current(self.engine.user_dirs());
        let units = prefs.pcb.default_units;
        let mut board = BoardView::new(units);
        if let Some(view) = pcb.and_then(|pcb| views.get(&pcb)) {
            board.layers = view.layers.clone();
            board.units = view.units.unwrap_or(units);
        }
        *self.board_view_mut() = board;
        // Views start on the grids the Preferences pick, unless view.json has their own.
        let kinds: BTreeMap<Uuid, DocumentKind> = self
            .engine
            .project()
            .map(|p| p.documents().map(|d| (d.uuid, d.kind)).collect())
            .unwrap_or_default();
        let mut working = WorkingState::following(&preferences::current(self.engine.user_dirs()));
        let mut picked = BTreeMap::new();
        for (document, view) in &views {
            let Some(kind) = kinds.get(document).copied() else {
                continue;
            };
            if let Some(g) = (view.grid).filter(|g| {
                grid::tables_in(&prefs.editing.grid, kind.into())
                    .row(*g)
                    .is_some()
            }) {
                picked.insert(*document, g);
            }
            let ty = kind.into();
            let mode = view
                .snap_mode
                .filter(|m| working_state::SnapMode::offered(ty).contains(m));
            if mode.is_some() || view.gridless || view.hotspots_off.is_some() {
                let mut snap = working.default_snap(ty);
                snap.mode = mode.unwrap_or(snap.mode);
                snap.gridless = view.gridless;
                if let Some(off) = &view.hotspots_off {
                    let offered = crate::snap::offered(ty);
                    snap.hotspots_off = (offered.iter())
                        .filter(|k| off.contains(crate::snap::hotspot_name(**k)))
                        .copied()
                        .collect();
                }
                working.snap.insert(*document, snap);
            }
            let list = working_state::filter_kinds(ty);
            let off: BTreeSet<_> = (view.selection_filter_off.iter())
                .filter_map(|name| working_state::FilterKind::parse(name))
                .filter(|k| list.contains(k))
                .collect();
            if !off.is_empty() {
                working.filters.insert(*document, SelectionFilter { off });
            }
        }
        if let Some(grids) = self.engine.view_mut().get_mut::<Grids>() {
            grids.follow(&prefs);
            grids.picked = picked;
        }
        self.engine.view_mut().set(working);
        self.documents.clear();
        let open: Vec<Uuid> = layout
            .groups()
            .into_iter()
            .flat_map(|(_, g)| g.tabs.iter())
            .filter_map(|t| match t {
                TabKey::Document(uuid) => Some(*uuid),
                _ => None,
            })
            .collect();
        for document in open {
            let view = views.remove(&document).unwrap_or_default();
            let scene = self.scene_of(document);
            let mut tab = DocumentTab::new(document, titles[&document].clone(), scene);
            tab.camera = view.camera;
            tab.layers = view.layers;
            self.documents.push(tab);
        }
        self.closed_views = views;
        self.replace_layout(layout);
        // Library elements opened before the Project stay open in their tabs.
        for element in crate::element_editor::open_elements(&self.engine) {
            if self.layout().place(&TabKey::Document(element)).is_none() {
                self.layout_mut().open_document(element);
            }
            if self.document_tab(element).is_none() {
                let title = crate::element_editor::title(&self.engine, element);
                let scene = self.scene_of(element);
                let tab = DocumentTab::new(element, title.unwrap_or_default(), scene);
                self.documents.push(tab);
            }
        }
        self.engine.view_mut().set(Selection::default());
        self.sync_selection();
        self.messages.clear_runs();
        self.load_fonts();
        self.change_list = self.engine.project().and_then(reda_engine::sync::plan);
        let bytes = self.view_bytes().map(|(_, bytes)| bytes);
        self.view_saves = Debounce::default();
        if let Some(bytes) = bytes {
            self.view_saves.saved(bytes);
        }
    }

    /// The layout a Project opens in without saved view state: the user's default Saved
    /// Layout when there is a usable one, else the Default Layout, with `document` open.
    fn default_layout(&self, document: Option<Uuid>) -> Layout {
        let mut layout = Layout::with_centre(Vec::new());
        if let Some(document) = document {
            layout.open_document(document);
        }
        let dirs = self.engine.user_dirs();
        let chosen = preferences::current(dirs).general.default_layout;
        let saved = dirs
            .zip(chosen)
            .and_then(|(dirs, name)| saved::load(dirs, &name).ok())
            .and_then(|saved| saved.apply_to(&layout).ok());
        saved.unwrap_or(layout)
    }

    /// Replaces the layout with `layout`. Preferences is per-user, so an open Preferences
    /// window stays open, where it was.
    fn replace_layout(&mut self, mut layout: Layout) {
        let preferences = TabKey::Settings(SettingsWindow::Preferences);
        let keep = self.layout().place(&preferences).map(|_| {
            let window = self.layout().window_of(&preferences);
            (self.layout().windows.iter())
                .find(|w| Some(w.id) == window)
                .and_then(|w| w.rect)
        });
        if let Some(rect) = keep
            && layout.place(&preferences).is_none()
        {
            layout.open_settings(SettingsWindow::Preferences, rect);
        }
        *self.layout_mut() = layout;
    }

    /// Rebuilds every tab's Scene and closes tabs whose Document is gone. With no
    /// Project open, the centre shows the Start page again.
    pub(crate) fn refresh(&mut self) {
        let elements: Vec<(Uuid, String)> = crate::element_editor::open_elements(&self.engine)
            .into_iter()
            .filter_map(|e| Some((e, crate::element_editor::title(&self.engine, e)?)))
            .collect();
        let project = self.engine.project();
        if project.is_none() && elements.is_empty() {
            self.documents.clear();
            self.replace_layout(Layout::default());
            self.change_list = None;
            self.messages.clear_runs();
            self.messages.set_notes(FONTS, Vec::new());
            self.messages.set_notes(MISSING_DOCUMENTS, Vec::new());
            return;
        }
        let no_project = project.is_none();
        let mut open: Vec<(Uuid, String)> = project
            .iter()
            .flat_map(|p| p.documents())
            .map(|d| (d.uuid, d.title.clone()))
            .collect();
        open.extend(elements);
        self.layout_mut().retain_tabs(|t| match t {
            // The Start page stays beside Library element tabs while no Project is open.
            TabKey::Start => no_project,
            TabKey::Document(uuid) => open.iter().any(|(u, _)| u == uuid),
            TabKey::Panel(_) => true,
            // The Project's settings windows close with it.
            TabKey::Settings(window) => !(window.of_project() && no_project),
        });
        self.forget_closed_documents();
        let mut documents = std::mem::take(&mut self.documents);
        for d in &mut documents {
            d.scene = self.scene_of(d.document);
            if let Some((_, title)) = open.iter().find(|(u, _)| *u == d.document) {
                d.title = title.clone();
            }
        }
        self.documents = documents;
        self.load_fonts();
        self.zones_stale = self
            .engine
            .project()
            .and_then(|p| p.pcb())
            .is_some_and(|pcb| {
                let fills = self.engine.project().and_then(|p| p.zone_fills(pcb.uuid));
                reda_engine::pcb::zone::status(pcb, fills).needs_refill
            });
        self.change_list = self.engine.project().and_then(reda_engine::sync::plan);
        self.edits += 1;
        self.mark_stale_runs();
        self.update_markers();
    }

    /// Matches the Document tabs to the layout after a view Command: drops the state of
    /// closed tabs and starts it for tabs `document.open` added.
    fn sync_document_tabs(&mut self) {
        self.forget_closed_documents();
        let opened: Vec<Uuid> = self
            .layout()
            .groups()
            .into_iter()
            .flat_map(|(_, g)| g.tabs.iter())
            .filter_map(|t| match t {
                TabKey::Document(uuid) if self.document_tab(*uuid).is_none() => Some(*uuid),
                _ => None,
            })
            .collect();
        for document in opened {
            let title = self
                .engine
                .project()
                .and_then(|p| p.document(document))
                .map(|d| d.title.clone())
                .or_else(|| crate::element_editor::title(&self.engine, document))
                .unwrap_or_default();
            let scene = self.scene_of(document);
            let mut tab = DocumentTab::new(document, title, scene);
            // A Document opened again comes back as it was last seen.
            if let Some(view) = self.closed_views.remove(&document) {
                tab.camera = view.camera;
                tab.layers = view.layers;
            }
            self.documents.push(tab);
        }
        self.sync_selection();
    }

    /// Keeps the clipboard from a copy or cut, cancels what is in flight when the Tool
    /// changes, selects what a paste or duplicate made and drops what a delete or cut
    /// removed from the selection.
    fn after_edit(&mut self, id: &str, out: &Value) {
        match id {
            "schematic.copy" | "pcb.copy" => self.clipboard = Some(out.clone()),
            "schematic.cut" => self.clipboard = Some(out["result"].clone()),
            // Escape (and any Tool change) puts back what the canvases have in flight.
            "tool.activate" if out["run"] == "ended" => {
                if let Some(sheet) = self.active_sheet() {
                    for (id, params) in crate::schematic_editor::end_run(self, sheet) {
                        let _ = self.run_reporting(&id, params);
                    }
                }
            }
            "tool.activate" | "tool.rotate_about" => {
                if out["run"] == "suspended"
                    && let Some(sheet) = self.active_sheet()
                {
                    crate::schematic_editor::suspend(self, sheet);
                }
                selection::picking::cancel(self);
                let documents: Vec<Uuid> = self.documents.iter().map(|t| t.document).collect();
                for document in documents {
                    crate::schematic_editor::cancel(self, document);
                    crate::element_editor::cancel(self, document);
                }
                self.pcb_cancel();
                self.with_router(|r, shell| r.cancel(shell));
                crate::placement::refresh(self);
                let tool = self.tool();
                self.pcb_draw.start(tool);
                if tool == Tool::Swap {
                    self.swap_selected_pair();
                }
                if out["run"] == "resumed"
                    && let Some(sheet) = self.active_sheet()
                {
                    crate::schematic_editor::resume(self, sheet);
                }
            }
            // The Symbol editor's Pin Tool turns the next Pin, and its polygon takes back
            // a corner or closes.
            "tool.rotate" | "tool.undo_segment" | "tool.finish"
                if crate::element_editor::active_symbol(self).is_some() =>
            {
                let symbol = crate::element_editor::active_symbol(self).expect("checked");
                match id {
                    "tool.rotate" => crate::symbol_editor::rotate(self, symbol),
                    "tool.undo_segment" => crate::symbol_editor::undo_segment(self, symbol),
                    _ => {
                        if let Some((id, params)) = crate::symbol_editor::finish(self, symbol) {
                            let _ = self.run_reporting(&id, params);
                        }
                    }
                }
            }
            "tool.cycle_corner" if self.tool() == Tool::Route => {
                self.with_router(|r, shell| r.cycle_corner(shell))
            }
            "tool.cycle_corner"
            | "tool.undo_segment"
            | "tool.cycle_wire_mode"
            | "tool.insert_vertex"
            | "tool.finish"
                if self.tool() == Tool::Wire && self.active_sheet().is_some() =>
            {
                let sheet = self.active_sheet().expect("checked above");
                let commands = match id {
                    "tool.cycle_corner" => {
                        crate::schematic_editor::corner(self, sheet);
                        Vec::new()
                    }
                    "tool.undo_segment" => {
                        crate::schematic_editor::remove_last(self, sheet);
                        Vec::new()
                    }
                    "tool.cycle_wire_mode" => {
                        crate::schematic_editor::restate_wiring(self, sheet);
                        Vec::new()
                    }
                    "tool.insert_vertex" => crate::schematic_editor::insert_vertex(self, sheet),
                    _ => crate::schematic_editor::fix_corner(self, sheet),
                };
                for (id, params) in commands {
                    let _ = self.run_reporting(&id, params);
                }
            }
            "tool.cycle_cutting_length" => {
                let (id, params) = crate::schematic_editor::cycle_cutting_length(self);
                let _ = self.run_reporting(&id, params);
            }
            // With no segment held, Insert resets the HUD's delta, as it does off a sheet.
            "tool.insert_vertex" if self.tool() == Tool::Select => {
                let held = self
                    .active_sheet()
                    .is_some_and(|sheet| crate::schematic_editor::insert_held_vertex(self, sheet));
                if !held && let Some(sheet) = self.active_sheet() {
                    crate::hud::reset(self, sheet);
                }
            }
            "view.hud.reset_delta" => crate::hud::apply_resets(self),
            // A custom grid's range at or above its step is kept, with this advice.
            "view.grid.custom" => {
                if let Some(warning) = out["warning"].as_str() {
                    self.set_note(warning.to_owned());
                }
            }
            "tool.edit_properties" if self.tool() == Tool::Place => {
                let _ = self.run_reporting("panel.show:properties", json!({}));
            }
            "tool.undo_segment" | "tool.finish" if self.sheet_drawing().is_some() => {
                let sheet = self.sheet_drawing().expect("checked above");
                if id == "tool.finish" {
                    for (id, params) in crate::schematic_editor::finish(self, sheet) {
                        let _ = self.run_reporting(&id, params);
                    }
                } else {
                    crate::schematic_editor::undo_point(self, sheet);
                }
            }
            "tool.undo_segment" if self.tool() == Tool::Route => {
                self.with_router(|r, shell| r.undo_segment(shell))
            }
            "pcb.route.add_via" => self.with_router(|r, shell| r.add_via(shell)),
            "pcb.route.via_end" => self.with_router(|r, shell| r.via_end(shell)),
            "pcb.route.end" => self.with_router(|r, shell| r.end(shell)),
            "pcb.route.mode" => {
                if let Ok(mode) = serde_json::from_value(out["mode"].clone()) {
                    self.with_router(|r, shell| r.change_mode(shell, mode));
                }
            }
            "pcb.route.chooser" => {
                if let Ok(kind) = serde_json::from_value(out["kind"].clone()) {
                    self.with_router(|r, shell| r.open_chooser(shell, kind));
                }
            }
            "pcb.route.via_size" => {
                if let (Some(d), Some(h)) = (out["diameter"].as_i64(), out["hole"].as_i64()) {
                    self.with_router(|r, shell| r.set_via_size(shell, Nm(d), Nm(h)));
                }
            }
            "pcb.route.finish" => self.with_router(|r, shell| r.finish(shell)),
            "pcb.drag.mode" => {
                if let Ok(mode) = serde_json::from_value(out["mode"].clone()) {
                    self.with_pcb_tool(|t, shell| t.drag_mode(shell, mode));
                }
            }
            // Unwinding hands the selection on to the neighbour, if there is one.
            "pcb.copper.unwind" => {
                let objects: Vec<Value> = out["result"]["next"]
                    .as_str()
                    .map(|n| vec![json!(n)])
                    .unwrap_or_default();
                let _ = self
                    .engine
                    .run("selection.set", json!({ "objects": objects }));
            }
            "pcb.route.width" => {
                let width = out["width"].as_i64().map(Nm);
                let step = match out["step"].as_str() {
                    Some("next") => Some(1),
                    Some("previous") => Some(-1),
                    _ => None,
                };
                self.with_router(|r, shell| r.set_width(shell, width, step));
            }
            "tool.undo_segment" if self.drawing() => self.pcb_draw.undo_segment(),
            "tool.finish" if self.drawing() => self.with_draw(|draw, shell| draw.finish(shell)),
            // What the placement Tool carries turned in the Command; draw it anew.
            "tool.place" | "tool.place.set" => crate::placement::refresh(self),
            // A Part being placed takes Space, X, Y and L; the selection stays as it is.
            "tool.rotate" | "tool.mirror_x" | "tool.mirror_y" | "tool.flip_side"
                if crate::placement::placing(self).is_some() =>
            {
                crate::placement::refresh(self);
            }
            "tool.rotate" if self.board_is_active() => self.pcb_rotate(),
            "tool.rotate" | "tool.mirror_x" | "tool.mirror_y" => {
                if let Some(sheet) = self.active_sheet() {
                    match id {
                        "tool.mirror_x" => self.sheet_turn(sheet, Turn::MirrorX),
                        "tool.mirror_y" => self.sheet_turn(sheet, Turn::MirrorY),
                        _ => self.sheet_turn(sheet, Turn::Rotate),
                    }
                }
            }
            "tool.flip_side" if self.board_is_active() => self.pcb_flip(),
            "tool.move_board_shape" => self.pcb_move_board_shape(false),
            "tool.move_board" => self.pcb_move_board_shape(true),
            id if id.starts_with("pcb.place.") => pcb::placing::after_run(self, id, out),
            "schematic.paste" | "schematic.duplicate" | "pcb.paste" => {
                let objects = out["result"]["objects"].clone();
                let _ = self
                    .engine
                    .run("selection.set", json!({ "objects": objects }));
            }
            _ => {}
        }
        let Some(project) = self.engine.elements() else {
            return;
        };
        let gone: Vec<Uuid> = self
            .selection()
            .objects
            .iter()
            .copied()
            .filter(|o| {
                project.object(*o).is_none() && !crate::element_editor::has_part(&self.engine, *o)
            })
            .collect();
        // A deleted segment, or one its Wire no longer has, is no longer selected.
        let segments: Vec<_> = (self.selection().segments.iter().copied())
            .filter(|s| {
                id != "schematic.delete"
                    && project.object(s.wire).is_some_and(|o| match o {
                        reda_engine::model::Object::Wire(w) => {
                            s.index + 1 < w.object.vertices.len()
                        }
                        _ => false,
                    })
            })
            .collect();
        if !gone.is_empty()
            && let Some(selection) = self.engine.view_mut().get_mut::<Selection>()
        {
            selection.objects.retain(|o| !gone.contains(o));
        }
        if segments != self.selection().segments
            && let Some(selection) = self.engine.view_mut().get_mut::<Selection>()
        {
            selection.segments = segments;
        }
        let unlit: Vec<Uuid> = match self.engine.project() {
            Some(project) => self
                .highlight()
                .objects
                .iter()
                .copied()
                .filter(|o| project.object(*o).is_none())
                .collect(),
            None => Vec::new(),
        };
        if !unlit.is_empty()
            && let Some(highlight) = self.engine.view_mut().get_mut::<Highlight>()
        {
            highlight.objects.retain(|o| !unlit.contains(o));
            if highlight.objects.is_empty() {
                highlight.net = None;
            }
        }
    }

    /// Drops the moves and Wire runs put down on every canvas, once their Command has run.
    fn end_committed_moves(&mut self) {
        for tab in &mut self.documents {
            crate::schematic_editor::end_commit(tab);
        }
    }

    /// The active sheet while its Line or Rect Tool is active.
    fn sheet_drawing(&self) -> Option<Uuid> {
        matches!(self.tool(), Tool::Line | Tool::Rect)
            .then(|| self.active_sheet())
            .flatten()
    }

    /// The active Document when it is a Schematic Sheet.
    pub fn active_sheet(&self) -> Option<Uuid> {
        let document = self.active_document()?.document;
        self.engine.project()?.schematic_sheet(document)?;
        Some(document)
    }

    /// Space, X or Y on a sheet: turns what its canvas carries, else the selection.
    fn sheet_turn(&mut self, document: Uuid, how: Turn) {
        if crate::schematic_editor::turn(self, document, how) {
            return;
        }
        let objects = self.selection().objects.clone();
        if objects.is_empty() {
            return;
        }
        let _ = match how {
            Turn::Rotate => self.run_reporting("schematic.rotate", json!({ "objects": objects })),
            Turn::MirrorX | Turn::MirrorY => {
                let axis = if how == Turn::MirrorX { "x" } else { "y" };
                self.run_reporting(
                    "schematic.mirror",
                    json!({ "objects": objects, "axis": axis }),
                )
            }
        };
    }

    /// Hands every canvas the [`Selection`]'s objects and the highlighted ones (see
    /// [`cross_probe::marks`]) to mark, with a new generation when they changed.
    fn sync_selection(&mut self) {
        let switches = self.cross_probe_switches();
        let key = (
            self.selection().clone(),
            self.highlight().clone(),
            self.edits,
            switches,
        );
        if self.probed.as_ref() != Some(&key) {
            let highlighted = match self.engine.project() {
                Some(project) => cross_probe::marks(project, &key.0, &key.1, switches),
                None => key.1.objects.clone(),
            };
            if self.marked.objects[..] != key.0.canvas_ids()[..]
                || self.marked.highlighted[..] != highlighted[..]
                || self.marked.mask != key.1.mask
            {
                self.generation += 1;
                self.marked = SelectionHandle {
                    generation: self.generation,
                    objects: Arc::from(key.0.canvas_ids()),
                    highlighted: Arc::from(highlighted),
                    mask: key.1.mask,
                };
            }
            self.probed = Some(key);
        }
        let grids = self
            .engine
            .view()
            .get::<Grids>()
            .cloned()
            .unwrap_or_default();
        let project = self.engine.elements();
        for tab in &mut self.documents {
            if tab.selection.generation != self.marked.generation {
                tab.selection = self.marked.clone();
            }
            tab.grid = grids.of(project, tab.document);
            tab.grid_visible = grids.visible(project, tab.document);
            tab.grid_origin = grid::origin(project, tab.document);
            if tab.palette.generation != self.palette.generation {
                tab.palette = self.palette.clone();
            }
        }
    }

    /// Hands the PCB tab the board's Layer state after a Command changed it, and redraws
    /// the board with it.
    fn sync_board_view(&mut self) {
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()).map(|p| p.uuid) else {
            return;
        };
        let layers = self.board_view().layers.clone();
        if self.document_tab(pcb).is_some_and(|t| t.layers != layers) {
            let scene = self.scene_of(pcb);
            let tab = self.document_tab_mut(pcb).expect("checked above");
            tab.layers = layers;
            tab.scene = scene;
        }
    }

    /// Drops the tab state of Documents whose tabs the layout no longer has.
    /// Their view is kept for `view.json` and for when they open again.
    fn forget_closed_documents(&mut self) {
        let documents = std::mem::take(&mut self.documents);
        let (open, closed): (Vec<_>, Vec<_>) = documents
            .into_iter()
            .partition(|d| self.layout().place(&TabKey::Document(d.document)).is_some());
        for tab in closed {
            let view = DocumentView {
                camera: tab.camera,
                layers: tab.layers,
                ..DocumentView::default()
            };
            self.closed_views.insert(tab.document, view);
        }
        self.documents = open;
    }

    fn scene_of(&mut self, document: Uuid) -> SceneHandle {
        self.generation += 1;
        if let Some((scene, layers)) = crate::element_editor::scene(&self.engine, document) {
            return SceneHandle {
                generation: self.generation,
                scene: Arc::new(scene),
                layers,
            };
        }
        let handle = match self.engine.project() {
            Some(project) => self.build_scene(project, document),
            None => SceneHandle {
                generation: 0,
                scene: Arc::default(),
                layers: None,
            },
        };
        SceneHandle {
            generation: self.generation,
            ..handle
        }
    }

    /// `document`'s Scene drawn from `project` (not necessarily the open one).
    fn scene_from(&mut self, project: &Project, document: Uuid) -> SceneHandle {
        self.generation += 1;
        SceneHandle {
            generation: self.generation,
            ..self.build_scene(project, document)
        }
    }

    /// The Scene of `document` in `project`, filtered and composited as the board view
    /// asks for a PCB. Its generation is left for the caller to set.
    fn build_scene(&self, project: &Project, document: Uuid) -> SceneHandle {
        let style = reda_engine::schematic_style::schematic_style(
            project,
            self.preferences.values(),
            reda_engine::schematic_style::Drawn::Canvas,
        );
        let board = self.board_view();
        let mut scene = if project.pcb_document(document).is_some() {
            let style = board.style(self.preferences.values());
            reda_engine::board_scene(project, document, self.engine.fonts(), &style)
        } else {
            reda_engine::document_scene(project, document, self.engine.fonts(), &style)
        }
        .unwrap_or_default();
        let layers = project.pcb_document(document).map(|pcb| {
            board.filter_scene(project, &mut scene);
            board
                .composites(pcb, self.preferences.values().display.inactive_layer_mode)
                .into()
        });
        SceneHandle {
            generation: 0,
            scene: Arc::new(scene),
            layers,
        }
    }

    /// A note in Messages for each Document whose file is missing: an error for the PCB
    /// Document, a warning for a sheet.
    fn note_missing_documents(&mut self) {
        let folder = self.engine.location().map(Path::to_path_buf);
        let notes = (self.engine.project().into_iter())
            .flat_map(|p| p.props().missing.iter())
            .map(|entry| {
                let path = match &folder {
                    Some(folder) => reda_engine::fs::display_path(&folder.join(entry.file_name())),
                    None => entry.file_name(),
                };
                Note {
                    source: MISSING_DOCUMENTS.to_owned(),
                    document: None,
                    severity: match entry.kind {
                        DocumentKind::Pcb => Severity::Error,
                        DocumentKind::Schematic => Severity::Warning,
                    },
                    text: tr!(
                        "message-missing-document",
                        name = entry.title.as_str(),
                        path = path.as_str()
                    ),
                }
            })
            .collect();
        self.messages.set_notes(MISSING_DOCUMENTS, notes);
    }

    /// Hands the engine the installed fonts that PCB text names and it does not have yet.
    fn load_fonts(&mut self) {
        self.note_missing_documents();
        let Some(pcb) = self.engine.project().and_then(|p| p.pcb()) else {
            self.messages.set_notes(FONTS, Vec::new());
            return;
        };
        let wanted: BTreeSet<String> = pcb
            .texts()
            .map(|t| t.font.clone())
            .filter(|f| f != HERSHEY_FONT && f != NOTO_SANS_FONT)
            .filter(|f| !self.asked_fonts.contains(f))
            .collect();
        for family in wanted {
            if let Some(face) = self.fonts.load(&family) {
                self.engine
                    .fonts_mut()
                    .add_face(family.clone(), face.bytes, face.index);
            }
            self.asked_fonts.insert(family);
        }
        let missing = reda_engine::pcb::missing_fonts(pcb_of(&self.engine), self.engine.fonts());
        let document = pcb_of(&self.engine).uuid;
        let notes = missing
            .into_iter()
            .map(|font| Note {
                source: FONTS.to_owned(),
                document: Some(document),
                severity: Severity::Warning,
                text: tr!("message-missing-font", font = font),
            })
            .collect();
        self.messages.set_notes(FONTS, notes);
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

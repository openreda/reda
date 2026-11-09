//! Board Setup: the settings stored in the PCB Document. The Rules pages (Clearance,
//! Track Width, Vias, Holes, Board Edge, Silkscreen) each hold one or more checks with
//! their board-wide values, the strictest Fab Preset limit beside each, a Net Class
//! override table and the check's DRC severity, plus "Apply Fab Preset Rules…". Net
//! Classes holds the grid (a Default row for the board-wide records, then one row per
//! class in rank order) and the Net table. DRC holds every check's severity, with the
//! values that have no rule page. Everything it changes is on the PCB Document's undo
//! stack.
//!
//! The pages are views of one shared [`BoardSetupDraft`], so a severity shown on a rule's
//! page and on the DRC page is one draft. The first page loads it and hands its steps to
//! the dialog; the others only draw it and flag their own fields.

use std::cell::{RefCell, RefMut};
use std::path::Path;
use std::rc::Rc;

use reda_engine::model::{ConnectionTarget, ConnectionValue, DrcCheck, Project, Scope};

use super::{FileRequest, Page, PageCx, Problem, SettingsDialog, Step};
use crate::shell::Shell;
use crate::tr;

mod draw;
mod length;
mod model;

pub use model::{
    ANGLES, Band, BoardSetupDraft, ClassKey, ClassRow, CrossBand, FabPreview, GRID_FIELDS,
    GRID_KINDS, ImportPreview, NetRow, PasteMode, RuleChange, RuleField, SPOKES, STYLES,
    StackupDraft, StackupRow, ValueKind, class_field, kind_name,
};

/// The draft every page of one dialog shares.
type Shared = Rc<RefCell<BoardSetupDraft>>;

/// A Rules page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Clearance,
    TrackWidth,
    Vias,
    /// Hole size, annular ring and hole-to-hole spacing.
    Holes,
    /// Edge clearance and silkscreen to edge.
    BoardEdge,
    /// Mask expansion, Via tenting and paste expansion. No DRC severity.
    MaskPaste,
    /// Silkscreen to mask.
    Silkscreen,
    /// How fills join through-hole Pads, SMD Pads and Vias, and the Zone naming scheme.
    /// No DRC severity.
    ZoneConnections,
}

/// One check on a rule page: its board-wide values, a Net Class override table and its
/// DRC severity.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Section {
    /// The check, for a section that has a DRC severity.
    pub check: Option<DrcCheck>,
    /// What the section is about, for one with no check: a Zone connection's target.
    pub target: Option<ConnectionTarget>,
    pub fields: &'static [RuleField],
}

impl Section {
    const fn check(check: DrcCheck, fields: &'static [RuleField]) -> Section {
        Section {
            check: Some(check),
            target: None,
            fields,
        }
    }

    const fn plain(target: Option<ConnectionTarget>, fields: &'static [RuleField]) -> Section {
        Section {
            check: None,
            target,
            fields,
        }
    }
}

impl Rule {
    pub const ALL: [Rule; 8] = [
        Rule::Clearance,
        Rule::TrackWidth,
        Rule::Vias,
        Rule::Holes,
        Rule::BoardEdge,
        Rule::MaskPaste,
        Rule::Silkscreen,
        Rule::ZoneConnections,
    ];

    /// The checks on the page, each with its values.
    pub fn sections(self) -> &'static [Section] {
        use ConnectionTarget as T;
        use ConnectionValue as V;
        use RuleField as F;
        const fn connection(t: ConnectionTarget) -> [RuleField; 5] {
            [
                F::Connection(t, V::Style),
                F::Connection(t, V::Spokes),
                F::Connection(t, V::SpokeWidth),
                F::Connection(t, V::Gap),
                F::Connection(t, V::Angle),
            ]
        }
        const TH: [RuleField; 5] = connection(T::ThroughHole);
        const SMD: [RuleField; 5] = connection(T::Smd);
        const VIA: [RuleField; 5] = connection(T::Via);
        const CLEARANCE: &[Section] = &[Section::check(DrcCheck::Clearance, &[F::Clearance])];
        const TRACK_WIDTH: &[Section] = &[Section::check(
            DrcCheck::TrackWidth,
            &[F::TrackWidth, F::TrackPreferred, F::TrackMax],
        )];
        const VIAS: &[Section] = &[Section::check(
            DrcCheck::ViaSize,
            &[
                F::ViaDiameter,
                F::ViaDiameterPreferred,
                F::ViaDiameterMax,
                F::ViaHole,
                F::ViaHolePreferred,
                F::ViaHoleMax,
            ],
        )];
        const HOLES: &[Section] = &[
            Section::check(DrcCheck::HoleSize, &[F::HoleMin, F::HoleMax]),
            Section::check(DrcCheck::AnnularRing, &[F::AnnularRing]),
            Section::check(DrcCheck::HoleToHole, &[F::HoleToHole]),
        ];
        const BOARD_EDGE: &[Section] = &[
            Section::check(DrcCheck::EdgeClearance, &[F::EdgeClearance]),
            Section::check(DrcCheck::SilkToEdge, &[F::SilkToEdge]),
        ];
        const MASK_PASTE: &[Section] = &[
            Section::plain(
                None,
                &[
                    F::MaskTop,
                    F::MaskBottom,
                    F::MaskFromHoleEdge,
                    F::TentViasTop,
                    F::TentViasBottom,
                ],
            ),
            Section::plain(None, &[F::PasteExpansion, F::PastePercent]),
        ];
        const SILKSCREEN: &[Section] = &[Section::check(DrcCheck::SilkToMask, &[F::SilkToMask])];
        const ZONE_CONNECTIONS: &[Section] = &[
            Section::plain(Some(T::ThroughHole), &TH),
            Section::plain(Some(T::Smd), &SMD),
            Section::plain(Some(T::Via), &VIA),
        ];
        match self {
            Rule::Clearance => CLEARANCE,
            Rule::TrackWidth => TRACK_WIDTH,
            Rule::Vias => VIAS,
            Rule::Holes => HOLES,
            Rule::BoardEdge => BOARD_EDGE,
            Rule::MaskPaste => MASK_PASTE,
            Rule::Silkscreen => SILKSCREEN,
            Rule::ZoneConnections => ZONE_CONNECTIONS,
        }
    }

    /// Every value on the page.
    pub fn fields(self) -> impl Iterator<Item = RuleField> {
        self.sections()
            .iter()
            .flat_map(|s| s.fields.iter().copied())
    }

    pub fn title(self) -> String {
        match self {
            Rule::Clearance => tr!("board-setup-page-clearance"),
            Rule::TrackWidth => tr!("board-setup-page-track-width"),
            Rule::Vias => tr!("board-setup-page-vias"),
            Rule::Holes => tr!("board-setup-page-holes"),
            Rule::BoardEdge => tr!("board-setup-page-board-edge"),
            Rule::Silkscreen => tr!("board-setup-page-silkscreen"),
            Rule::MaskPaste => tr!("board-setup-page-mask-paste"),
            Rule::ZoneConnections => tr!("board-setup-page-zone-connections"),
        }
    }

    fn owns(self, problem: &Problem) -> bool {
        owns(self.fields(), problem)
    }
}

/// Whether `problem` is on one of `fields`, board-wide or a class's.
fn owns(mut fields: impl Iterator<Item = RuleField>, problem: &Problem) -> bool {
    fields.any(|f| {
        problem.field == f.problem_field()
            || (problem.field.starts_with("class/")
                && problem.field.ends_with(&format!("/{}", f.id())))
    })
}

/// What every Board Setup page holds: the shared draft, and whether this page is the one
/// that loads it and reports its steps.
struct View {
    board: Shared,
    owner: bool,
}

impl View {
    fn load(&self, project: &Project, reset: bool) {
        if self.owner {
            self.board.borrow_mut().load(project, reset);
        }
    }

    fn steps(&self) -> Vec<Step> {
        if self.owner {
            self.board.borrow().steps()
        } else {
            Vec::new()
        }
    }

    fn checkpoint(&self) {
        if self.owner {
            self.board.borrow_mut().checkpoint();
        }
    }

    fn step_back(&self) -> bool {
        self.owner && self.board.borrow_mut().step_back()
    }

    fn picked_file(&self, shell: &mut Shell, request: FileRequest, path: &Path) {
        if self.owner {
            self.board.borrow_mut().picked_file(shell, request, path);
        }
    }
}

/// One rule's page.
pub struct RulePage {
    rule: Rule,
    view: View,
}

impl RulePage {
    pub fn rule(&self) -> Rule {
        self.rule
    }

    /// The dialog's drafts.
    pub fn board(&self) -> RefMut<'_, BoardSetupDraft> {
        self.view.board.borrow_mut()
    }
}

impl Page for RulePage {
    fn title(&self) -> String {
        self.rule.title()
    }

    fn group(&self) -> Option<String> {
        Some(tr!("board-setup-rules"))
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.view.load(project, reset);
    }

    fn steps(&self) -> Vec<Step> {
        self.view.steps()
    }

    fn checkpoint(&mut self) {
        self.view.checkpoint();
    }

    fn step_back(&mut self) -> bool {
        self.view.step_back()
    }

    fn picked_file(&mut self, shell: &mut Shell, request: FileRequest, path: &Path) {
        self.view.picked_file(shell, request, path);
    }

    fn problems(&self) -> Vec<Problem> {
        let problems = self.view.board.borrow().problems();
        problems.into_iter().filter(|p| self.rule.owns(p)).collect()
    }

    fn show(&mut self, ui: &mut egui::Ui, cx: &mut PageCx<'_>) {
        draw::rule_page(ui, cx, self.rule, &mut self.view.board.borrow_mut());
    }
}

/// The Net Classes page.
pub struct NetClassesPage {
    view: View,
}

impl NetClassesPage {
    pub fn board(&self) -> RefMut<'_, BoardSetupDraft> {
        self.view.board.borrow_mut()
    }
}

impl Page for NetClassesPage {
    fn title(&self) -> String {
        tr!("board-setup-page-net-classes")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.view.load(project, reset);
    }

    fn steps(&self) -> Vec<Step> {
        self.view.steps()
    }

    fn checkpoint(&mut self) {
        self.view.checkpoint();
    }

    fn step_back(&mut self) -> bool {
        self.view.step_back()
    }

    fn picked_file(&mut self, shell: &mut Shell, request: FileRequest, path: &Path) {
        self.view.picked_file(shell, request, path);
    }

    fn problems(&self) -> Vec<Problem> {
        let problems = self.view.board.borrow().problems();
        problems
            .into_iter()
            .filter(|p| {
                (p.field.starts_with("class/") && p.field.ends_with("/name"))
                    || owns(GRID_FIELDS.into_iter(), p)
            })
            .collect()
    }

    fn show(&mut self, ui: &mut egui::Ui, cx: &mut PageCx<'_>) {
        draw::net_classes_page(ui, cx, &mut self.view.board.borrow_mut());
    }
}

/// The Stackup page: the copper and dielectric thicknesses with their totals and a
/// cross-section, the finish and colours, and the Fab Notes text.
pub struct StackupPage {
    view: View,
}

impl StackupPage {
    pub fn board(&self) -> RefMut<'_, BoardSetupDraft> {
        self.view.board.borrow_mut()
    }
}

impl Page for StackupPage {
    fn title(&self) -> String {
        tr!("board-setup-page-stackup")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.view.load(project, reset);
    }

    fn steps(&self) -> Vec<Step> {
        self.view.steps()
    }

    fn checkpoint(&mut self) {
        self.view.checkpoint();
    }

    fn step_back(&mut self) -> bool {
        self.view.step_back()
    }

    fn picked_file(&mut self, shell: &mut Shell, request: FileRequest, path: &Path) {
        self.view.picked_file(shell, request, path);
    }

    fn problems(&self) -> Vec<Problem> {
        let problems = self.view.board.borrow().problems();
        let ours = |p: &Problem| p.field.starts_with("stackup/");
        problems.into_iter().filter(ours).collect()
    }

    fn show(&mut self, ui: &mut egui::Ui, cx: &mut PageCx<'_>) {
        draw::stackup_page(ui, cx, &mut self.view.board.borrow_mut());
    }
}

/// The DRC page: every check's severity.
pub struct DrcPage {
    view: View,
}

impl DrcPage {
    /// The values that sit beside their checks on the DRC page.
    pub const VALUES: [(DrcCheck, RuleField); 3] = [
        (DrcCheck::CourtyardOverlap, RuleField::CourtyardClearance),
        (DrcCheck::Dangling, RuleField::DanglingTolerance),
        (DrcCheck::MicroSegment, RuleField::MicroSegmentLength),
    ];

    pub fn board(&self) -> RefMut<'_, BoardSetupDraft> {
        self.view.board.borrow_mut()
    }
}

impl Page for DrcPage {
    fn title(&self) -> String {
        tr!("board-setup-page-drc")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        self.view.load(project, reset);
    }

    fn steps(&self) -> Vec<Step> {
        self.view.steps()
    }

    fn checkpoint(&mut self) {
        self.view.checkpoint();
    }

    fn step_back(&mut self) -> bool {
        self.view.step_back()
    }

    fn picked_file(&mut self, shell: &mut Shell, request: FileRequest, path: &Path) {
        self.view.picked_file(shell, request, path);
    }

    fn problems(&self) -> Vec<Problem> {
        let problems = self.view.board.borrow().problems();
        let fields = || DrcPage::VALUES.iter().map(|(_, f)| *f);
        problems.into_iter().filter(|p| owns(fields(), p)).collect()
    }

    fn show(&mut self, ui: &mut egui::Ui, cx: &mut PageCx<'_>) {
        draw::drc_page(ui, cx, &mut self.view.board.borrow_mut());
    }
}

/// The Board Setup dialog on `project`'s PCB Document.
pub fn dialog(project: &Project) -> SettingsDialog {
    let board: Shared = Rc::default();
    let view = |owner| View {
        board: board.clone(),
        owner,
    };
    let mut pages: Vec<Box<dyn Page>> = Rule::ALL
        .iter()
        .enumerate()
        .map(|(i, &rule)| {
            Box::new(RulePage {
                rule,
                view: view(i == 0),
            }) as Box<dyn Page>
        })
        .collect();
    pages.push(Box::new(StackupPage { view: view(false) }));
    pages.push(Box::new(NetClassesPage { view: view(false) }));
    pages.push(Box::new(DrcPage { view: view(false) }));
    let dialog = SettingsDialog::new(tr!("board-setup-title"), pages, project);
    match project.pcb() {
        Some(pcb) => dialog.on_stack(Scope::Document(pcb.uuid)),
        None => dialog,
    }
}

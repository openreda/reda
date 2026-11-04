//! Board Setup: the settings stored in the PCB Document. One page per rule (the
//! board-wide value, a Net Class override table and that check's DRC severity, plus
//! "Apply Fab Preset Rules…"), Net Classes (add, rename, delete, and the Net → Net Class
//! table) and DRC (every check's severity). Everything it changes is on the PCB
//! Document's undo stack.
//!
//! The pages are views of one shared [`BoardSetupDraft`], so a severity shown on a rule's
//! page and on the DRC page is one draft. The first page loads it and hands its steps to
//! the dialog; the others only draw it and flag their own fields.

use std::cell::{RefCell, RefMut};
use std::rc::Rc;

use reda_engine::model::{DrcCheck, Project, Scope};

use super::{Page, PageCx, Problem, SettingsDialog, Step};
use crate::tr;

mod draw;
mod length;
mod model;

pub use model::{
    BoardSetupDraft, ClassKey, ClassRow, FabPreview, NetRow, RuleChange, RuleField, class_field,
};

/// The draft every page of one dialog shares.
type Shared = Rc<RefCell<BoardSetupDraft>>;

/// A Rules page.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Rule {
    Clearance,
    TrackWidth,
    Via,
    AnnularRing,
    HoleSpacing,
    EdgeClearance,
}

impl Rule {
    pub const ALL: [Rule; 6] = [
        Rule::Clearance,
        Rule::TrackWidth,
        Rule::Via,
        Rule::AnnularRing,
        Rule::HoleSpacing,
        Rule::EdgeClearance,
    ];

    /// The board-wide values on the page.
    pub fn fields(self) -> &'static [RuleField] {
        match self {
            Rule::Clearance => &[RuleField::Clearance],
            Rule::TrackWidth => &[RuleField::TrackWidth],
            Rule::Via => &[RuleField::ViaDiameter, RuleField::ViaHole],
            Rule::AnnularRing => &[RuleField::AnnularRing],
            Rule::HoleSpacing => &[RuleField::HoleToHole],
            Rule::EdgeClearance => &[RuleField::EdgeClearance],
        }
    }

    /// The DRC check that enforces it.
    pub fn check(self) -> DrcCheck {
        match self {
            Rule::Clearance => DrcCheck::Clearance,
            Rule::TrackWidth => DrcCheck::TrackWidth,
            Rule::Via => DrcCheck::HoleSize,
            Rule::AnnularRing => DrcCheck::AnnularRing,
            Rule::HoleSpacing => DrcCheck::HoleToHole,
            Rule::EdgeClearance => DrcCheck::EdgeClearance,
        }
    }

    pub fn title(self) -> String {
        match self {
            Rule::Clearance => tr!("board-setup-page-clearance"),
            Rule::TrackWidth => tr!("board-setup-page-track-width"),
            Rule::Via => tr!("board-setup-page-via"),
            Rule::AnnularRing => tr!("board-setup-page-annular-ring"),
            Rule::HoleSpacing => tr!("board-setup-page-hole-spacing"),
            Rule::EdgeClearance => tr!("board-setup-page-edge-clearance"),
        }
    }

    /// The rules on the page a Net Class can override.
    pub fn overrides(self) -> Vec<RuleField> {
        self.fields()
            .iter()
            .copied()
            .filter(|f| RuleField::OVERRIDES.contains(f))
            .collect()
    }

    fn owns(self, problem: &Problem) -> bool {
        self.fields().iter().any(|f| {
            problem.field == f.problem_field()
                || (problem.field.starts_with("class/")
                    && problem.field.ends_with(&format!("/{}", f.id())))
        })
    }
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

    fn problems(&self) -> Vec<Problem> {
        let problems = self.view.board.borrow().problems();
        problems
            .into_iter()
            .filter(|p| p.field.starts_with("class/") && p.field.ends_with("/name"))
            .collect()
    }

    fn show(&mut self, ui: &mut egui::Ui, cx: &mut PageCx<'_>) {
        draw::net_classes_page(ui, cx, &mut self.view.board.borrow_mut());
    }
}

/// The DRC page: every check's severity.
pub struct DrcPage {
    view: View,
}

impl DrcPage {
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

    fn problems(&self) -> Vec<Problem> {
        Vec::new()
    }

    fn show(&mut self, ui: &mut egui::Ui, _cx: &mut PageCx<'_>) {
        draw::drc_page(ui, &mut self.view.board.borrow_mut());
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
    pages.push(Box::new(NetClassesPage { view: view(false) }));
    pages.push(Box::new(DrcPage { view: view(false) }));
    let dialog = SettingsDialog::new(tr!("board-setup-title"), pages, project);
    match project.pcb() {
        Some(pcb) => dialog.on_stack(Scope::Document(pcb.uuid)),
        None => dialog,
    }
}

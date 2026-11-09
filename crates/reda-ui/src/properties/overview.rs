//! What Properties shows with nothing selected, with no drawing: the board's Ready for
//! fab? lines and figures, the sheet's frame, and the check summary
//! (docs/spec/v0.1/editing.md#properties-with-nothing-selected,
//! docs/spec/v0.1/checks.md#check-summary).

use std::sync::Arc;
use std::time::SystemTime;

use reda_engine::model::{DrcCheck, Project, Severity, Uuid};
use reda_engine::pcb::{BoardInfoResult, Copper};
use serde_json::{Value, json};

use crate::messages::CheckKind;
use crate::shell::Shell;

/// The board's figures that need its copper: worked out once per board state.
#[derive(Clone)]
pub struct BoardFigures {
    /// The copper index they came from: a new board state has a new one.
    pub copper: Arc<Copper>,
    pub info: BoardInfoResult,
    /// Connections copper has not made yet.
    pub unrouted: usize,
    /// What each health check finds, in table order, with the count; checks that find
    /// nothing are left out.
    pub health: Vec<(DrcCheck, usize)>,
}

impl BoardFigures {
    /// The figures of the open Project's board; `None` without one.
    pub fn of(project: &Project) -> Option<BoardFigures> {
        let pcb = project.pcb()?;
        let copper = Copper::of(project)?;
        let connections = copper.all_connections();
        let findings = reda_engine::pcb::drc::health::findings(pcb);
        let health = DrcCheck::HEALTH
            .iter()
            .map(|c| (*c, findings.iter().filter(|f| f.check == *c).count()))
            .filter(|(_, n)| *n > 0)
            .collect();
        Some(BoardFigures {
            info: reda_engine::pcb::board_info(pcb, &copper),
            unrouted: connections.total - connections.made,
            health,
            copper,
        })
    }

    /// Whether these are still the figures of `project`'s board.
    pub fn current(&self, project: &Project) -> bool {
        Copper::of(project).is_some_and(|c| Arc::ptr_eq(&c, &self.copper))
    }
}

/// One line of Ready for fab?.
#[derive(Clone, Debug, PartialEq)]
pub struct FabLine {
    pub item: FabItem,
    /// Fine as it is; `false` marks the line `!`.
    pub ok: bool,
    /// A count to show, where the line has one.
    pub count: Option<usize>,
    /// What its button runs, while the line is not OK: `(id, params)`, or `None` for a
    /// check run, which goes through Messages.
    pub action: Option<FabAction>,
}

/// What a Ready for fab? line is about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FabItem {
    Outline,
    Unrouted,
    Zones,
    Drc,
    Sync,
    NotFitted,
}

/// A Ready for fab? button.
#[derive(Clone, Debug, PartialEq)]
pub enum FabAction {
    /// Runs this Command.
    Run(&'static str, Value),
    /// Runs DRC through Messages.
    RunDrc,
}

/// The Ready for fab? lines for the board, in the spec's order.
pub fn ready_for_fab(shell: &Shell, figures: &BoardFigures) -> Vec<FabLine> {
    let Some(pcb) = shell.engine().project().and_then(Project::pcb) else {
        return Vec::new();
    };
    let outline = pcb.outline().is_some_and(|o| o.vertices.len() >= 3);
    let drc_current = shell
        .messages()
        .run(CheckKind::Drc)
        .is_some_and(|r| r.ran_at.is_some() && !r.stale);
    let sync = shell.out_of_sync();
    let not_fitted = pcb.footprints().filter(|f| f.not_fitted).count();
    let action = |ok: bool, a: FabAction| (!ok).then_some(a);
    vec![
        FabLine {
            item: FabItem::Outline,
            ok: outline,
            count: None,
            action: None,
        },
        FabLine {
            item: FabItem::Unrouted,
            ok: figures.unrouted == 0,
            count: Some(figures.unrouted),
            action: action(
                figures.unrouted == 0,
                FabAction::Run("pcb.ratsnest.show", json!({"visible": true})),
            ),
        },
        FabLine {
            item: FabItem::Zones,
            ok: !shell.zones_need_refill(),
            count: None,
            action: action(
                !shell.zones_need_refill(),
                FabAction::Run("pcb.zone.fill", json!({})),
            ),
        },
        FabLine {
            item: FabItem::Drc,
            ok: drc_current,
            count: None,
            action: action(drc_current, FabAction::RunDrc),
        },
        FabLine {
            item: FabItem::Sync,
            ok: sync == 0,
            count: Some(sync),
            action: action(
                sync == 0,
                FabAction::Run("app.dialog.open", json!({"dialog": "change_list"})),
            ),
        },
        FabLine {
            item: FabItem::NotFitted,
            ok: true,
            count: Some(not_fitted),
            action: None,
        },
    ]
}

/// The check summary: the last run of a board's DRC or a sheet's ERC.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckSummary {
    pub kind: CheckKind,
    /// When the check last ran in full; `None` before it has (the health checks run on
    /// open don't count).
    pub ran_at: Option<SystemTime>,
    /// The design was edited after that run.
    pub stale: bool,
    pub errors: usize,
    pub warnings: usize,
    pub waived: usize,
    /// On a board: what each health check finds now, with its **Fix** Command.
    pub health: Vec<HealthLine>,
}

/// One health check that finds something, and the Command that removes what it finds.
#[derive(Clone, Debug, PartialEq)]
pub struct HealthLine {
    pub check: DrcCheck,
    pub count: usize,
    pub fix: (&'static str, Value),
}

impl CheckSummary {
    /// The summary of check `kind`; `figures` gives a board's health checks.
    pub fn of(shell: &Shell, kind: CheckKind, figures: Option<&BoardFigures>) -> CheckSummary {
        let run = shell.messages().run(kind);
        let unwaived = |severity: Severity| {
            run.map_or(0, |r| {
                r.entries
                    .iter()
                    .filter(|e| e.violation.waiver.is_none() && e.violation.severity == severity)
                    .count()
            })
        };
        let health = match (kind, figures) {
            (CheckKind::Drc, Some(f)) => f
                .health
                .iter()
                .map(|&(check, count)| HealthLine {
                    check,
                    count,
                    fix: ("drc.fix", json!({"check": check})),
                })
                .collect(),
            _ => Vec::new(),
        };
        CheckSummary {
            kind,
            ran_at: run.and_then(|r| r.ran_at),
            stale: run.is_some_and(|r| r.stale),
            errors: unwaived(Severity::Error),
            warnings: unwaived(Severity::Warning),
            waived: run.map_or(0, |r| {
                r.entries
                    .iter()
                    .filter(|e| e.violation.waiver.is_some())
                    .count()
            }),
            health,
        }
    }
}

/// The Violations of the last runs that name any of `objects`, as `(kind, line text)`.
pub fn violations_on(shell: &Shell, objects: &[Uuid]) -> Vec<(CheckKind, Severity, String)> {
    let mut out = Vec::new();
    for kind in CheckKind::ALL {
        let Some(run) = shell.messages().run(kind) else {
            continue;
        };
        for entry in &run.entries {
            if entry.violation.objects.iter().any(|o| objects.contains(o)) {
                out.push((kind, entry.violation.severity, entry.text.clone()));
            }
        }
    }
    out
}

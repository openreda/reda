//! The requested Layer pair, its permitted choices, and atomic placement of a stack.
use super::*;
use crate::view_state::ViaPairChoice;
use reda_engine::pcb::route::ViaCandidates;
use reda_engine::pcb::rules::{via_ranges, via_starting_size};

#[derive(Debug)]
pub(super) struct PairCandidates {
    generation: u64,
    document: Uuid,
    net: Option<Uuid>,
    positions: [usize; 2],
    choices: ViaCandidates,
}

pub(super) fn activate(shell: &mut Shell, layer: Uuid) {
    if let Some(pcb) = shell.engine().project().and_then(Project::pcb) {
        let set = pcb.layers().clone();
        shell.board_view_mut().activate(&set, layer);
    }
}

pub(crate) const PAGE_SIZE: usize = 16;

impl Router {
    fn previous_layer(&self, shell: &Shell) -> Option<Uuid> {
        let pcb = shell.engine().project()?.pcb()?;
        let from = self.route.as_ref()?.leg.layer;
        shell.board_view().layers.previous
            .filter(|&l| l != from && pcb.copper_number(l).is_some())
            .or_else(|| pcb.copper_layers().map(|l| l.uuid).filter(|&l| l != from).last())
    }

    fn candidates_for(&mut self, shell: &Shell, to: Uuid) -> Option<(&PairCandidates, usize)> {
        let pcb = shell.engine().project()?.pcb()?;
        let route = self.route.as_ref()?;
        let from = route.leg.layer;
        let mut positions = [pcb.copper_number(from)?, pcb.copper_number(to)?];
        positions.sort_unstable();
        let generation = shell.design_generation();
        if self.candidates.as_ref().is_none_or(|p| p.generation != generation ||
            p.document != pcb.uuid || p.net != route.net || p.positions != positions) {
            self.candidates = Some(PairCandidates {
                generation, document: pcb.uuid, net: route.net, positions,
                choices: ViaCandidates::between(pcb, route.net, from, to)
            });
        }
        let pair = self.candidates.as_ref()?;
        let index = shell.board_view().layers.via_choices.iter().find(|c| c.positions == positions)
            .and_then(|c| pair.choices.index_of(&c.types)).unwrap_or(0);
        Some((pair, index))
    }

    pub(crate) fn cycle_via(&mut self, shell: &mut Shell) {
        let Some(to) = self.previous_layer(shell) else { return };
        let Some((pair, index)) = self.candidates_for(shell, to) else {return};
        if pair.choices.is_empty() {
            self.no_candidate(shell);
            return;
        }
        let index = (index + 1) % pair.choices.len();
        self.pick_via(shell, super::super::route_commands::PickViaParams {
            target: Some(to), index: Some(index), page: None,
        });
    }

    pub(crate) fn pick_via(&mut self, shell: &mut Shell, params: super::super::route_commands::PickViaParams) {
        let Some(target) = params.target.or_else(|| self.previous_layer(shell)) else {return};
        let Some((pair, selected)) = self.candidates_for(shell, target) else {return};
        let count = pair.choices.len();
        if let Some(index) = params.index {
            let Some(types) = pair.choices.get(index) else {self.no_candidate(shell); return};
            let choice = ViaPairChoice { positions: pair.positions, types };
            let choices = &mut shell.board_view_mut().layers.via_choices;
            choices.retain(|c| c.positions != choice.positions);
            choices.push(choice);
            self.chooser = None;
        } else {
            let page = params.page.unwrap_or(selected / PAGE_SIZE).min(count.saturating_sub(1) / PAGE_SIZE);
            self.chooser = Some(Chooser::Candidates {target, selected, page, count});
        }
        self.recheck(shell);
    }

    pub(crate) fn candidate_label(&self, pcb: &PcbDocument, index: usize) -> Option<String> {
        let ids = self.candidates.as_ref()?.choices.get(index)?;
        Some(ids.into_iter().filter_map(|id| pcb.stackup().via_types.iter()
            .find(|t| t.uuid == id).map(|t| t.display_name(pcb))).collect::<Vec<_>>().join(" + "))
    }

    fn no_candidate(&mut self, shell: &mut Shell) {
        let note = tr!("route-no-via");
        shell.report(note.clone());
        self.note = Some(note);
    }

    fn members(&mut self, shell: &Shell, to: Uuid) -> Option<Vec<RouteVia>> {
        let (pair, rank) = self.candidates_for(shell, to)?;
        let ids = pair.choices.get(rank)?;
        let pcb = shell.engine().project()?.pcb()?;
        let net = self.route.as_ref()?.net;
        ids.iter().map(|id| {
            let kind = pcb.stackup().via_types.iter().find(|v| v.uuid == *id)?;
            let (diameter, hole) = if let Some(user) = self.via_size {
                let (pads, holes) = via_ranges(pcb, net, kind);
                (pads.clip(user.diameter).into(), holes.clip(user.hole))
            } else {
                via_starting_size(pcb, net, kind)
            };
            Some(RouteVia {span: kind.span(), diameter, hole, unused_layers: kind.unused_layers})
        }).collect()
    }

    pub fn add_via(&mut self, shell: &mut Shell) {
        if let Some(to) = self.previous_layer(shell) {
            self.jump(shell, to, true);
        }
    }

    pub fn via_end(&mut self, shell: &mut Shell) {
        if let Some(to) = self.previous_layer(shell)
            && self.jump(shell, to, false) {
            self.finish(shell);
        }
    }

    pub(crate) fn jump(&mut self, shell: &mut Shell, to: Uuid, switch: bool) -> bool {
        self.notice_cancel(shell);
        let (Some(route), Some(Snapped { at, .. })) = (&self.route, self.cursor) else { return false };
        let from = route.leg.layer;
        let net = route.net;
        if from == to || shell.engine().project().and_then(Project::pcb)
            .and_then(|p| p.copper_number(to)).is_none() {return false;}
        self.check(shell, at);
        if let Some(why) = self.why_blocked(shell) {
            shell.report(why);
            return false;
        }
        let Some(members) = self.members(shell, to) else {
            self.no_candidate(shell);
            return false;
        };
        for via in &members {
            let proposal = Proposal::Via { at, diameter: via.diameter, span: via.span, net };
            if let Some(why) = self.blocked_by(shell, &proposal) {
                shell.report(why);
                return false;
            }
        }
        let pcb = shell.engine().project().and_then(Project::pcb).expect("routing a board");
        let route = self.route.as_mut().expect("routing");
        let path = route::corner_path(route.last(), at, self.corner, self.posture);
        route.leg.points.extend_from_slice(&path[1..]);
        let layer = if switch { to } else { from };
        let next = Leg {
            layer,
            width: route::width::changed_layer(pcb, net, layer,
                (!route.follows_preferred_width).then_some(route.leg.width)),
            points: vec![at],
        };
        let done = std::mem::replace(&mut route.leg, next);
        route.steps.push(Step::Leg(done));
        route.steps.push(Step::Via(at, members));
        if switch { activate(shell, to); }
        self.depend_on(shell, at);
        self.check(shell, at);
        true
    }
}

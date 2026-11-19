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
        shell
            .board_view()
            .layers
            .previous
            .filter(|&l| l != from && pcb.copper_number(l).is_some())
            .or_else(|| {
                let outer = pcb.stackup().through_type().span();
                Some(if from == outer.to {
                    outer.from
                } else {
                    outer.to
                })
            })
    }

    fn candidates_for(&mut self, shell: &Shell, to: Uuid) -> Option<(&PairCandidates, usize)> {
        let pcb = shell.engine().project()?.pcb()?;
        let route = self.route.as_ref()?;
        let from = route.leg.layer;
        let mut positions = [pcb.copper_number(from)?, pcb.copper_number(to)?];
        positions.sort_unstable();
        let generation = shell.design_generation();
        if self.candidates.as_ref().is_none_or(|p| {
            p.generation != generation
                || p.document != pcb.uuid
                || p.net != route.net
                || p.positions != positions
        }) {
            self.candidates = Some(PairCandidates {
                generation,
                document: pcb.uuid,
                net: route.net,
                positions,
                choices: ViaCandidates::between(pcb, route.net, from, to),
            });
        }
        let pair = self.candidates.as_ref()?;
        let index = shell
            .board_view()
            .layers
            .via_choices
            .iter()
            .find(|c| c.positions == positions)
            .and_then(|c| pair.choices.index_of(&c.types))
            .unwrap_or(0);
        Some((pair, index))
    }

    pub(crate) fn cycle_via(&mut self, shell: &mut Shell) {
        let Some(to) = self.candidate_target(shell) else {
            return;
        };
        let Some((pair, index)) = self.candidates_for(shell, to) else {
            return;
        };
        if pair.choices.is_empty() {
            self.no_candidate(shell);
            return;
        }
        let index = (index + 1) % pair.choices.len();
        self.pick_via(
            shell,
            super::super::route_commands::PickViaParams {
                target: Some(to),
                index: Some(index),
                page: None,
            },
        );
    }

    pub(crate) fn pick_via(
        &mut self,
        shell: &mut Shell,
        params: super::super::route_commands::PickViaParams,
    ) {
        let Some(target) = params.target.or_else(|| self.candidate_target(shell)) else {
            return;
        };
        let Some((pair, selected)) = self.candidates_for(shell, target) else {
            return;
        };
        let count = pair.choices.len();
        if let Some(index) = params.index {
            let Some(types) = pair.choices.get(index) else {
                self.no_candidate(shell);
                return;
            };
            let choice = ViaPairChoice {
                positions: pair.positions,
                types,
            };
            let choices = &mut shell.board_view_mut().layers.via_choices;
            choices.retain(|c| c.positions != choice.positions);
            choices.push(choice);
            self.picked_target = Some(target);
            self.chooser = None;
        } else {
            let page = params
                .page
                .unwrap_or(selected / PAGE_SIZE)
                .min(count.saturating_sub(1) / PAGE_SIZE);
            self.chooser = Some(Chooser::Candidates {
                target,
                selected,
                page,
                count,
            });
        }
        self.recheck(shell);
    }

    pub(crate) fn candidate_label(&self, pcb: &PcbDocument, index: usize) -> Option<String> {
        let ids = self.candidates.as_ref()?.choices.get(index)?;
        Some(
            ids.into_iter()
                .filter_map(|id| {
                    pcb.stackup()
                        .via_types
                        .iter()
                        .find(|t| t.uuid == id)
                        .map(|t| crate::pcb::via_type_name(pcb, t))
                })
                .collect::<Vec<_>>()
                .join(" + "),
        )
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
        ids.iter()
            .map(|id| {
                let kind = pcb.stackup().via_types.iter().find(|v| v.uuid == *id)?;
                let (diameter, hole) = mode_size(pcb, net, kind, self.via_mode, self.via_size)
                    .unwrap_or_else(|| via_starting_size(pcb, net, kind));
                Some(RouteVia {
                    span: kind.span(),
                    diameter,
                    hole,
                    unused_layers: kind.unused_layers,
                })
            })
            .collect()
    }

    pub(super) fn candidate_target(&self, shell: &Shell) -> Option<Uuid> {
        match self.chooser {
            Some(Chooser::Candidates { target, .. }) => Some(target),
            _ => self
                .hovered_layer
                .or(self.picked_target)
                .or_else(|| self.previous_layer(shell)),
        }
    }

    pub(crate) fn hover_layer(&mut self, shell: &mut Shell, layer: Option<Uuid>) {
        let target = layer.filter(|l| {
            self.route.as_ref().is_some_and(|r| r.leg.layer != *l)
                && shell
                    .engine()
                    .project()
                    .and_then(Project::pcb)
                    .and_then(|p| p.copper_number(*l))
                    .is_some()
        });
        if self.hovered_layer != target {
            self.hovered_layer = target;
            self.recheck(shell);
        }
    }

    pub(super) fn preview_vias(
        &mut self,
        shell: &Shell,
        copper: &Copper,
        at: Point,
        target: Uuid,
    ) -> Option<ViaPreview> {
        let members = self.members(shell, target).unwrap_or_default();
        let route = self.route.as_ref()?;
        let pcb = shell.engine().project()?.pcb()?;
        let mut blocked = self.why_blocked(shell);
        let mut names = Vec::new();
        let mut rings = std::collections::BTreeSet::new();
        let display = shell.preferences().pcb.route.clearance_display;
        let near = (display == ClearanceDisplay::Near).then_some((at, CLEARANCE_NEAR));
        let path = route::corner_path(route.last(), at, self.corner, self.posture);
        let ends = route.track_ends(&path);
        for member in &members {
            if let Some(t) = pcb.stackup().type_for_span(member.span) {
                names.push(crate::pcb::via_type_name(pcb, t));
            }
            let proposal = copper.propose_via(pcb, at, route.net, *member, &ends);
            rings.insert(member.hole);
            for (layer, land) in proposal.via_layers(pcb) {
                rings.extend(land);
                if display != ClearanceDisplay::Off {
                    self.halos.extend(copper.via_halos(
                        pcb,
                        layer,
                        route.net,
                        near,
                        land,
                        member.hole,
                    ));
                }
            }
            if !member.diameter.fits_hole(member.hole) {
                blocked = Some(tr!("route-via-invalid-size"));
            } else if let Some(hit) = copper.clearance(pcb, &proposal, &[]).first() {
                let units = shell.board_view().units;
                blocked.get_or_insert_with(|| {
                    tr!(
                        "route-blocked",
                        actual = super::super::view::format_length(hit.actual, units),
                        required = super::super::view::format_length(hit.required, units)
                    )
                });
            } else if !keepout::keepouts_hit(pcb, &proposal).is_empty() {
                blocked.get_or_insert_with(|| tr!("route-keepout"));
            } else if !route::edges_hit(pcb, &proposal).is_empty() {
                blocked.get_or_insert_with(|| tr!("route-edge"));
            }
        }
        let label = if members.is_empty() {
            let note = tr!("route-no-via");
            blocked = Some(note.clone());
            note
        } else if pcb.copper_layers().nth(2).is_some() {
            names.join(" + ")
        } else {
            String::new()
        };
        Some(ViaPreview {
            at,
            target,
            members,
            label,
            rings: rings.into_iter().collect(),
            blocked,
        })
    }

    /// Before the first segment, L picks another land of the same starting Pad or Via.
    pub(crate) fn start_layer(&mut self, shell: &mut Shell) {
        let Some(route) = self.route.as_ref().filter(|r| r.segments() == 0) else {
            return;
        };
        let Some(pcb) = shell.engine().project().and_then(Project::pcb) else {
            return;
        };
        let Some(copper) = self.copper(shell) else {
            return;
        };
        let at = route.last();
        let origin = route.leg.layer;
        let starts: Vec<_> = copper
            .items_at(origin, at)
            .into_iter()
            .filter(|i| {
                matches!(
                    i,
                    reda_engine::pcb::Item::Pad { .. }
                        | reda_engine::pcb::Item::FreePad { .. }
                        | reda_engine::pcb::Item::Via { .. }
                ) && copper.net(*i) == route.net
            })
            .collect();
        let layers: Vec<_> = pcb.copper_layers().map(|l| l.uuid).collect();
        let Some(pos) = layers.iter().position(|&l| l == origin) else {
            return;
        };
        let Some(next) = layers
            .iter()
            .cycle()
            .skip(pos + 1)
            .take(layers.len() - 1)
            .find(|&&l| copper.items_at(l, at).iter().any(|i| starts.contains(i)))
            .copied()
        else {
            return;
        };
        let route = self.route.as_mut().expect("a route");
        route.leg.layer = next;
        route.leg.width = route::width::changed_layer(
            pcb,
            route.net,
            next,
            (!route.follows_preferred_width).then_some(route.leg.width),
        );
        activate(shell, next);
        self.depend_on(shell, at);
        self.recheck(shell);
    }

    pub fn add_via(&mut self, shell: &mut Shell) {
        if let Some(to) = self.previous_layer(shell) {
            self.jump(shell, to, true);
        }
    }

    pub(crate) fn via_stay(&mut self, shell: &mut Shell) {
        if let Some(to) = self.candidate_target(shell) {
            self.jump(shell, to, false);
        }
    }

    pub(super) fn cycle_via_size(&mut self, shell: &Shell) {
        use route::WidthMode::*;
        let Some(pcb) = shell.engine().project().and_then(Project::pcb) else {
            return;
        };
        let Some(candidate) = &self.via_candidate else {
            return;
        };
        let net = self.route.as_ref().and_then(|r| r.net);
        let modes = [Min, Preferred, Max, User];
        let at = modes
            .iter()
            .position(|m| *m == self.via_mode)
            .expect("a mode");
        if let Some(next) = (1..=4).map(|i| modes[(i + at) % 4]).find(|&m| {
            candidate
                .members
                .iter()
                .filter_map(|v| pcb.stackup().type_for_span(v.span))
                .any(|kind| mode_size(pcb, net, kind, m, self.via_size).is_some())
        }) {
            self.via_mode = next;
        }
    }

    pub fn via_end(&mut self, shell: &mut Shell) {
        if let Some(to) = self.candidate_target(shell)
            && self.jump(shell, to, false)
        {
            self.finish(shell);
        }
    }

    pub(crate) fn jump(&mut self, shell: &mut Shell, to: Uuid, switch: bool) -> bool {
        self.notice_cancel(shell);
        let (Some(route), Some(Snapped { at, .. })) = (&self.route, self.cursor) else {
            return false;
        };
        let from = route.leg.layer;
        let net = route.net;
        if from == to
            || shell
                .engine()
                .project()
                .and_then(Project::pcb)
                .and_then(|p| p.copper_number(to))
                .is_none()
        {
            return false;
        }
        self.check_to(shell, at, Some(to));
        let Some(candidate) = self.via_candidate.as_ref() else {
            return false;
        };
        let invalid = candidate.members.is_empty()
            || candidate
                .members
                .iter()
                .any(|v| !v.diameter.fits_hole(v.hole));
        if let Some(why) = &candidate.blocked
            && (invalid || shell.preferences().pcb.route.obstacle_mode == route::ObstacleMode::Stop)
        {
            shell.report(why.clone());
            self.note = Some(why.clone());
            return false;
        }
        let members = candidate.members.clone();
        let pcb = shell
            .engine()
            .project()
            .and_then(Project::pcb)
            .expect("routing a board");
        let route = self.route.as_mut().expect("routing");
        let path = route::corner_path(route.last(), at, self.corner, self.posture);
        route.leg.points.extend_from_slice(&path[1..]);
        let layer = if switch { to } else { from };
        let next = Leg {
            layer,
            width: route::width::changed_layer(
                pcb,
                net,
                layer,
                (!route.follows_preferred_width).then_some(route.leg.width),
            ),
            points: vec![at],
        };
        let done = std::mem::replace(&mut route.leg, next);
        route.steps.push(Step::Leg(done));
        route.steps.push(Step::Via(at, members));
        if switch {
            self.picked_target = None;
            activate(shell, to);
        }
        self.chooser = None;
        self.hovered_layer = None;
        self.depend_on(shell, at);
        self.check(shell, at);
        true
    }
}

fn mode_size(
    pcb: &PcbDocument,
    net: Option<Uuid>,
    kind: &reda_engine::model::ViaType,
    mode: route::WidthMode,
    user: Option<ViaSize>,
) -> Option<(reda_engine::model::ViaDiameter, Nm)> {
    use route::WidthMode::*;
    if mode == Preferred {
        return Some(via_starting_size(pcb, net, kind));
    }
    let (diameters, holes) = via_ranges(pcb, net, kind);
    let (pad, hole) = match mode {
        Min => (diameters.min, holes.min),
        Max => (diameters.max, holes.max),
        User => {
            let size = user?;
            (Some(size.diameter), Some(size.hole))
        }
        Preferred => unreachable!(),
    };
    if pad.is_none() && hole.is_none() {
        return None;
    }
    Some((
        pad.map_or_else(
            || via_starting_size(pcb, net, kind).0,
            |size| diameters.clip(size).into(),
        ),
        holes.clip(hole.unwrap_or(holes.preferred)),
    ))
}

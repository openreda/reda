//! Board Setup ▸ Grids (docs/spec/v0.3/grids-guides.md#board-grids): the board's Board
//! Grids, smallest step first, one row each with its step (`0.5mm`, or `0.5mm, 1.27mm` for a Step Y), its
//! visible spacing, In cycle and the Default mark, plus Add and Remove. Apply runs
//! `pcb.board_grid.delete`, then `set` (a row losing the Default mark first) and `add`.
//! A bare number is in mm; a value keeps the unit it is typed in.
//!
//! The board's Local Grids follow, in priority order (the first that contains the cursor
//! wins): each row's grip drags it up or down, and its name and Applies to can be edited.
//! Apply runs `pcb.local_grid.set` for those, then `pcb.local_grid.reorder` when the order
//! changed. Their areas and steps are edited on the canvas and in Properties.

use egui::{Role, Ui, WidgetInfo};
use reda_engine::model::{AppliesTo, BoardGrid, LocalGrid, Project, TypedLength, Uuid};
use reda_engine::preferences::Units;
use serde_json::{Value, json};

use crate::grid::custom_dialog::typed_step;
use crate::settings::{Draft, Page, PageCx, Problem, Step, field, layout};
use crate::theme::TokensExt as _;
use crate::tr;
use crate::widgets;

/// A step field.
const STEP_WIDTH: f32 = 160.0;
/// The visible spacing field.
const VISIBLE_WIDTH: f32 = 100.0;

/// One Board Grid as the page edits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BoardGridRow {
    /// The Board Grid it edits; `None` for a row added on the page.
    pub uuid: Option<Uuid>,
    /// Step X, and `, ` Step Y when it has one.
    pub step: String,
    /// The visible spacing; empty is Step X.
    pub visible: String,
    pub in_cycle: bool,
    pub default: bool,
}

impl BoardGridRow {
    fn of(grid: &BoardGrid) -> BoardGridRow {
        let text = |l: TypedLength| l.to_string();
        let step = match grid.step_y {
            Some(y) => format!("{}, {}", text(grid.step_x), text(y)),
            None => text(grid.step_x),
        };
        BoardGridRow {
            uuid: Some(grid.uuid),
            step,
            visible: grid.visible.map(text).unwrap_or_default(),
            in_cycle: grid.in_cycle,
            default: grid.default,
        }
    }
}

/// One Local Grid as the page edits it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LocalGridRow {
    pub uuid: Uuid,
    pub name: String,
    pub applies_to: AppliesTo,
    /// Its step, `0.5mm` or `0.5mm × 1mm`, shown for telling rows apart.
    pub step: String,
}

impl LocalGridRow {
    fn of(grid: &LocalGrid) -> LocalGridRow {
        let reda_engine::model::LocalGridShape::Cartesian(c) = &grid.shape;
        let step = if c.step_x == c.step_y {
            c.step_x.to_string()
        } else {
            format!("{} × {}", c.step_x, c.step_y)
        };
        LocalGridRow {
            uuid: grid.uuid,
            name: grid.name.clone(),
            applies_to: grid.applies_to,
            step,
        }
    }
}

/// A row's values, read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Parsed {
    x: TypedLength,
    y: Option<TypedLength>,
    visible: Option<TypedLength>,
}

/// The page's field names, for its problems.
fn step_field(i: usize) -> String {
    format!("grid/{i}/step")
}

fn visible_field(i: usize) -> String {
    format!("grid/{i}/visible")
}

impl BoardGridRow {
    /// The row's values, or the problems with them.
    fn parse(&self, i: usize) -> Result<Parsed, Vec<Problem>> {
        let mut problems = Vec::new();
        let step = typed_step(&self.step, Units::Mm, false)
            .map_err(|why| problems.push(Problem::new(step_field(i), why)));
        let visible = if self.visible.trim().is_empty() {
            Ok(None)
        } else {
            match typed_step(&self.visible, Units::Mm, false) {
                Ok((v, None)) => Ok(Some(v)),
                Ok(_) => Err(tr!("board-setup-grids-visible-one")),
                Err(why) => Err(why),
            }
            .map_err(|why| problems.push(Problem::new(visible_field(i), why)))
        };
        match (step, visible) {
            (Ok((x, y)), Ok(visible)) => Ok(Parsed {
                x,
                // `0.5mm, 0.5mm` is square.
                y: y.filter(|y| *y != x),
                visible,
            }),
            _ => Err(problems),
        }
    }
}

/// The Grids page.
#[derive(Clone, Debug, Default)]
pub struct GridsPage {
    rows: Draft<Vec<BoardGridRow>>,
    /// The Local Grids, highest priority first.
    local: Draft<Vec<LocalGridRow>>,
}

impl GridsPage {
    pub fn rows(&self) -> &[BoardGridRow] {
        self.rows.value()
    }

    /// The Local Grids as the page shows them, highest priority first.
    pub fn local_grids(&self) -> &[LocalGridRow] {
        self.local.value()
    }

    /// Puts Local Grid row `from` at position `to`, as a drag of its grip does.
    pub fn move_local_grid(&mut self, from: usize, to: usize) {
        let rows = self.local.value_mut();
        if from < rows.len() && to < rows.len() {
            let row = rows.remove(from);
            rows.insert(to, row);
        }
    }

    pub fn set_local_grid_name(&mut self, index: usize, name: &str) {
        if let Some(row) = self.local.value_mut().get_mut(index) {
            row.name = name.to_owned();
        }
    }

    pub fn set_local_grid_applies_to(&mut self, index: usize, applies_to: AppliesTo) {
        if let Some(row) = self.local.value_mut().get_mut(index) {
            row.applies_to = applies_to;
        }
    }

    /// The `pcb.local_grid.*` steps the Local Grid rows ask for.
    fn local_steps(&self) -> Vec<Step> {
        let (rows, base) = (self.local.value(), self.local.base());
        let mut steps = Vec::new();
        for row in rows {
            let Some(was) = base.iter().find(|r| r.uuid == row.uuid) else {
                continue;
            };
            let mut params = json!({ "uuid": row.uuid });
            if row.name.trim() != was.name {
                params["name"] = json!(row.name.trim());
            }
            if row.applies_to != was.applies_to {
                params["applies_to"] = json!(row.applies_to);
            }
            if params.as_object().is_some_and(|o| o.len() > 1) {
                steps.push(Step::new("pcb.local_grid.set", params));
            }
        }
        let now: Vec<Uuid> = base.iter().map(|r| r.uuid).collect();
        let new: Vec<Uuid> = rows.iter().map(|r| r.uuid).collect();
        let reorders = crate::panels::properties::local_grids::reorder_steps(&now, &new);
        steps.extend(
            (reorders.into_iter()).map(|params| Step::new("pcb.local_grid.reorder", params)),
        );
        steps
    }

    /// Adds an empty, In-cycle row to type a step into.
    pub fn add(&mut self) {
        self.rows.value_mut().push(BoardGridRow {
            uuid: None,
            step: String::new(),
            visible: String::new(),
            in_cycle: true,
            default: false,
        });
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.rows.value().len() {
            self.rows.value_mut().remove(index);
        }
    }

    pub fn set_step(&mut self, index: usize, text: &str) {
        if let Some(row) = self.rows.value_mut().get_mut(index) {
            row.step = text.to_owned();
        }
    }

    pub fn set_visible(&mut self, index: usize, text: &str) {
        if let Some(row) = self.rows.value_mut().get_mut(index) {
            row.visible = text.to_owned();
        }
    }

    pub fn set_in_cycle(&mut self, index: usize, on: bool) {
        if let Some(row) = self.rows.value_mut().get_mut(index) {
            row.in_cycle = on;
        }
    }

    /// Marks row `index` the Default, taking the mark off every other row; `false` clears
    /// it.
    pub fn set_default(&mut self, index: usize, on: bool) {
        for (i, row) in self.rows.value_mut().iter_mut().enumerate() {
            if i == index {
                row.default = on;
            } else if on {
                row.default = false;
            }
        }
    }

    /// The `pcb.board_grid.set` params that turn `base` into `row`, if it changed.
    fn set(base: &Parsed, base_row: &BoardGridRow, row: &BoardGridRow, now: &Parsed) -> Value {
        let mut params = json!({});
        if now.x != base.x {
            params["step_x"] = json!(now.x);
        }
        if now.y != base.y {
            params["step_y"] = json!(now.y);
        }
        if now.visible != base.visible {
            params["visible"] = json!(now.visible);
        }
        if row.in_cycle != base_row.in_cycle {
            params["in_cycle"] = json!(row.in_cycle);
        }
        if row.default != base_row.default {
            params["default"] = json!(row.default);
        }
        params
    }
}

impl Page for GridsPage {
    fn title(&self) -> String {
        tr!("board-setup-page-grids")
    }

    fn load(&mut self, project: &Project, reset: bool) {
        // Smallest step first, as G meets them.
        let mut grids: Vec<&BoardGrid> = project
            .pcb()
            .map(|pcb| pcb.board_grids().collect())
            .unwrap_or_default();
        grids.sort_by_key(|g| (g.x(), g.y(), g.uuid));
        let fresh: Vec<BoardGridRow> = grids.into_iter().map(BoardGridRow::of).collect();
        self.rows.load(fresh, reset);
        let local: Vec<LocalGridRow> = project
            .pcb()
            .map(|pcb| pcb.local_grids().map(LocalGridRow::of).collect())
            .unwrap_or_default();
        self.local.load(local, reset);
    }

    fn steps(&self) -> Vec<Step> {
        let rows = self.rows.value();
        let base = self.rows.base();
        let kept: Vec<Uuid> = rows.iter().filter_map(|r| r.uuid).collect();
        let deletes = (base.iter())
            .filter_map(|r| r.uuid)
            .filter(|u| !kept.contains(u))
            .map(|uuid| Step::new("pcb.board_grid.delete", json!({ "uuid": uuid })));
        let mut clears = Vec::new();
        let mut sets = Vec::new();
        let mut adds = Vec::new();
        for (i, row) in rows.iter().enumerate() {
            let Ok(now) = row.parse(i) else { continue };
            let Some(uuid) = row.uuid else {
                let params = json!({
                    "step_x": now.x,
                    "step_y": now.y,
                    "visible": now.visible,
                    "in_cycle": row.in_cycle,
                    "default": row.default,
                });
                let mut params = params;
                let object = params.as_object_mut().expect("an object");
                object.retain(|_, v| !v.is_null());
                adds.push(Step::new("pcb.board_grid.add", params));
                continue;
            };
            let Some(base_row) = base.iter().find(|r| r.uuid == Some(uuid)) else {
                continue;
            };
            let Ok(was) = base_row.parse(i) else { continue };
            let mut params = GridsPage::set(&was, base_row, row, &now);
            if params.as_object().is_some_and(|o| o.is_empty()) {
                continue;
            }
            params["uuid"] = json!(uuid);
            let step = Step::new("pcb.board_grid.set", params);
            // A row losing the Default mark goes first, so another can take it.
            if base_row.default && !row.default {
                clears.push(step);
            } else {
                sets.push(step);
            }
        }
        let mut steps: Vec<Step> = deletes.chain(clears).chain(sets).chain(adds).collect();
        steps.extend(self.local_steps());
        steps
    }

    fn problems(&self) -> Vec<Problem> {
        (self.rows.value().iter().enumerate())
            .filter_map(|(i, row)| row.parse(i).err())
            .flatten()
            .collect()
    }

    fn show(&mut self, ui: &mut Ui, cx: &mut PageCx<'_>) {
        let t = ui.tokens();
        layout::muted(ui, tr!("board-setup-grids-note"));
        ui.add_space(t.space.stack);
        let count = self.rows.value().len();
        if count == 0 {
            layout::muted(ui, tr!("board-setup-grids-none"));
        }
        let mut remove = None;
        let mut default = None;
        for i in 0..count {
            let step_problem = cx.problem(&step_field(i)).cloned();
            let visible_problem = cx.problem(&visible_field(i)).cloned();
            ui.horizontal_top(|ui| {
                let number = i + 1;
                let row = &mut self.rows.value_mut()[i];
                let label = tr!("board-setup-grids-step", number = number);
                field(ui, &label, &mut row.step, STEP_WIDTH, step_problem.as_ref());
                let label = tr!("board-setup-grids-visible", number = number);
                let visible = &mut row.visible;
                field(ui, &label, visible, VISIBLE_WIDTH, visible_problem.as_ref());
                let label = tr!("board-setup-grids-in-cycle");
                ui.add(widgets::checkbox(&mut row.in_cycle, &label));
                let mut marked = row.default;
                let label = tr!("board-setup-grids-default");
                if ui.add(widgets::checkbox(&mut marked, &label)).changed() {
                    default = Some((i, marked));
                }
                let name = tr!("board-setup-grids-remove", number = number);
                let button = ui.add(widgets::button(tr!("projects-remove")));
                button.widget_info(|| WidgetInfo::labeled(Role::Button, true, name.clone()));
                if button.clicked() {
                    remove = Some(i);
                }
            });
        }
        if let Some((i, on)) = default {
            self.set_default(i, on);
        }
        if let Some(i) = remove {
            self.remove(i);
        }
        if ui
            .add(widgets::button(tr!("board-setup-grids-add")))
            .clicked()
        {
            self.add();
        }
        self.show_local(ui);
    }
}

impl GridsPage {
    /// The Local Grid rows.
    fn show_local(&mut self, ui: &mut Ui) {
        let t = ui.tokens();
        ui.add_space(t.space.section);
        layout::muted(ui, tr!("board-setup-local-grids-note"));
        ui.add_space(t.space.stack);
        let count = self.local.value().len();
        if count == 0 {
            layout::muted(ui, tr!("board-setup-local-grids-none"));
        }
        let names: Vec<String> = self.local.value().iter().map(|r| r.name.clone()).collect();
        let grip = |i: usize| tr!("local-grid-drag", name = names[i].as_str());
        let mut picked = None;
        let rows = self.local.value_mut();
        let order = widgets::reorder_rows(ui, "board-setup-local-grids", count, grip, |ui, i| {
            let row = &mut rows[i];
            let label = tr!("board-setup-local-grids-name", number = i + 1);
            field(ui, &label, &mut row.name, STEP_WIDTH, None);
            let label = tr!("board-setup-local-grids-applies-to", number = i + 1);
            let menu = crate::panels::properties::local_grids::applies_to_menu;
            if let Some(a) = menu(ui, ("board-setup", row.uuid), &label, row.applies_to, true) {
                picked = Some((i, a));
            }
            ui.label(&row.step);
        });
        if let Some((i, a)) = picked {
            self.set_local_grid_applies_to(i, a);
        }
        if let Some((_, order)) = order {
            let rows = self.local.value().clone();
            self.local
                .set(order.into_iter().map(|i| rows[i].clone()).collect());
        }
    }
}

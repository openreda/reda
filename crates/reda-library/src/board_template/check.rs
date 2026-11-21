//! [`check`]: the load checks every `.redaboard` runs after its schema. A template that
//! fails any of them is a scan problem and can't be picked.

use std::collections::{BTreeMap, BTreeSet};

use reda_model::geom::expr;
use reda_model::{
    BoardTemplate, DrawingKind, TemplateDimensionEnd, TemplateDrawing, TemplateNumber,
    TemplateParameter, TemplatePoint, TemplateVertex, Uuid, When,
};

use super::eval::resolve;
use super::{BoardTemplateChoice, combinations, evaluate, gate_holds};

/// Every load-check failure of `template`, in file order; empty when it loads.
///
/// It checks that expressions parse and name only the template's names, that every
/// option of a choice binds the same names and `default` names an option, that every
/// `when` names a choice and its keys, that every `part`, `footprint`, `protects`, drawing
/// `group` and Dimension `follow` resolves, that exactly one outline row survives every
/// choice combination, and that `suggested_layers` is from 2 to 32. Each combination is
/// then evaluated at default lengths, so an expression that can't give its field's kind
/// is caught here too.
pub fn check(template: &BoardTemplate) -> Vec<String> {
    let mut c = Checker::default();
    c.header(template);
    c.parameters(template);
    if c.problems.is_empty() {
        c.rows(template);
    }
    if c.problems.is_empty() {
        c.combinations(template);
    }
    c.problems
}

#[derive(Default)]
struct Checker {
    problems: Vec<String>,
    /// Every expression name, with the parameter that provides it.
    names: BTreeSet<String>,
    /// Choice parameters and their option keys.
    choices: BTreeMap<String, Vec<String>>,
}

fn is_snake_case(name: &str) -> bool {
    let mut chars = name.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn layers_in_range(n: u8) -> bool {
    (2..=32).contains(&n)
}

impl Checker {
    fn problem(&mut self, message: impl Into<String>) {
        self.problems.push(message.into());
    }

    fn header(&mut self, t: &BoardTemplate) {
        if t.name.trim().is_empty() {
            self.problem("the template has an empty name");
        }
        if !layers_in_range(t.suggested_layers) {
            self.problem(format!(
                "suggested_layers is {}; it must be from 2 to 32",
                t.suggested_layers
            ));
        }
    }

    fn name(&mut self, name: &str, what: &str) {
        if !is_snake_case(name) {
            self.problem(format!("{what} `{name}` is not a snake_case name"));
        }
        if !self.names.insert(name.to_owned()) {
            self.problem(format!(
                "the name `{name}` is used twice; names must be unique across the template"
            ));
        }
    }

    fn parameters(&mut self, t: &BoardTemplate) {
        for param in &t.parameters {
            match param {
                TemplateParameter::Choice(choice) => {
                    let what = format!("choice `{}`", choice.name);
                    self.name(&choice.name, "parameter");
                    if choice.options.is_empty() {
                        self.problem(format!("{what} has no options"));
                        continue;
                    }
                    let keys: Vec<String> = choice.options.iter().map(|o| o.key.clone()).collect();
                    let unique: BTreeSet<&String> = keys.iter().collect();
                    if unique.len() != keys.len() {
                        self.problem(format!("{what} has two options with the same key"));
                    }
                    if let Some(default) = &choice.default
                        && !keys.contains(default)
                    {
                        self.problem(format!(
                            "{what}'s default `{default}` is not one of its options"
                        ));
                    }
                    let first: BTreeSet<&String> = choice.options[0].bind.keys().collect();
                    for option in &choice.options {
                        let bound: BTreeSet<&String> = option.bind.keys().collect();
                        if bound != first {
                            self.problem(format!(
                                "{what}: option `{}` binds {}, but option `{}` binds {}; \
                                 every option must bind the same names",
                                option.key,
                                list(&bound),
                                choice.options[0].key,
                                list(&first)
                            ));
                        }
                        if let Some(n) = option.suggested_layers
                            && !layers_in_range(n)
                        {
                            self.problem(format!(
                                "{what}: option `{}` suggests {n} layers; it must be from 2 to 32",
                                option.key
                            ));
                        }
                        for (name, value) in &option.bind {
                            self.constant(
                                value,
                                &format!("{what}: option `{}` binds `{name}`", option.key),
                            );
                        }
                    }
                    for name in first {
                        self.name(name, "bound name");
                    }
                    self.choices.insert(choice.name.clone(), keys);
                }
                TemplateParameter::Length(length) => {
                    let what = format!("length `{}`", length.name);
                    self.name(&length.name, "parameter");
                    for (field, value) in [
                        ("default", &length.default),
                        ("min", &length.min),
                        ("max", &length.max),
                    ] {
                        self.constant(value, &format!("{what} {field}"));
                    }
                }
            }
        }
        // `when` on parameters, now every choice is known.
        for param in &t.parameters {
            self.when(param.when(), &format!("parameter `{}`", param.name()));
        }
        if self.problems.is_empty()
            && let Err(e) = resolve(t, &BTreeMap::new())
        {
            self.problem(e.message);
        }
        if self.problems.is_empty() {
            for param in &t.parameters {
                if let TemplateParameter::Length(l) = param {
                    let ns = super::Namespace::empty();
                    let (Ok(d), Ok(min), Ok(max)) =
                        (ns.length(&l.default), ns.length(&l.min), ns.length(&l.max))
                    else {
                        continue;
                    };
                    if min > max || d < min || d > max {
                        self.problem(format!("length `{}` needs min ≤ default ≤ max", l.name));
                    }
                }
            }
        }
    }

    /// A value that must not name anything: a length parameter's bounds and a bound value.
    fn constant(&mut self, value: &TemplateNumber, what: &str) {
        if let Some(text) = value.expression() {
            match expr::references(text) {
                Err(e) => self.problem(format!("{what}: `{text}`: {e}")),
                Ok(r) if r.current || !r.names.is_empty() => {
                    self.problem(format!("{what}: `{text}` must be a plain value"));
                }
                Ok(_) => {}
            }
        }
    }

    fn expression(&mut self, value: &TemplateNumber, what: &str) {
        let Some(text) = value.expression() else {
            return;
        };
        match expr::references(text) {
            Err(e) => self.problem(format!("{what}: `{text}`: {e}")),
            Ok(r) => {
                if r.current {
                    self.problem(format!(
                        "{what}: `{text}`: `!` is not allowed in a template"
                    ));
                }
                for name in r.names {
                    if !self.names.contains(&name) {
                        self.problem(format!(
                            "{what}: `{text}` names `{name}`, which is not a parameter or bound name"
                        ));
                    }
                }
            }
        }
    }

    fn point(&mut self, p: &TemplatePoint, what: &str) {
        self.expression(&p.x, &format!("{what} x"));
        self.expression(&p.y, &format!("{what} y"));
    }

    fn vertices(&mut self, vertices: &[TemplateVertex], what: &str, plated_allowed: bool) {
        for (i, v) in vertices.iter().enumerate() {
            let at = format!("{what} vertex {i}");
            self.expression(&v.x, &format!("{at} x"));
            self.expression(&v.y, &format!("{at} y"));
            if let Some(s) = &v.sweep {
                self.expression(s, &format!("{at} sweep"));
            }
            if v.plated && !plated_allowed {
                self.problem(format!(
                    "{at} is marked plated; only outline and cutout edges can be"
                ));
            }
        }
    }

    fn when(&mut self, when: Option<&When>, what: &str) {
        let Some(when) = when else { return };
        for (param, keys) in when {
            let Some(options) = self.choices.get(param) else {
                self.problem(format!(
                    "{what}: `when` names `{param}`, which is not a choice parameter"
                ));
                continue;
            };
            let unknown: Vec<&String> = keys.iter().filter(|k| !options.contains(k)).collect();
            if !unknown.is_empty() {
                self.problem(format!(
                    "{what}: `when` names {} of `{param}`, which it does not have",
                    unknown
                        .iter()
                        .map(|k| format!("`{k}`"))
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
    }

    fn rows(&mut self, t: &BoardTemplate) {
        let mut seen = BTreeSet::new();
        let mut unique = |c: &mut Checker, uuid: Uuid, what: &str| {
            if !seen.insert(uuid) {
                c.problem(format!("{what} {uuid}: the UUID is used by another row"));
            }
        };
        if t.outline.is_empty() {
            self.problem("the template has no outline row");
        }
        for (rows, what) in [(&t.outline, "outline"), (&t.cutouts, "cutout")] {
            for row in rows {
                let what = format!("{what} {}", row.uuid);
                unique(self, row.uuid, &what);
                self.vertices(&row.vertices, &what, true);
                self.when(row.when.as_ref(), &what);
            }
        }
        let parts: BTreeMap<Uuid, &reda_model::TemplatePart> =
            t.parts.iter().map(|p| (p.uuid, p)).collect();
        let copies: BTreeSet<Uuid> = t.footprint_copies.iter().map(|c| c.uuid).collect();
        for entry in &t.parts {
            unique(self, entry.uuid, "part entry");
            if let Some(copy) = entry.footprints.iter().find(|c| !copies.contains(c)) {
                self.problem(format!(
                    "part entry {} lists Footprint copy {copy}, which footprint_copies does not hold",
                    entry.uuid
                ));
            }
            if entry.footprints.len() != entry.part.footprints.len() {
                self.problem(format!(
                    "part entry {} needs one Footprint copy per alternate Footprint",
                    entry.uuid
                ));
            }
        }
        for copy in &t.footprint_copies {
            unique(self, copy.uuid, "Footprint copy");
        }
        let mut placed = BTreeSet::new();
        let placement = |c: &mut Checker, what: &str, part: Uuid, footprint: Uuid| {
            match parts.get(&part) {
            None => c.problem(format!("{what} names part entry {part}, which parts does not hold")),
            Some(entry) if !entry.footprints.contains(&footprint) => c.problem(format!(
                "{what} names Footprint copy {footprint}, which is not one of part entry {part}'s copies"
            )),
            Some(_) => {}
        }
        };
        for row in &t.connectors {
            let what = format!("connector {}", row.uuid);
            unique(self, row.uuid, &what);
            placed.insert(row.uuid);
            placement(self, &what, row.part, row.footprint);
            self.point(&row.at, &format!("{what} at"));
            self.expression(&row.rotation, &format!("{what} rotation"));
            self.when(row.when.as_ref(), &what);
        }
        for row in &t.holes {
            let what = format!("hole {}", row.uuid);
            unique(self, row.uuid, &what);
            placed.insert(row.uuid);
            match (&row.pad, row.part, row.footprint) {
                (Some(pad), None, None) => {
                    if row.side.is_some() {
                        self.problem(format!(
                            "{what}: a Free Pad row has no `side`; set the Pad's own"
                        ));
                    }
                    self.expression(&pad.width, &format!("{what} pad width"));
                    self.expression(&pad.height, &format!("{what} pad height"));
                    if let Some(o) = &pad.offset {
                        self.point(o, &format!("{what} pad offset"));
                    }
                    if let Some(h) = &pad.hole {
                        self.expression(&h.width, &format!("{what} pad hole width"));
                        self.expression(&h.height, &format!("{what} pad hole height"));
                        if let Some(r) = &h.rotation {
                            self.expression(r, &format!("{what} pad hole rotation"));
                        }
                    }
                }
                (None, Some(part), Some(footprint)) => {
                    placement(self, &what, part, footprint);
                    if row.side.is_none() {
                        self.problem(format!("{what}: a Mechanical Part row needs a `side`"));
                    }
                }
                _ => self.problem(format!(
                    "{what} must be a Mechanical Part row (`part`, `footprint`, `side`) or a \
                     Free Pad row (`pad`)"
                )),
            }
            self.point(&row.at, &format!("{what} at"));
            self.expression(&row.rotation, &format!("{what} rotation"));
            self.when(row.when.as_ref(), &what);
        }
        let mut followable: BTreeMap<Uuid, Option<usize>> = BTreeMap::new();
        for row in t.outline.iter().chain(&t.cutouts) {
            followable.insert(row.uuid, Some(row.vertices.len()));
        }
        for uuid in &placed {
            followable.insert(*uuid, None);
        }
        for row in &t.keepouts {
            let what = format!("Keepout {}", row.uuid);
            unique(self, row.uuid, &what);
            followable.insert(row.uuid, Some(row.vertices.len()));
            self.vertices(&row.vertices, &what, false);
            if let Some(w) = &row.width {
                self.expression(w, &format!("{what} width"));
            }
            if let Some(p) = row.protects
                && !placed.contains(&p)
            {
                self.problem(format!(
                    "{what} protects {p}, which is not a connector or hole"
                ));
            }
            self.when(row.when.as_ref(), &what);
        }
        let groups: BTreeSet<Uuid> = t.drawing_groups.iter().map(|g| g.uuid).collect();
        for group in &t.drawing_groups {
            let what = format!("drawing group {}", group.uuid);
            unique(self, group.uuid, &what);
            self.when(group.when.as_ref(), &what);
        }
        for row in &t.drawings {
            let what = format!("drawing {}", row.uuid);
            unique(self, row.uuid, &what);
            if !groups.contains(&row.group) {
                self.problem(format!(
                    "{what} is in group {}, which drawing_groups does not hold",
                    row.group
                ));
            }
            self.drawing(row, &what, &followable);
            self.when(row.when.as_ref(), &what);
        }
        if let Some(edge) = t.board.as_ref().and_then(|b| b.edge_connector.as_ref())
            && let Some(bevel) = &edge.bevel
        {
            self.expression(&bevel.angle, "board edge_connector bevel angle");
            self.expression(&bevel.depth, "board edge_connector bevel depth");
        }
    }

    fn drawing(
        &mut self,
        row: &TemplateDrawing,
        what: &str,
        followable: &BTreeMap<Uuid, Option<usize>>,
    ) {
        let need = |c: &mut Checker, has: bool, field: &str| {
            if !has {
                c.problem(
                    format!("{what}: a {:?} drawing needs `{field}`", row.kind).to_lowercase(),
                );
            }
        };
        match row.kind {
            DrawingKind::Line | DrawingKind::Arc | DrawingKind::Rectangle => {
                need(self, row.from.is_some(), "from");
                need(self, row.to.is_some(), "to");
                need(self, row.width.is_some(), "width");
                if row.kind == DrawingKind::Arc {
                    need(self, row.sweep.is_some(), "sweep");
                }
            }
            DrawingKind::Text => {
                need(self, row.text.is_some(), "text");
                need(self, row.position.is_some(), "position");
                need(self, row.height.is_some(), "height");
            }
            DrawingKind::Dimension => {
                need(self, row.a.is_some(), "a");
                need(self, row.b.is_some(), "b");
            }
        }
        for (field, p) in [
            ("from", &row.from),
            ("to", &row.to),
            ("position", &row.position),
        ] {
            if let Some(p) = p {
                self.point(p, &format!("{what} {field}"));
            }
        }
        for (field, n) in [
            ("width", &row.width),
            ("sweep", &row.sweep),
            ("height", &row.height),
            ("rotation", &row.rotation),
            ("offset", &row.offset),
        ] {
            if let Some(n) = n {
                self.expression(n, &format!("{what} {field}"));
            }
        }
        for (field, end) in [("a", &row.a), ("b", &row.b)] {
            if let Some(end) = end {
                self.dimension_end(end, &format!("{what} {field}"), followable);
            }
        }
    }

    fn dimension_end(
        &mut self,
        end: &TemplateDimensionEnd,
        what: &str,
        followable: &BTreeMap<Uuid, Option<usize>>,
    ) {
        match (&end.follow, &end.x, &end.y) {
            (Some(follow), None, None) => match followable.get(&follow.object) {
                None => self.problem(format!(
                    "{what} follows {}, which is not an outline, cutout, Keepout, hole or connector",
                    follow.object
                )),
                Some(Some(count)) => {
                    if follow.vertex.unwrap_or(0) >= *count {
                        self.problem(format!(
                            "{what} follows vertex {} of {}, which has {count}",
                            follow.vertex.unwrap_or(0),
                            follow.object
                        ));
                    }
                }
                Some(None) => {
                    if follow.vertex.is_some() {
                        self.problem(format!("{what}: a hole or connector has no vertices to follow"));
                    }
                }
            },
            (None, Some(x), Some(y)) => {
                self.expression(x, &format!("{what} x"));
                self.expression(y, &format!("{what} y"));
            }
            _ => self.problem(format!("{what} needs `x` and `y`, or `follow` alone")),
        }
    }

    fn combinations(&mut self, t: &BoardTemplate) {
        for combo in combinations(t) {
            let shown = || {
                if combo.is_empty() {
                    "at its defaults".to_owned()
                } else {
                    format!(
                        "at {}",
                        combo
                            .iter()
                            .map(|(k, v)| format!("{k}={v}"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                }
            };
            let outlines = t
                .outline
                .iter()
                .filter(|r| gate_holds(r.when.as_ref(), &combo))
                .count();
            if outlines != 1 {
                self.problem(format!(
                    "{outlines} outline rows survive {}; exactly one must survive every choice combination",
                    shown()
                ));
                continue;
            }
            let choice = BoardTemplateChoice {
                element: t.uuid,
                values: combo
                    .iter()
                    .map(|(k, v)| (k.clone(), super::TemplateValue::Text(v.clone())))
                    .collect(),
                exclude: vec![],
            };
            if let Err(e) = evaluate(t, &choice) {
                self.problem(format!("{} {}", e.message, shown()));
            }
        }
    }
}

fn list(names: &BTreeSet<&String>) -> String {
    if names.is_empty() {
        return "nothing".into();
    }
    names
        .iter()
        .map(|n| format!("`{n}`"))
        .collect::<Vec<_>>()
        .join(", ")
}

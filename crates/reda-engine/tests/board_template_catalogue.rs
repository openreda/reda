//! The built-in Board Template catalogue (`docs/spec/v0.2.5/testing.md#built-in-catalogue`):
//! every `.redaboard` in the starter Library's `board-templates/`, checked as data.
//!
//! - **Exhaustive data test:** every choice combination of every built-in, with each
//!   length at its min, default and max, must load, keep exactly one outline, pass the
//!   outline, cutout and Keepout checks, and embed Part copies whose `hash` matches the
//!   starter Library's. The points listed in `board_template_catalogue/expected_invalid.tsv`
//!   must fail instead, and nothing else may.
//! - **Frame goldens:** `board_template_catalogue/frames/<file stem>/<combination>.json` is
//!   the `library.board_template.preview` result at each choice combination, default
//!   lengths.
//! - **Spec numbers:** each row of `board_template_catalogue/board-template-dimensions.tsv`
//!   is measured in the evaluated frame, and needs its sources.
//!
//! Adding a built-in is a data change: drop its `.redaboard` into
//! `library/starter/board-templates/`, add its dimension rows and any expected-invalid
//! points, and run `REDA_BLESS=1 cargo test -p reda-engine --test board_template_catalogue`
//! to canonicalise the file and write its frame goldens.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use reda_engine::format::{read_board_template, write_board_template};
use reda_engine::library::board_template::{
    BoardTemplateChoice, Combination, Frame, ResolvedValue, TemplateValue, check, combinations,
    constant_length, evaluate,
};
use reda_engine::library::{DiskLibraryFiles, LibraryOrigin, LibraryRoot, copy_part, scan};
use reda_engine::model::geom::expr::{self, Field, Kind, Object};
use reda_engine::model::geom::{Bounds, Millideg, Point, Vertex, arc_centre, contour_bounds};
use reda_engine::model::{BoardTemplate, TemplateParameter};
use reda_testkit::{assert_golden, starter_library, test_engine_with_library};
use serde_json::json;

const DATA: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/board_template_catalogue"
);

fn bless() -> bool {
    std::env::var_os("REDA_BLESS").is_some_and(|v| v == "1")
}

/// Every built-in: its file stem, path and template, by file name.
fn builtins() -> Vec<(String, PathBuf, BoardTemplate)> {
    let dir = starter_library().join("board-templates");
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "redaboard"))
        .collect();
    files.sort();
    assert!(
        !files.is_empty(),
        "no built-in Board Templates in {}",
        dir.display()
    );
    files
        .into_iter()
        .map(|path| {
            let bytes = std::fs::read(&path).unwrap();
            let template = read_board_template(&path, &bytes).unwrap_or_else(|e| panic!("{e}"));
            let stem = path.file_stem().unwrap().to_string_lossy().into_owned();
            (stem, path, template)
        })
        .collect()
}

fn mm(nm: i64) -> String {
    reda_engine::model::geom::exact_decimal(nm, 1_000_000)
}

/// A point of the walk: each parameter's value as given, in parameter order.
type Values = Vec<(String, TemplateValue)>;

fn shown(values: &Values) -> String {
    if values.is_empty() {
        return "default".into();
    }
    values
        .iter()
        .map(|(name, value)| match value {
            TemplateValue::Text(t) => format!("{name}={t}"),
            TemplateValue::Number(n) => format!("{name}={}", mm(*n)),
        })
        .collect::<Vec<_>>()
        .join(",")
}

fn choice(template: &BoardTemplate, values: &Values) -> BoardTemplateChoice {
    BoardTemplateChoice {
        element: template.uuid,
        values: values.iter().cloned().collect(),
        exclude: vec![],
    }
}

/// The combination's choices, then every length at its min, default and max.
fn points(template: &BoardTemplate, combo: &Combination) -> Vec<Values> {
    let mut out: Vec<Values> = vec![vec![]];
    for param in &template.parameters {
        let options: Vec<TemplateValue> = match param {
            TemplateParameter::Choice(c) => vec![TemplateValue::Text(combo[&c.name].clone())],
            TemplateParameter::Length(l) => {
                let mut v: Vec<i64> = [&l.min, &l.default, &l.max]
                    .iter()
                    .map(|n| constant_length(n).unwrap().0)
                    .collect();
                v.dedup();
                v.into_iter().map(TemplateValue::Number).collect()
            }
        };
        out = out
            .into_iter()
            .flat_map(|point| {
                options.iter().map(move |o| {
                    let mut next = point.clone();
                    next.push((param.name().to_owned(), o.clone()));
                    next
                })
            })
            .collect();
    }
    out
}

/// `template<TAB>point` rows, `#` comments and blank lines skipped.
fn expected_invalid() -> BTreeSet<(String, String)> {
    let text = std::fs::read_to_string(format!("{DATA}/expected_invalid.tsv")).unwrap();
    text.lines()
        .filter(|l| !l.trim().is_empty() && !l.starts_with('#'))
        .map(|l| {
            let (t, p) = l
                .split_once('\t')
                .unwrap_or_else(|| panic!("expected_invalid.tsv: {l:?} is not template<TAB>point"));
            (t.to_owned(), p.to_owned())
        })
        .collect()
}

#[test]
fn every_builtin_is_canonical_and_passes_its_load_checks() {
    let index = scan(
        &[LibraryRoot::new(starter_library(), LibraryOrigin::Starter)],
        &mut DiskLibraryFiles,
    );
    let template_problems: Vec<_> = index
        .problems()
        .iter()
        .filter(|p| p.path.to_string_lossy().contains("board-templates"))
        .collect();
    assert!(template_problems.is_empty(), "{template_problems:#?}");
    let mut names = BTreeSet::new();
    for (stem, path, template) in builtins() {
        assert_eq!(check(&template), Vec::<String>::new(), "{stem}");
        assert!(
            names.insert(template.name.clone()),
            "{stem}: name used twice"
        );
        assert!(
            index.board_template(template.uuid).is_some(),
            "{stem} is not scanned"
        );
        assert_eq!(
            template.suggested_layers, 2,
            "{stem}: every built-in suggests 2 layers"
        );
        // The file is the canonical writing of what it holds. Blessing rewrites it only
        // when it differs, so the other tests never read a half-written file.
        let canonical = write_board_template(&template).unwrap();
        if std::fs::read(&path).unwrap() != canonical {
            assert_golden(&path, &canonical);
        }
    }
}

#[test]
fn every_point_of_every_builtin_is_valid_except_the_expected_invalid_ones() {
    let starter = scan(
        &[LibraryRoot::new(starter_library(), LibraryOrigin::Starter)],
        &mut DiskLibraryFiles,
    );
    let expected = expected_invalid();
    let mut invalid = BTreeSet::new();
    let mut walked = BTreeSet::new();
    for (stem, _, template) in builtins() {
        for entry in &template.parts {
            let copy = copy_part(&starter, entry.part.uuid)
                .unwrap_or_else(|e| panic!("{stem}: Part {}: {}", entry.part.name, e.message));
            assert_eq!(
                entry.hash, copy.hash,
                "{stem}: the copy of {} differs from the starter Library's",
                entry.part.name
            );
        }
        for combo in combinations(&template) {
            for values in points(&template, &combo) {
                let point = (template.name.clone(), shown(&values));
                walked.insert(point.clone());
                let frame = evaluate(&template, &choice(&template, &values))
                    .unwrap_or_else(|e| panic!("{} at {}: {}", point.0, point.1, e.message));
                let outlines = template
                    .outline
                    .iter()
                    .filter(|r| {
                        reda_engine::library::board_template::gate_holds(r.when.as_ref(), &combo)
                    })
                    .count();
                assert_eq!(outlines, 1, "{} at {}", point.0, point.1);
                if !frame.is_valid() {
                    if !expected.contains(&point) {
                        panic!(
                            "{} at {} is invalid but not in expected_invalid.tsv: {:?}",
                            point.0, point.1, frame.problems
                        );
                    }
                    invalid.insert(point);
                }
            }
        }
    }
    let unknown: Vec<_> = expected.difference(&walked).collect();
    assert!(
        unknown.is_empty(),
        "expected_invalid.tsv names points the walk never reaches: {unknown:#?}"
    );
    let valid: Vec<_> = expected.difference(&invalid).collect();
    assert!(
        valid.is_empty(),
        "these expected-invalid points are valid: {valid:#?}"
    );
}

#[test]
fn frame_goldens_hold_the_preview_of_every_choice_combination() {
    let mut engine = test_engine_with_library(&starter_library());
    let frames = Path::new(DATA).join("frames");
    let mut written = BTreeSet::new();
    for (stem, _, template) in builtins() {
        for combo in combinations(&template) {
            let values: Values = template
                .parameters
                .iter()
                .filter_map(|p| {
                    combo
                        .get(p.name())
                        .map(|k| (p.name().to_owned(), TemplateValue::Text(k.clone())))
                })
                .collect();
            let params = json!({
                "element": template.uuid,
                "values": values.iter().cloned().collect::<BTreeMap<_, _>>(),
            });
            let frame = engine.run_ok("library.board_template.preview", params);
            let rel = format!("{stem}/{}.json", shown(&values));
            let mut text = serde_json::to_string_pretty(&frame).unwrap();
            text.push('\n');
            assert_golden(frames.join(&rel), text.as_bytes());
            written.insert(rel);
        }
    }
    let mut stale = vec![];
    for dir in std::fs::read_dir(&frames).unwrap().flatten() {
        for file in std::fs::read_dir(dir.path()).unwrap().flatten() {
            let rel = format!(
                "{}/{}",
                dir.file_name().to_string_lossy(),
                file.file_name().to_string_lossy()
            );
            if !written.contains(&rel) {
                if bless() {
                    std::fs::remove_file(file.path()).unwrap();
                } else {
                    stale.push(rel);
                }
            }
        }
    }
    assert!(
        stale.is_empty(),
        "frame goldens no combination makes (REDA_BLESS=1 removes them): {stale:?}"
    );
}

// ---------------------------------------------------------------------------------------
// Spec numbers.

/// One row of `board-template-dimensions.tsv`.
struct Row {
    line: usize,
    template: String,
    combination: String,
    quantity: String,
    expected: String,
    tolerance: String,
    sources: String,
}

fn dimension_rows() -> Vec<Row> {
    let text = std::fs::read_to_string(format!("{DATA}/board-template-dimensions.tsv")).unwrap();
    let mut lines = text
        .lines()
        .enumerate()
        .filter(|(_, l)| !l.trim().is_empty() && !l.starts_with('#'));
    let (_, header) = lines.next().expect("a header row");
    assert_eq!(
        header, "template\tcombination\tquantity\texpected\ttolerance\tsources",
        "board-template-dimensions.tsv header"
    );
    lines
        .map(|(i, l)| {
            let cells: Vec<&str> = l.split('\t').collect();
            assert_eq!(
                cells.len(),
                6,
                "line {}: needs 6 tab-separated cells: {l:?}",
                i + 1
            );
            Row {
                line: i + 1,
                template: cells[0].into(),
                combination: cells[1].into(),
                quantity: cells[2].into(),
                expected: cells[3].into(),
                tolerance: cells[4].into(),
                sources: cells[5].into(),
            }
        })
        .collect()
}

/// Whether `sources` meets the rule: two independent sources separated by `;`, or one
/// marked `primary:`, `decided #<ticket>:` (a single source a ticket accepted) or
/// `unverified:` (a Documentation drawing).
fn sources_ok(sources: &str) -> bool {
    let s = sources.trim();
    if let Some(rest) = s.strip_prefix("decided #") {
        return rest.split_once(':').is_some_and(|(n, src)| {
            n.chars().all(|c| c.is_ascii_digit()) && !n.is_empty() && !src.trim().is_empty()
        });
    }
    for mark in ["primary:", "unverified:"] {
        if let Some(rest) = s.strip_prefix(mark) {
            return !rest.trim().is_empty();
        }
    }
    s.split(';').filter(|p| !p.trim().is_empty()).count() >= 2
}

/// The frame at a row's combination: `default`, or `name=value` pairs (an option key, or
/// a length in mm) separated by commas.
fn frame_at(template: &BoardTemplate, combination: &str) -> Result<Frame, String> {
    let mut values = BTreeMap::new();
    if combination != "default" {
        for pair in combination.split(',') {
            let (name, value) = pair
                .split_once('=')
                .ok_or_else(|| format!("combination `{pair}` is not name=value"))?;
            values.insert(
                name.trim().to_owned(),
                TemplateValue::Text(value.trim().to_owned()),
            );
        }
    }
    evaluate(
        template,
        &BoardTemplateChoice {
            element: template.uuid,
            values,
            exclude: vec![],
        },
    )
    .map_err(|e| e.message)
}

/// A measured value in nm, or millidegrees for an angle.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Measured {
    Length(i64),
    Angle(i64),
}

/// The bounding box and arc radii of a closed contour.
fn contour_measure(vertices: &[Vertex], measure: &str) -> Result<i64, String> {
    let bounds: Bounds = contour_bounds(vertices).ok_or("an empty contour")?;
    Ok(match measure {
        "width" => (bounds.max.x - bounds.min.x).0,
        "height" => (bounds.max.y - bounds.min.y).0,
        "min_x" => bounds.min.x.0,
        "min_y" => bounds.min.y.0,
        "max_x" => bounds.max.x.0,
        "max_y" => bounds.max.y.0,
        "corner_radius" => {
            let radii: BTreeSet<i64> = vertices
                .iter()
                .enumerate()
                .filter(|(_, v)| v.sweep != Millideg::ZERO)
                .map(|(i, v)| {
                    let to = vertices[(i + 1) % vertices.len()].point();
                    let c = arc_centre(v.point(), to, v.sweep);
                    let (dx, dy) = ((v.x - c.x).0 as f64, (v.y - c.y).0 as f64);
                    dx.hypot(dy).round() as i64
                })
                .collect();
            match radii.len() {
                0 => 0,
                1 => *radii.first().unwrap(),
                _ => return Err(format!("its arcs have different radii: {radii:?}")),
            }
        }
        other => {
            if let Some(rest) = other.strip_prefix("arc:") {
                let (i, what) = rest.split_once('/').ok_or("arc:<i>/<measure>")?;
                let i: usize = i
                    .parse()
                    .map_err(|_| format!("`{i}` is not a vertex index"))?;
                return arc_measure(vertices, i, what);
            }
            if let Some(rest) = other.strip_prefix("vertex:") {
                let (i, axis) = rest.split_once('/').ok_or("vertex:<i>/x or /y")?;
                let i: usize = i
                    .parse()
                    .map_err(|_| format!("`{i}` is not a vertex index"))?;
                let v = vertices.get(i).ok_or(format!("there is no vertex {i}"))?;
                return axis_of(v.point(), axis);
            }
            return Err(format!("unknown contour measure `{other}`"));
        }
    })
}

/// The arc leaving vertex `i`: its centre (`x`, `y`), `radius`, or the point halfway
/// along it (`mid/x`, `mid/y`), such as the tip of a full-radius notch.
fn arc_measure(vertices: &[Vertex], i: usize, what: &str) -> Result<i64, String> {
    let v = vertices.get(i).ok_or(format!("there is no vertex {i}"))?;
    if v.sweep == Millideg::ZERO {
        return Err(format!("the edge leaving vertex {i} is not an arc"));
    }
    let to = vertices[(i + 1) % vertices.len()].point();
    let c = arc_centre(v.point(), to, v.sweep);
    let (dx, dy) = ((v.x - c.x).0 as f64, (v.y - c.y).0 as f64);
    let half = (v.sweep.0 as f64 / 2_000.0).to_radians();
    let mid = (
        c.x.0 as f64 + dx * half.cos() - dy * half.sin(),
        c.y.0 as f64 + dx * half.sin() + dy * half.cos(),
    );
    match what {
        "x" => Ok(c.x.0),
        "y" => Ok(c.y.0),
        "radius" => Ok(dx.hypot(dy).round() as i64),
        "mid/x" => Ok(mid.0.round() as i64),
        "mid/y" => Ok(mid.1.round() as i64),
        other => Err(format!(
            "unknown arc measure `{other}`; use x, y, radius, mid/x or mid/y"
        )),
    }
}

fn axis_of(p: Point, axis: &str) -> Result<i64, String> {
    match axis {
        "x" => Ok(p.x.0),
        "y" => Ok(p.y.0),
        other => Err(format!("unknown axis `{other}`; use x or y")),
    }
}

/// Measures one quantity term, `<object>/<measure>`:
///
/// - `outline/…`, `cutout:<uuid>/…`, `keepout:<uuid>/…`: `width`, `height`, `min_x`,
///   `min_y`, `max_x`, `max_y`, `corner_radius`, `vertex:<i>/x|y` or
///   `arc:<i>/x|y|radius|mid/x|mid/y` (the arc leaving vertex `i`);
/// - `hole:<uuid>/…`, `connector:<uuid>/…`: `x`, `y`, `rotation`, `drill` (the first Pad's
///   hole width), or `pad:<number>/x|y|drill|width|height|min_x|min_y|max_x|max_y`
///   (`width`, `height` and the bounds are the Pad's copper extent on the board), or
///   `pad:<number>/bottom_width`, the width of its bottom land in the Footprint copy;
/// - `drawing:<uuid>/…`: `from/x|y`, `to/x|y`, `position/x|y`;
/// - `board/bevel_angle`, `board/bevel_depth`: the `edge_connector` bevel.
fn measure_term(template: &BoardTemplate, frame: &Frame, term: &str) -> Result<Measured, String> {
    let (object, measure) = term
        .split_once('/')
        .ok_or_else(|| format!("`{term}` is not <object>/<measure>"))?;
    let (kind, uuid) = match object.split_once(':') {
        Some((kind, uuid)) => (
            kind,
            Some(
                uuid.parse::<reda_engine::model::Uuid>()
                    .map_err(|_| format!("`{uuid}` is not a UUID"))?,
            ),
        ),
        None => (object, None),
    };
    let len = Measured::Length;
    match (kind, uuid) {
        ("board", None) => {
            let bevel = (frame.board.as_ref())
                .and_then(|b| b.edge_connector.as_ref())
                .and_then(|e| e.bevel.as_ref())
                .ok_or("the board sets no edge-connector bevel")?;
            match measure {
                "bevel_angle" => Ok(Measured::Angle(bevel.angle.0)),
                "bevel_depth" => Ok(len(bevel.depth.0)),
                other => Err(format!("unknown board measure `{other}`")),
            }
        }
        ("outline", None) => contour_measure(&frame.outline.contour(), measure).map(len),
        ("cutout", Some(u)) => {
            let c = frame
                .cutouts
                .iter()
                .find(|c| c.uuid == u)
                .ok_or(format!("no cutout {u} in the frame"))?;
            contour_measure(&c.contour(), measure).map(len)
        }
        ("keepout", Some(u)) => {
            let k = frame
                .keepouts
                .iter()
                .find(|k| k.uuid == u)
                .ok_or(format!("no Keepout {u} in the frame"))?;
            contour_measure(&k.vertices, measure).map(len)
        }
        ("hole" | "connector", Some(u)) => {
            let (at, rotation, pads, copy) = if kind == "hole" {
                let h = frame
                    .holes
                    .iter()
                    .find(|h| h.uuid == u)
                    .ok_or(format!("no hole {u} in the frame"))?;
                (h.at, h.rotation, &h.pads, h.footprint)
            } else {
                let c = frame
                    .connectors
                    .iter()
                    .find(|c| c.uuid == u)
                    .ok_or(format!("no connector {u} in the frame"))?;
                (c.at, c.rotation, &c.pads, Some(c.footprint))
            };
            let drill = |pad: &reda_engine::library::board_template::FramePad| {
                pad.hole
                    .map(|h| h.width.0)
                    .ok_or_else(|| "the Pad has no hole".to_owned())
            };
            match measure {
                "x" | "y" => axis_of(at, measure).map(len),
                "rotation" => Ok(Measured::Angle(rotation.0)),
                "drill" => drill(pads.first().ok_or("it has no Pads")?).map(len),
                other => {
                    let rest = other
                        .strip_prefix("pad:")
                        .ok_or(format!("unknown measure `{other}`"))?;
                    let (number, what) = rest.split_once('/').ok_or("pad:<number>/<measure>")?;
                    let pad = pads
                        .iter()
                        .find(|p| p.number == number)
                        .ok_or(format!("no Pad {number}"))?;
                    let b = contour_bounds(&pad.outline).ok_or("an empty Pad outline")?;
                    match what {
                        "drill" => drill(pad).map(len),
                        "width" => Ok(len((b.max.x - b.min.x).0)),
                        "height" => Ok(len((b.max.y - b.min.y).0)),
                        "min_x" => Ok(len(b.min.x.0)),
                        "min_y" => Ok(len(b.min.y.0)),
                        "max_x" => Ok(len(b.max.x.0)),
                        "max_y" => Ok(len(b.max.y.0)),
                        "bottom_width" => {
                            let copy = (template.footprint_copies.iter())
                                .find(|c| Some(c.uuid) == copy)
                                .ok_or("no Footprint copy")?;
                            let pad = (copy.footprint.pads.iter())
                                .find(|p| p.number == number)
                                .ok_or(format!("no Pad {number} in the Footprint copy"))?;
                            Ok(len(pad.bottom.as_ref().map_or(pad.width, |b| b.width).0))
                        }
                        axis => axis_of(pad.at, axis).map(len),
                    }
                }
            }
        }
        ("drawing", Some(u)) => {
            let d = frame
                .drawing_groups
                .iter()
                .flat_map(|g| &g.drawings)
                .find(|d| d.uuid == u)
                .ok_or(format!("no drawing {u} in the frame"))?;
            let (which, axis) = measure.split_once('/').ok_or("from/x, to/y, position/x…")?;
            let p = match which {
                "from" => d.from,
                "to" => d.to,
                "position" => d.position,
                other => return Err(format!("unknown drawing point `{other}`")),
            };
            axis_of(p.ok_or(format!("the drawing has no `{which}`"))?, axis).map(len)
        }
        _ => Err(format!("unknown object `{object}`")),
    }
}

/// A quantity: one term, or the difference of two (`a - b`), such as a hole pitch.
fn measure(template: &BoardTemplate, frame: &Frame, quantity: &str) -> Result<Measured, String> {
    match quantity.split_once(" - ") {
        None => measure_term(template, frame, quantity.trim()),
        Some((a, b)) => match (
            measure_term(template, frame, a.trim())?,
            measure_term(template, frame, b.trim())?,
        ) {
            (Measured::Length(a), Measured::Length(b)) => Ok(Measured::Length(a - b)),
            (Measured::Angle(a), Measured::Angle(b)) => Ok(Measured::Angle(a - b)),
            _ => Err("can't subtract an angle and a length".into()),
        },
    }
}

/// A number as the row writes it: mm for a length, degrees for an angle.
fn parse_value(text: &str, angle: bool) -> Result<i64, String> {
    let field = if angle {
        Field {
            kind: Kind::Angle,
            unit: "deg",
        }
    } else {
        Field {
            kind: Kind::Length,
            unit: "mm",
        }
    };
    expr::evaluate(
        text,
        field,
        &Object {
            current: None,
            attributes: &[],
        },
    )
    .map_err(|e| e.to_string())
}

#[test]
fn every_spec_number_is_measured_in_its_frame_and_sourced() {
    let templates: BTreeMap<String, BoardTemplate> = builtins()
        .into_iter()
        .map(|(_, _, t)| (t.name.clone(), t))
        .collect();
    let rows = dimension_rows();
    let mut failures = vec![];
    let mut named: BTreeMap<String, String> = BTreeMap::new();
    for row in &rows {
        let at = format!("line {} ({} {})", row.line, row.template, row.quantity);
        if !sources_ok(&row.sources) {
            failures.push(format!(
                "{at}: sources need two independent sources separated by `;`, or one marked \
                 `primary:`, `decided #<ticket>:` or `unverified:`"
            ));
        }
        let Some(template) = templates.get(&row.template) else {
            failures.push(format!("{at}: no built-in is named {:?}", row.template));
            continue;
        };
        let frame = match frame_at(template, &row.combination) {
            Ok(frame) => frame,
            Err(e) => {
                failures.push(format!("{at}: {e}"));
                continue;
            }
        };
        if row.combination == "default" {
            named
                .entry(row.template.clone())
                .or_default()
                .push_str(&format!("{}\n", row.quantity));
        }
        let measured = match measure(template, &frame, &row.quantity) {
            Ok(m) => m,
            Err(e) => {
                failures.push(format!("{at}: {e}"));
                continue;
            }
        };
        let angle = matches!(measured, Measured::Angle(_));
        let (Ok(expected), Ok(tolerance)) = (
            parse_value(&row.expected, angle),
            parse_value(&row.tolerance, angle),
        ) else {
            failures.push(format!(
                "{at}: the expected value or tolerance is not a number"
            ));
            continue;
        };
        let (Measured::Length(actual) | Measured::Angle(actual)) = measured;
        if (actual - expected).abs() > tolerance {
            let unit = if angle { 1_000 } else { 1_000_000 };
            failures.push(format!(
                "{at}: measured {}, expected {} ± {}",
                reda_engine::model::geom::exact_decimal(actual, unit),
                row.expected,
                row.tolerance
            ));
        }
    }

    // Coverage at default values: the outline's size and every hole, connector and cutout.
    for (name, template) in &templates {
        let frame = frame_at(template, "default").unwrap();
        let rows = named.get(name).cloned().unwrap_or_default();
        let mut need: Vec<String> = vec!["outline/width".into(), "outline/height".into()];
        need.retain(|q| !rows.lines().any(|l| l.trim() == q));
        for missing in need {
            failures.push(format!("{name}: no default row measures {missing}"));
        }
        let objects = frame
            .holes
            .iter()
            .map(|h| format!("hole:{}", h.uuid))
            .chain(
                frame
                    .connectors
                    .iter()
                    .map(|c| format!("connector:{}", c.uuid)),
            )
            .chain(frame.cutouts.iter().map(|c| format!("cutout:{}", c.uuid)));
        for object in objects {
            if !rows.contains(&format!("{object}/")) {
                failures.push(format!("{name}: no default row measures {object}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "board-template-dimensions.tsv:\n{}",
        failures.join("\n")
    );
}

#[test]
fn the_dimension_measures_catch_a_wrong_number() {
    // The harness itself: a frame measured against a wrong value fails.
    let template = builtins()
        .into_iter()
        .find(|(_, _, t)| t.name == "Custom rectangle")
        .map(|(_, _, t)| t)
        .unwrap();
    let frame = frame_at(&template, "width=65,corner_radius=5").unwrap();
    assert_eq!(
        measure(&template, &frame, "outline/width"),
        Ok(Measured::Length(65_000_000))
    );
    assert_eq!(
        measure(&template, &frame, "outline/height"),
        Ok(Measured::Length(80_000_000))
    );
    assert_eq!(
        measure(&template, &frame, "outline/corner_radius"),
        Ok(Measured::Length(5_000_000))
    );
    assert_eq!(
        measure(&template, &frame, "outline/vertex:1/x - outline/vertex:0/x"),
        Ok(Measured::Length(55_000_000))
    );
    // The arc leaving vertex 1 runs from (60, 0) to (65, 5) round (60, 5).
    assert_eq!(
        measure(&template, &frame, "outline/arc:1/radius"),
        Ok(Measured::Length(5_000_000))
    );
    assert_eq!(
        measure(&template, &frame, "outline/arc:1/x"),
        Ok(Measured::Length(60_000_000))
    );
    assert_eq!(
        measure(&template, &frame, "outline/arc:1/y"),
        Ok(Measured::Length(5_000_000))
    );
    // Its middle, 45° round: 5 × (sin 45°, 1 − cos 45°) from (60, 0).
    assert_eq!(
        measure(&template, &frame, "outline/arc:1/mid/x"),
        Ok(Measured::Length(63_535_534))
    );
    assert_eq!(
        measure(&template, &frame, "outline/arc:1/mid/y"),
        Ok(Measured::Length(1_464_466))
    );
    assert!(measure(&template, &frame, "outline/arc:0/radius").is_err());
    assert!(measure(&template, &frame, "board/bevel_angle").is_err());
    assert!(measure(&template, &frame, "hole:00000000-0000-4000-8000-000000000001/x").is_err());
    assert_eq!(
        frame.values["width"],
        ResolvedValue::Length(reda_engine::model::geom::Nm(65_000_000))
    );
    assert!(sources_ok("primary: the spec"));
    assert!(sources_ok("decided #397: research"));
    assert!(sources_ok("A datasheet; B drawing"));
    assert!(!sources_ok("A datasheet"));
    assert!(!sources_ok("decided: research"));
}

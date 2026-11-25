//! `write`: the trial assembly, all of it through step-io's `StepBuilder`.

use std::path::Path;

use step_io::StepBuilder;
use step_io::build::{Frame, StyleTarget};
use step_io::scene::Rgb;

use crate::json::{Obj, shorten};
use crate::read::summarize;
use crate::transcribe::{IDENTITY, Stats, transcribe};

fn colours(list: &[Rgb]) -> String {
    let mut v: Vec<String> = list
        .iter()
        .map(|c| format!("({:.3},{:.3},{:.3})", c.red, c.green, c.blue))
        .collect();
    v.sort();
    format!("[{}]", v.join(" "))
}

/// The component alone, read and written back, to count entities after.
fn alone(scene: &step_io::scene::Scene<'_>, name: &str) -> (usize, Stats) {
    let mut b = StepBuilder::new();
    let (_, stats) = transcribe(&mut b, scene, name);
    let mut out = Vec::new();
    let _ = step_io::write(&b.finish(), &mut out);
    let entities = step_io::read(out.as_slice()).map_or(0, |(_, r)| r.entities_read);
    (entities, stats)
}

fn result(
    ok: bool,
    error: Option<&str>,
    bytes: Option<usize>,
    schema: Option<&str>,
    notes: &str,
) -> String {
    let mut o = Obj::new();
    o.bool("ok", ok)
        .opt_str("error", error.map(shorten).as_deref())
        .opt_int("bytes", bytes)
        .opt_str("schema", schema)
        .str("notes", notes);
    o.finish()
}

pub fn run(out_path: &str, component_path: &str, patch_names: bool) -> String {
    let bytes = match std::fs::read(component_path) {
        Ok(b) => b,
        Err(e) => {
            return result(
                false,
                Some(&format!("cannot open {component_path}: {e}")),
                None,
                None,
                "",
            );
        }
    };
    // A component that does not parse still leaves the board to write.
    let component = step_io::read(bytes.as_slice());
    let stem = Path::new(component_path)
        .file_stem()
        .map_or("component".to_owned(), |s| s.to_string_lossy().into_owned());

    let mut b = StepBuilder::new();
    let header = b.header_mut();
    header.file_name = Path::new(out_path)
        .file_name()
        .map_or(String::new(), |s| s.to_string_lossy().into_owned());
    header.originating_system = "REDA step-kernel-trial (step-io 0.3.1)".to_owned();
    header.descriptions = vec!["trial-board".to_owned()];

    let assembly = b.part("trial-board");
    let pcb = b.part("PCB");
    let mut notes = Vec::new();
    let mut error: Option<String> = None;

    match crate::board::build(&mut b, pcb) {
        Ok((solid, faces)) => {
            let green = Rgb {
                red: 0.0,
                green: 0.5,
                blue: 0.2,
            };
            if let Err(e) = b.style(StyleTarget::Solid(solid), green, None) {
                error.get_or_insert(format!("board colour: {e}"));
            }
            notes.push(format!(
                "PCB: one MANIFOLD_SOLID_BREP of {faces} faces built by hand (planes and cylinders, \
                 holes as inner bounds, round holes as two half cylinders), green on the solid"
            ));
        }
        Err(e) => {
            error.get_or_insert(format!("board: {e}"));
        }
    }

    // Each `place` numbers its occurrence; remember which two are U1 and U2.
    let mut instance_ids = None;
    let mut before = None;
    match &component {
        Ok((model, report)) => {
            let scene = model.scene();
            let summary = summarize(model);
            let (alone_entities, _) = alone(&scene, &stem);
            let (part, stats) = transcribe(&mut b, &scene, &stem);
            let z = [0.0, 0.0, 1.0];
            // 90 degrees about +Z: the component's X lands on the board's Y.
            let u1 = Frame {
                origin: [20.0, 10.0, 1.6],
                axis: z,
                ref_dir: [0.0, 1.0, 0.0],
            };
            let u2 = Frame {
                origin: [35.0, 20.0, 1.6],
                axis: z,
                ref_dir: [1.0, 0.0, 0.0],
            };
            for (frame, name) in [(u1, "U1"), (u2, "U2")] {
                if let Err(e) = b.place(assembly, part, frame) {
                    error.get_or_insert(format!("placing {name}: {e}"));
                }
            }
            instance_ids = Some((stats.placements + 1, stats.placements + 2));
            notes.push(format!(
                "component: read by step-io as {}, {} entities read / {} kept / {} dropped, {} solids, \
                 {} faces, {} products, colours {}; walked through the scene handles and rebuilt with \
                 StepBuilder: {}; one definition placed twice; written alone it is {} entities",
                model.header().schema,
                report.entities_read,
                report.entities_kept,
                report.dropped.len(),
                summary.solids,
                summary.faces_total,
                summary.products,
                colours(&summary.colours),
                stats.describe(),
                alone_entities,
            ));
            before = Some(summary);
        }
        Err(e) => {
            error.get_or_insert(format!("component did not parse: {e}"));
            notes.push(
                "component missing: the file did not parse, only the board is written".to_owned(),
            );
        }
    }
    if let Err(e) = b.place(assembly, pcb, IDENTITY) {
        error.get_or_insert(format!("placing PCB: {e}"));
    }

    let mut text = Vec::new();
    if let Err(e) = step_io::write(&b.finish(), &mut text) {
        return result(
            false,
            Some(&format!("write: {e}")),
            None,
            None,
            &notes.join(". "),
        );
    }

    // `StepBuilder::place` writes the occurrence with its running number as
    // id and an empty name, and a finished model cannot be edited. The two
    // names are therefore set in the Part 21 text, outside step-io.
    if let Some((n1, n2)) = instance_ids {
        if patch_names {
            let mut s = String::from_utf8(text).expect("step-io writes ASCII");
            let mut patched = 0;
            for (n, name) in [(n1, "U1"), (n2, "U2")] {
                let from = format!("NEXT_ASSEMBLY_USAGE_OCCURRENCE('{n}','',");
                if s.matches(&from).count() == 1 {
                    s = s.replace(
                        &from,
                        &format!("NEXT_ASSEMBLY_USAGE_OCCURRENCE('{name}','{name}',"),
                    );
                    patched += 1;
                }
            }
            notes.push(format!(
                "instance names: StepBuilder::place gives no way to name an occurrence, so U1 and U2 were \
                 set by editing the written text ({patched} of 2 NEXT_ASSEMBLY_USAGE_OCCURRENCE records)"
            ));
            text = s.into_bytes();
        } else {
            notes.push(format!(
                "instance names: not set, StepBuilder::place gives no way to name an occurrence \
                 (they are occurrences '{n1}' and '{n2}' with empty names)"
            ));
        }
    }

    if let Err(e) = std::fs::write(out_path, &text) {
        return result(
            false,
            Some(&format!("cannot write {out_path}: {e}")),
            None,
            None,
            &notes.join(". "),
        );
    }

    // Read the result back with step-io itself.
    let mut schema = None;
    match step_io::read(text.as_slice()) {
        Ok((model, report)) => {
            let after = summarize(&model);
            schema = Some(format!(
                "{} [{}]",
                model.header().schema,
                model.header().schema.raw.join(",")
            ));
            let mut n = format!(
                "output read back by step-io: {} entities, {} dropped, {} solids, {} faces, {} products, \
                 {} occurrences, colours {}",
                report.entities_read,
                report.dropped.len(),
                after.solids,
                after.faces_total,
                after.products,
                after.occurrences,
                colours(&after.colours),
            );
            if let Some(before) = &before {
                n.push_str(&format!(
                    " (expected {} solids and {} faces: the component once plus the board)",
                    before.solids + 1,
                    before.faces_total + 22
                ));
            }
            notes.push(n);
        }
        Err(e) => {
            error.get_or_insert(format!("output does not read back: {e}"));
        }
    }
    notes.push("not checked here: geometric validity (step-io evaluates no geometry)".to_owned());
    result(
        error.is_none(),
        error.as_deref(),
        Some(text.len()),
        schema.as_deref(),
        &notes.join(". "),
    )
}

/// step-io's own write of a model it read, untouched.
pub fn rewrite(out_path: &str, in_path: &str) -> String {
    let bytes = match std::fs::read(in_path) {
        Ok(b) => b,
        Err(e) => {
            return result(
                false,
                Some(&format!("cannot open {in_path}: {e}")),
                None,
                None,
                "",
            );
        }
    };
    let (model, report) = match step_io::read(bytes.as_slice()) {
        Ok(v) => v,
        Err(e) => return result(false, Some(&e.to_string()), None, None, ""),
    };
    let before = summarize(&model);
    let mut text = Vec::new();
    if let Err(e) = step_io::write(&model, &mut text) {
        return result(false, Some(&format!("write: {e}")), None, None, "");
    }
    if let Err(e) = std::fs::write(out_path, &text) {
        return result(
            false,
            Some(&format!("cannot write {out_path}: {e}")),
            None,
            None,
            "",
        );
    }
    match step_io::read(text.as_slice()) {
        Ok((again, report2)) => {
            let after = summarize(&again);
            let schema = format!(
                "{} [{}]",
                again.header().schema,
                again.header().schema.raw.join(",")
            );
            let notes = format!(
                "step_io::write of the model as read, no builder. in: {} entities read / {} kept, {} solids, \
                 {} faces, {} products, colours {}. out: {} entities read / {} kept, {} solids, {} faces, \
                 {} products, colours {}",
                report.entities_read,
                report.entities_kept,
                before.solids,
                before.faces_total,
                before.products,
                colours(&before.colours),
                report2.entities_read,
                report2.entities_kept,
                after.solids,
                after.faces_total,
                after.products,
                colours(&after.colours),
            );
            result(true, None, Some(text.len()), Some(&schema), &notes)
        }
        Err(e) => result(
            false,
            Some(&format!("output does not read back: {e}")),
            Some(text.len()),
            None,
            "",
        ),
    }
}

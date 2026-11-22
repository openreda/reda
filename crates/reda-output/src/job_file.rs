//! The Gerber job file: `<project>-job.gbrjob`, a minimal JSON file in the publisher's
//! Job spec form. It holds the generation software, the project ID, the board size, layer
//! count and thickness, the finish when the Job spec names it, whether the board has Edge
//! Connector or Castellated Pads, the edge connector's bevel (its angle and depth in
//! `Notes`) and hard gold area, and the path, function and
//! polarity of each Gerber and drill file in the zip, and the physical material stack.
//!
//! The Output Manifest holds the run date, so this file stays reproducible.

use reda_model::{PcbDocument, StackupEntry};
use serde::Serialize;

use crate::fab_notes::{bevel_line, board_size, hard_gold_area};
use crate::plot::{OutputFile, Plot};

/// The job file's name for the Project `name`.
pub fn gerber_job_name(name: &str) -> String {
    format!("{name}-job.gbrjob")
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Job {
    header: Header,
    general_specs: GeneralSpecs,
    files_attributes: Vec<FileEntry>,
    material_stackup: Vec<Material>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Header {
    generation_software: Software,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct Software {
    vendor: &'static str,
    application: &'static str,
    version: String,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct GeneralSpecs {
    project_id: ProjectId,
    #[serde(skip_serializing_if = "Option::is_none")]
    size: Option<Size>,
    layer_number: usize,
    board_thickness: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    finish: Option<&'static str>,
    /// `true` when any edge is a Plated Edge; left out otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    edge_plating: Option<bool>,
    /// `true` when any Pad is an Edge Connector Pad; left out otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    edge_connector: Option<bool>,
    /// `true` when the board's `edge_connector` has a bevel; left out otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    edge_connector_bevelled: Option<bool>,
    /// The summed Edge Connector Pad copper area in mm², when the board's
    /// `edge_connector` asks for hard gold; left out otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    hard_gold_area: Option<f64>,
    /// `true` when any Pad is a Castellated Pad; left out otherwise.
    #[serde(skip_serializing_if = "Option::is_none")]
    castellated: Option<bool>,
    /// The spec's free field for informal information (Job File §2.4): the
    /// bevel's angle and depth, which no other field holds. Left out when unset.
    #[serde(skip_serializing_if = "Option::is_none")]
    notes: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct ProjectId {
    name: String,
    #[serde(rename = "GUID")]
    guid: String,
    revision: String,
}

#[derive(Serialize)]
struct Size {
    #[serde(rename = "X")]
    x: f64,
    #[serde(rename = "Y")]
    y: f64,
}

#[derive(Serialize)]
#[serde(rename_all = "PascalCase")]
struct FileEntry {
    path: String,
    file_function: String,
    file_polarity: &'static str,
}

#[derive(Default, Serialize)]
#[serde(rename_all = "PascalCase")]
struct Material {
    #[serde(rename = "Type")]
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    thickness: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    material: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    dielectric_constant: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    loss_tangent: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    color: Option<String>,
}

/// Mask and legend exist in the material stack on the sides actually plotted.
/// Dielectric values come from the authored entry, including its optional thickness.
fn material_stackup(pcb: &PcbDocument, files: &[OutputFile]) -> Vec<Material> {
    let stack = pcb.stackup();
    let outer = |function: &str, kind, color: &Option<String>| {
        files
            .iter()
            .any(|f| f.function == function)
            .then(|| Material {
                kind,
                color: color.clone(),
                ..Default::default()
            })
    };
    let mut rows = Vec::new();
    rows.extend(outer("silkscreen_top", "Legend", &stack.silk_color));
    rows.extend(outer("solder_mask_top", "SolderMask", &stack.mask_color));
    rows.extend(stack.entries.iter().map(|entry| {
        match entry {
            StackupEntry::Copper(c) => Material {
                kind: "Copper",
                name: pcb
                    .layers()
                    .layer(c.layer)
                    .and_then(|layer| layer.name.clone())
                    .or_else(|| pcb.copper_number(c.layer).map(|n| format!("L{n}"))),
                thickness: Some(mm(c.thickness)),
                ..Default::default()
            },
            StackupEntry::Dielectric(d) => Material {
                kind: "Dielectric",
                name: d.name.clone(),
                thickness: d.thickness.map(mm),
                material: d.material.clone(),
                dielectric_constant: d.dk.map(|v| f64::from(v) / 1000.0),
                loss_tangent: d.df.map(|v| f64::from(v) / 1e6),
                ..Default::default()
            },
        }
    }));
    rows.extend(outer("solder_mask_bottom", "SolderMask", &stack.mask_color));
    rows.extend(outer("silkscreen_bottom", "Legend", &stack.silk_color));
    rows
}

/// The finishes the Job spec names, in its spelling.
const FINISHES: [&str; 12] = [
    "None",
    "HAL SnPb",
    "HAL lead-free",
    "ENIG",
    "ENEPIG",
    "Hard gold",
    "Immersion tin",
    "Immersion nickel",
    "Immersion silver",
    "Immersion gold",
    "OSP",
    "HT_OSP",
];

/// The Job spec's name for the finish typed, matched ignoring case, spaces and
/// punctuation, with the HASL spellings fabs use for the two HAL finishes (a bare `HASL` says
/// neither, so it is left out); `None` for a
/// finish the Job spec doesn't name.
fn job_finish(typed: &str) -> Option<&'static str> {
    let key = |s: &str| -> String {
        s.chars()
            .filter(char::is_ascii_alphanumeric)
            .map(|c| c.to_ascii_lowercase())
            .collect()
    };
    let typed = key(typed);
    match typed.as_str() {
        "haslleaded" | "haslwithlead" | "haslsnpb" => return Some("HAL SnPb"),
        "haslleadfree" | "lfhasl" => return Some("HAL lead-free"),
        _ => {}
    }
    FINISHES.into_iter().find(|f| key(f) == typed)
}

/// Millimetres as a JSON number: `30`, `1.6`.
fn mm(nm: reda_model::geom::Nm) -> f64 {
    nm.0 as f64 / 1e6
}

/// The job file of `plot`, listing the Gerber and drill files among `files` (the files
/// in the zip, in zip order).
pub fn gerber_job_file(plot: &Plot<'_>, files: &[OutputFile]) -> OutputFile {
    let props = plot.project.props();
    let pcb = plot.instances.first().map(|i| i.pcb);
    let revision = match props.properties.revision.as_str() {
        "" => "-",
        r => r,
    };
    let edge = pcb.and_then(|p| p.setup().edge_connector);
    let bevel = edge.and_then(|e| e.bevel);
    let job = Job {
        header: Header {
            generation_software: Software {
                vendor: "openreda",
                application: "REDA",
                version: plot.version.to_owned(),
            },
        },
        general_specs: GeneralSpecs {
            project_id: ProjectId {
                name: props.name.clone(),
                guid: props.uuid.to_string(),
                revision: revision.to_owned(),
            },
            size: pcb
                .and_then(board_size)
                .map(|(x, y)| Size { x: mm(x), y: mm(y) }),
            layer_number: pcb.map_or(0, |p| p.copper_layers().count()),
            board_thickness: pcb.map_or(0.0, |p| mm(p.stackup().thickness)),
            finish: pcb
                .and_then(|p| p.stackup().finish.as_deref())
                .and_then(job_finish),
            edge_plating: plot
                .instances
                .iter()
                .any(|i| !i.pcb.plated_edges().is_empty())
                .then_some(true),
            edge_connector: pcb
                .is_some_and(PcbDocument::has_edge_connector_pads)
                .then_some(true),
            edge_connector_bevelled: bevel.map(|_| true),
            hard_gold_area: pcb
                .filter(|_| edge.is_some_and(|e| e.hard_gold))
                .map(|p| (hard_gold_area(p) * 1e4).round() / 1e4),
            castellated: pcb
                .is_some_and(PcbDocument::has_castellated_pads)
                .then_some(true),
            notes: bevel.map(bevel_line),
        },
        material_stackup: pcb
            .map(|pcb| material_stackup(pcb, files))
            .unwrap_or_default(),
        files_attributes: files
            .iter()
            .filter_map(|f| {
                let attributes = f.attributes.as_ref()?;
                Some(FileEntry {
                    path: f.name.clone(),
                    file_function: attributes.function.clone(),
                    file_polarity: attributes.polarity,
                })
            })
            .collect(),
    };
    let mut bytes = serde_json::to_vec_pretty(&job).expect("the job file serialises");
    bytes.push(b'\n');
    OutputFile {
        name: gerber_job_name(&props.name),
        function: "gerber_job",
        bytes,
        attributes: None,
    }
}

#[cfg(test)]
mod tests {
    use super::job_finish;

    #[test]
    fn finishes_the_job_spec_names_are_written_in_its_spelling_and_others_left_out() {
        for (typed, expected) in [
            ("ENIG", Some("ENIG")),
            ("enig", Some("ENIG")),
            ("Immersion Gold", Some("Immersion gold")),
            ("OSP", Some("OSP")),
            ("HASL (leaded)", Some("HAL SnPb")),
            ("HASL with lead", Some("HAL SnPb")),
            ("HASL (lead-free)", Some("HAL lead-free")),
            ("HASL lead free", Some("HAL lead-free")),
            ("HASL", None),
            ("Carbon ink", None),
            ("", None),
        ] {
            assert_eq!(job_finish(typed), expected, "{typed}");
        }
    }
}

//! The Gerber job file: `<project>-job.gbrjob`, a minimal JSON file in the publisher's
//! Job spec form. It holds the generation software, the project ID, the board size, layer
//! count and thickness, the finish when the Job spec names it, and the path, function and
//! polarity of each Gerber and drill file in the zip.
//!
//! Left out on purpose: `MaterialStackup`, which must be complete when present while v0.1
//! dielectrics have no material, and `CreationDate`, so the same design and REDA version
//! give the same bytes (the Output Manifest holds the run's date).

use serde::Serialize;

use crate::fab_notes::board_size;
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
        },
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

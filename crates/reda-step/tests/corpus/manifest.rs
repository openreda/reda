//! `step-corpus.tsv`: one row per corpus model, tab-separated, under a header row.
//!
//! - `name`: the model's path in the corpus folder, `<source group>/<file>`.
//! - `url`: where the file is, at the pinned commit when it comes from a repository.
//! - `commit`: the pinned commit, or `-` for a download that is not in a repository.
//! - `sha256`: the file's SHA-256, which `fetch.sh` checks.
//! - `schema`: the `FILE_SCHEMA` name the file gives.
//! - `fetch`: `yes` for the 40 models `fetch.sh` downloads (KiCad's library and Würth's
//!   KiCad library); the other 18 stay on the maintainer's disk.
//! - `colours`: whether the file colours its faces or solids.
//! - `volume`: FreeCAD 1.1.4's volume of the file's solids, mm³, or `-` when it has none.
//! - `bbox_min`, `bbox_max`: FreeCAD's tight bounding box (`optimalBoundingBox`; its plain
//!   `BoundBox` is loose around curved faces), mm, as `x y z`.
//!
//! The models, their volumes and the schema names are the trial's (#442); each file was
//! matched to its repository and commit by its git blob hash.

use std::path::{Path, PathBuf};

/// One model of the corpus.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub name: String,
    pub url: String,
    pub commit: Option<String>,
    pub sha256: String,
    pub schema: String,
    pub fetch: bool,
    pub colours: bool,
    pub volume: Option<f64>,
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Manifest {
    pub entries: Vec<Entry>,
}

const COLUMNS: [&str; 10] = [
    "name", "url", "commit", "sha256", "schema", "fetch", "colours", "volume", "bbox_min",
    "bbox_max",
];

/// The folder that holds the manifest and `fetch.sh`.
pub fn folder() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus")
}

impl Manifest {
    /// The committed `step-corpus.tsv`.
    pub fn committed() -> Manifest {
        let path = folder().join("step-corpus.tsv");
        let text = std::fs::read_to_string(&path).unwrap();
        Manifest::parse(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    pub fn parse(text: &str) -> Result<Manifest, String> {
        let mut lines = text.lines().enumerate();
        let (_, head) = lines.next().ok_or("the manifest is empty")?;
        if head.split('\t').ne(COLUMNS) {
            return Err(format!("the header is not {}", COLUMNS.join(" ")));
        }
        let mut entries = Vec::new();
        for (i, line) in lines {
            let row = |e: String| format!("line {}: {e}", i + 1);
            let cells: Vec<&str> = line.split('\t').collect();
            let [
                name,
                url,
                commit,
                sha256,
                schema,
                fetch,
                colours,
                volume,
                bmin,
                bmax,
            ] = cells[..]
            else {
                return Err(row(format!("{} cells, not {}", cells.len(), COLUMNS.len())));
            };
            entries.push(Entry {
                name: name.to_string(),
                url: url.to_string(),
                commit: (commit != "-").then(|| commit.to_string()),
                sha256: sha256.to_string(),
                schema: schema.to_string(),
                fetch: yes_no(fetch).map_err(row)?,
                colours: yes_no(colours).map_err(row)?,
                volume: match volume {
                    "-" => None,
                    v => Some(number(v).map_err(row)?),
                },
                bbox_min: point(bmin).map_err(row)?,
                bbox_max: point(bmax).map_err(row)?,
            });
        }
        Ok(Manifest { entries })
    }

    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }
}

fn yes_no(s: &str) -> Result<bool, String> {
    match s {
        "yes" => Ok(true),
        "no" => Ok(false),
        _ => Err(format!("{s:?} is not yes or no")),
    }
}

fn number(s: &str) -> Result<f64, String> {
    s.parse().map_err(|_| format!("{s:?} is not a number"))
}

fn point(s: &str) -> Result<[f64; 3], String> {
    let v: Vec<f64> = s.split(' ').map(number).collect::<Result<_, _>>()?;
    v.try_into().map_err(|_| format!("{s:?} is not x y z"))
}

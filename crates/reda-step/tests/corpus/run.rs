//! Reading and meshing one model, each in its own process as in the trial, so that a crash or
//! a hang is reported for that model and the run goes on with the next.
//!
//! [`model`] starts this test binary again running only [`child`] (the `corpus_child` test),
//! which reads the file named by [`CHILD_FILE`], meshes it with the default tolerances and
//! prints one [`RESULT`] line of what it measured.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::io::Read as _;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use reda_step::{MeshOptions, load};

/// The trial's limit per model.
pub const TIMEOUT: Duration = Duration::from_secs(300);

/// The variable that names the child's model.
pub const CHILD_FILE: &str = "REDA_STEP_CORPUS_CHILD";

/// The child's line of figures.
const RESULT: &str = "CORPUS-RESULT";
/// The child's line for a file the reader refused.
const REFUSED: &str = "CORPUS-REFUSED";

/// What reading and meshing one model came to.
#[derive(Clone, Debug, PartialEq)]
pub enum Outcome {
    /// The file is not in the corpus folder.
    Missing,
    /// The reader refused the file, with its message.
    Refused(String),
    /// The process ended without a result: a panic, an abort or a signal, with the end of
    /// what it printed.
    Crash(String),
    /// Still running at the limit, and stopped.
    Timeout(Duration),
    Measured(Measured),
}

/// The figures of one model read and meshed.
#[derive(Clone, Debug, PartialEq)]
pub struct Measured {
    /// Reading, converting and meshing, in seconds.
    pub secs: f64,
    /// Faces in the model's B-reps, and those that were meshed.
    pub faces: usize,
    pub faces_meshed: usize,
    pub triangles: usize,
    /// The placed solids' volume, mm³.
    pub volume: f64,
    /// Whether every placed solid's mesh closes up.
    pub closed: bool,
    /// Distinct face and solid colours read.
    pub colours: usize,
    /// The protocol the reader took the file for (`Ap214`, ...).
    pub protocol: String,
    /// The mesh's box in the model's coordinates, mm.
    pub bbox: Option<([f64; 3], [f64; 3])>,
    pub warnings: usize,
    pub first_warning: String,
}

/// Reads and meshes one model in a process of its own, stopped at `timeout`.
pub fn model(path: &Path, timeout: Duration) -> Outcome {
    if !path.is_file() {
        return Outcome::Missing;
    }
    in_child("corpus_child", (CHILD_FILE, path.as_os_str()), timeout)
}

/// Runs the ignored test `test` of this binary alone, with `env` set, stopped at `timeout`,
/// and reads its outcome from what it printed.
pub fn in_child(test: &str, env: (&str, &OsStr), timeout: Duration) -> Outcome {
    let exe = std::env::current_exe().unwrap();
    let mut child = Command::new(exe)
        .args([test, "--exact", "--ignored", "--nocapture"])
        .args(["--test-threads", "1"])
        .env(env.0, env.1)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Drain both pipes while waiting, so a chatty child never blocks on a full one.
    let pipe = |mut r: Box<dyn std::io::Read + Send>| {
        std::thread::spawn(move || {
            let mut s = String::new();
            r.read_to_string(&mut s).ok();
            s
        })
    };
    let out = pipe(Box::new(child.stdout.take().unwrap()));
    let err = pipe(Box::new(child.stderr.take().unwrap()));
    let start = Instant::now();
    loop {
        if child.try_wait().unwrap().is_some() {
            break;
        }
        let elapsed = start.elapsed();
        if elapsed >= timeout {
            child.kill().ok();
            child.wait().ok();
            return Outcome::Timeout(timeout);
        }
        std::thread::sleep((timeout - elapsed).min(Duration::from_millis(20)));
    }
    let (out, err) = (out.join().unwrap(), err.join().unwrap());
    // libtest may print its own `test corpus_child ... ` ahead of the line.
    for line in out.lines() {
        if let Some((_, fields)) = line.split_once(RESULT) {
            return Outcome::Measured(parse(fields));
        }
        if let Some((_, message)) = line.split_once(REFUSED) {
            return Outcome::Refused(message.trim().to_string());
        }
    }
    let said = format!("{out}\n{err}");
    let panic = said.lines().find(|l| l.contains("panicked at")).map(|l| {
        let next = said.lines().skip_while(|x| *x != l).nth(1).unwrap_or("");
        format!("{} {next}", l.trim())
    });
    Outcome::Crash(panic.unwrap_or_else(|| {
        let tail: Vec<&str> = said.lines().filter(|l| !l.trim().is_empty()).collect();
        tail[tail.len().saturating_sub(2)..].join(" ")
    }))
}

/// The child's side: measures the model named by [`CHILD_FILE`], if any.
pub fn child() {
    let Some(path) = std::env::var_os(CHILD_FILE) else {
        return;
    };
    let bytes = std::fs::read(&path).unwrap();
    let start = Instant::now();
    let loaded = match load(&bytes, &MeshOptions::default()) {
        Ok(l) => l,
        Err(e) => {
            println!("{REFUSED} {}", one_line(&e.to_string()));
            return;
        }
    };
    let secs = start.elapsed().as_secs_f64();
    let meshed = &loaded.meshed;
    // The faces of every shell (a face no shell holds belongs to no solid).
    let faces: HashSet<_> = loaded
        .model
        .iter::<reda_step::records::Shell>()
        .flat_map(|(_, s)| s.cfs_faces.iter().copied())
        .collect();
    let faces = faces.len();
    let mut meshed_faces = HashSet::new();
    let mut colours = HashSet::new();
    let mut key = |c: &reda_step::Colour| [c.red, c.green, c.blue, c.alpha].map(f32::to_bits);
    for part in &meshed.parts {
        for s in &part.solids {
            colours.extend(s.colour.as_ref().map(&mut key));
            for f in &s.mesh.faces {
                meshed_faces.insert(f.face);
                colours.extend(f.colour.as_ref().map(&mut key));
            }
        }
    }
    let instances = meshed.instances();
    let solids = || instances.iter().flat_map(|i| &i.part.solids);
    let bbox = meshed.bounding_box().map(|b| (b.min, b.max));
    let fields = [
        ("secs", secs.to_string()),
        ("faces", faces.to_string()),
        ("faces_meshed", meshed_faces.len().to_string()),
        (
            "triangles",
            solids()
                .map(|s| s.mesh.triangles.len())
                .sum::<usize>()
                .to_string(),
        ),
        ("volume", meshed.volume().to_string()),
        ("closed", solids().all(|s| s.mesh.is_closed()).to_string()),
        ("colours", colours.len().to_string()),
        ("protocol", format!("{:?}", loaded.protocol)),
        (
            "bbox",
            bbox.map_or("-".to_string(), |(a, b)| {
                a.iter()
                    .chain(&b)
                    .map(f64::to_string)
                    .collect::<Vec<_>>()
                    .join(" ")
            }),
        ),
        ("warnings", loaded.warnings.len().to_string()),
        (
            "first_warning",
            loaded
                .warnings
                .first()
                .map_or(String::new(), |w| one_line(w)),
        ),
    ];
    let line: Vec<String> = fields.iter().map(|(k, v)| format!("{k}={v}")).collect();
    println!("{RESULT}\t{}", line.join("\t"));
}

fn one_line(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn parse(fields: &str) -> Measured {
    let get = |k: &str| {
        fields
            .split('\t')
            .find_map(|f| f.strip_prefix(k).and_then(|f| f.strip_prefix('=')))
            .unwrap_or_else(|| panic!("no {k} in {fields}"))
    };
    let num = |k: &str| get(k).parse::<f64>().unwrap();
    let count = |k: &str| get(k).parse::<usize>().unwrap();
    let bbox = match get("bbox") {
        "-" => None,
        b => {
            let v: Vec<f64> = b.split(' ').map(|x| x.parse().unwrap()).collect();
            Some(([v[0], v[1], v[2]], [v[3], v[4], v[5]]))
        }
    };
    Measured {
        secs: num("secs"),
        faces: count("faces"),
        faces_meshed: count("faces_meshed"),
        triangles: count("triangles"),
        volume: num("volume"),
        closed: get("closed") == "true",
        colours: count("colours"),
        protocol: get("protocol").to_string(),
        bbox,
        warnings: count("warnings"),
        first_warning: get("first_warning").to_string(),
    }
}

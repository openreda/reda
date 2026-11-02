//! `pcb.zone.fill`: fills every Zone, keeps the fills beside the PCB Document and writes
//! them to the Project's `.reda/` cache. Also reading that cache when a Project opens.
//!
//! The cache is one file per PCB Document, `.reda/<document uuid>.zonefill`, in the byte
//! format of [`reda_pcb::zone::encode`]. It carries the hash of the board the fills came
//! from, so a cache older than the board reads as stale fills, which still draw (dimmed)
//! until the next fill. A cache that does not decode is ignored.

use std::path::{Path, PathBuf};

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, NO_PROJECT, NoParams,
    Registry, Unavailable,
};
use reda_model::{Project, Uuid};
use schemars::JsonSchema;
use serde::Serialize;

use crate::fs::write_atomic;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ZoneFill);
}

/// The cache file of PCB Document `document` in the Project folder `folder`.
pub fn cache_path(folder: &Path, document: Uuid) -> PathBuf {
    folder.join(".reda").join(format!("{document}.zonefill"))
}

/// Reads every PCB Document's fill cache from `folder` into `project`, skipping caches
/// that are missing or do not decode.
pub fn load_caches(project: &mut Project, folder: &Path) {
    let pcbs: Vec<Uuid> = project.pcb().map(|p| p.uuid).into_iter().collect();
    for doc in pcbs {
        let fills = std::fs::read(cache_path(folder, doc))
            .ok()
            .and_then(|bytes| reda_pcb::zone::decode(&bytes));
        project.set_zone_fills(doc, fills);
    }
}

pub struct ZoneFill;

/// One filled Zone.
#[derive(Debug, Serialize, JsonSchema)]
pub struct FilledZone {
    pub zone: Uuid,
    /// The separate pieces of copper its fill came to.
    pub islands: usize,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ZoneFillResult {
    /// Every Zone, in fill order (priority, highest first).
    pub zones: Vec<FilledZone>,
    /// Whether the fills were written to the `.reda/` cache (not when the Project has no
    /// folder or the write failed).
    pub cached: bool,
}

impl Command for ZoneFill {
    const META: Meta = Meta {
        id: "pcb.zone.fill",
        title: "Fill Zones",
        description: "Fills every Zone of the PCB Document and stores the fills in the \
                      Project's `.reda/` cache (never in the Document, so filling is not an \
                      edit). Long-running: reports progress per Zone and can be cancelled, \
                      which keeps the previous fills.",
        category: "pcb",
        icon: "zone_fill",
        kind: Kind::File,
    };
    type Params = NoParams;
    type Output = ZoneFillResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        let reason = match cx.project {
            None => NO_PROJECT,
            Some(p) if p.pcb().is_none() => "the Project has no PCB Document",
            Some(_) => return Ok(()),
        };
        Err(Unavailable {
            reason: reason.into(),
        })
    }

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<ZoneFillResult, CommandError> {
        let pcb = cx
            .session()?
            .project
            .as_ref()
            .and_then(|p| p.pcb())
            .cloned()
            .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))?;
        let fills = reda_pcb::zone::fill(&pcb, &mut |done, total| {
            cx.progress(done as f64, Some(total as f64), "Filling Zones");
            !cx.is_cancelled()
        })
        .ok_or_else(CommandError::cancelled)?;
        let mut order: Vec<_> = pcb.zones().collect();
        order.sort_by(|a, b| b.priority.cmp(&a.priority).then(a.uuid.cmp(&b.uuid)));
        let zones = order
            .iter()
            .map(|z| FilledZone {
                zone: z.uuid,
                islands: fills.zones.get(&z.uuid).map_or(0, Vec::len),
            })
            .collect();
        let session = cx.session()?;
        let cached = session.location.as_ref().is_some_and(|folder| {
            let path = cache_path(folder, pcb.uuid);
            path.parent()
                .is_some_and(|dir| std::fs::create_dir_all(dir).is_ok())
                && write_atomic(&path, &reda_pcb::zone::encode(&fills)).is_ok()
        });
        if let Some(project) = session.project.as_mut() {
            project.set_zone_fills(pcb.uuid, Some(fills));
        }
        Ok(ZoneFillResult { zones, cached })
    }
}

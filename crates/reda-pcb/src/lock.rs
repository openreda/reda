//! Locks on placed objects (docs/spec/v0.1/pcb.md#locks).
//!
//! Footprints, Tracks, Vias, Zones, Free Pads, text, graphics, Dimensions, Keepouts, the
//! Board Outline and Board Cutouts carry `locked`. A lock blocks geometry changes only:
//! every Command that moves, turns, flips, deletes or reshapes an object refuses a locked
//! one through [`refuse`] unless given `include_locked`. Sync, Update from Library and
//! `pcb.board.move` still apply, and keep the lock.

use reda_command::CommandError;
use reda_model::{InDocument, Object, PcbDocument, Uuid};
use serde_json::json;

/// Whether `object` is a placed object with its lock on. Objects that cannot be locked
/// (and unknown UUIDs) never are.
pub fn is_locked(pcb: &PcbDocument, object: Uuid) -> bool {
    lock_of(pcb, object).unwrap_or(false)
}

/// The lock of `object`, or `None` when it is not a kind that can be locked.
pub fn lock_of(pcb: &PcbDocument, object: Uuid) -> Option<bool> {
    if let Some(f) = pcb.footprint(object) {
        return Some(f.locked);
    }
    if let Some(t) = pcb.track(object) {
        return Some(t.locked);
    }
    if let Some(v) = pcb.via(object) {
        return Some(v.locked);
    }
    if let Some(z) = pcb.zone(object) {
        return Some(z.locked);
    }
    if let Some(p) = pcb.free_pad(object) {
        return Some(p.locked);
    }
    if let Some(t) = pcb.text(object) {
        return Some(t.locked);
    }
    if let Some(g) = pcb.graphic(object) {
        return Some(g.locked);
    }
    if let Some(d) = pcb.dimension(object) {
        return Some(d.locked);
    }
    if let Some(k) = pcb.keepout(object) {
        return Some(k.locked);
    }
    if let Some(o) = pcb.outline().filter(|o| o.uuid == object) {
        return Some(o.locked);
    }
    pcb.cutout(object).map(|c| c.locked)
}

/// How a locked object is named in a refusal: a Footprint by its Designator, the rest by
/// kind.
fn name(pcb: &PcbDocument, object: Uuid) -> String {
    if let Some(f) = pcb.footprint(object) {
        return f.designator.clone();
    }
    let kind = if pcb.track(object).is_some() {
        "a Track"
    } else if pcb.via(object).is_some() {
        "a Via"
    } else if pcb.zone(object).is_some() {
        "a Zone"
    } else if pcb.free_pad(object).is_some() {
        "a Free Pad"
    } else if pcb.text(object).is_some() {
        "a text"
    } else if pcb.graphic(object).is_some() {
        "a graphic"
    } else if pcb.dimension(object).is_some() {
        "a Dimension"
    } else if pcb.keepout(object).is_some() {
        "a Keepout"
    } else if pcb.cutout(object).is_some() {
        "a Board Cutout"
    } else {
        "the Board Outline"
    };
    kind.to_owned()
}

/// `rejected` when any of `objects` is locked and `include_locked` is not given. `doing`
/// names the change in the message ("move", "delete", "reshape"…). `details.locked` lists
/// the locked objects in order.
pub fn refuse(
    pcb: &PcbDocument,
    objects: impl IntoIterator<Item = Uuid>,
    include_locked: bool,
    doing: &str,
) -> Result<(), CommandError> {
    if include_locked {
        return Ok(());
    }
    let mut locked: Vec<Uuid> = Vec::new();
    for o in objects {
        if is_locked(pcb, o) && !locked.contains(&o) {
            locked.push(o);
        }
    }
    if locked.is_empty() {
        return Ok(());
    }
    let names: Vec<String> = locked.iter().map(|o| name(pcb, *o)).collect();
    let mut err = CommandError::rejected(format!(
        "{} {} locked; unlock to {doing}, or pass include_locked",
        names.join(", "),
        if locked.len() == 1 { "is" } else { "are" }
    ));
    err.details = Some(json!({ "locked": locked }));
    Err(err)
}

/// `object` with its lock set to `locked`, ready to write back; `None` when it is not a
/// kind that can be locked.
pub(crate) fn with_lock(pcb: &PcbDocument, object: Uuid, locked: bool) -> Option<Object> {
    let doc = pcb.uuid;
    macro_rules! set {
        ($found:expr, $variant:ident) => {
            if let Some(found) = $found {
                let mut found = found.clone();
                found.locked = locked;
                return Some(Object::$variant(InDocument::new(doc, found)));
            }
        };
    }
    set!(pcb.footprint(object), FootprintInstance);
    set!(pcb.track(object), Track);
    set!(pcb.via(object), Via);
    set!(pcb.zone(object), Zone);
    set!(pcb.free_pad(object), FreePad);
    set!(pcb.text(object), PcbText);
    set!(pcb.graphic(object), PcbGraphic);
    set!(pcb.dimension(object), Dimension);
    set!(pcb.keepout(object), Keepout);
    set!(pcb.outline().filter(|o| o.uuid == object), BoardOutline);
    set!(pcb.cutout(object), BoardCutout);
    None
}

//! Net Classes and assigning Nets to them. A class's values are rule records scoped to it,
//! set with `pcb.rules.set` ([`super::rules`]).

use reda_command::{
    AvailabilityContext, Command, CommandError, Context, Kind, Meta, Registry, Unavailable,
};
use reda_model::{
    DesignRule, InDocument, Net, NetClass, Object, PcbDocument, RuleKind, RuleScope, Uuid,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::rules::{RecordEdit, ValuesParam, set_record};
use super::{Created, needs_pcb, pcb};
use crate::board::DEFAULT_NET_CLASS;

pub(super) fn register_all(registry: &mut Registry) {
    registry.add(NetClassAdd);
    registry.add(NetClassRename);
    registry.add(NetClassDelete);
    registry.add(NetAddToClass);
    registry.add(NetRemoveFromClass);
}

/// A trimmed name no other Net Class (but `except`) has, and not the Default Net Class's,
/// compared case-insensitively.
fn free_name(pcb: &PcbDocument, name: &str, except: Option<Uuid>) -> Result<String, CommandError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(CommandError::invalid_params("/name", "the name is empty"));
    }
    if name.eq_ignore_ascii_case(DEFAULT_NET_CLASS) {
        return Err(CommandError::invalid_params(
            "/name",
            format!("{DEFAULT_NET_CLASS:?} is the Default Net Class's name"),
        ));
    }
    if let Some(other) = pcb.net_class_named(name)
        && Some(other.uuid) != except
    {
        return Err(CommandError::invalid_params(
            "/name",
            format!("a Net Class named {:?} already exists", other.name),
        ));
    }
    Ok(name.to_owned())
}

fn class(pcb: &PcbDocument, uuid: Uuid) -> Result<NetClass, CommandError> {
    pcb.net_class(uuid).cloned().ok_or_else(|| {
        CommandError::not_found(format!(
            "no Net Class has UUID {uuid}; the Default Net Class is every Net's and is not \
             assigned"
        ))
    })
}

/// One rule record a new Net Class starts with.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ClassRuleParam {
    pub kind: RuleKind,
    /// The values it overrides, as `pcb.rules.set` takes them.
    pub values: ValuesParam,
    #[serde(default)]
    pub enabled: Option<bool>,
    #[serde(default)]
    pub comment: Option<String>,
}

pub struct NetClassAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetClassAddParams {
    /// Unique among the board's Net Classes, ignoring case, and not "Default".
    pub name: String,
    /// Nets to add to the new class.
    #[serde(default)]
    pub nets: Vec<Uuid>,
    /// Rule records the class starts with, one per kind.
    #[serde(default)]
    pub rules: Vec<ClassRuleParam>,
}

impl Command for NetClassAdd {
    const META: Meta = Meta {
        id: "pcb.net_class.add",
        title: "Add Net Class",
        description: "Adds a Net Class called `name`, ranked below every other class but \
                      above the Default Net Class, with optional rule records `rules` \
                      (`{kind, values}`, as `pcb.rules.set` takes them), and adds `nets` to \
                      it.",
        category: "pcb",
        icon: "net_class_add",
        kind: Kind::Edit,
    };
    type Params = NetClassAddParams;
    type Output = Created;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetClassAddParams) -> Result<Created, CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let name = free_name(pcb, &p.name, None)?;
        let rank = pcb.net_classes().map(|c| c.rank).max().unwrap_or(0) + 1;
        let uuid = cx.new_id();
        let class = NetClass { uuid, name, rank };
        let mut kinds = std::collections::BTreeSet::new();
        let mut records = Vec::new();
        for (i, r) in p.rules.iter().enumerate() {
            if !kinds.insert(r.kind) {
                return Err(CommandError::invalid_params(
                    &format!("/rules/{i}/kind"),
                    format!("two `{}` rules", r.kind.id()),
                ));
            }
            let scope = RuleScope::class(uuid);
            let base = DesignRule {
                uuid: cx.new_id(),
                kind: r.kind,
                scope,
                values: Default::default(),
                enabled: true,
                comment: None,
            };
            let edit = RecordEdit {
                kind: r.kind,
                scope,
                values: &r.values,
                enabled: r.enabled,
                comment: r.comment.clone().map(Some),
            };
            let record = set_record(super::pcb(cx)?, base, edit).map_err(|e| under(e, i))?;
            records.extend(record);
        }
        let nets = with_class(super::pcb(cx)?, &p.nets, uuid, true)?;
        let tx = cx.tx()?;
        tx.insert(Object::NetClass(InDocument::new(document, class)))?;
        for record in records {
            tx.insert(Object::DesignRule(InDocument::new(document, record)))?;
        }
        for net in nets {
            tx.replace(Object::Net(InDocument::new(document, net)))?;
        }
        Ok(Created { uuid })
    }
}

/// `error` about a `pcb.rules.set`-style param, re-pointed under `/rules/<i>`.
fn under(mut error: CommandError, i: usize) -> CommandError {
    if let Some(serde_json::Value::Object(details)) = &mut error.details
        && let Some(serde_json::Value::String(pointer)) = details.get_mut("pointer")
    {
        *pointer = format!("/rules/{i}{pointer}");
    }
    error
}

pub struct NetClassRename;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetClassRenameParams {
    /// The Net Class's UUID.
    pub class: Uuid,
    /// The new name, unique among the board's Net Classes, ignoring case.
    pub name: String,
}

impl Command for NetClassRename {
    const META: Meta = Meta {
        id: "pcb.net_class.rename",
        title: "Rename Net Class",
        description: "Renames a Net Class. Its Nets stay in it. The Default Net Class cannot \
                      be renamed.",
        category: "pcb",
        icon: "rename",
        kind: Kind::Edit,
    };
    type Params = NetClassRenameParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetClassRenameParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let mut class = class(pcb, p.class)?;
        class.name = free_name(pcb, &p.name, Some(p.class))?;
        cx.tx()?
            .replace(Object::NetClass(InDocument::new(document, class)))?;
        Ok(())
    }
}

pub struct NetClassDelete;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetClassDeleteParams {
    /// The Net Class's UUID.
    pub class: Uuid,
}

impl Command for NetClassDelete {
    const META: Meta = Meta {
        id: "pcb.net_class.delete",
        title: "Delete Net Class",
        description: "Deletes a Net Class: it leaves its Nets and its rule records are \
                      deleted. The Default Net Class cannot be deleted.",
        category: "pcb",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = NetClassDeleteParams;
    type Output = ();

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(&self, cx: &mut Context<'_>, p: NetClassDeleteParams) -> Result<(), CommandError> {
        let pcb = pcb(cx)?;
        let document = pcb.uuid;
        let class = class(pcb, p.class)?;
        let nets: Vec<Uuid> = pcb
            .nets()
            .filter(|n| n.classes.contains(&class.uuid))
            .map(|n| n.uuid)
            .collect();
        let left = with_class(pcb, &nets, class.uuid, false)?;
        let records: Vec<Uuid> = pcb
            .rules()
            .filter(|r| r.scope.net_class == Some(class.uuid))
            .map(|r| r.uuid)
            .collect();
        let tx = cx.tx()?;
        for net in left {
            tx.replace(Object::Net(InDocument::new(document, net)))?;
        }
        for record in records {
            tx.remove(record)?;
        }
        tx.remove(class.uuid)?;
        Ok(())
    }
}

/// Which Nets to add to or remove from a class: a list, or a pattern.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct NetMembershipParams {
    /// The Nets' UUIDs.
    #[serde(default)]
    pub nets: Option<Vec<Uuid>>,
    /// A glob over Net names, `*` for any run and `?` for one character, ignoring case. It
    /// matches the Nets that exist now; nothing is stored.
    #[serde(default)]
    pub pattern: Option<String>,
    /// The Net Class's UUID.
    pub class: Uuid,
}

/// The Nets whose class list changed.
#[derive(Debug, Serialize, JsonSchema)]
pub struct MembershipResult {
    pub nets: Vec<Uuid>,
}

fn membership(
    cx: &mut Context<'_>,
    p: NetMembershipParams,
    add: bool,
) -> Result<MembershipResult, CommandError> {
    let pcb = pcb(cx)?;
    let document = pcb.uuid;
    class(pcb, p.class)?;
    let nets = match (p.nets, p.pattern) {
        (Some(nets), None) => nets,
        (None, Some(pattern)) => pcb
            .nets()
            .filter(|n| glob_match(&pattern, &n.name))
            .map(|n| n.uuid)
            .collect(),
        _ => {
            return Err(CommandError::invalid_params(
                "/nets",
                "give exactly one of `nets` and `pattern`",
            ));
        }
    };
    let changed = with_class(pcb, &nets, p.class, add)?;
    let uuids = changed.iter().map(|n| n.uuid).collect();
    let tx = cx.tx()?;
    for net in changed {
        tx.replace(Object::Net(InDocument::new(document, net)))?;
    }
    Ok(MembershipResult { nets: uuids })
}

pub struct NetAddToClass;

impl Command for NetAddToClass {
    const META: Meta = Meta {
        id: "pcb.net.add_to_class",
        title: "Add Nets to Net Class",
        description: "Adds Nets to the Net Class `class`: the `nets` listed, or those whose \
                      names match the glob `pattern` now (`*` and `?`, ignoring case). Nets \
                      already in it are left alone. Returns the Nets that changed.",
        category: "pcb",
        icon: "net_class",
        kind: Kind::Edit,
    };
    type Params = NetMembershipParams;
    type Output = MembershipResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: NetMembershipParams,
    ) -> Result<MembershipResult, CommandError> {
        membership(cx, p, true)
    }
}

pub struct NetRemoveFromClass;

impl Command for NetRemoveFromClass {
    const META: Meta = Meta {
        id: "pcb.net.remove_from_class",
        title: "Remove Nets from Net Class",
        description: "Removes Nets from the Net Class `class`: the `nets` listed, or those \
                      whose names match the glob `pattern` (`*` and `?`, ignoring case). Every \
                      Net stays in the Default Net Class. Returns the Nets that changed.",
        category: "pcb",
        icon: "net_class",
        kind: Kind::Edit,
    };
    type Params = NetMembershipParams;
    type Output = MembershipResult;

    fn available(&self, cx: &AvailabilityContext<'_>) -> Result<(), Unavailable> {
        needs_pcb(cx)
    }

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: NetMembershipParams,
    ) -> Result<MembershipResult, CommandError> {
        membership(cx, p, false)
    }
}

/// `nets` with `class` added to (or removed from) their lists, leaving out those already
/// so; an unknown Net is `not_found`.
fn with_class(
    pcb: &PcbDocument,
    nets: &[Uuid],
    class: Uuid,
    add: bool,
) -> Result<Vec<Net>, CommandError> {
    let mut out: Vec<Net> = Vec::new();
    for (i, uuid) in nets.iter().enumerate() {
        let net = pcb.net(*uuid).ok_or_else(|| {
            CommandError::not_found(format!("no Net has UUID {uuid} (`nets[{i}]`)"))
        })?;
        if net.classes.contains(&class) == add || out.iter().any(|n| n.uuid == *uuid) {
            continue;
        }
        let mut net = net.clone();
        if add {
            net.classes.push(class);
            net.classes.sort();
        } else {
            net.classes.retain(|c| *c != class);
        }
        out.push(net);
    }
    Ok(out)
}

/// Whether `name` matches the glob `pattern`: `*` any run, `?` one character, the rest
/// literal, ignoring case.
pub fn glob_match(pattern: &str, name: &str) -> bool {
    let p: Vec<char> = pattern.to_lowercase().chars().collect();
    let n: Vec<char> = name.to_lowercase().chars().collect();
    let (mut pi, mut ni) = (0, 0);
    let mut star: Option<(usize, usize)> = None;
    while ni < n.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == n[ni]) {
            pi += 1;
            ni += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some((pi, ni));
            pi += 1;
        } else if let Some((sp, sn)) = star {
            pi = sp + 1;
            ni = sn + 1;
            star = Some((sp, sn + 1));
        } else {
            return false;
        }
    }
    p[pi..].iter().all(|c| *c == '*')
}

#[cfg(test)]
mod tests {
    use super::glob_match;

    #[test]
    fn globs_match_runs_and_single_characters_ignoring_case() {
        assert!(glob_match("VCC*", "vcc_3v3"));
        assert!(glob_match("*", ""));
        assert!(glob_match("D?", "D1"));
        assert!(!glob_match("D?", "D10"));
        assert!(glob_match("*_N", "USB_N"));
        assert!(!glob_match("*_N", "USB_P"));
        assert!(glob_match("a*b*c", "aXbYbZc"));
    }
}

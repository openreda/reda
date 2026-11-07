//! `schematic.port.partners`: the Ports a Port joins on other sheets, for jump to partner,
//! with the cross-reference note drawn past its name. The note's styles may come from the
//! Preferences, which live in the engine, so the Command does too; the derivation is
//! `reda_schematic::cross_ref`'s.

use std::collections::BTreeMap;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry, UserDirs};
use reda_model::{CrossReferenceStyles, Project, Uuid};
use reda_schematic::cross_ref::{self, Partner};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::preferences;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(PortPartners);
}

/// The styles the Project's cross-references are drawn in, from the Project or the
/// Preferences for `dirs`, or `None` when the Project shows none.
pub fn cross_reference_styles(
    project: &Project,
    dirs: Option<&UserDirs>,
) -> Option<CrossReferenceStyles> {
    let options = project.props().schematic;
    if !options.cross_references {
        return None;
    }
    let preferences = match options.cross_references_follow_preferences {
        true => preferences::current(dirs)
            .schematic
            .cross_reference_styles(),
        false => CrossReferenceStyles::DEFAULT,
    };
    options.cross_reference_styles(preferences)
}

/// Every Port's cross-reference note as the Project shows them, keyed by Port UUID: what
/// the canvas and the PDF draw past each Port's name.
pub fn cross_reference_notes(project: &Project, dirs: Option<&UserDirs>) -> BTreeMap<Uuid, String> {
    cross_ref::notes(project, cross_reference_styles(project, dirs))
}

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PortPartnersParams {
    /// The Port's UUID.
    pub port: Uuid,
}

/// What `schematic.port.partners` returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct PortPartnersResult {
    /// The Ports of the same name (ignoring case) on other sheets, in sheet order and then
    /// from the top left of each sheet. Power Ports are never partners.
    pub partners: Vec<Partner>,
    /// The cross-reference note drawn past the Port's name, such as `[2:B3, 3:A1]`, or
    /// null when the Project shows none, the Port hides it, or it has no partners.
    pub note: Option<String>,
}

pub struct PortPartners;

impl Command for PortPartners {
    const META: Meta = Meta {
        id: "schematic.port.partners",
        title: "Port Partners",
        description: "Lists the Ports a Port joins on other Schematic Sheets (same name, \
                      ignoring case), in sheet order, with each one's sheet number, title, \
                      position and Border Zone: the targets of jump to partner. Also gives \
                      the Port's cross-reference note in the Project's styles, or the \
                      Preferences' when the Project follows them.",
        category: "schematic",
        icon: "port",
        kind: Kind::Query,
    };
    type Params = PortPartnersParams;
    type Output = PortPartnersResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PortPartnersParams,
    ) -> Result<PortPartnersResult, CommandError> {
        let project = cx.project()?;
        let partners = cross_ref::partners(project, p.port)
            .ok_or_else(|| CommandError::not_found(format!("no Port has UUID {}", p.port)))?;
        let note = cross_reference_notes(project, cx.user_dirs()).remove(&p.port);
        Ok(PortPartnersResult { partners, note })
    }
}

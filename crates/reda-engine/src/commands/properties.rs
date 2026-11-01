//! `project.properties.set`: the Project properties that fill each sheet's title block.

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{Object, ProjectProperties};
use schemars::JsonSchema;
use serde::Deserialize;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ProjectPropertiesSet);
}

pub struct ProjectPropertiesSet;

/// The properties to change; those left out keep their value. An empty string clears one.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PropertiesSetParams {
    pub title: Option<String>,
    pub revision: Option<String>,
    /// Free text, such as `2026-11-01`.
    pub date: Option<String>,
    pub author: Option<String>,
    pub company: Option<String>,
}

impl Command for ProjectPropertiesSet {
    const META: Meta = Meta {
        id: "project.properties.set",
        title: "Set Project Properties",
        description: "Sets the Project properties (title, revision, date, author, company) \
                      that fill every sheet's title block. Properties left out keep their \
                      value. Returns all of them.",
        category: "project",
        icon: "properties",
        kind: Kind::Edit,
    };
    type Params = PropertiesSetParams;
    type Output = ProjectProperties;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PropertiesSetParams,
    ) -> Result<ProjectProperties, CommandError> {
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        let fields = &mut props.properties;
        for (new, field) in [
            (p.title, &mut fields.title),
            (p.revision, &mut fields.revision),
            (p.date, &mut fields.date),
            (p.author, &mut fields.author),
            (p.company, &mut fields.company),
        ] {
            if let Some(new) = new {
                *field = new;
            }
        }
        let out = props.properties.clone();
        tx.replace(Object::Project(props))?;
        Ok(out)
    }
}

//! `project.properties.set`: the Project properties that fill each sheet's title block,
//! and the board thickness and copper weight for fab notes.

use reda_command::{Command, CommandError, Context, Kind, Length, Meta, Registry};
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
    /// The finished board thickness, for fab notes, such as `1.6mm`. Zero clears it; a
    /// negative thickness is refused.
    pub board_thickness: Option<Length>,
    /// The copper weight, for fab notes, as free text such as `1 oz`.
    pub copper_weight: Option<String>,
}

impl Command for ProjectPropertiesSet {
    const META: Meta = Meta {
        id: "project.properties.set",
        title: "Set Project Properties",
        description: "Sets the Project properties: the title, revision, date, author and \
                      company that fill every sheet's title block, and the board thickness \
                      and copper weight that go into fab notes. Properties left out keep \
                      their value. Returns all of them.",
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
        if let Some(thickness) = p.board_thickness
            && thickness.nm().0 < 0
        {
            return Err(CommandError::invalid_params(
                "/board_thickness",
                "the board thickness cannot be negative",
            ));
        }
        for (pointer, value) in [
            ("/title", &p.title),
            ("/revision", &p.revision),
            ("/date", &p.date),
            ("/author", &p.author),
            ("/company", &p.company),
            ("/copper_weight", &p.copper_weight),
        ] {
            if let Some(value) = value {
                CommandError::check_text(pointer, value)?;
            }
        }
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        let fields = &mut props.properties;
        for (new, field) in [
            (p.title, &mut fields.title),
            (p.revision, &mut fields.revision),
            (p.date, &mut fields.date),
            (p.author, &mut fields.author),
            (p.company, &mut fields.company),
            (p.copper_weight, &mut fields.copper_weight),
        ] {
            if let Some(new) = new {
                *field = new;
            }
        }
        if let Some(thickness) = p.board_thickness {
            fields.board_thickness = (thickness.nm().0 > 0).then_some(thickness.nm());
        }
        let out = props.properties.clone();
        tx.replace(Object::Project(props))?;
        Ok(out)
    }
}

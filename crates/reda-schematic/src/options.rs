//! `schematic.options.set`: the Project's schematic options, which say how Nets are named.

use reda_command::{Command, CommandError, Context, Kind, Meta};
use reda_model::{Object, SchematicOptions};
use schemars::JsonSchema;
use serde::Deserialize;

/// The options to change; those left out keep their value.
#[derive(Debug, Default, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct OptionsSetParams {
    /// Power Port Names Take Priority: Power Port > Net Label > Port > auto-name instead
    /// of Net Label > Power Port > Port > auto-name. Off by default.
    pub power_port_names_priority: Option<bool>,
    /// Allow Ports to Name Nets. On by default; off, a Port still joins its Net across
    /// sheets but never names it.
    pub allow_ports_to_name_nets: Option<bool>,
    /// Append Sheet Numbers to Local Nets: a Local Net named by a Net Label gets the board
    /// name `<name>_<sheet number>`. Off by default.
    pub append_sheet_numbers: Option<bool>,
}

pub struct OptionsSet;

impl Command for OptionsSet {
    const META: Meta = Meta {
        id: "schematic.options.set",
        title: "Set Schematic Options",
        description: "Sets the Project's Net naming options: Power Port Names Take \
                      Priority, Allow Ports to Name Nets and Append Sheet Numbers to Local \
                      Nets. Options left out keep their value. Stored in the Project; \
                      returns all of them.",
        category: "schematic",
        icon: "properties",
        kind: Kind::Edit,
    };
    type Params = OptionsSetParams;
    type Output = SchematicOptions;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: OptionsSetParams,
    ) -> Result<SchematicOptions, CommandError> {
        let tx = cx.tx()?;
        let mut props = tx.project().props().clone();
        let options = &mut props.schematic;
        for (new, option) in [
            (
                p.power_port_names_priority,
                &mut options.power_port_names_priority,
            ),
            (
                p.allow_ports_to_name_nets,
                &mut options.allow_ports_to_name_nets,
            ),
            (p.append_sheet_numbers, &mut options.append_sheet_numbers),
        ] {
            if let Some(new) = new {
                *option = new;
            }
        }
        let out = *options;
        tx.replace(Object::Project(props))?;
        Ok(out)
    }
}

//! Fab Presets: listing the presets there are, the Project's copies of them ("Update from
//! source", "Save as user preset"), and adding and deleting copies.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, NoParams, Registry};
use reda_model::{FabPreset, Object, Project, ProjectFabPreset, Uuid};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::fab_presets::{self, Available, Origin};
use crate::state;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(PresetList);
    registry.add(PresetAdd);
    registry.add(PresetDelete);
    registry.add(PresetSaveUser);
    registry.add(PresetRenameUser);
    registry.add(PresetDeleteUser);
    registry.add(PresetSourceDiff);
    registry.add(PresetUpdateFromSource);
}

/// Names one of the Project's Fab Preset copies.
#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetRef {
    /// The Project copy's UUID.
    pub preset: Uuid,
}

fn copy(project: &Project, uuid: Uuid) -> Result<ProjectFabPreset, CommandError> {
    project
        .fab_preset(uuid)
        .cloned()
        .ok_or_else(|| CommandError::not_found(format!("the Project has no Fab Preset {uuid}")))
}

pub struct PresetList;

#[derive(Debug, Serialize, Deserialize, JsonSchema)]
pub struct PresetListResult {
    /// The shipped presets, then the user's.
    pub presets: Vec<Available>,
    /// The New Project dialog's default: the last preset used, else Generic.
    pub default: String,
    /// User preset files that could not be read.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
}

impl Command for PresetList {
    const META: Meta = Meta {
        id: "fab_preset.list",
        title: "List Fab Presets",
        description: "Lists the Fab Presets a new Project or a job can use: the shipped \
                      Generic, JLCPCB and PCBWay, then the user's `.redafab` presets. \
                      `default` is the last one used for a new Project, else Generic.",
        category: "output",
        icon: "fab_preset",
        kind: Kind::File,
    };
    type Params = NoParams;
    type Output = PresetListResult;

    fn run(&self, cx: &mut Context<'_>, _: NoParams) -> Result<PresetListResult, CommandError> {
        let dirs = cx.user_dirs();
        let (presets, warnings) = fab_presets::available(dirs);
        let last = dirs.and_then(|d| state::read(d).last_fab_preset);
        let default = last
            .and_then(|name| {
                presets
                    .iter()
                    .find(|a| a.preset.name.eq_ignore_ascii_case(&name))
                    .map(|a| a.preset.name.clone())
            })
            .unwrap_or_else(|| reda_output::GENERIC_NAME.into());
        Ok(PresetListResult {
            presets,
            default,
            warnings,
        })
    }
}

/// What a Command that creates a copy returns.
#[derive(Debug, Serialize, JsonSchema)]
pub struct CopyCreated {
    /// The new copy's UUID.
    pub uuid: Uuid,
}

pub struct PresetAdd;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetAddParams {
    /// The preset to copy: a shipped preset or one of the user's.
    pub name: String,
}

impl Command for PresetAdd {
    const META: Meta = Meta {
        id: "fab_preset.add",
        title: "Add Fab Preset to Project",
        description: "Copies a shipped or user Fab Preset into the Project, so Output Jobs \
                      can use it and it travels with the Project.",
        category: "output",
        icon: "fab_preset_add",
        kind: Kind::Edit,
    };
    type Params = PresetAddParams;
    type Output = CopyCreated;

    fn run(&self, cx: &mut Context<'_>, p: PresetAddParams) -> Result<CopyCreated, CommandError> {
        let preset = fab_presets::find_or_fail("/name", &p.name, cx.user_dirs())?.preset;
        let uuid = cx.new_id();
        cx.tx()?
            .insert(Object::FabPreset(ProjectFabPreset::new(uuid, preset)))?;
        Ok(CopyCreated { uuid })
    }
}

pub struct PresetDelete;

impl Command for PresetDelete {
    const META: Meta = Meta {
        id: "fab_preset.delete",
        title: "Remove Fab Preset from Project",
        description: "Deletes one of the Project's Fab Preset copies. A copy an Output Job \
                      uses is refused.",
        category: "output",
        icon: "delete",
        kind: Kind::Edit,
    };
    type Params = PresetRef;
    type Output = ();

    fn run(&self, cx: &mut Context<'_>, p: PresetRef) -> Result<(), CommandError> {
        let project = cx.project()?;
        let preset = copy(project, p.preset)?;
        if let Some(job) = project.output_jobs().find(|j| j.fab_preset == p.preset) {
            return Err(CommandError::rejected(format!(
                "the Output Job {:?} uses the Fab Preset {:?}",
                job.name, preset.name
            )));
        }
        cx.tx()?.remove(p.preset)?;
        Ok(())
    }
}

pub struct PresetSaveUser;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetSaveUserParams {
    /// The Project copy to save.
    pub preset: Uuid,
    /// The user preset's name; defaults to the copy's name. A shipped preset's name is
    /// refused, and a user preset of the same name is replaced.
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct PresetSaveUserResult {
    /// The `.redafab` file written.
    pub path: PathBuf,
}

impl Command for PresetSaveUser {
    const META: Meta = Meta {
        id: "fab_preset.save_user",
        title: "Save as User Preset",
        description: "Saves one of the Project's Fab Preset copies as a user preset, a \
                      `.redafab` file in the per-user config folder, so new Projects can \
                      use it.",
        category: "output",
        icon: "save",
        kind: Kind::File,
    };
    type Params = PresetSaveUserParams;
    type Output = PresetSaveUserResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PresetSaveUserParams,
    ) -> Result<PresetSaveUserResult, CommandError> {
        let mut preset = copy(cx.project()?, p.preset)?.preset();
        if let Some(name) = p.name {
            preset.name = name.trim().to_owned();
        }
        let dirs = cx
            .user_dirs()
            .ok_or_else(|| CommandError::unavailable("there is no per-user config folder"))?;
        let path = fab_presets::save_user(dirs, &preset)?;
        Ok(PresetSaveUserResult { path })
    }
}

fn user_dirs<'c>(cx: &'c Context<'_>) -> Result<&'c reda_command::UserDirs, CommandError> {
    cx.user_dirs()
        .ok_or_else(|| CommandError::unavailable("there is no per-user config folder"))
}

pub struct PresetRenameUser;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetRenameUserParams {
    /// The user preset's name (case does not matter).
    pub name: String,
    /// Its new name: a file name, and not a shipped preset's or another user preset's.
    pub new_name: String,
}

impl Command for PresetRenameUser {
    const META: Meta = Meta {
        id: "fab_preset.rename_user",
        title: "Rename User Fab Preset",
        description: "Renames one of the user's Fab Presets and its `.redafab` file. Project \
                      copies keep the old name, so they no longer update from it.",
        category: "output",
        icon: "rename",
        kind: Kind::File,
    };
    type Params = PresetRenameUserParams;
    type Output = PresetSaveUserResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PresetRenameUserParams,
    ) -> Result<PresetSaveUserResult, CommandError> {
        let path = fab_presets::rename_user(user_dirs(cx)?, &p.name, &p.new_name)?;
        Ok(PresetSaveUserResult { path })
    }
}

pub struct PresetDeleteUser;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct PresetDeleteUserParams {
    /// The user preset's name (case does not matter).
    pub name: String,
}

impl Command for PresetDeleteUser {
    const META: Meta = Meta {
        id: "fab_preset.delete_user",
        title: "Delete User Fab Preset",
        description: "Moves one of the user's Fab Presets, its `.redafab` file, to the OS \
                      trash. Project copies of it stay.",
        category: "output",
        icon: "delete",
        kind: Kind::File,
    };
    type Params = PresetDeleteUserParams;
    type Output = PresetSaveUserResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: PresetDeleteUserParams,
    ) -> Result<PresetSaveUserResult, CommandError> {
        let (_, path) = fab_presets::user_preset(user_dirs(cx)?, &p.name)?;
        let trash = cx
            .session()?
            .trash
            .clone()
            .ok_or_else(|| CommandError::unavailable("there is no trash to move it to"))?;
        trash.trash(&path).map_err(|e| {
            CommandError::internal(format!("moving {} to the trash: {e}", path.display()))
        })?;
        Ok(PresetSaveUserResult { path })
    }
}

/// One field where a Project copy and its source differ.
#[derive(Debug, PartialEq, Serialize, JsonSchema)]
pub struct FieldChange {
    /// The field, such as `rules.clearance`, `gerber.x2_attributes` or `bom`.
    pub field: String,
    /// The Project copy's value.
    pub project: Value,
    /// The source's value.
    pub source: Value,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct SourceDiff {
    /// Where the source preset comes from.
    pub origin: Origin,
    /// Every field that differs; empty when the copy is up to date.
    pub changes: Vec<FieldChange>,
}

/// The fields that differ between two presets, rules and Gerber switches one by one.
fn diff(project: &FabPreset, source: &FabPreset) -> Vec<FieldChange> {
    let to_value = |p: &FabPreset| serde_json::to_value(p).expect("presets serialise");
    let (Value::Object(a), Value::Object(b)) = (to_value(project), to_value(source)) else {
        unreachable!("a preset serialises as an object");
    };
    let mut changes = vec![];
    for (key, a) in &a {
        let b = &b[key];
        match (a, b) {
            (Value::Object(inner_a), Value::Object(inner_b)) => {
                for (inner, value) in inner_a {
                    if &inner_b[inner] != value {
                        changes.push(FieldChange {
                            field: format!("{key}.{inner}"),
                            project: value.clone(),
                            source: inner_b[inner].clone(),
                        });
                    }
                }
            }
            _ if a != b => changes.push(FieldChange {
                field: key.clone(),
                project: a.clone(),
                source: b.clone(),
            }),
            _ => {}
        }
    }
    changes
}

/// The copy and the preset it came from.
fn with_source(
    cx: &Context<'_>,
    uuid: Uuid,
) -> Result<(ProjectFabPreset, Available), CommandError> {
    let copy = copy(cx.project()?, uuid)?;
    let source = fab_presets::find(&copy.name, cx.user_dirs()).ok_or_else(|| {
        CommandError::not_found(format!(
            "the Fab Preset {:?} this copy came from no longer exists",
            copy.name
        ))
    })?;
    Ok((copy, source))
}

pub struct PresetSourceDiff;

impl Command for PresetSourceDiff {
    const META: Meta = Meta {
        id: "fab_preset.source_diff",
        title: "Preview Update from Source",
        description: "Compares one of the Project's Fab Preset copies with the shipped or \
                      user preset of the same name it came from, field by field. \
                      `fab_preset.update_from_source` applies the result.",
        category: "output",
        icon: "fab_preset",
        kind: Kind::Query,
    };
    type Params = PresetRef;
    type Output = SourceDiff;

    fn run(&self, cx: &mut Context<'_>, p: PresetRef) -> Result<SourceDiff, CommandError> {
        let (copy, source) = with_source(cx, p.preset)?;
        Ok(SourceDiff {
            origin: source.origin,
            changes: diff(&copy.preset(), &source.preset),
        })
    }
}

pub struct PresetUpdateFromSource;

impl Command for PresetUpdateFromSource {
    const META: Meta = Meta {
        id: "fab_preset.update_from_source",
        title: "Update from Source",
        description: "Replaces one of the Project's Fab Preset copies with the current \
                      values of the preset it came from, after `fab_preset.source_diff` has \
                      shown what changes. The board's own rules are not touched; use \
                      `pcb.rules.apply_fab_preset` for those. Returns the changes.",
        category: "output",
        icon: "fab_preset_update",
        kind: Kind::Edit,
    };
    type Params = PresetRef;
    type Output = SourceDiff;

    fn run(&self, cx: &mut Context<'_>, p: PresetRef) -> Result<SourceDiff, CommandError> {
        let (copy, source) = with_source(cx, p.preset)?;
        let changes = diff(&copy.preset(), &source.preset);
        if !changes.is_empty() {
            let updated = ProjectFabPreset::new(copy.uuid, source.preset);
            cx.tx()?.replace(Object::FabPreset(updated))?;
        }
        Ok(SourceDiff {
            origin: source.origin,
            changes,
        })
    }
}

//! Finding Fab Presets: the shipped ones (`reda-output`) and the user's `.redafab` files in
//! `<config>/fab-presets/`, and saving, renaming and deleting a user preset.
//!
//! Names are compared case-insensitively, and a shipped preset wins over a user preset of
//! the same name (saving refuses such a name).

use std::path::{Path, PathBuf};

use reda_command::{CommandError, UserDirs};
use reda_model::FabPreset;
use schemars::JsonSchema;
use serde::Serialize;

use crate::format::{read_bytes, read_fab_file, write_fab_file};

/// The extension of a user Fab Preset file.
pub const EXTENSION: &str = "redafab";

/// The folder of the user's Fab Presets.
pub fn dir(dirs: &UserDirs) -> PathBuf {
    dirs.config.join("fab-presets")
}

/// Where a preset comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Origin {
    /// Shipped with REDA.
    Shipped,
    /// A `.redafab` file in the user's config folder.
    User,
}

/// A preset that can be picked, with where it comes from.
#[derive(Clone, Debug, Serialize, JsonSchema)]
pub struct Available {
    pub preset: FabPreset,
    pub origin: Origin,
    /// The `.redafab` file of a user preset.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<PathBuf>,
}

/// Every user preset that reads, sorted by file name, and a warning per file that does not.
pub fn user_presets(dirs: &UserDirs) -> (Vec<Available>, Vec<String>) {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir(dirs))
        .map(|entries| {
            entries
                .flatten()
                .map(|e| e.path())
                .filter(|p| p.extension().is_some_and(|e| e == EXTENSION))
                .collect()
        })
        .unwrap_or_default();
    files.sort();
    let (mut presets, mut warnings) = (vec![], vec![]);
    for path in files {
        match read_bytes(&path).and_then(|bytes| read_fab_file(&path, &bytes)) {
            Ok(preset) => presets.push(Available {
                preset,
                origin: Origin::User,
                path: Some(path),
            }),
            Err(e) => warnings.push(e.to_string()),
        }
    }
    (presets, warnings)
}

/// Every preset that can be picked: the shipped ones first, then the user's, without user
/// presets whose name a shipped preset already has.
pub fn available(dirs: Option<&UserDirs>) -> (Vec<Available>, Vec<String>) {
    let mut all: Vec<Available> = reda_output::shipped()
        .into_iter()
        .map(|preset| Available {
            preset,
            origin: Origin::Shipped,
            path: None,
        })
        .collect();
    let Some(dirs) = dirs else {
        return (all, vec![]);
    };
    let (user, warnings) = user_presets(dirs);
    for preset in user {
        let taken = all
            .iter()
            .any(|a| a.preset.name.eq_ignore_ascii_case(&preset.preset.name));
        if !taken {
            all.push(preset);
        }
    }
    (all, warnings)
}

/// The preset called `name`: a shipped one, or else one of the user's.
pub fn find(name: &str, dirs: Option<&UserDirs>) -> Option<Available> {
    available(dirs)
        .0
        .into_iter()
        .find(|a| a.preset.name.eq_ignore_ascii_case(name.trim()))
}

/// [`find`], failing as `invalid_params` at `pointer` with the names there are.
pub fn find_or_fail(
    pointer: &str,
    name: &str,
    dirs: Option<&UserDirs>,
) -> Result<Available, CommandError> {
    find(name, dirs).ok_or_else(|| {
        let names: Vec<String> = available(dirs)
            .0
            .into_iter()
            .map(|a| a.preset.name)
            .collect();
        CommandError::invalid_params(
            pointer,
            format!(
                "no Fab Preset is named {name:?}; there are {}",
                names.join(", ")
            ),
        )
    })
}

/// The user preset called `name` (not a shipped one), with its file.
pub fn user_preset(dirs: &UserDirs, name: &str) -> Result<(FabPreset, PathBuf), CommandError> {
    user_presets(dirs)
        .0
        .into_iter()
        .find(|a| a.preset.name.eq_ignore_ascii_case(name.trim()))
        .and_then(|a| Some((a.preset, a.path?)))
        .ok_or_else(|| CommandError::not_found(format!("there is no user Fab Preset {name:?}")))
}

/// Refuses `name` for a user preset, at `pointer`: not a file name, or a shipped preset's.
fn check_user_name(pointer: &str, name: &str) -> Result<(), CommandError> {
    crate::commands::project::check_file_stem(name)
        .map_err(|why| CommandError::invalid_params(pointer, why))?;
    if reda_output::shipped_named(name).is_some() {
        return Err(CommandError::invalid_params(
            pointer,
            format!("{name:?} is a shipped Fab Preset; use another name"),
        ));
    }
    Ok(())
}

/// Renames the user preset `name` to `new_name`, file included; returns the new file. A
/// shipped preset's name or another user preset's is refused.
pub fn rename_user(dirs: &UserDirs, name: &str, new_name: &str) -> Result<PathBuf, CommandError> {
    let (mut preset, old) = user_preset(dirs, name)?;
    let new_name = new_name.trim();
    check_user_name("/new_name", new_name)?;
    let same = preset.name.eq_ignore_ascii_case(new_name);
    if !same && user_preset(dirs, new_name).is_ok() {
        return Err(CommandError::invalid_params(
            "/new_name",
            format!("there is already a user Fab Preset {new_name:?}"),
        ));
    }
    preset.name = new_name.to_owned();
    // A change of case only keeps the file: on a case-insensitive disk the new file name
    // would be the old file.
    let path = if same {
        old.clone()
    } else {
        dir(dirs).join(format!("{new_name}.{EXTENSION}"))
    };
    write_user(&path, &preset)?;
    if path != old {
        std::fs::remove_file(&old)
            .map_err(|e| CommandError::internal(format!("removing {}: {e}", old.display())))?;
    }
    Ok(path)
}

fn write_user(path: &Path, preset: &FabPreset) -> Result<(), CommandError> {
    let bytes = write_fab_file(preset).map_err(|e| CommandError::internal(e.to_string()))?;
    let io = |path: &Path, e: std::io::Error| {
        CommandError::internal(format!("writing {}: {e}", path.display()))
    };
    if let Some(folder) = path.parent() {
        std::fs::create_dir_all(folder).map_err(|e| io(folder, e))?;
    }
    crate::fs::write_atomic(path, &bytes).map_err(|e| io(path, e))
}

/// Writes `preset` as the user preset `<config>/fab-presets/<name>.redafab`, replacing a
/// user preset of the same name. A shipped preset's name is refused.
pub fn save_user(dirs: &UserDirs, preset: &FabPreset) -> Result<PathBuf, CommandError> {
    let name = preset.name.trim();
    check_user_name("/name", name)?;
    let folder = dir(dirs);
    // Replace the file that already holds this name, whatever its file name.
    let existing = user_presets(dirs)
        .0
        .into_iter()
        .find(|a| a.preset.name.eq_ignore_ascii_case(name))
        .and_then(|a| a.path);
    let path = existing.unwrap_or_else(|| folder.join(format!("{name}.{EXTENSION}")));
    write_user(&path, preset)?;
    Ok(path)
}

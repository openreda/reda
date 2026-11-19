//! `pcb.rules.export` and `pcb.rules.import`: a board's design rules as a `.redarules`
//! file, carrying the rule records, the DRC severities and the Net Class names, but no
//! Net membership. They read and write files, so they live in the engine; what an import
//! changes is `reda_pcb::rules::transfer`'s.

use std::path::PathBuf;

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_model::{BoardSetup, InDocument, Object, PcbDocument, RuleKind};
use reda_pcb::rules::transfer::{ImportPreview, export, plan};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::format::{read_bytes, read_rules_file, write_rules_file};
use crate::fs::write_atomic;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(RulesExport);
    registry.add(RulesImport);
}

fn pcb<'c>(cx: &'c Context<'_>) -> Result<&'c PcbDocument, CommandError> {
    cx.project()?
        .pcb()
        .ok_or_else(|| CommandError::unavailable("the Project has no PCB Document"))
}

pub struct RulesExport;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulesExportParams {
    /// Where to write the `.redarules` file. A relative path is taken from the Project
    /// folder.
    pub path: PathBuf,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RulesExportResult {
    /// The file written.
    pub path: PathBuf,
    /// How many rule records it holds.
    pub rules: usize,
}

impl Command for RulesExport {
    const META: Meta = Meta {
        id: "pcb.rules.export",
        title: "Export Design Rules",
        description: "Writes the PCB's design rules to a `.redarules` file: every rule \
                      record, the DRC severities and the Net Class names, which another \
                      board's `pcb.rules.import` matches by name. It records the copper \
                      count and writes explicit Layers and allowed Via spans by copper \
                      position. Net membership is left out.",
        category: "pcb",
        icon: "rules",
        kind: Kind::File,
    };
    type Params = RulesExportParams;
    type Output = RulesExportResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: RulesExportParams,
    ) -> Result<RulesExportResult, CommandError> {
        let set = export(pcb(cx)?);
        let session = cx.session()?;
        let path = match &session.location {
            Some(folder) if p.path.is_relative() => folder.join(&p.path),
            None if p.path.is_relative() => {
                return Err(CommandError::rejected(
                    "the Project has no folder yet: give an absolute path",
                ));
            }
            _ => p.path.clone(),
        };
        let bytes = write_rules_file(&set).map_err(|e| CommandError::internal(e.to_string()))?;
        write_atomic(&path, &bytes).map_err(|e| {
            CommandError::internal(format!("could not write {}: {e}", path.display()))
        })?;
        Ok(RulesExportResult {
            path,
            rules: set.rules.len(),
        })
    }
}

pub struct RulesImport;

#[derive(Debug, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct RulesImportParams {
    /// The `.redarules` file, as an absolute path.
    pub path: PathBuf,
    /// The rule kinds to import; left out, every kind in the file.
    #[serde(default)]
    pub kinds: Option<Vec<RuleKind>>,
    /// Checked position-dependent records, by zero-based index in the source file. Omit
    /// to select them only when the source and target have the same copper count.
    #[serde(default)]
    pub position_records: Option<std::collections::BTreeSet<usize>>,
    /// Whether to take the file's DRC severities too.
    #[serde(default = "yes")]
    pub severities: bool,
    /// Only report what would change.
    #[serde(default)]
    pub preview: bool,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct RulesImportResult {
    /// What the import changes, or with `preview` would change.
    #[serde(flatten)]
    pub changes: ImportPreview,
    /// Whether the board was changed.
    pub applied: bool,
}

impl Command for RulesImport {
    const META: Meta = Meta {
        id: "pcb.rules.import",
        title: "Import Design Rules",
        description: "Reads the chosen `kinds` from a Rules File as one undo step. Imported \
                      records replace matching scopes. Classes match by name, and missing \
                      classes are created empty. The preview lists the changed records, new \
                      classes, severity changes and missing copper positions or Via types. \
                      Position-dependent records start selected on a board with the same \
                      copper count. Pass `position_records` to select them explicitly. \
                      Set `severities` to false to keep the board's severities.",
        category: "pcb",
        icon: "rules",
        kind: Kind::Edit,
    };
    type Params = RulesImportParams;
    type Output = RulesImportResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: RulesImportParams,
    ) -> Result<RulesImportResult, CommandError> {
        if p.path.is_relative() {
            return Err(CommandError::invalid_params(
                "/path",
                "give the `.redarules` file as an absolute path",
            ));
        }
        let bytes = read_bytes(&p.path)?;
        let set = read_rules_file(&p.path, &bytes)?;
        let pcb = pcb(cx)?.clone();
        let plan = plan(
            &pcb,
            &set,
            p.kinds.as_deref(),
            p.severities,
            p.position_records.as_ref(),
            || cx.new_id(),
        )
        .map_err(CommandError::rejected)?;
        if p.preview {
            return Ok(RulesImportResult {
                changes: plan.preview,
                applied: false,
            });
        }
        let document = pcb.uuid;
        let tx = cx.tx()?;
        for class in plan.classes {
            tx.insert(Object::NetClass(InDocument::new(document, class)))?;
        }
        for (record, existed) in plan.records {
            let object = Object::DesignRule(InDocument::new(document, record));
            if existed {
                tx.replace(object)?;
            } else {
                tx.insert(object)?;
            }
        }
        if let Some(severities) = plan.severities {
            let setup = BoardSetup {
                severities,
                ..pcb.setup().clone()
            };
            tx.replace(Object::BoardSetup(InDocument::new(document, setup)))?;
        }
        Ok(RulesImportResult {
            changes: plan.preview,
            applied: true,
        })
    }
}

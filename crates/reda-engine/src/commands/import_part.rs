//! `library.import_part`: Import part. Reads one KiCad symbol and footprint pair through
//! `reda-import-kicad`, writes the Symbol, Footprint and Part into a registered Library,
//! then runs the Library check on them. It is one-shot: nothing is opened for editing and
//! nothing goes on an undo stack.

use std::path::{Path, PathBuf};

use reda_command::{Command, CommandError, Context, Kind, Meta, Registry};
use reda_import_kicad::{ImportOptions, Service, VENDOR_TERMS_NOTICE, import};
use reda_library::{ElementKind, Origin, check};
use reda_model::{LibraryElement, Provenance, Uuid, Violation};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::format::library::{write_footprint, write_part, write_symbol};
use crate::fs::write_atomic;
use crate::library::scan_registered;
use crate::state;

pub(crate) fn register_all(registry: &mut Registry) {
    registry.add(ImportPart);
}

pub struct ImportPart;

#[derive(Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct ImportPartParams {
    /// The registered Library to write the elements into. The starter Library is
    /// read-only.
    pub library: Uuid,
    /// The KiCad 6 or later symbol library (`.kicad_sym`, version 20210619 or later).
    pub symbol_file: PathBuf,
    /// The KiCad footprint (`.kicad_mod`), in the `(footprint …)` or legacy `(module …)`
    /// syntax.
    pub footprint_file: PathBuf,
    /// Which symbol to import when the `.kicad_sym` holds several.
    #[serde(default)]
    pub symbol: Option<String>,
    /// The Part's name; by default the symbol's.
    #[serde(default)]
    pub name: Option<String>,
    /// The service the files came from; recognised from their contents when left out.
    #[serde(default)]
    pub service: Option<Service>,
}

#[derive(Debug, Serialize, JsonSchema)]
pub struct ImportPartResult {
    pub symbol: Uuid,
    pub footprint: Uuid,
    pub part: Uuid,
    /// The files written, in the order above.
    pub files: Vec<PathBuf>,
    /// What every imported element records about where it came from.
    pub provenance: Provenance,
    /// The vendor-terms notice, included the first time this user imports a part (always,
    /// when REDA has no per-user folders to remember it in).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    /// What could not be carried over exactly, such as a custom pad shape.
    pub warnings: Vec<String>,
    /// The Library check on the imported Symbol, Footprint and Part.
    pub violations: Vec<Violation>,
}

fn read_input(path: &Path, what: &str) -> Result<String, CommandError> {
    let bytes = std::fs::read(path).map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => {
            CommandError::not_found(format!("the {what} file {} does not exist", path.display()))
        }
        _ => CommandError::internal(format!("could not read {}: {e}", path.display())),
    })?;
    String::from_utf8(bytes)
        .map_err(|_| CommandError::rejected(format!("{} is not UTF-8 text", path.display())))
}

impl Command for ImportPart {
    const META: Meta = Meta {
        id: "library.import_part",
        title: "Import Part",
        description: "Imports one KiCad 6+ symbol (`.kicad_sym`) and footprint (`.kicad_mod`), \
                      as vendor download services provide them, into a registered Library as \
                      a Symbol, a Footprint and a Part. Pins bind to Pads by number; MPN, \
                      manufacturer, datasheet and supplier properties map onto the Part's \
                      fields, and other properties become free-form fields. 3D models are \
                      ignored. Each element records the source service, its terms URL and \
                      any copyright lines. Existing files are never overwritten. The Library \
                      check runs on the result.",
        category: "library",
        icon: "library_import",
        kind: Kind::File,
    };
    type Params = ImportPartParams;
    type Output = ImportPartResult;

    fn run(
        &self,
        cx: &mut Context<'_>,
        p: ImportPartParams,
    ) -> Result<ImportPartResult, CommandError> {
        let symbol_text = read_input(&p.symbol_file, "symbol")?;
        let footprint_text = read_input(&p.footprint_file, "footprint")?;
        let options = ImportOptions {
            symbol: p.symbol,
            name: p.name,
            service: p.service,
        };
        let imported = import(&symbol_text, &footprint_text, &options, &mut || cx.new_id())
            .map_err(|e| CommandError::rejected(e.to_string()))?;

        let session = cx.session()?;
        let index = scan_registered(session);
        let library = index.library(p.library).ok_or_else(|| {
            CommandError::not_found(format!("no registered Library has UUID {}", p.library))
        })?;
        if library.origin == Origin::Starter {
            return Err(CommandError::rejected(
                "the starter Library is read-only; import into your own Library",
            ));
        }
        let path_of = |kind: ElementKind, name: &str| {
            library
                .root
                .join(kind.folder())
                .join(format!("{name}.{}", kind.extension()))
        };
        let encode = |r: Result<Vec<u8>, _>| {
            r.map_err(|e| CommandError::internal(format!("could not encode an element: {e}")))
        };
        let files = [
            (
                path_of(ElementKind::Symbol, &imported.symbol.name),
                encode(write_symbol(&imported.symbol))?,
            ),
            (
                path_of(ElementKind::Footprint, &imported.footprint.name),
                encode(write_footprint(&imported.footprint))?,
            ),
            (
                path_of(ElementKind::Part, &imported.part.name),
                encode(write_part(&imported.part))?,
            ),
        ];
        if let Some((path, _)) = files.iter().find(|(path, _)| path.exists()) {
            return Err(CommandError::rejected(format!(
                "{} already exists; nothing was imported",
                path.display()
            )));
        }
        for (path, bytes) in &files {
            let write = |path: &Path| -> std::io::Result<()> {
                if let Some(dir) = path.parent() {
                    std::fs::create_dir_all(dir)?;
                }
                write_atomic(path, bytes)
            };
            write(path).map_err(|e| {
                CommandError::internal(format!("could not write {}: {e}", path.display()))
            })?;
        }

        // Check what is now on disk, as the Library will be read from now on.
        let index = scan_registered(session);
        let severities = crate::preferences::library_severities(session);
        let violations = [
            LibraryElement::Symbol(imported.symbol.clone()),
            LibraryElement::Footprint(imported.footprint.clone()),
            LibraryElement::Part(imported.part.clone()),
        ]
        .iter()
        .flat_map(|e| check(&index, e, &severities))
        .collect();

        let notice = match &session.user_dirs {
            None => Some(VENDOR_TERMS_NOTICE.to_owned()),
            Some(dirs) if state::read(dirs).vendor_terms_notice_shown => None,
            Some(dirs) => {
                // Failing to remember only means the notice comes again next time.
                let _ = state::update(dirs, |s| s.vendor_terms_notice_shown = true);
                Some(VENDOR_TERMS_NOTICE.to_owned())
            }
        };
        Ok(ImportPartResult {
            symbol: imported.symbol.uuid,
            footprint: imported.footprint.uuid,
            part: imported.part.uuid,
            files: files.into_iter().map(|(path, _)| path).collect(),
            provenance: *imported.part.provenance.clone().expect("imports record it"),
            notice,
            warnings: imported.warnings,
            violations,
        })
    }
}

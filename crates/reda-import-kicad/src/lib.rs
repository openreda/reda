//! Import part: turns one KiCad 6+ symbol (`.kicad_sym`) and one footprint (`.kicad_mod`),
//! the pair vendor download services provide, into a REDA Symbol, Footprint and Part.
//!
//! [`import`] is pure: it takes the two files' text and returns the elements, so the
//! engine's `library.import_part` Command decides where they are written, and the fuzz
//! target can drive it with arbitrary bytes. Nothing here branches on a file's
//! `generator`: services write KiCad's own generator names, so it says nothing about
//! where a file came from.
//!
//! - Pins bind to Pads by number, so the Part has no pin-map overrides.
//! - Symbol properties map onto the built-in fields under the names services use for them
//!   ([`fields`]); the rest become free-form fields.
//! - Every element records its [`Provenance`]: the service, its terms URL and any copyright
//!   lines in the files.
//! - 3D models are ignored.
//!
//! [`import_net_classes`] separately maps a project file's net classes to rule records
//! (see `net_classes.rs`).
//!
//! KiCad symbol coordinates point up, like REDA's; footprint coordinates point down, so the
//! footprint's y is negated. Angles keep their sign, because a turn that looks
//! counter-clockwise on screen stays counter-clockwise when the axis flips with the view.

mod courtyard;
mod fields;
mod footprint;
mod net_classes;
mod node;
mod number;
mod provenance;
mod sexpr;
mod shapes;
mod symbol;

use std::fmt;

use reda_library::check_element_name;
use reda_model::{ComponentType, Footprint, Part, PinMap, Provenance, Symbol, Uuid};

pub use net_classes::{ImportedNetClass, ImportedRules, import_net_classes};
pub use provenance::{Service, VENDOR_TERMS_NOTICE};
pub use sexpr::MAX_DEPTH;

/// The oldest `.kicad_sym` format accepted: the first KiCad 6 symbol library format.
pub const MIN_SYMBOL_VERSION: u32 = 20210619;

/// Choices the caller makes about an import.
#[derive(Clone, Debug, Default)]
pub struct ImportOptions {
    /// Which symbol to take when the `.kicad_sym` holds several.
    pub symbol: Option<String>,
    /// The Part's name; by default the symbol's.
    pub name: Option<String>,
    /// The service the files came from, when the caller knows it; otherwise it is
    /// recognised from the files' contents.
    pub service: Option<Service>,
}

/// What an import made, not yet written anywhere.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Imported {
    pub symbol: Symbol,
    pub footprint: Footprint,
    /// References the Symbol and the Footprint.
    pub part: Part,
    /// Things in the files that REDA could not represent exactly, in file order.
    pub warnings: Vec<String>,
}

/// Which of the two input files an error is in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SourceFile {
    Symbol,
    Footprint,
    /// A project file, for its net classes ([`import_net_classes`]).
    Project,
}

/// Why the files could not be imported.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportError {
    pub file: SourceFile,
    /// The 1-based line and column, when the error has a place.
    pub line: Option<(u32, u32)>,
    pub message: String,
}

impl fmt::Display for ImportError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let file = match self.file {
            SourceFile::Symbol => "the symbol file",
            SourceFile::Footprint => "the footprint file",
            SourceFile::Project => "the project file",
        };
        match self.line {
            Some((line, column)) => {
                write!(f, "{file}, line {line}, column {column}: {}", self.message)
            }
            None => write!(f, "{file}: {}", self.message),
        }
    }
}

impl std::error::Error for ImportError {}

/// Imports the symbol in `symbol_lib` (the text of a `.kicad_sym`) and the footprint in
/// `footprint` (the text of a `.kicad_mod`). `new_id` hands out the elements' UUIDs.
pub fn import(
    symbol_lib: &str,
    footprint: &str,
    options: &ImportOptions,
    new_id: &mut dyn FnMut() -> Uuid,
) -> Result<Imported, ImportError> {
    let sym_root = sexpr::parse(symbol_lib).map_err(|e| e.into_import(SourceFile::Symbol))?;
    let fp_root = sexpr::parse(footprint).map_err(|e| e.into_import(SourceFile::Footprint))?;
    let source = symbol::read(&sym_root, options.symbol.as_deref())
        .map_err(|e| e.into_import(SourceFile::Symbol))?;
    let fp_source = footprint::read(&fp_root).map_err(|e| e.into_import(SourceFile::Footprint))?;

    let service = options
        .service
        .unwrap_or_else(|| provenance::recognise(&sym_root, &fp_root));
    let mapped = fields::map(&source.properties);
    let mut copyright = mapped.copyright.clone();
    for line in &fp_source.copyright {
        if !copyright.contains(line) {
            copyright.push(line.clone());
        }
    }
    let provenance = Provenance {
        service: service.name().into(),
        terms_url: service.terms_url().into(),
        copyright,
    };

    let mut warnings = source.warnings.clone();
    let symbol = symbol::build(&source, file_name(&source.name), provenance.clone(), new_id);
    let (in_bom, in_cpl) = (source.in_bom && fp_source.in_bom, fp_source.in_cpl);
    let footprint = footprint::build(
        fp_source.clone(),
        file_name(&fp_source.name),
        provenance.clone(),
        new_id,
    );
    warnings.extend(fp_source.warnings);

    let name = match &options.name {
        Some(name) => {
            check_element_name(name).map_err(|why| ImportError {
                file: SourceFile::Symbol,
                line: None,
                message: format!("the Part name {name:?} cannot be used: {why}"),
            })?;
            name.clone()
        }
        None => file_name(&source.name),
    };
    let part = Part {
        uuid: new_id(),
        name,
        // An imported Net Tie stays Standard until the user retypes it.
        component_type: ComponentType::Standard,
        in_bom: ComponentType::Standard.default_in_bom() && in_bom,
        in_cpl: ComponentType::Standard.default_in_cpl() && in_cpl,
        symbol: Some(symbol.uuid),
        footprints: vec![footprint.uuid],
        fields: mapped.fields,
        pin_map: PinMap::default(),
        provenance: Some(Box::new(provenance)),
    };
    Ok(Imported {
        symbol,
        footprint,
        part,
        warnings,
    })
}

/// `name` made usable as an element name, which is also a file stem: characters file
/// names cannot hold become `_`, and a trailing dot or space is dropped.
fn file_name(name: &str) -> String {
    const RESERVED: &[char] = &['/', '\\', '<', '>', ':', '"', '|', '?', '*'];
    let mut out: String = name
        .chars()
        .map(|c| {
            if RESERVED.contains(&c) || c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect();
    while out.ends_with(['.', ' ']) {
        out.pop();
    }
    if out.trim().is_empty() || out == "." || out == ".." {
        return "Imported".into();
    }
    out
}

impl sexpr::SexpError {
    fn into_import(self, file: SourceFile) -> ImportError {
        ImportError {
            file,
            line: Some((self.pos.line, self.pos.column)),
            message: self.message,
        }
    }
}

//! Symbol properties to Part fields. Services name the same data differently (`MP`,
//! `Manufacturer_Part_Number`, `MANUFACTURER_PART_NUMBER`…), so names are compared with case
//! and punctuation removed. Known MPN, manufacturer, datasheet, description and keyword
//! names fill the built-in fields; known supplier part numbers become free-form fields
//! under one name each (`LCSC`, `Mouser`, `DigiKey`…); everything else stays a free-form
//! field under its own name. KiCad's bookkeeping properties are dropped.

use reda_model::{Field, Fields};

use crate::provenance::is_copyright;

pub struct Mapped {
    pub fields: Fields,
    /// Copyright lines among the properties, which also stay as free-form fields.
    pub copyright: Vec<String>,
}

/// `name` lower-cased with everything but letters and digits removed.
fn key(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

enum Target {
    Drop,
    Value,
    Description,
    Datasheet,
    Manufacturer,
    Mpn,
    Keywords,
    Supplier(&'static str),
    Custom,
}

fn target(name: &str) -> Target {
    let k = key(name);
    match k.as_str() {
        "reference" | "footprint" | "kifpfilters" | "kilocked" => Target::Drop,
        "value" => Target::Value,
        "description" | "kidescription" => Target::Description,
        "datasheet" | "datasheeturl" => Target::Datasheet,
        "manufacturer" | "mf" | "mfr" | "mfg" | "manufacturername" | "mfrname" | "mfgname" => {
            Target::Manufacturer
        }
        "mpn"
        | "mp"
        | "manufacturerpartnumber"
        | "mfrpartnumber"
        | "mfgpartnumber"
        | "manufacturerpn"
        | "mfrpn"
        | "mfpn"
        | "partnumber" => Target::Mpn,
        "keywords" | "kikeywords" => Target::Keywords,
        "lcsc" | "lcscpart" | "lcscpartnumber" | "lcscpn" => Target::Supplier("LCSC"),
        "mouser" | "mouserpartnumber" | "mouserpn" | "mouserno" => Target::Supplier("Mouser"),
        "digikey" | "digikeypartnumber" | "digikeypn" | "digikeyno" => Target::Supplier("DigiKey"),
        "farnell" | "farnellpartnumber" | "farnellpn" => Target::Supplier("Farnell"),
        "newark" | "newarkpartnumber" | "newarkpn" => Target::Supplier("Newark"),
        "arrow" | "arrowpartnumber" | "arrowpn" => Target::Supplier("Arrow"),
        _ if k.starts_with("ki") && name.starts_with("ki_") => Target::Drop,
        _ => Target::Custom,
    }
}

pub fn map(properties: &[(String, String)]) -> Mapped {
    let mut fields = Fields::default();
    let mut copyright = vec![];
    let custom = |fields: &mut Fields, name: &str, value: &str| {
        let taken = fields.custom.iter().any(|f| f.name == name);
        if !taken {
            fields.custom.push(Field {
                name: name.into(),
                value: value.into(),
            });
        }
    };
    for (name, value) in properties {
        let value = value.trim();
        if value.is_empty() || value == "~" {
            continue;
        }
        if is_copyright(name) || is_copyright(value) {
            if !copyright.iter().any(|c: &String| c == value) {
                copyright.push(value.to_owned());
            }
            custom(&mut fields, name, value);
            continue;
        }
        let slot = match target(name) {
            Target::Drop => continue,
            Target::Value => &mut fields.value,
            Target::Description => &mut fields.description,
            Target::Datasheet => &mut fields.datasheet,
            Target::Manufacturer => &mut fields.manufacturer,
            Target::Mpn => &mut fields.mpn,
            Target::Keywords => &mut fields.keywords,
            Target::Supplier(canonical) => {
                custom(&mut fields, canonical, value);
                continue;
            }
            Target::Custom => {
                custom(&mut fields, name, value);
                continue;
            }
        };
        if slot.is_empty() {
            *slot = value.to_owned();
        } else if slot != value {
            // A second name for a built-in field that disagrees: keep it, don't lose it.
            custom(&mut fields, name, value);
        }
    }
    Mapped { fields, copyright }
}

//! Read the subset of the published Gerber job format REDA uses. These types describe
//! the output independently of the writer.
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct Job {
    pub general_specs: GeneralSpecs,
    pub material_stackup: Vec<Material>,
    pub files_attributes: Vec<File>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct GeneralSpecs {
    pub layer_number: usize,
    /// Set when any board edge is plated.
    #[allow(dead_code, reason = "used by the plated-edge reference suite")]
    pub edge_plating: Option<bool>,
    /// Set when any Pad is castellated.
    #[allow(dead_code, reason = "used by the plated-edge reference suite")]
    pub castellated: Option<bool>,
    /// Set when any Pad is an Edge Connector Pad.
    #[allow(dead_code, reason = "used by the plated-edge reference suite")]
    pub edge_connector: Option<bool>,
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
pub struct File {
    pub path: String,
    pub file_function: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase", deny_unknown_fields)]
pub struct Material {
    #[serde(rename = "Type")]
    pub kind: String,
    pub name: Option<String>,
    pub thickness: Option<f64>,
    pub material: Option<String>,
    pub dielectric_constant: Option<f64>,
    pub loss_tangent: Option<f64>,
    pub color: Option<String>,
}

pub fn read(bytes: &[u8]) -> Job {
    serde_json::from_slice(bytes).expect("a Gerber job with ordered files and its material stack")
}

/// Check each physical row against the source board, including dielectric loss and finish colors.
#[allow(dead_code, reason = "used by the production reference suites")]
pub fn check_materials(pcb: &reda_model::PcbDocument, job: &Job) {
    use reda_model::StackupEntry;
    assert_eq!(job.general_specs.layer_number, pcb.copper_layers().count());
    let stack = pcb.stackup();
    let rows = &job.material_stackup;
    assert_eq!(rows.len(), stack.entries.len() + 4);
    for (i, kind, color) in [
        (0, "Legend", &stack.silk_color),
        (1, "SolderMask", &stack.mask_color),
        (rows.len() - 2, "SolderMask", &stack.mask_color),
        (rows.len() - 1, "Legend", &stack.silk_color),
    ] {
        assert_eq!((&*rows[i].kind, &rows[i].color), (kind, color));
    }
    for (row, sheet) in rows[2..].iter().zip(&stack.entries) {
        match sheet {
            StackupEntry::Copper(c) => {
                let layer = pcb.layers().layer(c.layer).unwrap();
                let pos = pcb.copper_number(c.layer).unwrap();
                let name = layer.name.clone().unwrap_or_else(|| format!("L{pos}"));
                assert_eq!((&*row.kind, row.name.as_ref()), ("Copper", Some(&name)));
                assert_eq!(row.thickness, Some(c.thickness.0 as f64 / 1e6));
            }
            StackupEntry::Dielectric(d) => {
                assert_eq!(row.kind, "Dielectric");
                assert_eq!(row.name, d.name);
                assert_eq!(row.material, d.material);
                assert_eq!(row.thickness, d.thickness.map(|v| v.0 as f64 / 1e6));
                assert_eq!(row.dielectric_constant, d.dk.map(|v| f64::from(v) / 1000.));
                assert_eq!(row.loss_tangent, d.df.map(|v| f64::from(v) / 1e6));
            }
        }
    }
}

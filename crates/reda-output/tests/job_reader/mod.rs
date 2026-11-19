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

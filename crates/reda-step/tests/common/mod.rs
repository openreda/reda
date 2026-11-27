//! Helpers the mesher's tests share: reading fixtures and checking meshes against the volumes
//! and boxes each fixture was drawn with (tests/fixtures/README.md).

#![allow(dead_code)]

use reda_step::{Loaded, Mesh, MeshOptions, Solid, load};

pub fn fixture_path(name: &str) -> String {
    format!("{}/tests/fixtures/{name}", env!("CARGO_MANIFEST_DIR"))
}

/// A fixture loaded with the default tolerances, which must give no warnings.
pub fn fixture(name: &str) -> Loaded {
    fixture_with(name, &MeshOptions::default())
}

pub fn fixture_with(name: &str, options: &MeshOptions) -> Loaded {
    let loaded = load_fixture(name, options);
    assert!(loaded.warnings.is_empty(), "{name}: {:?}", loaded.warnings);
    loaded
}

/// A fixture loaded whatever it warns.
pub fn load_fixture(name: &str, options: &MeshOptions) -> Loaded {
    let path = fixture_path(name);
    let bytes = std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    load(&bytes, options).unwrap_or_else(|e| panic!("{path}: {e}"))
}

pub fn fixture_text(name: &str) -> String {
    std::fs::read_to_string(fixture_path(name)).unwrap()
}

pub fn assert_close(what: &str, got: f64, want: f64, relative: f64) {
    assert!(
        (got - want).abs() <= relative * want.abs(),
        "{what}: {got} is not within {}% of {want}",
        relative * 100.0
    );
}

/// The tolerances that close in on a fixture's exact volume.
pub const FINE: MeshOptions = MeshOptions {
    chord: 0.0005,
    angle: 2.0 * std::f64::consts::PI / 180.0,
};

/// The one solid of a single-part fixture meshes closed at the expected volume.
pub fn assert_solid(name: &str, volume: f64) {
    let loaded = fixture(name);
    let meshed = &loaded.meshed;
    let solids: Vec<&Solid> = meshed.parts.iter().flat_map(|p| &p.solids).collect();
    assert_eq!(solids.len(), 1, "{name}: one solid");
    let mesh = &solids[0].mesh;
    assert!(!mesh.triangles.is_empty(), "{name}: triangles");
    assert!(mesh.is_closed(), "{name}: the mesh is closed");
    assert_normals_agree(name, mesh);
    // The default tolerances keep the volume within the corpus grade of 1 %.
    assert_close(name, mesh.volume(), volume, 0.01);
    assert_close(name, meshed.volume(), volume, 0.01);

    // Finer tolerances close in on the exact volume.
    let fine = fixture_with(name, &FINE);
    assert_close(name, fine.meshed.volume(), volume, 0.001);
    let mesh = &fine.meshed.parts[0].solids[0].mesh;
    assert!(mesh.is_closed(), "{name}: the fine mesh is closed");
}

/// Each triangle's winding and its vertices' normals point the same way (outwards, since the
/// volume comes out positive).
pub fn assert_normals_agree(name: &str, mesh: &Mesh) {
    for t in &mesh.triangles {
        let [a, b, c] = t.map(|i| mesh.positions[i as usize]);
        let (u, v) = (sub(b, a), sub(c, a));
        let n = [
            u[1] * v[2] - u[2] * v[1],
            u[2] * v[0] - u[0] * v[2],
            u[0] * v[1] - u[1] * v[0],
        ];
        for &i in t {
            let vn = mesh.normals[i as usize];
            let d = n[0] * vn[0] + n[1] * vn[1] + n[2] * vn[2];
            assert!(
                d >= 0.0,
                "{name}: a vertex normal faces against its triangle"
            );
        }
    }
}

pub fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

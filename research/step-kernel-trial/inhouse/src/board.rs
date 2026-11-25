//! The in-house STEP writer: an entity emitter, the shared contexts, and the PCB prism written
//! directly as a boundary representation of planes and cylinders.

/// Collects `#id = body;` records with ascending ids.
pub struct Emitter {
    pub records: Vec<(u64, Vec<u8>)>,
    next: u64,
}

impl Emitter {
    pub fn new() -> Self {
        Emitter {
            records: Vec::new(),
            next: 1,
        }
    }
    pub fn add(&mut self, body: String) -> u64 {
        let id = self.next;
        self.next += 1;
        self.records.push((id, body.into_bytes()));
        id
    }
    /// Stores a record under an id chosen by the caller and keeps later ids above it.
    pub fn add_raw(&mut self, id: u64, body: Vec<u8>) {
        self.next = self.next.max(id + 1);
        self.records.push((id, body));
    }
    pub fn last_id(&self) -> u64 {
        self.next - 1
    }
    pub fn point(&mut self, p: [f64; 3]) -> u64 {
        self.add(format!(
            "CARTESIAN_POINT('',({},{},{}))",
            num(p[0]),
            num(p[1]),
            num(p[2])
        ))
    }
    pub fn direction(&mut self, d: [f64; 3]) -> u64 {
        self.add(format!(
            "DIRECTION('',({},{},{}))",
            num(d[0]),
            num(d[1]),
            num(d[2])
        ))
    }
    pub fn placement(
        &mut self,
        name: &str,
        origin: [f64; 3],
        axis: [f64; 3],
        ref_dir: [f64; 3],
    ) -> u64 {
        let p = self.point(origin);
        let a = self.direction(axis);
        let r = self.direction(ref_dir);
        self.add(format!("AXIS2_PLACEMENT_3D('{name}',#{p},#{a},#{r})"))
    }
}

/// A Part 21 REAL: always carries a decimal point, never an exponent or a negative zero.
pub fn num(v: f64) -> String {
    let v = if v.abs() < 5e-13 { 0.0 } else { v };
    let s = format!("{v:.12}");
    s.trim_end_matches('0').to_string()
}

pub fn ref_list(ids: &[u64]) -> String {
    ids.iter()
        .map(|i| format!("#{i}"))
        .collect::<Vec<_>>()
        .join(",")
}

pub const AP214_SCHEMA: &str = "AUTOMOTIVE_DESIGN { 1 0 10303 214 1 1 1 1 }";
pub const AP242_SCHEMA: &str =
    "AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF { 1 0 10303 442 1 1 4 }";

/// The contexts every product written by this tool shares.
pub struct Contexts {
    pub application: u64,
    pub protocol: u64,
    pub product: u64,
    pub definition: u64,
    /// Geometric context: 3D, millimetres, radians, 1e-6 mm uncertainty.
    pub geometric: u64,
}

pub fn write_contexts(e: &mut Emitter, ap242: bool) -> Contexts {
    let (ctx_name, ap_name, year) = if ap242 {
        (
            "managed model based 3d engineering",
            "ap242_managed_model_based_3d_engineering",
            2014,
        )
    } else {
        (
            "core data for automotive mechanical design processes",
            "automotive_design",
            2000,
        )
    };
    let application = e.add(format!("APPLICATION_CONTEXT('{ctx_name}')"));
    let protocol =
        e.add(format!("APPLICATION_PROTOCOL_DEFINITION('international standard','{ap_name}',{year},#{application})"));
    let product = e.add(format!("PRODUCT_CONTEXT('',#{application},'mechanical')"));
    let definition = e.add(format!(
        "PRODUCT_DEFINITION_CONTEXT('part definition',#{application},'design')"
    ));
    let mm = e.add("(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))".into());
    let rad = e.add("(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.))".into());
    let sr = e.add("(NAMED_UNIT(*)SI_UNIT($,.STERADIAN.)SOLID_ANGLE_UNIT())".into());
    let unc = e.add(format!(
        "UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-06),#{mm},'distance_accuracy_value','confusion accuracy')"
    ));
    let geometric = e.add(format!(
        "(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{unc}))\
         GLOBAL_UNIT_ASSIGNED_CONTEXT((#{mm},#{rad},#{sr}))REPRESENTATION_CONTEXT('trial-board','3D'))"
    ));
    Contexts {
        application,
        protocol,
        product,
        definition,
        geometric,
    }
}

/// What an assembly needs to know about a product it instances.
pub struct ProductIds {
    pub product: u64,
    pub definition: u64,
    pub representation: u64,
    /// An identity AXIS2_PLACEMENT_3D that is an item of `representation`.
    pub origin: u64,
}

/// PRODUCT, PRODUCT_DEFINITION_FORMATION, PRODUCT_DEFINITION and PRODUCT_DEFINITION_SHAPE.
/// Returns `(product, definition, definition shape)`.
pub fn write_product(e: &mut Emitter, c: &Contexts, name: &str) -> (u64, u64, u64) {
    let product = e.add(format!("PRODUCT('{name}','{name}','',(#{}))", c.product));
    let formation = e.add(format!("PRODUCT_DEFINITION_FORMATION('','',#{product})"));
    let definition = e.add(format!(
        "PRODUCT_DEFINITION('design','',#{formation},#{})",
        c.definition
    ));
    let shape = e.add(format!("PRODUCT_DEFINITION_SHAPE('','',#{definition})"));
    (product, definition, shape)
}

pub const THICKNESS: f64 = 1.6;

/// One piece of a 2D boundary loop, given in counter-clockwise order.
#[derive(Clone, Copy)]
enum Seg {
    Line {
        a: [f64; 2],
        b: [f64; 2],
    },
    /// Counter-clockwise from angle `a0` (degrees) to the start of the next segment; a loop of
    /// one arc is a full circle.
    Arc {
        c: [f64; 2],
        r: f64,
        a0: f64,
    },
}

/// Cosine and sine that are exact at multiples of 90 degrees.
pub fn cos_sin(deg: f64) -> (f64, f64) {
    let d = deg.rem_euclid(360.0);
    if d == 0.0 {
        (1.0, 0.0)
    } else if d == 90.0 {
        (0.0, 1.0)
    } else if d == 180.0 {
        (-1.0, 0.0)
    } else if d == 270.0 {
        (0.0, -1.0)
    } else {
        (d.to_radians().cos(), d.to_radians().sin())
    }
}

impl Seg {
    fn start(&self) -> [f64; 2] {
        match *self {
            Seg::Line { a, .. } => a,
            Seg::Arc { c, r, a0, .. } => {
                let (x, y) = cos_sin(a0);
                [c[0] + r * x, c[1] + r * y]
            }
        }
    }
}

struct Loop2 {
    segs: Vec<Seg>,
    hole: bool,
}

/// The board of the trial contract: 50 x 30 mm, 3 mm corners, four 3.2 mm holes 4 mm in from
/// the corners, and a 2 x 8 mm slot along X centred at (25, 15).
fn board_loops() -> Vec<Loop2> {
    let (w, h, r) = (50.0, 30.0, 3.0);
    let line = |a: [f64; 2], b: [f64; 2]| Seg::Line { a, b };
    let arc = |c: [f64; 2], r: f64, a0: f64| Seg::Arc { c, r, a0 };
    let mut loops = vec![Loop2 {
        hole: false,
        segs: vec![
            line([r, 0.0], [w - r, 0.0]),
            arc([w - r, r], r, 270.0),
            line([w, r], [w, h - r]),
            arc([w - r, h - r], r, 0.0),
            line([w - r, h], [r, h]),
            arc([r, h - r], r, 90.0),
            line([0.0, h - r], [0.0, r]),
            arc([r, r], r, 180.0),
        ],
    }];
    for c in [
        [4.0, 4.0],
        [w - 4.0, 4.0],
        [w - 4.0, h - 4.0],
        [4.0, h - 4.0],
    ] {
        loops.push(Loop2 {
            hole: true,
            segs: vec![arc(c, 1.6, 0.0)],
        });
    }
    loops.push(Loop2 {
        hole: true,
        segs: vec![
            line([22.0, 14.0], [28.0, 14.0]),
            arc([28.0, 15.0], 1.0, 270.0),
            line([28.0, 16.0], [22.0, 16.0]),
            arc([22.0, 15.0], 1.0, 90.0),
        ],
    });
    loops
}

const Z: [f64; 3] = [0.0, 0.0, 1.0];
const X: [f64; 3] = [1.0, 0.0, 0.0];

fn oriented(e: &mut Emitter, edge: u64, forward: bool) -> u64 {
    e.add(format!(
        "ORIENTED_EDGE('',*,*,#{edge},.{}.)",
        if forward { 'T' } else { 'F' }
    ))
}

fn edge_loop(e: &mut Emitter, edges: &[(u64, bool)]) -> u64 {
    let ids: Vec<u64> = edges
        .iter()
        .map(|&(id, fwd)| oriented(e, id, fwd))
        .collect();
    e.add(format!("EDGE_LOOP('',({}))", ref_list(&ids)))
}

/// Writes the PCB product: its B-rep solid, product structure and colour. Every loop is
/// extruded the same way: each 2D vertex gives a bottom vertex, a top vertex and a vertical
/// edge; each 2D segment gives a bottom edge, a top edge and one wall face. A full circle is a
/// single closed edge whose wall is one cylindrical face with a seam.
pub fn write_board(e: &mut Emitter, c: &Contexts) -> ProductIds {
    let loops = board_loops();
    let mut faces = Vec::new();
    let mut top_bounds = Vec::new();
    let mut bottom_bounds = Vec::new();

    for lp in &loops {
        let n = lp.segs.len();
        let mut verts = Vec::new(); // (bottom vertex, top vertex, vertical edge)
        for s in &lp.segs {
            let p = s.start();
            let pb = e.point([p[0], p[1], 0.0]);
            let vb = e.add(format!("VERTEX_POINT('',#{pb})"));
            let pt = e.point([p[0], p[1], THICKNESS]);
            let vt = e.add(format!("VERTEX_POINT('',#{pt})"));
            let d = e.direction(Z);
            let v = e.add(format!("VECTOR('',#{d},1.)"));
            let l = e.add(format!("LINE('',#{pb},#{v})"));
            let edge = e.add(format!("EDGE_CURVE('',#{vb},#{vt},#{l},.T.)"));
            verts.push((vb, vt, edge));
        }
        // Bottom and top edge of each segment, both running counter-clockwise seen from +Z.
        let mut edges = Vec::new();
        for (i, s) in lp.segs.iter().enumerate() {
            let j = (i + 1) % n;
            let mut level = |z: f64, v0: u64, v1: u64| -> u64 {
                let curve = match *s {
                    Seg::Line { a, b } => {
                        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                        let p = e.point([a[0], a[1], z]);
                        let d = e.direction([(b[0] - a[0]) / len, (b[1] - a[1]) / len, 0.0]);
                        let v = e.add(format!("VECTOR('',#{d},1.)"));
                        e.add(format!("LINE('',#{p},#{v})"))
                    }
                    Seg::Arc { c, r, .. } => {
                        let ax = e.placement("", [c[0], c[1], z], Z, X);
                        e.add(format!("CIRCLE('',#{ax},{})", num(r)))
                    }
                };
                e.add(format!("EDGE_CURVE('',#{v0},#{v1},#{curve},.T.)"))
            };
            let bottom = level(0.0, verts[i].0, verts[j].0);
            let top = level(THICKNESS, verts[i].1, verts[j].1);
            edges.push((bottom, top));
        }
        // Wall faces. Seen from outside the material the wall loop runs: along the bottom,
        // up, back along the top, down. For the outline "along" is counter-clockwise; for a
        // hole it is clockwise, so the stored (counter-clockwise) edges are used reversed.
        for (i, s) in lp.segs.iter().enumerate() {
            let j = (i + 1) % n;
            let (bottom, top) = edges[i];
            let wall = if lp.hole {
                [
                    (bottom, false),
                    (verts[i].2, true),
                    (top, true),
                    (verts[j].2, false),
                ]
            } else {
                [
                    (bottom, true),
                    (verts[j].2, true),
                    (top, false),
                    (verts[i].2, false),
                ]
            };
            let el = edge_loop(e, &wall);
            let bound = e.add(format!("FACE_OUTER_BOUND('',#{el},.T.)"));
            let (surface, same_sense) = match *s {
                Seg::Line { a, b } => {
                    let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
                    let (dx, dy) = ((b[0] - a[0]) / len, (b[1] - a[1]) / len);
                    // Normal pointing out of the material: right of the travel direction for
                    // the outline, left of it for a hole.
                    let nrm = if lp.hole {
                        [-dy, dx, 0.0]
                    } else {
                        [dy, -dx, 0.0]
                    };
                    let ax = e.placement("", [a[0], a[1], 0.0], nrm, Z);
                    (e.add(format!("PLANE('',#{ax})")), true)
                }
                Seg::Arc { c, r, .. } => {
                    let ax = e.placement("", [c[0], c[1], 0.0], Z, X);
                    // A cylinder's own normal points away from its axis: out of the material
                    // on an outline corner, into it on a hole wall.
                    (
                        e.add(format!("CYLINDRICAL_SURFACE('',#{ax},{})", num(r))),
                        !lp.hole,
                    )
                }
            };
            faces.push(e.add(format!(
                "ADVANCED_FACE('',(#{bound}),#{surface},.{}.)",
                if same_sense { 'T' } else { 'F' }
            )));
        }
        // Bounds of the top face (normal +Z) and the bottom face (normal -Z).
        let fwd_top: Vec<(u64, bool)> = edges.iter().map(|&(_, t)| (t, true)).collect();
        let rev_top: Vec<(u64, bool)> = edges.iter().rev().map(|&(_, t)| (t, false)).collect();
        let fwd_bottom: Vec<(u64, bool)> = edges.iter().map(|&(b, _)| (b, true)).collect();
        let rev_bottom: Vec<(u64, bool)> = edges.iter().rev().map(|&(b, _)| (b, false)).collect();
        let kind = if lp.hole {
            "FACE_BOUND"
        } else {
            "FACE_OUTER_BOUND"
        };
        let lt = edge_loop(e, if lp.hole { &rev_top } else { &fwd_top });
        top_bounds.push(e.add(format!("{kind}('',#{lt},.T.)")));
        let lb = edge_loop(e, if lp.hole { &fwd_bottom } else { &rev_bottom });
        bottom_bounds.push(e.add(format!("{kind}('',#{lb},.T.)")));
    }

    let top_ax = e.placement("", [0.0, 0.0, THICKNESS], Z, X);
    let top_plane = e.add(format!("PLANE('',#{top_ax})"));
    faces.push(e.add(format!(
        "ADVANCED_FACE('',({}),#{top_plane},.T.)",
        ref_list(&top_bounds)
    )));
    let bottom_ax = e.placement("", [0.0, 0.0, 0.0], [0.0, 0.0, -1.0], X);
    let bottom_plane = e.add(format!("PLANE('',#{bottom_ax})"));
    faces.push(e.add(format!(
        "ADVANCED_FACE('',({}),#{bottom_plane},.T.)",
        ref_list(&bottom_bounds)
    )));

    let shell = e.add(format!("CLOSED_SHELL('',({}))", ref_list(&faces)));
    let solid = e.add(format!("MANIFOLD_SOLID_BREP('PCB',#{shell})"));

    let (product, definition, shape) = write_product(e, c, "PCB");
    let origin = e.placement("", [0.0; 3], Z, X);
    let representation = e.add(format!(
        "ADVANCED_BREP_SHAPE_REPRESENTATION('PCB',(#{origin},#{solid}),#{})",
        c.geometric
    ));
    e.add(format!(
        "SHAPE_DEFINITION_REPRESENTATION(#{shape},#{representation})"
    ));

    // Colour: one style on the solid, green (0, 0.5, 0.2).
    let rgb = e.add(format!(
        "COLOUR_RGB('',{},{},{})",
        num(0.0),
        num(0.5),
        num(0.2)
    ));
    let fill_colour = e.add(format!("FILL_AREA_STYLE_COLOUR('',#{rgb})"));
    let fill = e.add(format!("FILL_AREA_STYLE('',(#{fill_colour}))"));
    let fill_area = e.add(format!("SURFACE_STYLE_FILL_AREA(#{fill})"));
    let side = e.add(format!("SURFACE_SIDE_STYLE('',(#{fill_area}))"));
    let usage = e.add(format!("SURFACE_STYLE_USAGE(.BOTH.,#{side})"));
    let assignment = e.add(format!("PRESENTATION_STYLE_ASSIGNMENT((#{usage}))"));
    let styled = e.add(format!("STYLED_ITEM('color',(#{assignment}),#{solid})"));
    e.add(format!(
        "MECHANICAL_DESIGN_GEOMETRIC_PRESENTATION_REPRESENTATION('',(#{styled}),#{})",
        c.geometric
    ));

    ProductIds {
        product,
        definition,
        representation,
        origin,
    }
}

/// Serialises the header and the collected records.
pub fn finish(e: &Emitter, schema: &str, description: &str) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(
        format!(
            "ISO-10303-21;\nHEADER;\nFILE_DESCRIPTION(('{description}'),'2;1');\n\
             FILE_NAME('trial-board.step','1970-01-01T00:00:00',(''),(''),\
             'REDA step-kernel-trial inhouse writer','REDA','');\n\
             FILE_SCHEMA(('{schema}'));\nENDSEC;\nDATA;\n"
        )
        .as_bytes(),
    );
    for (id, body) in &e.records {
        out.extend_from_slice(format!("#{id}=").as_bytes());
        out.extend_from_slice(body);
        out.extend_from_slice(b";\n");
    }
    out.extend_from_slice(b"ENDSEC;\nEND-ISO-10303-21;\n");
    out
}

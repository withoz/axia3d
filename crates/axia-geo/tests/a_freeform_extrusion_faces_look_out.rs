//! A FREEFORM EXTRUSION'S FACES LOOK OUT OF THE SOLID.
//!
//! The closed Bezier / BSpline / NURBS extrude (ADR-192) mirrors the Path B
//! cylinder builder, caps included — so the cap on the back of the extrusion
//! looked in. Its side had two more ways to face the wrong way:
//!
//! - the swept surface is `profile + v·dist·n`, so for `dist < 0` its own normal
//!   `∂u × ∂v` points in;
//! - `n` comes from the profile's first control-point triangle, not from the way
//!   the rim winds. A curve whose second control point is a notch winds one way
//!   about its plane and reports the other.
//!
//! Measured 2026-09-16 — volume over rim area × h (h = 300, the rim area from a
//! 1e-4 mm tessellation):
//!
//! ```text
//!                         base z = 0   z = 100   z = −150
//!   bezier    +300          0.9915     1.2134     0.6588
//!   bezier    −300         −0.9915    −0.7697    −1.3243
//!   nurbs     +300          0.9912     1.2131     0.6584
//!   nurbs     −300         −0.9912    −0.7693    −1.3241
//!   notched   +300         −0.3276    −0.5495     0.0052
//!   notched   −300          0.3276     0.1057     0.6605
//! ```
//!
//! BSpline read the same as Bezier to four places.

use axia_geo::curves::{bezier, bspline, nurbs, AnalyticCurve};
use axia_geo::mesh::Mesh;
use axia_geo::operations::create_solid::{CreateSolidMode, CreateSolidResult};
use axia_geo::surfaces::AnalyticSurface;
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

const H: f64 = 300.0;
const KNOTS: [f64; 10] = [0.0, 0.0, 0.0, 0.0, 1.0 / 3.0, 2.0 / 3.0, 1.0, 1.0, 1.0, 1.0];
const WEIGHTS: [f64; 6] = [1.0, 0.8, 1.2, 1.0, 0.9, 1.0];

fn mat() -> MaterialId {
    MaterialId::new(0)
}

#[derive(Clone, Copy)]
enum Kind {
    Bezier,
    BSpline,
    Nurbs,
    Notched,
}

const KINDS: [(&str, Kind); 4] = [
    ("bezier", Kind::Bezier),
    ("bspline", Kind::BSpline),
    ("nurbs", Kind::Nurbs),
    ("notched bezier", Kind::Notched),
];

fn ctrl(kind: Kind, z: f64) -> Vec<DVec3> {
    let xy: [(f64, f64); 6] = match kind {
        // The second control point is a notch: the rim winds CCW about +Z while
        // the first control triangle turns right, so the face reports −Z.
        Kind::Notched => [(40., 40.), (0., 10.), (-40., 40.), (-40., -40.), (40., -40.), (40., 40.)],
        _ => [(40., 0.), (40., 60.), (-40., 60.), (-40., -60.), (40., -60.), (40., 0.)],
    };
    xy.iter().map(|&(x, y)| DVec3::new(x, y, z)).collect()
}

fn curve(kind: Kind, z: f64) -> AnalyticCurve {
    let cp = ctrl(kind, z);
    match kind {
        Kind::Bezier | Kind::Notched => AnalyticCurve::Bezier { control_pts: cp },
        Kind::BSpline => AnalyticCurve::BSpline { control_pts: cp, knots: KNOTS.to_vec(), degree: 3 },
        Kind::Nurbs => AnalyticCurve::NURBS {
            control_pts: cp,
            weights: WEIGHTS.to_vec(),
            knots: KNOTS.to_vec(),
            degree: 3,
        },
    }
}

/// The area the rim encloses, read off a fine tessellation of the curve itself.
fn rim_area(kind: Kind) -> f64 {
    let cp = ctrl(kind, 0.0);
    let pts = match kind {
        Kind::Bezier | Kind::Notched => bezier::tessellate(&cp, 1e-4),
        Kind::BSpline => bspline::tessellate(&cp, &KNOTS, 3, 1e-4),
        Kind::Nurbs => nurbs::tessellate(&cp, &WEIGHTS, &KNOTS, 3, 1e-4),
    }
    .expect("tessellate");
    let mut twice = 0.0;
    for i in 0..pts.len() {
        let (p, q) = (pts[i], pts[(i + 1) % pts.len()]);
        twice += p.x * q.y - q.x * p.y;
    }
    twice.abs() / 2.0
}

fn swept(kind: Kind, z: f64, dist: f64) -> (Mesh, CreateSolidResult) {
    let mut m = Mesh::new();
    m.set_cylinder_path_b_default(true);
    let anchor = m.add_vertex(ctrl(kind, z)[0]);
    let f = m
        .add_face_closed_curve(anchor, curve(kind, z), mat())
        .expect("closed curve");
    let r = m
        .create_solid(f, CreateSolidMode::Extrude { distance: dist }, mat())
        .expect("sweep");
    (m, r)
}

/// Where a cap stands: the height of its rim anchor.
fn cap_height(m: &Mesh, cap: FaceId) -> f64 {
    let anchor = m.collect_loop_verts(m.faces[cap].outer().start).expect("rim")[0];
    m.vertex_pos(anchor).expect("anchor").z
}

#[test]
fn a_freeform_extrusion_measures_its_rim_area_times_height_wherever_it_stands() {
    let mut wrong = Vec::new();
    for (label, kind) in KINDS {
        let truth = rim_area(kind) * H;
        for dist in [H, -H] {
            let v0 = swept(kind, 0.0, dist).0.mesh_volume();
            for z in [0.0, 100.0, -150.0] {
                let v = swept(kind, z, dist).0.mesh_volume();
                // The caps and the side are read through tessellation, which lands a
                // little under the fine rim here; where the solid stands must not matter.
                if (v / truth - 1.0).abs() > 0.02 || (v / v0 - 1.0).abs() > 2e-3 {
                    wrong.push(format!(
                        "{label} {dist:+}, base z = {z}: {:.4} of the truth, {:.4} of z = 0",
                        v / truth,
                        v / v0
                    ));
                }
            }
        }
    }
    assert!(wrong.is_empty(), "rim area × h; these read otherwise:\n  {}", wrong.join("\n  "));
}

#[test]
fn each_freeform_cap_looks_away_from_the_solid() {
    for (label, kind) in KINDS {
        for dist in [H, -H] {
            let (m, r) = swept(kind, 0.0, dist);
            let (bottom, top) = if cap_height(&m, r.profile_face) < cap_height(&m, r.top_face) {
                (r.profile_face, r.top_face)
            } else {
                (r.top_face, r.profile_face)
            };
            for (which, cap, out) in [("bottom", bottom, DVec3::NEG_Z), ("top", top, DVec3::Z)] {
                let looks = m.faces[cap].normal();
                assert!(
                    looks.dot(out) > 0.999,
                    "{label} {dist:+}: the {which} cap looks {looks:?}, it should look {out:?}"
                );
                match m.face_surface(cap) {
                    Some(AnalyticSurface::Plane { normal, .. }) => assert!(
                        normal.dot(out) > 0.999,
                        "{label} {dist:+}: the {which} cap looks {looks:?} but its Plane says {normal:?}"
                    ),
                    other => panic!("{label} {dist:+}: the {which} cap carries {other:?}, not a Plane"),
                }
            }
        }
    }
}

/// Caps by their height, the side by the rest; every profile here surrounds the
/// origin, so the side looks out where its normal has a positive radial part.
#[test]
fn the_renderer_draws_every_freeform_face_facing_out() {
    for (label, kind) in KINDS {
        for dist in [H, -H] {
            let (mut m, r) = swept(kind, 0.0, dist);
            let (za, zb) = (cap_height(&m, r.profile_face), cap_height(&m, r.top_face));
            let (z_lo, z_hi) = (za.min(zb), za.max(zb));

            let (pos, nrm, tris, _, _) = m.export_buffers().expect("export");
            let p = |i: u32| {
                let i = i as usize * 3;
                DVec3::new(pos[i] as f64, pos[i + 1] as f64, pos[i + 2] as f64)
            };
            let n = |i: u32| {
                let i = i as usize * 3;
                DVec3::new(nrm[i] as f64, nrm[i + 1] as f64, nrm[i + 2] as f64)
            };

            let mut counts = [("bottom", 0, 0, 0), ("top", 0, 0, 0), ("side", 0, 0, 0)];
            for t in tris.chunks(3) {
                let on = |z: f64| t.iter().all(|&i| (p(i).z - z).abs() < 1e-3);
                let (slot, out) = if on(z_lo) {
                    (0, DVec3::NEG_Z)
                } else if on(z_hi) {
                    (1, DVec3::Z)
                } else {
                    let c = (p(t[0]) + p(t[1]) + p(t[2])) / 3.0;
                    (2, DVec3::new(c.x, c.y, 0.0).normalize_or_zero())
                };
                counts[slot].1 += 1;
                if (p(t[1]) - p(t[0])).cross(p(t[2]) - p(t[0])).dot(out) <= 0.0 {
                    counts[slot].2 += 1;
                }
                // The side's vertex normals are not this test's question. Its
                // surface is degree 1 along the extrusion, and `derivative_v`
                // comes back (0, 0, 0) there at v = 0, 0.5 and 1: the degree-0
                // curve `bspline::derivative` builds is refused ("degree must be
                // ≥ 1, got 0") and `derivative_v` turns that error into zero. The
                // renderer then falls back to the face's stored normal — the
                // profile's — for every side vertex. Its winding is the answer.
                if slot < 2 && t.iter().any(|&i| n(i).dot(out) <= 0.0) {
                    counts[slot].3 += 1;
                }
            }
            for (which, total, wound_in, normal_in) in counts {
                assert!(total > 0, "{label} {dist:+}: no {which} triangles — the question never arrived");
                assert_eq!(
                    (wound_in, normal_in),
                    (0, 0),
                    "{label} {dist:+}: of {total} {which} triangles, {wound_in} are wound inward and \
                     {normal_in} carry an inward normal"
                );
            }
        }
    }
}

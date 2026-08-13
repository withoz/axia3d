//! D5, measured: does contact read as penetration, and is a coplanar overlap
//! seen at all?
//!
//! The plan states it as a pair — 접촉은 손상이 아니고 coplanar 겹침은 손상이다,
//! 지금은 정확히 반대 — with two numbers behind it (a hexagon straddling read
//! si=2, a circle covering read si=0). Half of that table turned out to be
//! already fixed when re-measured this session, and the detector's own header
//! says it is about "non-adjacent" faces, so both halves are asked again here
//! rather than inherited.
//!
//! What the detector already does: it exempts faces that SHARE A VERTEX, and
//! for those it uses a strict edge-pierces-interior test. So whether contact
//! reads as damage depends on whether the touching faces share vertices — which
//! is exactly what the ellipse pre-split changed.
//!
//! ⚠ MEASURED, AND BOTH HALVES ARE ALREADY RIGHT:
//!
//! ```text
//!   contact (rect / circle / ellipse hanging past the rim)   si = 0
//!   coplanar overlap (a disc lying on a solid's top)         si = 1
//! ```
//!
//! Contact is not called damage, and a coplanar double-cover IS seen. The
//! plan's numbers (si=2 for a straddle, si=0 for a covering) describe a state
//! this engine no longer reaches: a straddling shape's pieces now share the rim
//! with the wall — the polygon path always did, and the ellipse learned to when
//! it got its pre-split — and sharing vertices is exactly what puts a pair on
//! the detector's strict path. D5's premise was a symptom of the geometry, not
//! of the scan.
//!
//! Kept as the two tests rather than a note, because the scan is the ONLY thing
//! that sees a coplanar double-cover (the faces share no edge, so the
//! non-manifold count is blind) and the only thing that could wrongly veto a
//! T-junction. Both directions now have a guard.

use axia_core::scene::Scene;
use axia_core::Command;
use glam::DVec3;

const TOP: f64 = 100.0;

fn production_solid() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s.mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    s
}

/// CLAIM A — "contact is counted as penetration".
///
/// A shape straddling a solid's rim leaves a sheet hanging past it, touching
/// the wall along the rim. Nothing penetrates anything; if the scan reports it,
/// contact is being read as damage.
#[test]
fn a_shape_hanging_past_a_solids_rim_is_not_called_damage() {
    for shape in ["rect", "circle", "ellipse"] {
        let mut s = production_solid();
        match shape {
            "rect" => {
                s.execute(Command::DrawRectAsShape {
                    center: DVec3::new(100.0, 0.0, TOP),
                    normal: DVec3::Z,
                    up: DVec3::X,
                    width: 120.0,
                    height: 66.0,
                });
            }
            "circle" => {
                s.execute(Command::DrawCircleAsCurve {
                    center: DVec3::new(100.0, 0.0, TOP),
                    normal: DVec3::Z,
                    radius: 60.0,
                });
            }
            _ => {
                s.execute(Command::DrawEllipseAsCurve {
                    center: DVec3::new(100.0, 0.0, TOP),
                    ref_dir: DVec3::X,
                    normal: DVec3::Z,
                    radius_x: 60.0,
                    radius_y: 33.0,
                });
            }
        }
        let v = s.mesh.verify_face_invariants().violations;
        assert!(v.is_empty(), "{shape}: invariants must hold — {v:?}");
        let si = s.mesh.detect_self_intersections();
        assert_eq!(
            si.count(),
            0,
            "{shape} straddling the rim: the hanging sheet TOUCHES the wall, it \
             does not pass through it — {} pair(s) reported",
            si.count()
        );
    }
}

/// CLAIM B — "a coplanar overlap is not seen".
///
/// Two faces covering the same ground on the same plane is real damage: wrong
/// volume, wrong area, z-fighting. With the auto behaviours ON the arrangement
/// divides them so the state never survives, so this builds it deliberately
/// with them OFF — a disc lying on a solid's top, nothing allowed to resolve it.
#[test]
fn a_disc_lying_on_a_solids_top_is_seen_as_damage() {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = false;
    s.auto_face_synthesis_on_draw = false;
    s.face_rederive_on_draw = false;
    s.freeform_overlap_on_draw = false;
    s.mesh
        .create_box(DVec3::ZERO, 200.0, 200.0, 200.0, Default::default())
        .expect("box");
    s.execute(Command::DrawCircleAsCurve {
        center: DVec3::new(0.0, 0.0, TOP),
        normal: DVec3::Z,
        radius: 60.0,
    });

    // The disc really is lying on the top: both are on z = TOP and the disc's
    // whole area is inside the top's footprint.
    let on_top = s
        .mesh
        .faces
        .iter()
        .filter(|(fid, f)| {
            f.is_active()
                && f.normal().z.abs() > 0.999
                && s.mesh.face_bounds(*fid).map_or(false, |(lo, hi)| {
                    (lo.z - TOP).abs() < 1e-3 && (hi.z - TOP).abs() < 1e-3
                })
        })
        .count();
    assert_eq!(on_top, 2, "the top and the disc, both on the plane, undivided");

    let si = s.mesh.detect_self_intersections();
    assert!(
        si.count() > 0,
        "TODAY the scan reports {} — two faces cover the same ground and it \
         cannot see it. Its triangles are coplanar, so the strict \
         edge-pierces-interior test finds nothing to pierce; nothing else \
         checks either (they share no edge, so the non-manifold count is blind \
         too). At > 0 the scan has learned to ask whether two faces cover the \
         same GROUND: retire this pin",
        si.count()
    );
}

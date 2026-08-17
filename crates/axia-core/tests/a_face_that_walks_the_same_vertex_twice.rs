//! Two instruments disagreed about one pair of faces. Both were right.
//!
//! Session 3's op-15 reduction — six operations, ending in a push — leaves a
//! stacked pair, and the two things that look at it say opposite things:
//!
//! ```text
//!   edge_stacked_face_pair          "이 둘은 같은 자리를 덮는다"
//!   coplanar_intersection_segments  "교차점 0개, lens 0정점"  (안 만난다)
//! ```
//!
//! ── What was between them ────────────────────────────────────────────────
//!
//! One of the two faces walked the same vertex twice. Its outer loop had 31
//! corners and two repeats, so the boundary crossed itself — a circle, then a
//! tail that wandered off and came back through points it had already used:
//!
//! ```text
//!   (-2.047, 120.070)   ← 15번째
//!    …
//!   (-2.047, 120.070)   ← 21번째, 같은 점
//!   (-10.046, 121.732)  ← 14번째의 반복
//! ```
//!
//! Neither instrument can read that. `edge_stacked_face_pair` asks which side of
//! the shared edge each face occupies right AT the edge, which is a local
//! question and correctly answered "the same side". The clipper projects the
//! whole polygon and runs Sutherland-Hodgman on it, which for a self-touching
//! ring returns nonsense — here, no overlap at all. Both answered what they were
//! asked; the face was what was wrong.
//!
//! ── Where it came from ───────────────────────────────────────────────────
//!
//! Measured by turning the repair off: with `subtract_double_covered_faces`
//! disabled after a push, op 5 leaves NO face whose loop repeats a vertex; with
//! it enabled, one. So the repair built it.
//!
//! Its difference walk returned a ring that came back to a point it had already
//! visited — `dedup_ring_2d` drops only CONSECUTIVE repeats — and the rebuild
//! hands every corner to `add_vertex`, which dedups at 0.15 μm (LOCKED #5). The
//! repeat became one VertId visited twice.
//!
//! Such a difference is two regions pinched at a point, not one polygon, so the
//! walk says so now instead of returning a ring that cannot be a face. The
//! overlap is left for something else to divide — which is where it was before
//! the repair reached it — and the face is readable again:
//!
//! ```text
//!   was   FaceId(37)  31 corners, 2 repeats   → 교차점 0, lens 0      (unreadable)
//!   is    FaceId(4)   23 corners, simple      → 교차점 4, lens 8정점   (a real overlap)
//! ```

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

const STACKED: &str = "cover the same ground";

#[derive(Clone, Copy, Debug)]
enum Op {
    Circle { x: f64, y: f64, z: f64, r: f64 },
    Polygon { x: f64, y: f64, z: f64, r: f64, n: u32 },
    Rect { x: f64, y: f64, z: f64, w: f64 },
    Extrude { face: u32, d: f64 },
    PushIn { face: u32, d: f64 },
}

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn apply(s: &mut Scene, op: Op) {
    match op {
        Op::Circle { x, y, z, r } => {
            s.execute(Command::DrawCircleAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
                segments: 24,
            });
        }
        Op::Polygon { x, y, z, r, n } => {
            s.execute(Command::DrawPolygonAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
                sides: n,
            });
        }
        Op::Rect { x, y, z, w } => {
            s.execute(Command::DrawRectAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                up: DVec3::X,
                width: w,
                height: w * 0.75,
            });
        }
        Op::Extrude { face, d } | Op::PushIn { face, d } => {
            s.execute(Command::CreateSolid {
                face_id: FaceId::new(face),
                mode: CreateSolidMode::Extrude { distance: d },
            });
        }
    }
}

/// Session 3's op-15 break, as reduced. FORM_MATERIAL is used by `create_box`
/// elsewhere in the family; this reduction needs no box.
fn six_ops() -> Vec<Op> {
    let _ = FORM_MATERIAL;
    vec![
        Op::Circle { x: 0.0, y: 150.0, z: 0.0, r: 30.0 },
        Op::Rect { x: 150.0, y: 0.0, z: 0.0, w: 100.0 },
        Op::Extrude { face: 1, d: 200.0 },
        Op::PushIn { face: 27, d: -100.0 },
        Op::Polygon { x: 100.0, y: 100.0, z: 200.0, r: 70.0, n: 4 },
        Op::PushIn { face: 21, d: -50.0 },
    ]
}

/// Every active face whose outer loop visits a vertex more than once.
fn self_touching(s: &Scene) -> Vec<(FaceId, usize, usize)> {
    let mut out = Vec::new();
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        let vs = s.mesh.collect_loop_verts(f.outer().start).unwrap_or_default();
        let mut seen = std::collections::HashSet::new();
        let dups = vs.iter().filter(|v| !seen.insert(**v)).count();
        if dups > 0 {
            out.push((fid, vs.len(), dups));
        }
    }
    out
}

/// No operation leaves a face whose outer loop is not simple.
///
/// Mutation-checked: dropping the self-touch refusal from
/// `polygon_difference_by_clip` brings it back — `FaceId(37)`, 31 corners, two
/// repeats — and fails this.
#[test]
fn no_operation_leaves_a_loop_that_walks_a_vertex_twice() {
    let mut s = prod();
    println!("\n  연산별 — 바깥 루프가 정점을 두 번 지나는 면\n");
    let mut first: Option<usize> = None;
    for (i, op) in six_ops().iter().enumerate() {
        apply(&mut s, *op);
        let bad = self_touching(&s);
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        println!(
            "    op {i} {:<48} 면 {live:>3}  겹친 루프 {:>2}{}",
            format!("{op:?}"),
            bad.len(),
            if bad.is_empty() { String::new() } else { format!("  {bad:?}") }
        );
        if first.is_none() && !bad.is_empty() {
            first = Some(i);
        }
    }
    println!();
    assert_eq!(
        first, None,
        "a face whose outer loop repeats a vertex cannot be read by anything — \
         the clipper returns nonsense and the local same-side test reports a \
         double cover, and both are right"
    );
}

/// The face the repair leaves is readable, and the overlap is measurable.
///
/// The pair is still there — this is not the fix for the overlap, it is the fix
/// for the FACE. What changed is that the clipper can now see what it is looking
/// at: `교차점 0, lens 0` on a self-touching ring became `교차점 4, lens 8정점`
/// on a simple one, which is a real two-bite overlap and is what a later pass
/// has to divide.
#[test]
fn the_pair_is_readable_even_though_it_is_still_a_pair() {
    use axia_geo::operations::coplanar as cop;

    let mut s = prod();
    for op in six_ops() {
        apply(&mut s, op);
    }
    let text: Vec<String> = s
        .mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|x| format!("{x:?}"))
        .collect();
    let Some(t) = text.iter().find(|t| t.contains(STACKED)) else {
        println!("\n  겹침 없음 — 나뉘게 되었다면 이 시험이 무엇이 고쳤는지 물어야 한다\n");
        return;
    };
    println!("\n  {t}\n");

    let ids: Vec<u32> = t
        .split("FaceId(")
        .skip(1)
        .filter_map(|q| q.split(')').next())
        .filter_map(|n| n.parse().ok())
        .collect();
    assert_eq!(ids.len(), 2, "the violation names two faces: {t}");
    let (a, b) = (FaceId::new(ids[0]), FaceId::new(ids[1]));
    let (big, small) = if s.mesh.face_outer_area(a) >= s.mesh.face_outer_area(b) {
        (a, b)
    } else {
        (b, a)
    };
    println!(
        "  big {big:?} 넓이 {:.0}   small {small:?} 넓이 {:.0}",
        s.mesh.face_outer_area(big),
        s.mesh.face_outer_area(small)
    );

    let vs = s.mesh.collect_loop_verts(s.mesh.faces[big].outer().start).unwrap_or_default();
    let mut seen = std::collections::HashSet::new();
    let dups = vs.iter().filter(|v| !seen.insert(**v)).count();
    println!("  big 의 바깥 루프 {}정점, 중복 {dups}개\n", vs.len());

    let measured = match cop::coplanar_intersection_segments(&s.mesh, big, small) {
        Err(e) => {
            println!("  coplanar_intersection_segments → Err({e})");
            None
        }
        Ok(ci) => {
            println!(
                "  coplanar_intersection_segments → 교차점 {}개, lens {}정점",
                ci.crossings.len(),
                ci.lens_polygon.len()
            );
            for c in ci.crossings.iter().take(4) {
                println!(
                    "        ({:.3},{:.3})  base edge {} t={:.4}",
                    c.point.x, c.point.y, c.face_a_edge, c.face_a_t
                );
            }
            Some(ci.crossings.len())
        }
    };
    println!();

    assert_eq!(
        dups, 0,
        "the face the repair leaves has to be readable — {dups} repeats in {} corners",
        vs.len()
    );
    assert!(
        matches!(measured, Some(n) if n >= 2 && n % 2 == 0),
        "and the overlap has to be MEASURABLE, in entry/exit pairs, which is what \
         a simple loop buys: {measured:?}"
    );
}

/// The two instruments, side by side, on the pair they used to disagree about.
///
/// Kept because the disagreement was the symptom that led here, and because
/// either one changing its mind would mean this file no longer describes what
/// happens.
#[test]
fn the_local_test_and_the_clipper_now_describe_the_same_pair() {
    use axia_geo::operations::coplanar as cop;

    let mut s = prod();
    for op in six_ops() {
        apply(&mut s, op);
    }
    let text: Vec<String> = s
        .mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|x| format!("{x:?}"))
        .collect();
    let Some(t) = text.iter().find(|t| t.contains(STACKED)) else {
        return;
    };
    let eid: u32 = t
        .split("EdgeId(")
        .nth(1)
        .and_then(|q| q.split(')').next())
        .and_then(|n| n.parse().ok())
        .expect("the violation names an edge");
    let e = axia_geo::EdgeId::new(eid);
    let (faces, _) = s.mesh.get_faces_sharing_edge(e);
    let pair = s.mesh.edge_stacked_face_pair(e);
    println!("\n  EdgeId({eid}) 위의 면 {faces:?}");
    println!("  edge_stacked_face_pair → {pair:?}");

    let (a, b) = pair.expect("the local test still names a pair");
    let overlap = cop::coplanar_intersection_segments(&s.mesh, a, b)
        .or_else(|_| cop::coplanar_intersection_segments(&s.mesh, b, a));
    match &overlap {
        Ok(ci) => println!(
            "  clipper              → 교차점 {}개, lens {}정점\n",
            ci.crossings.len(),
            ci.lens_polygon.len()
        ),
        Err(e) => println!("  clipper              → Err({e})\n"),
    }

    // The disagreement was: one said "same ground", the other said "they do not
    // meet". Whatever the clipper answers now, it must not be "they do not meet"
    // — an empty lens AND no crossings is the reading that meant the polygon was
    // unreadable.
    if let Ok(ci) = &overlap {
        assert!(
            !(ci.crossings.is_empty() && ci.lens_polygon.is_empty()),
            "the clipper saying two faces do not meet while the local test says \
             they cover the same ground is the signature of a self-touching loop"
        );
    }
}

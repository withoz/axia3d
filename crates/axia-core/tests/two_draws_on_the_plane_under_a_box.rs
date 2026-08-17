//! Session 9's op-13 break: two draws on the plane a box's BOTTOM occupies.
//!
//! ```text
//!   box     centre (-50,-150), 140 wide, 120 tall   →  x ∈ [-120, 20]
//!                                                       y ∈ [-220, -80]
//!                                                       z ∈ [ 200, 320]
//!   pentagon  (-100, -50, 200)  r=30   →  y ∈ [-74, -26]  beside the box
//!   circle    (-100,-100, 200)  r=30   →  y ∈ [-130, -70] straddling its rim
//! ```
//!
//! Both draws land on z=200, the plane the box's bottom face is on. The pentagon
//! sits clear of the box; the circle straddles the rim, half under the box and
//! half beside it.
//!
//! ── What it was ─────────────────────────────────────────────────────────
//!
//! Three faces came out with the same four corners. Read one thing at a time:
//!
//! ```text
//!   circle wholly under the box     8 faces    0 violations
//!   circle straddling the rim      13 faces    4 violations
//!   circle wholly outside           9 faces    0 violations
//!
//!   auto_intersect   off           13 faces    4     — not it
//!   face_rederive    off            9 faces    0     — downstream of it
//!   freeform_overlap off           13 faces    4     — not it
//!   face_synthesis   off           13 faces    4     — not it
//! ```
//!
//! But the re-derive itself was innocent. Run by hand on the finished mesh it
//! left eleven faces and no violations, and running it again changed nothing —
//! it is idempotent. Seeding it made no difference, and neither did re-tiling
//! the plane from both of its normals. What the flag also gates is the pass
//! AFTER it: `split_faces_crossing_other_planes`.
//!
//! That pass takes its work from `detect_self_intersections`, and after the
//! re-derive there was exactly one pair left — a box WALL against a piece on
//! z=200. The wall's foot is on that plane and the piece ends at it, so they
//! touch along a line rather than pass through each other. Nothing to divide,
//! and the split handed the same region back twice.
//!
//! ── What it is ──────────────────────────────────────────────────────────
//!
//! Telling touching from crossing before the fact is its own problem. The result
//! is not in doubt, so that pass now undoes itself when it leaves the mesh
//! objecting where it did not before — the same judgement the re-derive and the
//! post-draw repair already make.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::operations::face_rederive::rebuild_coplanar_faces_analytic_scoped;
use axia_geo::FaceId;
use glam::DVec3;

#[derive(Clone, Copy, Debug)]
enum Op {
    Box { x: f64, y: f64, z: f64, w: f64 },
    Polygon { x: f64, y: f64, z: f64, r: f64, n: u32 },
    CircleCurve { x: f64, y: f64, z: f64, r: f64 },
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
        Op::Box { x, y, z, w } => {
            let _ = s.mesh.create_box(
                DVec3::new(x, y, z) + DVec3::new(0.0, 0.0, 60.0),
                w,
                120.0,
                w,
                FORM_MATERIAL,
            );
        }
        Op::Polygon { x, y, z, r, n } => {
            s.execute(Command::DrawPolygonAsShape {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
                sides: n,
            });
        }
        Op::CircleCurve { x, y, z, r } => {
            s.execute(Command::DrawCircleAsCurve {
                center: DVec3::new(x, y, z),
                normal: DVec3::Z,
                radius: r,
            });
        }
    }
}

/// The three operations, as reduced from session 9's twenty.
fn shrunk() -> Vec<Op> {
    vec![
        Op::Box { x: -50.0, y: -150.0, z: 200.0, w: 140.0 },
        Op::Polygon { x: -100.0, y: -50.0, z: 200.0, r: 30.0, n: 5 },
        Op::CircleCurve { x: -100.0, y: -100.0, z: 200.0, r: 30.0 },
    ]
}

fn violations(s: &Scene) -> Vec<String> {
    s.mesh
        .verify_face_invariants()
        .violations
        .iter()
        .map(|v| format!("{v:?}"))
        .collect()
}

fn built(ops: &[Op]) -> Scene {
    let mut s = prod();
    for op in ops {
        apply(&mut s, *op);
    }
    s
}

fn live_faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// The three operations, sound, with every face on the plane written out.
///
/// Mutation-checked: dropping the rollback from
/// `split_faces_crossing_other_planes` brings back three faces with the same
/// four corners and four violations, and fails this.
#[test]
fn the_three_operations_are_sound() {
    let s = built(&shrunk());
    let v = violations(&s);

    println!("\n  z=200 위의 면\n");
    let mut on_plane = 0;
    for (fid, f) in s.mesh.faces.iter() {
        if !f.is_active() {
            continue;
        }
        let vv = s.mesh.collect_loop_verts(f.outer().start).unwrap_or_default();
        let pts: Vec<DVec3> = vv
            .iter()
            .filter_map(|x| s.mesh.verts.get(*x))
            .map(|p| p.pos())
            .collect();
        if pts.is_empty() || !pts.iter().all(|p| (p.z - 200.0).abs() < 1e-6) {
            continue;
        }
        on_plane += 1;
        let corners: Vec<String> = pts
            .iter()
            .take(5)
            .map(|p| format!("({:.0},{:.0})", p.x, p.y))
            .collect();
        println!(
            "    {fid:?}  n_z={:+.0}  verts={:<3} {}  {}{}",
            f.normal().normalize_or_zero().z,
            pts.len(),
            if s.mesh.is_face_in_volume(fid) { "solid" } else { "sheet" },
            corners.join(" "),
            if pts.len() > 5 { " …" } else { "" }
        );
    }
    println!("\n  면 {on_plane}개, 위반 {}\n", v.len());
    for t in v.iter().take(4) {
        println!("    ✗ {t}");
    }

    assert!(
        v.is_empty(),
        "the three-operation reduction has to stay sound — it used to hand back \
         the same four-cornered piece three times: {v:?}"
    );
    // Not just clean — still divided. Without this the test would pass on a
    // version that stopped putting the draws on the plane at all.
    assert!(
        on_plane >= 3,
        "and the plane has to still carry the draws — {on_plane} faces on it"
    );
}

/// The circle in three places, which is the reading that named the condition.
///
/// Wholly under the box, straddling its rim, wholly outside. The middle row was
/// the only one that broke; all three are clean now, and the outer two stay so a
/// fix that trades one case for another fails here.
#[test]
fn the_circle_under_the_box_on_its_rim_and_outside() {
    let mut rows: Vec<(&str, f64, usize, usize)> = Vec::new();
    for (name, cy) in [("상자 밑", -150.0), ("걸침", -100.0), ("상자 밖", -20.0)] {
        let s = built(&[
            shrunk()[0],
            shrunk()[1],
            Op::CircleCurve { x: -100.0, y: cy, z: 200.0, r: 30.0 },
        ]);
        let v = violations(&s);
        println!("\n  원 y={cy:>5} ({name}): 면 {}, 위반 {}", live_faces(&s), v.len());
        for t in v.iter().take(2) {
            println!("      ✗ {t}");
        }
        rows.push((name, cy, live_faces(&s), v.len()));
    }
    println!();
    for (name, cy, live, bad) in &rows {
        assert_eq!(*bad, 0, "{name} (y={cy}): {live} faces and {bad} violations");
    }
}

/// The pentagon is part of the repro, not scenery.
///
/// The box and the circle alone were always clean; it takes a second, separate
/// region on the same plane. Kept because a later change that breaks the
/// two-draw case would otherwise read as this file's business, and it is not.
#[test]
fn the_box_and_the_circle_alone_were_always_clean() {
    let two = built(&[shrunk()[0], shrunk()[2]]);
    let three = built(&shrunk());
    let (a, b) = (violations(&two).len(), violations(&three).len());
    println!(
        "\n  상자+원: 면 {}, 위반 {a}\n  상자+오각형+원: 면 {}, 위반 {b}\n",
        live_faces(&two),
        live_faces(&three)
    );
    assert_eq!(a, 0, "the two-draw case was never the defect");
    assert_eq!(b, 0, "and the three-draw one is fixed");
}

/// The re-derive settles: a second pass on the same plane changes nothing.
///
/// Measured while looking for the culprit, and it cleared the re-derive — three
/// passes by hand left eleven faces and no violations every time. Kept because
/// "the re-derive is idempotent" is what made the pass after it the suspect.
#[test]
fn the_rederive_settles_on_this_plane() {
    let mut s = prod();
    s.face_rederive_on_draw = false;
    for op in shrunk() {
        apply(&mut s, op);
    }
    println!(
        "\n  그리기만 (재유도 끔): 면 {}, 위반 {}",
        live_faces(&s),
        violations(&s).len()
    );

    let mut counts: Vec<(usize, usize)> = Vec::new();
    for pass in 1..=3 {
        let r = rebuild_coplanar_faces_analytic_scoped(
            &mut s.mesh,
            DVec3::new(0.0, 0.0, 200.0),
            DVec3::Z,
            1e-3,
            true,
            None,
        )
        .expect("the re-derive has to run");
        println!(
            "    {pass}회차 → 제거 {:>2} 생성 {:>2}  면 {:>3}  위반 {:>2}",
            r.removed_faces,
            r.created_faces,
            live_faces(&s),
            violations(&s).len()
        );
        counts.push((live_faces(&s), violations(&s).len()));
    }
    println!();
    assert_eq!(counts[0], counts[1], "pass 2 has to change nothing: {counts:?}");
    assert_eq!(counts[1], counts[2], "and neither does pass 3: {counts:?}");
    assert_eq!(counts[0].1, 0, "and it leaves the plane sound: {counts:?}");
}

/// Re-tiling the same plane from both of its normals does not duplicate.
///
/// The box's bottom points down and every drawn face points up, so z=200 is one
/// plane wearing two normals — which looked like an explanation and was not.
/// Kept as a guard: if re-tiling once per orientation ever starts duplicating,
/// this says so rather than the fuzz finding it twenty operations later.
#[test]
fn re_tiling_from_both_normals_does_not_duplicate() {
    let origin = DVec3::new(0.0, 0.0, 200.0);
    let mut rows: Vec<(&str, usize, usize)> = Vec::new();
    for (name, normals) in [
        ("+Z 만", vec![DVec3::Z]),
        ("+Z 그리고 −Z", vec![DVec3::Z, -DVec3::Z]),
    ] {
        let mut s = prod();
        s.face_rederive_on_draw = false;
        for op in shrunk() {
            apply(&mut s, op);
        }
        for n in &normals {
            let _ = rebuild_coplanar_faces_analytic_scoped(
                &mut s.mesh, origin, *n, 1e-3, true, None,
            );
        }
        println!("\n  {name}: 면 {}, 위반 {}", live_faces(&s), violations(&s).len());
        rows.push((name, live_faces(&s), violations(&s).len()));
    }
    println!();
    assert_eq!(rows[0].2, 0, "one orientation stays clean: {rows:?}");
    assert_eq!(rows[1].2, 0, "and so does the pair: {rows:?}");
    assert_eq!(
        rows[0].1, rows[1].1,
        "and the second orientation adds nothing: {rows:?}"
    );
}

/// After the re-derive, the pair that is left is a touch, not a crossing.
///
/// One box WALL against a piece on z=200. The wall's foot is on that plane and
/// the piece ends at it, so they meet along a line rather than pass through each
/// other — and this is what the split used to be handed.
#[test]
fn the_pair_left_after_the_rederive_is_a_touch() {
    let mut s = prod();
    s.face_rederive_on_draw = false;
    for op in shrunk() {
        apply(&mut s, op);
    }
    let _ = rebuild_coplanar_faces_analytic_scoped(
        &mut s.mesh, DVec3::new(0.0, 0.0, 200.0), DVec3::Z, 1e-3, true, None,
    );
    println!(
        "\n  재유도 뒤: 면 {}, 위반 {}\n",
        live_faces(&s),
        violations(&s).len()
    );

    let pairs = s.mesh.detect_self_intersections().intersecting_pairs;
    println!("  자기교차 쌍 {}개\n", pairs.len());
    let span = |f: FaceId| -> (f64, f64) {
        let Some(face) = s.mesh.faces.get(f) else { return (f64::NAN, f64::NAN) };
        let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
        let zs: Vec<f64> = vv
            .iter()
            .filter_map(|x| s.mesh.verts.get(*x))
            .map(|p| p.pos().z)
            .collect();
        zs.iter()
            .fold((f64::MAX, f64::MIN), |(lo, hi), z| (lo.min(*z), hi.max(*z)))
    };
    let mut touching = 0;
    for (a, b) in pairs.iter() {
        let (az0, az1) = span(*a);
        let (bz0, bz1) = span(*b);
        println!("    {a:?} z∈[{az0:.0},{az1:.0}]  ×  {b:?} z∈[{bz0:.0},{bz1:.0}]");
        // One lies flat on the plane, the other stands on it: they share the
        // plane's z and nothing else.
        let flat_on_the_others_foot = ((az1 - az0).abs() < 1e-9 && (az0 - bz0).abs() < 1e-6)
            || ((bz1 - bz0).abs() < 1e-9 && (bz0 - az0).abs() < 1e-6);
        if flat_on_the_others_foot {
            touching += 1;
        }
    }
    println!("\n  그 중 발끝만 닿는 쌍 {touching}개\n");

    assert_eq!(violations(&s).len(), 0, "the re-derive leaves this plane sound");
    assert_eq!(
        touching,
        pairs.len(),
        "every pair the split is handed here is a wall standing on the plane its \
         neighbour lies flat on — if one of them really crosses, the rollback is \
         declining work it should be doing"
    );
}

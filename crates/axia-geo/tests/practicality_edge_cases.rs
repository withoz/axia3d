//! 실용성 테스트 — Edge case / degenerate geometry / stress tests.
//!
//! AXiA 3D 엔진이 비정상 입력이나 규모 있는 씬에서도 crash/hang 없이
//! 예측 가능하게 작동하는지 확인.
//!
//! ## ADR-304 — what these tests used to be, and what they are now
//!
//! Three of these ended in `let _ = result;` and checked nothing, while
//! `docs/PRACTICALITY_REPORT.md` cited them as "✅ ADR-003 degenerate guard"
//! verified. Measuring the contract they claimed to cover showed it does not
//! exist: `add_face_with_holes` accepts a collinear zero-area triangle, a loop
//! that repeats a vertex, and a face with a NaN coordinate. Only a loop of
//! fewer than three vertices is refused.
//!
//! That permissiveness is deliberate — `NORMAL_EPSILON = 0.0`, commented "keep
//! at 0 to avoid missing thin faces". Its side effect was not: `len < 0.0` is
//! never true, so `compute_normal`'s degenerate `bail!` is unreachable and a
//! zero-length Newell normal falls through to `Ok(normal / 0.0)` = NaN.
//! `verify_face_invariants` then missed it too, because `NaN > 1e-10` is false
//! and the whole winding check was skipped.
//!
//! So the real contract is **permissive creation, detection at the verifier**,
//! and ADR-304 added the missing half. These tests pin both ends of it.
//!
//! Run: `cargo test --test practicality_edge_cases`

use axia_geo::entities::*;
use axia_geo::mesh::Mesh;
use glam::DVec3;

/// Every degenerate face this file builds must satisfy the same contract:
/// accepted at creation, then caught by the verifier. `expected` names WHICH
/// invariant catches it — they do not all trip the same one, and pinning the
/// specific message is what makes these tests fail if detection moves.
fn assert_degenerate_contract(mesh: &Mesh, fid: FaceId, what: &str, expected: &str) {
    let report = mesh.verify_face_invariants();
    assert!(
        !report.is_valid(),
        "{what}: verify_face_invariants reported degenerate face {fid:?} \
         (normal {:?}) as VALID. Every gate in this codebase (ADR-267 integrity, \
         ADR-272 closure, the ADR-302/303 sims) asks this verifier, so a blind \
         spot here is a blind spot everywhere. Report: {}",
        mesh.faces[fid].normal(),
        report.summary()
    );
    assert!(
        report.violations.iter().any(|v| v.contains(expected)),
        "{what}: expected a violation containing {expected:?}, got {:?}",
        report.violations
    );
}

// ─── Category 1: Degenerate input — accepted, then caught ────────────

#[test]
fn nan_vertex_is_accepted_and_flagged() {
    let mut mesh = Mesh::new();
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1000.0, 0.0, 0.0));
    let v2 = mesh.add_vertex(DVec3::new(f64::NAN, 0.0, 1000.0));
    let v3 = mesh.add_vertex(DVec3::new(0.0, 0.0, 1000.0));
    let fid = mesh
        .add_face_with_holes(&[v0, v1, v2, v3], &[], MaterialId::new(0))
        .expect("creation is permissive by design (NORMAL_EPSILON = 0.0)");
    assert_degenerate_contract(&mesh, fid, "NaN vertex", "not finite");
}

#[test]
fn zero_area_triangle_is_accepted_and_flagged() {
    // Collinear 3 vertices — zero area. Newell gives (0,0,0); `len < 0.0` is
    // never true, so it divides by zero rather than reaching the bail.
    let mut mesh = Mesh::new();
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1000.0, 0.0, 0.0));
    let v2 = mesh.add_vertex(DVec3::new(2000.0, 0.0, 0.0));
    let fid = mesh
        .add_face_with_holes(&[v0, v1, v2], &[], MaterialId::new(0))
        .expect("creation is permissive by design");
    assert_degenerate_contract(&mesh, fid, "collinear triangle", "not finite");
}

#[test]
fn duplicate_vertex_in_face_is_accepted_and_flagged() {
    let mut mesh = Mesh::new();
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1000.0, 0.0, 0.0));
    let fid = mesh
        .add_face_with_holes(&[v0, v1, v0, v1], &[], MaterialId::new(0))
        .expect("creation is permissive by design");
    // Caught by I1, not I6: the DCEL folds [a, b, a, b] into a 2-vertex
    // loop, so it trips the outer-loop size check before the normal is ever
    // consulted. Same contract, different invariant.
    assert_degenerate_contract(&mesh, fid, "duplicate-vertex loop", "outer loop has 2 verts");
}

#[test]
fn fewer_than_three_verts_is_the_one_creation_guard() {
    // The only thing `add_face_with_holes` actually refuses.
    let mut mesh = Mesh::new();
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1000.0, 0.0, 0.0));
    let err = mesh
        .add_face_with_holes(&[v0, v1], &[], MaterialId::new(0))
        .expect_err("a 2-vertex loop must be refused");
    assert!(
        err.to_string().contains("at least 3 vertices"),
        "unexpected rejection reason: {err}"
    );
    assert_eq!(
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        0,
        "a refused face must not be left in the mesh"
    );
}

#[test]
fn a_valid_face_is_not_flagged() {
    // The control. Without this, every assertion above would still pass if the
    // verifier simply reported everything as broken.
    let mut mesh = Mesh::new();
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1000.0, 0.0, 0.0));
    let v2 = mesh.add_vertex(DVec3::new(1000.0, 0.0, 1000.0));
    let v3 = mesh.add_vertex(DVec3::new(0.0, 0.0, 1000.0));
    let fid = mesh
        .add_face_with_holes(&[v0, v1, v2, v3], &[], MaterialId::new(0))
        .expect("valid quad");
    assert!(mesh.faces[fid].normal().is_finite(), "valid face must have a finite normal");
    let report = mesh.verify_face_invariants();
    assert!(report.is_valid(), "valid quad flagged: {}", report.summary());
}

// ─── Category 2: Stress / Scale ───────────────────────────────────

#[test]
fn build_1000_quad_scene_completes() {
    // 1000개 독립 quad face 생성 — 성능/메모리 sanity.
    let start = std::time::Instant::now();
    let mut mesh = Mesh::new();

    for i in 0..1000 {
        let x = (i % 50) as f64 * 100.0;
        let z = (i / 50) as f64 * 100.0;
        let y = 500.0 + (i as f64 * 0.1);
        let v0 = mesh.add_vertex(DVec3::new(x,     y, z));
        let v1 = mesh.add_vertex(DVec3::new(x,     y, z + 80.0));
        let v2 = mesh.add_vertex(DVec3::new(x + 80.0, y, z + 80.0));
        let v3 = mesh.add_vertex(DVec3::new(x + 80.0, y, z));
        mesh.add_face_with_holes(&[v0, v1, v2, v3], &[], MaterialId::new(0))
            .expect("face creation must succeed");
    }
    let build_elapsed = start.elapsed();

    assert_eq!(
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        1000,
        "all 1000 quads must be present"
    );
    // 1000 face < 1초 예상. CI 느린 환경 고려 5초 한도.
    assert!(build_elapsed.as_secs() < 5, "1000 face build too slow: {:?}", build_elapsed);
    println!("Build 1000 quads: {:?}", build_elapsed);
}

#[test]
fn repeated_add_remove_leaves_no_active_faces() {
    // ADR-304 — this was `deep_undo_does_not_leak`, whose name promised the
    // only undo-depth coverage in the repo. It exercised no undo path: there is
    // no undo API in axia-geo at all (`grep "fn undo" crates/axia-geo/src` is
    // empty; undo lives in axia-transaction). It also discarded remove_face's
    // Result, so a remove regression passed silently. Renamed to what it does,
    // and now it checks the outcome.
    let mut mesh = Mesh::new();
    for iter in 0..100 {
        let offset = iter as f64 * 10.0;
        let v0 = mesh.add_vertex(DVec3::new(offset,         0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(offset + 100.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(offset + 100.0, 0.0, 100.0));
        let v3 = mesh.add_vertex(DVec3::new(offset,         0.0, 100.0));
        let fid = mesh.add_face_with_holes(&[v0, v1, v2, v3], &[], MaterialId::new(0))
            .expect("face creation must succeed");
        mesh.remove_face(fid).unwrap_or_else(|e| panic!("iter {iter}: remove_face failed: {e}"));
    }
    assert_eq!(
        mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        0,
        "every face was removed, so none may remain active"
    );
}

// ─── Category 3: (removed — shadow system deferred to future ADR-106) ────

// ─── Category 4: Boundary coordinate magnitudes ─────────────────

#[test]
fn very_large_coordinate_does_not_overflow() {
    // 지구 규모 건물(위경도 km) — 좌표가 1e6 넘어도 안전해야 함.
    let mut mesh = Mesh::new();
    let s = 1_000_000.0;  // 1km
    let v0 = mesh.add_vertex(DVec3::new(s, s, s));
    let v1 = mesh.add_vertex(DVec3::new(s, s, s + 1000.0));
    let v2 = mesh.add_vertex(DVec3::new(s + 1000.0, s, s + 1000.0));
    let v3 = mesh.add_vertex(DVec3::new(s + 1000.0, s, s));
    let fid = mesh
        .add_face_with_holes(&[v0, v1, v2, v3], &[], MaterialId::new(0))
        .expect("1km-scale face must be accepted");
    assert!(
        mesh.faces[fid].normal().is_finite(),
        "1km-scale face must still produce a finite normal"
    );
}

#[test]
fn very_small_coordinate_stays_above_the_dedup_floor() {
    // 서브밀리미터 face — numerical precision 경계. 1μm (1e-3 mm) is well above
    // the 0.15μm spatial-hash dedup cell (LOCKED #5), so the four corners stay
    // distinct and the face is valid rather than degenerate.
    let mut mesh = Mesh::new();
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(0.001, 0.0, 0.0));
    let v2 = mesh.add_vertex(DVec3::new(0.001, 0.0, 0.001));
    let v3 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.001));
    assert!(
        v0 != v1 && v1 != v2 && v2 != v3 && v3 != v0,
        "1μm apart must not weld — dedup cell is 0.15μm (LOCKED #5)"
    );
    let fid = mesh
        .add_face_with_holes(&[v0, v1, v2, v3], &[], MaterialId::new(0))
        .expect("a 1μm face is small but valid");
    assert!(
        mesh.faces[fid].normal().is_finite(),
        "a 1μm face must produce a finite normal, not NaN"
    );
    assert!(
        mesh.verify_face_invariants().is_valid(),
        "a 1μm face is thin, not degenerate — it must not be flagged"
    );
}

#[test]
fn below_the_dedup_floor_the_corners_weld() {
    // The other side of LOCKED #5: closer than 0.15μm and the vertices become
    // one, which is what makes a "thin" face degenerate rather than small.
    let mut mesh = Mesh::new();
    let a = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let b = mesh.add_vertex(DVec3::new(1e-5, 0.0, 0.0));
    assert_eq!(a, b, "1e-5 mm apart is below the 1.5e-4 mm dedup cell — must weld");
}

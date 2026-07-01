//! Slice (Plane Cut) — volume splitting tests.
//!
//! Build a closed cube, slice it with various planes, verify that:
//! - Both resulting halves are closed Wall solids
//! - Cap face count matches expected loops
//! - All resulting faces classify as Wall (is_face_in_volume == true)
//! - Cut loops have the right vertex count

use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use axia_geo::operations::slice::SlicePlane;
use glam::DVec3;

/// Build a unit cube of side `s` centered at origin. Returns the 6 face ids.
fn make_cube(mesh: &mut Mesh, m: MaterialId, s: f64) -> [axia_geo::FaceId; 6] {
    let h = s * 0.5;
    let v000 = mesh.add_vertex(DVec3::new(-h, -h, -h));
    let v100 = mesh.add_vertex(DVec3::new( h, -h, -h));
    let v110 = mesh.add_vertex(DVec3::new( h,  h, -h));
    let v010 = mesh.add_vertex(DVec3::new(-h,  h, -h));
    let v001 = mesh.add_vertex(DVec3::new(-h, -h,  h));
    let v101 = mesh.add_vertex(DVec3::new( h, -h,  h));
    let v111 = mesh.add_vertex(DVec3::new( h,  h,  h));
    let v011 = mesh.add_vertex(DVec3::new(-h,  h,  h));

    // CCW from outside.
    let bottom = mesh.add_face(&[v000, v010, v110, v100], m).unwrap(); // -Y? actually -Z
    let top    = mesh.add_face(&[v001, v101, v111, v011], m).unwrap();
    let front  = mesh.add_face(&[v000, v100, v101, v001], m).unwrap();
    let back   = mesh.add_face(&[v010, v011, v111, v110], m).unwrap();
    let left   = mesh.add_face(&[v000, v001, v011, v010], m).unwrap();
    let right  = mesh.add_face(&[v100, v110, v111, v101], m).unwrap();

    [bottom, top, front, back, left, right]
}

#[test]
fn slice_cube_horizontally_through_middle() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);

    // Sanity: cube starts as a closed Wall solid.
    let info = mesh.face_set_manifold_info(&faces);
    assert!(info.is_closed_solid, "cube should be closed initially");

    // Cut at z=0 (XY plane).
    let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();
    let result = mesh.slice_volume_by_plane(&faces, plane, m).expect("slice should succeed");

    // Expectations:
    // - 4 side walls were each crossed → each split into 2 sub-faces → 8 wall sub-faces.
    // - top wall stays as-is in above; bottom wall stays in below.
    // - 1 cut loop, 2 cap faces (above + below).
    assert_eq!(result.cut_loops.len(), 1, "single cut loop expected");
    assert_eq!(result.cut_loops[0].len(), 4, "cut loop should be a quad (4 verts)");
    assert_eq!(result.cap_above.len(), 1);
    assert_eq!(result.cap_below.len(), 1);
    // 4 split walls (above subface) + 1 top  = 5
    assert_eq!(result.above_walls.len(), 5);
    // 4 split walls (below subface) + 1 bottom = 5
    assert_eq!(result.below_walls.len(), 5);

    // Both halves must form closed Wall solids.
    let above_set: Vec<_> = result.above_walls.iter().chain(result.cap_above.iter()).copied().collect();
    let below_set: Vec<_> = result.below_walls.iter().chain(result.cap_below.iter()).copied().collect();
    assert!(mesh.face_set_manifold_info(&above_set).is_closed_solid,
        "above half must be a closed solid");
    assert!(mesh.face_set_manifold_info(&below_set).is_closed_solid,
        "below half must be a closed solid");

    // Every face in both halves must classify as Wall (in volume).
    for &fid in above_set.iter().chain(below_set.iter()) {
        assert!(mesh.is_face_in_volume(fid),
            "face {:?} after slice must be a Wall", fid);
    }
}

#[test]
fn slice_cube_diagonally() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);

    // Tilted plane through origin: normal = normalize(1, 1, 1).
    let plane = SlicePlane::new(DVec3::ZERO, DVec3::new(1.0, 1.0, 1.0)).unwrap();
    let result = mesh.slice_volume_by_plane(&faces, plane, m).expect("diagonal slice ok");

    // Cut loop should be a hexagon (6 verts) for unit cube cut by (1,1,1) plane through origin.
    assert_eq!(result.cut_loops.len(), 1);
    assert_eq!(result.cut_loops[0].len(), 6, "diagonal cut should produce hexagon");
    assert_eq!(result.cap_above.len(), 1);
    assert_eq!(result.cap_below.len(), 1);

    // Verify closure on both halves.
    let above_set: Vec<_> = result.above_walls.iter().chain(result.cap_above.iter()).copied().collect();
    let below_set: Vec<_> = result.below_walls.iter().chain(result.cap_below.iter()).copied().collect();
    assert!(mesh.face_set_manifold_info(&above_set).is_closed_solid);
    assert!(mesh.face_set_manifold_info(&below_set).is_closed_solid);
}

#[test]
fn slice_cube_with_plane_off_center() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);

    // Off-center horizontal cut at z = 200.
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 200.0), DVec3::Z).unwrap();
    let result = mesh.slice_volume_by_plane(&faces, plane, m).expect("off-center slice ok");

    assert_eq!(result.cut_loops.len(), 1);
    assert_eq!(result.cut_loops[0].len(), 4);
    assert_eq!(result.above_walls.len(), 5);
    assert_eq!(result.below_walls.len(), 5);

    let above_set: Vec<_> = result.above_walls.iter().chain(result.cap_above.iter()).copied().collect();
    let below_set: Vec<_> = result.below_walls.iter().chain(result.cap_below.iter()).copied().collect();
    assert!(mesh.face_set_manifold_info(&above_set).is_closed_solid);
    assert!(mesh.face_set_manifold_info(&below_set).is_closed_solid);
}

#[test]
fn slice_with_non_intersecting_plane_errors() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);

    // Plane far above the cube — no crossing.
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 5000.0), DVec3::Z).unwrap();
    let res = mesh.slice_volume_by_plane(&faces, plane, m);
    assert!(res.is_err(), "non-intersecting plane should error");
}

#[test]
fn slice_with_face_on_plane_errors() {
    // If the plane coincides exactly with the cube's top face, that face
    // sits entirely on the plane → bail.
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);
    // Top is at z = 500.
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 500.0), DVec3::Z).unwrap();
    let res = mesh.slice_volume_by_plane(&faces, plane, m);
    assert!(res.is_err(), "face-on-plane case should error in MVP");
}

#[test]
fn slice_rejects_inactive_face() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);
    let _ = mesh.remove_face(faces[0]);
    let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();
    let res = mesh.slice_volume_by_plane(&faces, plane, m);
    assert!(res.is_err(), "inactive face should error");
}

#[test]
fn slice_global_invariants_must_pass() {
    // ADR-007 I5: every edge must be incident to ≤ 2 active faces.
    // After slicing, the two halves must be topologically independent —
    // no edge should be shared by both above and below halves.
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);
    let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();
    let _ = mesh.slice_volume_by_plane(&faces, plane, m).unwrap();

    let report = mesh.verify_face_invariants();
    assert!(
        report.is_valid(),
        "ADR-007 invariants must hold after slice — violations:\n{}",
        report.summary()
    );
}

#[test]
fn slice_invariants_preserved() {
    // Verify ADR-007 normal cache invariant: after slicing, cached normals
    // match topology (no stale cache).
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 500.0);
    let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();
    let result = mesh.slice_volume_by_plane(&faces, plane, m).unwrap();

    // After reconcile, calling reconcile again should be a no-op.
    let drift = mesh.reconcile_face_normals();
    assert_eq!(drift, 0, "after slice, reconcile should be no-op (drift=0)");

    // Cap face normals must oppose each other.
    let cap_a_normal = mesh.faces[result.cap_above[0]].normal();
    let cap_b_normal = mesh.faces[result.cap_below[0]].normal();
    let dot = cap_a_normal.dot(cap_b_normal);
    assert!(dot < -0.99,
        "cap_above and cap_below normals should be anti-parallel (dot={})", dot);
}

// ── ADR-242 Phase 1 C1 — non-convex face slice (>2 On verts) ─────────────

/// Build a prism from an arbitrary CCW (viewed from +Z) simple-polygon
/// footprint, extruded in Z by `h`. Returns all faces (2 caps + N sides).
/// Caps may be non-convex (U / comb / etc.).
fn make_prism(mesh: &mut Mesh, m: MaterialId, fp: &[(f64, f64)], h: f64) -> Vec<axia_geo::FaceId> {
    let n = fp.len();
    let b: Vec<_> = fp.iter().map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, 0.0))).collect();
    let t: Vec<_> = fp.iter().map(|&(x, y)| mesh.add_vertex(DVec3::new(x, y, h))).collect();
    let mut faces = Vec::new();
    // Bottom cap (normal −Z): reversed footprint. Top cap (normal +Z): footprint.
    let bot: Vec<_> = b.iter().rev().copied().collect();
    faces.push(mesh.add_face(&bot, m).unwrap());
    faces.push(mesh.add_face(&t, m).unwrap());
    for i in 0..n {
        let j = (i + 1) % n;
        faces.push(mesh.add_face(&[b[i], b[j], t[j], t[i]], m).unwrap());
    }
    faces
}

/// Build a U-shaped prism (8-vert U footprint extruded in Z). The 2 U-cap
/// faces are non-convex octagons. Returns all 10 face ids (2 caps + 8 sides).
fn make_u_prism(mesh: &mut Mesh, m: MaterialId, h: f64) -> Vec<axia_geo::FaceId> {
    // U opening +Y, notch at x∈[10,20], y∈[10,20]. CCW viewed from +Z.
    let fp = [
        (0.0, 0.0), (30.0, 0.0), (30.0, 20.0), (20.0, 20.0),
        (20.0, 10.0), (10.0, 10.0), (10.0, 20.0), (0.0, 20.0),
    ];
    make_prism(mesh, m, &fp, h)
}

#[test]
fn slice_u_prism_nonconvex_cap_through_notch() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_u_prism(&mut mesh, m, 10.0);
    assert!(mesh.face_set_manifold_info(&faces).is_closed_solid,
        "U-prism must be a closed solid initially");

    // Vertical plane y=15 (normal +Y) crosses each U-cap in TWO segments
    // (x∈[0,10] left prong + x∈[20,30] right prong) → 4 On verts per cap =
    // non-convex crossing. The two prong-tips (y>15) are disconnected → 2 loops.
    let plane = SlicePlane::new(DVec3::new(0.0, 15.0, 0.0), DVec3::Y).unwrap();
    let result = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("non-convex U-cap slice should succeed");

    assert_eq!(result.cut_loops.len(), 2, "two prongs → two cut loops");
    assert_eq!(result.cap_above.len(), 2, "two caps sealing the two prong tips");
    assert_eq!(result.cap_below.len(), 2);

    // Both halves closed (boundary_edge_count == 0) + invariants hold.
    let above: Vec<_> = result.above_walls.iter().chain(result.cap_above.iter()).copied().collect();
    let below: Vec<_> = result.below_walls.iter().chain(result.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0,
        "above half must be closed (no boundary edges)");
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0,
        "below half must be closed");
    assert!(mesh.verify_face_invariants().is_valid(),
        "ADR-007 invariants must hold after non-convex slice:\n{}",
        mesh.verify_face_invariants().summary());
}

#[test]
fn trim_u_prism_nonconvex_keep_below() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_u_prism(&mut mesh, m, 10.0);
    let plane = SlicePlane::new(DVec3::new(0.0, 15.0, 0.0), DVec3::Y).unwrap();
    // Keep below (y<15) = the connected U-base; discard the 2 prong tips.
    let kept = mesh.trim_volume_by_plane(&faces, plane, /*keep_above*/ false, m)
        .expect("non-convex trim keep-below should succeed");
    assert_eq!(mesh.face_set_manifold_info(&kept).boundary_edge_count, 0,
        "kept lower half must be closed");
    // Every active face sits on/below y=15 (prong tips removed).
    for (fid, f) in mesh.faces.iter() {
        if !f.is_active() { continue; }
        for v in mesh.collect_loop_verts(f.outer().start).unwrap() {
            let y = mesh.verts.get(v).unwrap().pos().y;
            assert!(y < 16.0, "face {:?} vert above cut (y={}) — prong tip not removed", fid, y);
        }
    }
    assert!(mesh.verify_face_invariants().is_valid());
}

// ADR-242 C1 adversarial-review hardening: thin-chamber perpendicular-nudge
// false-negative concern (workflow wf_cf5a3086). A narrow U-channel / multi-
// tooth comb crossed through its arms exercises the interior-chord test where
// the perp nudge could (per the concern) land in an empty chamber. These prove
// the t-adjacent cut-segment pairing + perp nudge correctly distinguishes real
// cut segments (arms) from exterior gaps (chambers).

/// Narrow U-channel prism (chamber x∈[4,6], opening +Y), cut HORIZONTALLY at
/// y=10 → each cap crosses both arms = 4 On verts (complex, two cut loops).
#[test]
fn slice_u_channel_through_arms_4on() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let fp = [
        (0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (6.0, 20.0),
        (6.0, 5.0), (4.0, 5.0), (4.0, 20.0), (0.0, 20.0),
    ];
    let faces = make_prism(&mut mesh, m, &fp, 8.0);
    assert!(mesh.face_set_manifold_info(&faces).is_closed_solid);

    let plane = SlicePlane::new(DVec3::new(0.0, 10.0, 0.0), DVec3::Y).unwrap();
    let r = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("U-channel arms slice must succeed (perp nudge skips the chamber gap)");
    assert_eq!(r.cut_loops.len(), 2, "two arms → two cut loops (chamber gap skipped)");
    assert_eq!(r.cap_above.len(), 2);
    assert_eq!(r.cap_below.len(), 2);
    let above: Vec<_> = r.above_walls.iter().chain(r.cap_above.iter()).copied().collect();
    let below: Vec<_> = r.below_walls.iter().chain(r.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0);
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0);
    assert!(mesh.verify_face_invariants().is_valid());
}

/// Same U-channel, cut VERTICALLY at x=5 → each cap crosses in ONE segment =
/// 2 On verts (the convex path). This is the exact shape an adversarial
/// reviewer flagged as a "bug"; it is in fact the convex path and slices fine.
#[test]
fn slice_u_channel_vertical_2on_convex() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let fp = [
        (0.0, 0.0), (10.0, 0.0), (10.0, 20.0), (6.0, 20.0),
        (6.0, 5.0), (4.0, 5.0), (4.0, 20.0), (0.0, 20.0),
    ];
    let faces = make_prism(&mut mesh, m, &fp, 8.0);
    // x=5 crosses each cap at (5,0) and (5,5) only → 2 On → convex path.
    let plane = SlicePlane::new(DVec3::new(5.0, 0.0, 0.0), DVec3::X).unwrap();
    let r = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("U-channel vertical cut is the convex (2-On) path and must succeed");
    assert_eq!(r.cut_loops.len(), 1, "single connected cross-section → one loop");
    let above: Vec<_> = r.above_walls.iter().chain(r.cap_above.iter()).copied().collect();
    let below: Vec<_> = r.below_walls.iter().chain(r.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0);
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0);
    assert!(mesh.verify_face_invariants().is_valid());
}

/// 3-tooth comb prism, cut HORIZONTALLY through all teeth → 6 On verts per cap
/// (complex, three cut loops). Stresses multiple cut segments per face.
#[test]
fn slice_three_tooth_comb_6on() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    // Teeth x∈[25,30],[12.5,17.5],[0,5] up to y=20; base y∈[0,10]; gaps between.
    let fp = [
        (0.0, 0.0), (30.0, 0.0), (30.0, 20.0), (25.0, 20.0),
        (25.0, 10.0), (17.5, 10.0), (17.5, 20.0), (12.5, 20.0),
        (12.5, 10.0), (5.0, 10.0), (5.0, 20.0), (0.0, 20.0),
    ];
    let faces = make_prism(&mut mesh, m, &fp, 8.0);
    assert!(mesh.face_set_manifold_info(&faces).is_closed_solid);

    let plane = SlicePlane::new(DVec3::new(0.0, 15.0, 0.0), DVec3::Y).unwrap();
    let r = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("comb slice through all teeth must succeed");
    assert_eq!(r.cut_loops.len(), 3, "three teeth → three cut loops");
    assert_eq!(r.cap_above.len(), 3);
    let above: Vec<_> = r.above_walls.iter().chain(r.cap_above.iter()).copied().collect();
    let below: Vec<_> = r.below_walls.iter().chain(r.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0);
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0);
    assert!(mesh.verify_face_invariants().is_valid());
}

// ── ADR-243 Phase 1 C2 — slice solids WITH HOLES (Tier A: hole one side) ──

/// Box x,y∈[−10,10] z∈[0,10] with a square BLIND POCKET carved into the top
/// face (opening x,y∈[−4,4] at z=10, floor at z=6). The top face is HOLED
/// (outer square + inner pocket-opening loop). Returns all 11 face ids; the
/// holed top face is index 5. A closed genus-0 solid (sphere surface; the
/// annular top face makes V−E+F = 3, not 2).
fn make_box_with_top_pocket(mesh: &mut Mesh, m: MaterialId) -> Vec<axia_geo::FaceId> {
    let b00 = mesh.add_vertex(DVec3::new(-10.0, -10.0, 0.0));
    let b10 = mesh.add_vertex(DVec3::new( 10.0, -10.0, 0.0));
    let b11 = mesh.add_vertex(DVec3::new( 10.0,  10.0, 0.0));
    let b01 = mesh.add_vertex(DVec3::new(-10.0,  10.0, 0.0));
    let t00 = mesh.add_vertex(DVec3::new(-10.0, -10.0, 10.0));
    let t10 = mesh.add_vertex(DVec3::new( 10.0, -10.0, 10.0));
    let t11 = mesh.add_vertex(DVec3::new( 10.0,  10.0, 10.0));
    let t01 = mesh.add_vertex(DVec3::new(-10.0,  10.0, 10.0));
    // Pocket opening (z=10) + floor (z=6).
    let p00 = mesh.add_vertex(DVec3::new(-4.0, -4.0, 10.0));
    let p10 = mesh.add_vertex(DVec3::new( 4.0, -4.0, 10.0));
    let p11 = mesh.add_vertex(DVec3::new( 4.0,  4.0, 10.0));
    let p01 = mesh.add_vertex(DVec3::new(-4.0,  4.0, 10.0));
    let f00 = mesh.add_vertex(DVec3::new(-4.0, -4.0, 6.0));
    let f10 = mesh.add_vertex(DVec3::new( 4.0, -4.0, 6.0));
    let f11 = mesh.add_vertex(DVec3::new( 4.0,  4.0, 6.0));
    let f01 = mesh.add_vertex(DVec3::new(-4.0,  4.0, 6.0));

    let mut faces = Vec::new();
    // 0 bottom (−Z), 1-4 box walls (make_cube winding), then holed top.
    faces.push(mesh.add_face(&[b00, b01, b11, b10], m).unwrap()); // bottom
    faces.push(mesh.add_face(&[b00, b10, t10, t00], m).unwrap()); // front y=−10
    faces.push(mesh.add_face(&[b01, t01, t11, b11], m).unwrap()); // back  y=+10
    faces.push(mesh.add_face(&[b00, t00, t01, b01], m).unwrap()); // left  x=−10
    faces.push(mesh.add_face(&[b10, b11, t11, t10], m).unwrap()); // right x=+10
    // 5 — TOP holed (+Z) : outer CCW, hole CW (opposite).
    faces.push(mesh.add_face_with_holes(
        &[t00, t10, t11, t01],
        &[&[p00, p01, p11, p10]],
        m,
    ).unwrap());
    // 6-9 — pocket walls (normal points INTO the cavity).
    faces.push(mesh.add_face(&[f10, f00, p00, p10], m).unwrap()); // y=−4 (+Y)
    faces.push(mesh.add_face(&[f01, f11, p11, p01], m).unwrap()); // y=+4 (−Y)
    faces.push(mesh.add_face(&[f00, f01, p01, p00], m).unwrap()); // x=−4 (+X)
    faces.push(mesh.add_face(&[f11, f10, p10, p11], m).unwrap()); // x=+4 (−X)
    // 10 — pocket floor (+Z, up into cavity).
    faces.push(mesh.add_face(&[f00, f10, f11, f01], m).unwrap());
    faces
}

#[test]
fn slice_box_with_pocket_clear_of_hole_preserves_hole() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_box_with_top_pocket(&mut mesh, m);
    assert_eq!(mesh.face_set_manifold_info(&faces).boundary_edge_count, 0,
        "box-with-pocket must be a closed solid initially");
    let top = faces[5];
    assert_eq!(mesh.faces[top].inners().len(), 1, "top face starts holed");

    // Horizontal cut at z=3 — BELOW the pocket (floor z=6). The holed top face
    // is strictly above (z=10); only the 4 box walls cross. Tier A.
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 3.0), DVec3::Z).unwrap();
    let r = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("Tier A slice (cut clear of hole) must succeed");

    assert_eq!(r.cut_loops.len(), 1, "single box-outline cut loop");
    // The holed top face is untouched (strictly above) → still in the above
    // half AND still holed (hole preserved with zero special handling).
    assert!(r.above_walls.contains(&top), "holed top face stays in above half");
    assert_eq!(mesh.faces[top].inners().len(), 1, "hole preserved after slice");

    let above: Vec<_> = r.above_walls.iter().chain(r.cap_above.iter()).copied().collect();
    let below: Vec<_> = r.below_walls.iter().chain(r.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0,
        "above half (with the holed top + pocket) must be closed");
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0,
        "below half must be closed");
    assert!(mesh.verify_face_invariants().is_valid(),
        "invariants after holed-solid slice:\n{}",
        mesh.verify_face_invariants().summary());
}

/// ADR-243 C2 Tier B — convex-crossed holed face with the hole strictly on one
/// side: split the outer + reassign the hole to the containing sub-face. The
/// vertical cut x=7 crosses the holed top's OUTER but the pocket (x∈[−4,4]) is
/// wholly on the x<7 (below) side → the hole moves to the below half + survives
/// step 5.5's detach rebuild.
#[test]
fn slice_box_with_pocket_vertical_tier_b_reassigns_hole() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_box_with_top_pocket(&mut mesh, m);
    let plane = SlicePlane::new(DVec3::new(7.0, 0.0, 0.0), DVec3::X).unwrap();
    let r = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("Tier B vertical holed crossing (hole one side) must succeed");
    assert_eq!(r.cut_loops.len(), 1, "single rectangular cut loop (hole not crossed)");

    let above: Vec<_> = r.above_walls.iter().chain(r.cap_above.iter()).copied().collect();
    let below: Vec<_> = r.below_walls.iter().chain(r.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0, "above half closed");
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0, "below half closed");
    // The pocket hole was reassigned to the below half and survived step 5.5.
    let below_inners: usize = below.iter()
        .filter_map(|&f| mesh.faces.get(f)).map(|f| f.inners().len()).sum();
    assert_eq!(below_inners, 1, "pocket hole preserved on the below (x<7) half");
    let above_inners: usize = above.iter()
        .filter_map(|&f| mesh.faces.get(f)).map(|f| f.inners().len()).sum();
    assert_eq!(above_inners, 0, "above (x>7) half has no hole (cut clear of pocket)");
    assert!(mesh.verify_face_invariants().is_valid(),
        "invariants after Tier B slice:\n{}", mesh.verify_face_invariants().summary());
}

/// ADR-243 C2 Tier B — twin of the above, but the pocket lands on the ABOVE
/// side (cut x=−7, pocket x∈[−4,4] is x>−7). The hole is reassigned to the above
/// sub-face, which step 5.5 leaves untouched → hole survives on the above half.
#[test]
fn slice_box_with_pocket_vertical_hole_above() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_box_with_top_pocket(&mut mesh, m);
    let plane = SlicePlane::new(DVec3::new(-7.0, 0.0, 0.0), DVec3::X).unwrap();
    let r = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("Tier B (hole on above side) must succeed");
    assert_eq!(r.cut_loops.len(), 1);
    let above: Vec<_> = r.above_walls.iter().chain(r.cap_above.iter()).copied().collect();
    let below: Vec<_> = r.below_walls.iter().chain(r.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0, "above half closed");
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0, "below half closed");
    let above_inners: usize = above.iter()
        .filter_map(|&f| mesh.faces.get(f)).map(|f| f.inners().len()).sum();
    let below_inners: usize = below.iter()
        .filter_map(|&f| mesh.faces.get(f)).map(|f| f.inners().len()).sum();
    assert_eq!(above_inners, 1, "pocket hole preserved on the above (x>−7) half");
    assert_eq!(below_inners, 0, "below (x<−7) half has no hole");
    assert!(mesh.verify_face_invariants().is_valid());
}

/// ADR-243 C2 Tier C guard — the cut x=0 crosses the holed top's outer AND the
/// pocket hole (x∈[−4,4]) → a crossed inner loop = annular cross-section. Tier B
/// detects the crossed inner and bails (Tier C unsupported), not a silent corruption.
#[test]
fn slice_through_holed_face_bails_tier_bc() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_box_with_top_pocket(&mut mesh, m);
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 0.0), DVec3::X).unwrap();
    let err = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect_err("slicing through the hole (crossed inner) must bail (Tier C unsupported)");
    let msg = format!("{err}");
    assert!(msg.contains("hole") && (msg.contains("annular") || msg.contains("Tier C")),
        "bail must explain the crossed-hole/annular limitation, got: {msg}");
}

#[test]
fn trim_box_with_pocket_keep_above_preserves_hole() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_box_with_top_pocket(&mut mesh, m);
    let top = faces[5];
    // Keep the upper half (with the pocket); cut z=3 clear of the hole.
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 3.0), DVec3::Z).unwrap();
    let kept = mesh.trim_volume_by_plane(&faces, plane, /*keep_above*/ true, m)
        .expect("Tier A trim keep-above must succeed");
    assert_eq!(mesh.face_set_manifold_info(&kept).boundary_edge_count, 0,
        "kept upper half must be closed");
    assert!(kept.contains(&top) && mesh.faces[top].inners().len() == 1,
        "kept upper half retains the holed top face with its hole");
    assert!(mesh.verify_face_invariants().is_valid());
}

// ── ADR-241 Phase 1 C5 — polygonal TRIM (keep one half) ──────────────────

#[test]
fn trim_cube_keep_above_leaves_upper_closed_half() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);
    let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();

    let kept = mesh.trim_volume_by_plane(&faces, plane, /*keep_above*/ true, m)
        .expect("trim keep-above should succeed");

    // Upper half = 4 split side sub-faces + original top + 1 cap = 6 faces.
    assert_eq!(kept.len(), 6, "kept upper half must have 6 faces");
    // Kept half is a closed Wall solid.
    assert!(mesh.face_set_manifold_info(&kept).is_closed_solid,
        "kept upper half must be a closed solid");
    for &fid in &kept {
        assert!(mesh.is_face_in_volume(fid), "kept face {:?} must be a Wall", fid);
    }
    // The discarded lower half is gone: every active face sits on/above z=0.
    for (fid, f) in mesh.faces.iter() {
        if !f.is_active() { continue; }
        let start = f.outer().start;
        for v in mesh.collect_loop_verts(start).unwrap() {
            let z = mesh.verts.get(v).unwrap().pos().z;
            assert!(z > -1.0, "face {:?} vert below cut plane (z={}) — lower half not removed", fid, z);
        }
    }
    // Global invariants hold.
    assert!(mesh.verify_face_invariants().is_valid(),
        "ADR-007 invariants must hold after trim");
}

#[test]
fn trim_cube_keep_below_leaves_lower_closed_half() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_cube(&mut mesh, m, 1000.0);
    let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();

    let kept = mesh.trim_volume_by_plane(&faces, plane, /*keep_above*/ false, m)
        .expect("trim keep-below should succeed");

    assert_eq!(kept.len(), 6, "kept lower half must have 6 faces");
    assert!(mesh.face_set_manifold_info(&kept).is_closed_solid,
        "kept lower half must be a closed solid");
    // Every active face sits on/below z=0.
    for (fid, f) in mesh.faces.iter() {
        if !f.is_active() { continue; }
        let start = f.outer().start;
        for v in mesh.collect_loop_verts(start).unwrap() {
            let z = mesh.verts.get(v).unwrap().pos().z;
            assert!(z < 1.0, "face {:?} vert above cut plane (z={}) — upper half not removed", fid, z);
        }
    }
    assert!(mesh.verify_face_invariants().is_valid(),
        "ADR-007 invariants must hold after trim");
}

#[test]
fn test_claim_u_shaped_8vert_face() {
    // Test the exact scenario from the claim:
    // Non-convex U-shaped face with 8 vertices, 4 On vertices at [1, 3, 5, 7]
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    
    // Create the 8-vertex non-convex U-shaped face
    // P0=(0,0,0), P1=(1,0,0) [ON], P2=(2,0,0), P3=(3,0,0) [ON], 
    // P4=(4,0,0), P5=(3,1,0) [ON], P6=(2,1,0), P7=(1,1,0) [ON]
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0)); // ON
    let v2 = mesh.add_vertex(DVec3::new(2.0, 0.0, 0.0));
    let v3 = mesh.add_vertex(DVec3::new(3.0, 0.0, 0.0)); // ON
    let v4 = mesh.add_vertex(DVec3::new(4.0, 0.0, 0.0));
    let v5 = mesh.add_vertex(DVec3::new(3.0, 1.0, 0.0)); // ON
    let v6 = mesh.add_vertex(DVec3::new(2.0, 1.0, 0.0));
    let v7 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0)); // ON
    
    // Create face with those 8 vertices
    let bottom = mesh.add_face(&[v0, v1, v2, v3, v4, v5, v6, v7], m).unwrap();
    
    // Create top face (z=1) - same U-shape
    let h = 1.0;
    let v0_top = mesh.add_vertex(DVec3::new(0.0, 0.0, h));
    let v1_top = mesh.add_vertex(DVec3::new(1.0, 0.0, h));
    let v2_top = mesh.add_vertex(DVec3::new(2.0, 0.0, h));
    let v3_top = mesh.add_vertex(DVec3::new(3.0, 0.0, h));
    let v4_top = mesh.add_vertex(DVec3::new(4.0, 0.0, h));
    let v5_top = mesh.add_vertex(DVec3::new(3.0, 1.0, h));
    let v6_top = mesh.add_vertex(DVec3::new(2.0, 1.0, h));
    let v7_top = mesh.add_vertex(DVec3::new(1.0, 1.0, h));
    
    let top = mesh.add_face(&[v7_top, v6_top, v5_top, v4_top, v3_top, v2_top, v1_top, v0_top], m).unwrap();
    
    // Build side faces
    let mut faces_vec = vec![bottom, top];
    let verts = vec![v0, v1, v2, v3, v4, v5, v6, v7];
    let verts_top = vec![v0_top, v1_top, v2_top, v3_top, v4_top, v5_top, v6_top, v7_top];
    for i in 0..8 {
        let j = (i + 1) % 8;
        let side = mesh.add_face(&[verts[i], verts[j], verts_top[j], verts_top[i]], m).unwrap();
        faces_vec.push(side);
    }
    
    // Slice with horizontal plane at z=0.5
    // All vertices at z=0 are below (distance = -0.5)
    // All vertices at z=1 are above (distance = +0.5)
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 0.5), DVec3::Z).unwrap();
    
    // This should succeed - the algorithm should handle the non-convex crossing
    let result = mesh.slice_volume_by_plane(&faces_vec, plane, m)
        .expect("slice of non-convex U face should succeed");
    
    // Verify the result is valid
    assert!(result.cut_loops.len() > 0, "should have at least one cut loop");
    
    let above: Vec<_> = result.above_walls.iter().chain(result.cap_above.iter()).copied().collect();
    let below: Vec<_> = result.below_walls.iter().chain(result.cap_below.iter()).copied().collect();
    assert!(mesh.face_set_manifold_info(&above).is_closed_solid, "above half must be closed");
    assert!(mesh.face_set_manifold_info(&below).is_closed_solid, "below half must be closed");
}

#[test]
fn test_claim_debug_probe_function() {
    // Debug test to understand what the probe function does
    // for the claim's scenario
    use axia_geo::FaceId;
    
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    
    // Same 8-vertex non-convex U-shaped face
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
    let v2 = mesh.add_vertex(DVec3::new(2.0, 0.0, 0.0));
    let v3 = mesh.add_vertex(DVec3::new(3.0, 0.0, 0.0));
    let v4 = mesh.add_vertex(DVec3::new(4.0, 0.0, 0.0));
    let v5 = mesh.add_vertex(DVec3::new(3.0, 1.0, 0.0));
    let v6 = mesh.add_vertex(DVec3::new(2.0, 1.0, 0.0));
    let v7 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
    
    let bottom = mesh.add_face(&[v0, v1, v2, v3, v4, v5, v6, v7], m).unwrap();
    
    // Top face
    let h = 1.0;
    let v0_top = mesh.add_vertex(DVec3::new(0.0, 0.0, h));
    let v1_top = mesh.add_vertex(DVec3::new(1.0, 0.0, h));
    let v2_top = mesh.add_vertex(DVec3::new(2.0, 0.0, h));
    let v3_top = mesh.add_vertex(DVec3::new(3.0, 0.0, h));
    let v4_top = mesh.add_vertex(DVec3::new(4.0, 0.0, h));
    let v5_top = mesh.add_vertex(DVec3::new(3.0, 1.0, h));
    let v6_top = mesh.add_vertex(DVec3::new(2.0, 1.0, h));
    let v7_top = mesh.add_vertex(DVec3::new(1.0, 1.0, h));
    
    let top = mesh.add_face(&[v7_top, v6_top, v5_top, v4_top, v3_top, v2_top, v1_top, v0_top], m).unwrap();
    
    // Sides
    let mut faces_vec = vec![bottom, top];
    let verts = vec![v0, v1, v2, v3, v4, v5, v6, v7];
    let verts_top = vec![v0_top, v1_top, v2_top, v3_top, v4_top, v5_top, v6_top, v7_top];
    for i in 0..8 {
        let j = (i + 1) % 8;
        let side = mesh.add_face(&[verts[i], verts[j], verts_top[j], verts_top[i]], m).unwrap();
        faces_vec.push(side);
    }
    
    // Test both crossing faces with plane at z=0.5
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 0.5), DVec3::Z).unwrap();
    
    // Manually check which vertices are ON
    for &fid in &[bottom, top] {
        let outer_start = mesh.faces[fid].outer().start;
        let loop_verts = mesh.collect_loop_verts(outer_start).unwrap();
        let mut on_indices = Vec::new();
        
        for (i, &v) in loop_verts.iter().enumerate() {
            let pos = mesh.verts.get(v).unwrap().pos();
            let d = plane.signed_distance(pos);
            if d.abs() < 1e-4 {
                on_indices.push(i);
                println!("Face {:?}: vertex {} (pos={:?}) is ON", fid, i, pos);
            }
        }
        
        println!("Face {:?}: on_indices = {:?}", fid, on_indices);
    }
    
    // The actual slice
    let result = mesh.slice_volume_by_plane(&faces_vec, plane, m).expect("should succeed");
    println!("Slice succeeded with {} cut loops", result.cut_loops.len());
}

#[test]
fn test_claim_with_on_vertices() {
    // Test the exact scenario with 4 ON vertices at [1, 3, 5, 7]
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    
    // Create vertices such that P1, P3, P5, P7 are ON the plane
    // Plane at z=0, so vertices with z=0 are ON
    // Let's create a prism where the bottom cap has 4 vertices at z=0 (ON)
    // and the other 4 at z=-1 (BELOW), and top vertices at z=1 (ABOVE)
    
    // Bottom U-shape cap at z=-1
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, -1.0));
    let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0)); // ON
    let v2 = mesh.add_vertex(DVec3::new(2.0, 0.0, -1.0));
    let v3 = mesh.add_vertex(DVec3::new(3.0, 0.0, 0.0)); // ON
    let v4 = mesh.add_vertex(DVec3::new(4.0, 0.0, -1.0));
    let v5 = mesh.add_vertex(DVec3::new(3.0, 1.0, 0.0)); // ON
    let v6 = mesh.add_vertex(DVec3::new(2.0, 1.0, -1.0));
    let v7 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0)); // ON
    
    // Bottom cap
    let bottom = mesh.add_face(&[v0, v1, v2, v3, v4, v5, v6, v7], m).unwrap();
    
    // Top cap (all at z=1, above)
    let v0_top = mesh.add_vertex(DVec3::new(0.0, 0.0, 1.0));
    let v1_top = mesh.add_vertex(DVec3::new(1.0, 0.0, 1.0));
    let v2_top = mesh.add_vertex(DVec3::new(2.0, 0.0, 1.0));
    let v3_top = mesh.add_vertex(DVec3::new(3.0, 0.0, 1.0));
    let v4_top = mesh.add_vertex(DVec3::new(4.0, 0.0, 1.0));
    let v5_top = mesh.add_vertex(DVec3::new(3.0, 1.0, 1.0));
    let v6_top = mesh.add_vertex(DVec3::new(2.0, 1.0, 1.0));
    let v7_top = mesh.add_vertex(DVec3::new(1.0, 1.0, 1.0));
    
    let top = mesh.add_face(&[v7_top, v6_top, v5_top, v4_top, v3_top, v2_top, v1_top, v0_top], m).unwrap();
    
    // Sides
    let mut faces_vec = vec![bottom, top];
    let verts = vec![v0, v1, v2, v3, v4, v5, v6, v7];
    let verts_top = vec![v0_top, v1_top, v2_top, v3_top, v4_top, v5_top, v6_top, v7_top];
    for i in 0..8 {
        let j = (i + 1) % 8;
        let side = mesh.add_face(&[verts[i], verts[j], verts_top[j], verts_top[i]], m).unwrap();
        faces_vec.push(side);
    }
    
    // Slice with plane at z=0 (normal = +Z)
    let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();
    
    // Verify on_indices before slice
    println!("\nBefore slice - bottom face vertices:");
    let outer_start = mesh.faces[bottom].outer().start;
    let loop_verts = mesh.collect_loop_verts(outer_start).unwrap();
    let mut on_indices = Vec::new();
    
    for (i, &v) in loop_verts.iter().enumerate() {
        let pos = mesh.verts.get(v).unwrap().pos();
        let d = plane.signed_distance(pos);
        let class = if d > 1e-4 { "ABOVE" } else if d < -1e-4 { "BELOW" } else { "ON" };
        println!("  Vertex {} (pos={:?}): distance={:.4}, class={}", i, pos, d, class);
        if class == "ON" {
            on_indices.push(i);
        }
    }
    
    println!("on_indices for bottom face: {:?}", on_indices);
    
    // The slice should succeed
    let result = mesh.slice_volume_by_plane(&faces_vec, plane, m)
        .expect("slice with on-vertices should succeed");
    
    println!("\nSlice succeeded!");
    println!("Cut loops: {}", result.cut_loops.len());
    for (i, loop_verts) in result.cut_loops.iter().enumerate() {
        println!("  Loop {}: {} vertices", i, loop_verts.len());
    }
    
    // Verify closure
    let above: Vec<_> = result.above_walls.iter().chain(result.cap_above.iter()).copied().collect();
    let below: Vec<_> = result.below_walls.iter().chain(result.cap_below.iter()).copied().collect();
    assert!(mesh.face_set_manifold_info(&above).is_closed_solid, "above must be closed");
    assert!(mesh.face_set_manifold_info(&below).is_closed_solid, "below must be closed");
}
/// ADR-245 C2 Tier C — slicing through a HOLE REGION (between pocket floor and
/// opening) crosses the non-holed pocket walls and yields an ANNULAR cross-
/// section (outer box loop + nested inner pocket loop). The loop-nesting
/// classifier groups them and step 6 seals each half with a single HOLED cap.
/// Box x,y∈[−10,10] z∈[0,10] with TWO square blind pockets in the top face
/// (centers ±5 on X, half-size 2, floor z=6). The top face has TWO holes.
fn make_box_with_two_pockets(mesh: &mut Mesh, m: MaterialId) -> Vec<axia_geo::FaceId> {
    let b00 = mesh.add_vertex(DVec3::new(-10.0, -10.0, 0.0));
    let b10 = mesh.add_vertex(DVec3::new( 10.0, -10.0, 0.0));
    let b11 = mesh.add_vertex(DVec3::new( 10.0,  10.0, 0.0));
    let b01 = mesh.add_vertex(DVec3::new(-10.0,  10.0, 0.0));
    let t00 = mesh.add_vertex(DVec3::new(-10.0, -10.0, 10.0));
    let t10 = mesh.add_vertex(DVec3::new( 10.0, -10.0, 10.0));
    let t11 = mesh.add_vertex(DVec3::new( 10.0,  10.0, 10.0));
    let t01 = mesh.add_vertex(DVec3::new(-10.0,  10.0, 10.0));
    let mut faces = Vec::new();
    faces.push(mesh.add_face(&[b00, b01, b11, b10], m).unwrap()); // bottom
    faces.push(mesh.add_face(&[b00, b10, t10, t00], m).unwrap());
    faces.push(mesh.add_face(&[b01, t01, t11, b11], m).unwrap());
    faces.push(mesh.add_face(&[b00, t00, t01, b01], m).unwrap());
    faces.push(mesh.add_face(&[b10, b11, t11, t10], m).unwrap());
    // Two pockets — collect opening loops for the holed top, add walls+floor.
    let mut holes: Vec<[axia_geo::VertId; 4]> = Vec::new();
    let mut pocket_faces: Vec<axia_geo::FaceId> = Vec::new();
    for &cx in &[-5.0f64, 5.0] {
        let hs = 2.0;
        let (cy, fz) = (0.0f64, 6.0f64);
        let p0 = mesh.add_vertex(DVec3::new(cx - hs, cy - hs, 10.0));
        let p1 = mesh.add_vertex(DVec3::new(cx + hs, cy - hs, 10.0));
        let p2 = mesh.add_vertex(DVec3::new(cx + hs, cy + hs, 10.0));
        let p3 = mesh.add_vertex(DVec3::new(cx - hs, cy + hs, 10.0));
        let f0 = mesh.add_vertex(DVec3::new(cx - hs, cy - hs, fz));
        let f1 = mesh.add_vertex(DVec3::new(cx + hs, cy - hs, fz));
        let f2 = mesh.add_vertex(DVec3::new(cx + hs, cy + hs, fz));
        let f3 = mesh.add_vertex(DVec3::new(cx - hs, cy + hs, fz));
        holes.push([p0, p3, p2, p1]); // CW hole (opposite outer CCW)
        pocket_faces.push(mesh.add_face(&[f1, f0, p0, p1], m).unwrap()); // walls
        pocket_faces.push(mesh.add_face(&[f3, f2, p2, p3], m).unwrap());
        pocket_faces.push(mesh.add_face(&[f0, f3, p3, p0], m).unwrap());
        pocket_faces.push(mesh.add_face(&[f2, f1, p1, p2], m).unwrap());
        pocket_faces.push(mesh.add_face(&[f0, f1, f2, f3], m).unwrap()); // floor
    }
    // Holed top with TWO inner loops.
    let hole_refs: Vec<&[axia_geo::VertId]> = holes.iter().map(|h| h.as_slice()).collect();
    faces.push(mesh.add_face_with_holes(&[t00, t10, t11, t01], &hole_refs, m).unwrap());
    faces.extend(pocket_faces);
    faces
}

/// ADR-245 C2 Tier C (multi-hole) — a cut through TWO pocket regions yields one
/// outer loop with TWO nested hole loops → one group, holed cap with 2 inners.
#[test]
fn slice_through_two_hole_regions_multi_hole_cap() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_box_with_two_pockets(&mut mesh, m);
    assert_eq!(mesh.face_set_manifold_info(&faces).boundary_edge_count, 0,
        "two-pocket box must be closed initially");
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 8.0), DVec3::Z).unwrap();
    let r = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("Tier C multi-hole annular must succeed");
    assert_eq!(r.cut_loops.len(), 3, "outer box loop + two pocket loops");
    assert_eq!(r.cap_above.len(), 1, "single group → single cap");
    assert_eq!(mesh.faces[r.cap_above[0]].inners().len(), 2, "above cap has TWO holes");
    assert_eq!(mesh.faces[r.cap_below[0]].inners().len(), 2, "below cap has TWO holes");
    let above: Vec<_> = r.above_walls.iter().chain(r.cap_above.iter()).copied().collect();
    let below: Vec<_> = r.below_walls.iter().chain(r.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0, "above closed");
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0, "below closed");
    assert!(mesh.verify_face_invariants().is_valid());
}

#[test]
fn slice_through_hole_region_annular_caps() {
    let mut mesh = Mesh::new();
    let m = MaterialId::new(0);
    let faces = make_box_with_top_pocket(&mut mesh, m);
    let plane = SlicePlane::new(DVec3::new(0.0, 0.0, 8.0), DVec3::Z).unwrap();
    let r = mesh.slice_volume_by_plane(&faces, plane, m)
        .expect("Tier C annular cross-section must succeed with a holed cap");
    assert_eq!(r.cut_loops.len(), 2, "outer box loop + inner pocket loop");
    assert_eq!(r.cap_above.len(), 1, "single holed cap (one nesting group)");
    assert_eq!(r.cap_below.len(), 1);
    // Both caps are annular (one hole each).
    assert_eq!(mesh.faces[r.cap_above[0]].inners().len(), 1, "above cap is annular");
    assert_eq!(mesh.faces[r.cap_below[0]].inners().len(), 1, "below cap is annular");
    let above: Vec<_> = r.above_walls.iter().chain(r.cap_above.iter()).copied().collect();
    let below: Vec<_> = r.below_walls.iter().chain(r.cap_below.iter()).copied().collect();
    assert_eq!(mesh.face_set_manifold_info(&above).boundary_edge_count, 0, "above half closed");
    assert_eq!(mesh.face_set_manifold_info(&below).boundary_edge_count, 0, "below half closed");
    assert!(mesh.verify_face_invariants().is_valid(),
        "invariants after annular slice:\n{}", mesh.verify_face_invariants().summary());
}

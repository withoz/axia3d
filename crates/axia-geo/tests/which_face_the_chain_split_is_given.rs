//! Measuring the face id, rather than reasoning about it.
//!
//! The C-2 materializer kept being refused with
//!
//! ```text
//!   split_face_by_chain: chain start vert 50 not on any loop of face 4
//! ```
//!
//! while `the_cap_rim_is_also_the_hosts_hole` says the rim belongs to face 2.
//! Three things could produce that and only one of them is true, so this walks
//! the sequence and prints what each step leaves behind:
//!
//!   1. `raw()` is not the index, and "face 4" is not FaceId(4)
//!   2. splitting a rim edge replaces the host, so the caller's id goes stale
//!   3. the vertex genuinely is not on the host, twin half-edges notwithstanding
//!
//! Nothing here fixes anything. It is the measurement the fix has to stand on.

use axia_geo::surfaces::{cylinder, AnalyticSurface};
use axia_geo::{FaceId, MaterialId, Mesh, VertId};
use glam::DVec3;

const R: f64 = 10.0;

fn geodesic_circle(u0: f64, v0: f64, r: f64, n: usize) -> Vec<DVec3> {
    (0..n)
        .map(|i| {
            let t = std::f64::consts::TAU * (i as f64) / (n as f64);
            cylinder::evaluate(
                DVec3::ZERO,
                DVec3::Z,
                R,
                DVec3::X,
                u0 + (r * t.cos()) / R,
                v0 + r * t.sin(),
            )
        })
        .collect()
}

/// Every active face, and which loops it has.
fn shape(mesh: &Mesh) -> Vec<(u32, Vec<usize>)> {
    mesh.faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, f)| {
            let mut loops = vec![mesh.collect_loop_verts(f.outer().start).map(|v| v.len()).unwrap_or(0)];
            for l in f.inners() {
                loops.push(mesh.collect_loop_verts(l.start).map(|v| v.len()).unwrap_or(0));
            }
            (fid.raw(), loops)
        })
        .collect()
}

/// Which active faces have `v` on one of their loops.
fn faces_carrying(mesh: &Mesh, v: VertId) -> Vec<u32> {
    mesh.faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .filter(|(_, f)| {
            let on = |s| mesh.collect_loop_verts(s).map(|vs| vs.contains(&v)).unwrap_or(false);
            on(f.outer().start) || f.inners().iter().any(|l| on(l.start))
        })
        .map(|(fid, _)| fid.raw())
        .collect()
}

#[test]
fn splitting_a_rim_edge_and_who_still_owns_it() {
    let mut mesh = Mesh::new();
    mesh.cylinder_path_b_default = true;
    mesh.create_cylinder(DVec3::ZERO, R, 20.0, 16, MaterialId::new(0)).expect("cylinder");
    let side = mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface().is_some_and(|s| !matches!(s, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .expect("wall");
    println!("STEP 0 wall={} faces={:?}", side.raw(), shape(&mesh));

    let a = geodesic_circle(std::f64::consts::PI, 10.0, 3.0, 48);
    let (cap, host) = mesh.split_cylinder_face_by_circle(side, &a).expect("split");
    println!(
        "STEP 1 cap={} host={} faces={:?}",
        cap.raw(),
        host.raw(),
        shape(&mesh)
    );

    // CLAIM 1 — is the id the index?
    assert_eq!(
        FaceId::new(host.raw()),
        host,
        "raw() round-trips, so the number in an error message IS the face"
    );

    // Take a rim edge and split it in the middle, exactly as the materializer
    // does before it cuts.
    let rim = mesh
        .collect_loop_verts(mesh.faces.get(cap).unwrap().outer().start)
        .expect("rim");
    let (v0, v1) = (rim[0], rim[1]);
    let mid = 0.5 * (mesh.vertex_pos(v0).unwrap() + mesh.vertex_pos(v1).unwrap());
    let e = mesh.find_edge(v0, v1).expect("rim edge");
    let carried_before = (faces_carrying(&mesh, v0), faces_carrying(&mesh, v1));
    let (vm, _, _) = mesh.split_edge(e, mid).expect("split the rim edge");
    println!(
        "STEP 2 split rim -> vert {}; faces={:?}",
        vm.raw(),
        shape(&mesh)
    );
    println!(
        "       v0 was on faces {:?}, v1 on {:?}; the new vert is on {:?}",
        carried_before.0,
        carried_before.1,
        faces_carrying(&mesh, vm)
    );

    // CLAIM 2 — did the host survive the edge split with the same id?
    assert!(
        mesh.faces.get(host).is_some_and(|f| f.is_active()),
        "TODAY the host is still face {} after a rim edge is split",
        host.raw()
    );
    assert!(
        mesh.faces.get(cap).is_some_and(|f| f.is_active()),
        "and so is the cap"
    );

    // CLAIM 3 — the new vertex is on BOTH, because the edge is shared through
    // twin half-edges. This is what the materializer needs and what the refusal
    // said was not so.
    let on = faces_carrying(&mesh, vm);
    assert!(
        on.contains(&cap.raw()),
        "the split vertex is on the cap ({on:?})"
    );
    assert!(
        on.contains(&host.raw()),
        "and on the host — {on:?} does not include {}",
        host.raw()
    );
}

/// The materializer's own sequence, step by step, printing what each one leaves.
///
/// The single-edge test above rules out every explanation that does not involve
/// something the materializer itself does: the id round-trips, the host keeps
/// its id through an edge split, and the new vertex lands on both faces. So the
/// face the refusal names must be one this sequence CREATES.
#[test]
fn what_the_materializers_own_steps_leave_behind() {
    use axia_geo::operations::curved_arrange::crossing_on_cylinder;
    use axia_geo::operations::face_split::split_face_by_chain;

    let mut mesh = Mesh::new();
    mesh.cylinder_path_b_default = true;
    mesh.create_cylinder(DVec3::ZERO, R, 20.0, 16, MaterialId::new(0)).expect("cylinder");
    let side = mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface().is_some_and(|s| !matches!(s, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .expect("wall");
    let (r, d) = (3.0_f64, 3.6_f64);
    let a = geodesic_circle(std::f64::consts::PI, 10.0, r, 48);
    let b = geodesic_circle(std::f64::consts::PI + d / R, 10.0, r, 48);
    let (cap, host) = mesh.split_cylinder_face_by_circle(side, &a).expect("split");

    let surface = mesh.face_surface(host).expect("surface").clone();
    let loop_a: Vec<DVec3> = mesh
        .collect_loop_verts(mesh.faces.get(cap).unwrap().outer().start)
        .expect("rim")
        .iter()
        .map(|&v| mesh.vertex_pos(v).unwrap())
        .collect();
    let x = crossing_on_cylinder(&surface, &loop_a, &b).expect("they cross");

    // (1) the two crossings become vertices on the rim
    let rim: Vec<VertId> = mesh
        .collect_loop_verts(mesh.faces.get(cap).unwrap().outer().start)
        .expect("rim");
    let m = rim.len();
    let mut split_at = |mesh: &mut Mesh, k: usize| -> VertId {
        let (va, vb) = (rim[x.seg[k] % m], rim[(x.seg[k] + 1) % m]);
        let e = mesh.find_edge(va, vb).expect("rim edge");
        mesh.split_edge(e, x.points[k]).expect("split").0
    };
    let p0 = split_at(&mut mesh, 0);
    let p1 = split_at(&mut mesh, 1);
    println!(
        "MAT 0 split_cylinder_face_by_circle returned cap={} host={}",
        cap.raw(),
        host.raw()
    );
    println!("MAT 1 crossings -> verts {} {}; faces={:?}", p0.raw(), p1.raw(), shape(&mesh));
    println!("      p0 on faces {:?}, p1 on {:?}", faces_carrying(&mesh, p0), faces_carrying(&mesh, p1));

    // (2) the chains get their own vertices and edges
    let mut chain_of = |mesh: &mut Mesh, pts: &[DVec3], first: VertId, last: VertId| -> Vec<VertId> {
        let mut vs = vec![first];
        for &p in &pts[1..pts.len() - 1] {
            vs.push(mesh.add_vertex(p));
        }
        vs.push(last);
        vs.dedup();
        for w in vs.windows(2) {
            if w[0] != w[1] {
                let _ = mesh.add_edge(w[0], w[1]);
            }
        }
        vs
    };
    let outside = chain_of(&mut mesh, &x.outside, p0, p1);
    println!("MAT 2 outside chain {} verts; faces={:?}", outside.len(), shape(&mesh));
    let inside = chain_of(&mut mesh, &x.inside, p0, p1);
    println!("MAT 3 inside chain {} verts; faces={:?}", inside.len(), shape(&mesh));
    println!("      chain start {} is on faces {:?}", outside[0].raw(), faces_carrying(&mesh, outside[0]));

    // (3) the cut that was refused — with the state captured BEFORE it, because
    // that turned out to matter.
    let outer_before = mesh
        .collect_loop_verts(mesh.faces.get(host).unwrap().outer().start)
        .unwrap()
        .len();
    let snap_before = mesh.snapshot();
    let attempt = split_face_by_chain(&mut mesh, host, &outside, MaterialId::new(0));
    match &attempt {
        Ok(res) => println!("MAT 4 host split OK -> new faces {:?}", res.new_faces),
        Err(e) => println!("MAT 4 host split REFUSED: {e}"),
    }

    // ── The answer ────────────────────────────────────────────────────────
    //
    // Nothing this sequence does is wrong. `split_face_by_chain` USED TO open
    // with an unconditional
    //
    //     let face_id = polygonize_if_closed_curve(mesh, face_id)?;
    //
    // and the host is a Path B band: its OUTER loop is ONE vertex and a
    // self-loop circle (ADR-089 Phase 2). So the split polygonised it, got a
    // NEW face back, and looked for the chain on that one — while the chain's
    // vertices belong to the face that was just replaced. Hence a refusal
    // naming face 4 when the caller passed face 2, and hence "vert 50 not on
    // any loop", which was true of the new face and false of the old.
    //
    // And worse than a wrong id: polygonising KEEPS ONLY THE OUTER LOOP,
    // measured `[1, 1, 48] -> [23]` (`polygonize_keeps_or_drops_the_holes`).
    // So "polygonise the host first" was never a route either — it would have
    // dropped the cap's hole and the top rim.
    //
    // The fix was to stop transforming a face that did not need it: the chain
    // is looked for FIRST, and only a chain with nowhere to live triggers the
    // polygonisation. The outer loop also only has to be a polygon when it is
    // the loop being cut.
    //
    // That uncovered the real gap — cutting a face ACROSS ONE OF ITS HOLES,
    // which the engine had described and refused since 2026-04-28. Built
    // (`split_face_across_hole`), then met one more wall: the piece that KEEPS
    // the outer loop was rebuilt with `add_face_with_holes`, and a Path B
    // band's outer loop is one vertex and a self-loop circle. The answer was
    // to stop rebuilding — `splice_face_across_hole` reassigns half-edges
    // instead, so nothing has to be expressible as a vertex list.
    //
    // The cut this file was written to explain now LANDS. What it still
    // records is the route, because every step of it was a wrong turn caught
    // by a measurement rather than by reasoning.
    assert_eq!(
        outer_before, 1,
        "the host's outer loop is one vertex — what used to trigger the \
         polygonisation, and no longer does"
    );
    let cut = attempt.expect("the host is cut across its hole");
    assert_eq!(cut.new_faces.len(), 1, "the lune comes out as one new face");
    let v = mesh.verify_face_invariants();
    assert!(v.is_valid(), "and the mesh is sound — {:?}", v.violations);

    // ── And the refusal is CLEAN, which it was not when this was written ──
    //
    // The polygonisation runs before the split has checked anything, so a
    // refusal used to hand the caller both an error and a different mesh:
    //
    //     before   faces [(0,[1]), (1,[1]), (2,[1,1,50]), (3,[50])]
    //     after    faces [(0,[0]), (1,[1]),               (3,[50]), (4,[23])]
    //
    // The host gone, a 23-vertex face in its place, and face 0 left with a
    // boundary loop of NO vertices. Same shape as the "refused but already
    // mutated" defect ADR-298 corrected elsewhere. `split_face_by_chain` and
    // `split_face_by_line` now snapshot before polygonising and restore on any
    // error — and only when polygonising would actually fire, so the ordinary
    // path pays nothing.
    //
    // The cut SUCCEEDS now, so this no longer watches a refusal — it watches
    // that the host survives its own splitting with its identity intact, which
    // is the whole reason the splice exists. (`snap_before` is kept because
    // the byte-identical claim still belongs to the refusal path, exercised in
    // `a_refused_line_split_also_leaves_the_mesh_alone`.)
    let _ = &snap_before;
    println!(
        "MAT 5 after the cut: host still there? {}; faces={:?}",
        mesh.faces.get(host).is_some_and(|f| f.is_active()),
        shape(&mesh)
    );
    assert!(
        mesh.faces.get(host).is_some_and(|f| f.is_active()),
        "the host keeps its id through the cut — nothing is rebuilt"
    );
    let empty: Vec<u32> = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .filter(|(_, f)| {
            mesh.collect_loop_verts(f.outer().start).map(|v| v.is_empty()).unwrap_or(true)
        })
        .map(|(fid, _)| fid.raw())
        .collect();
    assert!(
        empty.is_empty(),
        "and no face is left with an empty boundary loop ({empty:?})"
    );

}

/// The other half of the asymmetry: the CAP cuts cleanly, because its boundary
/// is a 48-gon and nothing is polygonised on the way in.
///
/// That is the route. Whatever fixes the host has to polygonise it BEFORE the
/// crossings and chains are built, so they are made on the face that survives.
#[test]
fn the_cap_cuts_cleanly_because_its_boundary_is_already_a_polygon() {
    use axia_geo::operations::curved_arrange::crossing_on_cylinder;
    use axia_geo::operations::face_split::split_face_by_chain;

    let mut mesh = Mesh::new();
    mesh.cylinder_path_b_default = true;
    mesh.create_cylinder(DVec3::ZERO, R, 20.0, 16, MaterialId::new(0)).expect("cylinder");
    let side = mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface().is_some_and(|s| !matches!(s, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .expect("wall");
    let (r, d) = (3.0_f64, 3.6_f64);
    let a = geodesic_circle(std::f64::consts::PI, 10.0, r, 48);
    let b = geodesic_circle(std::f64::consts::PI + d / R, 10.0, r, 48);
    let (cap, host) = mesh.split_cylinder_face_by_circle(side, &a).expect("split");
    let surface = mesh.face_surface(host).expect("surface").clone();
    let loop_a: Vec<DVec3> = mesh
        .collect_loop_verts(mesh.faces.get(cap).unwrap().outer().start)
        .expect("rim")
        .iter()
        .map(|&v| mesh.vertex_pos(v).unwrap())
        .collect();
    let x = crossing_on_cylinder(&surface, &loop_a, &b).expect("cross");

    let rim: Vec<VertId> = mesh
        .collect_loop_verts(mesh.faces.get(cap).unwrap().outer().start)
        .expect("rim");
    let m = rim.len();
    let mut split_at = |mesh: &mut Mesh, k: usize| -> VertId {
        let (va, vb) = (rim[x.seg[k] % m], rim[(x.seg[k] + 1) % m]);
        let e = mesh.find_edge(va, vb).expect("rim edge");
        mesh.split_edge(e, x.points[k]).expect("split").0
    };
    let p0 = split_at(&mut mesh, 0);
    let p1 = split_at(&mut mesh, 1);
    let mut vs = vec![p0];
    for &p in &x.inside[1..x.inside.len() - 1] {
        vs.push(mesh.add_vertex(p));
    }
    vs.push(p1);
    vs.dedup();
    for w in vs.windows(2) {
        if w[0] != w[1] {
            let _ = mesh.add_edge(w[0], w[1]);
        }
    }
    let cap_outer = mesh
        .collect_loop_verts(mesh.faces.get(cap).unwrap().outer().start)
        .unwrap()
        .len();
    let cut = split_face_by_chain(&mut mesh, cap, &vs, MaterialId::new(0));
    match &cut {
        Ok(res) => println!(
            "CAP outer {cap_outer} verts -> split OK, {} new face(s)",
            res.new_faces.len()
        ),
        Err(e) => println!("CAP outer {cap_outer} verts -> REFUSED: {e}"),
    }
    assert!(cap_outer >= 3, "the cap's boundary is a polygon, not a self-loop");
    assert!(
        cut.is_ok(),
        "so nothing is polygonised on the way in and the cut lands: {cut:?}"
    );
    let v = mesh.verify_face_invariants();
    assert!(v.is_valid(), "and the mesh is sound — {:?}", v.violations);
}

/// The same contract on the other entry that polygonises: a refused
/// `split_face_by_line` leaves the mesh alone too.
///
/// Both call `polygonize_if_closed_curve` before they check anything, so both
/// had the same defect. Claiming one is fixed without measuring the other
/// would be claiming more than was tested.
#[test]
fn a_refused_line_split_also_leaves_the_mesh_alone() {
    use axia_geo::operations::face_split::split_face_by_line;

    let mut mesh = Mesh::new();
    mesh.cylinder_path_b_default = true;
    mesh.create_cylinder(DVec3::ZERO, R, 20.0, 16, MaterialId::new(0)).expect("cylinder");
    let side = mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface().is_some_and(|s| !matches!(s, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .expect("wall");
    // The wall is a Path B band: a one-vertex outer loop, so the split
    // polygonises on the way in.
    assert_eq!(
        mesh.collect_loop_verts(mesh.faces.get(side).unwrap().outer().start).unwrap().len(),
        1,
        "the wall's outer loop is one vertex — the trigger"
    );

    let before = mesh.snapshot();
    // A line nowhere near the wall: nothing to cut, so the split must refuse.
    let out = split_face_by_line(
        &mut mesh,
        side,
        DVec3::new(500.0, 500.0, 5.0),
        DVec3::new(600.0, 600.0, 5.0),
    );
    println!(
        "LINE refused? {}; mesh unchanged? {}",
        out.is_err(),
        mesh.snapshot() == before
    );
    assert!(out.is_err(), "a line off the face is refused");
    assert_eq!(
        mesh.snapshot(),
        before,
        "and the refusal leaves the mesh byte-identical"
    );
}

/// Before designing the splice: is the cap's rim edge already used on BOTH
/// sides?
///
/// The lune would be bounded partly by that rim, and if both sides are taken —
/// the host on one, the cap on the other — then it cannot be built with
/// `add_face_with_holes` at all: there is no free side to make a half-edge on.
/// The piece has to be REASSIGNED from the host instead. Asked rather than
/// assumed, because the whole shape of the fix turns on it.
#[test]
fn how_many_faces_use_a_cap_rim_edge() {
    let mut mesh = Mesh::new();
    mesh.cylinder_path_b_default = true;
    mesh.create_cylinder(DVec3::ZERO, R, 20.0, 16, MaterialId::new(0)).expect("cylinder");
    let side = mesh
        .faces
        .iter()
        .find(|(_, f)| {
            f.is_active()
                && f.surface().is_some_and(|s| !matches!(s, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .expect("wall");
    let a = geodesic_circle(std::f64::consts::PI, 10.0, 3.0, 48);
    let (cap, host) = mesh.split_cylinder_face_by_circle(side, &a).expect("split");

    let rim = mesh
        .collect_loop_verts(mesh.faces.get(cap).unwrap().outer().start)
        .expect("rim");
    let e = mesh.find_edge(rim[0], rim[1]).expect("a rim edge");
    // Which active faces have this edge on one of their loops?
    let users: Vec<u32> = mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .filter(|(fid, f)| {
            let on = |s| {
                mesh.collect_loop_hes(s)
                    .map(|hs| hs.iter().any(|&h| mesh.hes.get(h).map(|x| x.edge()) == Some(e)))
                    .unwrap_or(false)
            };
            let _ = fid;
            on(f.outer().start) || f.inners().iter().any(|l| on(l.start))
        })
        .map(|(fid, _)| fid.raw())
        .collect();
    println!("RIM edge {:?} is used by faces {users:?}", e);

    assert_eq!(
        users.len(),
        2,
        "both sides are taken — the cap and the host. So a lune bounded by this \
         rim cannot be BUILT; the piece must be reassigned from the host"
    );
    assert!(users.contains(&cap.raw()) && users.contains(&host.raw()));
}

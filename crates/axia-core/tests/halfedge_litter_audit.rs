//! AUDIT — where does the leftover half-edge come from, and how far does it go?
//!
//! In a CLOSED solid every half-edge should carry a face: each edge borders two
//! faces and that is all there is. A face-less half-edge in a closed solid is
//! left over from how it was built, nothing more.
//!
//! Measured, and it is not extrude alone. Two leftovers appear per boundary edge
//! of the profile or base, wherever a face is attached to an edge that already
//! had one:
//!
//!   extrude rect / hexagon / circle   8 / 12 / 46   (= 2 × boundary edges)
//!   extrude tapered, bidirectional    8
//!   CONE PRIMITIVE                   48            (= 2 × 24 base segments)
//!   through-drill                    24            (0 → 24 on a clean box)
//!
//! Clean: box, cylinder, sphere primitives, and the kernel-native cone/frustum
//! extrude — which is the interesting one, because it takes the same shape
//! through a different route and leaves nothing behind.
//!
//! It does NOT accumulate: pushing the top face again leaves the count where it
//! was. So this is waste and a behaviour hazard (the loop walk treats a leftover
//! as a way on — see `overlap_walk_sim`), not a leak.
//!
//! HYPOTHESIS, not yet measured: `find_halfedge` Pass 1 only reuses a free
//! half-edge pointing the way the new loop needs. If the wall ends up traversing
//! the shared edge the same way the cap does — which an outward cap flip
//! (ADR-183) would produce after the fact — Pass 1 misses and Pass 2 allocates a
//! second pair, stranding the first.
use axia_core::{Command, Scene, FORM_MATERIAL};
use axia_geo::CreateSolidMode;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// (active half-edges, of which face-less, active faces, open boundary edges)
fn count(s: &Scene) -> (usize, usize, usize, usize) {
    let mut total = 0;
    let mut spare = 0;
    for (_h, he) in s.mesh.hes.iter() {
        if !he.is_active() {
            continue;
        }
        total += 1;
        if he.face().is_null() {
            spare += 1;
        }
    }
    let faces: Vec<_> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(i, _)| i).collect();
    let open = s.mesh.face_set_manifold_info(&faces).boundary_edge_count;
    (total, spare, faces.len(), open)
}

fn row(name: &str, s: &Scene) {
    let (total, spare, faces, open) = count(s);
    let verdict = if open > 0 {
        "(열린 메시 — 판정 제외)"
    } else if spare == 0 {
        "깨끗"
    } else {
        "찌꺼기"
    };
    println!("{:<34} he {:>4}  면없는he {:>3}  면 {:>3}  열린엣지 {:>2}  {}",
        name, total, spare, faces, open, verdict);
}

fn drawn_profile(s: &mut Scene, kind: u8) {
    match kind {
        0 => { s.execute(Command::DrawRectAsShape { center: DVec3::new(0.0,0.0,0.0),
                normal: DVec3::Z, up: DVec3::Y, width: 200.0, height: 200.0 }); }
        1 => { s.execute(Command::DrawCircleAsShape { center: DVec3::ZERO,
                normal: DVec3::Z, radius: 100.0, segments: 24 }); }
        _ => { s.execute(Command::DrawPolygonAsShape { center: DVec3::ZERO,
                normal: DVec3::Z, radius: 100.0, sides: 6 }); }
    }
}

fn first_face(s: &Scene) -> axia_geo::FaceId {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(i, _)| i).next().unwrap()
}

/// Pinned so a fix is visible. These numbers are not a target — they are what
/// the engine does today. Any that drops means a build path stopped leaving
/// litter, and the assertion below says so rather than passing in silence.
#[test]
fn leftover_half_edges_per_build_path() {
    let spare = |s: &Scene| count(s).1;

    let mut s = prod();
    s.mesh.create_box(DVec3::new(0.0,0.0,50.0),100.0,100.0,100.0,FORM_MATERIAL).unwrap();
    assert_eq!(spare(&s), 0, "the box primitive is clean — keep it that way");

    let mut s = prod();
    s.mesh.create_cylinder(DVec3::ZERO,50.0,100.0,24,FORM_MATERIAL).unwrap();
    assert_eq!(spare(&s), 0, "the cylinder primitive is clean");

    let mut s = prod();
    s.mesh.create_cone(DVec3::ZERO,50.0,100.0,24,FORM_MATERIAL).unwrap();
    assert_eq!(spare(&s), 48,
        "the cone primitive strands 2 half-edges per base segment. If this is          now lower, that build path was fixed — say so and lower the number");

    for (k, expect, what) in [(0u8, 8usize, "rect"), (2, 12, "hexagon"), (1, 46, "circle")] {
        let mut s = prod();
        drawn_profile(&mut s, k);
        let f = first_face(&s);
        s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: 100.0 } });
        assert_eq!(spare(&s), expect,
            "extruding a {what} strands 2 half-edges per boundary edge. Lower              means the extrude stopped allocating a second pair — the change              `overlap_walk_sim` is waiting for");
    }

    // The same shape through the kernel-native route leaves nothing behind,
    // which is what makes the others look like an accident rather than a rule.
    let mut s = prod();
    drawn_profile(&mut s, 1);
    let f = first_face(&s);
    s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::ExtrudeCone { distance: 100.0, top_scale: 0.4 } });
    assert_eq!(spare(&s), 0, "the kernel-native frustum route is clean");
}

#[test]
fn where_does_the_leftover_come_from() {
    println!("\n─── 프리미티브 (한 번에 만든 것) ───");
    for (name, build) in [
        ("박스", 0u8), ("원통", 1), ("콘", 2), ("구", 3), ("토러스", 4),
    ] {
        let mut s = prod();
        match build {
            0 => { s.mesh.create_box(DVec3::new(0.0,0.0,50.0),100.0,100.0,100.0,FORM_MATERIAL).unwrap(); }
            1 => { s.mesh.create_cylinder(DVec3::ZERO,50.0,100.0,24,FORM_MATERIAL).unwrap(); }
            2 => { s.mesh.create_cone(DVec3::ZERO,50.0,100.0,24,FORM_MATERIAL).unwrap(); }
            3 => { s.mesh.create_sphere(DVec3::ZERO,50.0,12,12,FORM_MATERIAL).unwrap(); }
            _ => { s.mesh.create_torus_kernel_native(DVec3::ZERO,80.0,20.0,FORM_MATERIAL).unwrap(); }
        }
        row(name, &s);
    }

    println!("\n─── 그린 프로파일 (돌출 전) ───");
    for (name, k) in [("사각형 시트", 0u8), ("원 시트", 1), ("육각형 시트", 2)] {
        let mut s = prod();
        drawn_profile(&mut s, k);
        row(name, &s);
    }

    println!("\n─── 돌출 (그린 프로파일 → 솔리드) ───");
    for (name, k) in [("사각형 → 돌출", 0u8), ("원 → 돌출", 1), ("육각형 → 돌출", 2)] {
        let mut s = prod();
        drawn_profile(&mut s, k);
        let f = first_face(&s);
        s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: 100.0 } });
        row(name, &s);
    }

    println!("\n─── 돌출 변형 ───");
    for (name, mode) in [
        ("테이퍼", CreateSolidMode::ExtrudeTapered { distance: 100.0, taper_deg: 10.0 }),
        ("양방향", CreateSolidMode::ExtrudeBidirectional { dist_pos: 50.0, dist_neg: 50.0 }),
    ] {
        let mut s = prod();
        drawn_profile(&mut s, 0);
        let f = first_face(&s);
        s.execute(Command::CreateSolid { face_id: f, mode });
        row(name, &s);
    }
    {
        let mut s = prod();
        drawn_profile(&mut s, 1);
        let f = first_face(&s);
        s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::ExtrudeCone { distance: 100.0, top_scale: 0.4 } });
        row("콘/프러스텀", &s);
    }

    println!("\n─── 쌓이는가 (같은 솔리드에 계속) ───");
    {
        let mut s = prod();
        drawn_profile(&mut s, 0);
        let f = first_face(&s);
        s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: 100.0 } });
        row("1회 돌출", &s);
        // push the top face further (MoveOnly path)
        let top = s.mesh.faces.iter().filter(|(_, x)| x.is_active()).map(|(i, _)| i)
            .find(|&i| s.mesh.collect_loop_verts(s.mesh.faces[i].outer().start).map_or(false, |vs|
                !vs.is_empty() && vs.iter().all(|v| (s.mesh.vertex_pos(*v).unwrap().z - 100.0).abs() < 1e-6)));
        if let Some(t) = top {
            s.execute(Command::CreateSolid { face_id: t, mode: CreateSolidMode::Extrude { distance: 50.0 } });
            row("  + 윗면 밀기", &s);
        }
    }

    println!("\n─── 카브 / 불리언 ───");
    {
        let mut s = prod();
        drawn_profile(&mut s, 0);
        let f = first_face(&s);
        s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: 100.0 } });
        let _ = s.drill_rect_through_hole(DVec3::new(-40.0,-40.0,100.0), DVec3::new(40.0,40.0,100.0), DVec3::Z);
        row("돌출 박스 관통 드릴", &s);
    }
    {
        let mut s = prod();
        s.mesh.create_box(DVec3::new(0.0,0.0,50.0),100.0,100.0,100.0,FORM_MATERIAL).unwrap();
        let _ = s.drill_rect_through_hole(DVec3::new(-40.0,-40.0,100.0), DVec3::new(40.0,40.0,100.0), DVec3::Z);
        row("프리미티브 박스 관통 드릴", &s);
    }
}

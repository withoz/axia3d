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
//!   extrude rect / hexagon            8 / 12 → 0   (fixed 2026-09-28)
//!   extrude tapered, bidirectional    8 → 0        (fixed 2026-09-28)
//!   extrude circle                    46           (measured, NOT taken — below)
//!   CONE PRIMITIVE                   48 → 0        (fixed 2026-09-27)
//!   through-drill                    24 → 16       (half fixed 2026-09-28)
//!
//! Clean: box, cylinder, sphere primitives, the cone primitive since 2026-09-27,
//! and the kernel-native cone/frustum extrude — which is the interesting one,
//! because it takes the same shape through a different route and leaves nothing
//! behind.
//!
//! It does NOT accumulate: pushing the top face again leaves the count where it
//! was. So this is waste and a behaviour hazard (the loop walk treats a leftover
//! as a way on — see `overlap_walk_sim`), not a leak.
//!
//! The HYPOTHESIS below is no longer a hypothesis for the cone: `find_halfedge`
//! Pass 1 only reuses a free half-edge pointing the way the new loop needs, so if
//! a wall traverses the shared edge the SAME way the cap does, Pass 1 misses and
//! Pass 2 allocates a second pair, stranding the first.
//!
//! `create_cone` wound its side triangles INWARD — `verify_outward_normals` said
//! `inward_count = 16` and nothing asserted it — which made every side walk its
//! base edge the same way the base cap did. Winding them outward (2026-09-27,
//! primitives.rs) took this row from 48 to 0 and cost nothing else. That is the
//! mechanism, measured.
//!
//! ## 2026-09-28 — the drill's half, and it was not Pass 1 at all
//!
//! The three punches (circle, rect, polygon) re-derive their host face with the
//! hole in it, and dropped the old one with `self.faces.remove(host)` — a raw
//! storage removal that leaves every half-edge of its loops still NAMING it.
//! Such a half-edge is in a third state: not free, so Pass 1 will not reuse it
//! and Pass 2 doubles the pair; and not valid, since `faces.contains(he.face())`
//! is false. `remove_face` is the same drop plus one step — it nulls each loop
//! half-edge's face first — and using it took a clean box from
//!
//! ```text
//!                  dangling   spare
//!   one punch       4 → 0     8 → 4     (the 4 are the open rim, not litter)
//!   through-drill   8 → 0    24 → 16
//! ```
//!
//! Ids are never recycled, so a dangling one stays dangling rather than coming
//! to mean another face — a panic waiting on an unguarded index, not silent
//! corruption. Guarded by `a_punched_face_leaves_no_half_edge_pointing_at_it`.
//!
//! ⚠ The 16 that remain are 2 on each of the 8 bore-rim edges and come from
//! `bridge_through_loops`: the tube wall asks for a rim half-edge in a direction
//! the free one does not point. Its winding is chosen at run time (ADR-268), so
//! this is not the same reorder as below and is left measured, not changed.
//!
//! ## 2026-09-28 — it was ADR-183's cap flip, and it only half-lands
//!
//! The rows that remained were wound correctly, so the question was what ELSE
//! made a wall traverse a shared edge the way the cap already did. It was the
//! ORDER: ADR-183 flips the bottom cap AFTER the walls are built, so while a wall
//! is asking for its base half-edge the cap's loop still runs the same way, Pass 1
//! misses, and Pass 2 allocates. Flipping first — only the winding; the Plane is
//! still re-synthesized afterwards, since the top face's surface is attached in
//! between and would overwrite it — makes Pass 1 take the free one.
//!
//! In `extrude_planar_box` and `..._tapered` that is the whole story, and an
//! extruded box is now the shape `create_box` makes: 24 half-edges over 12 edges,
//! nothing spare. Measured with the overlapping ground draw applied, nothing a
//! user sees moves (그라운드 3 / faces 8, before and after).
//!
//! ⚠ **The same move in `extrude_planar_cylinder` is measured and NOT taken.** It
//! works — the circle row goes 46 → 0 — but it is a fifth lever on the plane
//! `what_overlapping_draws_leave_on_the_ground` keeps a ledger of, and it fails
//! the way the fourth did: `a_vertex_whose_outgoing_half_edge_is_gone` gains a
//! face built from three COLLINEAR points at op 18 (a `DrawLine` at y = −100,
//! z = 100, so a NaN normal), and `the_fifty_operation_inventory`'s MoveOnly push
//! gains a stacked pair, 0 → 1. Attributed both ways: with only the box + tapered
//! moves both pass; with only the cylinder move all three fail. Traded, not
//! fixed — so the circle keeps its 46 until the walk's own weakness is the thing
//! being fixed.
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
    assert_eq!(spare(&s), 0,
        "the cone primitive is clean since its sides were wound outward (2026-09-27, was 48) — keep it that way");

    // rect and hexagon go through `extrude_planar_box`, whose ADR-183 cap flip
    // moved to before the walls on 2026-09-28; the circle goes through
    // `extrude_planar_cylinder`, where the same move is measured and NOT taken —
    // see the note at the top of this file.
    for (k, expect, what) in [(0u8, 0usize, "rect"), (2, 0, "hexagon"), (1, 46, "circle")] {
        let mut s = prod();
        drawn_profile(&mut s, k);
        let f = first_face(&s);
        s.execute(Command::CreateSolid { face_id: f, mode: CreateSolidMode::Extrude { distance: 100.0 } });
        assert_eq!(spare(&s), expect,
            "extruding a {what}: if this is LOWER, another build path stopped              allocating a second pair — say so and lower the number");
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

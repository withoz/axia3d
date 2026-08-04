//! How far does "how the solid was built" reach?
//!
//! Two boxes of identical geometry, one grown from a drawn rectangle and one
//! from `create_box`. They differ only in leftover half-edges (see
//! `halfedge_litter_audit`). Run the same operations on both and see which
//! notice.
//!
//! Measured 2026-08-03: all eight behave identically. Of the eight, seven always
//! did; the last — drawing a shape that overlaps the solid's coplanar face —
//! joined them when that draw learned to resolve past a face's edge.
//!
//! Originally measured: of eight, seven behave identically — push, pull-in, through-drill,
//! face punch, plane trim, plane slice, and drawing inside the top face all
//! produce the same faces, the same manifold count, the same everything. Exactly
//! ONE differs: drawing a shape that overlaps the solid's coplanar face.
//!
//! So the leftover is not something the engine reads all over the place. One
//! operation treats it as signal — the loop walk, which takes a face-less
//! half-edge as a way on — and everything else ignores it. That is worth knowing
//! before deciding how much to spend: the fix is narrow, and the argument for
//! doing it is that anything NEW which consults free half-edges inherits the
//! same trap, not that the engine is riddled with it today.

use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;
fn prod()->Scene{let mut s=Scene::new();s.auto_intersect_on_draw=true;s.auto_face_synthesis_on_draw=true;
    s.face_rederive_on_draw=true;s.freeform_overlap_on_draw=true;s}
fn nf(s:&Scene)->usize{s.mesh.faces.iter().filter(|(_,f)|f.is_active()).count()}
fn health(s:&Scene)->String{
    let fs:Vec<FaceId>=s.mesh.faces.iter().filter(|(_,f)|f.is_active()).map(|(i,_)|i).collect();
    format!("면{} nm{} SI{} 열림{}", fs.len(), s.mesh.collect_non_manifold_edges().len(),
        s.mesh.detect_self_intersections().count(), s.mesh.face_set_manifold_info(&fs).boundary_edge_count)
}
fn build(prim:bool)->Scene{
    let mut s=prod();
    if prim { let f=s.mesh.create_box(DVec3::new(100.0,100.0,50.0),200.0,100.0,200.0,FORM_MATERIAL).unwrap();
        s.create_xia_with_faces("b".into(), DVec3::ZERO, f); }
    else {
        s.execute(Command::DrawRectAsShape{center:DVec3::new(100.0,100.0,0.0),normal:DVec3::Z,up:DVec3::Y,width:200.0,height:200.0});
        let f=s.mesh.faces.iter().filter(|(_,x)|x.is_active()).map(|(i,_)|i).next().unwrap();
        s.execute(Command::CreateSolid{face_id:f,mode:CreateSolidMode::Extrude{distance:100.0}});
    }
    s
}
fn top(s:&Scene)->Option<FaceId>{
    s.mesh.faces.iter().filter(|(_,f)|f.is_active()).map(|(i,_)|i).find(|&i|
        s.mesh.collect_loop_verts(s.mesh.faces[i].outer().start).map_or(false,|vs|
            !vs.is_empty() && vs.iter().all(|v| (s.mesh.vertex_pos(*v).unwrap().z-100.0).abs()<1e-6)))
}

/// The list is pinned. If a second operation ever starts caring how the solid
/// was built, this fails — which is the warning worth having.
#[test]
fn blast_radius() {
    let ops: Vec<(&str, Box<dyn Fn(&mut Scene)->String>)> = vec![
        ("윗면 밀기 (push)", Box::new(|s:&mut Scene|{ let t=top(s).unwrap();
            let r=s.execute(Command::CreateSolid{face_id:t,mode:CreateSolidMode::Extrude{distance:50.0}});
            format!("{} {}", if matches!(r,CommandResult::Error(_)){"거부"}else{"수락"}, health(s)) })),
        ("윗면 넣기 (pull in)", Box::new(|s:&mut Scene|{ let t=top(s).unwrap();
            let r=s.execute(Command::CreateSolid{face_id:t,mode:CreateSolidMode::Extrude{distance:-50.0}});
            format!("{} {}", if matches!(r,CommandResult::Error(_)){"거부"}else{"수락"}, health(s)) })),
        ("관통 드릴", Box::new(|s:&mut Scene|{
            let r=s.drill_rect_through_hole(DVec3::new(60.0,60.0,100.0),DVec3::new(140.0,140.0,100.0),DVec3::Z);
            format!("{} {}", if r.is_ok(){"수락"}else{"거부"}, health(s)) })),
        ("면 펀치", Box::new(|s:&mut Scene|{
            let r=s.punch_rect_hole(DVec3::new(60.0,60.0,100.0),DVec3::new(140.0,140.0,100.0),DVec3::Z);
            format!("{} {}", if r.is_ok(){"수락"}else{"거부"}, health(s)) })),
        ("평면 트림", Box::new(|s:&mut Scene|{
            let fs:Vec<FaceId>=s.mesh.faces.iter().filter(|(_,f)|f.is_active()).map(|(i,_)|i).collect();
            let pl=axia_geo::operations::slice::SlicePlane::new(DVec3::new(0.0,0.0,50.0),DVec3::Z).unwrap();
            let r=s.trim_volume_by_plane(&fs,pl,true);
            format!("{} {}", if r.is_ok(){"수락"}else{"거부"}, health(s)) })),
        ("평면 슬라이스", Box::new(|s:&mut Scene|{
            let fs:Vec<FaceId>=s.mesh.faces.iter().filter(|(_,f)|f.is_active()).map(|(i,_)|i).collect();
            let pl=axia_geo::operations::slice::SlicePlane::new(DVec3::new(0.0,0.0,50.0),DVec3::Z).unwrap();
            let r=s.slice_volume_by_plane(&fs,pl);
            format!("{} {}", if r.is_ok(){"수락"}else{"거부"}, health(s)) })),
        ("윗면 안쪽 그리기", Box::new(|s:&mut Scene|{
            let r=s.execute(Command::DrawRectAsShape{center:DVec3::new(100.0,100.0,100.0),
                normal:DVec3::Z,up:DVec3::Y,width:100.0,height:100.0});
            format!("{} {}", if matches!(r,CommandResult::Error(_)){"거부"}else{"수락"}, health(s)) })),
        ("겹쳐 그리기", Box::new(|s:&mut Scene|{
            let r=s.execute(Command::DrawRectAsShape{center:DVec3::new(200.0,200.0,0.0),
                normal:DVec3::Z,up:DVec3::Y,width:200.0,height:200.0});
            format!("{} {}", if matches!(r,CommandResult::Error(_)){"거부"}else{"수락"}, health(s)) })),
    ];
    println!();
    let mut differ: Vec<&str> = Vec::new();
    for (name, op) in ops {
        let mut a=build(false); let ra=op(&mut a);
        let mut b=build(true);  let rb=op(&mut b);
        let same = ra==rb;
        if !same { differ.push(name); }
        println!("{:<20} 돌출:{:<28} 도구:{:<28} {}", name, ra, rb, if same{"같음"}else{"★다름"});
    }
    // 2026-08-03 — none. The overlapping draw resolves the same way on both, so
    // nothing here depends on how the solid was built any more. Any entry
    // reappearing means a build path started mattering again.
    assert!(
        differ.is_empty(),
        "these operations must not care how the solid was built: {differ:?}"
    );
}

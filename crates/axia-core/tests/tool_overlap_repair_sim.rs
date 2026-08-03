//! SIM — can the existing ADR-101 coplanar split repair what the shape tool
//! leaves behind?
//!
//! No. Measured on both the box top and its side face: the tool leaves the whole
//! drawn rectangle covering the overlap region a second time, and
//! `auto_intersect_coplanar` returns `Ok(None)` for that pair — not a split
//! candidate. It resolves two faces whose boundaries properly CROSS; here one
//! sits inside the other (a corner bite, touching the boundary), which is
//! containment. So the repair route is closed and the fix has to stop the extra
//! face being made rather than clean it up afterwards.
//!
//! Kept so the next attempt does not spend a day on the same idea.
use axia_core::{Command, Scene, FORM_MATERIAL};
use axia_geo::FaceId;
use glam::DVec3;
fn prod()->Scene{let mut s=Scene::new();s.auto_intersect_on_draw=true;s.auto_face_synthesis_on_draw=true;
    s.face_rederive_on_draw=true;s.freeform_overlap_on_draw=true;s}
fn nf(s:&Scene)->usize{s.mesh.faces.iter().filter(|(_,f)|f.is_active()).count()}
#[test]
fn the_existing_coplanar_split_cannot_repair_the_tool_output() {
    for (name, side) in [("박스 윗면", false), ("박스 옆면", true)] {
        let mut s = prod();
        let f = s.mesh.create_box(DVec3::new(100.0,100.0,50.0),200.0,100.0,200.0,FORM_MATERIAL).unwrap();
        s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
        let (o,n,u,v) = if side {
            (DVec3::new(200.0,100.0,50.0), DVec3::X, DVec3::Y, DVec3::Z)
        } else {
            (DVec3::new(100.0,100.0,100.0), DVec3::Z, DVec3::X, DVec3::Y)
        };
        // the tool path, unguarded
        s.execute(Command::DrawRect{ center: o + u*100.0 + v*100.0, normal: n, up: v,
            width: 200.0, height: 200.0 });
        let si = s.mesh.detect_self_intersections();
        println!("\n{name}: 그린 뒤 면 {} 겹침쌍 {}", nf(&s), si.count());
        // repair every overlapping coplanar pair with the ADR-101 split
        let mut repaired = 0;
        for (a,b) in si.intersecting_pairs.clone() {
            match axia_geo::operations::coplanar::auto_intersect_coplanar(
                &mut s.mesh, a, b, FORM_MATERIAL) {
                Ok(Some(_)) => { repaired += 1; }
                Ok(None) => println!("   {:?}↔{:?} : 분할 대상 아님", a, b),
                Err(e) => println!("   {:?}↔{:?} : 실패 {e}", a, b),
            }
        }
        let after = s.mesh.detect_self_intersections();
        let fs: Vec<FaceId> = s.mesh.faces.iter().filter(|(_,x)|x.is_active()).map(|(i,_)|i).collect();
        println!("   복구 {repaired}건 → 면 {} 겹침 {} nm {} 열림 {} valid {}",
            nf(&s), after.count(), s.mesh.collect_non_manifold_edges().len(),
            s.mesh.face_set_manifold_info(&fs).boundary_edge_count,
            s.mesh.verify_face_invariants().is_valid());
        assert_eq!(repaired, 0,
            "ADR-101's split now handles this pair — the repair route is open              after all, and the shape tool could be fixed by calling it");
        assert_eq!(after.count(), 1, "and the overlap is still there");
    }
}

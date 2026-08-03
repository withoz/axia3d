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
//! But the pieces to build it are all here, and the last two tests show it.
//!
//! The pair reports THREE crossings where the difference walk wants two. The
//! extra one is the corner the two outlines share: a shape drawn past a face's
//! edge runs along that face's outline for a stretch before parting from it, and
//! every point of that shared run reads as a crossing. Filtering the ones where
//! nothing actually switches sides brings it to two — and then
//! `polygon_difference_walking`, given the BIGGER face as the base, returns
//! exactly the L: six corners, turning around the region that already exists.
//!
//! So the remaining work is wiring, not new geometry: subtract the existing
//! coplanar face from the newly drawn one instead of leaving both.
//!
//! Kept so the next attempt does not spend a day on the same ideas.
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

/// Can the machinery even SEE this configuration? The difference walk wants
/// exactly two crossings; the lens sits in the rectangle's corner, so the two
/// points where its boundary leaves the rectangle's are the candidates.
#[test]
fn what_does_the_intersection_primitive_report() {
    println!("
교차 primitive 가 보는 것
");
    for (name, side) in [("박스 윗면", false), ("박스 옆면", true)] {
        let mut s = prod();
        let f = s.mesh.create_box(DVec3::new(100.0,100.0,50.0),200.0,100.0,200.0,FORM_MATERIAL).unwrap();
        s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
        let (o,n,u,v) = if side {
            (DVec3::new(200.0,100.0,50.0), DVec3::X, DVec3::Y, DVec3::Z)
        } else {
            (DVec3::new(100.0,100.0,100.0), DVec3::Z, DVec3::X, DVec3::Y)
        };
        let _ = n;
        s.execute(Command::DrawRect{ center: o + u*100.0 + v*100.0, normal: n, up: v,
            width: 200.0, height: 200.0 });
        let si = s.mesh.detect_self_intersections();
        for (a,b) in si.intersecting_pairs.clone() {
            match axia_geo::operations::coplanar::coplanar_intersection_segments(&s.mesh, a, b) {
                Ok(ci) => {
                    println!("  {name} {:?}↔{:?}: 렌즈 {}점  교차 {}개",
                        a, b, ci.lens_polygon.len(), ci.crossings.len());
                    for p in &ci.lens_polygon {
                        println!("       렌즈점 ({:.0},{:.0},{:.0})", p.x, p.y, p.z);
                    }
                    for c in &ci.crossings {
                        println!("       교차   ({:.0},{:.0},{:.0})", c.point.x, c.point.y, c.point.z);
                    }
                }
                Err(e) => println!("  {name} {:?}↔{:?}: 실패 — {e}", a, b),
            }
        }
    }
}

/// With the tangential crossing filtered out, can the difference walk produce
/// the L — the drawn rectangle minus the part that already exists?
#[test]
fn can_the_difference_walk_produce_the_l() {
    use axia_geo::operations::coplanar as cop;
    println!("
차집합 걷기 (큰 면 − 작은 면)
");
    for (name, side) in [("박스 윗면", false), ("박스 옆면", true)] {
        let mut s = prod();
        let f = s.mesh.create_box(DVec3::new(100.0,100.0,50.0),200.0,100.0,200.0,FORM_MATERIAL).unwrap();
        s.create_xia_with_faces("b".into(), DVec3::ZERO, f);
        let (o,n,u,v) = if side {
            (DVec3::new(200.0,100.0,50.0), DVec3::X, DVec3::Y, DVec3::Z)
        } else {
            (DVec3::new(100.0,100.0,100.0), DVec3::Z, DVec3::X, DVec3::Y)
        };
        s.execute(Command::DrawRect{ center: o + u*100.0 + v*100.0, normal: n, up: v,
            width: 200.0, height: 200.0 });
        let si = s.mesh.detect_self_intersections();
        for (a,b) in si.intersecting_pairs.clone() {
            // put the BIGGER face first, so face_a indexes the polygon we subtract from
            let (big, small) = {
                let count = |f: FaceId| s.mesh.collect_loop_verts(s.mesh.faces[f].outer().start)
                    .map(|v| v.len()).unwrap_or(0);
                if count(a) >= count(b) { (a, b) } else { (b, a) }
            };
            let ci = match cop::coplanar_intersection_segments(&s.mesh, big, small) {
                Ok(c) => c, Err(e) => { println!("  {name}: 교차 실패 {e}"); continue }
            };
            let base2d: Vec<(f64,f64)> = s.mesh.collect_loop_verts(s.mesh.faces[big].outer().start)
                .unwrap().iter().map(|x| ci.plane.project(s.mesh.vertex_pos(*x).unwrap())).collect();
            let lens2d: Vec<(f64,f64)> = ci.lens_polygon.iter().map(|p| ci.plane.project(*p)).collect();
            let cr: Vec<(usize,f64,(f64,f64))> = ci.crossings.iter()
                .map(|c| (c.face_a_edge, c.face_a_t, ci.plane.project(c.point))).collect();
            match cop::polygon_difference_walking(&base2d, &lens2d, &cr) {
                Ok(poly) => {
                    let pts: Vec<String> = poly.iter().map(|p| format!("({:.0},{:.0})", p.0, p.1)).collect();
                    println!("  {name}: {}각 → {}", poly.len(), pts.join(" "));
                    assert_eq!(poly.len(), 6,
                        "the difference must be the six-cornered L. Fewer or more                          means the tangential filter or the walk changed");
                }
                Err(e) => panic!("{name}: the difference walk must succeed now — {e}"),
            }
        }
    }
}

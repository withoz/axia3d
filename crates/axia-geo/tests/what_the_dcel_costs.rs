//! What the DCEL actually costs per entity, and what a scene costs whole.
//!
//! LOCKED #12 (메타-원칙 #12) sets a memory budget per subsystem. A budget you
//! cannot measure is a wish, so this measures.
//!
//! ── What it read on 2026-08-21 ──────────────────────────────────────────
//!
//! ```text
//!   Face 288 B   HalfEdge 36 B   Edge 200 B   Vertex 48 B
//! ```
//!
//! Half of `Edge` and nearly half of `Face` is an inline `Option`:
//! `Option<AnalyticCurve>` is **104 B**, `Option<AnalyticSurface>` is **128 B**,
//! and an edge with no curve pays for it anyway. On a scene of 20 boxes and
//! 5 Path B spheres that is **24,960 B idle out of 112,000 — 22%**. Real, and
//! small.
//!
//! ⚠ THE BOTTLENECK IS NOT MEMORY. Timing the calls a draw makes on that same
//! scene:
//!
//! ```text
//!   verify_face_invariants      0.06 ms
//!   collect_non_manifold        0.02 ms
//!   detect_self_intersections   0.79 ms
//!   export_buffers             10.50 ms   ← 12x the other three together
//! ```
//!
//! And it is all in the curved surfaces: 20 boxes export 480 vertices in
//! 0.13 ms, while **5 Path B spheres export 97,000 in 10.78 ms** — 19,400
//! vertices each, from a DCEL of 2 faces. That is `tessellate_face_surface`
//! honouring `DEFAULT_ANALYTIC_CHORD_TOL` (0.02 mm), and it is already
//! FRUGAL: the chord-sag arithmetic for r=150 at that tolerance implies ~55,000
//! vertices, so the tessellator is coming in under its own budget.
//!
//! So Path B's 300x DCEL saving (896 B against ~264 KB for the same sphere
//! tessellated as Path A) does not carry into the export, because the export
//! re-tessellates from the surface either way. ADR-135's distance LOD is what
//! addresses this, and it is the reason the same call gets cheap when the
//! camera pulls back.
//!
//! ⚠ Shrinking `Edge`/`Face` by boxing the analytic option would save ~22% of
//! a small number and cost a pointer chase on the hot paths above. Not worth
//! doing on this evidence — recorded so the next person does not re-derive it.
use axia_geo::mesh::Mesh;
use axia_geo::MaterialId;
use glam::DVec3;
use std::mem::size_of;

#[test]
#[ignore = "a measurement — run by hand"]
fn what_the_dcel_costs() {
    println!("\n  ── 엔티티 1개당 바이트 ──");
    println!("    Face      {:>4}", size_of::<axia_geo::Face>());
    println!("    HalfEdge  {:>4}", size_of::<axia_geo::HalfEdge>());
    println!("    Edge      {:>4}", size_of::<axia_geo::Edge>());
    println!("    Vertex    {:>4}", size_of::<axia_geo::Vertex>());
    println!("    DVec3     {:>4}", size_of::<DVec3>());
    println!();
    println!("  ── Edge 200 B 의 내역 ──");
    println!("    Option<AnalyticCurve>  {:>4}", size_of::<Option<axia_geo::curves::AnalyticCurve>>());
    println!("    AnalyticCurve          {:>4}", size_of::<axia_geo::curves::AnalyticCurve>());
    println!("  ── Face 288 B 의 내역 ──");
    println!("    Option<AnalyticSurface> {:>4}", size_of::<Option<axia_geo::surfaces::AnalyticSurface>>());

    // A box: 6 faces, 24 half-edges, 12 edges, 8 verts.
    let mut m = Mesh::new();
    let _ = m;
    let mut m = Mesh::new();
    let vs: Vec<_> = [
        (0.0, 0.0, 0.0), (100.0, 0.0, 0.0), (100.0, 100.0, 0.0), (0.0, 100.0, 0.0),
    ].iter().map(|&(x, y, z)| m.add_vertex(DVec3::new(x, y, z))).collect();
    let f = m.add_face(&vs, MaterialId::new(0)).unwrap();
    let _ = f;
    let (fc, hc, ec, vc) = (
        m.faces.iter().count(), m.hes.iter().count(),
        m.edges.iter().count(), m.verts.iter().count(),
    );
    println!("\n  ── 사각형 하나 ──");
    println!("    면 {fc}  HE {hc}  엣지 {ec}  정점 {vc}");
    let bytes = fc * size_of::<axia_geo::Face>()
        + hc * size_of::<axia_geo::HalfEdge>()
        + ec * size_of::<axia_geo::Edge>()
        + vc * size_of::<axia_geo::Vertex>();
    println!("    엔티티 합계 {bytes} B");

    // How much of that inline space is actually USED in a real scene?
    let mut m2 = Mesh::new();
    let c = m2.create_box(DVec3::ZERO, 1000.0, 1000.0, 1000.0, MaterialId::new(0));
    let _ = c;
    let ec = m2.edges.iter().filter(|(_, e)| e.is_active()).count();
    let ecurved = m2.edges.iter().filter(|(_, e)| e.is_active() && e.curve().is_some()).count();
    let fc2 = m2.faces.iter().filter(|(_, f)| f.is_active()).count();
    let fsurf = m2.faces.iter().filter(|(_, f)| f.is_active() && f.surface().is_some()).count();
    println!("
  ── 상자 하나에서 실제 사용률 ──");
    println!("    엣지 {ec} 중 곡선 있음 {ecurved}");
    println!("    면   {fc2} 중 곡면 있음 {fsurf}");

    let mut m3 = Mesh::new();
    let s3 = m3.create_sphere(DVec3::ZERO, 500.0, 24, 16, MaterialId::new(0));
    let _ = s3;
    let ec3 = m3.edges.iter().filter(|(_, e)| e.is_active()).count();
    let ecurved3 = m3.edges.iter().filter(|(_, e)| e.is_active() && e.curve().is_some()).count();
    let fc3 = m3.faces.iter().filter(|(_, f)| f.is_active()).count();
    let fsurf3 = m3.faces.iter().filter(|(_, f)| f.is_active() && f.surface().is_some()).count();
    println!("
  ── 구 하나 (Path A 24x16) ──");
    println!("    엣지 {ec3} 중 곡선 있음 {ecurved3}");
    println!("    면   {fc3} 중 곡면 있음 {fsurf3}");
    let waste = (ec3 - ecurved3) * 104 + (fc3 - fsurf3) * 128;
    println!("    안 쓰는 인라인 공간 {waste} B");

    // Path B is the production default for sphere/cylinder/cone/torus
    // (LOCKED #47-#51). What does the same sphere cost there?
    let mut m4 = Mesh::new();
    let s4 = m4.create_sphere_kernel_native(DVec3::ZERO, 500.0, MaterialId::new(0));
    let _ = s4;
    let e4 = m4.edges.iter().filter(|(_, e)| e.is_active()).count();
    let ec4 = m4.edges.iter().filter(|(_, e)| e.is_active() && e.curve().is_some()).count();
    let f4 = m4.faces.iter().filter(|(_, f)| f.is_active()).count();
    let h4 = m4.hes.iter().count();
    let v4 = m4.verts.iter().filter(|(_, v)| v.is_active()).count();
    let bytes4 = f4 * size_of::<axia_geo::Face>() + h4 * size_of::<axia_geo::HalfEdge>()
        + e4 * size_of::<axia_geo::Edge>() + v4 * size_of::<axia_geo::Vertex>();
    println!("
  ── 같은 구, Path B (프로덕션 기본) ──");
    println!("    면 {f4}  HE {h4}  엣지 {e4}(곡선 {ec4})  정점 {v4}");
    println!("    엔티티 합계 {bytes4} B");

    // The real question: in a scene built the way the app builds them, how much
    // of the DCEL is inline space nobody uses?
    let mut m5 = Mesh::new();
    for i in 0..20 {
        let x = (i % 5) as f64 * 300.0;
        let y = (i / 5) as f64 * 300.0;
        let _ = m5.create_box(DVec3::new(x, y, 0.0), 200.0, 200.0, 200.0, MaterialId::new(0));
    }
    for i in 0..5 {
        let _ = m5.create_sphere_kernel_native(DVec3::new(i as f64 * 400.0, -500.0, 0.0), 150.0, MaterialId::new(0));
    }
    let e5 = m5.edges.iter().filter(|(_, e)| e.is_active()).count();
    let ec5 = m5.edges.iter().filter(|(_, e)| e.is_active() && e.curve().is_some()).count();
    let f5 = m5.faces.iter().filter(|(_, f)| f.is_active()).count();
    let fs5 = m5.faces.iter().filter(|(_, f)| f.is_active() && f.surface().is_some()).count();
    let h5 = m5.hes.iter().count();
    let v5 = m5.verts.iter().filter(|(_, v)| v.is_active()).count();
    let total = f5 * size_of::<axia_geo::Face>() + h5 * size_of::<axia_geo::HalfEdge>()
        + e5 * size_of::<axia_geo::Edge>() + v5 * size_of::<axia_geo::Vertex>();
    let idle = (e5 - ec5) * 104 + (f5 - fs5) * 128;
    println!("
  ── 상자 20 + Path B 구 5 ──");
    println!("    면 {f5}(곡면 {fs5})  HE {h5}  엣지 {e5}(곡선 {ec5})  정점 {v5}");
    println!("    엔티티 합계 {total} B,  안 쓰는 인라인 {idle} B ({}%)", idle * 100 / total.max(1));

    // Bytes are cheap; the question is what the app WAITS on. Time the calls a
    // draw makes on that same scene.
    use std::time::Instant;
    let t = Instant::now(); let inv = m5.verify_face_invariants(); let d_inv = t.elapsed();
    let t = Instant::now(); let si = m5.detect_self_intersections(); let d_si = t.elapsed();
    let t = Instant::now(); let nm = m5.collect_non_manifold_edges(); let d_nm = t.elapsed();
    let t = Instant::now(); let buf = m5.export_buffers(); let d_ex = t.elapsed();
    println!("
  ── 같은 장면에서 시간 ──");
    println!("    verify_face_invariants   {:>8.2} ms  (위반 {})", d_inv.as_secs_f64()*1000.0, inv.violations.len());
    println!("    detect_self_intersections{:>8.2} ms  (쌍 {})", d_si.as_secs_f64()*1000.0, si.intersecting_pairs.len());
    println!("    collect_non_manifold     {:>8.2} ms  ({})", d_nm.as_secs_f64()*1000.0, nm.len());
    println!("    export_buffers           {:>8.2} ms  (정점 {})", d_ex.as_secs_f64()*1000.0, buf.map(|b| b.0.len()/3).unwrap_or(0));

    // 97,480 vertices from 130 faces is ~750 each. Where do they come from?
    let boxes_only = {
        let mut mb = Mesh::new();
        for i in 0..20 {
            let x = (i % 5) as f64 * 300.0;
            let y = (i / 5) as f64 * 300.0;
            let _ = mb.create_box(DVec3::new(x, y, 0.0), 200.0, 200.0, 200.0, MaterialId::new(0));
        }
        let t = Instant::now(); let b = mb.export_buffers(); let d = t.elapsed();
        (b.map(|x| x.0.len()/3).unwrap_or(0), d.as_secs_f64()*1000.0)
    };
    let spheres_only = {
        let mut ms = Mesh::new();
        for i in 0..5 {
            let _ = ms.create_sphere_kernel_native(DVec3::new(i as f64 * 400.0, -500.0, 0.0), 150.0, MaterialId::new(0));
        }
        let t = Instant::now(); let b = ms.export_buffers(); let d = t.elapsed();
        (b.map(|x| x.0.len()/3).unwrap_or(0), d.as_secs_f64()*1000.0)
    };
    println!("
  ── export_buffers 의 출처 ──");
    println!("    상자 20개만    정점 {:>7}   {:>6.2} ms", boxes_only.0, boxes_only.1);
    println!("    Path B 구 5개만 정점 {:>7}   {:>6.2} ms", spheres_only.0, spheres_only.1);
}

//! Open a saved `.xia`, look at what is wrong with it, and try the repairs.
//!
//! `what_makes_a_saved_file_slow.rs` found a user's file carrying 498
//! intersecting pairs, 3 stacked pairs and 10 non-manifold edges, and two faces
//! belonging to no Shape accounted for half of it. This asks what those faces
//! are and whether the repairs already in the engine can take them out.
//!
//! ```bash
//! AXIA_XIA="/path/in.xia" AXIA_XIA_OUT="/path/out.xia" \
//!   cargo test --release -p axia-core --test what_repairs_a_damaged_file \
//!   -- --ignored --nocapture
//! ```
//!
//! ⚠ It never writes over its input. `AXIA_XIA_OUT` must be a different path,
//! and without it nothing is written at all — the run is a report.
//!
//! Ignored by default — it needs a file that is not in this repository.

use axia_core::scene::Scene;
use std::time::Instant;

fn snapshot_of(bytes: &[u8]) -> (Vec<u8>, Vec<u8>) {
    if bytes.len() < 12 || &bytes[0..4] != b"AIXA" {
        return (Vec::new(), bytes.to_vec());
    }
    let mlen = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    (bytes[..12 + mlen].to_vec(), bytes[12 + mlen..].to_vec())
}

struct Health {
    faces: usize,
    verts: usize,
    si: usize,
    stacked: usize,
    nm: usize,
    violations: usize,
}

fn health(s: &Scene) -> Health {
    let nm = s.mesh.collect_non_manifold_edges();
    Health {
        faces: s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
        verts: s.mesh.verts.iter().filter(|(_, v)| v.is_active()).count(),
        si: s.mesh.detect_self_intersections().intersecting_pairs.len(),
        stacked: nm.iter().filter(|&&e| s.mesh.edge_stacked_face_pair(e).is_some()).count(),
        nm: nm.len(),
        violations: s.mesh.verify_face_invariants().violations.len(),
    }
}

fn line(label: &str, h: &Health) {
    println!(
        "    {label:<28} 면 {:>4}  정점 {:>4}  자기교차 {:>4}  겹침 {:>2}  비-manifold {:>2}  위반 {:>2}",
        h.faces, h.verts, h.si, h.stacked, h.nm, h.violations
    );
}

#[test]
#[ignore = "needs AXIA_XIA=<path to a .xia> — run by hand"]
fn what_repairs_a_damaged_file() {
    let Ok(path) = std::env::var("AXIA_XIA") else {
        println!("\n  AXIA_XIA 를 .xia 경로로 지정하세요.\n");
        return;
    };
    let bytes = std::fs::read(&path).expect("파일을 읽을 수 없음");
    let (container, snap) = snapshot_of(&bytes);

    let mut s = Scene::new();
    s.import_versioned_snapshot(&snap).expect("import");
    println!("\n  파일 {path}\n");
    let before = health(&s);
    line("처음", &before);

    // ── 무엇이 그렇게 만드나 ────────────────────────────────────────────
    //
    // Half the damage sits on two faces. Say what they are before touching
    // anything: an unowned face is not automatically rubbish — it may be a
    // legitimate piece the arrangement produced and never handed to a Shape.
    {
        let mut per_face: std::collections::BTreeMap<u32, usize> = Default::default();
        for (a, b) in s.mesh.detect_self_intersections().intersecting_pairs {
            *per_face.entry(a.raw()).or_default() += 1;
            *per_face.entry(b.raw()).or_default() += 1;
        }
        let mut worst: Vec<(usize, u32)> = per_face.iter().map(|(f, c)| (*c, *f)).collect();
        worst.sort_by(|a, b| b.0.cmp(&a.0));
        println!("\n  가장 많이 걸린 면:");
        for (count, raw) in worst.iter().take(6) {
            let fid = axia_geo::FaceId::new(*raw);
            let Some(face) = s.mesh.faces.get(fid) else { continue };
            let verts = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
            let inners = face.inners().len();
            let n = face.normal().normalize_or_zero();
            let owner = s
                .shapes
                .iter()
                .find(|(_, sh)| sh.face_ids.contains(&fid))
                .map(|(id, sh)| format!("Shape {} \"{}\"", id.raw(), sh.name))
                .or_else(|| {
                    s.xias
                        .iter()
                        .find(|(_, x)| x.face_ids.contains(&fid))
                        .map(|(id, x)| format!("XIA {} \"{}\"", id, x.name))
                })
                .unwrap_or_else(|| "주인 없음".into());
            println!(
                "      FaceId({raw})  {count:>3} 건   정점 {:>3}  구멍 {inners}  넓이 {:>10.0}  \
                 법선 ({:.2},{:.2},{:.2})  {owner}",
                verts.len(),
                s.mesh.face_area(fid),
                n.x,
                n.y,
                n.z
            );
        }
    }

    // ⚠ Not every reported pair is damage.
    //
    // `detect_self_intersections` reports three different things the same way
    // and only two of them are wrong (`ContactKind`): a sheet resting on the
    // walls below it TOUCHES, and this repo has already decided two solids may
    // legitimately sit in one another. Count what is actually there before
    // recommending anyone change their model.
    {
        let (mut touching, mut coplanar, mut crossing, mut unknown) = (0, 0, 0, 0);
        for (a, b) in s.mesh.detect_self_intersections().intersecting_pairs {
            match s.mesh.classify_contact(a, b) {
                Some(axia_geo::operations::self_intersect::ContactKind::Touching) => touching += 1,
                Some(axia_geo::operations::self_intersect::ContactKind::CoplanarOverlap) => {
                    coplanar += 1
                }
                Some(axia_geo::operations::self_intersect::ContactKind::Crossing) => crossing += 1,
                None => unknown += 1,
            }
        }
        println!("\n  그 {} 쌍의 정체:", before.si);
        println!("      맞닿음 (손상 아님)   {touching}");
        println!("      같은 면 겹침 (손상)  {coplanar}");
        println!("      뚫고 지나감 (손상)   {crossing}");
        if unknown > 0 {
            println!("      읽을 수 없음         {unknown}");
        }
    }

    // ⚠ Each repair on its own, from a fresh load, before any chain.
    //
    // Chaining them hides which one did what and which one cost something. In
    // particular `normalize_for_import` flips winding by majority vote — 284
    // faces on this file — and that is a visible change to a model somebody
    // built, not a free tidy-up.
    println!("\n  복구 수단 하나씩 (매번 새로 불러와서):");
    let fresh = || {
        let mut t = Scene::new();
        t.import_versioned_snapshot(&snap).expect("import");
        t
    };
    let classify = |sc: &Scene| -> (usize, usize, usize) {
        let (mut t, mut c, mut x) = (0, 0, 0);
        for (a, b) in sc.mesh.detect_self_intersections().intersecting_pairs {
            match sc.mesh.classify_contact(a, b) {
                Some(axia_geo::operations::self_intersect::ContactKind::Touching) => t += 1,
                Some(axia_geo::operations::self_intersect::ContactKind::CoplanarOverlap) => c += 1,
                _ => x += 1,
            }
        }
        (t, c, x)
    };
    let (t0, c0, x0) = classify(&s);
    {
        let mut a = fresh();
        let rep = a.repair_non_manifold_edges();
        line("비-manifold 복구만", &health(&a));
        println!(
            "        (엣지 {} 수리, 면 {} 분리, 정점 {} 생성)",
            rep.edges_repaired, rep.faces_detached, rep.vertices_created
        );
        // ⚠ The pair count RISES here. Detaching gives each face its own copy of
        // the shared edge, so two faces that were one edge apart now register as
        // a pair — ask which KIND before calling that a regression.
        let (t1, c1, x1) = classify(&a);
        println!(
            "        맞닿음 {t0} -> {t1}   같은 면 겹침 {c0} -> {c1}   뚫고 지나감 {x0} -> {x1}"
        );
    }
    {
        let mut a = fresh();
        let r = a.mesh.normalize_for_import(&axia_geo::NormalizeOptions {
            normalize_winding: false,
            ..Default::default()
        });
        line("normalize (winding 제외)", &health(&a));
        println!("        ({})", r.summary());
    }
    {
        let mut a = fresh();
        let r = a.mesh.normalize_for_import(&axia_geo::NormalizeOptions::default());
        line("normalize (winding 포함)", &health(&a));
        println!("        ({})", r.summary());
    }

    // ── 있는 복구 수단을 순서대로 ───────────────────────────────────────
    println!("\n  복구 단계별 (연쇄):");

    let t = Instant::now();
    let removed = s.mesh.deactivate_empty_emit_faces();
    let h = health(&s);
    println!("      (빈 emit 면 {removed} 개 제거, {:.0} ms)", t.elapsed().as_secs_f64() * 1000.0);
    line("빈 면 정리 후", &h);

    let t = Instant::now();
    let report = s.mesh.normalize_for_import(&axia_geo::NormalizeOptions::default());
    let h = health(&s);
    println!(
        "      ({}, {:.0} ms)",
        report.summary(),
        t.elapsed().as_secs_f64() * 1000.0
    );
    line("normalize 후", &h);

    let t = Instant::now();
    let rep = s.repair_non_manifold_edges();
    let h = health(&s);
    println!("      ({rep:?}, {:.0} ms)", t.elapsed().as_secs_f64() * 1000.0);
    line("비-manifold 복구 후", &h);

    println!();
    println!(
        "  요약   자기교차 {} -> {}   겹침 {} -> {}   비-manifold {} -> {}   위반 {} -> {}",
        before.si, h.si, before.stacked, h.stacked, before.nm, h.nm, before.violations, h.violations
    );

    // ── 쓰기 (요청했을 때만, 다른 경로로만) ─────────────────────────────
    //
    // ⚠ What gets written is `repair_non_manifold_edges` ALONE, not the chain
    // above. Measured on this file:
    //
    //   repair only        위상 손상 전부 0, 손상 쌍 23 -> 22, winding 그대로
    //   normalize 포함     위반 그대로 3, winding 284 개 뒤집힘
    //
    // Flipping the winding of 284 faces in a model somebody built is a visible
    // change that bought nothing here, so it is not in the file we hand back.
    if let Ok(out) = std::env::var("AXIA_XIA_OUT") {
        assert_ne!(out, path, "출력이 입력과 같으면 안 됩니다");
        let mut w = fresh();
        let rep = w.repair_non_manifold_edges();
        let hw = health(&w);
        println!(
            "\n  쓸 상태: 비-manifold 복구만 (엣지 {} 수리, 면 {} 분리)",
            rep.edges_repaired, rep.faces_detached
        );
        line("  →", &hw);
        assert_eq!(hw.violations, 0, "고쳐지지 않은 파일을 내보내지 않습니다");
        assert_eq!(hw.nm, 0, "비-manifold 가 남은 파일을 내보내지 않습니다");
        let fixed = w.export_versioned_snapshot().expect("export");
        let mut buf = container.clone();
        buf.extend_from_slice(&fixed);
        std::fs::write(&out, &buf).expect("쓸 수 없음");
        println!("\n  저장 {out}  ({} 바이트)\n", buf.len());
    } else {
        println!("\n  (AXIA_XIA_OUT 이 없어 아무것도 쓰지 않았습니다)\n");
    }
}

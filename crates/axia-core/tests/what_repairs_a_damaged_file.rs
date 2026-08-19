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

    // ⚠ Two vertices at bit-identical coordinates.
    //
    // LOCKED #5 says the spatial hash merges anything within 0.15 µm, so a pair
    // at 0.000000000 mm should not exist. Some ops make them on purpose — a
    // detach or an ADR-102 cleave gives each side its own copy — so the number
    // matters more than the existence. Census the whole model.
    {
        let mut by_key: std::collections::HashMap<(i64, i64, i64), Vec<axia_geo::VertId>> =
            Default::default();
        // 1 nm buckets: far below the 0.15 µm dedup floor, so anything landing
        // in one bucket is coincident by any measure this engine uses.
        let q = |p: glam::DVec3| {
            (
                (p.x * 1e6).round() as i64,
                (p.y * 1e6).round() as i64,
                (p.z * 1e6).round() as i64,
            )
        };
        for (id, v) in s.mesh.verts.iter().filter(|(_, v)| v.is_active()) {
            by_key.entry(q(v.pos())).or_default().push(id);
        }
        let dup_groups: Vec<&Vec<axia_geo::VertId>> =
            by_key.values().filter(|g| g.len() > 1).collect();
        let dup_verts: usize = dup_groups.iter().map(|g| g.len()).sum();
        println!(
            "\n  같은 자리에 있는 정점: {} 무리, 정점 {} 개 (활성 {} 중)",
            dup_groups.len(),
            dup_verts,
            s.mesh.verts.iter().filter(|(_, v)| v.is_active()).count()
        );
        let mut sizes: std::collections::BTreeMap<usize, usize> = Default::default();
        for g in &dup_groups {
            *sizes.entry(g.len()).or_default() += 1;
        }
        if !sizes.is_empty() {
            println!("      무리 크기별 {sizes:?}");
            for g in dup_groups.iter().take(3) {
                if let Ok(p) = s.mesh.vertex_pos(g[0]) {
                    println!("      예: {:?} at {p:?}", g);
                }
            }
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

    // ⚠ Damage is not the same as a mistake.
    //
    // Two boxes deliberately pushed into each other cross, and this repo has
    // already decided that is legal. A solid folding through ITSELF is not. The
    // owner is what separates them, so ask it of every damaging pair rather than
    // reporting a number and leaving the reader to guess.
    {
        let owner_of = |f: axia_geo::FaceId| -> Option<String> {
            s.shapes
                .iter()
                .find(|(_, sh)| sh.face_ids.contains(&f))
                .map(|(id, sh)| format!("Shape {} \"{}\"", id.raw(), sh.name))
                .or_else(|| {
                    s.xias
                        .iter()
                        .find(|(_, x)| x.face_ids.contains(&f))
                        .map(|(id, x)| format!("XIA {} \"{}\"", id, x.name))
                })
        };
        use axia_geo::operations::self_intersect::ContactKind;
        let mut same: std::collections::BTreeMap<String, usize> = Default::default();
        let mut across: std::collections::BTreeMap<String, usize> = Default::default();
        let mut unowned = 0usize;
        let (mut same_n, mut across_n) = (0usize, 0usize);
        for (a, b) in s.mesh.detect_self_intersections().intersecting_pairs {
            let kind = s.mesh.classify_contact(a, b);
            if !matches!(kind, Some(k) if k.is_damage()) {
                continue;
            }
            let tag = match kind {
                Some(ContactKind::CoplanarOverlap) => "겹침",
                _ => "관통",
            };
            match (owner_of(a), owner_of(b)) {
                (Some(x), Some(y)) if x == y => {
                    same_n += 1;
                    *same.entry(format!("{tag}  {x}")).or_default() += 1;
                }
                (Some(x), Some(y)) => {
                    across_n += 1;
                    let (lo, hi) = if x < y { (x, y) } else { (y, x) };
                    *across.entry(format!("{tag}  {lo}  ×  {hi}")).or_default() += 1;
                }
                _ => unowned += 1,
            }
        }
        println!("\n  손상 쌍의 주인:");
        println!("      같은 물체가 스스로 (결함)   {same_n}");
        println!("      다른 두 물체 사이 (의도?)   {across_n}");
        println!("      주인 없는 면이 낀 것        {unowned}");

        let mut v: Vec<(&String, &usize)> = same.iter().collect();
        v.sort_by(|a, b| b.1.cmp(a.1));
        if !v.is_empty() {
            println!("\n      스스로 겹치는 물체:");
            for (what, n) in v.iter().take(8) {
                println!("          {n:>3} 쌍   {what}");
            }
        }
        let mut w: Vec<(&String, &usize)> = across.iter().collect();
        w.sort_by(|a, b| b.1.cmp(a.1));
        if !w.is_empty() {
            println!("\n      물체끼리 겹치는 짝:");
            for (what, n) in w.iter().take(8) {
                println!("          {n:>3} 쌍   {what}");
            }
        }
    }

    // Whatever object carries the most self-crossing, described.
    //
    // If the damage concentrates in ONE object the answer is about that object,
    // not about the model, and the user can act on it.
    {
        use axia_geo::operations::self_intersect::ContactKind;
        let mut worst_xia: Option<(usize, u32)> = None;
        for (id, x) in s.xias.iter() {
            let n = s
                .mesh
                .detect_self_intersections()
                .intersecting_pairs
                .into_iter()
                .filter(|(a, b)| {
                    x.face_ids.contains(a)
                        && x.face_ids.contains(b)
                        && matches!(s.mesh.classify_contact(*a, *b), Some(k) if k.is_damage())
                })
                .count();
            if n > 0 && worst_xia.map_or(true, |(m, _)| n > m) {
                worst_xia = Some((n, *id));
            }
        }
        if let Some((n, id)) = worst_xia {
            let x = &s.xias[&id];
            let live: Vec<axia_geo::FaceId> = x
                .face_ids
                .iter()
                .copied()
                .filter(|f| s.mesh.faces.get(*f).is_some_and(|ff| ff.is_active()))
                .collect();
            let mut lo = glam::DVec3::splat(f64::INFINITY);
            let mut hi = glam::DVec3::splat(f64::NEG_INFINITY);
            let mut area = 0.0;
            for &f in &live {
                area += s.mesh.face_area(f);
                if let Some(face) = s.mesh.faces.get(f) {
                    if let Ok(vv) = s.mesh.collect_loop_verts(face.outer().start) {
                        for &v in &vv {
                            if let Ok(p) = s.mesh.vertex_pos(v) {
                                lo = lo.min(p);
                                hi = hi.max(p);
                            }
                        }
                    }
                }
            }
            println!("\n  가장 많이 스스로 뚫는 물체 — XIA {id} \"{}\"", x.name);
            println!("      면 {} (등록 {})   손상 쌍 {n}", live.len(), x.face_ids.len());
            println!(
                "      범위  x {:.0}..{:.0}   y {:.0}..{:.0}   z {:.0}..{:.0}   (넓이 합 {:.0} mm²)",
                lo.x, hi.x, lo.y, hi.y, lo.z, hi.z, area
            );
            // A box has six faces. Anything else here says what happened to it.
            let mut by_normal: std::collections::BTreeMap<(i64, i64, i64), usize> =
                Default::default();
            for &f in &live {
                let nn = s.mesh.faces.get(f).map(|ff| ff.normal().normalize_or_zero());
                if let Some(nn) = nn {
                    let k = |v: f64| (v * 10.0).round() as i64;
                    *by_normal.entry((k(nn.x), k(nn.y), k(nn.z))).or_default() += 1;
                }
            }
            println!("      법선별 면 수 {by_normal:?}");
            let sealed = live.iter().filter(|&&f| s.mesh.is_face_in_volume(f)).count();
            println!("      부피 안에 봉인된 면 {sealed} / {}", live.len());

            // Which shape is the damage? Two faces folding at a shared corner is
            // one thing; two faces lying on top of each other, far from any
            // shared vertex, is a doubled shell — a different thing and a
            // different fix.
            let centroid = |f: axia_geo::FaceId| -> Option<glam::DVec3> {
                let face = s.mesh.faces.get(f)?;
                let vv = s.mesh.collect_loop_verts(face.outer().start).ok()?;
                if vv.is_empty() {
                    return None;
                }
                let mut c = glam::DVec3::ZERO;
                for &v in &vv {
                    c += s.mesh.vertex_pos(v).ok()?;
                }
                Some(c / vv.len() as f64)
            };
            let verts_of = |f: axia_geo::FaceId| -> Vec<axia_geo::VertId> {
                s.mesh
                    .faces
                    .get(f)
                    .and_then(|ff| s.mesh.collect_loop_verts(ff.outer().start).ok())
                    .unwrap_or_default()
            };
            let (mut adjacent, mut apart) = (0usize, 0usize);
            let mut dists: Vec<f64> = Vec::new();
            for (a, b) in s.mesh.detect_self_intersections().intersecting_pairs {
                if !(x.face_ids.contains(&a) && x.face_ids.contains(&b)) {
                    continue;
                }
                if !matches!(s.mesh.classify_contact(a, b), Some(k) if k.is_damage()) {
                    continue;
                }
                let va = verts_of(a);
                let vb = verts_of(b);
                if va.iter().any(|v| vb.contains(v)) {
                    adjacent += 1;
                } else {
                    apart += 1;
                }
                if let (Some(ca), Some(cb)) = (centroid(a), centroid(b)) {
                    dists.push((ca - cb).length());
                }
            }
            // ⚠ One object is not necessarily one piece.
            //
            // If these faces form several disconnected shells, then "the object
            // crosses itself" is really "two separate pieces that happen to
            // share a name cross each other" — which this repo already calls
            // legal. Connectivity decides it, so ask.
            {
                let mut comp: std::collections::HashMap<axia_geo::FaceId, usize> = Default::default();
                let mut next = 0usize;
                for &f in &live {
                    if comp.contains_key(&f) {
                        continue;
                    }
                    let mut stack = vec![f];
                    comp.insert(f, next);
                    while let Some(cur) = stack.pop() {
                        for e in s.mesh.face_outer_edges(cur).unwrap_or_default() {
                            for nb in s.mesh.get_faces_sharing_edge(e).0 {
                                if live.contains(&nb) && !comp.contains_key(&nb) {
                                    comp.insert(nb, next);
                                    stack.push(nb);
                                }
                            }
                        }
                    }
                    next += 1;
                }
                let mut sizes: std::collections::BTreeMap<usize, usize> = Default::default();
                for c in comp.values() {
                    *sizes.entry(*c).or_default() += 1;
                }
                let mut v: Vec<usize> = sizes.values().copied().collect();
                v.sort_unstable_by(|a, b| b.cmp(a));
                println!("      떨어진 조각 {next} 개, 면 수 {:?}", &v[..v.len().min(8)]);

                let cross_piece = s
                    .mesh
                    .detect_self_intersections()
                    .intersecting_pairs
                    .into_iter()
                    .filter(|(a, b)| {
                        x.face_ids.contains(a)
                            && x.face_ids.contains(b)
                            && matches!(s.mesh.classify_contact(*a, *b), Some(k) if k.is_damage())
                            && comp.get(a) != comp.get(b)
                    })
                    .count();
                println!("      그중 다른 조각끼리 {cross_piece} 쌍, 같은 조각 안에서 {} 쌍",
                    169usize.saturating_sub(cross_piece).min(dists.len()));
            }

            dists.sort_by(|p, q| p.partial_cmp(q).unwrap());
            println!("      정점을 공유하는 쌍 {adjacent}   떨어진 쌍 {apart}");
            if !dists.is_empty() {
                let n = dists.len();
                println!(
                    "      두 면 중심 거리  최소 {:.1}  중앙 {:.1}  최대 {:.1} mm",
                    dists[0],
                    dists[n / 2],
                    dists[n - 1]
                );
            }
        }
    }

    // ⚠ The owner is the wrong question. The PIECE is the right one.
    //
    // Ownership said 171 of the 197 damaging pairs were "one object crossing
    // itself", which sounds like corruption. It is not: `XIA 33 "Box"` turned
    // out to be three disconnected shells sharing one name, and every one of its
    // 169 pairs is between DIFFERENT shells. Two separate solids overlapping is
    // something this repo has already decided is legal; a single connected shell
    // passing through itself is not. So walk the connectivity of the whole mesh
    // and ask that instead.
    {
        use axia_geo::operations::self_intersect::ContactKind;
        let all: Vec<axia_geo::FaceId> =
            s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(i, _)| i).collect();
        let mut comp: std::collections::HashMap<axia_geo::FaceId, usize> = Default::default();
        let mut next = 0usize;
        for &f in &all {
            if comp.contains_key(&f) {
                continue;
            }
            let mut stack = vec![f];
            comp.insert(f, next);
            while let Some(cur) = stack.pop() {
                for e in s.mesh.face_outer_edges(cur).unwrap_or_default() {
                    for nb in s.mesh.get_faces_sharing_edge(e).0 {
                        if s.mesh.faces.get(nb).is_some_and(|x| x.is_active())
                            && !comp.contains_key(&nb)
                        {
                            comp.insert(nb, next);
                            stack.push(nb);
                        }
                    }
                }
            }
            next += 1;
        }
        let (mut within, mut between) = (0usize, 0usize);
        let (mut within_cop, mut within_cross) = (0usize, 0usize);
        let mut offenders: std::collections::BTreeMap<usize, usize> = Default::default();
        for (a, b) in s.mesh.detect_self_intersections().intersecting_pairs {
            let kind = s.mesh.classify_contact(a, b);
            if !matches!(kind, Some(k) if k.is_damage()) {
                continue;
            }
            if comp.get(&a) == comp.get(&b) {
                within += 1;
                if matches!(kind, Some(ContactKind::CoplanarOverlap)) {
                    within_cop += 1;
                } else {
                    within_cross += 1;
                }
                if let Some(c) = comp.get(&a) {
                    *offenders.entry(*c).or_default() += 1;
                }
            } else {
                between += 1;
            }
        }
        println!("\n  손상 쌍을 조각 기준으로 (떨어진 덩어리 {next} 개):");
        println!("      다른 덩어리끼리 (겹쳐 놓음 — 합법)   {between}");
        println!("      한 덩어리가 스스로 (진짜 결함)       {within}   겹침 {within_cop} / 관통 {within_cross}");
        // The few that survive every distinction are the ones worth naming.
        for (a, b) in s.mesh.detect_self_intersections().intersecting_pairs {
            let kind = s.mesh.classify_contact(a, b);
            if !matches!(kind, Some(k) if k.is_damage()) || comp.get(&a) != comp.get(&b) {
                continue;
            }
            let describe = |f: axia_geo::FaceId| -> String {
                let owner = s
                    .shapes
                    .iter()
                    .find(|(_, sh)| sh.face_ids.contains(&f))
                    .map(|(id, sh)| format!("Shape {} \"{}\"", id.raw(), sh.name))
                    .or_else(|| {
                        s.xias
                            .iter()
                            .find(|(_, x)| x.face_ids.contains(&f))
                            .map(|(id, x)| format!("XIA {} \"{}\"", id, x.name))
                    })
                    .unwrap_or_else(|| "주인 없음".into());
                let n = s.mesh.faces.get(f).map(|x| x.normal().normalize_or_zero()).unwrap_or_default();
                format!(
                    "{f:?} 넓이 {:.0} 법선 ({:.2},{:.2},{:.2}) {owner}",
                    s.mesh.face_area(f),
                    n.x,
                    n.y,
                    n.z
                )
            };
            println!("          · {}", describe(a));
            println!("            {}", describe(b));
            // ⚠ "sliver" was a guess the first time and only half right — one of
            // these pairs is 55 mm² against 2 m², the other is 2 m² against
            // 0.67 m². Print the corners so the shape is read, not assumed.
            for f in [a, b] {
                let Some(face) = s.mesh.faces.get(f) else { continue };
                let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
                let pts: Vec<String> = vv
                    .iter()
                    .filter_map(|&v| s.mesh.vertex_pos(v).ok())
                    .map(|p| format!("({:.0},{:.0},{:.0})", p.x, p.y, p.z))
                    .collect();
                println!(
                    "              {f:?} 정점 {} 구멍 {}  {}",
                    vv.len(),
                    face.inners().len(),
                    pts.join(" ")
                );
            }
            // ⚠ The repair reports 0 for these. Which gate declines? The one it
            // has to pass is `crossings.len() >= 2 && even`, so read that rather
            // than guessing at it.
            let (big, small) = if s.mesh.face_outer_area(a) >= s.mesh.face_outer_area(b) {
                (a, b)
            } else {
                (b, a)
            };
            match axia_geo::operations::coplanar::coplanar_intersection_segments(
                &s.mesh, big, small,
            ) {
                Ok(ci) => println!(
                    "              교차점 {}  lens 정점 {}",
                    ci.crossings.len(),
                    ci.lens_polygon.len()
                ),
                Err(e) => println!("              coplanar_intersection_segments: {e}"),
            }
            // ⚠ Before trusting anything measured off `export_buffers`: those
            // positions are f32. At x ≈ 4469 an f32 step is 0.00027 mm, so a
            // "2.4 µm" reading from that buffer is nine steps and may be noise
            // the detector never sees — it tessellates in f64. The DCEL
            // vertices are the f64 truth, so ask them whether the two faces
            // share their corners or merely come close.
            {
                let vs = |f: axia_geo::FaceId| -> Vec<axia_geo::VertId> {
                    s.mesh
                        .faces
                        .get(f)
                        .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
                        .unwrap_or_default()
                };
                let (va, vb) = (vs(a), vs(b));
                let shared: Vec<axia_geo::VertId> =
                    va.iter().copied().filter(|v| vb.contains(v)).collect();
                println!("              공유 정점 {} 개 {:?}", shared.len(), shared);
                let mut worst = f64::MAX;
                let mut pair = String::new();
                for &p in &va {
                    for &q in &vb {
                        if p == q {
                            continue;
                        }
                        if let (Ok(pp), Ok(qq)) = (s.mesh.vertex_pos(p), s.mesh.vertex_pos(q)) {
                            let d = (pp - qq).length();
                            if d < worst {
                                worst = d;
                                pair = format!("{p:?}{pp:?} ↔ {q:?}{qq:?}");
                            }
                        }
                    }
                }
                if worst < f64::MAX {
                    println!("              다른 정점끼리 가장 가까운 거리 {worst:.9} mm");
                    println!("                 {pair}");
                }
            }

            // ⚠ A corner list is not the face.
            //
            // `face_area` reported 672,431 for FaceId(599) while the shoelace of
            // its four corners is 638,375. That is not a bug in `face_area` — it
            // adds `loop_curve_bulge`, the area an edge carrying an `Arc` sweeps
            // beyond its chord. Which means a 2D test built from CORNERS alone
            // measures a smaller face than the one that is really there, and a
            // "0% overlap" from such a test is only as good as the assumption
            // that no edge is curved. So say whether any is.
            for f in [a, b] {
                let poly = s.mesh.face_outer_area(f);
                let Some(face) = s.mesh.faces.get(f) else { continue };
                let vv = s.mesh.collect_loop_verts(face.outer().start).unwrap_or_default();
                let shoelace = {
                    let pts: Vec<glam::DVec3> =
                        vv.iter().filter_map(|&v| s.mesh.vertex_pos(v).ok()).collect();
                    let mut sum = 0.0;
                    for i in 0..pts.len() {
                        let p = pts[i];
                        let q = pts[(i + 1) % pts.len()];
                        sum += p.x * q.y - q.x * p.y;
                    }
                    (sum * 0.5).abs()
                };
                let curved_edges = s
                    .mesh
                    .face_outer_edges(f)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|&e| s.mesh.edges.get(e).is_some_and(|x| x.curve().is_some()))
                    .count();
                println!(
                    "              {f:?} 넓이(엔진) {poly:.0}  모서리 다각형 {shoelace:.0}  \
                     차이 {:.0}  곡선 모서리 {curved_edges}",
                    poly - shoelace
                );
            }

            // ⚠ So sample the TRIANGLES, not the corners.
            //
            // A first version of this measurement built its polygons from the
            // corner list and answered "0% overlap" for both pairs. The areas
            // above say why that answer was not safe: FaceId(599)'s real region
            // reaches 34,234 mm² beyond the polygon its corners trace. The
            // triangles `export_buffers` emits ARE the face the engine draws and
            // the detector tests, arcs included, so ask them.
            {
                // ⚠ `face_tessellation`, not `export_buffers` — the detector's own
                // f64 triangles. The render buffer is f32 and a reading taken
                // off it put a crossing 2.4 µm from an endpoint that the
                // detector never saw there.
                // ⚠ Project onto the FACE's plane, not onto XY.
                //
                // A first version flattened every triangle by dropping z, which
                // reads a vertical wall as zero area and any two of them as
                // 100% overlapping. The pair shares a plane by the time it gets
                // here (that is what `CoplanarOverlap` means), so one basis
                // built from its normal serves both faces.
                let n = s
                    .mesh
                    .faces
                    .get(a)
                    .map(|f| f.normal().normalize_or_zero())
                    .unwrap_or(glam::DVec3::Z);
                let up = if n.z.abs() < 0.9 { glam::DVec3::Z } else { glam::DVec3::X };
                let ux = up.cross(n).normalize_or_zero();
                let uy = n.cross(ux);
                let flat = move |p: glam::DVec3| (p.dot(ux), p.dot(uy));
                let tris_of = |want: axia_geo::FaceId| -> Vec<[(f64, f64); 3]> {
                    s.mesh
                        .face_tessellation(want)
                        .unwrap_or_default()
                        .into_iter()
                        .map(|t| [flat(t[0]), flat(t[1]), flat(t[2])])
                        .collect()
                };
                let in_tri = |p: (f64, f64), t: &[(f64, f64); 3]| -> bool {
                    let sign = |a: (f64, f64), b: (f64, f64), c: (f64, f64)| {
                        (a.0 - c.0) * (b.1 - c.1) - (b.0 - c.0) * (a.1 - c.1)
                    };
                    let (d1, d2, d3) = (sign(p, t[0], t[1]), sign(p, t[1], t[2]), sign(p, t[2], t[0]));
                    let neg = d1 < 0.0 || d2 < 0.0 || d3 < 0.0;
                    let pos_ = d1 > 0.0 || d2 > 0.0 || d3 > 0.0;
                    !(neg && pos_)
                };
                let (ta, tb) = (tris_of(a), tris_of(b));
                // ⚠ Does the tessellation agree with the area?
                //
                // `face_outer_area` adds `loop_curve_bulge`, so an inward arc
                // makes it SMALLER than the corner polygon. If the triangles
                // still fill the straight-chord polygon, then two faces sharing
                // that arc both cover the bulge — the area says they tile and
                // the geometry says they overlap. Sum the triangles and compare.
                for (f, tris) in [(a, &ta), (b, &tb)] {
                    let area: f64 = tris
                        .iter()
                        .map(|t| {
                            ((t[1].0 - t[0].0) * (t[2].1 - t[0].1)
                                - (t[2].0 - t[0].0) * (t[1].1 - t[0].1))
                                .abs()
                                * 0.5
                        })
                        .sum();
                    println!(
                        "              {f:?} 삼각형 합 {area:.0}   엔진 넓이 {:.0}   차이 {:.0}",
                        s.mesh.face_outer_area(f),
                        area - s.mesh.face_outer_area(f)
                    );
                }
                // ⚠ A pair the sampler says overlaps by 0 mm² is still reported.
                // `coplanar_tris_overlap` is three tests and each excludes shared
                // corners and edges, so replicate them and say WHICH one fires.
                {
                    let n = glam::DVec3::Z;
                    let strictly_in = |p: (f64, f64), t: &[(f64, f64); 3]| -> bool {
                        let cr = |o: (f64, f64), q: (f64, f64), r: (f64, f64)| {
                            (q.0 - o.0) * (r.1 - o.1) - (q.1 - o.1) * (r.0 - o.0)
                        };
                        let (d1, d2, d3) =
                            (cr(t[0], t[1], p), cr(t[1], t[2], p), cr(t[2], t[0], p));
                        (d1 > 0.0 && d2 > 0.0 && d3 > 0.0) || (d1 < 0.0 && d2 < 0.0 && d3 < 0.0)
                    };
                    let cross = |a0: (f64, f64), a1: (f64, f64), b0: (f64, f64), b1: (f64, f64)| {
                        const E: f64 = 1e-7;
                        let da = (a1.0 - a0.0, a1.1 - a0.1);
                        let db = (b1.0 - b0.0, b1.1 - b0.1);
                        let den = da.0 * db.1 - da.1 * db.0;
                        let scale = ((da.0.hypot(da.1)) * (db.0.hypot(db.1))).max(1e-18);
                        if den.abs() < E * scale {
                            return None;
                        }
                        let w = (b0.0 - a0.0, b0.1 - a0.1);
                        let t = (w.0 * db.1 - w.1 * db.0) / den;
                        let u = (w.0 * da.1 - w.1 * da.0) / den;
                        if t > E && t < 1.0 - E && u > E && u < 1.0 - E {
                            Some((t, u))
                        } else {
                            None
                        }
                    };
                    let _ = n;
                    let (mut by_point, mut by_cross) = (0usize, 0usize);
                    let mut near: Vec<f64> = Vec::new();
                    let mut example = String::new();
                    for x in &ta {
                        for y in &tb {
                            for &p in x.iter() {
                                if strictly_in(p, y) {
                                    by_point += 1;
                                }
                            }
                            for &p in y.iter() {
                                if strictly_in(p, x) {
                                    by_point += 1;
                                }
                            }
                            for i in 0..3 {
                                for j in 0..3 {
                                    if let Some((t, u)) =
                                        cross(x[i], x[(i + 1) % 3], y[j], y[(j + 1) % 3])
                                    {
                                        by_cross += 1;
                                        // ⚠ The number that decides whether a
                                        // world-unit tolerance would change
                                        // anything: how far the crossing sits
                                        // from the nearest endpoint, in mm.
                                        let la = (x[(i + 1) % 3].0 - x[i].0)
                                            .hypot(x[(i + 1) % 3].1 - x[i].1);
                                        let lb = (y[(j + 1) % 3].0 - y[j].0)
                                            .hypot(y[(j + 1) % 3].1 - y[j].1);
                                        let d = (t * la)
                                            .min((1.0 - t) * la)
                                            .min(u * lb)
                                            .min((1.0 - u) * lb);
                                        near.push(d);
                                        if example.is_empty() {
                                            example = format!("t={t:.9} u={u:.9}  끝점까지 {d:.9} mm");
                                        }
                                    }
                                }
                            }
                        }
                    }
                    println!(
                        "              무엇이 발화하나: 꼭짓점이 안쪽 {by_point}건, 모서리 교차 {by_cross}건"
                    );
                    if !example.is_empty() {
                        println!("                 첫 교차  {example}");
                    }
                    if !near.is_empty() {
                        near.sort_by(|p, q| p.partial_cmp(q).unwrap());
                        println!(
                            "                 교차→끝점 거리  최소 {:.9}  중앙 {:.9}  최대 {:.9} mm",
                            near[0],
                            near[near.len() / 2],
                            near[near.len() - 1]
                        );
                    }
                }
                let (small_t, big_t) = if s.mesh.face_outer_area(a) < s.mesh.face_outer_area(b) {
                    (&ta, &tb)
                } else {
                    (&tb, &ta)
                };
                // ⚠ Exact, not sampled.
                //
                // The grid said 0 mm² for a pair whose triangles put 21 vertices
                // strictly inside each other, and both cannot be true. Clip each
                // triangle of one against each triangle of the other
                // (Sutherland-Hodgman, convex clipper on a convex subject — the
                // one case where it is exact) and sum. No resolution to miss a
                // thin region with.
                {
                    let clip_poly = |subject: &[(f64, f64)], clip: &[(f64, f64); 3]| -> Vec<(f64, f64)> {
                        // Orient the clip triangle CCW so "inside" is one sign.
                        let area2 = (clip[1].0 - clip[0].0) * (clip[2].1 - clip[0].1)
                            - (clip[2].0 - clip[0].0) * (clip[1].1 - clip[0].1);
                        let c: [(f64, f64); 3] = if area2 >= 0.0 {
                            *clip
                        } else {
                            [clip[0], clip[2], clip[1]]
                        };
                        let mut out: Vec<(f64, f64)> = subject.to_vec();
                        for k in 0..3 {
                            if out.is_empty() {
                                break;
                            }
                            let (e0, e1) = (c[k], c[(k + 1) % 3]);
                            let side = |p: (f64, f64)| {
                                (e1.0 - e0.0) * (p.1 - e0.1) - (e1.1 - e0.1) * (p.0 - e0.0)
                            };
                            let input = std::mem::take(&mut out);
                            for i in 0..input.len() {
                                let cur = input[i];
                                let prev = input[(i + input.len() - 1) % input.len()];
                                let (sc, sp) = (side(cur), side(prev));
                                if sc >= 0.0 {
                                    if sp < 0.0 {
                                        let t = sp / (sp - sc);
                                        out.push((
                                            prev.0 + (cur.0 - prev.0) * t,
                                            prev.1 + (cur.1 - prev.1) * t,
                                        ));
                                    }
                                    out.push(cur);
                                } else if sp >= 0.0 {
                                    let t = sp / (sp - sc);
                                    out.push((
                                        prev.0 + (cur.0 - prev.0) * t,
                                        prev.1 + (cur.1 - prev.1) * t,
                                    ));
                                }
                            }
                        }
                        out
                    };
                    let shoelace = |poly: &[(f64, f64)]| -> f64 {
                        if poly.len() < 3 {
                            return 0.0;
                        }
                        let mut sum = 0.0;
                        for i in 0..poly.len() {
                            let p = poly[i];
                            let q = poly[(i + 1) % poly.len()];
                            sum += p.0 * q.1 - q.0 * p.1;
                        }
                        (sum * 0.5).abs()
                    };
                    let mut exact_overlap = 0.0;
                    for x in &ta {
                        for y in &tb {
                            let piece = clip_poly(&x.to_vec(), y);
                            exact_overlap += shoelace(&piece);
                        }
                    }
                    println!("              정확 계산: 겹친 넓이 {exact_overlap:.3} mm²");
                }
                if small_t.is_empty() || big_t.is_empty() {
                    println!("              ⚠ 삼각형이 없어 넓이를 잴 수 없음 ({} / {})", ta.len(), tb.len());
                } else {
                    let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
                    for t in small_t {
                        for v in t {
                            lo = (lo.0.min(v.0), lo.1.min(v.1));
                            hi = (hi.0.max(v.0), hi.1.max(v.1));
                        }
                    }
                    const M: usize = 400;
                    let (mut in_s, mut in_both) = (0usize, 0usize);
                    for i in 0..M {
                        for j in 0..M {
                            let p = (
                                lo.0 + (hi.0 - lo.0) * (i as f64 + 0.5) / M as f64,
                                lo.1 + (hi.1 - lo.1) * (j as f64 + 0.5) / M as f64,
                            );
                            if small_t.iter().any(|t| in_tri(p, t)) {
                                in_s += 1;
                                if big_t.iter().any(|t| in_tri(p, t)) {
                                    in_both += 1;
                                }
                            }
                        }
                    }
                    let cell = (hi.0 - lo.0) * (hi.1 - lo.1) / (M * M) as f64;
                    println!(
                        "              삼각형 기준: 작은 면 {:.0} mm² 중 겹친 넓이 {:.0} mm²  ({:.2}%)   \
                         (삼각형 {} / {})",
                        in_s as f64 * cell,
                        in_both as f64 * cell,
                        if in_s > 0 { in_both as f64 * 100.0 / in_s as f64 } else { 0.0 },
                        small_t.len(),
                        big_t.len()
                    );
                }
            }

            // ⚠ Two instruments disagreeing about one pair is a shape this repo
            // has met before (LOCKED #105). `classify_contact` says they overlap
            // and the clipper says they do not meet at all, so ask a third way:
            // sample the smaller face's interior and count how much of it lands
            // inside the bigger one. Area, not opinion.
            {
                let flat = |f: axia_geo::FaceId| -> Vec<(f64, f64)> {
                    s.mesh
                        .faces
                        .get(f)
                        .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
                        .unwrap_or_default()
                        .iter()
                        .filter_map(|&v| s.mesh.vertex_pos(v).ok())
                        .map(|p| (p.x, p.y)) // every one of these is on z = 0
                        .collect()
                };
                let inside = |p: (f64, f64), poly: &[(f64, f64)]| -> bool {
                    let mut c = false;
                    let n = poly.len();
                    for i in 0..n {
                        let (x1, y1) = poly[i];
                        let (x2, y2) = poly[(i + 1) % n];
                        if (y1 > p.1) != (y2 > p.1)
                            && p.0 < (x2 - x1) * (p.1 - y1) / (y2 - y1) + x1
                        {
                            c = !c;
                        }
                    }
                    c
                };
                let (pa, pb) = (flat(a), flat(b));
                // ⚠ Does the sampler work at all? A "0% overlap" reading is only
                // worth something if the same test says a face contains its own
                // interior. Both polygons are checked against a point of their
                // own before the answer below is believed.
                for (name, poly) in [("A", &pa), ("B", &pb)] {
                    let c = poly.iter().fold((0.0, 0.0), |acc, p| (acc.0 + p.0, acc.1 + p.1));
                    let c = (c.0 / poly.len() as f64, c.1 / poly.len() as f64);
                    let mut hits = 0;
                    for k in 0..poly.len() {
                        // Midpoint of each corner and the centroid — at least one
                        // lands inside any simple polygon, convex or not.
                        let m = ((poly[k].0 + c.0) / 2.0, (poly[k].1 + c.1) / 2.0);
                        if inside(m, poly) {
                            hits += 1;
                        }
                    }
                    if hits == 0 {
                        println!("              ⚠ 표본기가 {name} 의 내부를 못 읽음 — 아래 수치 믿지 말 것");
                    }
                }
                let (target, other) =
                    if s.mesh.face_outer_area(a) < s.mesh.face_outer_area(b) { (&pa, &pb) } else { (&pb, &pa) };
                let (mut lo, mut hi) = ((f64::MAX, f64::MAX), (f64::MIN, f64::MIN));
                for &(x, y) in target {
                    lo = (lo.0.min(x), lo.1.min(y));
                    hi = (hi.0.max(x), hi.1.max(y));
                }
                const N: usize = 300;
                let (mut in_t, mut in_both) = (0usize, 0usize);
                for i in 0..N {
                    for j in 0..N {
                        let p = (
                            lo.0 + (hi.0 - lo.0) * (i as f64 + 0.5) / N as f64,
                            lo.1 + (hi.1 - lo.1) * (j as f64 + 0.5) / N as f64,
                        );
                        if inside(p, target) {
                            in_t += 1;
                            if inside(p, other) {
                                in_both += 1;
                            }
                        }
                    }
                }
                let cell = (hi.0 - lo.0) * (hi.1 - lo.1) / (N * N) as f64;
                println!(
                    "              작은 면 넓이 {:.0} mm² 중 겹친 넓이 {:.0} mm²  ({:.1}%)",
                    in_t as f64 * cell,
                    in_both as f64 * cell,
                    if in_t > 0 { in_both as f64 * 100.0 / in_t as f64 } else { 0.0 }
                );
            }
        }
        if !offenders.is_empty() {
            let mut v: Vec<(&usize, &usize)> = offenders.iter().collect();
            v.sort_by(|a, b| b.1.cmp(a.1));
            let sizes: std::collections::BTreeMap<usize, usize> =
                comp.values().fold(Default::default(), |mut m, c| {
                    *m.entry(*c).or_default() += 1;
                    m
                });
            println!("      스스로 겹치는 덩어리:");
            for (c, n) in v.iter().take(6) {
                println!("          덩어리 {c} (면 {})   {n} 쌍", sizes[c]);
            }
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
        // Empty set — a file has no "before", so every standing double-cover is
        // fair game. See the note on the method.
        let n = a.subtract_double_covered_faces(&Default::default());
        line("이중덮임 수리만", &health(&a));
        println!("        (면 {n} 개 수리)");
        let (t1, c1, x1) = classify(&a);
        println!("        맞닿음 {t0} -> {t1}   같은 면 겹침 {c0} -> {c1}   뚫고 지나감 {x0} -> {x1}");
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

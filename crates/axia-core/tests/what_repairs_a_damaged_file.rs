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

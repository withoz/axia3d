//! Open a saved `.xia` and say what in it costs time.
//!
//! A user reported a file that made the app hard to work in. "Slow" is not a
//! measurement, so this opens the file the app opens and times the things the
//! app does — restore, the render export, the wireframe export, the invariant
//! scan — and counts what those numbers are made of.
//!
//! ```bash
//! AXIA_XIA="/path/to/file.xia" cargo test --release -p axia-core \
//!     --test what_makes_a_saved_file_slow -- --ignored --nocapture
//! ```
//!
//! ⚠ The container is not the snapshot. A `.xia` written by the app is
//! `"AIXA"` + u32 version + u32 metadata length + that much JSON, and only then
//! the engine snapshot (which carries its own `"AXIA"` magic). Handing the whole
//! file to `restore_scene_snapshot` reads the container as geometry.
//!
//! Ignored by default — it needs a file that is not in this repository.

use axia_core::scene::Scene;
use axia_core::{Command, CommandResult};
use std::time::Instant;

/// Strip the `.xia` container and return the engine snapshot.
fn snapshot_of(bytes: &[u8]) -> Result<(String, Vec<u8>), String> {
    if bytes.len() < 12 || &bytes[0..4] != b"AIXA" {
        // Not a container — assume it is already a bare snapshot.
        return Ok(("(container 없음 — 스냅샷 그대로)".into(), bytes.to_vec()));
    }
    let ver = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
    let mlen = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
    if bytes.len() < 12 + mlen {
        return Err(format!("container 잘림: meta {mlen} 바이트를 담을 수 없음"));
    }
    let meta = String::from_utf8_lossy(&bytes[12..12 + mlen]).to_string();
    Ok((format!("container v{ver}, meta {meta}"), bytes[12 + mlen..].to_vec()))
}

#[test]
#[ignore = "needs AXIA_XIA=<path to a .xia> — run by hand"]
fn what_makes_a_saved_file_slow() {
    let path = match std::env::var("AXIA_XIA") {
        Ok(p) => p,
        Err(_) => {
            println!("\n  AXIA_XIA 를 .xia 경로로 지정하세요.\n");
            return;
        }
    };
    let bytes = std::fs::read(&path).expect("파일을 읽을 수 없음");
    println!("\n  파일 {path}");
    println!("  크기 {} 바이트\n", bytes.len());

    let (desc, snap) = snapshot_of(&bytes).expect("container");
    println!("  {desc}");

    let info = Scene::analyze_snapshot(&snap).expect("analyze");
    println!(
        "  스냅샷 v{}  magic {}  섹션 mesh={} xias={} shapes={} groups={} constraints={}",
        info.version,
        info.has_magic,
        info.sections.mesh,
        info.sections.xias,
        info.sections.shapes,
        info.sections.groups,
        info.sections.constraints
    );
    if let Some(e) = &info.error {
        println!("  ⚠ {e}");
    }

    // ⚠ `restore_scene_snapshot` is the headerless internal form (undo frames).
    // A saved file carries the AXIA header, and handing it to that one reads the
    // header as a length prefix and quietly restores an EMPTY scene — 0 verts,
    // 0 faces, no error. `import_versioned_snapshot` is what the app opens with.
    let mut s = Scene::new();
    let t = Instant::now();
    let restored = s.import_versioned_snapshot(&snap);
    let restore_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("\n  복원 {restore_ms:.0} ms   결과 {restored:?}\n");

    // ── 무엇이 들어 있나 ────────────────────────────────────────────────
    let faces: Vec<_> = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(i, _)| i).collect();
    let verts = s.mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
    let edges = s.mesh.edges.iter().filter(|(_, e)| e.is_active()).count();
    println!("  정점 {verts}   엣지 {edges}   면 {}", faces.len());
    println!("  Shape {}   XIA {}", s.shapes.len(), s.xias.len());

    // 곡면 면은 렌더에서 chord tolerance 로 잘게 쪼개진다 — 개수가 곧 비용.
    let mut curved = 0usize;
    let mut curved_kinds: std::collections::BTreeMap<&str, usize> = Default::default();
    for &f in &faces {
        match s.mesh.face_surface(f) {
            None | Some(axia_geo::surfaces::AnalyticSurface::Plane { .. }) => {}
            Some(k) => {
                curved += 1;
                let name = match k {
                    axia_geo::surfaces::AnalyticSurface::Cylinder { .. } => "Cylinder",
                    axia_geo::surfaces::AnalyticSurface::Sphere { .. } => "Sphere",
                    axia_geo::surfaces::AnalyticSurface::Cone { .. } => "Cone",
                    axia_geo::surfaces::AnalyticSurface::Torus { .. } => "Torus",
                    axia_geo::surfaces::AnalyticSurface::BezierPatch { .. } => "BezierPatch",
                    axia_geo::surfaces::AnalyticSurface::BSplineSurface { .. } => "BSpline",
                    axia_geo::surfaces::AnalyticSurface::NURBSSurface { .. } => "NURBS",
                    _ => "기타",
                };
                *curved_kinds.entry(name).or_default() += 1;
            }
        }
    }
    println!("  곡면 면 {curved}  {curved_kinds:?}");

    // 큰 경계는 arrangement 와 tessellation 양쪽에서 비싸다.
    let mut loop_sizes: Vec<usize> = faces
        .iter()
        .filter_map(|&f| {
            let start = s.mesh.faces.get(f)?.outer().start;
            s.mesh.collect_loop_verts(start).ok().map(|v| v.len())
        })
        .collect();
    loop_sizes.sort_unstable();
    if !loop_sizes.is_empty() {
        let n = loop_sizes.len();
        println!(
            "  면 경계 정점수  중앙 {}  90% {}  최대 {}",
            loop_sizes[n / 2],
            loop_sizes[n * 9 / 10],
            loop_sizes[n - 1]
        );
    }

    // ── 앱이 매번 하는 일의 값 ──────────────────────────────────────────
    let t = Instant::now();
    let (positions, _normals, indices, _face_map, _areas) =
        s.mesh.export_buffers().expect("export_buffers");
    let export_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!(
        "\n  export_buffers {export_ms:.0} ms   삼각형 {}   정점 {}",
        indices.len() / 3,
        positions.len() / 3
    );

    let t = Instant::now();
    let lines = s.mesh.export_edge_lines(20.1);
    let lines_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("  export_edge_lines {lines_ms:.0} ms   선분 {}", lines.len() / 6);

    let t = Instant::now();
    let inv = s.mesh.verify_face_invariants();
    let inv_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("  verify_face_invariants {inv_ms:.0} ms   위반 {}", inv.violations.len());
    for v in inv.violations.iter().take(5) {
        println!("      {v:?}");
    }

    let t = Instant::now();
    let nm = s.mesh.collect_non_manifold_edges();
    let nm_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("  collect_non_manifold_edges {nm_ms:.0} ms   비-manifold 엣지 {}", nm.len());

    let t = Instant::now();
    let si = s.mesh.detect_self_intersections().intersecting_pairs.len();
    let si_ms = t.elapsed().as_secs_f64() * 1000.0;
    println!("  detect_self_intersections {si_ms:.0} ms   자기교차 {si}");

    // ── 그래서 한 번 편집하면 얼마나 걸리나 ─────────────────────────────
    //
    // The counts above are what the file holds; this is what the user feels.
    // Production flags on, the way the app runs.
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;

    // Which stage costs it. Same draw, one flag off at a time, always from the
    // freshly loaded file so the scenes are comparable.
    println!("\n  ── 빈 곳에 사각형 하나, 단계별 ──");
    let combos: [(&str, bool, bool, bool, bool); 6] = [
        ("전부 켬 (앱 기본)", true, true, true, true),
        ("재유도만 끔", true, true, false, true),
        ("자동 교차만 끔", false, true, true, true),
        ("freeform 만 끔", true, true, true, false),
        ("면 합성만 끔", true, false, true, true),
        ("전부 끔", false, false, false, false),
    ];
    // ⚠ Two positions, because they take different code paths. (9000, 9000)
    // looks empty but sits INSIDE the model's 75 m extent, so it goes down the
    // four-line pipeline; 500 km out takes the atomic fast path.
    for (where_, centre) in [
        ("모델 안", glam::DVec3::new(9_000.0, 9_000.0, 0.0)),
        ("모델 밖", glam::DVec3::new(500_000.0, 500_000.0, 0.0)),
    ] {
        println!("    [{where_}]");
        for (name, ai, afs, fr, ff) in combos {
            let mut t2 = Scene::new();
            t2.import_versioned_snapshot(&snap).expect("reload");
            t2.auto_intersect_on_draw = ai;
            t2.auto_face_synthesis_on_draw = afs;
            t2.face_rederive_on_draw = fr;
            t2.freeform_overlap_on_draw = ff;
            let t = Instant::now();
            let _ = t2.execute(Command::DrawRectAsShape {
                center: centre,
                normal: glam::DVec3::Z,
                up: glam::DVec3::X,
                width: 100.0,
                height: 100.0,
            });
            println!("      {name:<22} {:>6.0} ms", t.elapsed().as_secs_f64() * 1000.0);
        }
    }

    // ⚠ Every combination costs the same, so the auto behaviours are not it.
    // What else does one draw pay for on a loaded model? A transaction takes a
    // full scene snapshot before and after, and an empty scene is the control.
    println!("\n  ── 그리기 밖의 값 ──");
    let t = Instant::now();
    let snap_bytes = s.scene_snapshot();
    println!(
        "    scene_snapshot()      {:>6.0} ms   {} 바이트",
        t.elapsed().as_secs_f64() * 1000.0,
        snap_bytes.len()
    );

    // ⚠ `exec_draw_rect` has a fast path: if the new rectangle's AABB touches no
    // active edge and no face AABB, it draws atomically instead of running the
    // four-line pipeline. One oversized face makes that test true everywhere.
    {
        let mut mn = glam::DVec3::splat(f64::INFINITY);
        let mut mx = glam::DVec3::splat(f64::NEG_INFINITY);
        for (_, v) in s.mesh.verts.iter().filter(|(_, v)| v.is_active()) {
            mn = mn.min(v.pos());
            mx = mx.max(v.pos());
        }
        println!(
            "\n  모델 범위  x {:.0}..{:.0}  y {:.0}..{:.0}  z {:.0}..{:.0}",
            mn.x, mx.x, mn.y, mx.y, mn.z, mx.z
        );

        let mut spans: Vec<(f64, axia_geo::FaceId)> = Vec::new();
        for &f in &faces {
            let Some(face) = s.mesh.faces.get(f) else { continue };
            let Ok(vv) = s.mesh.collect_loop_verts(face.outer().start) else { continue };
            let mut a = glam::DVec3::splat(f64::INFINITY);
            let mut b = glam::DVec3::splat(f64::NEG_INFINITY);
            for &v in &vv {
                if let Ok(p) = s.mesh.vertex_pos(v) {
                    a = a.min(p);
                    b = b.max(p);
                }
            }
            if a.x.is_finite() {
                spans.push(((b - a).max_element(), f));
            }
        }
        spans.sort_by(|x, y| y.0.partial_cmp(&x.0).unwrap());
        println!("  가장 큰 면 AABB 다섯:");
        for (span, f) in spans.iter().take(5) {
            println!("      {f:?}  최대변 {span:.0}");
        }
    }

    // The two scans `exec_draw_rect` runs before it decides which path to take.
    {
        let t = Instant::now();
        let n = s
            .mesh
            .edges
            .iter()
            .filter(|(_, e)| e.is_active() && e.class().is_topological())
            .count();
        println!(
            "    엣지 스캔               {:>6.0} ms   ({n} 개)",
            t.elapsed().as_secs_f64() * 1000.0
        );

        let t = Instant::now();
        let mut walked = 0usize;
        for &f in &faces {
            if let Some(face) = s.mesh.faces.get(f) {
                if let Ok(v) = s.mesh.collect_loop_verts(face.outer().start) {
                    walked += v.len();
                }
            }
        }
        println!(
            "    면 경계 스캔            {:>6.0} ms   (정점 {walked})",
            t.elapsed().as_secs_f64() * 1000.0
        );
    }

    // The Xia -> Shape phase does three things per created face: attach a Plane
    // surface, snap the face to it, then re-snapshot. Time each on a real face.
    {
        let mut m = Scene::new();
        m.import_versioned_snapshot(&snap).expect("reload");
        let f = *faces.first().expect("a face");
        let plane = axia_geo::AnalyticSurface::Plane {
            origin: glam::DVec3::ZERO,
            normal: glam::DVec3::Z,
            basis_u: glam::DVec3::X,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        let t = Instant::now();
        m.mesh.set_face_surface(f, Some(plane));
        println!("    set_face_surface       {:>6.0} ms", t.elapsed().as_secs_f64() * 1000.0);

        let p = axia_geo::Plane::from_point_normal(glam::DVec3::ZERO, glam::DVec3::Z);
        let t = Instant::now();
        let _ = axia_geo::operations::plane_snap::snap_face_to_plane(
            &mut m.mesh,
            f,
            &p,
            axia_geo::operations::plane_snap::PLANE_SNAP_OFFSET,
        );
        println!("    snap_face_to_plane     {:>6.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    // Mesh level, under the Scene — `exec_draw_rect` delegates the geometry to
    // this and then does bookkeeping.
    {
        let mut m = Scene::new();
        m.import_versioned_snapshot(&snap).expect("reload");
        let t = Instant::now();
        let _ = m.mesh.draw_rectangle(
            glam::DVec3::new(9_000.0, 9_000.0, 0.0),
            glam::DVec3::Z,
            glam::DVec3::X,
            100.0,
            100.0,
            axia_core::FORM_MATERIAL,
        );
        println!("    mesh.draw_rectangle    {:>6.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    let mut empty = Scene::new();
    empty.auto_intersect_on_draw = true;
    empty.auto_face_synthesis_on_draw = true;
    empty.face_rederive_on_draw = true;
    empty.freeform_overlap_on_draw = true;
    let t = Instant::now();
    let _ = empty.execute(Command::DrawRectAsShape {
        center: glam::DVec3::ZERO,
        normal: glam::DVec3::Z,
        up: glam::DVec3::X,
        width: 100.0,
        height: 100.0,
    });
    println!("    빈 씬에 같은 사각형     {:>6.0} ms", t.elapsed().as_secs_f64() * 1000.0);

    // The fast path, actually taken: outside every face AABB.
    {
        let mut m = Scene::new();
        m.import_versioned_snapshot(&snap).expect("reload");
        m.auto_intersect_on_draw = true;
        m.auto_face_synthesis_on_draw = true;
        m.face_rederive_on_draw = true;
        m.freeform_overlap_on_draw = true;
        let t = Instant::now();
        let _ = m.execute(Command::DrawRectAsShape {
            center: glam::DVec3::new(500_000.0, 500_000.0, 0.0),
            normal: glam::DVec3::Z,
            up: glam::DVec3::X,
            width: 100.0,
            height: 100.0,
        });
        println!(
            "    모델 밖 사각형          {:>6.0} ms   (fast path)",
            t.elapsed().as_secs_f64() * 1000.0
        );
    }
    // One line, inside the model — the unit the unified pipeline is built from.
    {
        let mut m = Scene::new();
        m.import_versioned_snapshot(&snap).expect("reload");
        m.auto_intersect_on_draw = true;
        m.auto_face_synthesis_on_draw = true;
        m.face_rederive_on_draw = true;
        m.freeform_overlap_on_draw = true;
        let t = Instant::now();
        let _ = m.execute(Command::DrawLine {
            start: glam::DVec3::new(9_000.0, 9_000.0, 0.0),
            end: glam::DVec3::new(9_100.0, 9_000.0, 0.0),
            surface_normal: Some(glam::DVec3::Z),
        });
        println!("    모델 안 선 하나         {:>6.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    // ⚠ Position is not it: the far-away rect takes the fast path and is still
    // slow, and a LINE inside the model is 2 ms. So it is the rectangle command.
    // `DrawRectAsShape` = `DrawRect` (the Xia pipeline) + a Xia -> Shape phase.
    for (name, cmd) in [
        (
            "Command::DrawRect (Xia)",
            Command::DrawRect {
                center: glam::DVec3::new(500_000.0, 500_000.0, 0.0),
                normal: glam::DVec3::Z,
                up: glam::DVec3::X,
                width: 100.0,
                height: 100.0,
            },
        ),
        (
            "Command::DrawCircleAsShape",
            Command::DrawCircleAsShape {
                center: glam::DVec3::new(500_000.0, 500_000.0, 0.0),
                normal: glam::DVec3::Z,
                radius: 50.0,
                segments: 24,
            },
        ),
    ] {
        let mut m = Scene::new();
        m.import_versioned_snapshot(&snap).expect("reload");
        m.auto_intersect_on_draw = true;
        m.auto_face_synthesis_on_draw = true;
        m.face_rederive_on_draw = true;
        m.freeform_overlap_on_draw = true;
        let t = Instant::now();
        let _ = m.execute(cmd);
        println!("    {name:<24} {:>6.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    // ⚠ Is it the SIZE or the DAMAGE? `guard_imprint` runs
    // `detect_self_intersections` before and after every face-creating draw, and
    // its own comment cites 7.8 ms on an 888-face scene. Build a clean scene of
    // comparable size and ask the same question of it.
    {
        let mut clean = Scene::new();
        let mut n = 0usize;
        while n < 120 {
            let x = (n % 12) as f64 * 1000.0;
            let y = (n / 12) as f64 * 1000.0;
            let _ = clean.mesh.create_box(
                glam::DVec3::new(x, y, 0.0),
                200.0,
                200.0,
                200.0,
                axia_core::FORM_MATERIAL,
            );
            n += 1;
        }
        let cf = clean.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let t = Instant::now();
        let csi = clean.mesh.detect_self_intersections().intersecting_pairs.len();
        let cms = t.elapsed().as_secs_f64() * 1000.0;
        println!(
            "\n  대조군: 떨어져 있는 상자 120개 — 면 {cf}, 자기교차 {csi}, 스캔 {cms:.0} ms"
        );
        println!("  이 파일             — 면 {}, 자기교차 {si}, 스캔 {si_ms:.0} ms", faces.len());

        let t = Instant::now();
        let _ = clean.execute(Command::DrawRectAsShape {
            center: glam::DVec3::new(500_000.0, 500_000.0, 0.0),
            normal: glam::DVec3::Z,
            up: glam::DVec3::X,
            width: 100.0,
            height: 100.0,
        });
        println!("  대조군에 사각형 하나  {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);
    }

    // Where the damage sits, so it can be looked at rather than guessed about.
    {
        let mut per_face: std::collections::BTreeMap<u32, usize> = Default::default();
        for (a, bb) in s.mesh.detect_self_intersections().intersecting_pairs {
            *per_face.entry(a.raw()).or_default() += 1;
            *per_face.entry(bb.raw()).or_default() += 1;
        }
        let mut worst: Vec<(usize, u32)> = per_face.iter().map(|(f, c)| (*c, *f)).collect();
        worst.sort_by(|a, b| b.0.cmp(&a.0));
        println!("\n  자기교차에 걸린 면 {} 개, 많은 순:", per_face.len());
        for (count, f) in worst.iter().take(10) {
            let owner = s
                .shapes
                .iter()
                .find(|(_, sh)| sh.face_ids.iter().any(|x| x.raw() == *f))
                .map(|(id, sh)| format!("Shape {} \"{}\"", id.raw(), sh.name))
                .unwrap_or_else(|| "(주인 없음)".into());
            println!("      FaceId({f})  {count} 건   {owner}");
        }
    }

    println!("\n  ── 한 번의 편집 ──");
    let far = glam::DVec3::new(9_000.0, 9_000.0, 0.0);
    let t = Instant::now();
    let r = s.execute(Command::DrawRectAsShape {
        center: far,
        normal: glam::DVec3::Z,
        up: glam::DVec3::X,
        width: 100.0,
        height: 100.0,
    });
    println!(
        "  빈 곳에 사각형 하나        {:.0} ms   {}",
        t.elapsed().as_secs_f64() * 1000.0,
        if matches!(r, CommandResult::Error(_)) { "실패" } else { "성공" }
    );

    let t = Instant::now();
    let _ = s.execute(Command::DrawRectAsShape {
        center: glam::DVec3::new(0.0, 0.0, 0.0),
        normal: glam::DVec3::Z,
        up: glam::DVec3::X,
        width: 500.0,
        height: 500.0,
    });
    println!("  모델 한가운데에 사각형     {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);

    let t = Instant::now();
    let _ = s.mesh.export_buffers();
    println!("  그리고 다시 그리기         {:.0} ms", t.elapsed().as_secs_f64() * 1000.0);

    println!();
}

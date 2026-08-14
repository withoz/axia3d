//! C-0: the measurement the plan asks for BEFORE anything is called a defect.
//!
//! Drawing a SINGLE shape on a curved face is already wide — circles on all
//! four surfaces (ADR-202/257/263), freeform and Bezier as open seams, OSNAP
//! on curved hosts (LOCKED #104). What has never been measured is two shapes
//! that meet: 곡면 4종 × {원×원 겹침 · 원×원 포함 · seam 교차}.
//!
//! Nothing here is a fix, and nothing here asserts a wish. It pins what the
//! engine does today so C-2 knows what it is for. Five times this session a
//! plan row turned out to describe a feature that already worked (D5, D7, D8,
//! D9′, and 트랙 B's own premise), so the matrix is asked rather than assumed.
//!
//! ⚠ INSTRUMENTS. A curved face's boundary is one vertex and a curve, so every
//! polygon-derived reader lies about it. This file uses only:
//!
//! ```text
//!   face_outer_area      reads the SURFACE for a curved face (its own comment
//!                        records 314.159 → 0.000 when it did not)
//!   verify_face_invariants / detect_self_intersections   topology, not polygons
//! ```
//!
//! and it finds curved faces BY SURFACE, never by normal — a Path B cylinder's
//! side reports n = (0,0,1) exactly like its caps (trap #6,
//! `what_blocks_a_path_on_a_curved_host.rs`).

use axia_core::scene::Scene;
use axia_geo::surfaces::{cone, cylinder, sphere, torus, AnalyticSurface};
use axia_geo::{FaceId, MaterialId};
use glam::DVec3;

/// Which curved primitive, and how to place points on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Host {
    Cylinder,
    Sphere,
    Cone,
    Torus,
}

impl Host {
    const ALL: [Host; 4] = [Host::Cylinder, Host::Sphere, Host::Cone, Host::Torus];

    fn name(self) -> &'static str {
        match self {
            Host::Cylinder => "cylinder",
            Host::Sphere => "sphere",
            Host::Cone => "cone",
            Host::Torus => "torus",
        }
    }
}

/// Build the primitive with the production Path B defaults, and hand back the
/// ONE curved face to draw on — found by asking the surface.
fn build(host: Host) -> (Scene, FaceId) {
    let mut s = Scene::new();
    let m = MaterialId::new(0);
    s.mesh.set_cylinder_path_b_default(true);
    s.mesh.set_sphere_path_b_default(true);
    s.mesh.set_cone_path_b_default(true);
    s.mesh.set_torus_path_b_default(true);
    match host {
        Host::Cylinder => {
            s.mesh.create_cylinder(DVec3::ZERO, 10.0, 20.0, 16, m).unwrap();
        }
        Host::Sphere => {
            s.mesh.create_sphere(DVec3::ZERO, 10.0, 16, 12, m).unwrap();
        }
        Host::Cone => {
            s.mesh.create_cone(DVec3::ZERO, 10.0, 20.0, 16, m).unwrap();
        }
        Host::Torus => {
            // The torus has no Path A at all — kernel-native from day one
            // (LOCKED #49), so there is no segment count to pass.
            s.mesh
                .create_torus_kernel_native(DVec3::ZERO, 10.0, 3.0, m)
                .unwrap();
        }
    }
    let curved: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| {
            f.is_active()
                && f.surface()
                    .is_some_and(|su| !matches!(su, AnalyticSurface::Plane { .. }))
        })
        .map(|(fid, _)| fid)
        .collect();
    assert!(
        !curved.is_empty(),
        "{}: no curved face — the surface is where the shape lives",
        host.name()
    );
    (s, curved[0])
}

/// A point ON the host surface at parameter (u, v).
fn at(surf: &AnalyticSurface, u: f64, v: f64) -> DVec3 {
    match surf {
        AnalyticSurface::Cylinder {
            axis_origin,
            axis_dir,
            radius,
            ref_dir,
            ..
        } => cylinder::evaluate(*axis_origin, *axis_dir, *radius, *ref_dir, u, v),
        AnalyticSurface::Sphere {
            center,
            radius,
            axis_dir,
            ref_dir,
            ..
        } => sphere::evaluate(*center, *radius, *axis_dir, *ref_dir, u, v),
        AnalyticSurface::Cone {
            apex,
            axis_dir,
            half_angle,
            ref_dir,
            ..
        } => cone::evaluate(*apex, *axis_dir, *half_angle, *ref_dir, u, v),
        AnalyticSurface::Torus {
            center,
            axis_dir,
            ref_dir,
            major_radius,
            minor_radius,
            ..
        } => torus::evaluate(
            *center,
            *axis_dir,
            *ref_dir,
            *major_radius,
            *minor_radius,
            u,
            v,
        ),
        _ => panic!("not a curved surface"),
    }
}

/// The parameter window of the host, and a v that sits comfortably inside it.
fn ranges(surf: &AnalyticSurface) -> ((f64, f64), (f64, f64)) {
    match surf {
        AnalyticSurface::Cylinder { u_range, v_range, .. }
        | AnalyticSurface::Sphere { u_range, v_range, .. }
        | AnalyticSurface::Cone { u_range, v_range, .. }
        | AnalyticSurface::Torus { u_range, v_range, .. } => (*u_range, *v_range),
        _ => panic!("not a curved surface"),
    }
}

/// Draw a circle on whichever curved host it is, by the surface's own entry.
fn draw_circle(s: &mut Scene, host: Host, face: FaceId, c: DVec3, r: DVec3) -> Option<(FaceId, FaceId)> {
    match host {
        Host::Cylinder => s.draw_circle_on_cylinder(face, c, r),
        Host::Sphere => s.draw_circle_on_sphere(face, c, r),
        Host::Cone => s.draw_circle_on_cone(face, c, r),
        Host::Torus => s.draw_circle_on_torus(face, c, r),
    }
}

/// What a scene reads like through surface-aware instruments only.
struct Read {
    faces: usize,
    area: f64,
    valid: bool,
    si: usize,
}

fn read(s: &Scene) -> Read {
    let ids: Vec<FaceId> = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| fid)
        .collect();
    Read {
        faces: ids.len(),
        area: ids.iter().map(|&f| s.mesh.face_outer_area(f)).sum(),
        valid: s.mesh.verify_face_invariants().is_valid(),
        si: s.mesh.detect_self_intersections().count(),
    }
}

/// ONE circle on each curved host — the control. If this ever fails, the
/// overlap rows below are measuring nothing.
#[test]
fn c0_control_one_circle_divides_every_curved_host() {
    for host in Host::ALL {
        let (mut s, face) = build(host);
        let before = read(&s);
        let surf = s.mesh.face_surface(face).unwrap().clone();
        let ((u0, u1), (v0, v1)) = ranges(&surf);
        let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
        let du = 0.15 * (u1 - u0);

        let out = draw_circle(&mut s, host, face, at(&surf, um, vm), at(&surf, um + du, vm));
        let after = read(&s);
        println!(
            "CONTROL {:<9} {:?} faces {}→{} area {:.3}→{:.3} valid={} si={}",
            host.name(),
            out.is_some(),
            before.faces,
            after.faces,
            before.area,
            after.area,
            after.valid,
            after.si
        );
        assert!(out.is_some(), "{}: a lone circle must draw", host.name());
        assert_eq!(
            after.faces,
            before.faces + 1,
            "{}: cap + remainder replaces the host",
            host.name()
        );
        assert!(after.valid, "{}: invariants", host.name());
        assert_eq!(after.si, 0, "{}: no self-intersection", host.name());
        // Area is deliberately NOT asserted here. Dividing a face cannot change
        // how much surface exists, but `face_outer_area` says it does — every
        // host above grows. That is trap #7, pinned in its own test below, and
        // asserting the truth here would just fail for a reason that has
        // nothing to do with whether the circle drew.
    }
}

/// ⚠ SEVENTH instrument that lies about a curved face — and it is the one the
/// plan named for judging this very matrix: **`face_outer_area` after a split**.
///
/// A curved face is read from its surface, which is right (its boundary is a
/// chord net that says nothing about the bulge). But the reader integrates the
/// surface's whole `u_range × v_range`, and a split hands BOTH children the
/// parent's full range — deliberately, so the render can compute each one's
/// sub-slice from its boundary verts (LOCKED #35 A-χ / A-ρ). The render
/// narrows; the area reader does not. So after one circle both pieces report
/// the entire side.
///
/// The reader's own comment claims the opposite — *"a face that covers part of
/// its surface is measured, not assumed"* — which is true of the integral and
/// false of the range it is given.
///
/// Measured on a Ø20 × 20 cylinder: side 1256.637 (= 2πRh). After one circle,
/// cap and remainder report 1256.637 EACH, so the solid's surface appears to
/// grow from 1884.956 to 3141.593 by being drawn on.
///
/// Narrowing the range is not the fix: a geodesic disc on a cylinder is not a
/// u-v rectangle, so no `(u_range, v_range)` can describe it. The area has to
/// come from the same clipped tessellation the render already builds.
#[test]
fn c0_trap7_area_double_counts_after_a_curved_split() {
    let (mut s, face) = build(Host::Cylinder);
    let before: f64 = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| s.mesh.face_outer_area(fid))
        .sum();

    let surf = s.mesh.face_surface(face).unwrap().clone();
    let ((u0, u1), (v0, v1)) = ranges(&surf);
    let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
    let du = 0.15 * (u1 - u0);
    let side_before = s.mesh.face_outer_area(face);

    let (cap, rem) = draw_circle(&mut s, Host::Cylinder, face, at(&surf, um, vm), at(&surf, um + du, vm))
        .expect("one circle draws");

    let (cap_a, rem_a) = (s.mesh.face_outer_area(cap), s.mesh.face_outer_area(rem));
    let after: f64 = s
        .mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| s.mesh.face_outer_area(fid))
        .sum();

    let (cap_u, cap_v) = ranges(s.mesh.face_surface(cap).unwrap());
    let (rem_u, rem_v) = ranges(s.mesh.face_surface(rem).unwrap());
    println!(
        "TRAP7 side {side_before:.3} → cap {cap_a:.3} + rem {rem_a:.3};  \
         total {before:.3} → {after:.3}\n      \
         cap u{cap_u:?} v{cap_v:?}\n      rem u{rem_u:?} v{rem_v:?}"
    );

    // PIN the trap at today's numbers. When the area learns to read the piece
    // instead of the surface, cap + rem sums back to the side and this fails —
    // that is the retire signal.
    assert!(
        (cap_a - side_before).abs() < 1e-6 && (rem_a - side_before).abs() < 1e-6,
        "TODAY both pieces report the WHOLE side ({side_before:.3}): \
         cap {cap_a:.3}, rem {rem_a:.3}"
    );
    assert!(
        (after - (before + side_before)).abs() < 1e-6,
        "so the total grows by one whole side: {before:.3} → {after:.3}"
    );
    // And the reason, stated so the fix is aimed at the right thing: the split
    // handed both children the parent's full parameter window.
    assert_eq!(cap_u, rem_u, "both children keep the parent's u_range");
    assert_eq!(cap_v, rem_v, "and its v_range");
}

/// Where the honest area already lives: the RENDER's clipped tessellation.
///
/// `mesh_export` runs a cascade of per-face clippers before it falls back to
/// filling the whole surface — `tessellate_cylinder_circle_clipped` exists for
/// exactly the face a circle just split. So the render draws each piece
/// correctly while the area reader integrates the whole side; the two disagree
/// because only one of them narrows.
///
/// Measured here so the fix is aimed rather than guessed: if the clipped
/// triangles already sum to the right numbers, correcting `face_outer_area` is
/// wiring, not new geometry.
#[test]
fn c0_the_clipped_tessellation_knows_the_real_area() {
    let (mut s, face) = build(Host::Cylinder);
    let surf = s.mesh.face_surface(face).unwrap().clone();
    let ((u0, u1), (v0, v1)) = ranges(&surf);
    let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
    let du = 0.15 * (u1 - u0);
    let side = s.mesh.face_outer_area(face);
    let r_geo = 10.0 * du; // arc length on a Ø20 cylinder

    let (cap, rem) =
        draw_circle(&mut s, Host::Cylinder, face, at(&surf, um, vm), at(&surf, um + du, vm))
            .expect("one circle draws");

    let sum_tris = |t: &axia_geo::surfaces::SurfaceTessellation| -> f64 {
        t.triangles
            .iter()
            .map(|&[a, b, c]| {
                let (a, b, c) = (
                    t.vertices[a as usize],
                    t.vertices[b as usize],
                    t.vertices[c as usize],
                );
                0.5 * (b - a).cross(c - a).length()
            })
            .sum()
    };
    let clipped = |fid: FaceId| -> Option<(f64, usize)> {
        s.mesh
            .tessellate_cylinder_circle_clipped(fid, 0.02)
            .map(|t| (sum_tris(&t), t.triangles.len()))
    };
    let (cap_t, cap_n) = clipped(cap).expect("cap is clipped");
    let (rem_t, rem_n) = clipped(rem).expect("remainder is clipped");
    println!(
        "CLIPPED side {side:.3}  cap {cap_t:.3}/{cap_n}tri (πr² ≈ {:.3})  \
         rem {rem_t:.3}/{rem_n}tri  sum {:.3}",
        std::f64::consts::PI * r_geo * r_geo,
        cap_t + rem_t
    );

    // The cap is the piece with a closed-form answer, so it is the one that can
    // say whether summing clipped triangles is a sound instrument at all.
    assert!(
        (cap_t - std::f64::consts::PI * r_geo * r_geo).abs()
            < 0.05 * std::f64::consts::PI * r_geo * r_geo,
        "the cap's clipped triangles agree with πr² ({cap_t:.3})"
    );

    // PIN, and it is not what I expected. The two pieces do NOT add up: the
    // remainder alone measures more than the whole side it was cut from. So
    // the render's clip is not a drop-in area either — one of the two clippers
    // covers ground twice. Whatever fixes the area has to fix this first.
    assert!(
        cap_t + rem_t > 1.2 * side,
        "TODAY the pieces over-sum: {cap_t:.3} + {rem_t:.3} = {:.3} vs a side of \
         {side:.3}. When they add up, retire this pin",
        cap_t + rem_t
    );
    assert!(
        rem_t > side,
        "TODAY the remainder alone ({rem_t:.3}) exceeds the whole side \
         ({side:.3}) — that is the part that cannot be right"
    );

    // WHY, measured rather than reasoned. The remainder is earcut in (u, v)
    // parameter space and mapped back, and u is an ANGLE: a triangle joining
    // two points far apart in u is a thin sliver on the chart and a flat slab
    // cutting THROUGH the cylinder in space. Its area is the slab's, not the
    // band's. If the widest triangle spans most of a turn, that is the cause.
    let t = s
        .mesh
        .tessellate_cylinder_circle_clipped(rem, 0.02)
        .expect("remainder tessellation");
    let axis = DVec3::Z;
    let ang = |p: DVec3| p.y.atan2(p.x);
    let mut worst_span = 0.0_f64;
    let mut worst_area = 0.0_f64;
    for &[a, b, c] in &t.triangles {
        let (pa, pb, pc) = (
            t.vertices[a as usize],
            t.vertices[b as usize],
            t.vertices[c as usize],
        );
        let (ua, ub, uc) = (ang(pa), ang(pb), ang(pc));
        // span the short way round, so a seam-straddling triangle is not
        // mistaken for a wide one.
        let d = |x: f64, y: f64| {
            let d = (x - y).abs() % std::f64::consts::TAU;
            d.min(std::f64::consts::TAU - d)
        };
        let span = d(ua, ub).max(d(ub, uc)).max(d(ua, uc));
        let area = 0.5 * (pb - pa).cross(pc - pa).length();
        if span > worst_span {
            worst_span = span;
        }
        if area > worst_area {
            worst_area = area;
        }
    }
    let _ = axis;
    println!(
        "REM SPANS widest triangle spans {:.1}° of the turn; largest single \
         triangle {:.1} mm² (the whole band is {side:.1})",
        worst_span.to_degrees(),
        worst_area
    );
    assert!(
        worst_span > 1.0,
        "TODAY at least one triangle reaches {:.1}° across the cylinder — a \
         flat slab through the solid, not a patch of its skin. That is where \
         the extra area comes from",
        worst_span.to_degrees()
    );

    // And why nobody has SEEN this. A chord slab lies inside the cylinder, so
    // the skin occludes it: the render looks right and only a measurement
    // notices. Stated as a fact rather than an inference — the deep triangles
    // are counted, and their centroids really are inside the radius.
    let radius = 10.0;
    let deep = t
        .triangles
        .iter()
        .filter(|&&[a, b, c]| {
            let (pa, pb, pc) = (
                t.vertices[a as usize],
                t.vertices[b as usize],
                t.vertices[c as usize],
            );
            let ctr = (pa + pb + pc) / 3.0;
            (ctr.x * ctr.x + ctr.y * ctr.y).sqrt() < 0.9 * radius
        })
        .count();
    let radii: Vec<f64> = t.vertices.iter().map(|p| (p.x * p.x + p.y * p.y).sqrt()).collect();
    let (rmin, rmax) = radii.iter().fold((f64::MAX, 0.0f64), |(lo, hi), &r| (lo.min(r), hi.max(r)));
    println!(
        "REM DEPTH {deep} of {} triangles sit inside the skin; vertex radii {rmin:.3}..{rmax:.3}",
        t.triangles.len()
    );
    assert!(
        deep > 0,
        "the extra triangles are INSIDE the cylinder ({deep}), which is why the \
         viewport looks correct while the area does not"
    );
}

/// The end-to-end check, through the buffers the viewport actually draws —
/// and the finding this file exists for.
///
/// Calling one clipper directly says what that clipper returns; it does not say
/// what the render uses. `export_buffers` runs the whole cascade — uv-slice
/// narrowing, every clipper in order, the fallback — so its triangles are the
/// picture. Summing them is the only area measurement that is not an opinion
/// about which code path applies.
///
/// ```text
///   plain Ø20 × 20 cylinder     1882.477   (true 1884.956 — 0.13% under, as
///                                           inscribed polygons should be)
///   after ONE circle on it      2292.511   (+21.6%)
/// ```
///
/// Drawing a circle makes the render emit ~410 mm² of triangles that are not
/// skin. The remainder is earcut in (u, v) and mapped back, and u is an ANGLE:
/// a triangle joining two points far apart in u is a sliver on the chart and a
/// flat slab through the solid in space. The widest here spans **177°**, and 84
/// of the remainder's 133 triangles have centroids well inside the radius.
///
/// Nobody has seen it because a chord slab lies INSIDE the cylinder, where the
/// skin occludes it. The picture looks right; the buffers do not. Anything that
/// measures through them — area, volume, export — is wrong by a fifth after one
/// circle is drawn.
///
/// The cap is fine (273.289 against πr² = 279.056), so the clip is not wrong in
/// principle. What is missing is refinement: uv triangles must be subdivided to
/// the chord tolerance before they are mapped back, exactly as the surface's
/// own tessellator does for the unsplit face.
#[test]
fn c0_the_viewport_draws_slabs_through_the_solid_after_a_curved_split() {
    let (mut s, face) = build(Host::Cylinder);
    let sum_export = |s: &mut Scene| -> f64 {
        let (pos, _n, idx, _fm, _d) = s.mesh.export_buffers().expect("export");
        idx.chunks_exact(3)
            .map(|t| {
                let p = |i: u32| {
                    DVec3::new(
                        pos[3 * i as usize] as f64,
                        pos[3 * i as usize + 1] as f64,
                        pos[3 * i as usize + 2] as f64,
                    )
                };
                let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
                0.5 * (b - a).cross(c - a).length()
            })
            .sum()
    };
    let plain = sum_export(&mut s);

    let surf = s.mesh.face_surface(face).unwrap().clone();
    let ((u0, u1), (v0, v1)) = ranges(&surf);
    let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
    let du = 0.15 * (u1 - u0);
    draw_circle(&mut s, Host::Cylinder, face, at(&surf, um, vm), at(&surf, um + du, vm))
        .expect("one circle draws");
    let drawn = sum_export(&mut s);

    // A Ø20 × 20 cylinder: 2πRh + 2πR² = 1256.637 + 628.319 = 1884.956.
    let truth = 1884.956;
    println!(
        "EXPORT plain {plain:.3}  after one circle {drawn:.3}  (true surface {truth:.3})"
    );
    // The control: before anything is drawn the buffers ARE the cylinder, so
    // the instrument is sound and the number below is not its fault.
    assert!(
        (plain - truth).abs() < 0.02 * truth,
        "the undrawn cylinder's buffers agree with 2πRh + 2πR²: {plain:.3} vs \
         {truth:.3}"
    );
    // PIN. When the uv triangles are refined before map-back, this drops to
    // ~truth and the assertion fails — that is the retire signal, and the
    // control above is what says the fix worked rather than the reader changing.
    assert!(
        drawn > 1.15 * truth,
        "TODAY drawing one circle takes the drawn area from {plain:.3} to \
         {drawn:.3} against a true {truth:.3} — the extra is slabs through the \
         solid. When it lands near {truth:.3}, retire this pin"
    );
}

/// ROW 1 — 원×원 겹침, judged topologically because no area reader can.
///
/// On a PLANE this is what the whole of 트랙 A is about: the pair becomes three
/// regions (A-only, lens, B-only). The measurement below shows the curved host
/// does something else — it adds exactly ONE face, the same as containment
/// does, which is the shape of "the second circle did not notice the first".
///
/// The judgement cannot be area (trap #7 breaks the reader, and the render's
/// own buffers are inflated by slabs) and cannot be `point_in_face` (it tests
/// the face PLANE first, so every point on a curved face reads outside). What
/// is left is topology: **if the two regions were resolved, the first cap must
/// have lost its lens** — its boundary would have changed. It does not.
///
/// Not called a defect here. It is what C-2 is for, and C-0's job was to find
/// out whether there is anything to do.
#[test]
fn c0_row1_the_second_circle_does_not_notice_the_first() {
    // The geometry really does overlap, by construction: equal geodesic radii
    // r, centres 1.2 r apart, so 1.2 r < 2 r.
    assert!(1.2 < 2.0, "the two discs share ground");

    let (mut s, face) = build(Host::Cylinder);
    let surf = s.mesh.face_surface(face).unwrap().clone();
    let ((u0, u1), (v0, v1)) = ranges(&surf);
    let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
    let du = 0.15 * (u1 - u0);

    let (cap_a, rem_a) =
        draw_circle(&mut s, Host::Cylinder, face, at(&surf, um, vm), at(&surf, um + du, vm))
            .expect("first circle");
    let loop_of = |s: &Scene, f: FaceId| -> Option<Vec<axia_geo::VertId>> {
        s.mesh
            .faces
            .get(f)
            .filter(|x| x.is_active())
            .and_then(|x| s.mesh.collect_loop_verts(x.outer().start).ok())
    };
    let before = loop_of(&s, cap_a).expect("cap A is a face");

    let second = draw_circle(
        &mut s,
        Host::Cylinder,
        rem_a,
        at(&surf, um + 1.2 * du, vm),
        at(&surf, um + 2.2 * du, vm),
    );
    assert!(second.is_some(), "the second circle draws");
    let after = loop_of(&s, cap_a);

    println!(
        "ROW1 cap A boundary {} vert(s) → {:?};  faces now {}",
        before.len(),
        after.as_ref().map(|v| v.len()),
        s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
    );

    // PIN. Two overlapping circles resolve into three regions on a plane; here
    // the first cap is untouched, so the ground under the lens belongs to two
    // faces at once. When C-2 teaches the curved host to divide, cap A's
    // boundary changes (or cap A is replaced) and this fails — the retire
    // signal.
    assert_eq!(
        after.as_deref(),
        Some(before.as_slice()),
        "TODAY the first cap is untouched by a circle drawn across it"
    );
    // …and the mesh is sound, which is why nothing else catches it.
    assert!(s.mesh.verify_face_invariants().is_valid(), "invariants hold");
    assert_eq!(
        s.mesh.detect_self_intersections().count(),
        0,
        "and the scan is blind to two faces sharing ground on one surface — \
         the same blindness D5 measured on a plane"
    );
}

/// The same row across all four hosts, kept as the survey it was written to be.
#[test]
fn c0_row1_two_overlapping_circles() {
    for host in Host::ALL {
        let (mut s, face) = build(host);
        let surf = s.mesh.face_surface(face).unwrap().clone();
        let ((u0, u1), (v0, v1)) = ranges(&surf);
        let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
        let du = 0.15 * (u1 - u0);

        let first = draw_circle(&mut s, host, face, at(&surf, um, vm), at(&surf, um + du, vm))
            .map(|(_cap, rem)| rem);
        let mid = read(&s);

        // B's centre sits 1.2 radii away from A's, so their boundaries cross.
        let second = first.and_then(|rem| {
            let c = at(&surf, um + 1.2 * du, vm);
            let r = at(&surf, um + 2.2 * du, vm);
            draw_circle(&mut s, host, rem, c, r)
        });
        let after = read(&s);
        println!(
            "OVERLAP  {:<9} 2nd={:?} faces {}→{} area {:.3}→{:.3} valid={} si={}",
            host.name(),
            second.is_some(),
            mid.faces,
            after.faces,
            mid.area,
            after.area,
            after.valid,
            after.si
        );
        // Whatever the answer, the mesh must not be damaged by asking.
        assert!(after.valid, "{}: invariants after the second draw", host.name());
        assert_eq!(after.si, 0, "{}: no self-intersection", host.name());
    }
}

/// ROW 2 — 원×원 포함. The second circle lies entirely inside the first.
#[test]
fn c0_row2_a_circle_inside_a_circle() {
    for host in Host::ALL {
        let (mut s, face) = build(host);
        let surf = s.mesh.face_surface(face).unwrap().clone();
        let ((u0, u1), (v0, v1)) = ranges(&surf);
        let (um, vm) = (0.5 * (u0 + u1), 0.5 * (v0 + v1));
        let du = 0.15 * (u1 - u0);

        let cap = draw_circle(&mut s, host, face, at(&surf, um, vm), at(&surf, um + du, vm))
            .map(|(cap, _rem)| cap);
        let mid = read(&s);

        // Half the radius, same centre: wholly within.
        let inner = cap.and_then(|c| {
            let ctr = at(&surf, um, vm);
            let rim = at(&surf, um + 0.5 * du, vm);
            draw_circle(&mut s, host, c, ctr, rim)
        });
        let after = read(&s);
        println!(
            "CONTAIN  {:<9} inner={:?} faces {}→{} area {:.3}→{:.3} valid={} si={}",
            host.name(),
            inner.is_some(),
            mid.faces,
            after.faces,
            mid.area,
            after.area,
            after.valid,
            after.si
        );
        assert!(after.valid, "{}: invariants after the inner draw", host.name());
        assert_eq!(after.si, 0, "{}: no self-intersection", host.name());
    }
}

/// ROW 3 — seam 교차. A circle centred ON the parameter seam (u = u_range.0),
/// so it must wrap around the join where the surface closes on itself.
///
/// This is the row with no planar analogue at all: a plane has no seam.
#[test]
fn c0_row3_a_circle_across_the_seam() {
    for host in Host::ALL {
        let (mut s, face) = build(host);
        let before = read(&s);
        let surf = s.mesh.face_surface(face).unwrap().clone();
        let ((u0, u1), (v0, v1)) = ranges(&surf);
        let vm = 0.5 * (v0 + v1);
        let du = 0.15 * (u1 - u0);

        // Centre exactly on the seam; the circle reaches across it both ways.
        let out = draw_circle(&mut s, host, face, at(&surf, u0, vm), at(&surf, u0 + du, vm));
        let after = read(&s);
        println!(
            "SEAM     {:<9} {:?} faces {}→{} area {:.3}→{:.3} valid={} si={}",
            host.name(),
            out.is_some(),
            before.faces,
            after.faces,
            before.area,
            after.area,
            after.valid,
            after.si
        );
        assert!(after.valid, "{}: invariants after a seam-crossing draw", host.name());
        assert_eq!(after.si, 0, "{}: no self-intersection", host.name());
    }
}

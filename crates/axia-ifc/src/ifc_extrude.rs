//! Export-direction parametric profile classification (ADR-203 §42).
//!
//! The engine stores no swept-solid provenance — a wall extruded from a
//! rectangle is just a mesh box by the time it is exported. So to give BIM
//! tools a *parametric* `IfcExtrudedAreaSolid` (editable, tiny, semantic) rather
//! than an opaque brep, this module detects a **polygonal extruded prism**
//! straight from an element's faces and classifies its cap into a parametric
//! [`IfcRectangleProfileDef`] / [`IfcTrapeziumProfileDef`], falling back to a
//! faithful [`IfcArbitraryClosedProfileDef`] (the literal cap polygon) for any
//! other polygon. A shape that is not a clean prism yields `None`, and the
//! caller keeps its existing advanced/faceted brep.
//!
//! Curved-side prisms (circle/ellipse → cylinder side) and prisms whose mesh
//! still carries a baked opening are out of scope here (they return `None`).

use crate::ifc_common::{dir, pt};
use crate::step_value::{EntityRef, StepValue};
use crate::step_writer::StepWriter;
use axia_geo::{FaceId, Mesh};
use glam::DVec3;
use std::collections::HashSet;

/// Unit-normal dot tolerance (parallel / perpendicular tests on normalised dirs).
const NORMAL_EPS: f64 = 1e-4;
/// Cap-vertex congruence tolerance in engine units (~1 µm — matches the mesh's
/// spatial-hash dedup, LOCKED #5).
const CONGRUENCE_TOL: f64 = 1e-3;
/// Right-angle / parallel tolerance for the 2D profile classification, on
/// normalised edge directions.
const DIR_EPS: f64 = 1e-3;

fn newell(pts: &[DVec3]) -> DVec3 {
    let n = pts.len();
    let mut acc = DVec3::ZERO;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        acc.x += (a.y - b.y) * (a.z + b.z);
        acc.y += (a.z - b.z) * (a.x + b.x);
        acc.z += (a.x - b.x) * (a.y + b.y);
    }
    acc
}

fn centroid(pts: &[DVec3]) -> DVec3 {
    let mut acc = DVec3::ZERO;
    for &p in pts {
        acc += p;
    }
    acc / pts.len() as f64
}

/// A detected prism: its bottom cap loop (3D), the unit extrusion axis (bottom →
/// top), and the extrusion depth in engine units.
struct Prism {
    loop3d: Vec<DVec3>,
    axis: DVec3,
    depth: f64,
}

/// Detect a clean polygonal extruded prism from `allowed`'s active faces: exactly
/// two congruent planar caps with antiparallel normals plus one planar quad side
/// per cap edge. Returns `None` for anything else (holes, curved sides, non-prism
/// topology) so the caller falls back to a brep.
fn detect_prism(mesh: &Mesh, allowed: &HashSet<FaceId>) -> Option<Prism> {
    // (loop verts 3D, unit normal) per active face. Iterate in sorted FaceId order
    // so the detected axis (and thus the emitted IFC) is deterministic — the caller
    // passes a HashSet, whose iteration order is not stable.
    let mut ids: Vec<FaceId> = allowed.iter().copied().collect();
    ids.sort_by_key(|f| f.raw());
    let mut faces: Vec<(Vec<DVec3>, DVec3)> = Vec::new();
    for fid in ids {
        let f = mesh.faces.get(fid)?;
        if !f.is_active() {
            continue;
        }
        // A profile with a hole is not an MVP prism (would need a void profile).
        if !f.inners().is_empty() {
            return None;
        }
        let vids = mesh.collect_loop_verts(f.outer().start).ok()?;
        let mut pts = Vec::with_capacity(vids.len());
        for v in vids {
            pts.push(mesh.vertex_pos(v).ok()?);
        }
        if pts.len() < 3 {
            return None;
        }
        let nrm = newell(&pts);
        if nrm.length() < 1e-12 {
            return None; // degenerate face → not a clean prism
        }
        faces.push((pts, nrm.normalize()));
    }

    let m = faces.len();
    // A triangular prism (the smallest) already has 5 faces.
    if m < 5 {
        return None;
    }

    for i in 0..m {
        let ni = faces[i].1;
        // Exactly one face antiparallel to i (the opposite cap)…
        let anti: Vec<usize> =
            (0..m).filter(|&j| j != i && faces[j].1.dot(ni) < -1.0 + NORMAL_EPS).collect();
        if anti.len() != 1 {
            continue;
        }
        let j = anti[0];
        // …and every other face perpendicular to i (the sides run along the axis).
        if !(0..m).filter(|&k| k != i && k != j).all(|k| faces[k].1.dot(ni).abs() < NORMAL_EPS) {
            continue;
        }
        // Caps must share a vertex count, be the only non-quads' partners, and the
        // face total must be exactly caps + one side per cap edge.
        let cap_n = faces[i].0.len();
        if faces[j].0.len() != cap_n || m != cap_n + 2 {
            continue;
        }
        if !(0..m).filter(|&k| k != i && k != j).all(|k| faces[k].0.len() == 4) {
            continue;
        }
        let ci = centroid(&faces[i].0);
        let cj = centroid(&faces[j].0);
        let av = cj - ci;
        let depth = av.length();
        if depth < CONGRUENCE_TOL {
            continue;
        }
        let axis = av / depth;
        // Congruence: every bottom vertex, translated by depth·axis, lands on a top
        // vertex. This is what makes the profile-extrude faithful.
        let top = &faces[j].0;
        let congruent = faces[i].0.iter().all(|&p| {
            let q = p + axis * depth;
            top.iter().any(|&t| (t - q).length() < CONGRUENCE_TOL)
        });
        if !congruent {
            continue;
        }
        return Some(Prism { loop3d: faces[i].0.clone(), axis, depth });
    }
    None
}

/// Signed area of a 2D loop (CCW positive).
fn signed_area(p: &[(f64, f64)]) -> f64 {
    let n = p.len();
    let mut a = 0.0;
    for i in 0..n {
        let (x0, y0) = p[i];
        let (x1, y1) = p[(i + 1) % n];
        a += x0 * y1 - x1 * y0;
    }
    a / 2.0
}

/// A 2D `IfcCartesianPoint`.
fn pt2d(w: &mut StepWriter, x: f64, y: f64) -> EntityRef {
    w.add("IFCCARTESIANPOINT", vec![StepValue::List(vec![StepValue::Real(x), StepValue::Real(y)])])
}

/// An identity `IfcAxis2Placement2D` (origin, +X). The parametric profiles centre
/// and orient themselves through the 3D `IfcExtrudedAreaSolid.Position`, so their
/// own 2D placement is the identity — but written explicitly (not `$`), which
/// stricter readers such as web-ifc expect for a profile's Position.
fn identity_placement_2d(w: &mut StepWriter) -> EntityRef {
    let o = pt2d(w, 0.0, 0.0);
    let d = w.add("IFCDIRECTION", vec![StepValue::List(vec![StepValue::Real(1.0), StepValue::Real(0.0)])]);
    w.add("IFCAXIS2PLACEMENT2D", vec![StepValue::Ref(o), StepValue::Ref(d)])
}

/// A detected right-circular cylinder: the base cap centre, the unit axis (base →
/// top), the radius, the profile's reference direction, and the height.
struct CylPrism {
    base: DVec3,
    axis: DVec3,
    radius: f64,
    ref_dir: DVec3,
    depth: f64,
}

/// Detect a Path B cylinder (§43): exactly three active faces — one
/// `AnalyticSurface::Cylinder` side plus two planar caps perpendicular to its
/// axis, each a hole-free closed-curve loop. Returns `None` for anything else
/// (a tube's annulus caps, a cone, a partial cylinder) so the caller falls back
/// to the advanced brep, which already exports those exactly.
fn detect_cylinder_prism(mesh: &Mesh, allowed: &HashSet<FaceId>) -> Option<CylPrism> {
    use axia_geo::AnalyticSurface;

    let mut ids: Vec<FaceId> = allowed.iter().copied().collect();
    ids.sort_by_key(|f| f.raw());
    let mut side: Option<(f64, DVec3, DVec3, DVec3)> = None; // (radius, origin, axis, ref_dir)
    let mut caps: Vec<DVec3> = Vec::new(); // one boundary point per cap
    let mut active = 0usize;

    for fid in ids {
        let f = mesh.faces.get(fid)?;
        if !f.is_active() {
            continue;
        }
        active += 1;
        match mesh.face_surface(fid) {
            Some(AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, .. }) => {
                if side.is_some() || *radius <= 0.0 {
                    return None; // two cylindrical sides is not a simple cylinder
                }
                side = Some((*radius, *axis_origin, axis_dir.normalize(), ref_dir.normalize()));
            }
            Some(AnalyticSurface::Plane { normal, .. }) => {
                // A cap with a hole would need a void profile — out of scope.
                if !f.inners().is_empty() || caps.len() == 2 {
                    return None;
                }
                let _ = normal;
                let vids = mesh.collect_loop_verts(f.outer().start).ok()?;
                caps.push(mesh.vertex_pos(*vids.first()?).ok()?);
            }
            _ => return None, // sphere / cone / torus / surfaceless → brep path
        }
    }
    if active != 3 || caps.len() != 2 {
        return None;
    }
    let (radius, axis_origin, axis, ref_dir) = side?;

    // Both cap planes must be perpendicular to the axis, i.e. their boundary
    // points sit at distinct heights along it, and both rims must be at the
    // cylinder's radius (the congruence check for the curved case).
    let height_of = |p: DVec3| (p - axis_origin).dot(axis);
    let radial_of = |p: DVec3| {
        let d = p - axis_origin;
        (d - axis * d.dot(axis)).length()
    };
    for &c in &caps {
        if (radial_of(c) - radius).abs() > CONGRUENCE_TOL.max(radius * 1e-6) {
            return None;
        }
    }
    let (h0, h1) = (height_of(caps[0]), height_of(caps[1]));
    let (lo, hi) = if h0 <= h1 { (h0, h1) } else { (h1, h0) };
    let depth = hi - lo;
    if depth < CONGRUENCE_TOL {
        return None;
    }
    Some(CylPrism { base: axis_origin + axis * lo, axis, radius, ref_dir, depth })
}

/// A detected right-circular cone or frustum (§44): the base cap centre, the unit
/// axis (base → top), the two radii, the reference direction, and the height. A
/// full cone has `top_radius == 0`.
struct ConePrism {
    base: DVec3,
    axis: DVec3,
    base_radius: f64,
    top_radius: f64,
    ref_dir: DVec3,
    height: f64,
}

/// Detect a Path B cone or frustum (§44): one `AnalyticSurface::Cone` side plus
/// one planar cap (apex cone) or two (frustum), each hole-free. A cone is not an
/// extrusion — its cross-section changes — so it exports as a revolution, not a
/// sweep. Returns `None` for a partial cone (`u_range` < 2π) or anything else.
fn detect_cone_prism(mesh: &Mesh, allowed: &HashSet<FaceId>) -> Option<ConePrism> {
    use axia_geo::AnalyticSurface;

    let mut ids: Vec<FaceId> = allowed.iter().copied().collect();
    ids.sort_by_key(|f| f.raw());
    // (apex, axis_dir, half_angle, ref_dir, v_range)
    let mut side: Option<(DVec3, DVec3, f64, DVec3, (f64, f64))> = None;
    let mut caps: Vec<DVec3> = Vec::new();
    let mut active = 0usize;

    for fid in ids {
        let f = mesh.faces.get(fid)?;
        if !f.is_active() {
            continue;
        }
        active += 1;
        match mesh.face_surface(fid) {
            Some(AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, u_range, v_range }) => {
                if side.is_some() {
                    return None;
                }
                // Only a full turn is a simple revolution of one profile.
                if (u_range.1 - u_range.0 - std::f64::consts::TAU).abs() > 1e-6 {
                    return None;
                }
                if !(*half_angle > 0.0) || *half_angle >= std::f64::consts::FRAC_PI_2 {
                    return None;
                }
                side = Some((
                    *apex,
                    axis_dir.normalize(),
                    *half_angle,
                    ref_dir.normalize(),
                    *v_range,
                ));
            }
            Some(AnalyticSurface::Plane { .. }) => {
                // A cap with a hole would need a void profile — out of scope.
                if !f.inners().is_empty() || caps.len() == 2 {
                    return None;
                }
                let vids = mesh.collect_loop_verts(f.outer().start).ok()?;
                caps.push(mesh.vertex_pos(*vids.first()?).ok()?);
            }
            _ => return None,
        }
    }
    let (apex, axis, half_angle, ref_dir, (v0, v1)) = side?;
    // An apex cone is 1 cap + 1 side; a frustum is 2 caps + 1 side.
    let expected_caps = if v0.abs() <= CONGRUENCE_TOL { 1 } else { 2 };
    if active != expected_caps + 1 || caps.len() != expected_caps {
        return None;
    }
    // Radius at a distance v from the apex along the axis.
    let radius_at = |v: f64| v * half_angle.tan();
    let (v_lo, v_hi) = if v0 <= v1 { (v0, v1) } else { (v1, v0) };
    let height = v_hi - v_lo;
    if height < CONGRUENCE_TOL || v_lo < -CONGRUENCE_TOL {
        return None;
    }
    let base_radius = radius_at(v_hi);
    let top_radius = radius_at(v_lo);
    if base_radius <= CONGRUENCE_TOL {
        return None;
    }
    // Congruence: every cap boundary point must sit on the cone at its own height.
    for &c in &caps {
        let d = c - apex;
        let v = d.dot(axis);
        let radial = (d - axis * v).length();
        let tol = CONGRUENCE_TOL.max(base_radius * 1e-6);
        if (radial - radius_at(v)).abs() > tol || v < v_lo - tol || v > v_hi + tol {
            return None;
        }
    }
    // The base is the wide end (v_hi); the profile is revolved from there back
    // toward the apex, so the axis points base → top (apex side).
    Some(ConePrism {
        base: apex + axis * v_hi,
        axis: -axis,
        base_radius,
        top_radius,
        ref_dir,
        height,
    })
}

/// Try to emit `allowed` as an `IfcExtrudedAreaSolid` with a classified profile.
/// Returns the solid's ref (profile + placement + solid all emitted to `w`) when
/// the faces form a clean polygonal prism (§42) or a right-circular cylinder
/// (§43), else `None`. `scale` = engine-units → metre.
pub fn try_extruded_area_solid(
    w: &mut StepWriter,
    mesh: &Mesh,
    allowed: &HashSet<FaceId>,
    scale: f64,
) -> Option<EntityRef> {
    // §44 — a cone/frustum's cross-section changes along the axis, so it is a
    // revolution, not a sweep: revolve its meridian (a triangle for an apex cone,
    // a trapezium for a frustum) a full turn about the axis.
    if let Some(c) = detect_cone_prism(mesh, allowed) {
        // Profile plane: local X = ref_dir (radial), local Y = the cone axis, so
        // local Z = X × Y. The meridian lies in that plane with the axis at u = 0.
        let z = c.ref_dir.cross(c.axis);
        if z.length() > 0.5 {
            let meridian: Vec<(f64, f64)> = if c.top_radius <= CONGRUENCE_TOL {
                // Apex cone: base radius down to the tip.
                vec![(0.0, 0.0), (c.base_radius, 0.0), (0.0, c.height)]
            } else {
                vec![(0.0, 0.0), (c.base_radius, 0.0), (c.top_radius, c.height), (0.0, c.height)]
            };
            let (profile, _, _) = emit_arbitrary(w, &meridian, c.base, c.ref_dir, scale);
            return Some(emit_revolved(
                w,
                profile,
                c.base,
                z.normalize(),
                c.ref_dir,
                c.axis,
                scale,
            ));
        }
    }
    // §43 — a Path B cylinder is a circle swept along its axis.
    if let Some(c) = detect_cylinder_prism(mesh, allowed) {
        let pos2d = identity_placement_2d(w);
        let profile = w.add(
            "IFCCIRCLEPROFILEDEF",
            vec![
                StepValue::Enum("AREA".into()),
                StepValue::Unset,
                StepValue::Ref(pos2d),
                StepValue::Real(c.radius * scale),
            ],
        );
        return Some(emit_swept(w, profile, c.base, c.axis, c.ref_dir, c.depth, scale));
    }
    let prism = detect_prism(mesh, allowed)?;
    let cen = centroid(&prism.loop3d);
    // In-plane basis: u along the first cap edge, v = axis × u (right-handed so
    // u × v = axis).
    let u = (prism.loop3d[1] - prism.loop3d[0]).normalize_or_zero();
    if u.length() < 0.5 {
        return None;
    }
    let v = prism.axis.cross(u);
    if v.length() < 0.5 {
        return None;
    }
    let v = v.normalize();
    // Cap loop as 2D points (engine units) about the centroid, forced CCW.
    let mut p2: Vec<(f64, f64)> =
        prism.loop3d.iter().map(|p| { let d = *p - cen; (d.dot(u), d.dot(v)) }).collect();
    if signed_area(&p2) < 0.0 {
        p2.reverse();
    }

    // Classify. Each returns (profile ref, placement origin in engine units, world
    // x-direction for the placement). Rectangle & trapezium re-centre onto their
    // parametric convention; the arbitrary polygon keeps the centroid frame.
    let (profile, origin3d, xdir3d) = classify_rectangle(w, &p2, cen, u, v, scale)
        .or_else(|| classify_trapezium(w, &p2, cen, u, v, scale))
        .unwrap_or_else(|| emit_arbitrary(w, &p2, cen, u, scale));

    Some(emit_swept(w, profile, origin3d, prism.axis, xdir3d, prism.depth, scale))
}

/// The cross-section a line member is thick with, as the exporter sees it.
///
/// Mirrors `axia_core::profile::Profile` without depending on it — this crate
/// writes IFC and knows nothing about the scene.
#[derive(Clone, Debug, PartialEq)]
pub enum SectionProfile {
    Rectangular { width: f64, height: f64 },
    Circular { radius: f64 },
    /// A closed outline in the section's own plane, points not repeated. This is
    /// where the parametric structural shapes (I, T, U, L, C, Z) arrive, already
    /// flattened by the reader that produced them.
    Polygon(Vec<(f64, f64)>),
}

/// A member that is a line with a section: a column, a beam, a brace.
#[derive(Clone, Debug, PartialEq)]
pub struct LineMember {
    pub start: DVec3,
    pub end: DVec3,
    pub profile: SectionProfile,
}

/// Sweep a line member's section along its own length.
///
/// The section sits at the start, square to the line, and is extruded to the
/// far end — which is what a column or a beam is. Returns `None` for a line of
/// no length or a section of no area, because either would write a solid that
/// encloses nothing.
///
/// The section's local X is chosen, not given: a line says which way it runs but
/// not which way its section is turned. World +Z projected square to the line is
/// used where that is possible, so an upright column's section keeps its width
/// horizontal; for a vertical line, +X. A member that needs its section rotated
/// about its own axis needs that rotation stated, and that is not something this
/// engine records yet.
pub fn emit_line_member(
    w: &mut StepWriter,
    member: &LineMember,
    scale: f64,
) -> Option<EntityRef> {
    let span = member.end - member.start;
    let depth = span.length();
    if depth < 1e-9 {
        return None;
    }
    let axis = span / depth;
    // The section stands level: width across, height upright.
    //
    // The profile lies in the placement's local XY with width along RefDir, so
    // RefDir has to be the HORIZONTAL one. Taking `up` squared to the axis put
    // width vertical instead, and a 200×400 beam came out lying on its side —
    // 200 tall, 400 across (measured). `Z × axis` is horizontal by construction,
    // which leaves local Y = `axis × RefDir` = up squared to the axis. That is
    // what the old comment already said it wanted.
    //
    // A vertical member has no horizontal `Z × axis`, and no preferred way round
    // either — both of its section axes are horizontal — so it takes +X. Which
    // way a section faces about its own axis is the user's to say, and it is not
    // said yet: see `EdgeSection`.
    let horizontal = DVec3::Z.cross(axis);
    let ref_dir = if horizontal.length() > 1e-6 {
        horizontal.normalize()
    } else {
        DVec3::X
    };

    let pos2d = identity_placement_2d(w);
    let profile = match &member.profile {
        SectionProfile::Rectangular { width, height } => {
            if !(*width > 0.0 && *height > 0.0) {
                return None;
            }
            w.add(
                "IFCRECTANGLEPROFILEDEF",
                vec![
                    StepValue::Enum("AREA".into()),
                    StepValue::Unset,
                    StepValue::Ref(pos2d),
                    StepValue::Real(width * scale),
                    StepValue::Real(height * scale),
                ],
            )
        }
        SectionProfile::Circular { radius } => {
            if !(*radius > 0.0) {
                return None;
            }
            w.add(
                "IFCCIRCLEPROFILEDEF",
                vec![
                    StepValue::Enum("AREA".into()),
                    StepValue::Unset,
                    StepValue::Ref(pos2d),
                    StepValue::Real(radius * scale),
                ],
            )
        }
        SectionProfile::Polygon(points) => {
            if points.len() < 3 {
                return None;
            }
            // Forced CCW, as IFC expects an outer boundary to be.
            let mut p2 = points.clone();
            if signed_area(&p2) < 0.0 {
                p2.reverse();
            }
            emit_arbitrary_2d(w, &p2, scale)
        }
    };
    Some(emit_swept(w, profile, member.start, axis, ref_dir, depth, scale))
}

/// Place a classified profile and sweep it: `IfcAxis2Placement3D(origin, axis,
/// ref_dir)` + a local `+Z` `ExtrudedDirection` + the depth.
fn emit_swept(
    w: &mut StepWriter,
    profile: EntityRef,
    origin: DVec3,
    axis: DVec3,
    ref_dir: DVec3,
    depth: f64,
    scale: f64,
) -> EntityRef {
    let loc = pt(w, origin * scale);
    let axis_d = dir(w, axis);
    let ref_d = dir(w, ref_dir);
    let pos = w.add(
        "IFCAXIS2PLACEMENT3D",
        vec![StepValue::Ref(loc), StepValue::Ref(axis_d), StepValue::Ref(ref_d)],
    );
    // ExtrudedDirection is local: +Z of the placement = the sweep axis.
    let ext_dir = dir(w, DVec3::Z);
    w.add(
        "IFCEXTRUDEDAREASOLID",
        vec![
            StepValue::Ref(profile),
            StepValue::Ref(pos),
            StepValue::Ref(ext_dir),
            StepValue::Real(depth * scale),
        ],
    )
}

/// Place a meridian profile and revolve it a full turn: `IfcRevolvedAreaSolid`
/// with `IfcAxis2Placement3D(origin, plane_normal, ref_dir)` — so the profile's
/// local X is `ref_dir` and its local Y is the revolution axis — plus an
/// `IfcAxis1Placement(origin, axis)`. Per IFC4 the revolution axis is given in the
/// same object coordinate system as `Position`, and it must lie in the profile's
/// plane, which it does here (local Y).
fn emit_revolved(
    w: &mut StepWriter,
    profile: EntityRef,
    origin: DVec3,
    plane_normal: DVec3,
    ref_dir: DVec3,
    axis: DVec3,
    scale: f64,
) -> EntityRef {
    let loc = pt(w, origin * scale);
    let n_d = dir(w, plane_normal);
    let ref_d = dir(w, ref_dir);
    let pos = w.add(
        "IFCAXIS2PLACEMENT3D",
        vec![StepValue::Ref(loc), StepValue::Ref(n_d), StepValue::Ref(ref_d)],
    );
    let axis_loc = pt(w, origin * scale);
    let axis_d = dir(w, axis);
    let axis1 = w.add(
        "IFCAXIS1PLACEMENT",
        vec![StepValue::Ref(axis_loc), StepValue::Ref(axis_d)],
    );
    w.add(
        "IFCREVOLVEDAREASOLID",
        vec![
            StepValue::Ref(profile),
            StepValue::Ref(pos),
            StepValue::Ref(axis1),
            StepValue::Real(std::f64::consts::TAU),
        ],
    )
}

/// Map a 2D point in the `(u, v)` frame to a world point (engine units).
fn to_world(cen: DVec3, u: DVec3, v: DVec3, x: f64, y: f64) -> DVec3 {
    cen + u * x + v * y
}

/// A 4-corner loop with right angles and equal opposite sides → an axis-aligned
/// `IfcRectangleProfileDef` centred on the cap centroid.
fn classify_rectangle(
    w: &mut StepWriter,
    p2: &[(f64, f64)],
    cen: DVec3,
    u: DVec3,
    v: DVec3,
    scale: f64,
) -> Option<(EntityRef, DVec3, DVec3)> {
    if p2.len() != 4 {
        return None;
    }
    let edge = |k: usize| {
        let (x0, y0) = p2[k];
        let (x1, y1) = p2[(k + 1) % 4];
        (x1 - x0, y1 - y0)
    };
    let len = |(x, y): (f64, f64)| (x * x + y * y).sqrt();
    let dot = |a: (f64, f64), b: (f64, f64)| a.0 * b.0 + a.1 * b.1;
    let (e0, e1, e2, e3) = (edge(0), edge(1), edge(2), edge(3));
    let (l0, l1, l2, l3) = (len(e0), len(e1), len(e2), len(e3));
    if l0 < CONGRUENCE_TOL || l1 < CONGRUENCE_TOL {
        return None;
    }
    // Right angles at every corner + equal opposite sides.
    let perp = |a: (f64, f64), la: f64, b: (f64, f64), lb: f64| (dot(a, b) / (la * lb)).abs() < DIR_EPS;
    if !(perp(e0, l0, e1, l1) && perp(e1, l1, e2, l2) && perp(e2, l2, e3, l3) && perp(e3, l3, e0, l0)) {
        return None;
    }
    if (l0 - l2).abs() > CONGRUENCE_TOL || (l1 - l3).abs() > CONGRUENCE_TOL {
        return None;
    }
    let (xdim, ydim) = (l0, l1);
    // Centre of the rectangle (mean of corners) in the (u,v) frame.
    let cx = p2.iter().map(|p| p.0).sum::<f64>() / 4.0;
    let cy = p2.iter().map(|p| p.1).sum::<f64>() / 4.0;
    let origin = to_world(cen, u, v, cx, cy);
    // X of the placement runs along edge 0 (the XDim edge).
    let (e0x, e0y) = e0;
    let xdir = (u * e0x + v * e0y).normalize();
    let pos2d = identity_placement_2d(w);
    let profile = w.add(
        "IFCRECTANGLEPROFILEDEF",
        vec![
            StepValue::Enum("AREA".into()),
            StepValue::Unset,
            StepValue::Ref(pos2d), // identity — the 3D placement re-centres
            StepValue::Real(xdim * scale),
            StepValue::Real(ydim * scale),
        ],
    );
    Some((profile, origin, xdir))
}

/// A 4-corner loop with exactly one pair of parallel opposite edges → an
/// `IfcTrapeziumProfileDef` (bottom edge along local X, box-centre origin).
fn classify_trapezium(
    w: &mut StepWriter,
    p2: &[(f64, f64)],
    cen: DVec3,
    u: DVec3,
    v: DVec3,
    scale: f64,
) -> Option<(EntityRef, DVec3, DVec3)> {
    if p2.len() != 4 {
        return None;
    }
    let edge = |k: usize| {
        let (x0, y0) = p2[k];
        let (x1, y1) = p2[(k + 1) % 4];
        (x1 - x0, y1 - y0)
    };
    let len = |(x, y): (f64, f64)| (x * x + y * y).sqrt();
    let cross = |a: (f64, f64), b: (f64, f64)| a.0 * b.1 - a.1 * b.0;
    let (e0, e1, e2, e3) = (edge(0), edge(1), edge(2), edge(3));
    let par = |a: (f64, f64), b: (f64, f64)| {
        let (la, lb) = (len(a), len(b));
        la > CONGRUENCE_TOL && lb > CONGRUENCE_TOL && (cross(a, b) / (la * lb)).abs() < DIR_EPS
    };
    let p02 = par(e0, e2);
    let p13 = par(e1, e3);
    // Exactly one parallel pair. Both → parallelogram/rectangle (not our case).
    let (pa, pb) = if p02 && !p13 {
        (0, 2)
    } else if p13 && !p02 {
        (1, 3)
    } else {
        return None;
    };
    // Canonicalise: the longer parallel edge is the bottom, so the same trapezium
    // always exports the same parameters regardless of vertex order.
    let bottom_edge = if len(edge(pa)) >= len(edge(pb)) { pa } else { pb };
    // Bottom edge endpoints (indices k, k+1); the parallel top edge is k+2 → k+3.
    let k = bottom_edge;
    let mut bl = p2[k];
    let mut br = p2[(k + 1) % 4];
    // Top edge, walked the same rotational way: its two vertices are k+2, k+3.
    let ta = p2[(k + 2) % 4];
    let tb = p2[(k + 3) % 4];
    // Local X along the bottom edge; Y perpendicular (+90°). If the top edge is on
    // the −Y side, reverse the bottom direction so +Y points toward the top while
    // the frame stays right-handed (yhat always = xhat rotated +90°).
    let bx = (br.0 - bl.0, br.1 - bl.1);
    let bxl = len(bx);
    if bxl < CONGRUENCE_TOL {
        return None;
    }
    let mut xhat = (bx.0 / bxl, bx.1 / bxl);
    let mut yhat = (-xhat.1, xhat.0);
    let rel = (ta.0 - bl.0, ta.1 - bl.1);
    if rel.0 * yhat.0 + rel.1 * yhat.1 < 0.0 {
        std::mem::swap(&mut bl, &mut br);
        xhat = (-xhat.0, -xhat.1);
        yhat = (-yhat.0, -yhat.1);
    }
    let ydim = ((ta.0 - bl.0) * yhat.0 + (ta.1 - bl.1) * yhat.1).abs();
    if ydim < CONGRUENCE_TOL {
        return None;
    }
    // Project endpoints onto local X (origin at the new bottom-left).
    let px = |q: (f64, f64)| (q.0 - bl.0) * xhat.0 + (q.1 - bl.1) * xhat.1;
    let ta_x = px(ta);
    let tb_x = px(tb);
    let bottom_x = bxl; // bottom edge length
    // Top endpoints ordered by X (left first).
    let (top_lx, top_rx) = if ta_x <= tb_x { (ta_x, tb_x) } else { (tb_x, ta_x) };
    let top_x = top_rx - top_lx;
    if bottom_x < CONGRUENCE_TOL || top_x < CONGRUENCE_TOL {
        return None;
    }
    // Box centre = midpoint of the bottom edge, lifted YDim/2 toward the top.
    let bmid = ((bl.0 + br.0) * 0.5, (bl.1 + br.1) * 0.5);
    let cx2 = bmid.0 + yhat.0 * (ydim * 0.5);
    let cy2 = bmid.1 + yhat.1 * (ydim * 0.5);
    let origin = to_world(cen, u, v, cx2, cy2);
    let xdir = (u * xhat.0 + v * xhat.1).normalize();
    // TopXOffset: local x of the top-left corner from the bottom-left. The bottom
    // edge runs [0, bottom_x] from the bottom-left, so centring both edges by
    // −bottom_x/2 cancels and the offset is just the top-left's local x.
    let top_offset = top_lx;
    let pos2d = identity_placement_2d(w);
    let profile = w.add(
        "IFCTRAPEZIUMPROFILEDEF",
        vec![
            StepValue::Enum("AREA".into()),
            StepValue::Unset,
            StepValue::Ref(pos2d),
            StepValue::Real(bottom_x * scale),
            StepValue::Real(top_x * scale),
            StepValue::Real(ydim * scale),
            StepValue::Real(top_offset * scale),
        ],
    );
    Some((profile, origin, xdir))
}

/// Any other polygon → a faithful `IfcArbitraryClosedProfileDef` whose outer curve
/// is the literal cap polyline, in the centroid `(u, ·)` frame.
fn emit_arbitrary(
    w: &mut StepWriter,
    p2: &[(f64, f64)],
    cen: DVec3,
    u: DVec3,
    scale: f64,
) -> (EntityRef, DVec3, DVec3) {
    (emit_arbitrary_2d(w, p2, scale), cen, u)
}

/// The outline itself, as an `IfcArbitraryClosedProfileDef`.
fn emit_arbitrary_2d(w: &mut StepWriter, p2: &[(f64, f64)], scale: f64) -> EntityRef {
    let mut refs: Vec<StepValue> =
        p2.iter().map(|&(x, y)| StepValue::Ref(pt2d(w, x * scale, y * scale))).collect();
    // Closed polyline repeats its first point.
    if let Some(StepValue::Ref(first)) = refs.first().cloned() {
        refs.push(StepValue::Ref(first));
    }
    let poly = w.add("IFCPOLYLINE", vec![StepValue::List(refs)]);
    w.add(
        "IFCARBITRARYCLOSEDPROFILEDEF",
        vec![StepValue::Enum("AREA".into()), StepValue::Unset, StepValue::Ref(poly)],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build_with<F: FnOnce(&mut StepWriter)>(f: F) -> String {
        let mut w = StepWriter::new();
        f(&mut w);
        w.build()
    }

    #[test]
    fn rectangle_classifier_emits_a_rectangle_profile() {
        let (cen, u, v) = (DVec3::ZERO, DVec3::X, DVec3::Y);
        // 2 × 1 rectangle centred at the origin.
        let rect = vec![(-1.0, -0.5), (1.0, -0.5), (1.0, 0.5), (-1.0, 0.5)];
        let s = build_with(|w| assert!(classify_rectangle(w, &rect, cen, u, v, 1.0).is_some()));
        let line = s.lines().find(|l| l.contains("IFCRECTANGLEPROFILEDEF(")).unwrap();
        assert!(line.ends_with(",2.,1.);"), "XDim 2, YDim 1: {}", line);
        // A trapezoid is not a rectangle (no right angles).
        let trap = vec![(-2.0, -0.5), (2.0, -0.5), (1.0, 0.5), (-1.0, 0.5)];
        let mut w = StepWriter::new();
        assert!(classify_rectangle(&mut w, &trap, cen, u, v, 1.0).is_none());
    }

    #[test]
    fn trapezium_classifier_matches_the_import_convention() {
        let (cen, u, v) = (DVec3::ZERO, DVec3::X, DVec3::Y);
        // Isosceles trapezium: bottom 4, top 2, height 1, top centred → offset 1.
        // Exactly the shape the import test builds, so export ↔ import round-trips.
        let trap = vec![(-2.0, -0.5), (2.0, -0.5), (1.0, 0.5), (-1.0, 0.5)];
        let s = build_with(|w| assert!(classify_trapezium(w, &trap, cen, u, v, 1.0).is_some()));
        let line = s.lines().find(|l| l.contains("IFCTRAPEZIUMPROFILEDEF(")).unwrap();
        assert!(line.ends_with(",4.,2.,1.,1.);"), "bottom 4, top 2, height 1, offset 1: {}", line);
        // A rectangle has two parallel pairs → not a (one-pair) trapezium.
        let rect = vec![(-1.0, -0.5), (1.0, -0.5), (1.0, 0.5), (-1.0, 0.5)];
        let mut w = StepWriter::new();
        assert!(classify_trapezium(&mut w, &rect, cen, u, v, 1.0).is_none());
    }

    #[test]
    fn trapezium_classifier_is_orientation_independent() {
        // Same trapezium but wound the other way / rotated frame still yields the
        // same parametric dims (the Y-flip reverses the bottom edge correctly).
        let (cen, u, v) = (DVec3::ZERO, DVec3::X, DVec3::Y);
        // Top edge listed first (so the parallel pair is on the +Y side initially).
        let trap = vec![(-1.0, 0.5), (1.0, 0.5), (2.0, -0.5), (-2.0, -0.5)];
        let s = build_with(|w| assert!(classify_trapezium(w, &trap, cen, u, v, 1.0).is_some()));
        let line = s.lines().find(|l| l.contains("IFCTRAPEZIUMPROFILEDEF(")).unwrap();
        assert!(line.ends_with(",4.,2.,1.,1.);"), "bottom 4, top 2, height 1, offset 1: {}", line);
    }

    #[test]
    fn arbitrary_classifier_emits_a_closed_polyline() {
        let (cen, u) = (DVec3::ZERO, DVec3::X);
        // An L-shaped hexagon → arbitrary profile.
        let l = vec![(0.0, 0.0), (2.0, 0.0), (2.0, 1.0), (1.0, 1.0), (1.0, 2.0), (0.0, 2.0)];
        let s = build_with(|w| { emit_arbitrary(w, &l, cen, u, 1.0); });
        assert!(s.contains("IFCARBITRARYCLOSEDPROFILEDEF("), "{}", s);
        assert_eq!(s.matches("IFCPOLYLINE(").count(), 1);
        // 6 distinct corner points; the closing repeat reuses the first point's ref.
        assert_eq!(s.matches("IFCCARTESIANPOINT(").count(), 6, "closed polyline: {}", s);
    }
}


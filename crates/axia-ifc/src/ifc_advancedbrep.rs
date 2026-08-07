//! IfcAdvancedBrep emitter (ADR-203 β-2) — analytic surfaces + edge loops.
//!
//! Where [`crate::ifc_facetedbrep`] emits a triangle soup (`IfcFacetedBrep`),
//! this emits a true B-rep whose faces carry their **analytic surface**
//! (`IfcPlane` / `IfcCylindricalSurface` / `IfcSphericalSurface` /
//! `IfcConicalSurface` / `IfcToroidalSurface`, mapped 1:1 from axia-geo's
//! [`AnalyticSurface`]) and whose boundaries are `IfcEdgeLoop`s of
//! `IfcOrientedEdge` → `IfcEdgeCurve`.
//!
//! **Coverage** (β-2 planar → β-3 circular → β-3b spline):
//! - Surfaces: `Plane` / `Cylinder` / `Sphere` / `Cone` / `Torus` → the matching
//!   elementary IFC surface; `BezierPatch` / `BSplineSurface` / `NURBSSurface` →
//!   `IFCBSPLINESURFACEWITHKNOTS` (rational form when weighted).
//! - Edges: `Line` → `IFCLINE`, `Circle`/`Arc` → `IFCCIRCLE` (trimmed by the
//!   edge's vertices), `Bezier`/`BSpline`/`NURBS` → `IFCBSPLINECURVEWITHKNOTS`
//!   (rational form when weighted). A Bezier is emitted as a B-spline with
//!   synthesized clamped knots.
//! - Knots are compressed to IFC's distinct-values + multiplicities form.
//!
//! **Units**: face fields (surface + loop verts) are in engine units (mm); the
//! emitter multiplies every position/radius by `scale` (0.001 for mm→metre).
//! Directions, angles, knots and weights are unitless and are *not* scaled.

use crate::ifc_common::{emit_owner_units_context, emit_product_and_spatial, dir, placement_axes, pt};
use crate::step_value::{EntityRef, StepValue};
use crate::step_writer::StepWriter;
use axia_geo::{AnalyticCurve, AnalyticSurface, HeId, Mesh};
use glam::DVec3;

/// Geometry of one boundary edge (ADR-203 β-3). β-3 maps `Line` + `Circle`/`Arc`;
/// Spline edges (Bezier/BSpline/NURBS) map to `IfcBSplineCurveWithKnots`
/// (β-3b); NURBS-class surfaces map to `IfcBSplineSurfaceWithKnots`.
#[derive(Clone, Debug)]
pub enum EdgeCurve {
    /// Straight segment (the default edge). Emits `IFCLINE`.
    Line,
    /// Full circle / circular support curve. Emits `IFCCIRCLE`; the edge's
    /// start/end vertices trim it — a self-loop (start == end) is the whole
    /// circle (a Path B rim), a partial span is an arc.
    Circle { center: DVec3, radius: f64, normal: DVec3, basis_u: DVec3 },
    /// Circular arc — same `IFCCIRCLE` support, trimmed by the edge vertices.
    /// `ccw` (end_angle ≥ start_angle) sets `IfcEdgeCurve.SameSense`.
    Arc { center: DVec3, radius: f64, normal: DVec3, basis_u: DVec3, ccw: bool },
    /// ADR-203 β-3b — spline edge. Bezier/BSpline/NURBS all land here: a Bezier
    /// is a BSpline with synthesized clamped knots, a NURBS carries `weights`.
    /// Emits `IFCBSPLINECURVEWITHKNOTS` (or the `RATIONAL` form when weighted).
    BSpline {
        control_pts: Vec<DVec3>,
        knots: Vec<f64>,
        degree: u32,
        weights: Option<Vec<f64>>,
    },
}

/// One ordered boundary edge: its two vertices + geometry. `start == end` for a
/// closed self-loop (a full-circle rim). Coordinates are engine units (mm).
#[derive(Clone, Debug)]
pub struct IfcEdge {
    pub start: DVec3,
    pub end: DVec3,
    pub curve: EdgeCurve,
}

/// One face of an advanced B-rep: an analytic surface trimmed by boundary loops
/// of [`IfcEdge`]s. Coordinates are engine units (mm) — the emitter converts via
/// `scale`.
pub struct AdvancedFace {
    /// The face's geometric surface (axia-geo). Plane/Cylinder/Sphere/Cone/Torus;
    /// NURBS-class → export error.
    pub surface: AnalyticSurface,
    /// Ordered outer boundary edges.
    pub outer: Vec<IfcEdge>,
    /// Hole boundary loops.
    pub inners: Vec<Vec<IfcEdge>>,
    /// `true` if the face normal agrees with the surface's parametric normal
    /// (`IfcAdvancedFace.SameSense`).
    pub same_sense: bool,
}

impl AdvancedFace {
    /// Convenience: a planar face from a CCW vertex polygon (all straight edges).
    /// Consecutive + wrap-around duplicate verts are collapsed.
    pub fn planar(surface: AnalyticSurface, verts: Vec<DVec3>, same_sense: bool) -> Self {
        AdvancedFace { surface, outer: line_loop(verts), inners: Vec::new(), same_sense }
    }
}

/// Build a straight-edge loop (all [`EdgeCurve::Line`]) from a vertex polygon,
/// collapsing consecutive + wrap-around duplicates.
fn line_loop(verts: Vec<DVec3>) -> Vec<IfcEdge> {
    let mut vs: Vec<DVec3> = Vec::with_capacity(verts.len());
    for v in verts {
        if vs.last().map_or(true, |&p| (p - v).length() > 1e-9) {
            vs.push(v);
        }
    }
    if vs.len() >= 2 && (vs[0] - *vs.last().unwrap()).length() <= 1e-9 {
        vs.pop();
    }
    let n = vs.len();
    (0..n)
        .map(|i| IfcEdge { start: vs[i], end: vs[(i + 1) % n], curve: EdgeCurve::Line })
        .collect()
}

/// Emit `faces` as a complete IFC4.3 `IFCADVANCEDBREP` file. `scale` converts
/// engine units to the IFC unit (metre): pass `0.001` for mm. `name` labels the
/// single IfcWall. Errors if any face carries a NURBS-class surface (β-3) or a
/// degenerate boundary loop.
pub fn emit_advanced_brep(faces: &[AdvancedFace], scale: f64, name: &str) -> Result<String, String> {
    if faces.is_empty() {
        return Err("emit_advanced_brep: no faces".into());
    }
    let mut w = StepWriter::new();
    w.file_description = format!("AXiA IFC4.3 '{}' (IfcAdvancedBrep, ADR-203)", name);
    w.file_name = format!("{}.ifc", name);

    // ── Owner / units / context (shared scaffold) ──
    let sc = emit_owner_units_context(&mut w);

    // ── Geometry: analytic advanced faces ──
    let brep = emit_advanced_geometry(&mut w, faces, scale)?;

    // ── Product + spatial hierarchy (shared scaffold) ──
    emit_product_and_spatial(&mut w, &sc, name, brep, "AdvancedBrep");

    Ok(w.build())
}

/// Emit the analytic-face geometry (`IFCADVANCEDFACE`* → `IFCCLOSEDSHELL` →
/// `IFCADVANCEDBREP`) for `faces`, returning the brep `#N`. Shared by
/// [`emit_advanced_brep`] (single wall) and `emit_ifc_model` (β-γ, per element).
pub(crate) fn emit_advanced_geometry(
    w: &mut StepWriter,
    faces: &[AdvancedFace],
    scale: f64,
) -> Result<EntityRef, String> {
    let mut face_refs = Vec::with_capacity(faces.len());
    for (i, f) in faces.iter().enumerate() {
        let surface = emit_surface(w, &f.surface, scale)
            .map_err(|e| format!("face[{}]: {}", i, e))?;

        let face_is_curved = !matches!(f.surface, AnalyticSurface::Plane { .. });
        let outer_loop = emit_edge_loop(w, &f.outer, scale, face_is_curved)
            .map_err(|e| format!("face[{}] outer: {}", i, e))?;
        let outer_bound = w.add(
            "IFCFACEOUTERBOUND",
            vec![StepValue::Ref(outer_loop), StepValue::Enum("T".into())],
        );
        let mut bounds = vec![StepValue::Ref(outer_bound)];
        for (j, inner) in f.inners.iter().enumerate() {
            let inner_loop = emit_edge_loop(w, inner, scale, face_is_curved)
                .map_err(|e| format!("face[{}] inner[{}]: {}", i, j, e))?;
            let inner_bound = w.add(
                "IFCFACEBOUND",
                vec![StepValue::Ref(inner_loop), StepValue::Enum("T".into())],
            );
            bounds.push(StepValue::Ref(inner_bound));
        }

        let adv_face = w.add(
            "IFCADVANCEDFACE",
            vec![
                StepValue::List(bounds),
                StepValue::Ref(surface),
                StepValue::Enum(if f.same_sense { "T" } else { "F" }.into()),
            ],
        );
        face_refs.push(adv_face);
    }
    let shell = w.add(
        "IFCCLOSEDSHELL",
        vec![StepValue::List(face_refs.iter().map(|&f| StepValue::Ref(f)).collect())],
    );
    Ok(w.add("IFCADVANCEDBREP", vec![StepValue::Ref(shell)]))
}

/// Extract an [`AdvancedBrep`](emit_advanced_brep) directly from a live DCEL
/// [`Mesh`] (ADR-203 β-2.5). Walks every **active** face, reading its analytic
/// surface + boundary loops (outer + holes) + orientation. `scale` converts
/// engine units to the IFC unit (metre); `name` labels the wall.
///
/// **All-or-nothing**: errors (→ caller falls back to β-1.5 faceted export) if
/// *any* active face lacks a supported analytic surface (Plane/Cylinder/Sphere/
/// Cone/Torus) or has a boundary that cannot be a straight-edge loop — in
/// particular Path B curved faces, whose rims are 1-vertex self-loops, need
/// curved edges (β-3). Planar-face models (boxes, extruded polygons) export as
/// exact `IfcAdvancedFace(IfcPlane)` here.
pub fn emit_advanced_brep_from_mesh(mesh: &Mesh, scale: f64, name: &str) -> Result<String, String> {
    let faces = advanced_faces_from_mesh(mesh)?;
    emit_advanced_brep(&faces, scale, name)
}

/// Build the [`AdvancedFace`] list from a mesh's active faces (engine units).
fn advanced_faces_from_mesh(mesh: &Mesh) -> Result<Vec<AdvancedFace>, String> {
    advanced_faces_filtered(mesh, None)
}

/// Like [`advanced_faces_from_mesh`] but, when `allowed` is `Some`, only the
/// listed faces are included (β-γ: one element's owned faces). Errors if any
/// included face lacks a supported analytic surface / straight-or-circular
/// boundary (→ caller's faceted fallback).
pub(crate) fn advanced_faces_filtered(
    mesh: &Mesh,
    allowed: Option<&std::collections::HashSet<axia_geo::FaceId>>,
) -> Result<Vec<AdvancedFace>, String> {
    let mut out = Vec::new();
    for (fid, face) in mesh.faces.iter() {
        if !face.is_active() {
            continue;
        }
        if let Some(set) = allowed {
            if !set.contains(&fid) {
                continue;
            }
        }
        let outer = loop_edges(mesh, face.outer().start)
            .map_err(|e| format!("face {:?} outer: {}", fid, e))?;
        // A face's stored analytic surface is the truth. When it carries none —
        // e.g. faces created by a Boolean subtract (an imported IfcRelVoidsElement
        // opening bakes a hole into the wall this way) — synthesize an *exact*
        // `Plane` from its outer loop iff that loop is coplanar. A tessellated
        // curved region is not coplanar → `None` → the error below → faceted
        // fallback, so nothing curved is ever silently flattened.
        let surface = match mesh.face_surface(fid) {
            Some(s) => s.clone(),
            None => synthesize_plane_from_loop(&outer).ok_or_else(|| {
                format!("face {:?}: no analytic surface (advanced brep needs every face analytic)", fid)
            })?,
        };
        let mut inners = Vec::with_capacity(face.inners().len());
        for lr in face.inners() {
            inners.push(
                loop_edges(mesh, lr.start).map_err(|e| format!("face {:?} inner: {}", fid, e))?,
            );
        }
        let same_sense = compute_same_sense(&outer, &surface);
        out.push(AdvancedFace { surface, outer, inners, same_sense });
    }
    if out.is_empty() {
        return Err("no active faces".into());
    }
    Ok(out)
}

/// Resolve a DCEL boundary loop (from its start half-edge) to ordered
/// [`IfcEdge`]s — each half-edge's destination vertex + its edge's analytic
/// curve. Errors on a curve β-3 can't map yet (Bezier/BSpline/NURBS).
///
/// A half-edge points to its `dst`, so edge `hes[i]` runs from `dst(hes[i-1])`
/// to `dst(hes[i])`. A self-loop (rim) is one half-edge whose `dst` is the sole
/// anchor → `start == end` (a full-circle edge).
fn loop_edges(mesh: &Mesh, start: HeId) -> Result<Vec<IfcEdge>, String> {
    let hes = mesh.collect_loop_hes(start).map_err(|e| e.to_string())?;
    let n = hes.len();
    if n == 0 {
        return Err("empty loop".into());
    }
    // Destination vertex position of each half-edge.
    let mut d = Vec::with_capacity(n);
    for &h in &hes {
        let he = mesh.hes.get(h).ok_or_else(|| format!("half-edge {:?} missing", h))?;
        d.push(mesh.vertex_pos(he.dst()).map_err(|e| e.to_string())?);
    }
    let mut edges = Vec::with_capacity(n);
    for i in 0..n {
        let he = mesh.hes.get(hes[i]).unwrap();
        let curve = edge_curve_to_ifc(mesh.edge_curve(he.edge()))?;
        edges.push(IfcEdge { start: d[(i + n - 1) % n], end: d[i], curve });
    }
    Ok(edges)
}

/// Map an [`AnalyticCurve`] (or `None` = plain straight edge) to an [`EdgeCurve`].
/// Bezier/BSpline/NURBS become a unified [`EdgeCurve::BSpline`] (β-3b).
fn edge_curve_to_ifc(c: Option<&AnalyticCurve>) -> Result<EdgeCurve, String> {
    match c {
        None | Some(AnalyticCurve::Line { .. }) => Ok(EdgeCurve::Line),
        Some(AnalyticCurve::Circle { center, radius, normal, basis_u }) => Ok(EdgeCurve::Circle {
            center: *center,
            radius: *radius,
            normal: *normal,
            basis_u: *basis_u,
        }),
        Some(AnalyticCurve::Arc { center, radius, normal, basis_u, start_angle, end_angle }) => {
            Ok(EdgeCurve::Arc {
                center: *center,
                radius: *radius,
                normal: *normal,
                basis_u: *basis_u,
                ccw: end_angle >= start_angle,
            })
        }
        // β-3b — a Bezier is a BSpline whose knot vector is clamped [0,1].
        Some(AnalyticCurve::Bezier { control_pts }) => {
            if control_pts.len() < 2 {
                return Err("Bezier edge needs ≥ 2 control points".into());
            }
            let degree = (control_pts.len() - 1) as u32;
            Ok(EdgeCurve::BSpline {
                control_pts: control_pts.clone(),
                knots: clamped_knots(degree),
                degree,
                weights: None,
            })
        }
        Some(AnalyticCurve::BSpline { control_pts, knots, degree }) => Ok(EdgeCurve::BSpline {
            control_pts: control_pts.clone(),
            knots: knots.clone(),
            degree: *degree,
            weights: None,
        }),
        Some(AnalyticCurve::NURBS { control_pts, weights, knots, degree }) => {
            if weights.len() != control_pts.len() {
                return Err("NURBS edge: weights/control_pts length mismatch".into());
            }
            Ok(EdgeCurve::BSpline {
                control_pts: control_pts.clone(),
                knots: knots.clone(),
                degree: *degree,
                weights: Some(weights.clone()),
            })
        }
    }
}

/// Clamped knot vector for a single Bezier span of `degree`: `degree+1` zeros
/// then `degree+1` ones (the IFC/NURBS canonical form).
fn clamped_knots(degree: u32) -> Vec<f64> {
    let d = degree as usize + 1;
    let mut k = vec![0.0; d];
    k.extend(std::iter::repeat(1.0).take(d));
    k
}

/// IFC stores knots as *distinct* values + multiplicities; our engine (like most
/// NURBS kernels) stores the flat vector with repeats. Compress it.
fn compress_knots(knots: &[f64]) -> (Vec<f64>, Vec<i64>) {
    let mut vals: Vec<f64> = Vec::new();
    let mut mults: Vec<i64> = Vec::new();
    for &k in knots {
        match vals.last() {
            Some(&last) if (k - last).abs() < 1e-12 => *mults.last_mut().unwrap() += 1,
            _ => {
                vals.push(k);
                mults.push(1);
            }
        }
    }
    (vals, mults)
}

/// Emit `IFCBSPLINECURVEWITHKNOTS` / `IFCRATIONALBSPLINECURVEWITHKNOTS`.
fn emit_bspline_curve(
    w: &mut StepWriter,
    control_pts: &[DVec3],
    knots: &[f64],
    degree: u32,
    weights: Option<&[f64]>,
    scale: f64,
) -> Result<EntityRef, String> {
    if control_pts.len() < 2 {
        return Err("spline edge needs ≥ 2 control points".into());
    }
    if knots.is_empty() {
        return Err("spline edge has no knots".into());
    }
    let pts: Vec<StepValue> = control_pts
        .iter()
        .map(|&p| StepValue::Ref(pt(w, p * scale)))
        .collect();
    let (kvals, kmults) = compress_knots(knots);
    let mut args = vec![
        StepValue::Int(degree as i64),
        StepValue::List(pts),
        StepValue::Enum("UNSPECIFIED".into()), // CurveForm
        StepValue::Enum("U".into()),           // ClosedCurve (LOGICAL unknown)
        StepValue::Enum("U".into()),           // SelfIntersect
        StepValue::List(kmults.iter().map(|&m| StepValue::Int(m)).collect()),
        StepValue::List(kvals.iter().map(|&v| StepValue::Real(v)).collect()),
        StepValue::Enum("UNSPECIFIED".into()), // KnotSpec
    ];
    Ok(match weights {
        None => w.add("IFCBSPLINECURVEWITHKNOTS", args),
        Some(ws) => {
            args.push(StepValue::List(ws.iter().map(|&x| StepValue::Real(x)).collect()));
            w.add("IFCRATIONALBSPLINECURVEWITHKNOTS", args)
        }
    })
}

/// `IfcAdvancedFace.SameSense`: does the face's outward normal (Newell of the
/// outer loop vertices, CCW-outward per ADR-007) agree with the surface's
/// parametric normal at a boundary point? Defaults `true` on a degenerate probe
/// — e.g. a single-edge circular rim (matching the ADR-033 convention that
/// attached surfaces are oriented face-outward).
fn compute_same_sense(outer: &[IfcEdge], surface: &AnalyticSurface) -> bool {
    let verts: Vec<DVec3> = outer.iter().map(|e| e.start).collect();
    let n_face = newell(&verts);
    if n_face.length_squared() < 1e-18 {
        return true;
    }
    // A boundary vertex lies on the surface, so its normal is well-defined
    // (unlike the loop centroid, which is inside a curved face).
    let n_surf = surface.normal_at_world_pos(verts[0]);
    if n_surf.length_squared() < 1e-18 {
        return true;
    }
    n_face.dot(n_surf) >= 0.0
}

/// A planar face's analytic surface *is* a plane. When a face carries no stored
/// surface (Boolean-subtract results — an imported RelVoids opening bakes its
/// hole into the wall this way), synthesize an exact [`AnalyticSurface::Plane`]
/// from the outer loop — but only if the loop is genuinely coplanar (max vertex
/// deviation ≤ 1 µm, well above axis-aligned Boolean noise, well below a
/// tessellated curved face's per-face deviation). Returns `None` for a
/// degenerate or non-planar loop so the caller errors → faceted fallback (no
/// curve is ever flattened).
fn synthesize_plane_from_loop(outer: &[IfcEdge]) -> Option<AnalyticSurface> {
    let verts: Vec<DVec3> = outer.iter().map(|e| e.start).collect();
    if verts.len() < 3 {
        return None;
    }
    let n = newell(&verts);
    let nlen = n.length();
    if nlen < 1e-9 {
        return None; // degenerate (collinear / zero-area)
    }
    let normal = n / nlen;
    let origin = verts[0];
    let max_dev = verts
        .iter()
        .map(|&v| (v - origin).dot(normal).abs())
        .fold(0.0, f64::max);
    if max_dev > 1e-3 {
        return None; // not planar (engine units mm; 1e-3 mm = 1 µm)
    }
    // The first non-degenerate outer edge lies in the plane → a valid basis_u ⊥ normal.
    let basis_u = outer.iter().find_map(|e| {
        let d = e.end - e.start;
        (d.length() > 1e-9).then(|| d.normalize())
    })?;
    Some(AnalyticSurface::Plane {
        origin,
        normal,
        basis_u,
        u_range: (-1e6, 1e6),
        v_range: (-1e6, 1e6),
    })
}

/// An opening box's orthonormal frame (u, v, n + extents), from the eight corners
/// in [`crate::ModelOpening`] order (bottom 0-3, top 4-7 — see `emit_box`): edge
/// c0→c1 is u, c0→c3 is v, c0→c4 is n (the wall-thickness direction).
struct BoxFrame {
    c0: DVec3,
    u: DVec3,
    v: DVec3,
    n: DVec3,
    lu: f64,
    lv: f64,
    ln: f64,
}

impl BoxFrame {
    fn from_corners(c: &[DVec3; 8]) -> Option<Self> {
        let (eu, ev, en) = (c[1] - c[0], c[3] - c[0], c[4] - c[0]);
        let (lu, lv, ln) = (eu.length(), ev.length(), en.length());
        if lu < 1e-6 || lv < 1e-6 || ln < 1e-6 {
            return None;
        }
        Some(BoxFrame { c0: c[0], u: eu / lu, v: ev / lv, n: en / ln, lu, lv, ln })
    }

    /// Is `p` within this box's extent (10 µm slack — the box, rebuilt from the
    /// recorded opening, is a superset of the hole the subtract cut, so its verts
    /// sit on/inside it; wall verts are hundreds of mm beyond the u/v extent)?
    fn contains(&self, p: DVec3) -> bool {
        let d = p - self.c0;
        let (pu, pv, pn) = (d.dot(self.u), d.dot(self.v), d.dot(self.n));
        const T: f64 = 1e-2;
        pu >= -T && pu <= self.lu + T && pv >= -T && pv <= self.lv + T && pn >= -T && pn <= self.ln + T
    }
}

/// Does every vertex of this (non-empty) loop lie within `fr`? A hole-outline
/// inner loop and a tunnel-wall outer loop do; a wall's outer/side loop does not.
fn loop_on_box(edges: &[IfcEdge], fr: &BoxFrame) -> bool {
    !edges.is_empty() && edges.iter().all(|e| fr.contains(e.start))
}

/// Is this opening box a **through-hole** in `faces` — some face carries an inner
/// loop entirely on it — rather than a notch/edge opening whose void is part of
/// the outer boundary? A notch can't be restored by the local loop fill, so the
/// caller refills it with a boolean union instead (§36-amendment).
pub(crate) fn is_through_hole(faces: &[AdvancedFace], corners: &[DVec3; 8]) -> bool {
    match BoxFrame::from_corners(corners) {
        Some(fr) => faces.iter().any(|f| f.inners.iter().any(|inner| loop_on_box(inner, &fr))),
        None => false,
    }
}

/// Is this opening box a baked **notch** — a face's whole *outer* loop lies on it
/// (a jamb/tunnel wall of the void) but no inner loop does? A through-hole has an
/// inner loop; a solid wall (the opening was recorded but never baked into the
/// geometry) has neither, so it needs no refill. Only a notch needs the boolean
/// union (its void is part of the outer boundary — the local loop fill can't
/// restore it). §36-amendment.
pub(crate) fn is_notch(faces: &[AdvancedFace], corners: &[DVec3; 8]) -> bool {
    if is_through_hole(faces, corners) {
        return false;
    }
    match BoxFrame::from_corners(corners) {
        Some(fr) => faces.iter().any(|f| loop_on_box(&f.outer, &fr)),
        None => false,
    }
}

/// Reconstruct the SOLID wall body from the holed faces by **filling** each
/// through-hole opening. A recorded opening box is the region a RelVoids subtract
/// removed, so it exactly fills the hole it cut: strip the hole-outline inner
/// loops (their verts lie on the box) and drop the tunnel-wall faces (whole outer
/// loop on the box). Emitting the *solid* body + the opening as a separate
/// `IfcOpeningElement` is standard IFC — a consumer (and our own re-import) voids
/// it exactly once, where a holed body + a void would **double-void** it.
///
/// Only a **through-hole** box (one with a matching inner loop) is filled; a
/// notch / edge opening (no matching inner loop — a user-drawn door's U-notch,
/// ADR-262) is left untouched, so this never opens a solid it cannot re-close.
pub(crate) fn fill_through_hole_openings(faces: Vec<AdvancedFace>, boxes: &[[DVec3; 8]]) -> Vec<AdvancedFace> {
    let frames: Vec<BoxFrame> = boxes.iter().filter_map(BoxFrame::from_corners).collect();
    if frames.is_empty() {
        return faces;
    }
    // A box is a through-hole iff some face carries an inner loop entirely on it.
    let through: Vec<bool> = frames
        .iter()
        .map(|fr| faces.iter().any(|f| f.inners.iter().any(|inner| loop_on_box(inner, fr))))
        .collect();
    let on_a_through_box = |edges: &[IfcEdge]| {
        frames.iter().zip(&through).any(|(fr, &t)| t && loop_on_box(edges, fr))
    };
    let mut out = Vec::with_capacity(faces.len());
    for mut f in faces {
        // A tunnel-wall face: its whole outer loop lies on a through-hole box.
        if on_a_through_box(&f.outer) {
            continue;
        }
        // A hole outline: strip inner loops that lie on a through-hole box.
        f.inners.retain(|inner| !on_a_through_box(inner));
        out.push(f);
    }
    out
}

/// Newell's method — area-weighted polygon normal (robust to non-planarity).
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

/// Map an [`AnalyticSurface`] to its IFC surface entity, returning the `#N` ref.
/// Positions/radii are scaled to the IFC unit; axes/angles are not.
fn emit_surface(w: &mut StepWriter, s: &AnalyticSurface, scale: f64) -> Result<EntityRef, String> {
    match s {
        AnalyticSurface::Plane { origin, normal, basis_u, .. } => {
            let pl = placement_axes(w, *origin * scale, *normal, *basis_u);
            Ok(w.add("IFCPLANE", vec![StepValue::Ref(pl)]))
        }
        AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, .. } => {
            let pl = placement_axes(w, *axis_origin * scale, *axis_dir, *ref_dir);
            Ok(w.add(
                "IFCCYLINDRICALSURFACE",
                vec![StepValue::Ref(pl), StepValue::Real(radius * scale)],
            ))
        }
        AnalyticSurface::Sphere { center, radius, axis_dir, ref_dir, .. } => {
            let pl = placement_axes(w, *center * scale, *axis_dir, *ref_dir);
            Ok(w.add(
                "IFCSPHERICALSURFACE",
                vec![StepValue::Ref(pl), StepValue::Real(radius * scale)],
            ))
        }
        AnalyticSurface::Cone { apex, axis_dir, half_angle, ref_dir, v_range, .. } => {
            // IfcConicalSurface parametric: P = C + (R + v·tanθ)·(cos u·x + sin u·y) + v·z,
            // with the apex at v = -R/tanθ. To place the apex at `apex`, put the
            // reference plane a representative distance up the axis (toward the
            // base) so R is a natural, positive radius near the used region.
            let v_ref = {
                let d = v_range.1.abs();
                if d > 1e-9 { d } else { 1.0 }
            } * scale;
            let radius = v_ref * half_angle.tan();
            let location = *apex * scale + *axis_dir * v_ref;
            let pl = placement_axes(w, location, *axis_dir, *ref_dir);
            Ok(w.add(
                "IFCCONICALSURFACE",
                vec![StepValue::Ref(pl), StepValue::Real(radius), StepValue::Real(*half_angle)],
            ))
        }
        AnalyticSurface::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. } => {
            let pl = placement_axes(w, *center * scale, *axis_dir, *ref_dir);
            Ok(w.add(
                "IFCTOROIDALSURFACE",
                vec![
                    StepValue::Ref(pl),
                    StepValue::Real(major_radius * scale),
                    StepValue::Real(minor_radius * scale),
                ],
            ))
        }
        // β-3b — NURBS-class surfaces. A Bezier patch is a B-spline surface with
        // synthesized clamped knots; a NURBS surface carries a weight grid.
        AnalyticSurface::BezierPatch { ctrl_grid } => {
            let (du, dv) = grid_degrees(ctrl_grid)?;
            emit_bspline_surface(
                w, ctrl_grid, &clamped_knots(du), &clamped_knots(dv), du, dv, None, scale,
            )
        }
        AnalyticSurface::BSplineSurface { ctrl_grid, knots_u, knots_v, deg_u, deg_v } => {
            emit_bspline_surface(w, ctrl_grid, knots_u, knots_v, *deg_u, *deg_v, None, scale)
        }
        AnalyticSurface::NURBSSurface {
            ctrl_grid, weights, knots_u, knots_v, deg_u, deg_v, ..
        } => emit_bspline_surface(
            w, ctrl_grid, knots_u, knots_v, *deg_u, *deg_v, Some(weights), scale,
        ),
    }
}

/// Degrees implied by a Bezier control grid (`(deg_u+1) × (deg_v+1)`).
fn grid_degrees(grid: &[Vec<DVec3>]) -> Result<(u32, u32), String> {
    let nu = grid.len();
    let nv = grid.first().map_or(0, |r| r.len());
    if nu < 2 || nv < 2 {
        return Err(format!("Bezier patch grid too small ({}×{})", nu, nv));
    }
    Ok(((nu - 1) as u32, (nv - 1) as u32))
}

/// Emit `IFCBSPLINESURFACEWITHKNOTS` / `IFCRATIONALBSPLINESURFACEWITHKNOTS`.
/// `ctrl_grid` rows are the u direction (IFC `ControlPointsList` is u-major).
#[allow(clippy::too_many_arguments)]
fn emit_bspline_surface(
    w: &mut StepWriter,
    ctrl_grid: &[Vec<DVec3>],
    knots_u: &[f64],
    knots_v: &[f64],
    deg_u: u32,
    deg_v: u32,
    weights: Option<&Vec<Vec<f64>>>,
    scale: f64,
) -> Result<EntityRef, String> {
    let nu = ctrl_grid.len();
    let nv = ctrl_grid.first().map_or(0, |r| r.len());
    if nu < 2 || nv < 2 {
        return Err(format!("spline surface grid too small ({}×{})", nu, nv));
    }
    if ctrl_grid.iter().any(|r| r.len() != nv) {
        return Err("spline surface control grid is ragged".into());
    }
    if knots_u.is_empty() || knots_v.is_empty() {
        return Err("spline surface has no knots".into());
    }
    if let Some(ws) = weights {
        if ws.len() != nu || ws.iter().any(|r| r.len() != nv) {
            return Err("spline surface: weight grid shape ≠ control grid".into());
        }
    }

    // Control points, row (u) major.
    let mut rows = Vec::with_capacity(nu);
    for row in ctrl_grid {
        let pts: Vec<StepValue> = row.iter().map(|&p| StepValue::Ref(pt(w, p * scale))).collect();
        rows.push(StepValue::List(pts));
    }
    let (uvals, umults) = compress_knots(knots_u);
    let (vvals, vmults) = compress_knots(knots_v);

    let mut args = vec![
        StepValue::Int(deg_u as i64),
        StepValue::Int(deg_v as i64),
        StepValue::List(rows),
        StepValue::Enum("UNSPECIFIED".into()), // SurfaceForm
        StepValue::Enum("U".into()),           // UClosed (LOGICAL unknown)
        StepValue::Enum("U".into()),           // VClosed
        StepValue::Enum("U".into()),           // SelfIntersect
        StepValue::List(umults.iter().map(|&m| StepValue::Int(m)).collect()),
        StepValue::List(vmults.iter().map(|&m| StepValue::Int(m)).collect()),
        StepValue::List(uvals.iter().map(|&v| StepValue::Real(v)).collect()),
        StepValue::List(vvals.iter().map(|&v| StepValue::Real(v)).collect()),
        StepValue::Enum("UNSPECIFIED".into()), // KnotSpec
    ];
    Ok(match weights {
        None => w.add("IFCBSPLINESURFACEWITHKNOTS", args),
        Some(ws) => {
            let grid: Vec<StepValue> = ws
                .iter()
                .map(|row| StepValue::List(row.iter().map(|&x| StepValue::Real(x)).collect()))
                .collect();
            args.push(StepValue::List(grid));
            w.add("IFCRATIONALBSPLINESURFACEWITHKNOTS", args)
        }
    })
}

/// Build an `IfcEdgeLoop` from ordered [`IfcEdge`]s (engine units, scaled by
/// `scale`). Straight edges emit `IFCLINE`; circular edges emit `IFCCIRCLE`
/// (the edge vertices trim it — a self-loop is a whole rim). A single closed
/// circular edge is a valid loop; an all-straight loop needs ≥ 3 edges.
fn emit_edge_loop(
    w: &mut StepWriter,
    edges: &[IfcEdge],
    scale: f64,
    face_is_curved: bool,
) -> Result<EntityRef, String> {
    if edges.is_empty() {
        return Err("empty loop".into());
    }
    let all_line = edges.iter().all(|e| matches!(e.curve, EdgeCurve::Line));
    // A SEAM loop — two edges walking one edge out and back — bounds a CURVED
    // face. Its two straight edges enclose no area in any plane, which is
    // exactly why the check below rejects two straight edges, but here the face
    // between them is the surface: a sphere written by its axis is two poles and
    // a meridian seam (IFC's own `IfcSeamCurve` + `IfcVertexLoop` shape).
    //
    // Measured 2026-08-07: without this the export of an axis-defined sphere
    // failed here, fell through to the faceted fallback, and that refused the
    // same loop for the same reason — so `curved.ifc` came out **0 bytes** and
    // the external web-ifc check could not even open it.
    // Gated on the face being CURVED. Two straight edges walking out and back
    // enclose no area, so on a PLANE that is a degenerate sliver and must stay
    // rejected — `degenerate_loop_rejected` is the guard that says so, and it
    // caught this exemption when it was ungated (a collapsed planar loop reduces
    // to the same two edges once duplicate vertices are dropped).
    let is_seam = face_is_curved
        && edges.len() == 2
        && (edges[0].start - edges[1].end).length() < 1e-9
        && (edges[0].end - edges[1].start).length() < 1e-9;
    if all_line && edges.len() < 3 && !is_seam {
        return Err(format!("degenerate straight loop ({} edges)", edges.len()));
    }
    let n = edges.len();

    // Vertex points, shared between adjacent edges: vpt[i] at edges[i].start
    // (== edges[i-1].end). A self-loop (n == 1, start == end) has one vertex.
    let vpts: Vec<EntityRef> = edges
        .iter()
        .map(|e| {
            let p = pt(w, e.start * scale);
            w.add("IFCVERTEXPOINT", vec![StepValue::Ref(p)])
        })
        .collect();

    let mut oedges = Vec::with_capacity(n);
    for (i, e) in edges.iter().enumerate() {
        let (geom, same_sense) = match &e.curve {
            EdgeCurve::Line => {
                let d = e.end - e.start;
                let len = d.length();
                if len < 1e-9 {
                    return Err("degenerate line edge (start == end)".into());
                }
                let pnt = pt(w, e.start * scale);
                let direction = dir(w, d / len);
                let vector = w.add(
                    "IFCVECTOR",
                    vec![StepValue::Ref(direction), StepValue::Real(len * scale)],
                );
                let line = w.add("IFCLINE", vec![StepValue::Ref(pnt), StepValue::Ref(vector)]);
                (line, true)
            }
            EdgeCurve::Circle { center, radius, normal, basis_u } => {
                let pl = placement_axes(w, *center * scale, *normal, *basis_u);
                let circ = w.add("IFCCIRCLE", vec![StepValue::Ref(pl), StepValue::Real(radius * scale)]);
                (circ, true)
            }
            EdgeCurve::Arc { center, radius, normal, basis_u, ccw } => {
                let pl = placement_axes(w, *center * scale, *normal, *basis_u);
                let circ = w.add("IFCCIRCLE", vec![StepValue::Ref(pl), StepValue::Real(radius * scale)]);
                (circ, *ccw)
            }
            EdgeCurve::BSpline { control_pts, knots, degree, weights } => {
                let c = emit_bspline_curve(w, control_pts, knots, *degree, weights.as_deref(), scale)?;
                (c, true)
            }
        };
        let edge = w.add(
            "IFCEDGECURVE",
            vec![
                StepValue::Ref(vpts[i]),
                StepValue::Ref(vpts[(i + 1) % n]),
                StepValue::Ref(geom),
                StepValue::Enum(if same_sense { "T" } else { "F" }.into()),
            ],
        );
        // IfcOrientedEdge.EdgeStart/EdgeEnd are DERIVED (`*`).
        let oedge = w.add(
            "IFCORIENTEDEDGE",
            vec![
                StepValue::Derived,
                StepValue::Derived,
                StepValue::Ref(edge),
                StepValue::Enum("T".into()),
            ],
        );
        oedges.push(oedge);
    }
    Ok(w.add(
        "IFCEDGELOOP",
        vec![StepValue::List(oedges.iter().map(|&e| StepValue::Ref(e)).collect())],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `#N` referenced in arg position must be a defined id in range.
    fn assert_refs_resolve(s: &str) {
        let mut max_def = 0u32;
        for line in s.lines() {
            if let Some(eq) = line.find('=') {
                if line.starts_with('#') {
                    if let Ok(id) = line[1..eq].parse::<u32>() {
                        max_def = max_def.max(id);
                    }
                }
            }
        }
        for line in s.lines() {
            if !line.starts_with('#') {
                continue;
            }
            let args = &line[line.find('(').map(|i| i + 1).unwrap_or(line.len())..];
            let mut chars = args.char_indices().peekable();
            while let Some((i, c)) = chars.next() {
                if c == '#' {
                    let rest = &args[i + 1..];
                    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(id) = num.parse::<u32>() {
                        assert!(id >= 1 && id <= max_def, "ref #{} out of range (max #{})", id, max_def);
                    }
                }
            }
        }
    }

    fn plane(origin: DVec3, normal: DVec3, basis_u: DVec3) -> AnalyticSurface {
        AnalyticSurface::Plane {
            origin,
            normal,
            basis_u,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        }
    }

    /// A unit box `[0,1000]³` mm as 6 planar advanced faces (scale 0.001 → metre).
    fn box_faces() -> Vec<AdvancedFace> {
        let s = 1000.0;
        let c = |x: f64, y: f64, z: f64| DVec3::new(x * s, y * s, z * s);
        vec![
            // bottom (-Z), normal -Z
            AdvancedFace::planar(
                plane(c(0.0, 0.0, 0.0), -DVec3::Z, DVec3::X),
                vec![c(0.0, 0.0, 0.0), c(0.0, 1.0, 0.0), c(1.0, 1.0, 0.0), c(1.0, 0.0, 0.0)],
                true,
            ),
            // top (+Z)
            AdvancedFace::planar(
                plane(c(0.0, 0.0, 1.0), DVec3::Z, DVec3::X),
                vec![c(0.0, 0.0, 1.0), c(1.0, 0.0, 1.0), c(1.0, 1.0, 1.0), c(0.0, 1.0, 1.0)],
                true,
            ),
            // front (-Y)
            AdvancedFace::planar(
                plane(c(0.0, 0.0, 0.0), -DVec3::Y, DVec3::X),
                vec![c(0.0, 0.0, 0.0), c(1.0, 0.0, 0.0), c(1.0, 0.0, 1.0), c(0.0, 0.0, 1.0)],
                true,
            ),
            // back (+Y)
            AdvancedFace::planar(
                plane(c(0.0, 1.0, 0.0), DVec3::Y, DVec3::X),
                vec![c(1.0, 1.0, 0.0), c(0.0, 1.0, 0.0), c(0.0, 1.0, 1.0), c(1.0, 1.0, 1.0)],
                true,
            ),
            // left (-X)
            AdvancedFace::planar(
                plane(c(0.0, 0.0, 0.0), -DVec3::X, DVec3::Y),
                vec![c(0.0, 0.0, 0.0), c(0.0, 0.0, 1.0), c(0.0, 1.0, 1.0), c(0.0, 1.0, 0.0)],
                true,
            ),
            // right (+X)
            AdvancedFace::planar(
                plane(c(1.0, 0.0, 0.0), DVec3::X, DVec3::Y),
                vec![c(1.0, 0.0, 0.0), c(1.0, 1.0, 0.0), c(1.0, 1.0, 1.0), c(1.0, 0.0, 1.0)],
                true,
            ),
        ]
    }

    #[test]
    fn advanced_box_six_planar_faces() {
        let s = emit_advanced_brep(&box_faces(), 0.001, "adv-box").unwrap();
        // true IFC4X3 advanced brep
        assert!(s.contains("FILE_SCHEMA(('IFC4X3'));"));
        assert!(s.contains("=IFCADVANCEDBREP("));
        assert!(s.contains("IFCWALL"));
        assert!(s.contains("'AdvancedBrep'"), "RepresentationType AdvancedBrep");
        // 6 planar advanced faces
        assert_eq!(s.matches("=IFCADVANCEDFACE(").count(), 6);
        assert_eq!(s.matches("=IFCPLANE(").count(), 6);
        assert_eq!(s.matches("=IFCEDGELOOP(").count(), 6);
        // 6 faces × 4 edges = 24 edges/lines/vectors/vertexpoints/orientededges
        assert_eq!(s.matches("=IFCEDGECURVE(").count(), 24);
        assert_eq!(s.matches("=IFCLINE(").count(), 24);
        assert_eq!(s.matches("=IFCVERTEXPOINT(").count(), 24);
        assert_eq!(s.matches("=IFCORIENTEDEDGE(").count(), 24);
        assert_eq!(s.matches("=IFCCLOSEDSHELL(").count(), 1);
        // NOT faceted
        assert!(!s.contains("=IFCFACETEDBREP("));
        assert_refs_resolve(&s);
    }

    #[test]
    fn advanced_brep_byte_identical() {
        let a = emit_advanced_brep(&box_faces(), 0.001, "b").unwrap();
        let b = emit_advanced_brep(&box_faces(), 0.001, "b").unwrap();
        assert_eq!(a, b, "deterministic (L-203-2)");
    }

    #[test]
    fn scale_converts_mm_to_metre() {
        // A 1000mm×1000mm planar quad → coords 0. and 1. in metres.
        let s = emit_advanced_brep(&box_faces(), 0.001, "b").unwrap();
        assert!(s.contains("IFCCARTESIANPOINT((1.,1.,0.))"), "1000mm → 1.m");
        assert!(s.contains("IFCCARTESIANPOINT((0.,0.,0.))"));
    }

    /// A synthetic single face carrying a given surface (quad boundary — the
    /// edges only approximate a curved surface, but the surface entity is exact;
    /// β-3 supplies curved edges).
    fn one_face(surface: AnalyticSurface) -> Vec<AdvancedFace> {
        let s = 1000.0;
        let c = |x: f64, y: f64, z: f64| DVec3::new(x * s, y * s, z * s);
        vec![AdvancedFace::planar(
            surface,
            vec![c(0.0, 0.0, 0.0), c(1.0, 0.0, 0.0), c(1.0, 1.0, 0.0), c(0.0, 1.0, 0.0)],
            true,
        )]
    }

    #[test]
    fn surface_cylinder_maps_to_cylindrical() {
        let surf = AnalyticSurface::Cylinder {
            axis_origin: DVec3::new(0.0, 0.0, 0.0),
            axis_dir: DVec3::Z,
            radius: 500.0, // mm
            ref_dir: DVec3::X,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (0.0, 1000.0),
        };
        let s = emit_advanced_brep(&one_face(surf), 0.001, "cyl").unwrap();
        assert!(s.contains("=IFCCYLINDRICALSURFACE("));
        assert!(s.contains("IFCCYLINDRICALSURFACE(#") && s.contains(",0.5)"), "radius 500mm → 0.5m: {}", s);
        assert_refs_resolve(&s);
    }

    #[test]
    fn surface_sphere_maps_to_spherical() {
        let surf = AnalyticSurface::Sphere {
            center: DVec3::new(0.0, 0.0, 0.0),
            radius: 250.0,
            axis_dir: DVec3::Z,
            ref_dir: DVec3::X,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
        };
        let s = emit_advanced_brep(&one_face(surf), 0.001, "sph").unwrap();
        assert!(s.contains("=IFCSPHERICALSURFACE("));
        assert!(s.contains(",0.25)"), "radius 250mm → 0.25m: {}", s);
        assert_refs_resolve(&s);
    }

    #[test]
    fn surface_cone_maps_to_conical() {
        let surf = AnalyticSurface::Cone {
            apex: DVec3::new(0.0, 0.0, 0.0),
            axis_dir: DVec3::Z,
            half_angle: std::f64::consts::FRAC_PI_4, // 45° → tan = 1
            ref_dir: DVec3::X,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (0.0, 1000.0), // apex→base 1000mm
        };
        let s = emit_advanced_brep(&one_face(surf), 0.001, "cone").unwrap();
        assert!(s.contains("=IFCCONICALSURFACE("));
        // semiangle (last arg) = π/4 exactly; radius = v_ref·tan(45°) ≈ 1m
        // (tan(π/4) is 0.9999999999999999 in f64, so don't assert it exactly).
        assert!(s.contains(",0.7853981633974483);"), "semiangle π/4: {}", s);
        assert_refs_resolve(&s);
    }

    #[test]
    fn surface_torus_maps_to_toroidal() {
        let surf = AnalyticSurface::Torus {
            center: DVec3::new(0.0, 0.0, 0.0),
            axis_dir: DVec3::Z,
            ref_dir: DVec3::X,
            major_radius: 1000.0,
            minor_radius: 200.0,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (0.0, std::f64::consts::TAU),
        };
        let s = emit_advanced_brep(&one_face(surf), 0.001, "tor").unwrap();
        assert!(s.contains("=IFCTOROIDALSURFACE("));
        assert!(s.contains(",1.,0.2)"), "major 1m, minor 0.2m: {}", s);
        assert_refs_resolve(&s);
    }

    #[test]
    fn nurbs_surface_maps_to_rational_bspline_surface() {
        // β-3b — a weighted control grid → IFCRATIONALBSPLINESURFACEWITHKNOTS.
        let grid = vec![
            vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 1000.0, 0.0)],
            vec![DVec3::new(1000.0, 0.0, 0.0), DVec3::new(1000.0, 1000.0, 500.0)],
        ];
        let surf = AnalyticSurface::NURBSSurface {
            ctrl_grid: grid,
            weights: vec![vec![1.0, 0.5], vec![0.5, 1.0]],
            knots_u: vec![0.0, 0.0, 1.0, 1.0],
            knots_v: vec![0.0, 0.0, 1.0, 1.0],
            deg_u: 1,
            deg_v: 1,
            trim_loops: vec![],
        };
        let s = emit_advanced_brep(&one_face(surf), 0.001, "n").unwrap();
        assert_eq!(s.matches("=IFCRATIONALBSPLINESURFACEWITHKNOTS(").count(), 1);
        assert!(s.contains("IFCRATIONALBSPLINESURFACEWITHKNOTS(1,1,"), "u/v degree first: {}", s);
        assert!(s.contains("((1.,0.5),(0.5,1.))"), "weight grid: {}", s);
        assert_refs_resolve(&s);
    }

    #[test]
    fn bspline_and_bezier_surfaces_map_to_bspline_surface() {
        let grid = vec![
            vec![DVec3::new(0.0, 0.0, 0.0), DVec3::new(0.0, 1000.0, 0.0)],
            vec![DVec3::new(1000.0, 0.0, 0.0), DVec3::new(1000.0, 1000.0, 0.0)],
        ];
        // non-rational B-spline surface
        let s = emit_advanced_brep(
            &one_face(AnalyticSurface::BSplineSurface {
                ctrl_grid: grid.clone(),
                knots_u: vec![0.0, 0.0, 1.0, 1.0],
                knots_v: vec![0.0, 0.0, 1.0, 1.0],
                deg_u: 1,
                deg_v: 1,
            }),
            0.001,
            "bs",
        )
        .unwrap();
        assert_eq!(s.matches("=IFCBSPLINESURFACEWITHKNOTS(").count(), 1);
        assert_eq!(s.matches("=IFCRATIONALBSPLINESURFACEWITHKNOTS(").count(), 0);
        assert_refs_resolve(&s);

        // Bezier patch → degrees from the grid, clamped knots synthesized
        let b = emit_advanced_brep(
            &one_face(AnalyticSurface::BezierPatch { ctrl_grid: grid }),
            0.001,
            "bez",
        )
        .unwrap();
        assert_eq!(b.matches("=IFCBSPLINESURFACEWITHKNOTS(").count(), 1);
        assert!(b.contains("IFCBSPLINESURFACEWITHKNOTS(1,1,"), "1×1 degrees: {}", b);
        assert_refs_resolve(&b);
    }

    #[test]
    fn ragged_control_grid_rejected() {
        let surf = AnalyticSurface::BSplineSurface {
            ctrl_grid: vec![vec![DVec3::ZERO; 2], vec![DVec3::ZERO; 3]], // ragged
            knots_u: vec![0.0, 0.0, 1.0, 1.0],
            knots_v: vec![0.0, 0.0, 1.0, 1.0],
            deg_u: 1,
            deg_v: 1,
        };
        assert!(emit_advanced_brep(&one_face(surf), 0.001, "r").is_err());
    }

    #[test]
    fn degenerate_loop_rejected() {
        // A face whose outer loop collapses to < 3 distinct verts.
        let surf = plane(DVec3::ZERO, DVec3::Z, DVec3::X);
        let faces = vec![AdvancedFace::planar(
            surf,
            vec![DVec3::ZERO, DVec3::ZERO, DVec3::new(1.0, 0.0, 0.0)],
            true,
        )];
        assert!(emit_advanced_brep(&faces, 0.001, "d").is_err());
    }

    /// An axis-defined sphere — poles and one meridian seam — must export. Its
    /// boundary is two straight edges walking one edge out and back, which
    /// `degenerate_loop_rejected` above is right to refuse on a PLANE and wrong
    /// to refuse here: the face between them is the sphere.
    ///
    /// Measured 2026-08-07 before the exemption: the advanced-brep emit failed
    /// here, fell through to the faceted fallback, and that refused the same loop
    /// for the same reason — so `curved.ifc` came out **0 bytes** and the
    /// external web-ifc check could not open it. `npm run validate:ifc` is the
    /// end-to-end form of this test.
    #[test]
    fn an_axis_defined_sphere_exports() {
        let mut mesh = axia_geo::Mesh::new();
        mesh.set_sphere_path_b_default(true);
        let f = mesh
            .create_sphere(DVec3::ZERO, 1000.0, 16, 12, axia_geo::entities::MaterialId::new(0))
            .unwrap();
        assert_eq!(f.len(), 1, "the axis definition is one face");
        let faces = crate::ifc_advancedbrep::advanced_faces_filtered(&mesh, None)
            .expect("a seam-bounded sphere must survive face collection");
        let s = emit_advanced_brep(&faces, 0.001, "sphere")
            .expect("and must emit — 0 bytes here is the regression this pins");
        assert!(s.contains("IFCSPHERICALSURFACE"), "it carries its surface");
        assert!(s.len() > 1000, "a real file, not a stub: {} bytes", s.len());
    }

    #[test]
    fn empty_faces_rejected() {
        assert!(emit_advanced_brep(&[], 0.001, "e").is_err());
    }

    // ── §34: synthesize an exact IfcPlane for a surfaceless planar face ──
    // (a Boolean-cut wall — an imported RelVoids opening bakes its hole this way —
    // has faces carrying no stored surface; a planar face's surface *is* a plane).

    #[test]
    fn surfaceless_planar_loop_synthesizes_an_exact_plane() {
        // A CCW quad in the z = 200 plane (engine units mm).
        let loop_edges = line_loop(vec![
            DVec3::new(0.0, 0.0, 200.0),
            DVec3::new(1000.0, 0.0, 200.0),
            DVec3::new(1000.0, 1000.0, 200.0),
            DVec3::new(0.0, 1000.0, 200.0),
        ]);
        let s = synthesize_plane_from_loop(&loop_edges).expect("coplanar loop → a plane");
        match s {
            AnalyticSurface::Plane { origin, normal, basis_u, .. } => {
                assert!((normal - DVec3::Z).length() < 1e-9, "normal is +Z: {normal:?}");
                assert!((origin.z - 200.0).abs() < 1e-9, "origin on the plane");
                assert!(basis_u.dot(normal).abs() < 1e-9, "basis_u ⊥ normal");
            }
            other => panic!("expected Plane, got {other:?}"),
        }
    }

    #[test]
    fn non_planar_loop_is_not_flattened() {
        // One vertex lifted well off the plane (2 mm) — a curved region must NOT
        // be silently flattened to a plane (→ None → caller errors → faceted).
        let loop_edges = line_loop(vec![
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1000.0, 0.0, 0.0),
            DVec3::new(1000.0, 1000.0, 2.0),
            DVec3::new(0.0, 1000.0, 0.0),
        ]);
        assert!(synthesize_plane_from_loop(&loop_edges).is_none(), "non-planar → None");
    }

    // ── §35: fill a through-hole opening back to a solid wall body ──

    /// An opening box (emit_box corner order) spanning X∈[-500,500] (u),
    /// Z∈[-600,600] (v), Y∈[-100,100] (n, the wall thickness).
    fn opening_box() -> [DVec3; 8] {
        let c = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
        [
            c(-500.0, -100.0, -600.0), // 0
            c(500.0, -100.0, -600.0),  // 1  (c1-c0 = +X = u)
            c(500.0, -100.0, 600.0),   // 2
            c(-500.0, -100.0, 600.0),  // 3  (c3-c0 = +Z = v)
            c(-500.0, 100.0, -600.0),  // 4  (c4-c0 = +Y = n)
            c(500.0, 100.0, -600.0),   // 5
            c(500.0, 100.0, 600.0),    // 6
            c(-500.0, 100.0, 600.0),   // 7
        ]
    }

    #[test]
    fn fill_strips_hole_outline_and_drops_tunnel_faces() {
        let s = plane(DVec3::ZERO, DVec3::Y, DVec3::X);
        // Front cap (Y=-100): a big wall rect with the hole as an inner loop.
        let wall = |y: f64| {
            vec![DVec3::new(-2000.0, y, -2000.0), DVec3::new(2000.0, y, -2000.0),
                 DVec3::new(2000.0, y, 2000.0), DVec3::new(-2000.0, y, 2000.0)]
        };
        let hole = |y: f64| {
            vec![DVec3::new(-500.0, y, -600.0), DVec3::new(500.0, y, -600.0),
                 DVec3::new(500.0, y, 600.0), DVec3::new(-500.0, y, 600.0)]
        };
        let holed_cap = AdvancedFace {
            surface: s.clone(),
            outer: line_loop(wall(-100.0)),
            inners: vec![line_loop(hole(-100.0))],
            same_sense: true,
        };
        // A tunnel wall at X=-500 (box u=0), spanning Y and Z of the box.
        let tunnel = AdvancedFace::planar(
            plane(DVec3::new(-500.0, 0.0, 0.0), -DVec3::X, DVec3::Y),
            vec![DVec3::new(-500.0, -100.0, -600.0), DVec3::new(-500.0, 100.0, -600.0),
                 DVec3::new(-500.0, 100.0, 600.0), DVec3::new(-500.0, -100.0, 600.0)],
            true,
        );
        // A genuine wall side face (X=2000) — well outside the box.
        let side = AdvancedFace::planar(
            plane(DVec3::new(2000.0, 0.0, 0.0), DVec3::X, DVec3::Y),
            vec![DVec3::new(2000.0, -100.0, -2000.0), DVec3::new(2000.0, 100.0, -2000.0),
                 DVec3::new(2000.0, 100.0, 2000.0), DVec3::new(2000.0, -100.0, 2000.0)],
            true,
        );
        let out = fill_through_hole_openings(vec![holed_cap, tunnel, side], &[opening_box()]);
        // tunnel dropped → cap + side remain
        assert_eq!(out.len(), 2, "tunnel-wall face dropped");
        let cap = out.iter().find(|f| f.outer.len() == 4 && f.outer[0].start.x.abs() > 1000.0
            && (f.outer[0].start.y + 100.0).abs() < 1e-6).expect("cap kept");
        assert!(cap.inners.is_empty(), "hole outline stripped → solid cap");
        assert!(out.iter().any(|f| (f.outer[0].start.x - 2000.0).abs() < 1e-6), "wall side kept");
    }

    #[test]
    fn fill_leaves_a_notch_untouched() {
        // No face carries an inner loop on the box → not a through-hole → the box
        // is a notch/edge opening and NOTHING is filled (a solid we couldn't
        // re-close must not be opened). A tunnel-looking face stays.
        let notch_wall = AdvancedFace::planar(
            plane(DVec3::new(-500.0, 0.0, 0.0), -DVec3::X, DVec3::Y),
            vec![DVec3::new(-500.0, -100.0, -600.0), DVec3::new(-500.0, 100.0, -600.0),
                 DVec3::new(-500.0, 100.0, 600.0), DVec3::new(-500.0, -100.0, 600.0)],
            true,
        );
        let out = fill_through_hole_openings(vec![notch_wall], &[opening_box()]);
        assert_eq!(out.len(), 1, "no inner loop → not a through-hole → untouched");
    }

    #[test]
    fn a_holed_box_face_without_a_surface_still_exports() {
        // A single planar face carrying NO stored analytic surface exports via the
        // synthesized plane (previously: hard error → faceted fallback).
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0))
            .unwrap();
        // Strip every face's surface → all six now rely on synthesis.
        let fids: Vec<_> = mesh.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
        for fid in fids {
            mesh.set_face_surface(fid, None);
        }
        let s = emit_advanced_brep_from_mesh(&mesh, 0.001, "holed")
            .expect("surfaceless planar faces synthesize planes");
        assert_eq!(s.matches("=IFCADVANCEDFACE(").count(), 6);
        assert_eq!(s.matches("=IFCPLANE(").count(), 6, "six synthesized planes");
        assert!(!s.contains("=IFCFACETEDBREP("), "advanced, not faceted fallback");
        assert_refs_resolve(&s);
    }

    // ── β-2.5: extract advanced brep directly from a live DCEL mesh ──

    #[test]
    fn box_mesh_exports_six_planar_advanced_faces() {
        // Mesh::create_box attaches a Plane surface to all 6 faces (ADR-087 K-δ).
        let mut mesh = Mesh::new();
        mesh.create_box(
            DVec3::new(0.0, 0.0, 0.0),
            2000.0, // width  (X) mm
            3000.0, // height (Z) mm
            4000.0, // depth  (Y) mm
            axia_geo::MaterialId::new(0),
        )
        .unwrap();

        let s = emit_advanced_brep_from_mesh(&mesh, 0.001, "box").unwrap();
        assert!(s.contains("FILE_SCHEMA(('IFC4X3'));"));
        assert!(s.contains("=IFCADVANCEDBREP("));
        assert!(s.contains("'AdvancedBrep'"));
        // Every box face is planar → 6 IfcAdvancedFace(IfcPlane), 4-edge loops.
        assert_eq!(s.matches("=IFCADVANCEDFACE(").count(), 6);
        assert_eq!(s.matches("=IFCPLANE(").count(), 6);
        assert_eq!(s.matches("=IFCEDGELOOP(").count(), 6);
        assert_eq!(s.matches("=IFCEDGECURVE(").count(), 24);
        assert_eq!(s.matches("=IFCCLOSEDSHELL(").count(), 1);
        assert!(!s.contains("=IFCFACETEDBREP("), "advanced, not faceted");
        assert_refs_resolve(&s);
        // width 2000(X)→±1m, depth 4000(Y)→±2m, height 3000(Z)→±1.5m: the
        // +++ corner is (1,2,1.5) in metres.
        assert!(s.contains("IFCCARTESIANPOINT((1.,2.,1.5))"), "corner in metres");
    }

    #[test]
    fn box_mesh_faces_are_same_sense_outward() {
        // create_box winds CCW-outward and attaches face-outward Plane surfaces,
        // so every advanced face should be SameSense .T. (no .F.).
        let mut mesh = Mesh::new();
        mesh.create_box(DVec3::ZERO, 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0))
            .unwrap();
        let s = emit_advanced_brep_from_mesh(&mesh, 0.001, "b").unwrap();
        // 6 advanced faces, all SameSense .T.
        assert_eq!(s.matches("=IFCADVANCEDFACE(").count(), 6);
        for line in s.lines().filter(|l| l.contains("=IFCADVANCEDFACE(")) {
            assert!(line.ends_with(",.T.);"), "outward face is SameSense .T.: {}", line);
        }
    }

    #[test]
    fn empty_mesh_errors_for_faceted_fallback() {
        let mesh = Mesh::new();
        // No active faces → Err → caller (wasm) falls back to faceted export.
        assert!(emit_advanced_brep_from_mesh(&mesh, 0.001, "e").is_err());
    }

    #[test]
    fn box_mesh_export_byte_identical() {
        let build = || {
            let mut mesh = Mesh::new();
            mesh.create_box(DVec3::ZERO, 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0))
                .unwrap();
            emit_advanced_brep_from_mesh(&mesh, 0.001, "b").unwrap()
        };
        assert_eq!(build(), build(), "deterministic (L-203-2)");
    }

    // ── β-3: curved edge curves (IfcCircle) ──

    #[test]
    fn emitter_circle_self_loop_disk() {
        // A planar disk: Plane surface bounded by ONE closed circular edge
        // (a self-loop, start == end). Emits IFCPLANE + a single-edge IfcEdgeLoop
        // whose geometry is IFCCIRCLE (not IFCLINE).
        let anchor = DVec3::new(500.0, 0.0, 0.0); // on the rim, radius 500mm
        let face = AdvancedFace {
            surface: plane(DVec3::ZERO, DVec3::Z, DVec3::X),
            outer: vec![IfcEdge {
                start: anchor,
                end: anchor, // closed self-loop = whole circle
                curve: EdgeCurve::Circle {
                    center: DVec3::ZERO,
                    radius: 500.0,
                    normal: DVec3::Z,
                    basis_u: DVec3::X,
                },
            }],
            inners: vec![],
            same_sense: true,
        };
        let s = emit_advanced_brep(&[face], 0.001, "disk").unwrap();
        assert!(s.contains("=IFCADVANCEDBREP("));
        assert_eq!(s.matches("=IFCADVANCEDFACE(").count(), 1);
        assert_eq!(s.matches("=IFCPLANE(").count(), 1);
        assert_eq!(s.matches("=IFCCIRCLE(").count(), 1);
        assert!(s.contains(",0.5)"), "radius 500mm → 0.5m: {}", s);
        // single closed edge: 1 edge curve, 1 vertex point, 0 lines
        assert_eq!(s.matches("=IFCEDGECURVE(").count(), 1);
        assert_eq!(s.matches("=IFCVERTEXPOINT(").count(), 1);
        assert_eq!(s.matches("=IFCLINE(").count(), 0);
        assert_refs_resolve(&s);
    }

    #[test]
    fn path_b_cylinder_exports_analytic_circles() {
        // A Path B cylinder attaches Circle curves to its rims + Cylinder/Plane
        // surfaces — β-3 exports it as an exact IfcAdvancedBrep (no faceted
        // fallback): cylindrical side + planar caps, IfcCircle rim edges.
        let mut mesh = Mesh::new();
        mesh.create_cylinder_kernel_native_clean(
            DVec3::ZERO,
            500.0,  // radius mm
            1000.0, // height mm
            axia_geo::MaterialId::new(0),
        )
        .unwrap();

        let s = emit_advanced_brep_from_mesh(&mesh, 0.001, "cyl").unwrap();
        assert!(s.contains("=IFCADVANCEDBREP("));
        assert!(!s.contains("=IFCFACETEDBREP("), "analytic, not faceted");
        // 3 faces: base (Plane) + top (Plane) + side (Cylinder)
        assert_eq!(s.matches("=IFCADVANCEDFACE(").count(), 3);
        assert_eq!(s.matches("=IFCCYLINDRICALSURFACE(").count(), 1);
        assert_eq!(s.matches("=IFCPLANE(").count(), 2);
        // rims are analytic circles, not straight lines
        assert!(s.matches("=IFCCIRCLE(").count() >= 3, "circular rims: {}", s);
        assert!(s.contains(",0.5)"), "radius 500mm → 0.5m circle");
        assert_refs_resolve(&s);
    }

    #[test]
    fn spline_edge_curves_map_to_bspline() {
        use axia_geo::AnalyticCurve;
        assert!(matches!(edge_curve_to_ifc(None).unwrap(), EdgeCurve::Line));

        // β-3b — a Bezier becomes a BSpline with synthesized clamped knots.
        match edge_curve_to_ifc(Some(&AnalyticCurve::Bezier {
            control_pts: vec![DVec3::ZERO, DVec3::X, DVec3::Y],
        }))
        .unwrap()
        {
            EdgeCurve::BSpline { degree, knots, weights, control_pts } => {
                assert_eq!(degree, 2, "3 control points → degree 2");
                assert_eq!(knots, vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0]);
                assert_eq!(control_pts.len(), 3);
                assert!(weights.is_none(), "Bezier is non-rational");
            }
            other => panic!("expected BSpline, got {:?}", other),
        }

        // A B-spline passes its own knots/degree through, still non-rational.
        match edge_curve_to_ifc(Some(&AnalyticCurve::BSpline {
            control_pts: vec![DVec3::ZERO, DVec3::X],
            knots: vec![0.0, 0.0, 1.0, 1.0],
            degree: 1,
        }))
        .unwrap()
        {
            EdgeCurve::BSpline { degree, knots, weights, .. } => {
                assert_eq!(degree, 1);
                assert_eq!(knots, vec![0.0, 0.0, 1.0, 1.0]);
                assert!(weights.is_none());
            }
            other => panic!("expected BSpline, got {:?}", other),
        }

        // A NURBS carries its weights (→ rational IFC entity).
        match edge_curve_to_ifc(Some(&AnalyticCurve::NURBS {
            control_pts: vec![DVec3::ZERO, DVec3::X],
            weights: vec![1.0, 0.5],
            knots: vec![0.0, 0.0, 1.0, 1.0],
            degree: 1,
        }))
        .unwrap()
        {
            EdgeCurve::BSpline { weights, .. } => assert_eq!(weights, Some(vec![1.0, 0.5])),
            other => panic!("expected BSpline, got {:?}", other),
        }

        // Length mismatch is rejected rather than silently emitted.
        assert!(edge_curve_to_ifc(Some(&AnalyticCurve::NURBS {
            control_pts: vec![DVec3::ZERO, DVec3::X],
            weights: vec![1.0],
            knots: vec![0.0, 0.0, 1.0, 1.0],
            degree: 1,
        }))
        .is_err());
    }

    #[test]
    fn knot_vector_compresses_to_values_and_multiplicities() {
        // IFC stores distinct knots + multiplicities; we store the flat vector.
        let (v, m) = compress_knots(&[0.0, 0.0, 0.0, 0.5, 1.0, 1.0, 1.0]);
        assert_eq!(v, vec![0.0, 0.5, 1.0]);
        assert_eq!(m, vec![3, 1, 3]);
        assert_eq!(clamped_knots(3), vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0]);
    }

    #[test]
    fn spline_edge_emits_bspline_curve_entity() {
        // A face bounded by one closed spline edge → IFCBSPLINECURVEWITHKNOTS.
        let a = DVec3::new(1000.0, 0.0, 0.0);
        let face = AdvancedFace {
            surface: plane(DVec3::ZERO, DVec3::Z, DVec3::X),
            outer: vec![IfcEdge {
                start: a,
                end: a,
                curve: EdgeCurve::BSpline {
                    control_pts: vec![a, DVec3::new(0.0, 1000.0, 0.0), DVec3::new(-1000.0, 0.0, 0.0), a],
                    knots: vec![0.0, 0.0, 0.0, 0.0, 1.0, 1.0, 1.0, 1.0],
                    degree: 3,
                    weights: None,
                },
            }],
            inners: vec![],
            same_sense: true,
        };
        let s = emit_advanced_brep(&[face], 0.001, "spline").unwrap();
        assert_eq!(s.matches("=IFCBSPLINECURVEWITHKNOTS(").count(), 1);
        assert_eq!(s.matches("=IFCRATIONALBSPLINECURVEWITHKNOTS(").count(), 0);
        // degree 3, knots compressed to (0,1) with multiplicity 4 each
        assert!(s.contains("IFCBSPLINECURVEWITHKNOTS(3,"), "degree first: {}", s);
        assert!(s.contains("(4,4),(0.,1.)"), "compressed knots: {}", s);
        assert_refs_resolve(&s);
    }

    #[test]
    fn rational_spline_edge_emits_weights() {
        let a = DVec3::new(1000.0, 0.0, 0.0);
        let face = AdvancedFace {
            surface: plane(DVec3::ZERO, DVec3::Z, DVec3::X),
            outer: vec![IfcEdge {
                start: a,
                end: a,
                curve: EdgeCurve::BSpline {
                    control_pts: vec![a, DVec3::new(0.0, 1000.0, 0.0), a],
                    knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
                    degree: 2,
                    weights: Some(vec![1.0, 0.7071, 1.0]),
                },
            }],
            inners: vec![],
            same_sense: true,
        };
        let s = emit_advanced_brep(&[face], 0.001, "rational").unwrap();
        assert_eq!(s.matches("=IFCRATIONALBSPLINECURVEWITHKNOTS(").count(), 1);
        assert!(s.contains("0.7071"), "weights emitted: {}", s);
        assert_refs_resolve(&s);
    }

    #[test]
    fn arc_edge_maps_to_circle_with_sense() {
        use axia_geo::AnalyticCurve;
        // Arc → IFCCIRCLE support; ccw (end ≥ start) sets SameSense.
        let ccw = edge_curve_to_ifc(Some(&AnalyticCurve::Arc {
            center: DVec3::ZERO,
            radius: 1.0,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            start_angle: 0.0,
            end_angle: 1.5,
        }))
        .unwrap();
        match ccw {
            EdgeCurve::Arc { ccw, radius, .. } => {
                assert!(ccw);
                assert_eq!(radius, 1.0);
            }
            _ => panic!("expected Arc"),
        }
    }
}

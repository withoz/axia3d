//! IfcAdvancedBrep emitter (ADR-203 β-2) — analytic surfaces + edge loops.
//!
//! Where [`crate::ifc_facetedbrep`] emits a triangle soup (`IfcFacetedBrep`),
//! this emits a true B-rep whose faces carry their **analytic surface**
//! (`IfcPlane` / `IfcCylindricalSurface` / `IfcSphericalSurface` /
//! `IfcConicalSurface` / `IfcToroidalSurface`, mapped 1:1 from axia-geo's
//! [`AnalyticSurface`]) and whose boundaries are `IfcEdgeLoop`s of
//! `IfcOrientedEdge` → `IfcEdgeCurve`.
//!
//! **β-2 scope**: edge geometry is straight `IfcLine` only (β-2 roadmap).
//! - **Planar faces are geometrically exact** — a box exports as 6 clean
//!   `IfcAdvancedFace(IfcPlane)` instead of β-1.5's 12 triangles.
//! - Curved-surface faces get an exact analytic **surface** but line-approximate
//!   trim **edges**; proper curved edges (`IfcCircle`/`IfcBSplineCurve`) are β-3.
//!
//! **Units**: face fields (surface + loop verts) are in engine units (mm); the
//! emitter multiplies every position/radius by `scale` (0.001 for mm→metre).
//! Directions (axes) and angles are unit/radian and are *not* scaled.
//!
//! NURBS-class surfaces (BezierPatch/BSplineSurface/NURBSSurface) are rejected
//! with an error in β-2 — they need `IfcBSplineSurfaceWithKnots` (β-3+).

use crate::ifc_common::{emit_owner_units_context, emit_product_and_spatial, dir, placement_axes, pt};
use crate::step_value::{EntityRef, StepValue};
use crate::step_writer::StepWriter;
use axia_geo::{AnalyticCurve, AnalyticSurface, HeId, Mesh};
use glam::DVec3;

/// Geometry of one boundary edge (ADR-203 β-3). β-3 maps `Line` + `Circle`/`Arc`;
/// Bezier/BSpline/NURBS edges are β-3b (`IfcBSplineCurveWithKnots`).
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
    let mut face_refs = Vec::with_capacity(faces.len());
    for (i, f) in faces.iter().enumerate() {
        let surface = emit_surface(&mut w, &f.surface, scale)
            .map_err(|e| format!("face[{}]: {}", i, e))?;

        let outer_loop = emit_edge_loop(&mut w, &f.outer, scale)
            .map_err(|e| format!("face[{}] outer: {}", i, e))?;
        let outer_bound = w.add(
            "IFCFACEOUTERBOUND",
            vec![StepValue::Ref(outer_loop), StepValue::Enum("T".into())],
        );
        let mut bounds = vec![StepValue::Ref(outer_bound)];
        for (j, inner) in f.inners.iter().enumerate() {
            let inner_loop = emit_edge_loop(&mut w, inner, scale)
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
    let brep = w.add("IFCADVANCEDBREP", vec![StepValue::Ref(shell)]);

    // ── Product + spatial hierarchy (shared scaffold) ──
    emit_product_and_spatial(&mut w, &sc, name, brep, "AdvancedBrep");

    Ok(w.build())
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
    let mut out = Vec::new();
    for (fid, face) in mesh.faces.iter() {
        if !face.is_active() {
            continue;
        }
        let surface = mesh
            .face_surface(fid)
            .ok_or_else(|| format!("face {:?}: no analytic surface (advanced brep needs every face analytic)", fid))?
            .clone();
        let outer = loop_edges(mesh, face.outer().start)
            .map_err(|e| format!("face {:?} outer: {}", fid, e))?;
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
/// Bezier/BSpline/NURBS are β-3b → error (→ caller's faceted fallback).
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
        Some(AnalyticCurve::Bezier { .. })
        | Some(AnalyticCurve::BSpline { .. })
        | Some(AnalyticCurve::NURBS { .. }) => {
            Err("Bezier/BSpline/NURBS edge needs IfcBSplineCurveWithKnots (β-3b)".into())
        }
    }
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
        AnalyticSurface::BezierPatch { .. }
        | AnalyticSurface::BSplineSurface { .. }
        | AnalyticSurface::NURBSSurface { .. } => Err(
            "NURBS-class surface needs IfcBSplineSurfaceWithKnots (β-3)".into(),
        ),
    }
}

/// Build an `IfcEdgeLoop` from ordered [`IfcEdge`]s (engine units, scaled by
/// `scale`). Straight edges emit `IFCLINE`; circular edges emit `IFCCIRCLE`
/// (the edge vertices trim it — a self-loop is a whole rim). A single closed
/// circular edge is a valid loop; an all-straight loop needs ≥ 3 edges.
fn emit_edge_loop(w: &mut StepWriter, edges: &[IfcEdge], scale: f64) -> Result<EntityRef, String> {
    if edges.is_empty() {
        return Err("empty loop".into());
    }
    let all_line = edges.iter().all(|e| matches!(e.curve, EdgeCurve::Line));
    if all_line && edges.len() < 3 {
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
    fn nurbs_surface_rejected() {
        let surf = AnalyticSurface::NURBSSurface {
            ctrl_grid: vec![vec![DVec3::ZERO; 2]; 2],
            weights: vec![vec![1.0; 2]; 2],
            knots_u: vec![0.0, 0.0, 1.0, 1.0],
            knots_v: vec![0.0, 0.0, 1.0, 1.0],
            deg_u: 1,
            deg_v: 1,
            trim_loops: vec![],
        };
        let err = emit_advanced_brep(&one_face(surf), 0.001, "n").unwrap_err();
        assert!(err.contains("β-3") || err.contains("NURBS"), "err: {}", err);
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

    #[test]
    fn empty_faces_rejected() {
        assert!(emit_advanced_brep(&[], 0.001, "e").is_err());
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
    fn bezier_bspline_nurbs_edge_curves_deferred_to_beta3b() {
        use axia_geo::AnalyticCurve;
        // β-3 maps Line/Circle/Arc; NURBS-class edges error → faceted fallback.
        assert!(edge_curve_to_ifc(None).is_ok());
        assert!(matches!(edge_curve_to_ifc(None).unwrap(), EdgeCurve::Line));
        assert!(edge_curve_to_ifc(Some(&AnalyticCurve::Bezier {
            control_pts: vec![DVec3::ZERO, DVec3::X, DVec3::Y],
        }))
        .is_err());
        assert!(edge_curve_to_ifc(Some(&AnalyticCurve::BSpline {
            control_pts: vec![DVec3::ZERO, DVec3::X],
            knots: vec![0.0, 0.0, 1.0, 1.0],
            degree: 1,
        }))
        .is_err());
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

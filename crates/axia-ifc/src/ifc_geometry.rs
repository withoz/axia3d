//! IFC B-rep → face loops (ADR-203 I-3) — where geometry actually arrives.
//!
//! I-2 found the members and the geometry each one points at. This turns those
//! `IfcAdvancedBrep` / `IfcFacetedBrep` items into plain polygon loops in engine
//! units, ready for `Mesh::add_face_with_holes`. Walking the two shells:
//!
//! ```text
//! IfcFacetedBrep  → IfcClosedShell → IfcFace        → IfcFaceOuterBound/Bound
//!                                                   → IfcPolyLoop  → IfcCartesianPoint
//! IfcAdvancedBrep → IfcClosedShell → IfcAdvancedFace→ IfcFaceOuterBound/Bound
//!                                                   → IfcEdgeLoop  → IfcOrientedEdge
//!                                                   → IfcEdgeCurve → IfcVertexPoint
//! ```
//!
//! **Curved edges are read by their endpoints.** An `IfcEdgeCurve` whose
//! geometry is an `IfcCircle` becomes a straight chord here — the loop is a
//! polygon. A polygonised cylinder (24 segments, 26 faces) therefore round-trips
//! whole, but a kernel-native rim (ADR-089 Path B: one self-loop edge) collapses
//! to a single point, and that face is dropped rather than invented. Every drop
//! is named in [`GeometryImport::warnings`], so a thinner import is visible
//! instead of silent. Rebuilding analytic curves on import is a later step.
//!
//! Faces arrive with their plane attached ([`FaceLoops::plane`]) because a
//! surface-less face is refused by every kernel-aware op (ADR-087 K-ε).

use crate::ifc_placement::Placement;
use axia_foreign::step_parser::{self, Entity, StepFile, Value};
use axia_geo::AnalyticSurface;
use glam::DVec3;

/// One face's boundary loops, in engine units (mm).
#[derive(Clone, Debug, PartialEq)]
pub struct FaceLoops {
    pub outer: Vec<DVec3>,
    pub inners: Vec<Vec<DVec3>>,
}

impl FaceLoops {
    /// Move every loop point through a placement (I-4).
    pub fn transform(&mut self, p: &crate::ifc_placement::Placement) {
        for v in &mut self.outer {
            *v = p.apply(*v);
        }
        for ring in &mut self.inners {
            for v in ring {
                *v = p.apply(*v);
            }
        }
    }

    /// The plane this face lies in, as an [`AnalyticSurface`].
    ///
    /// An imported face has to carry a surface like any other face in the
    /// engine (ADR-087 K-ε, LOCKED #34). Without one it still renders, but
    /// every kernel-aware op refuses it — Push/Pull, Offset, Boolean, and
    /// re-export as `IfcAdvancedBrep` all require `face_surface`.
    ///
    /// The normal comes from Newell's method, which stays correct for
    /// non-convex loops and for loops whose first three points are collinear.
    /// Returns `None` for a degenerate loop (no area, or no usable first edge)
    /// so the caller leaves the face surface-less rather than attaching a
    /// meaningless plane.
    pub fn plane(&self) -> Option<AnalyticSurface> {
        let p = &self.outer;
        if p.len() < 3 {
            return None;
        }

        let mut n = DVec3::ZERO;
        for i in 0..p.len() {
            let a = p[i];
            let b = p[(i + 1) % p.len()];
            n.x += (a.y - b.y) * (a.z + b.z);
            n.y += (a.z - b.z) * (a.x + b.x);
            n.z += (a.x - b.x) * (a.y + b.y);
        }
        if n.length() < 1e-12 {
            return None;
        }
        let normal = n.normalize();

        // basis_u: first edge long enough to normalize, projected into the
        // plane so it is exactly perpendicular to the normal.
        let origin = p[0];
        let mut basis_u = DVec3::ZERO;
        for q in &p[1..] {
            let d = *q - origin;
            let t = d - normal * d.dot(normal);
            if t.length() > 1e-9 {
                basis_u = t.normalize();
                break;
            }
        }
        if basis_u == DVec3::ZERO {
            return None;
        }

        Some(AnalyticSurface::Plane {
            origin,
            normal,
            basis_u,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        })
    }
}

/// Geometry extracted for one element.
#[derive(Clone, Debug, PartialEq)]
pub struct ElementGeometry {
    /// `#N` of the product entity (matches `ImportedElement::id`).
    pub element_id: u32,
    pub name: Option<String>,
    pub material: Option<String>,
    /// `#N` of the spatial container holding this member, if the file says
    /// (`IfcRelContainedInSpatialStructure`, I-5).
    pub container: Option<u32>,
    pub faces: Vec<FaceLoops>,
}

/// Result of reading a whole file's geometry.
#[derive(Clone, Debug, Default)]
pub struct GeometryImport {
    pub elements: Vec<ElementGeometry>,
    /// Site / building / storey structure, and which container holds what (I-5).
    pub spatial: crate::ifc_spatial::SpatialTree,
    /// Length unit → mm factor actually used.
    pub scale_to_mm: f64,
    /// How many members were moved by a non-identity placement chain (I-4).
    /// Zero for our own files, which bake world coordinates.
    pub placed: usize,
    /// Things we could not read, in file order. Never silent.
    pub warnings: Vec<String>,
}

impl GeometryImport {
    pub fn face_count(&self) -> usize {
        self.elements.iter().map(|e| e.faces.len()).sum()
    }
}

/// Read `IfcSIUnit(*, .LENGTHUNIT., prefix, name)` and return the factor that
/// converts file coordinates to millimetres. Defaults to metre (×1000) — the
/// IFC default — with a warning when no length unit is declared.
pub fn length_scale_to_mm(file: &StepFile, warnings: &mut Vec<String>) -> f64 {
    for (_, ent) in file.iter_entities() {
        if !ent.tag.eq_ignore_ascii_case("IFCSIUNIT") {
            continue;
        }
        let is_length = ent
            .args
            .get(1)
            .and_then(|v| v.as_enum())
            .map(|e| e.eq_ignore_ascii_case("LENGTHUNIT"))
            .unwrap_or(false);
        if !is_length {
            continue;
        }
        let name = ent.args.get(3).and_then(|v| v.as_enum()).unwrap_or("METRE").to_ascii_uppercase();
        if name != "METRE" {
            warnings.push(format!("unsupported length unit {} — assuming metre", name));
        }
        let prefix = ent.args.get(2).and_then(|v| v.as_enum()).map(|s| s.to_ascii_uppercase());
        let factor = match prefix.as_deref() {
            None => 1.0,
            Some("MILLI") => 1e-3,
            Some("CENTI") => 1e-2,
            Some("DECI") => 1e-1,
            Some("DECA") => 1e1,
            Some("HECTO") => 1e2,
            Some("KILO") => 1e3,
            Some("MICRO") => 1e-6,
            Some(other) => {
                warnings.push(format!("unknown SI prefix {} — assuming none", other));
                1.0
            }
        };
        return factor * 1000.0; // metres → mm
    }
    warnings.push("no IfcSIUnit LENGTHUNIT — assuming metre".into());
    1000.0
}

/// Read every element's geometry from an `.ifc`.
pub fn import_ifc_geometry(src: &str) -> Result<GeometryImport, String> {
    let file = step_parser::parse(src).map_err(|e| format!("{:?}", e))?;
    Ok(from_file(&file))
}

/// Read geometry from an already-parsed file, reusing I-2's classification so
/// element identity (name, material) stays in one place.
pub fn from_file(file: &StepFile) -> GeometryImport {
    let mut warnings = Vec::new();
    let scale = length_scale_to_mm(file, &mut warnings);

    // A non-identity WorldCoordinateSystem shifts the whole model. It is
    // almost always the identity; when it is not, say so rather than importing
    // everything quietly offset.
    if let Some(wcs) = crate::ifc_placement::world_coordinate_system(file, scale) {
        warnings.push(format!(
            "file sets a non-identity WorldCoordinateSystem (origin {:.1},{:.1},{:.1} mm) — not applied",
            wcs.origin.x, wcs.origin.y, wcs.origin.z
        ));
    }

    let spatial = crate::ifc_spatial::spatial_tree(file);
    let report = crate::ifc_elements::classify(file);

    let mut elements = Vec::new();
    let mut placed = 0usize;
    for el in &report.elements {
        let label = || match &el.name {
            Some(n) if !n.is_empty() => format!("#{} '{}'", el.id, n),
            _ => format!("#{} {}", el.id, el.ifc_type),
        };
        let mut faces = Vec::new();
        let mut supported_geometry = 0usize;
        let mut dropped_faces = 0usize;
        // I-4 — a member's B-rep is written in its own coordinate system and
        // located by a placement chain. Our own files use the identity (we bake
        // world coordinates), so this is free for them and correct for Revit /
        // ArchiCAD, where skipping it piles every member on the origin.
        let placement = el
            .object_placement
            .map(|pid| crate::ifc_placement::resolve_placement(file, pid, scale))
            .unwrap_or_default();
        let mut moved = false;

        for g in &el.geometry {
            if !g.supported {
                continue; // I-2 already reported it
            }
            supported_geometry += 1;
            match brep_face_loops_counted(file, g.id, scale) {
                Ok((mut fs, dropped)) => {
                    if !placement.is_identity() {
                        for f in &mut fs {
                            f.transform(&placement);
                        }
                        moved = true;
                    }
                    faces.append(&mut fs);
                    dropped_faces += dropped;
                }
                Err(e) => warnings.push(format!("{}: {}", label(), e)),
            }
        }
        if dropped_faces > 0 {
            // Curved rims read by their endpoints collapse to <3 points. Say so
            // rather than handing back a quietly thinner solid.
            warnings.push(format!(
                "{}: {} face(s) skipped — their boundary is a curve we cannot yet rebuild",
                label(),
                dropped_faces
            ));
        }
        if faces.is_empty() {
            if supported_geometry > 0 {
                warnings.push(format!("{}: no usable faces", label()));
            }
            continue;
        }
        if moved {
            placed += 1;
        }
        elements.push(ElementGeometry {
            element_id: el.id,
            name: el.name.clone(),
            material: el.material.clone(),
            container: spatial.container_of.get(&el.id).copied(),
            faces,
        });
    }
    GeometryImport { elements, spatial, scale_to_mm: scale, placed, warnings }
}

/// Face loops of one `IfcFacetedBrep` / `IfcAdvancedBrep`.
pub fn brep_face_loops(file: &StepFile, brep_id: u32, scale: f64) -> Result<Vec<FaceLoops>, String> {
    brep_face_loops_counted(file, brep_id, scale).map(|(loops, _)| loops)
}

/// As [`brep_face_loops`], plus how many faces were dropped as degenerate — the
/// caller turns that into a warning so a silently-thinner import is visible.
pub(crate) fn brep_face_loops_counted(
    file: &StepFile,
    brep_id: u32,
    scale: f64,
) -> Result<(Vec<FaceLoops>, usize), String> {
    let brep = file.entity(brep_id).ok_or_else(|| format!("brep #{} missing", brep_id))?;
    let tag = brep.tag.to_ascii_uppercase();
    if tag != "IFCFACETEDBREP" && tag != "IFCADVANCEDBREP" {
        return Err(format!("#{} is {}, not a brep", brep_id, tag));
    }
    // Both take Outer: IfcClosedShell as attribute 0.
    let shell_id = brep.args.first().and_then(|v| v.as_ref())
        .ok_or_else(|| format!("brep #{} has no shell", brep_id))?;
    let shell = file.entity(shell_id).ok_or_else(|| format!("shell #{} missing", shell_id))?;
    let faces = shell.args.first().and_then(|v| v.as_list())
        .ok_or_else(|| format!("shell #{} has no face list", shell_id))?;

    let mut out = Vec::new();
    let mut dropped = 0usize;
    for f in faces {
        let Some(face_id) = f.as_ref() else { continue };
        let Some(face) = file.entity(face_id) else {
            dropped += 1;
            continue;
        };
        match face_bounds(file, face, scale) {
            Some(loops) => out.push(loops),
            None => dropped += 1,
        }
    }
    Ok((out, dropped))
}

/// `IfcFace` / `IfcAdvancedFace` → outer + inner loops.
fn face_bounds(file: &StepFile, face: &Entity, scale: f64) -> Option<FaceLoops> {
    // IfcFace.Bounds / IfcAdvancedFace.Bounds are attribute 0.
    let bounds = face.args.first()?.as_list()?;
    let mut outer: Option<Vec<DVec3>> = None;
    let mut inners: Vec<Vec<DVec3>> = Vec::new();

    for b in bounds {
        let Some(bound_id) = b.as_ref() else { continue };
        let Some(bound) = file.entity(bound_id) else { continue };
        let is_outer = bound.tag.eq_ignore_ascii_case("IFCFACEOUTERBOUND");
        // IfcFaceBound.Bound = attribute 0, Orientation = 1.
        let Some(loop_id) = bound.args.first().and_then(|v| v.as_ref()) else { continue };
        let Some(pts) = loop_points(file, loop_id, scale) else { continue };
        if pts.len() < 3 {
            continue; // degenerate — e.g. a circular rim read by its endpoints
        }
        if is_outer && outer.is_none() {
            outer = Some(pts);
        } else {
            inners.push(pts);
        }
    }
    // A face with no outer bound but one inner is still a face; promote it.
    let outer = match (outer, inners.is_empty()) {
        (Some(o), _) => o,
        (None, false) => inners.remove(0),
        (None, true) => return None,
    };
    Some(FaceLoops { outer, inners })
}

/// `IfcPolyLoop` or `IfcEdgeLoop` → ordered points (engine units).
fn loop_points(file: &StepFile, loop_id: u32, scale: f64) -> Option<Vec<DVec3>> {
    let lp = file.entity(loop_id)?;
    if lp.tag.eq_ignore_ascii_case("IFCPOLYLOOP") {
        // Polygon: attribute 0 is the point list.
        let pts = lp.args.first()?.as_list()?;
        return Some(
            pts.iter()
                .filter_map(|p| p.as_ref().and_then(|id| cartesian_point(file, id, scale)))
                .collect(),
        );
    }
    if lp.tag.eq_ignore_ascii_case("IFCEDGELOOP") {
        // EdgeList: attribute 0 → IfcOrientedEdge → IfcEdgeCurve → vertices.
        let edges = lp.args.first()?.as_list()?;
        let mut pts: Vec<DVec3> = Vec::new();
        for e in edges {
            let Some(oe) = e.as_ref().and_then(|id| file.entity(id)) else { continue };
            // IfcOrientedEdge(EdgeStart*, EdgeEnd*, EdgeElement, Orientation)
            let (edge_ent, orientation) = if oe.tag.eq_ignore_ascii_case("IFCORIENTEDEDGE") {
                let inner = oe.args.get(2).and_then(|v| v.as_ref()).and_then(|id| file.entity(id));
                let ori = oe.args.get(3).and_then(|v| v.as_enum()).map(|s| s != "F").unwrap_or(true);
                (inner, ori)
            } else {
                (Some(oe), true)
            };
            let Some(edge) = edge_ent else { continue };
            // IfcEdge/IfcEdgeCurve(EdgeStart, EdgeEnd, …)
            let a = edge.args.first().and_then(|v| v.as_ref()).and_then(|id| vertex_point(file, id, scale));
            let b = edge.args.get(1).and_then(|v| v.as_ref()).and_then(|id| vertex_point(file, id, scale));
            let (start, end) = if orientation { (a, b) } else { (b, a) };
            if let Some(p) = start {
                // Skip a repeat of the previous point (closed rims repeat their anchor).
                if pts.last().map_or(true, |q: &DVec3| (*q - p).length() > 1e-9) {
                    pts.push(p);
                }
            }
            // A curved edge is not the straight line between its endpoints. Walk
            // it — a circle, or a spline (Bezier / B-spline / NURBS / ellipse,
            // all of which our exporter and most tools write as an
            // IfcBSplineCurveWithKnots) — or it silently becomes a chord, a face
            // that looks fine and is the wrong shape. A spline self-loop read by
            // its one vertex collapses the face entirely.
            if let (Some(p0), Some(p1)) = (start, end) {
                if let Some(mid) = arc_interior_points(file, edge, p0, p1, orientation, scale)
                    .or_else(|| spline_interior_points(file, edge, p0, p1, scale))
                {
                    pts.extend(mid);
                }
            }
        }
        // Drop a wrap-around duplicate.
        if pts.len() >= 2 && (pts[0] - *pts.last().unwrap()).length() <= 1e-9 {
            pts.pop();
        }
        return Some(pts);
    }
    None
}

/// Chord tolerance for walking an imported arc, in mm. Matches the render-side
/// value (LOCKED #40) so an imported curve is as smooth as a drawn one.
const ARC_CHORD_TOL_MM: f64 = 0.02;

/// The points *between* a curved edge's endpoints, or `None` when the edge is
/// straight.
///
/// An `IfcEdgeCurve` whose geometry is an `IfcCircle` (usually wrapped in an
/// `IfcTrimmedCurve`) is an arc. Reading only its endpoints turns it into a
/// chord: the face still imports, still looks plausible, and is the wrong
/// shape — worse than being dropped, because nothing warns.
///
/// The endpoints alone cannot say *which* arc joins them — two points a
/// diameter apart are joined by two different half-circles. Only the trimmed
/// curve knows: `Trim1`, `Trim2`, and `SenseAgreement` fix the exact sweep, so
/// this reads them rather than guessing a direction from the edge flags. The
/// resulting arc is then oriented to the loop's own start→end traversal.
fn arc_interior_points(
    file: &StepFile,
    edge: &Entity,
    start: DVec3,
    end: DVec3,
    _orientation: bool,
    scale: f64,
) -> Option<Vec<DVec3>> {
    // IfcEdgeCurve(EdgeStart, EdgeEnd, EdgeGeometry, SameSense)
    let geom_id = edge.args.get(2).and_then(|v| v.as_ref())?;
    let geom = file.entity(geom_id)?;

    // Unwrap IfcTrimmedCurve → basis circle, keeping the two trims and the
    // sense that together pin down which arc is meant.
    let (circle, trims): (&Entity, Option<(&Value, &Value, bool)>) =
        if geom.tag.eq_ignore_ascii_case("IFCTRIMMEDCURVE") {
            // (BasisCurve, Trim1, Trim2, SenseAgreement, MasterRepresentation)
            let basis = geom.args.first().and_then(|v| v.as_ref())?;
            let t1 = geom.args.get(1)?;
            let t2 = geom.args.get(2)?;
            let sense = geom.args.get(3).and_then(|v| v.as_enum()).map(|s| s != "F").unwrap_or(true);
            (file.entity(basis)?, Some((t1, t2, sense)))
        } else {
            (geom, None)
        };
    if !circle.tag.eq_ignore_ascii_case("IFCCIRCLE") {
        return None; // straight, or a curve we do not walk yet
    }

    // IfcCircle(Position: IfcAxis2Placement3D, Radius)
    let pos = circle.args.first().and_then(|v| v.as_ref())?;
    let place = crate::ifc_placement::axis_placement(file, pos, scale)?;
    let radius = circle.args.get(1).and_then(|v| v.as_f64())? * scale;
    if !(radius > 0.0) {
        return None;
    }

    let angle_of = |p: DVec3| -> f64 {
        let d = p - place.origin;
        d.dot(place.y).atan2(d.dot(place.x))
    };

    // Start/end angles come from the trims when present — that is what makes a
    // half-circle unambiguous. A bare (untrimmed) circle falls back to the loop
    // vertices and CCW, the only reasonable default.
    let (a0, sweep_ccw) = if let Some((t1, _t2, sense)) = trims {
        (trim_angle(file, t1, &place, scale).unwrap_or_else(|| angle_of(start)), sense)
    } else {
        (angle_of(start), true)
    };
    let a1 = if let Some((_t1, t2, _)) = trims {
        trim_angle(file, t2, &place, scale).unwrap_or_else(|| angle_of(end))
    } else {
        angle_of(end)
    };

    // A self-loop edge — start and end are the same vertex — is a *closed*
    // circle: the whole rim carried in one edge, the way ADR-089 Path B and
    // many BIM tools write a round disk or a circular hole. Read by its single
    // vertex it collapses the face; it has to sweep the full turn. This is made
    // explicit rather than left to fall out of the `<= 1e-9` roll-over below, so
    // a future zero-length guard cannot silently un-close every circle.
    const TAU: f64 = std::f64::consts::TAU;
    let closed = (start - end).length_squared() < 1e-12;

    // Otherwise sweep a0→a1 in the sense the trim declares.
    let mut sweep = if closed {
        if sweep_ccw {
            TAU
        } else {
            -TAU
        }
    } else {
        let mut s = a1 - a0;
        if sweep_ccw {
            while s <= 1e-9 {
                s += TAU;
            }
        } else {
            while s >= -1e-9 {
                s -= TAU;
            }
        }
        s
    };

    // Segment count from the chord tolerance: cos(θ/2) = 1 - tol/r.
    let ratio = (1.0 - ARC_CHORD_TOL_MM / radius).clamp(-1.0, 1.0);
    let step = 2.0 * ratio.acos();
    let segments = if step > 1e-9 {
        ((sweep.abs() / step).ceil() as usize).clamp(2, 512)
    } else {
        16
    };

    let point_at =
        |a: f64| place.origin + place.x * (radius * a.cos()) + place.y * (radius * a.sin());

    // The arc runs Trim1→Trim2, but the loop is walked start→end. Trim1 sits on
    // one of them; if it sits on `end`, reverse so the interior comes out in
    // traversal order.
    let forward = (point_at(a0) - start).length_squared() <= (point_at(a0) - end).length_squared();

    // Interior only — the endpoints are already the loop's vertices.
    let mut out = Vec::with_capacity(segments.saturating_sub(1));
    for i in 1..segments {
        let a = a0 + sweep * (i as f64) / (segments as f64);
        out.push(point_at(a));
    }
    if !forward {
        out.reverse();
    }
    Some(out)
}

/// The angle on the circle of one `IfcTrimmedCurve` trim (`Trim1` / `Trim2`).
///
/// A trim is a *set* — it may carry an `IfcCartesianPoint`, an
/// `IfcParameterValue`, or both. The cartesian point is geometrically exact, so
/// it wins; the parameter (an angle in radians for a circle) is the fallback.
fn trim_angle(file: &StepFile, trim: &Value, place: &Placement, scale: f64) -> Option<f64> {
    let items = trim.as_list()?;
    for it in items {
        if let Some(p) = it.as_ref().and_then(|id| cartesian_point(file, id, scale)) {
            let d = p - place.origin;
            return Some(d.dot(place.y).atan2(d.dot(place.x)));
        }
    }
    // IfcParameterValue — the angle itself, for a circle.
    items.iter().find_map(|it| it.as_f64())
}

/// The points *between* a spline edge's endpoints, or `None` when the geometry
/// is not a B-spline.
///
/// Bezier, B-spline, NURBS and even an ellipse all reach IFC as an
/// `IfcBSplineCurveWithKnots` (or the `RATIONAL` form when weighted) — that is
/// what our own exporter writes and what most tools do. Read by its endpoints
/// the curve is a chord; a *closed* spline (a self-loop edge, EdgeStart ==
/// EdgeEnd) collapses to a single point and the whole face is dropped, which is
/// the gap this closes.
///
/// The engine's own tessellator is reused (`bspline` / `nurbs`), so an imported
/// spline is sampled exactly as a drawn one, at the same chord tolerance.
fn spline_interior_points(
    file: &StepFile,
    edge: &Entity,
    start: DVec3,
    end: DVec3,
    scale: f64,
) -> Option<Vec<DVec3>> {
    use axia_geo::curves::{bspline, nurbs};

    let geom_id = edge.args.get(2).and_then(|v| v.as_ref())?;
    let mut curve = file.entity(geom_id)?;
    // A spline may be wrapped in an IfcTrimmedCurve; we walk the whole basis.
    if curve.tag.eq_ignore_ascii_case("IFCTRIMMEDCURVE") {
        let basis = curve.args.first().and_then(|v| v.as_ref())?;
        curve = file.entity(basis)?;
    }
    let rational = curve.tag.eq_ignore_ascii_case("IFCRATIONALBSPLINECURVEWITHKNOTS");
    if !rational && !curve.tag.eq_ignore_ascii_case("IFCBSPLINECURVEWITHKNOTS") {
        return None;
    }

    // IfcBSplineCurveWithKnots(Degree, ControlPointsList, CurveForm, ClosedCurve,
    //   SelfIntersect, KnotMultiplicities, Knots, KnotSpec [, WeightsData])
    let degree = curve.args.first().and_then(|v| v.as_f64())? as usize;
    let control_pts: Vec<DVec3> = curve
        .args
        .get(1)?
        .as_list()?
        .iter()
        .filter_map(|v| v.as_ref().and_then(|id| cartesian_point(file, id, scale)))
        .collect();
    let mults: Vec<usize> = curve
        .args
        .get(5)?
        .as_list()?
        .iter()
        .filter_map(|v| v.as_f64().map(|m| m as usize))
        .collect();
    let distinct: Vec<f64> = curve.args.get(6)?.as_list()?.iter().filter_map(|v| v.as_f64()).collect();
    if control_pts.len() < 2 || mults.len() != distinct.len() {
        return None;
    }

    // Expand distinct knots + multiplicities back into the flat vector the
    // tessellator wants — the inverse of the exporter's `compress_knots`.
    let mut knots: Vec<f64> = Vec::new();
    for (k, m) in distinct.iter().zip(&mults) {
        knots.extend(std::iter::repeat(*k).take(*m));
    }
    if knots.len() != control_pts.len() + degree + 1 {
        return None; // malformed — leave the face surface-less rather than guess
    }

    let full = if rational {
        let weights: Vec<f64> = curve.args.get(8)?.as_list()?.iter().filter_map(|v| v.as_f64()).collect();
        if weights.len() != control_pts.len() {
            return None;
        }
        nurbs::tessellate(&control_pts, &weights, &knots, degree, ARC_CHORD_TOL_MM).ok()?
    } else {
        bspline::tessellate(&control_pts, &knots, degree, ARC_CHORD_TOL_MM).ok()?
    };
    if full.len() < 3 {
        return None;
    }

    // Interior only — the endpoints are already the loop's vertices — oriented to
    // the loop's own start→end traversal.
    let forward = (full[0] - start).length_squared() <= (full[0] - end).length_squared();
    let mut interior: Vec<DVec3> = full[1..full.len() - 1].to_vec();
    if !forward {
        interior.reverse();
    }
    Some(interior)
}

/// `IfcVertexPoint` → its `IfcCartesianPoint`.
fn vertex_point(file: &StepFile, id: u32, scale: f64) -> Option<DVec3> {
    let v = file.entity(id)?;
    if v.tag.eq_ignore_ascii_case("IFCVERTEXPOINT") {
        let p = v.args.first()?.as_ref()?;
        return cartesian_point(file, p, scale);
    }
    cartesian_point(file, id, scale)
}

/// `IfcCartesianPoint((x,y,z))` → engine-unit position.
fn cartesian_point(file: &StepFile, id: u32, scale: f64) -> Option<DVec3> {
    let p = file.entity(id)?;
    if !p.tag.eq_ignore_ascii_case("IFCCARTESIANPOINT") {
        return None;
    }
    let coords = p.args.first()?.as_list()?;
    let x = coords.first()?.as_f64()?;
    let y = coords.get(1)?.as_f64()?;
    let z = coords.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0); // 2D points are legal
    Some(DVec3::new(x, y, z) * scale)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{emit_box, emit_ifc_model, IfcElement};
    use axia_geo::{MaterialId, Mesh};

    #[test]
    fn faceted_box_round_trips_to_six_quads() {
        // emit_box writes a 1×2×3 m box as an IfcFacetedBrep of 6 polyloops.
        let ifc = emit_box(DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0), "Box");
        let g = import_ifc_geometry(&ifc).unwrap();

        assert_eq!(g.scale_to_mm, 1000.0, "file is in metres");
        assert_eq!(g.elements.len(), 1);
        let e = &g.elements[0];
        assert_eq!(e.name.as_deref(), Some("Box"));
        assert_eq!(e.faces.len(), 6, "six box faces");
        for f in &e.faces {
            assert_eq!(f.outer.len(), 4, "each face is a quad");
            assert!(f.inners.is_empty());
        }
        assert_eq!(g.face_count(), 6);
        assert!(g.warnings.is_empty(), "warnings: {:?}", g.warnings);

        // metres → mm: the far corner is (1000, 2000, 3000).
        let far = e.faces.iter().flat_map(|f| f.outer.iter()).fold(DVec3::ZERO, |a, &p| a.max(p));
        assert!((far - DVec3::new(1000.0, 2000.0, 3000.0)).length() < 1e-6, "far corner {:?}", far);
    }

    /// A semicircle face: an arc from A(4,0.5) to B(4,3.5) on the circle
    /// centred (4,2) r=1.5, closed by the straight diameter B→A. `sense` is the
    /// trimmed curve's `SenseAgreement` — the only thing that says which half.
    fn semicircle_ifc(sense: &str) -> String {
        format!(
            "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINT((4.,0.5,0.));
#11=IFCCARTESIANPOINT((4.,3.5,0.));
#12=IFCCARTESIANPOINT((4.,2.,0.));
#13=IFCDIRECTION((0.,0.,1.));
#14=IFCDIRECTION((1.,0.,0.));
#15=IFCAXIS2PLACEMENT3D(#12,#13,#14);
#16=IFCCIRCLE(#15,1.5);
#20=IFCVERTEXPOINT(#10);
#21=IFCVERTEXPOINT(#11);
#22=IFCTRIMMEDCURVE(#16,(#10),(#11),{sense},.CARTESIAN.);
#23=IFCEDGECURVE(#20,#21,#22,.T.);
#25=IFCDIRECTION((0.,-1.,0.));
#26=IFCVECTOR(#25,1.);
#27=IFCLINE(#11,#26);
#28=IFCEDGECURVE(#21,#20,#27,.T.);
#30=IFCORIENTEDEDGE(*,*,#23,.T.);
#31=IFCORIENTEDEDGE(*,*,#28,.T.);
#32=IFCEDGELOOP((#30,#31));
#33=IFCFACEOUTERBOUND(#32,.T.);
#35=IFCPLANE(#15);
#36=IFCADVANCEDFACE((#33),#35,.T.);
#37=IFCCLOSEDSHELL((#36));
#38=IFCADVANCEDBREP(#37);
#39=IFCSHAPEREPRESENTATION($,'Body','AdvancedBrep',(#38));
#40=IFCPRODUCTDEFINITIONSHAPE($,$,(#39));
#41=IFCBUILDINGELEMENTPROXY('gid',$,'Arc',$,$,$,#40,$,.NOTDEFINED.);
ENDSEC;
END-ISO-10303-21;
",
            sense = sense
        )
    }

    #[test]
    fn an_arc_edge_is_walked_not_chorded() {
        // A curved edge read by its endpoints alone is a straight chord — the
        // face looks fine and is the wrong shape. The arc must gain interior
        // points, all of them exactly on the circle.
        let g = import_ifc_geometry(&semicircle_ifc(".T.")).unwrap();
        let f = &g.elements[0].faces[0];
        assert!(f.outer.len() > 4, "the arc added interior points: {}", f.outer.len());

        let center = DVec3::new(4000.0, 2000.0, 0.0); // metres → mm
        let mut on_arc = 0;
        for p in &f.outer {
            let r = (*p - center).length();
            if (r - 1500.0).abs() < 1.0 {
                on_arc += 1;
            }
        }
        assert!(on_arc >= 8, "interior points sit on the r=1500 circle: {on_arc}");
    }

    #[test]
    fn the_trim_sense_picks_which_half_circle() {
        // Same two endpoints, a diameter apart — the sense is the *only* thing
        // that says which half. This is exactly what reading endpoints (or
        // guessing from edge flags) cannot get right.
        let center = DVec3::new(4000.0, 2000.0, 0.0);
        let right = DVec3::new(5500.0, 2000.0, 0.0); // angle 0
        let left = DVec3::new(2500.0, 2000.0, 0.0); // angle π

        let near = |loops: &FaceLoops, target: DVec3| {
            loops.outer.iter().any(|p| (*p - target).length() < 10.0)
        };

        // SenseAgreement TRUE → CCW from bottom to top → through the right side.
        let t = import_ifc_geometry(&semicircle_ifc(".T.")).unwrap();
        let ft = &t.elements[0].faces[0];
        assert!(near(ft, right), "sense .T. sweeps the right half (through {right:?})");
        assert!(!near(ft, left), "and not the left");

        // SenseAgreement FALSE → CW → through the left side. The opposite arc,
        // from identical endpoints.
        let f = import_ifc_geometry(&semicircle_ifc(".F.")).unwrap();
        let ff = &f.elements[0].faces[0];
        assert!(near(ff, left), "sense .F. sweeps the left half (through {left:?})");
        assert!(!near(ff, right), "and not the right");

        // The centre never moves — this is a direction flip, not a translation.
        let _ = center;
    }

    /// A closed-circle face: one self-loop edge (EdgeStart == EdgeEnd) whose
    /// geometry is the circle — how ADR-089 Path B and BIM tools write a round
    /// disk. `trimmed` toggles the two forms producers use.
    fn closed_circle_ifc(trimmed: bool) -> String {
        let curve = if trimmed {
            // Trim1 == Trim2 (one point, full turn).
            "#17=IFCTRIMMEDCURVE(#15,(#10),(#10),.T.,.CARTESIAN.);\n#18=IFCEDGECURVE(#16,#16,#17,.T.);"
        } else {
            // Bare circle, no trim.
            "#18=IFCEDGECURVE(#16,#16,#15,.T.);"
        };
        format!(
            "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINT((1.5,0.,0.));
#11=IFCCARTESIANPOINT((0.,0.,0.));
#12=IFCDIRECTION((0.,0.,1.));
#13=IFCDIRECTION((1.,0.,0.));
#14=IFCAXIS2PLACEMENT3D(#11,#12,#13);
#15=IFCCIRCLE(#14,1.5);
#16=IFCVERTEXPOINT(#10);
{curve}
#19=IFCORIENTEDEDGE(*,*,#18,.T.);
#20=IFCEDGELOOP((#19));
#21=IFCFACEOUTERBOUND(#20,.T.);
#22=IFCPLANE(#14);
#23=IFCADVANCEDFACE((#21),#22,.T.);
#24=IFCCLOSEDSHELL((#23));
#25=IFCADVANCEDBREP(#24);
#26=IFCSHAPEREPRESENTATION($,'Body','AdvancedBrep',(#25));
#27=IFCPRODUCTDEFINITIONSHAPE($,$,(#26));
#28=IFCBUILDINGELEMENTPROXY('g',$,'Disk',$,$,$,#27,$,.NOTDEFINED.);
ENDSEC;
END-ISO-10303-21;
",
            curve = curve
        )
    }

    /// Build a mesh holding one closed-spline face and export it, so the
    /// importer meets a real `IfcBSplineCurveWithKnots` self-loop — the form our
    /// exporter and most tools use for Bezier / B-spline / NURBS / ellipse.
    fn closed_spline_ifc(rational: bool) -> String {
        use axia_geo::curves::AnalyticCurve;
        // A clamped quadratic closed loop: first control point repeated at the
        // end, clamped end knots (ADR-089 A-Α / A-Β).
        let cps = vec![
            DVec3::new(500.0, 0.0, 0.0),
            DVec3::new(500.0, 500.0, 0.0),
            DVec3::new(-500.0, 500.0, 0.0),
            DVec3::new(-500.0, 0.0, 0.0),
            DVec3::new(500.0, 0.0, 0.0),
        ];
        // 5 control points, degree 2 → 5 + 2 + 1 = 8 knots (clamped ends).
        let knots = vec![0.0, 0.0, 0.0, 0.33, 0.66, 1.0, 1.0, 1.0];
        let degree = 2;
        let curve = if rational {
            AnalyticCurve::NURBS { control_pts: cps, weights: vec![1.0; 5], knots, degree }
        } else {
            AnalyticCurve::BSpline { control_pts: cps, knots, degree }
        };

        let mut mesh = Mesh::new();
        let anchor = mesh.add_vertex(DVec3::new(500.0, 0.0, 0.0));
        let f = mesh
            .add_face_closed_curve(anchor, curve, MaterialId::new(0))
            .expect("closed spline face");
        emit_ifc_model(
            &mesh,
            &[IfcElement {
                name: "Spline".into(),
                material_name: None,
                kind: crate::IfcElementKind::Wall,
                face_ids: vec![f],
            }],
            0.001,
            "Spline",
        )
        .expect("emit")
    }

    #[test]
    fn a_closed_spline_self_loop_becomes_a_ring() {
        // Bezier / B-spline / NURBS / ellipse all reach IFC as an
        // IfcBSplineCurveWithKnots. A self-loop of one — start == end — used to
        // collapse to a point and drop the face. It must walk the whole curve,
        // in both the plain and the rational (weighted) forms.
        for rational in [false, true] {
            let ifc = closed_spline_ifc(rational);
            assert!(
                ifc.contains(if rational {
                    "IFCRATIONALBSPLINECURVEWITHKNOTS"
                } else {
                    "IFCBSPLINECURVEWITHKNOTS"
                }),
                "the fixture really is a {} spline",
                if rational { "rational" } else { "plain" }
            );

            let g = import_ifc_geometry(&ifc).unwrap();
            assert_eq!(g.elements.len(), 1, "the spline face imports (rational={rational})");
            let f = &g.elements[0].faces[0];
            assert!(
                f.outer.len() > 16,
                "walked to a ring, not collapsed to a point (rational={rational}): {}",
                f.outer.len()
            );

            // The loop closes and stays near the control hull (a sanity bound —
            // no point flies off), and it is genuinely 2D-spread, not a spike.
            let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
            for p in &f.outer {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
            assert!(hi.x - lo.x > 300.0 && hi.y - lo.y > 300.0, "spread in X and Y (rational={rational})");
            assert!(
                f.outer.iter().all(|p| p.x.abs() < 700.0 && p.y.abs() < 700.0),
                "no point escapes the control hull (rational={rational})"
            );
        }
    }

    #[test]
    fn a_closed_circle_self_loop_becomes_a_full_ring() {
        // The whole rim lives in one edge whose start == end. Read by that
        // single vertex it collapses to a point and the face is dropped — the
        // bug this closes. It must sweep the full turn, both in the bare-circle
        // form and the trim-to-the-same-point form producers use.
        for trimmed in [false, true] {
            let g = import_ifc_geometry(&closed_circle_ifc(trimmed)).unwrap();
            assert_eq!(g.elements.len(), 1, "the disk imports (trimmed={trimmed})");
            let f = &g.elements[0].faces[0];

            // A ring, not a point: many vertices, every one on the r=1500 circle.
            assert!(f.outer.len() > 32, "full ring, not a chord (trimmed={trimmed}): {}", f.outer.len());
            let center = DVec3::ZERO;
            assert!(
                f.outer.iter().all(|p| ((*p - center).length() - 1500.0).abs() < 1.0),
                "every point sits on the circle (trimmed={trimmed})"
            );

            // It spans the whole circle, not just an arc of it — points near
            // all four cardinal directions.
            let has = |tx: f64, ty: f64| {
                f.outer.iter().any(|p| (p.x - tx).abs() < 30.0 && (p.y - ty).abs() < 30.0)
            };
            assert!(has(1500.0, 0.0) && has(-1500.0, 0.0), "reaches ±X (trimmed={trimmed})");
            assert!(has(0.0, 1500.0) && has(0.0, -1500.0), "reaches ±Y (trimmed={trimmed})");
        }
    }

    #[test]
    fn a_circular_hole_self_loop_imports_as_an_inner_ring() {
        // A round hole is the same self-loop, used as an inner bound. It has to
        // arrive as a full inner ring, not a single collapsed point.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#30=IFCCARTESIANPOINT((-4.,-4.,0.));
#31=IFCCARTESIANPOINT((4.,-4.,0.));
#32=IFCCARTESIANPOINT((4.,4.,0.));
#33=IFCCARTESIANPOINT((-4.,4.,0.));
#34=IFCPOLYLOOP((#30,#31,#32,#33));
#35=IFCFACEOUTERBOUND(#34,.T.);
#40=IFCCARTESIANPOINT((1.5,0.,0.));
#41=IFCCARTESIANPOINT((0.,0.,0.));
#42=IFCDIRECTION((0.,0.,1.));
#43=IFCDIRECTION((1.,0.,0.));
#44=IFCAXIS2PLACEMENT3D(#41,#42,#43);
#45=IFCCIRCLE(#44,1.5);
#46=IFCVERTEXPOINT(#40);
#47=IFCEDGECURVE(#46,#46,#45,.T.);
#48=IFCORIENTEDEDGE(*,*,#47,.T.);
#49=IFCEDGELOOP((#48));
#50=IFCFACEBOUND(#49,.T.);
#51=IFCPLANE(#44);
#52=IFCADVANCEDFACE((#35,#50),#51,.T.);
#53=IFCCLOSEDSHELL((#52));
#54=IFCADVANCEDBREP(#53);
#55=IFCSHAPEREPRESENTATION($,'Body','AdvancedBrep',(#54));
#56=IFCPRODUCTDEFINITIONSHAPE($,$,(#55));
#57=IFCBUILDINGELEMENTPROXY('g',$,'Holed',$,$,$,#56,$,.NOTDEFINED.);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        let f = &g.elements[0].faces[0];
        assert_eq!(f.outer.len(), 4, "the square outer boundary");
        assert_eq!(f.inners.len(), 1, "one hole");
        let ring = &f.inners[0];
        assert!(ring.len() > 32, "the hole is a full ring, not a point: {}", ring.len());
        assert!(
            ring.iter().all(|p| ((*p - DVec3::ZERO).length() - 1500.0).abs() < 1.0),
            "every hole point sits on the r=1500 circle"
        );
    }

    #[test]
    fn an_open_arc_is_not_turned_into_a_full_circle() {
        // Guard the other direction: the closed-loop path must not swallow an
        // open arc. The semicircle has distinct endpoints and stays a half.
        let g = import_ifc_geometry(&semicircle_ifc(".T.")).unwrap();
        let f = &g.elements[0].faces[0];
        // A full ring would reach the left side (−X); a right half does not.
        assert!(
            !f.outer.iter().any(|p| p.x < 3000.0),
            "the open arc stayed a half-circle, no wrap to the far side"
        );
    }

    #[test]
    fn advanced_box_round_trips_with_material() {
        let mut mesh = Mesh::new();
        let faces = mesh
            .create_box(DVec3::ZERO, 2000.0, 3000.0, 4000.0, MaterialId::new(0))
            .unwrap();
        let ifc = emit_ifc_model(
            &mesh,
            &[IfcElement { name: "벽체".into(), material_name: Some("강철".into()), kind: crate::IfcElementKind::Wall, face_ids: faces }],
            0.001,
            "House",
        )
        .unwrap();

        let g = import_ifc_geometry(&ifc).unwrap();
        assert_eq!(g.elements.len(), 1);
        let e = &g.elements[0];
        assert_eq!(e.name.as_deref(), Some("벽체"));
        assert_eq!(e.material.as_deref(), Some("강철"));
        assert_eq!(e.faces.len(), 6, "IfcAdvancedBrep edge loops → 6 quads");
        for f in &e.faces {
            assert_eq!(f.outer.len(), 4);
        }

        // The exported box was 2000×4000×3000 mm (w=X, h=Z, d=Y) centred at the
        // origin, so it comes back spanning ±1000 / ±2000 / ±1500.
        let pts: Vec<DVec3> = e.faces.iter().flat_map(|f| f.outer.iter().copied()).collect();
        let max = pts.iter().fold(DVec3::splat(f64::MIN), |a, &p| a.max(p));
        let min = pts.iter().fold(DVec3::splat(f64::MAX), |a, &p| a.min(p));
        assert!((max - DVec3::new(1000.0, 2000.0, 1500.0)).length() < 1e-6, "max {:?}", max);
        assert!((min + DVec3::new(1000.0, 2000.0, 1500.0)).length() < 1e-6, "min {:?}", min);
    }

    #[test]
    fn millimetre_files_are_not_rescaled() {
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);
ENDSEC;
END-ISO-10303-21;
";
        let file = step_parser::parse(src).unwrap();
        let mut w = Vec::new();
        assert_eq!(length_scale_to_mm(&file, &mut w), 1.0, "milli-metre file is already mm");
        assert!(w.is_empty());
    }

    #[test]
    fn missing_unit_warns_and_assumes_metre() {
        let file = step_parser::parse("ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\nENDSEC;\nEND-ISO-10303-21;\n").unwrap();
        let mut w = Vec::new();
        assert_eq!(length_scale_to_mm(&file, &mut w), 1000.0);
        assert_eq!(w.len(), 1);
        assert!(w[0].contains("assuming metre"), "{}", w[0]);
    }

    #[test]
    fn hand_written_polyloop_triangle_reads() {
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#2=IFCCARTESIANPOINT((0.,0.,0.));
#3=IFCCARTESIANPOINT((1.,0.,0.));
#4=IFCCARTESIANPOINT((0.,1.,0.));
#5=IFCPOLYLOOP((#2,#3,#4));
#6=IFCFACEOUTERBOUND(#5,.T.);
#7=IFCFACE((#6));
#8=IFCCLOSEDSHELL((#7));
#9=IFCFACETEDBREP(#8);
#10=IFCSHAPEREPRESENTATION($,'Body','Brep',(#9));
#11=IFCPRODUCTDEFINITIONSHAPE($,$,(#10));
#12=IFCWALL('gid',$,'Tri',$,$,$,#11,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        assert_eq!(g.elements.len(), 1);
        assert_eq!(g.elements[0].faces.len(), 1);
        let f = &g.elements[0].faces[0];
        assert_eq!(f.outer, vec![DVec3::ZERO, DVec3::new(1000.0, 0.0, 0.0), DVec3::new(0.0, 1000.0, 0.0)]);
    }

    #[test]
    fn face_with_a_hole_keeps_the_inner_loop() {
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,.MILLI.,.METRE.);
#2=IFCCARTESIANPOINT((0.,0.,0.));
#3=IFCCARTESIANPOINT((10.,0.,0.));
#4=IFCCARTESIANPOINT((10.,10.,0.));
#5=IFCCARTESIANPOINT((0.,10.,0.));
#6=IFCCARTESIANPOINT((3.,3.,0.));
#7=IFCCARTESIANPOINT((6.,3.,0.));
#8=IFCCARTESIANPOINT((6.,6.,0.));
#9=IFCPOLYLOOP((#2,#3,#4,#5));
#10=IFCPOLYLOOP((#6,#7,#8));
#11=IFCFACEOUTERBOUND(#9,.T.);
#12=IFCFACEBOUND(#10,.T.);
#13=IFCFACE((#11,#12));
#14=IFCCLOSEDSHELL((#13));
#15=IFCFACETEDBREP(#14);
#16=IFCSHAPEREPRESENTATION($,'Body','Brep',(#15));
#17=IFCPRODUCTDEFINITIONSHAPE($,$,(#16));
#18=IFCSLAB('gid',$,'Holed',$,$,$,#17,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        let f = &g.elements[0].faces[0];
        assert_eq!(f.outer.len(), 4);
        assert_eq!(f.inners.len(), 1, "the hole survives");
        assert_eq!(f.inners[0].len(), 3);
        // milli prefix → coordinates are already mm
        assert_eq!(f.outer[1], DVec3::new(10.0, 0.0, 0.0));
    }

    #[test]
    fn degenerate_loops_are_dropped_not_imported_wrong() {
        // A "face" whose loop has two points cannot be a polygon.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#2=IFCCARTESIANPOINT((0.,0.,0.));
#3=IFCCARTESIANPOINT((1.,0.,0.));
#4=IFCPOLYLOOP((#2,#3));
#5=IFCFACEOUTERBOUND(#4,.T.);
#6=IFCFACE((#5));
#7=IFCCLOSEDSHELL((#6));
#8=IFCFACETEDBREP(#7);
#9=IFCSHAPEREPRESENTATION($,'Body','Brep',(#8));
#10=IFCPRODUCTDEFINITIONSHAPE($,$,(#9));
#11=IFCWALL('gid',$,'Degenerate',$,$,$,#10,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        assert_eq!(g.face_count(), 0, "no face invented from a 2-point loop");
        assert!(g.elements.is_empty(), "element with no usable face is not listed");

        // Dropping it silently would look like an empty file. The user gets
        // told which member was skipped and why.
        let joined = g.warnings.join(" | ");
        assert!(
            joined.contains("Degenerate") && joined.contains("skipped"),
            "the skipped face is named: {joined}"
        );
        assert!(
            joined.contains("no usable faces"),
            "and so is the member that ended up empty: {joined}"
        );
    }

    #[test]
    fn a_member_is_placed_by_its_local_placement_chain() {
        // I-4. The triangle is written at the member's own origin; the chain
        // says the storey is 3 m up and the wall 1 m along +X, yawed 90°.
        // Without the chain this lands at the origin — the bug I-4 fixes.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#2=IFCCARTESIANPOINT((0.,0.,3.));
#3=IFCAXIS2PLACEMENT3D(#2,$,$);
#4=IFCLOCALPLACEMENT($,#3);
#5=IFCCARTESIANPOINT((1.,0.,0.));
#6=IFCDIRECTION((0.,0.,1.));
#7=IFCDIRECTION((0.,1.,0.));
#8=IFCAXIS2PLACEMENT3D(#5,#6,#7);
#9=IFCLOCALPLACEMENT(#4,#8);
#10=IFCCARTESIANPOINT((0.,0.,0.));
#11=IFCCARTESIANPOINT((2.,0.,0.));
#12=IFCCARTESIANPOINT((0.,1.,0.));
#13=IFCPOLYLOOP((#10,#11,#12));
#14=IFCFACEOUTERBOUND(#13,.T.);
#15=IFCFACE((#14));
#16=IFCCLOSEDSHELL((#15));
#17=IFCFACETEDBREP(#16);
#18=IFCSHAPEREPRESENTATION($,'Body','Brep',(#17));
#19=IFCPRODUCTDEFINITIONSHAPE($,$,(#18));
#20=IFCWALL('gid',$,'Placed',$,$,#9,#19,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        let f = &g.elements[0].faces[0];

        // Wall origin: storey (0,0,3000) + yawed offset (1000,0,0)→(1000,0,0)
        // — the parent is unrotated, so the offset stays on +X.
        assert!(
            (f.outer[0] - DVec3::new(1000.0, 0.0, 3000.0)).length() < 1e-6,
            "local origin lands at the wall's placed origin: {:?}",
            f.outer[0]
        );
        // Local +X (2 m) is yawed 90° by the wall's own placement → world +Y.
        assert!(
            (f.outer[1] - DVec3::new(1000.0, 2000.0, 3000.0)).length() < 1e-6,
            "local +X becomes world +Y: {:?}",
            f.outer[1]
        );
        // Local +Y (1 m) → world −X.
        assert!(
            (f.outer[2] - DVec3::new(0.0, 0.0, 3000.0)).length() < 1e-6,
            "local +Y becomes world −X: {:?}",
            f.outer[2]
        );

        // The face still knows its plane after being moved.
        assert!(f.plane().is_some(), "a placed face keeps a usable plane");
    }

    #[test]
    fn identity_placement_leaves_our_own_files_untouched() {
        // We bake world coordinates and emit an identity placement, so I-4 must
        // be a no-op for our own export — this is the regression that catches a
        // double transform.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#2=IFCCARTESIANPOINT((0.,0.,0.));
#3=IFCAXIS2PLACEMENT3D(#2,#20,#21);
#4=IFCLOCALPLACEMENT($,#3);
#20=IFCDIRECTION((0.,0.,1.));
#21=IFCDIRECTION((1.,0.,0.));
#10=IFCCARTESIANPOINT((0.8,1.6,2.7));
#11=IFCCARTESIANPOINT((1.2,1.6,2.7));
#12=IFCCARTESIANPOINT((1.2,2.4,2.7));
#13=IFCPOLYLOOP((#10,#11,#12));
#14=IFCFACEOUTERBOUND(#13,.T.);
#15=IFCFACE((#14));
#16=IFCCLOSEDSHELL((#15));
#17=IFCFACETEDBREP(#16);
#18=IFCSHAPEREPRESENTATION($,'Body','Brep',(#17));
#19=IFCPRODUCTDEFINITIONSHAPE($,$,(#18));
#22=IFCWALL('gid',$,'Baked',$,$,#4,#19,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        let f = &g.elements[0].faces[0];
        assert_eq!(f.outer[0], DVec3::new(800.0, 1600.0, 2700.0));
        assert_eq!(f.outer[2], DVec3::new(1200.0, 2400.0, 2700.0));
        assert!(g.warnings.is_empty(), "no warning for an identity file: {:?}", g.warnings);
    }

    #[test]
    fn a_member_carries_its_spatial_container() {
        // I-5. Without this the model arrives as one flat pile — no way to hide
        // a floor or select a whole member.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#2=IFCBUILDING('b',$,'Building',$,$,$,$,$,.ELEMENT.,$,$,$);
#3=IFCBUILDINGSTOREY('l1',$,'Level 1',$,$,$,$,$,.ELEMENT.,$);
#4=IFCRELAGGREGATES('a',$,$,$,#2,(#3));
#10=IFCCARTESIANPOINT((0.,0.,0.));
#11=IFCCARTESIANPOINT((1.,0.,0.));
#12=IFCCARTESIANPOINT((1.,1.,0.));
#13=IFCPOLYLOOP((#10,#11,#12));
#14=IFCFACEOUTERBOUND(#13,.T.);
#15=IFCFACE((#14));
#16=IFCCLOSEDSHELL((#15));
#17=IFCFACETEDBREP(#16);
#18=IFCSHAPEREPRESENTATION($,'Body','Brep',(#17));
#19=IFCPRODUCTDEFINITIONSHAPE($,$,(#18));
#20=IFCWALL('w',$,'Wall A',$,$,$,#19,$,$);
#21=IFCRELCONTAINEDINSPATIALSTRUCTURE('c',$,$,$,(#20),#3);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        assert_eq!(g.elements[0].container, Some(3), "the wall knows its storey");
        assert_eq!(g.spatial.nodes[&3].parent, Some(2), "and the storey its building");
        assert_eq!(g.spatial.nodes[&3].label(), "Level 1");
    }

    #[test]
    fn a_member_with_no_container_is_left_unfiled() {
        // Not every file carries the relation; inventing a container would be
        // worse than leaving the member at the top level.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINT((0.,0.,0.));
#11=IFCCARTESIANPOINT((1.,0.,0.));
#12=IFCCARTESIANPOINT((1.,1.,0.));
#13=IFCPOLYLOOP((#10,#11,#12));
#14=IFCFACEOUTERBOUND(#13,.T.);
#15=IFCFACE((#14));
#16=IFCCLOSEDSHELL((#15));
#17=IFCFACETEDBREP(#16);
#18=IFCSHAPEREPRESENTATION($,'Body','Brep',(#17));
#19=IFCPRODUCTDEFINITIONSHAPE($,$,(#18));
#20=IFCWALL('w',$,'Lonely',$,$,$,#19,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        assert_eq!(g.elements[0].container, None);
        assert!(g.spatial.is_empty(), "no containers invented");
    }

    #[test]
    fn a_shifted_world_coordinate_system_is_reported() {
        // We do not apply the context WCS; a file that sets one must not import
        // silently as if it had not.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#2=IFCCARTESIANPOINT((100.,0.,0.));
#3=IFCAXIS2PLACEMENT3D(#2,$,$);
#4=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#3,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        assert!(
            g.warnings.iter().any(|w| w.contains("WorldCoordinateSystem")),
            "warnings: {:?}",
            g.warnings
        );
    }

    #[test]
    fn missing_brep_is_an_error_not_a_panic() {
        let file = step_parser::parse(
            "ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=IFCFACETEDBREP(#99);\nENDSEC;\nEND-ISO-10303-21;\n",
        )
        .unwrap();
        assert!(brep_face_loops(&file, 1, 1000.0).is_err(), "dangling shell ref");
        assert!(brep_face_loops(&file, 42, 1000.0).is_err(), "missing brep");
    }

    #[test]
    fn face_loops_derive_their_plane() {
        // A face imported without a surface is refused by every kernel-aware
        // op (ADR-087 K-ε), so the plane has to come out of the loop itself.
        let f = FaceLoops {
            outer: vec![
                DVec3::new(0.0, 0.0, 5.0),
                DVec3::new(10.0, 0.0, 5.0),
                DVec3::new(10.0, 4.0, 5.0),
                DVec3::new(0.0, 4.0, 5.0),
            ],
            inners: vec![],
        };
        match f.plane().expect("planar loop yields a plane") {
            AnalyticSurface::Plane {
                origin,
                normal,
                basis_u,
                ..
            } => {
                assert!((normal - DVec3::Z).length() < 1e-12, "CCW in XY faces +Z: {normal}");
                assert!((origin - DVec3::new(0.0, 0.0, 5.0)).length() < 1e-12);
                assert!((basis_u - DVec3::X).length() < 1e-12, "first edge is +X: {basis_u}");
                assert!(normal.dot(basis_u).abs() < 1e-12, "basis_u ⟂ normal");
            }
            other => panic!("expected a plane, got {other:?}"),
        }
    }

    #[test]
    fn newell_survives_a_collinear_opening_triple() {
        // The first three points are collinear — a naive (b-a)×(c-a) normal is
        // zero here, Newell's is not.
        let f = FaceLoops {
            outer: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 0.0, 0.0),
                DVec3::new(2.0, 0.0, 0.0),
                DVec3::new(2.0, 3.0, 0.0),
                DVec3::new(0.0, 3.0, 0.0),
            ],
            inners: vec![],
        };
        let AnalyticSurface::Plane { normal, .. } = f.plane().expect("plane") else {
            panic!("expected a plane");
        };
        assert!((normal - DVec3::Z).length() < 1e-12, "got {normal}");
    }

    #[test]
    fn degenerate_loops_have_no_plane() {
        // Zero area (all collinear) and too-few points both yield None rather
        // than a meaningless plane.
        let line = FaceLoops {
            outer: vec![
                DVec3::new(0.0, 0.0, 0.0),
                DVec3::new(1.0, 0.0, 0.0),
                DVec3::new(2.0, 0.0, 0.0),
            ],
            inners: vec![],
        };
        assert!(line.plane().is_none(), "collinear loop has no plane");

        let two = FaceLoops {
            outer: vec![DVec3::ZERO, DVec3::X],
            inners: vec![],
        };
        assert!(two.plane().is_none(), "2 points cannot span a plane");
    }
}

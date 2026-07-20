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

use axia_foreign::step_parser::{self, Entity, StepFile};
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
    pub faces: Vec<FaceLoops>,
}

/// Result of reading a whole file's geometry.
#[derive(Clone, Debug, Default)]
pub struct GeometryImport {
    pub elements: Vec<ElementGeometry>,
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
            faces,
        });
    }
    GeometryImport { elements, scale_to_mm: scale, placed, warnings }
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
            let (start, _end) = if orientation { (a, b) } else { (b, a) };
            if let Some(p) = start {
                // Skip a repeat of the previous point (closed rims repeat their anchor).
                if pts.last().map_or(true, |q: &DVec3| (*q - p).length() > 1e-9) {
                    pts.push(p);
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

    #[test]
    fn advanced_box_round_trips_with_material() {
        let mut mesh = Mesh::new();
        let faces = mesh
            .create_box(DVec3::ZERO, 2000.0, 3000.0, 4000.0, MaterialId::new(0))
            .unwrap();
        let ifc = emit_ifc_model(
            &mesh,
            &[IfcElement { name: "벽체".into(), material_name: Some("강철".into()), face_ids: faces }],
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

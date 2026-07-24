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
    /// The exact curve when this whole face is a single closed-curve disk — one
    /// self-loop edge, no holes (ADR-089 Path B). Present, the importer can
    /// build a *kernel-native* face (one anchor + one self-loop edge carrying
    /// the curve) instead of baking the tessellated `outer` polygon into the
    /// DCEL, so a drawn circle and an imported one are the same thing.
    ///
    /// `outer` is still filled (the tessellation) as a fallback for when the
    /// kernel-native build is not applicable — e.g. under a non-identity
    /// placement, which moves the polygon but not this curve.
    pub closed_curve: Option<axia_geo::AnalyticCurve>,
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

/// A CSG operator from an `IfcBooleanResult` — how two operands combine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    /// `.DIFFERENCE.` — the first operand minus the second (a wall minus its
    /// opening, the common case).
    Subtract,
    Intersect,
}

/// One operand of a boolean result: a solid, a nested boolean, or a half-space
/// clip. A half-space isn't a closed solid (it's unbounded), so it can only be
/// the *subtrahend* — it clips the other operand rather than being built itself.
#[derive(Clone, Debug, PartialEq)]
pub enum CsgOperand {
    Solid(Vec<FaceLoops>),
    Node(Box<CsgNode>),
    HalfSpace(HalfSpace),
}

/// An `IfcHalfSpaceSolid` / `IfcPolygonalBoundedHalfSpace` — the half of space on
/// one side of a plane, optionally bounded laterally by a polygon prism. Real BIM
/// tools clip a wall with one (a sloped cut, a channel). It has no closed volume,
/// so import evaluates it as a *trim*: unbounded ones cut the other operand by the
/// plane; polygonally bounded ones cut it by the polygon's prism ∩ the half-space.
#[derive(Clone, Debug, PartialEq)]
pub struct HalfSpace {
    /// A point on the base plane (world, engine units).
    pub base_origin: DVec3,
    /// The base plane's unit normal (world) — the `IfcPlane` local Z.
    pub base_normal: DVec3,
    /// IFC `AgreementFlag`: FALSE → the material is on the normal-positive side of
    /// the plane, TRUE → the negative side.
    pub agreement: bool,
    /// `IfcPolygonalBoundedHalfSpace` only: `(polygon, extrude_dir)` — the boundary
    /// polygon in world space and the perpendicular (Position local Z) it sweeps.
    /// `None` for an unbounded `IfcHalfSpaceSolid`.
    pub boundary: Option<(Vec<DVec3>, DVec3)>,
}

impl HalfSpace {
    fn transform(&mut self, p: &crate::ifc_placement::Placement) {
        // A point moves; a direction only rotates (apply minus the origin shift).
        let rotate = |d: DVec3| (p.apply(d) - p.origin).normalize_or_zero();
        self.base_origin = p.apply(self.base_origin);
        self.base_normal = rotate(self.base_normal);
        if let Some((poly, dir)) = &mut self.boundary {
            for q in poly.iter_mut() {
                *q = p.apply(*q);
            }
            *dir = rotate(*dir);
        }
    }
}

/// An `IfcBooleanResult`: two operands combined by an operator. This is how real
/// BIM tools write a wall *with an opening* — the wall solid minus the opening
/// solid — so the tree is walked and evaluated with the engine's own boolean.
#[derive(Clone, Debug, PartialEq)]
pub struct CsgNode {
    pub op: BoolOp,
    pub first: CsgOperand,
    pub second: CsgOperand,
}

impl CsgOperand {
    fn transform(&mut self, p: &crate::ifc_placement::Placement) {
        match self {
            CsgOperand::Solid(fs) => {
                for f in fs {
                    f.transform(p);
                    f.closed_curve = None;
                }
            }
            CsgOperand::Node(n) => n.transform(p),
            CsgOperand::HalfSpace(h) => h.transform(p),
        }
    }
}

impl CsgNode {
    fn transform(&mut self, p: &crate::ifc_placement::Placement) {
        self.first.transform(p);
        self.second.transform(p);
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
    /// `IfcBooleanResult` trees — a wall with an opening, evaluated with the
    /// engine's boolean when the member is imported.
    pub booleans: Vec<CsgNode>,
    /// The wall this member fills an opening in (`IfcRelFillsElement` →
    /// `IfcRelVoidsElement`, I-5). A door or window is grouped under that wall
    /// rather than sitting loose in the storey. `None` for a plain member.
    pub fills_wall: Option<u32>,
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
    // Openings that void a wall (IfcRelVoidsElement) — subtracted below so a door
    // or window opening becomes a real hole rather than a phantom overlap.
    let voids = collect_voids(file);
    // Which wall each door / window fills an opening in (IfcRelFillsElement) —
    // used to group the filler under its wall (I-5).
    let fills = collect_fills(file);

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

        let mut booleans: Vec<CsgNode> = Vec::new();
        for g in &el.geometry {
            if !g.supported {
                continue; // I-2 already reported it
            }
            supported_geometry += 1;
            // An IfcBooleanResult (a wall with an opening) is a CSG tree, not a
            // face list — parse it and evaluate it at import time.
            let gtag = file.entity(g.id).map(|e| e.tag.to_ascii_uppercase()).unwrap_or_default();
            if gtag == "IFCBOOLEANRESULT" || gtag == "IFCBOOLEANCLIPPINGRESULT" {
                match parse_boolean(file, g.id, scale) {
                    Some(mut node) => {
                        if !placement.is_identity() {
                            node.transform(&placement);
                            moved = true;
                        }
                        booleans.push(node);
                    }
                    None => warnings.push(format!(
                        "{}: boolean geometry has an operand we cannot read yet (a half-space, or an unsupported solid)",
                        label()
                    )),
                }
                continue;
            }
            match geometry_face_loops_counted(file, g.id, scale) {
                Ok((mut fs, dropped)) => {
                    if !placement.is_identity() {
                        for f in &mut fs {
                            f.transform(&placement);
                            // The polygon moved but the analytic curve did not;
                            // fall back to the (transformed) polygon rather than
                            // place the curve wrong.
                            f.closed_curve = None;
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
        // IfcRelVoidsElement — cut this element's openings out of it. Both the
        // wall (already world, above) and each opening (placed by its own chain,
        // which runs through the wall) are in world space, so the synthesized
        // Subtract needs no further placement and is added after the transform.
        if let Some(opening_ids) = voids.get(&el.id) {
            let mut opening_solids: Vec<Vec<FaceLoops>> = Vec::new();
            for &op_id in opening_ids {
                let of = opening_world_faces(file, op_id, scale);
                if of.len() >= 4 {
                    opening_solids.push(of);
                } else {
                    warnings.push(format!(
                        "{}: opening #{} is not a buildable solid — hole not cut",
                        label(),
                        op_id
                    ));
                }
            }
            if !opening_solids.is_empty() {
                // The minuend is the element's own solid (faces) or, if its shape
                // was itself a boolean, that result. Fold each opening in as a
                // left-nested Subtract, which eval_csg walks like any CSG tree.
                let base: Option<CsgOperand> = if !faces.is_empty() {
                    Some(CsgOperand::Solid(std::mem::take(&mut faces)))
                } else if booleans.len() == 1 {
                    Some(CsgOperand::Node(Box::new(booleans.remove(0))))
                } else {
                    None
                };
                match base {
                    Some(base) => {
                        let mut acc = base;
                        for solid in opening_solids {
                            acc = CsgOperand::Node(Box::new(CsgNode {
                                op: BoolOp::Subtract,
                                first: acc,
                                second: CsgOperand::Solid(solid),
                            }));
                        }
                        if let CsgOperand::Node(n) = acc {
                            booleans.push(*n);
                        }
                    }
                    None => warnings.push(format!(
                        "{}: has openings but no single solid to cut them from — hole not cut",
                        label()
                    )),
                }
            }
        }
        if faces.is_empty() && booleans.is_empty() {
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
            booleans,
            fills_wall: fills.get(&el.id).copied(),
        });
    }
    GeometryImport { elements, spatial, scale_to_mm: scale, placed, warnings }
}

/// Face loops of one geometry item — a B-rep or a swept solid. Dispatches on
/// the entity tag so `from_file` does not care which representation a member
/// uses.
fn geometry_face_loops_counted(
    file: &StepFile,
    id: u32,
    scale: f64,
) -> Result<(Vec<FaceLoops>, usize), String> {
    let tag = file
        .entity(id)
        .map(|e| e.tag.to_ascii_uppercase())
        .unwrap_or_default();
    if tag == "IFCEXTRUDEDAREASOLID" {
        extruded_area_solid_loops(file, id, scale)
    } else if tag == "IFCREVOLVEDAREASOLID" {
        revolved_area_solid_loops(file, id, scale)
    } else if tag == "IFCSWEPTDISKSOLID" {
        swept_disk_solid_loops(file, id, scale)
    } else if tag == "IFCTRIANGULATEDFACESET" {
        triangulated_face_set_loops(file, id, scale)
    } else if tag == "IFCPOLYGONALFACESET" {
        polygonal_face_set_loops(file, id, scale)
    } else {
        brep_face_loops_counted(file, id, scale)
    }
}

/// A loop of world points from a `CoordIndex` list of 1-based indices into
/// `points`. `None` on an out-of-range or non-integer index.
fn loop_from_index_list(val: Option<&Value>, points: &[DVec3]) -> Option<Vec<DVec3>> {
    let idx = val?.as_list()?;
    let mut out = Vec::with_capacity(idx.len());
    for i in idx {
        let raw = i.as_f64()? as i64;
        if raw < 1 {
            return None;
        }
        out.push(*points.get((raw - 1) as usize)?);
    }
    Some(out)
}

/// An `IfcPolygonalFaceSet` — a mesh of arbitrary polygonal faces (the sibling of
/// `IfcTriangulatedFaceSet`, but each face is an N-gon that may carry holes). Each
/// `IfcIndexedPolygonalFace` becomes one `FaceLoops`, so a quad stays a quad
/// rather than two triangles. Faces we can't read (bad indices, < 3 vertices) are
/// counted, not silently dropped.
fn polygonal_face_set_loops(
    file: &StepFile,
    id: u32,
    scale: f64,
) -> Result<(Vec<FaceLoops>, usize), String> {
    let set = file.entity(id).ok_or_else(|| format!("#{} missing", id))?;
    // IfcPolygonalFaceSet(Coordinates, Closed, Faces, PnIndex).
    let coords_id = set
        .args
        .first()
        .and_then(|v| v.as_ref())
        .ok_or_else(|| format!("#{} has no coordinates", id))?;
    let points = cartesian_point_list_3d(file, coords_id, scale)
        .ok_or_else(|| format!("#{} coordinates are not an IfcCartesianPointList3D", id))?;
    let face_refs = set
        .args
        .get(2)
        .and_then(|v| v.as_list())
        .ok_or_else(|| format!("#{} has no faces", id))?;

    let mut faces = Vec::with_capacity(face_refs.len());
    let mut dropped = 0usize;
    for fref in face_refs {
        let face = match fref.as_ref().and_then(|f| file.entity(f)) {
            Some(f) => f,
            None => {
                dropped += 1;
                continue;
            }
        };
        let tag = face.tag.to_ascii_uppercase();
        if tag != "IFCINDEXEDPOLYGONALFACE" && tag != "IFCINDEXEDPOLYGONALFACEWITHVOIDS" {
            dropped += 1;
            continue;
        }
        // IfcIndexedPolygonalFace(CoordIndex) — the outer loop.
        let outer = match loop_from_index_list(face.args.first(), &points) {
            Some(o) if o.len() >= 3 => o,
            _ => {
                dropped += 1;
                continue;
            }
        };
        // IfcIndexedPolygonalFaceWithVoids adds InnerCoordIndices — the holes.
        let mut inners = Vec::new();
        if tag == "IFCINDEXEDPOLYGONALFACEWITHVOIDS" {
            if let Some(inner_lists) = face.args.get(1).and_then(|v| v.as_list()) {
                for il in inner_lists {
                    if let Some(hole) = loop_from_index_list(Some(il), &points) {
                        if hole.len() >= 3 {
                            inners.push(hole);
                        }
                    }
                }
            }
        }
        faces.push(FaceLoops { outer, inners, closed_curve: None });
    }
    Ok((faces, dropped))
}

/// The 3D points of an `IfcCartesianPointList3D` — an inline `((x,y,z), …)`
/// list rather than a list of `IfcCartesianPoint` refs, which is what the
/// tessellated formats use to keep the file small.
fn cartesian_point_list_3d(file: &StepFile, id: u32, scale: f64) -> Option<Vec<DVec3>> {
    let e = file.entity(id)?;
    if !e.tag.eq_ignore_ascii_case("IFCCARTESIANPOINTLIST3D") {
        return None;
    }
    let coords = e.args.first()?.as_list()?;
    let mut out = Vec::with_capacity(coords.len());
    for c in coords {
        let t = c.as_list()?;
        let x = t.first()?.as_f64()? * scale;
        let y = t.get(1)?.as_f64()? * scale;
        let z = t.get(2)?.as_f64()? * scale;
        out.push(DVec3::new(x, y, z));
    }
    Some(out)
}

/// An `IfcTriangulatedFaceSet` — a triangle mesh (SketchUp / Revit tessellated
/// exports). Each triangle becomes a three-vertex face; the shared-vertex dedup
/// welds them into one mesh and the render hides the coplanar diagonals, so a
/// tessellated quad reads as a quad. Degenerate triangles are counted, not
/// silently dropped.
fn triangulated_face_set_loops(
    file: &StepFile,
    id: u32,
    scale: f64,
) -> Result<(Vec<FaceLoops>, usize), String> {
    let set = file.entity(id).ok_or_else(|| format!("#{} missing", id))?;
    // IfcTriangulatedFaceSet(Coordinates, Normals, Closed, CoordIndex, PnIndex).
    let coords_id = set
        .args
        .first()
        .and_then(|v| v.as_ref())
        .ok_or_else(|| format!("#{} has no coordinates", id))?;
    let points = cartesian_point_list_3d(file, coords_id, scale)
        .ok_or_else(|| format!("#{} coordinates are not an IfcCartesianPointList3D", id))?;
    let tris = set
        .args
        .get(3)
        .and_then(|v| v.as_list())
        .ok_or_else(|| format!("#{} has no triangle index", id))?;

    let mut faces = Vec::with_capacity(tris.len());
    let mut dropped = 0usize;
    for tri in tris {
        let idx = match tri.as_list() {
            Some(i) if i.len() >= 3 => i,
            _ => {
                dropped += 1;
                continue;
            }
        };
        // 1-based indices into the coordinate list.
        let pick = |k: usize| -> Option<DVec3> {
            let raw = idx[k].as_f64()? as i64;
            if raw < 1 {
                return None;
            }
            points.get((raw - 1) as usize).copied()
        };
        match (pick(0), pick(1), pick(2)) {
            (Some(a), Some(b), Some(c))
                if (b - a).cross(c - a).length_squared() > 1e-12 =>
            {
                faces.push(FaceLoops { outer: vec![a, b, c], inners: vec![], closed_curve: None });
            }
            _ => dropped += 1, // out-of-range index or a zero-area sliver
        }
    }
    Ok((faces, dropped))
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

/// `IfcExtrudedAreaSolid` → the faces of the prism it sweeps.
///
/// This is the representation real BIM tools (Revit, ArchiCAD) use for almost
/// every wall, slab and column: a 2D profile placed in space and extruded a
/// depth along a direction. We read the profile, place it, and generate the two
/// caps and the side walls as ordinary face loops — so the rest of the importer
/// (the polygon path, spatial groups, re-export) treats it like any other
/// solid. A profile we cannot read yet, or a degenerate depth, drops the item
/// with a warning rather than inventing geometry.
fn extruded_area_solid_loops(
    file: &StepFile,
    id: u32,
    scale: f64,
) -> Result<(Vec<FaceLoops>, usize), String> {
    let solid = file.entity(id).ok_or_else(|| format!("#{} missing", id))?;
    // IfcExtrudedAreaSolid(SweptArea, Position, ExtrudedDirection, Depth)
    let area_id = solid
        .args
        .first()
        .and_then(|v| v.as_ref())
        .ok_or_else(|| format!("#{} has no swept area", id))?;
    let profile = match parse_profile(file, area_id, scale) {
        Some(p) if p.len() >= 3 => p,
        _ => return Ok((Vec::new(), 1)), // unsupported / degenerate profile
    };
    let depth = solid.args.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0) * scale;
    if depth.abs() <= 1e-9 {
        return Ok((Vec::new(), 1));
    }

    // The profile's 2D coordinates live in the swept solid's Position frame;
    // the extrusion runs along ExtrudedDirection expressed in that same frame.
    let place = solid
        .args
        .get(1)
        .and_then(|v| v.as_ref())
        .and_then(|pid| crate::ifc_placement::axis_placement(file, pid, scale))
        .unwrap_or_default();
    let dir_local = solid
        .args
        .get(2)
        .and_then(|v| v.as_ref())
        .and_then(|did| read_direction(file, did))
        .unwrap_or(DVec3::Z);
    let world_dir =
        (place.x * dir_local.x + place.y * dir_local.y + place.z * dir_local.z).normalize_or_zero();
    if world_dir.length_squared() < 0.5 {
        return Ok((Vec::new(), 1));
    }

    let to_3d = |u: f64, v: f64| place.origin + place.x * u + place.y * v;
    let base: Vec<DVec3> = profile.iter().map(|&(u, v)| to_3d(u, v)).collect();
    let top: Vec<DVec3> = base.iter().map(|&p| p + world_dir * depth).collect();
    let n = base.len();

    // Voids (IfcArbitraryProfileDefWithVoids) → through-holes: each becomes a hole
    // in both caps and a wall of its own, so a profile-with-a-void extrudes to a
    // genus-1 closed solid (a tube).
    let voids = parse_profile_voids(file, area_id, scale);
    let void_base: Vec<Vec<DVec3>> = voids
        .iter()
        .map(|loop_| loop_.iter().map(|&(u, v)| to_3d(u, v)).collect())
        .collect();
    let void_top: Vec<Vec<DVec3>> = void_base
        .iter()
        .map(|ring| ring.iter().map(|&p| p + world_dir * depth).collect())
        .collect();

    // Two caps (each carrying the voids as holes) + one quad per outer edge + one
    // quad per void edge. The engine normalizes winding to outward (ADR-007), so
    // consistent-but-not-necessarily-outward input is enough to form a closed solid.
    let wall_quads = n + void_base.iter().map(|r| r.len()).sum::<usize>();
    let mut faces = Vec::with_capacity(wall_quads + 2);
    faces.push(FaceLoops { outer: base.clone(), inners: void_base.clone(), closed_curve: None });
    faces.push(FaceLoops { outer: top.clone(), inners: void_top.clone(), closed_curve: None });
    for i in 0..n {
        let j = (i + 1) % n;
        faces.push(FaceLoops {
            outer: vec![base[i], base[j], top[j], top[i]],
            inners: vec![],
            closed_curve: None,
        });
    }
    // A wall around each void, wound opposite to the outer wall so its normal faces
    // into the hole; its edges coincide with the caps' hole loops (watertight) —
    // the same pattern the hollow swept disk uses for its inner wall.
    for (vb, vt) in void_base.iter().zip(&void_top) {
        let vn = vb.len();
        for i in 0..vn {
            let j = (i + 1) % vn;
            faces.push(FaceLoops {
                outer: vec![vb[i], vt[i], vt[j], vb[j]],
                inners: vec![],
                closed_curve: None,
            });
        }
    }
    Ok((faces, 0))
}

/// The factor from the file's plane-angle unit to radians. A conversion-based
/// unit (degrees) *wins* over the SI radian it references — the file assigns the
/// conversion unit, the SI radian is only its base. Defaults to radians.
fn plane_angle_scale_to_radians(file: &StepFile) -> f64 {
    for (_, ent) in file.iter_entities() {
        let is_angle = ent
            .args
            .get(1)
            .and_then(|v| v.as_enum())
            .map(|e| e.eq_ignore_ascii_case("PLANEANGLEUNIT"))
            .unwrap_or(false);
        if !is_angle {
            continue;
        }
        // A conversion-based plane-angle unit is degrees in practice.
        if ent.tag.eq_ignore_ascii_case("IFCCONVERSIONBASEDUNIT") {
            return std::f64::consts::PI / 180.0;
        }
    }
    1.0 // SI radian (the IFC default)
}

/// An `IfcAxis1Placement(Location, Axis)` → (point, unit direction). The axis
/// defaults to +Z when omitted.
fn read_axis1_placement(file: &StepFile, id: u32, scale: f64) -> Option<(DVec3, DVec3)> {
    let e = file.entity(id)?;
    if !e.tag.eq_ignore_ascii_case("IFCAXIS1PLACEMENT") {
        return None;
    }
    let loc = e.args.first().and_then(|v| v.as_ref()).and_then(|p| cartesian_point(file, p, scale))?;
    let dir = e
        .args
        .get(1)
        .and_then(|v| v.as_ref())
        .and_then(|d| read_direction(file, d))
        .unwrap_or(DVec3::Z)
        .normalize_or_zero();
    if dir.length_squared() < 0.5 {
        return None;
    }
    Some((loc, dir))
}

/// Rotate a point around an axis (Rodrigues) — the sweep of a revolved solid.
fn rotate_around_axis(p: DVec3, axis_pt: DVec3, axis_dir: DVec3, angle: f64) -> DVec3 {
    let v = p - axis_pt;
    let v_par = axis_dir * v.dot(axis_dir);
    let v_perp = v - v_par;
    axis_pt + v_par + v_perp * angle.cos() + axis_dir.cross(v_perp) * angle.sin()
}

/// An `IfcRevolvedAreaSolid(SweptArea, Position, Axis, Angle)` — a 2D profile
/// revolved around an axis. The profile is placed in its Position frame, then
/// swept in angular steps; consecutive rings are joined by quad side faces, and a
/// partial (< 360°) revolution is capped by the start and end profiles. A full
/// turn closes on itself with no caps.
fn revolved_area_solid_loops(
    file: &StepFile,
    id: u32,
    scale: f64,
) -> Result<(Vec<FaceLoops>, usize), String> {
    let solid = file.entity(id).ok_or_else(|| format!("#{} missing", id))?;
    let area_id = solid
        .args
        .first()
        .and_then(|v| v.as_ref())
        .ok_or_else(|| format!("#{} has no swept area", id))?;
    let profile = match parse_profile(file, area_id, scale) {
        Some(p) if p.len() >= 3 => p,
        _ => return Ok((Vec::new(), 1)),
    };
    let place = solid
        .args
        .get(1)
        .and_then(|v| v.as_ref())
        .and_then(|pid| crate::ifc_placement::axis_placement(file, pid, scale))
        .unwrap_or_default();
    let (axis_pt, axis_dir) = solid
        .args
        .get(2)
        .and_then(|v| v.as_ref())
        .and_then(|aid| read_axis1_placement(file, aid, scale))
        .ok_or_else(|| format!("#{} has no revolution axis", id))?;
    let angle = solid.args.get(3).and_then(|v| v.as_f64()).unwrap_or(0.0)
        * plane_angle_scale_to_radians(file);
    if angle.abs() <= 1e-6 {
        return Ok((Vec::new(), 1));
    }

    // The profile in world space, in its Position plane.
    let base: Vec<DVec3> =
        profile.iter().map(|&(u, v)| place.origin + place.x * u + place.y * v).collect();
    let n = base.len();

    // ~15° per step; a full turn closes on itself (last ring == first).
    let full = (angle.abs() - std::f64::consts::TAU).abs() < 1e-4;
    let steps = ((angle.abs() / (std::f64::consts::PI / 12.0)).ceil() as usize).max(2);
    let ring_count = if full { steps } else { steps + 1 };
    let rings: Vec<Vec<DVec3>> = (0..ring_count)
        .map(|i| {
            let a = angle * (i as f64) / (steps as f64);
            base.iter().map(|&p| rotate_around_axis(p, axis_pt, axis_dir, a)).collect()
        })
        .collect();

    let mut faces = Vec::with_capacity(steps * n + 2);
    for i in 0..steps {
        let r0 = &rings[i];
        let r1 = if full && i == steps - 1 { &rings[0] } else { &rings[i + 1] };
        for j in 0..n {
            let k = (j + 1) % n;
            faces.push(FaceLoops {
                outer: vec![r0[j], r0[k], r1[k], r1[j]],
                inners: vec![],
                closed_curve: None,
            });
        }
    }
    // A partial revolution is closed by the start and end profile caps.
    if !full {
        faces.push(FaceLoops { outer: rings[0].clone(), inners: vec![], closed_curve: None });
        faces.push(FaceLoops {
            outer: rings[ring_count - 1].clone(),
            inners: vec![],
            closed_curve: None,
        });
    }
    Ok((faces, 0))
}

/// The 3D points of an `IfcPolyline` directrix, consecutive duplicates dropped.
fn polyline_3d(file: &StepFile, id: u32, scale: f64) -> Option<Vec<DVec3>> {
    let e = file.entity(id)?;
    if !e.tag.eq_ignore_ascii_case("IFCPOLYLINE") {
        return None;
    }
    let pts = e.args.first()?.as_list()?;
    let mut out: Vec<DVec3> = Vec::with_capacity(pts.len());
    for pv in pts {
        let p = cartesian_point(file, pv.as_ref()?, scale)?;
        if out.last().map_or(true, |&last| (last - p).length_squared() > 1e-12) {
            out.push(p);
        }
    }
    if out.len() >= 2 {
        Some(out)
    } else {
        None
    }
}

/// The cartesian point of an `IfcTrimmedCurve` trim, if it carries one.
fn trim_point(file: &StepFile, trim: &Value, scale: f64) -> Option<DVec3> {
    trim.as_list()?
        .iter()
        .find_map(|it| it.as_ref().and_then(|id| cartesian_point(file, id, scale)))
}

/// The vector of an `IfcVector` — its direction scaled by its magnitude — in mm.
fn read_vector(file: &StepFile, id: u32, scale: f64) -> Option<DVec3> {
    let e = file.entity(id)?;
    if !e.tag.eq_ignore_ascii_case("IFCVECTOR") {
        return None;
    }
    // (Orientation: IfcDirection, Magnitude: length).
    let dir = e
        .args
        .first()
        .and_then(|v| v.as_ref())
        .and_then(|d| read_direction(file, d))?
        .normalize_or_zero();
    if dir.length_squared() < 1e-18 {
        return None;
    }
    let mag = e.args.get(1).and_then(|v| v.as_f64()).unwrap_or(1.0) * scale;
    Some(dir * mag)
}

/// A parameter value out of a trim list — a bare number or an `IfcParameterValue`.
/// (The cartesian-point form is handled by `trim_point`.)
fn trim_param(trim: &Value) -> Option<f64> {
    trim.as_list()?.iter().find_map(|it| match it {
        Value::Float(f) => Some(*f),
        Value::Int(i) => Some(*i as f64),
        Value::Typed { tag, args } if tag.eq_ignore_ascii_case("IFCPARAMETERVALUE") => {
            args.first().and_then(|v| v.as_f64())
        }
        _ => None,
    })
}

/// A 2D cartesian point list read into the XY plane (z = 0), in mm. `IfcIndexedPolyCurve`
/// may carry its points as 2D.
fn cartesian_point_list_2d(file: &StepFile, id: u32, scale: f64) -> Option<Vec<DVec3>> {
    let e = file.entity(id)?;
    if !e.tag.eq_ignore_ascii_case("IFCCARTESIANPOINTLIST2D") {
        return None;
    }
    let coords = e.args.first()?.as_list()?;
    let mut out = Vec::with_capacity(coords.len());
    for c in coords {
        let t = c.as_list()?;
        let x = t.first()?.as_f64()? * scale;
        let y = t.get(1)?.as_f64()? * scale;
        out.push(DVec3::new(x, y, 0.0));
    }
    Some(out)
}

/// A B-spline / NURBS directrix sampled to a polyline — the whole run, at a chord
/// tolerance coarse enough to keep a metre-scale pipe path from exploding into
/// hundreds of rings.
fn sample_spline_directrix(file: &StepFile, curve: &Entity, scale: f64) -> Option<Vec<DVec3>> {
    use axia_geo::curves::{bspline, nurbs};
    let rational = curve.tag.eq_ignore_ascii_case("IFCRATIONALBSPLINECURVEWITHKNOTS");
    if !rational && !curve.tag.eq_ignore_ascii_case("IFCBSPLINECURVEWITHKNOTS") {
        return None;
    }
    let (control_pts, knots, degree, weights) = parse_bspline(file, curve, scale)?;
    let full = if let Some(w) = weights.filter(|_| rational) {
        nurbs::tessellate(&control_pts, &w, &knots, degree, DIRECTRIX_SPLINE_TOL_MM).ok()?
    } else {
        bspline::tessellate(&control_pts, &knots, degree, DIRECTRIX_SPLINE_TOL_MM).ok()?
    };
    if full.len() < 2 {
        return None;
    }
    Some(decimate_polyline(full, 64))
}

/// Keep a polyline's endpoints but thin its interior to at most `max` points.
fn decimate_polyline(pts: Vec<DVec3>, max: usize) -> Vec<DVec3> {
    if pts.len() <= max || max < 2 {
        return pts;
    }
    let last = pts.len() - 1;
    (0..max).map(|i| pts[i * last / (max - 1)]).collect()
}

/// The circular arc through three points, sampled at ~15°/step from `a` to `b`
/// the way that passes through `m`. Collinear points fall back to the straight
/// chain a→m→b.
fn sample_arc_3pt(a: DVec3, m: DVec3, b: DVec3) -> Vec<DVec3> {
    let u = m - a;
    let v = b - a;
    let n = u.cross(v);
    let n2 = n.length_squared();
    if n2 < 1e-18 {
        return vec![a, m, b];
    }
    // Circumcenter: c = a + ((|u|²v − |v|²u) × (u×v)) / (2|u×v|²).
    let center = a + (v * u.length_squared() - u * v.length_squared()).cross(n) / (2.0 * n2);
    let radius = (a - center).length();
    if !(radius > 1e-9) {
        return vec![a, m, b];
    }
    let ex = (a - center).normalize();
    let ey = n.normalize().cross(ex);
    let angle_of = |p: DVec3| {
        let d = p - center;
        d.dot(ey).atan2(d.dot(ex))
    };
    let two_pi = std::f64::consts::TAU;
    let wrap = |x: f64| ((x % two_pi) + two_pi) % two_pi;
    let am = wrap(angle_of(m)); // `a` sits at angle 0 by construction
    let ab = wrap(angle_of(b));
    // Sweep 0→ab passing through the mid: CCW if the mid is between, else CW.
    let sweep = if am > 1e-9 && am < ab { ab } else { ab - two_pi };
    let segments = ((sweep.abs() / (std::f64::consts::PI / 12.0)).ceil() as usize).clamp(2, 64);
    (0..=segments)
        .map(|i| {
            let t = sweep * (i as f64) / (segments as f64);
            center + ex * (radius * t.cos()) + ey * (radius * t.sin())
        })
        .collect()
}

/// An `IfcIndexedPolyCurve` directrix — a shared point list indexed by straight
/// (`IfcLineIndex`) and 3-point-arc (`IfcArcIndex`) segments. With no segments it
/// is a plain polyline through every point.
fn sample_indexed_polycurve(file: &StepFile, pc: &Entity, scale: f64) -> Option<Vec<DVec3>> {
    // (Points, Segments, SelfIntersect).
    let pts_id = pc.args.first().and_then(|v| v.as_ref())?;
    let points = cartesian_point_list_3d(file, pts_id, scale)
        .or_else(|| cartesian_point_list_2d(file, pts_id, scale))?;
    let at = |i: f64| -> Option<DVec3> {
        let idx = i as i64;
        (idx >= 1).then(|| points.get((idx - 1) as usize).copied()).flatten()
    };
    let push = |out: &mut Vec<DVec3>, p: DVec3| {
        if out.last().map(|q| (*q - p).length_squared() > 1e-12).unwrap_or(true) {
            out.push(p);
        }
    };

    match pc.args.get(1).and_then(|v| v.as_list()) {
        // No segments — the point list is the polyline.
        None => (points.len() >= 2).then_some(points),
        Some(segs) if segs.is_empty() => (points.len() >= 2).then_some(points),
        Some(segs) => {
            let mut out: Vec<DVec3> = Vec::new();
            for sv in segs {
                if let Value::Typed { tag, args } = sv {
                    let idx = args.first().and_then(|v| v.as_list())?;
                    if tag.eq_ignore_ascii_case("IFCARCINDEX") {
                        let a = at(idx.first()?.as_f64()?)?;
                        let m = at(idx.get(1)?.as_f64()?)?;
                        let b = at(idx.get(2)?.as_f64()?)?;
                        for p in sample_arc_3pt(a, m, b) {
                            push(&mut out, p);
                        }
                    } else {
                        // IfcLineIndex (or any straight run) — its points, in order.
                        for iv in idx {
                            push(&mut out, at(iv.as_f64()?)?);
                        }
                    }
                }
            }
            (out.len() >= 2).then_some(out)
        }
    }
}

/// A directrix sampled to a 3D polyline — the sweep only ever needs points, so an
/// arc, a line, or a chain of them collapses to the same thing a polyline does.
/// Handles `IfcPolyline`, `IfcTrimmedCurve` (a circle arc or a line), and
/// `IfcCompositeCurve` (the standard pipe path — straight runs joined by bends).
fn sample_directrix(file: &StepFile, id: u32, scale: f64) -> Option<Vec<DVec3>> {
    let e = file.entity(id)?;
    match e.tag.to_ascii_uppercase().as_str() {
        "IFCPOLYLINE" => polyline_3d(file, id, scale),
        "IFCTRIMMEDCURVE" => sample_trimmed_curve(file, e, scale),
        "IFCCOMPOSITECURVE" => sample_composite_curve(file, e, scale),
        "IFCINDEXEDPOLYCURVE" => sample_indexed_polycurve(file, e, scale),
        "IFCBSPLINECURVEWITHKNOTS" | "IFCRATIONALBSPLINECURVEWITHKNOTS" => {
            sample_spline_directrix(file, e, scale)
        }
        _ => None,
    }
}

/// An `IfcTrimmedCurve` directrix — a circle arc (sampled at the chord tolerance,
/// both endpoints included) or a straight line between its trim points.
fn sample_trimmed_curve(file: &StepFile, trimmed: &Entity, scale: f64) -> Option<Vec<DVec3>> {
    // (BasisCurve, Trim1, Trim2, SenseAgreement, MasterRepresentation)
    let basis = trimmed.args.first().and_then(|v| v.as_ref()).and_then(|b| file.entity(b))?;
    let t1 = trimmed.args.get(1)?;
    let t2 = trimmed.args.get(2)?;
    let sense = trimmed.args.get(3).and_then(|v| v.as_enum()).map(|s| s != "F").unwrap_or(true);
    let btag = basis.tag.to_ascii_uppercase();

    if btag == "IFCCIRCLE" {
        // IfcCircle(Position, Radius) — sweep from Trim1's angle to Trim2's.
        let pos = basis.args.first().and_then(|v| v.as_ref())?;
        let place = crate::ifc_placement::axis_placement(file, pos, scale)?;
        let radius = basis.args.get(1).and_then(|v| v.as_f64())? * scale;
        if !(radius > 0.0) {
            return None;
        }
        let a0 = trim_angle(file, t1, &place, scale)?;
        let a1 = trim_angle(file, t2, &place, scale)?;
        let mut sweep = a1 - a0;
        if sense {
            while sweep <= 1e-9 {
                sweep += std::f64::consts::TAU;
            }
        } else {
            while sweep >= -1e-9 {
                sweep -= std::f64::consts::TAU;
            }
        }
        // ~15° per step — a pipe path needs angular resolution, not the sub-mm
        // chord tolerance a rendered boundary curve uses (which would put hundreds
        // of rings on a metre-scale elbow).
        let segments =
            ((sweep.abs() / (std::f64::consts::PI / 12.0)).ceil() as usize).clamp(2, 64);
        let point_at =
            |a: f64| place.origin + place.x * (radius * a.cos()) + place.y * (radius * a.sin());
        // Both endpoints — a directrix carries its whole run, not just the interior.
        Some((0..=segments).map(|i| point_at(a0 + sweep * (i as f64) / (segments as f64))).collect())
    } else if btag == "IFCLINE" {
        // A straight run. Its trim can be the two endpoints (the common CARTESIAN
        // form) or a pair of parameters along Pnt + u·Dir.
        if let (Some(a), Some(b)) = (trim_point(file, t1, scale), trim_point(file, t2, scale)) {
            return Some(vec![a, b]);
        }
        // IfcLine(Pnt, Dir: IfcVector) — the point at parameter u is Pnt + u·Dir.
        let pnt = basis
            .args
            .first()
            .and_then(|v| v.as_ref())
            .and_then(|p| cartesian_point(file, p, scale))?;
        let dir = basis
            .args
            .get(1)
            .and_then(|v| v.as_ref())
            .and_then(|d| read_vector(file, d, scale))?;
        let u0 = trim_param(t1)?;
        let u1 = trim_param(t2)?;
        Some(vec![pnt + dir * u0, pnt + dir * u1])
    } else if btag == "IFCBSPLINECURVEWITHKNOTS" || btag == "IFCRATIONALBSPLINECURVEWITHKNOTS" {
        // A trimmed spline directrix — the sweep needs the whole run, and pipe
        // splines are in practice authored to their full parameter range, so we
        // sample the basis rather than honour the (usually full-range) trims.
        sample_spline_directrix(file, basis, scale)
    } else {
        None // a basis curve we do not sample yet
    }
}

/// An `IfcCompositeCurve` directrix — its segments' parent curves sampled and
/// concatenated, each reversed when the segment runs against its parent, with the
/// shared junction points de-duplicated.
fn sample_composite_curve(file: &StepFile, cc: &Entity, scale: f64) -> Option<Vec<DVec3>> {
    let segments = cc.args.first()?.as_list()?;
    let mut out: Vec<DVec3> = Vec::new();
    for sv in segments {
        let Some(seg) = sv.as_ref().and_then(|s| file.entity(s)) else { continue };
        if !seg.tag.eq_ignore_ascii_case("IFCCOMPOSITECURVESEGMENT") {
            continue;
        }
        // IfcCompositeCurveSegment(Transition, SameSense, ParentCurve)
        let same_sense = seg.args.get(1).and_then(|v| v.as_enum()).map(|s| s != "F").unwrap_or(true);
        let Some(parent) = seg.args.get(2).and_then(|v| v.as_ref()) else { continue };
        let Some(mut pts) = sample_directrix(file, parent, scale) else { continue };
        if !same_sense {
            pts.reverse();
        }
        for p in pts {
            if out.last().map_or(true, |&last| (last - p).length_squared() > 1e-12) {
                out.push(p);
            }
        }
    }
    if out.len() >= 2 {
        Some(out)
    } else {
        None
    }
}

/// A unit vector perpendicular to `t` — the seed of a sweep cross-section frame.
fn perpendicular(t: DVec3) -> DVec3 {
    let seed = if t.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
    (seed - t * seed.dot(t)).normalize_or_zero()
}

/// An `IfcSweptDiskSolid(Directrix, Radius, InnerRadius, StartParam, EndParam)` —
/// a circular disk (a pipe cross-section, hollow when InnerRadius is set) swept
/// along a curve. The directrix is sampled to a polyline, a twist-minimizing frame
/// is carried along it, and a ring of the cross-section is placed at each point;
/// consecutive rings are joined by quads and an open pipe is capped at both ends.
/// Only a polyline directrix is read so far (the common case for pipes and rails).
fn swept_disk_solid_loops(
    file: &StepFile,
    id: u32,
    scale: f64,
) -> Result<(Vec<FaceLoops>, usize), String> {
    let solid = file.entity(id).ok_or_else(|| format!("#{} missing", id))?;
    let directrix_id = solid
        .args
        .first()
        .and_then(|v| v.as_ref())
        .ok_or_else(|| format!("#{} has no directrix", id))?;
    let path = sample_directrix(file, directrix_id, scale)
        .ok_or_else(|| format!("#{} directrix is a curve we cannot sweep yet", id))?;
    let radius = solid.args.get(1).and_then(|v| v.as_f64()).unwrap_or(0.0) * scale;
    if !(radius > 0.0) {
        return Ok((Vec::new(), 1));
    }
    let inner = solid
        .args
        .get(2)
        .and_then(|v| v.as_f64())
        .map(|r| r * scale)
        .filter(|&r| r > 1e-9 && r < radius);

    let m = path.len();
    // Tangents (central difference in the interior, one-sided at the ends).
    let tangents: Vec<DVec3> = (0..m)
        .map(|i| {
            let t = if i == 0 {
                path[1] - path[0]
            } else if i == m - 1 {
                path[m - 1] - path[m - 2]
            } else {
                path[i + 1] - path[i - 1]
            };
            t.normalize_or_zero()
        })
        .collect();
    // A rotation-minimizing frame by projecting the previous normal onto each new
    // cross-section plane — no twist on a straight run, gentle on bends.
    let mut u = perpendicular(tangents[0]);
    let mut frames: Vec<(DVec3, DVec3)> = Vec::with_capacity(m);
    for &t in &tangents {
        u = (u - t * u.dot(t)).normalize_or_zero();
        if u.length_squared() < 0.5 {
            u = perpendicular(t);
        }
        let v = t.cross(u).normalize_or_zero();
        frames.push((u, v));
    }

    const N: usize = 16; // cross-section segments
    let ring = |c: DVec3, (u, v): (DVec3, DVec3), r: f64| -> Vec<DVec3> {
        (0..N)
            .map(|j| {
                let a = std::f64::consts::TAU * (j as f64) / (N as f64);
                c + u * (r * a.cos()) + v * (r * a.sin())
            })
            .collect()
    };
    let outer: Vec<Vec<DVec3>> = (0..m).map(|i| ring(path[i], frames[i], radius)).collect();
    let inner_rings: Option<Vec<Vec<DVec3>>> =
        inner.map(|ir| (0..m).map(|i| ring(path[i], frames[i], ir)).collect());

    let mut faces = Vec::new();
    // Outer wall.
    for i in 0..m - 1 {
        for j in 0..N {
            let k = (j + 1) % N;
            faces.push(FaceLoops {
                outer: vec![outer[i][j], outer[i][k], outer[i + 1][k], outer[i + 1][j]],
                inners: vec![],
                closed_curve: None,
            });
        }
    }
    if let Some(inner_rings) = &inner_rings {
        // Inner wall (wound the other way; the kernel normalizes it outward).
        for i in 0..m - 1 {
            for j in 0..N {
                let k = (j + 1) % N;
                faces.push(FaceLoops {
                    outer: vec![
                        inner_rings[i][j],
                        inner_rings[i + 1][j],
                        inner_rings[i + 1][k],
                        inner_rings[i][k],
                    ],
                    inners: vec![],
                    closed_curve: None,
                });
            }
        }
        // Annular end caps — the outer ring with the inner ring as a hole.
        faces.push(FaceLoops {
            outer: outer[0].clone(),
            inners: vec![inner_rings[0].clone()],
            closed_curve: None,
        });
        faces.push(FaceLoops {
            outer: outer[m - 1].clone(),
            inners: vec![inner_rings[m - 1].clone()],
            closed_curve: None,
        });
    } else {
        // Solid disk end caps.
        faces.push(FaceLoops { outer: outer[0].clone(), inners: vec![], closed_curve: None });
        faces.push(FaceLoops { outer: outer[m - 1].clone(), inners: vec![], closed_curve: None });
    }
    Ok((faces, 0))
}

/// `wall #N → [opening #M, …]` from every `IfcRelVoidsElement`. A door or window
/// opening is usually not baked into the wall's own shape — it is a separate
/// `IfcOpeningElement` tied to the wall by this relationship, and the wall only
/// gets its hole once the opening is subtracted.
fn collect_voids(file: &StepFile) -> std::collections::HashMap<u32, Vec<u32>> {
    let mut voids: std::collections::HashMap<u32, Vec<u32>> = std::collections::HashMap::new();
    for (_, e) in file.iter_entities() {
        if !e.tag.eq_ignore_ascii_case("IFCRELVOIDSELEMENT") {
            continue;
        }
        // (GlobalId, OwnerHistory, Name, Description, RelatingBuildingElement,
        //  RelatedOpeningElement)
        let wall = e.args.get(4).and_then(|v| v.as_ref());
        let opening = e.args.get(5).and_then(|v| v.as_ref());
        if let (Some(w), Some(o)) = (wall, opening) {
            voids.entry(w).or_default().push(o);
        }
    }
    voids
}

/// `filler #N → wall #M` — the door or window that fills an opening, and the
/// wall that opening voids. Composes `IfcRelFillsElement` (opening → filler)
/// with `IfcRelVoidsElement` (opening → wall), so a window imported as its own
/// member can be grouped under the wall it belongs to.
fn collect_fills(file: &StepFile) -> std::collections::HashMap<u32, u32> {
    // opening → wall, from the void relationships.
    let mut opening_wall: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for (_, e) in file.iter_entities() {
        if e.tag.eq_ignore_ascii_case("IFCRELVOIDSELEMENT") {
            let wall = e.args.get(4).and_then(|v| v.as_ref());
            let opening = e.args.get(5).and_then(|v| v.as_ref());
            if let (Some(w), Some(o)) = (wall, opening) {
                opening_wall.insert(o, w);
            }
        }
    }
    let mut fills: std::collections::HashMap<u32, u32> = std::collections::HashMap::new();
    for (_, e) in file.iter_entities() {
        if !e.tag.eq_ignore_ascii_case("IFCRELFILLSELEMENT") {
            continue;
        }
        // (GlobalId, OwnerHistory, Name, Description, RelatingOpeningElement,
        //  RelatedBuildingElement)
        let opening = e.args.get(4).and_then(|v| v.as_ref());
        let filler = e.args.get(5).and_then(|v| v.as_ref());
        if let (Some(o), Some(f)) = (opening, filler) {
            if let Some(&w) = opening_wall.get(&o) {
                if w != f {
                    fills.insert(f, w);
                }
            }
        }
    }
    fills
}

/// The world-space solid of an `IfcOpeningElement` — its representation items
/// built and placed by its own placement chain (which runs through the wall, so
/// the hole lands where the file put it). Empty if it isn't a buildable solid.
fn opening_world_faces(file: &StepFile, opening_id: u32, scale: f64) -> Vec<FaceLoops> {
    let mut out = Vec::new();
    let Some(op) = file.entity(opening_id) else { return out };
    // IfcOpeningElement: 5 ObjectPlacement, 6 Representation (an IfcElement).
    let placement = op
        .args
        .get(5)
        .and_then(|v| v.as_ref())
        .map(|pid| crate::ifc_placement::resolve_placement(file, pid, scale))
        .unwrap_or_default();
    let Some(shape) = op.args.get(6).and_then(|v| v.as_ref()).and_then(|s| file.entity(s)) else {
        return out;
    };
    if !shape.tag.eq_ignore_ascii_case("IFCPRODUCTDEFINITIONSHAPE") {
        return out;
    }
    let Some(reps) = shape.args.get(2).and_then(|v| v.as_list()) else { return out };
    for rep_val in reps {
        let Some(rep) = rep_val.as_ref().and_then(|r| file.entity(r)) else { continue };
        if !rep.tag.eq_ignore_ascii_case("IFCSHAPEREPRESENTATION") {
            continue;
        }
        let Some(items) = rep.args.get(3).and_then(|v| v.as_list()) else { continue };
        for item_val in items {
            let Some(item_id) = item_val.as_ref() else { continue };
            if let Ok((mut fs, _)) = geometry_face_loops_counted(file, item_id, scale) {
                for f in &mut fs {
                    f.transform(&placement);
                    f.closed_curve = None;
                }
                out.append(&mut fs);
            }
        }
    }
    out
}

/// An `IfcBooleanResult` / `IfcBooleanClippingResult` → a CSG tree. `None` when
/// an operand is a half-space or a solid we cannot build, so the caller reports
/// the whole member as unreadable rather than importing a wrong shape.
fn parse_boolean(file: &StepFile, id: u32, scale: f64) -> Option<CsgNode> {
    let e = file.entity(id)?;
    let tag = e.tag.to_ascii_uppercase();
    if tag != "IFCBOOLEANRESULT" && tag != "IFCBOOLEANCLIPPINGRESULT" {
        return None;
    }
    // (Operator, FirstOperand, SecondOperand)
    let op = match e.args.first()?.as_enum()?.to_ascii_uppercase().as_str() {
        "UNION" => BoolOp::Union,
        "DIFFERENCE" => BoolOp::Subtract,
        "INTERSECTION" => BoolOp::Intersect,
        _ => return None,
    };
    let first = parse_boolean_operand(file, e.args.get(1).and_then(|v| v.as_ref())?, scale)?;
    let second = parse_boolean_operand(file, e.args.get(2).and_then(|v| v.as_ref())?, scale)?;
    Some(CsgNode { op, first, second })
}

/// One boolean operand: a nested result, a half-space clip, or a solid (extruded
/// / brep). Anything that is neither buildable nor a recognized half-space —
/// including a solid whose faces don't close — returns `None`.
fn parse_boolean_operand(file: &StepFile, id: u32, scale: f64) -> Option<CsgOperand> {
    let tag = file.entity(id)?.tag.to_ascii_uppercase();
    if tag == "IFCBOOLEANRESULT" || tag == "IFCBOOLEANCLIPPINGRESULT" {
        return Some(CsgOperand::Node(Box::new(parse_boolean(file, id, scale)?)));
    }
    if tag == "IFCHALFSPACESOLID" || tag == "IFCPOLYGONALBOUNDEDHALFSPACE" {
        return parse_half_space(file, id, scale).map(CsgOperand::HalfSpace);
    }
    let (loops, _) = geometry_face_loops_counted(file, id, scale).ok()?;
    if loops.len() < 4 {
        return None; // not a closed solid — degenerate
    }
    Some(CsgOperand::Solid(loops))
}

/// An `IfcHalfSpaceSolid(BaseSurface, AgreementFlag)` or its polygonally-bounded
/// subtype `IfcPolygonalBoundedHalfSpace(BaseSurface, AgreementFlag, Position,
/// PolygonalBoundary)`. The base surface must be a planar `IfcPlane`; a curved
/// base or a missing polygon returns `None` (reported, never guessed).
fn parse_half_space(file: &StepFile, id: u32, scale: f64) -> Option<HalfSpace> {
    let e = file.entity(id)?;
    let tag = e.tag.to_ascii_uppercase();

    // BaseSurface = IfcPlane(Position) — the plane's Position gives origin + normal.
    let plane = file.entity(e.args.first().and_then(|v| v.as_ref())?)?;
    if plane.tag.to_ascii_uppercase() != "IFCPLANE" {
        return None; // a curved base surface — not supported yet
    }
    let place = crate::ifc_placement::axis_placement(
        file,
        plane.args.first().and_then(|v| v.as_ref())?,
        scale,
    )?;
    let base_origin = place.origin;
    let base_normal = place.z.normalize_or_zero();
    if base_normal.length_squared() < 0.5 {
        return None;
    }
    let agreement = e.args.get(1).and_then(|v| v.as_enum()).map_or(false, |s| {
        s.eq_ignore_ascii_case("T") || s.eq_ignore_ascii_case("TRUE")
    });

    let boundary = if tag == "IFCPOLYGONALBOUNDEDHALFSPACE" {
        // Position places the boundary; PolygonalBoundary is a polyline in its XY.
        let bplace = crate::ifc_placement::axis_placement(
            file,
            e.args.get(2).and_then(|v| v.as_ref())?,
            scale,
        )?;
        let poly2d = polyline_2d(file, e.args.get(3).and_then(|v| v.as_ref())?, scale)?;
        if poly2d.len() < 3 {
            return None;
        }
        let world: Vec<DVec3> = poly2d
            .iter()
            .map(|&(u, v)| bplace.origin + bplace.x * u + bplace.y * v)
            .collect();
        Some((world, bplace.z.normalize_or_zero()))
    } else {
        None
    };

    Some(HalfSpace { base_origin, base_normal, agreement, boundary })
}

/// A profile's outer boundary as 2D points in its own plane (engine units).
/// Handles the shapes real files lean on: a rectangle, a circle (tessellated),
/// and an arbitrary closed polyline. `None` for a profile we do not read yet
/// (with voids, an I-beam, a composite curve) so the caller reports it.
/// A parameterized structural section (I/H, L, U/channel, T) as a closed CCW
/// polygon in the profile's own frame, before its `Position` is applied. Sharp
/// corners — fillet and edge radii are read past but not rounded: they leave the
/// section extent unchanged (the outline area is a hair over the true one), and
/// the bounding box (what verification checks) is identical.
///
/// The origin is the **centre of the section's bounding box**, which is where IFC
/// (and the web-ifc reference kernel) places these — cross-checked against it. So
/// the I is symmetric about both axes, the T/U about one, and the L's outer corner
/// sits at (−w/2, −h/2). (IFC reports the centre of gravity as a separate derived
/// attribute, precisely because the origin is geometric, not the centroid.)
fn parametric_section_local(tag: &str, p: &Entity, scale: f64) -> Option<Vec<(f64, f64)>> {
    let f = |i: usize| p.args.get(i).and_then(|v| v.as_f64()).map(|x| x * scale);
    match tag {
        // (…, OverallWidth[3], OverallDepth[4], WebThickness[5], FlangeThickness[6], …)
        "IFCISHAPEPROFILEDEF" => {
            let (b, h, tw, tf) = (f(3)?, f(4)?, f(5)?, f(6)?);
            if !(b > 0.0 && h > 0.0 && tw > 0.0 && tf > 0.0) || tw >= b || 2.0 * tf >= h {
                return None;
            }
            let (hb, hh, hw) = (b / 2.0, h / 2.0, tw / 2.0);
            Some(vec![
                (-hb, -hh), (hb, -hh), (hb, -hh + tf), (hw, -hh + tf),
                (hw, hh - tf), (hb, hh - tf), (hb, hh), (-hb, hh),
                (-hb, hh - tf), (-hw, hh - tf), (-hw, -hh + tf), (-hb, -hh + tf),
            ])
        }
        // (…, Depth[3], FlangeWidth[4], WebThickness[5], FlangeThickness[6], …)
        // Flange on top, web hanging down; symmetric about the vertical axis,
        // bounding box centred so y ∈ [−h/2, h/2].
        "IFCTSHAPEPROFILEDEF" => {
            let (h, bf, tw, tf) = (f(3)?, f(4)?, f(5)?, f(6)?);
            if !(h > 0.0 && bf > 0.0 && tw > 0.0 && tf > 0.0) || tw >= bf || tf >= h {
                return None;
            }
            let (hh, hw, hbf) = (h / 2.0, tw / 2.0, bf / 2.0);
            Some(vec![
                (-hw, -hh), (hw, -hh), (hw, hh - tf), (hbf, hh - tf),
                (hbf, hh), (-hbf, hh), (-hbf, hh - tf), (-hw, hh - tf),
            ])
        }
        // (…, Depth[3], FlangeWidth[4], WebThickness[5], FlangeThickness[6], …)
        // A channel: web on the left, two flanges out to the right; bounding box
        // centred so x ∈ [−bf/2, bf/2] and y ∈ [−h/2, h/2].
        "IFCUSHAPEPROFILEDEF" => {
            let (h, bf, tw, tf) = (f(3)?, f(4)?, f(5)?, f(6)?);
            if !(h > 0.0 && bf > 0.0 && tw > 0.0 && tf > 0.0) || tw >= bf || 2.0 * tf >= h {
                return None;
            }
            let (hb, hh, ht, xw) = (bf / 2.0, h / 2.0, h / 2.0 - tf, -bf / 2.0 + tw);
            Some(vec![
                (-hb, -hh), (hb, -hh), (hb, -ht), (xw, -ht),
                (xw, ht), (hb, ht), (hb, hh), (-hb, hh),
            ])
        }
        // (…, Depth[3], Width[4]?, Thickness[5], …) — Width null ⇒ equal legs.
        // Bounding box centred: outer corner at (−w/2, −h/2), legs up and right.
        "IFCLSHAPEPROFILEDEF" => {
            let h = f(3)?;
            let w = f(4).unwrap_or(h);
            let t = f(5)?;
            if !(h > 0.0 && w > 0.0 && t > 0.0) || t >= h || t >= w {
                return None;
            }
            let (x0, y0) = (-w / 2.0, -h / 2.0);
            Some(vec![
                (x0, y0), (x0 + w, y0), (x0 + w, y0 + t), (x0 + t, y0 + t),
                (x0 + t, y0 + h), (x0, y0 + h),
            ])
        }
        // (…, Depth[3], Width[4], WallThickness[5], Girth[6], …) — a lipped channel:
        // web on the left, flanges to the right, tips turned back in by the girth.
        // Symmetric about X, bounding box centred.
        "IFCCSHAPEPROFILEDEF" => {
            let (h, w, t, g) = (f(3)?, f(4)?, f(5)?, f(6)?);
            if !(h > 0.0 && w > 0.0 && t > 0.0 && g > 0.0) || 2.0 * t >= h || t >= w || g >= h / 2.0
                || g <= t
            {
                return None;
            }
            let (hw, hh) = (w / 2.0, h / 2.0);
            Some(vec![
                (-hw, -hh), (hw, -hh), (hw, -hh + g), (hw - t, -hh + g),
                (hw - t, -hh + t), (-hw + t, -hh + t), (-hw + t, hh - t), (hw - t, hh - t),
                (hw - t, hh - g), (hw, hh - g), (hw, hh), (-hw, hh),
            ])
        }
        // (…, Depth[3], FlangeWidth[4], WebThickness[5], FlangeThickness[6], …) — a Z:
        // web centred, top flange to +x, bottom flange to −x. Point-symmetric about
        // the origin. FlangeWidth is the section's overall width (tip to tip), matched
        // against web-ifc, so x ∈ [−bf/2, bf/2].
        "IFCZSHAPEPROFILEDEF" => {
            let (h, bf, tw, tf) = (f(3)?, f(4)?, f(5)?, f(6)?);
            if !(h > 0.0 && bf > 0.0 && tw > 0.0 && tf > 0.0) || 2.0 * tf >= h || bf <= tw {
                return None;
            }
            let (hh, hw) = (h / 2.0, tw / 2.0);
            let (xr, xl) = (bf / 2.0, -bf / 2.0); // top-flange right edge, bottom-flange left edge
            Some(vec![
                (xl, -hh), (hw, -hh), (hw, hh - tf), (xr, hh - tf),
                (xr, hh), (-hw, hh), (-hw, -hh + tf), (xl, -hh + tf),
            ])
        }
        // (…, BottomFlangeWidth[3], OverallDepth[4], WebThickness[5],
        //  BottomFlangeThickness[6], BottomFlangeFilletRadius[7]?, TopFlangeWidth[8],
        //  TopFlangeThickness[9]?, …) — an I with unequal flanges, symmetric about Y.
        "IFCASYMMETRICISHAPEPROFILEDEF" => {
            let (bfw, h, tw, btf) = (f(3)?, f(4)?, f(5)?, f(6)?);
            let tfw = f(8)?;
            let ttf = f(9).unwrap_or(btf);
            if !(bfw > 0.0 && h > 0.0 && tw > 0.0 && btf > 0.0 && tfw > 0.0 && ttf > 0.0)
                || tw >= bfw || tw >= tfw || btf + ttf >= h
            {
                return None;
            }
            let (hh, hw, hbf, htf) = (h / 2.0, tw / 2.0, bfw / 2.0, tfw / 2.0);
            Some(vec![
                (-hbf, -hh), (hbf, -hh), (hbf, -hh + btf), (hw, -hh + btf),
                (hw, hh - ttf), (htf, hh - ttf), (htf, hh), (-htf, hh),
                (-htf, hh - ttf), (-hw, hh - ttf), (-hw, -hh + btf), (-hbf, -hh + btf),
            ])
        }
        _ => None,
    }
}

fn parse_profile(file: &StepFile, id: u32, scale: f64) -> Option<Vec<(f64, f64)>> {
    let p = file.entity(id)?;
    let tag = p.tag.to_ascii_uppercase();

    // Parameterized structural sections — placed by their own Position like the
    // rectangle and circle below.
    if let Some(local) = parametric_section_local(&tag, p, scale) {
        let (o, dx, dy) = profile_placement_2d(file, p.args.get(2).and_then(|v| v.as_ref()), scale);
        return Some(local.iter().map(|&(u, v)| place2d((u, v), o, dx, dy)).collect());
    }

    if tag == "IFCRECTANGLEPROFILEDEF" {
        // (ProfileType, ProfileName, Position, XDim, YDim)
        let xd = p.args.get(3).and_then(|v| v.as_f64())? * scale;
        let yd = p.args.get(4).and_then(|v| v.as_f64())? * scale;
        let (hx, hy) = (xd / 2.0, yd / 2.0);
        let local = [(-hx, -hy), (hx, -hy), (hx, hy), (-hx, hy)];
        let (o, dx, dy) = profile_placement_2d(file, p.args.get(2).and_then(|v| v.as_ref()), scale);
        return Some(local.iter().map(|&(u, v)| place2d((u, v), o, dx, dy)).collect());
    }

    if tag == "IFCCIRCLEPROFILEDEF" {
        // (ProfileType, ProfileName, Position, Radius) — tessellated to a polygon.
        let r = p.args.get(3).and_then(|v| v.as_f64())? * scale;
        if !(r > 0.0) {
            return None;
        }
        let (o, dx, dy) = profile_placement_2d(file, p.args.get(2).and_then(|v| v.as_ref()), scale);
        let segments = circle_segments(r);
        return Some(
            (0..segments)
                .map(|i| {
                    let a = std::f64::consts::TAU * (i as f64) / (segments as f64);
                    place2d((r * a.cos(), r * a.sin()), o, dx, dy)
                })
                .collect(),
        );
    }

    if tag == "IFCARBITRARYCLOSEDPROFILEDEF" || tag == "IFCARBITRARYPROFILEDEFWITHVOIDS" {
        // (ProfileType, ProfileName, OuterCurve[, InnerCurves]) — the outer boundary.
        // The voids (InnerCurves) are read separately by `parse_profile_voids`, so a
        // caller that can punch holes (extrude) does and one that can't stays solid.
        let outer = p.args.get(2).and_then(|v| v.as_ref())?;
        return profile_loop_2d(file, outer, scale);
    }
    None
}

/// The inner curves (voids) of an `IfcArbitraryProfileDefWithVoids`, each as a 2D
/// polyline loop in the profile's own frame. Empty for any other profile (it has no
/// voids) or when an inner curve is not a readable closed polyline.
fn parse_profile_voids(file: &StepFile, id: u32, scale: f64) -> Vec<Vec<(f64, f64)>> {
    let Some(p) = file.entity(id) else {
        return Vec::new();
    };
    if !p.tag.eq_ignore_ascii_case("IFCARBITRARYPROFILEDEFWITHVOIDS") {
        return Vec::new();
    }
    // (ProfileType, ProfileName, OuterCurve, InnerCurves).
    let Some(inner_curves) = p.args.get(3).and_then(|v| v.as_list()) else {
        return Vec::new();
    };
    inner_curves
        .iter()
        .filter_map(|c| c.as_ref())
        .filter_map(|cid| profile_loop_2d(file, cid, scale))
        .filter(|loop_| loop_.len() >= 3)
        .collect()
}

/// The outer curve of an arbitrary profile as 2D points, when it is an
/// `IfcPolyline` (the common case). Curve outer boundaries (composite / indexed /
/// arc) go through `profile_loop_2d`, which falls back here for the polyline case.
fn polyline_2d(file: &StepFile, id: u32, scale: f64) -> Option<Vec<(f64, f64)>> {
    let c = file.entity(id)?;
    if !c.tag.eq_ignore_ascii_case("IFCPOLYLINE") {
        return None;
    }
    let pts = c.args.first()?.as_list()?;
    let mut out: Vec<(f64, f64)> = Vec::new();
    for pv in pts {
        let e = pv.as_ref().and_then(|pid| file.entity(pid))?;
        let coords = e.args.first()?.as_list()?;
        let x = coords.first()?.as_f64()? * scale;
        let y = coords.get(1)?.as_f64()? * scale;
        // A closed polyline repeats its first point last; drop the duplicate.
        if out.last().map_or(true, |&(lx, ly)| (lx - x).abs() > 1e-9 || (ly - y).abs() > 1e-9) {
            out.push((x, y));
        }
    }
    if out.len() >= 2 && {
        let (fx, fy) = out[0];
        let (lx, ly) = *out.last().unwrap();
        (fx - lx).abs() <= 1e-9 && (fy - ly).abs() <= 1e-9
    } {
        out.pop();
    }
    Some(out)
}

/// A profile's outer or inner curve as 2D `(x, y)` points in its own frame. A
/// profile curve is just a directrix that happens to live in the z = 0 plane, so
/// anything the directrix path already samples — `IfcCompositeCurve` (line + arc
/// runs, the way a rounded section is drawn), `IfcIndexedPolyCurve`, an arc or a
/// spline — is sampled the same way and projected to 2D. `IfcPolyline` keeps its
/// own proven reader as the fast common case.
fn profile_loop_2d(file: &StepFile, id: u32, scale: f64) -> Option<Vec<(f64, f64)>> {
    if let Some(poly) = polyline_2d(file, id, scale) {
        return Some(poly);
    }
    // A curve: sample it as a z = 0 directrix and drop z.
    let mut pts: Vec<(f64, f64)> =
        sample_directrix(file, id, scale)?.into_iter().map(|p| (p.x, p.y)).collect();
    // A closed profile curve repeats its start at the end; drop that duplicate so
    // the loop is not degenerate (matches `polyline_2d`).
    if pts.len() >= 2 {
        let (fx, fy) = pts[0];
        let (lx, ly) = *pts.last().unwrap();
        if (fx - lx).abs() <= 1e-6 && (fy - ly).abs() <= 1e-6 {
            pts.pop();
        }
    }
    (pts.len() >= 3).then_some(pts)
}

/// An `IfcAxis2Placement2D` → (origin, x-axis, y-axis) in 2D, defaulting to the
/// identity. The profile's local frame within the swept area.
fn profile_placement_2d(
    file: &StepFile,
    id: Option<u32>,
    scale: f64,
) -> ((f64, f64), (f64, f64), (f64, f64)) {
    let default = ((0.0, 0.0), (1.0, 0.0), (0.0, 1.0));
    let Some(place) = id.and_then(|pid| file.entity(pid)) else { return default };
    if !place.tag.eq_ignore_ascii_case("IFCAXIS2PLACEMENT2D") {
        return default;
    }
    let origin = place
        .args
        .first()
        .and_then(|v| v.as_ref())
        .and_then(|pid| file.entity(pid))
        .and_then(|e| {
            let c = e.args.first()?.as_list()?;
            Some((c.first()?.as_f64()? * scale, c.get(1)?.as_f64()? * scale))
        })
        .unwrap_or((0.0, 0.0));
    let refd = place
        .args
        .get(1)
        .and_then(|v| v.as_ref())
        .and_then(|did| file.entity(did))
        .and_then(|e| {
            let c = e.args.first()?.as_list()?;
            Some((c.first()?.as_f64()?, c.get(1)?.as_f64()?))
        })
        .unwrap_or((1.0, 0.0));
    let len = (refd.0 * refd.0 + refd.1 * refd.1).sqrt();
    let dx = if len > 1e-12 { (refd.0 / len, refd.1 / len) } else { (1.0, 0.0) };
    let dy = (-dx.1, dx.0); // +90° so (dx, dy) is right-handed
    (origin, dx, dy)
}

/// Apply a 2D placement to a local (u, v).
fn place2d((u, v): (f64, f64), o: (f64, f64), dx: (f64, f64), dy: (f64, f64)) -> (f64, f64) {
    (o.0 + dx.0 * u + dy.0 * v, o.1 + dx.1 * u + dy.1 * v)
}

/// `IfcDirection` → a vector (direction ratios are unitless).
fn read_direction(file: &StepFile, id: u32) -> Option<DVec3> {
    let e = file.entity(id)?;
    if !e.tag.eq_ignore_ascii_case("IFCDIRECTION") {
        return None;
    }
    let c = e.args.first()?.as_list()?;
    let x = c.first()?.as_f64()?;
    let y = c.get(1)?.as_f64()?;
    let z = c.get(2).and_then(|v| v.as_f64()).unwrap_or(0.0);
    Some(DVec3::new(x, y, z))
}

/// Segment count to tessellate a profile circle at the render chord tolerance.
fn circle_segments(radius: f64) -> usize {
    let ratio = (1.0 - ARC_CHORD_TOL_MM / radius).clamp(-1.0, 1.0);
    let step = 2.0 * ratio.acos();
    if step > 1e-9 {
        ((std::f64::consts::TAU / step).ceil() as usize).clamp(8, 512)
    } else {
        32
    }
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
    // A disk — one bound, one self-loop curve edge, no holes — can be rebuilt
    // as its exact curve instead of the tessellated polygon.
    let closed_curve = if inners.is_empty() {
        single_closed_curve(file, face, scale)
    } else {
        None
    };
    Some(FaceLoops { outer, inners, closed_curve })
}

/// The exact curve when a face is a single closed-curve disk: exactly one
/// bound, one edge loop, one self-loop edge (`EdgeStart == EdgeEnd`) whose
/// geometry is a circle or a B-spline. `None` for anything else — a box, a
/// holed face, a multi-edge boundary — so the importer keeps its polygon path
/// for all of those.
fn single_closed_curve(file: &StepFile, face: &Entity, scale: f64) -> Option<axia_geo::AnalyticCurve> {
    let bounds = face.args.first()?.as_list()?;
    if bounds.len() != 1 {
        return None;
    }
    let bound = file.entity(bounds[0].as_ref()?)?;
    let lp = file.entity(bound.args.first().and_then(|v| v.as_ref())?)?;
    if !lp.tag.eq_ignore_ascii_case("IFCEDGELOOP") {
        return None;
    }
    let edges = lp.args.first()?.as_list()?;
    if edges.len() != 1 {
        return None;
    }
    let oe = file.entity(edges[0].as_ref()?)?;
    // IfcOrientedEdge.EdgeElement is attribute 2; a bare edge is itself.
    let edge = if oe.tag.eq_ignore_ascii_case("IFCORIENTEDEDGE") {
        file.entity(oe.args.get(2).and_then(|v| v.as_ref())?)?
    } else {
        oe
    };
    // Self-loop only: the whole rim in one edge.
    let a = edge.args.first().and_then(|v| v.as_ref()).and_then(|id| vertex_point(file, id, scale))?;
    let b = edge.args.get(1).and_then(|v| v.as_ref()).and_then(|id| vertex_point(file, id, scale))?;
    if (a - b).length_squared() > 1e-9 {
        return None;
    }
    edge_closed_curve(file, edge, scale)
}

/// Build the exact [`AnalyticCurve`] for a self-loop curve edge — circle,
/// B-spline, or rational B-spline (NURBS / ellipse). `None` for a line or an
/// unhandled geometry.
fn edge_closed_curve(file: &StepFile, edge: &Entity, scale: f64) -> Option<axia_geo::AnalyticCurve> {
    use axia_geo::AnalyticCurve;

    let geom_id = edge.args.get(2).and_then(|v| v.as_ref())?;
    let mut curve = file.entity(geom_id)?;
    if curve.tag.eq_ignore_ascii_case("IFCTRIMMEDCURVE") {
        curve = file.entity(curve.args.first().and_then(|v| v.as_ref())?)?;
    }

    if curve.tag.eq_ignore_ascii_case("IFCCIRCLE") {
        let pos = curve.args.first().and_then(|v| v.as_ref())?;
        let place = crate::ifc_placement::axis_placement(file, pos, scale)?;
        let radius = curve.args.get(1).and_then(|v| v.as_f64())? * scale;
        if !(radius > 0.0) {
            return None;
        }
        return Some(AnalyticCurve::Circle {
            center: place.origin,
            radius,
            normal: place.z,
            basis_u: place.x,
        });
    }

    let rational = curve.tag.eq_ignore_ascii_case("IFCRATIONALBSPLINECURVEWITHKNOTS");
    if rational || curve.tag.eq_ignore_ascii_case("IFCBSPLINECURVEWITHKNOTS") {
        let (control_pts, knots, degree, weights) = parse_bspline(file, curve, scale)?;
        return Some(if let Some(weights) = weights.filter(|_| rational) {
            AnalyticCurve::NURBS { control_pts, weights, knots, degree: degree as u32 }
        } else {
            AnalyticCurve::BSpline { control_pts, knots, degree: degree as u32 }
        });
    }
    None
}

/// Parse an `IfcBSplineCurveWithKnots` (or `RATIONAL`) into the pieces the
/// engine's curve types want: control points (scaled), the *flat* knot vector
/// (distinct knots expanded by their multiplicities — the inverse of the
/// exporter's `compress_knots`), the degree, and weights when present.
fn parse_bspline(
    file: &StepFile,
    curve: &Entity,
    scale: f64,
) -> Option<(Vec<DVec3>, Vec<f64>, usize, Option<Vec<f64>>)> {
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
    let mut knots: Vec<f64> = Vec::new();
    for (k, m) in distinct.iter().zip(&mults) {
        knots.extend(std::iter::repeat(*k).take(*m));
    }
    if knots.len() != control_pts.len() + degree + 1 {
        return None;
    }
    let weights = curve
        .args
        .get(8)
        .and_then(|v| v.as_list())
        .map(|l| l.iter().filter_map(|v| v.as_f64()).collect::<Vec<f64>>())
        .filter(|w| w.len() == control_pts.len());
    Some((control_pts, knots, degree, weights))
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

/// Chord tolerance for a spline used as a *directrix* (a sweep path), in mm.
/// Coarser than a rendered edge: the fine boundary tolerance would put hundreds
/// of cross-section rings on a metre-scale pipe, so a path only needs to be
/// smooth to a couple of millimetres.
const DIRECTRIX_SPLINE_TOL_MM: f64 = 2.0;

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

    let (control_pts, knots, degree, weights) = parse_bspline(file, curve, scale)?;
    let full = if let Some(weights) = weights.filter(|_| rational) {
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
    fn a_curve_disk_carries_its_exact_curve_a_box_does_not() {
        use axia_geo::AnalyticCurve;

        // A single closed-curve disk hands the importer the *exact* curve, so it
        // can build a kernel-native face (one anchor + one self-loop edge)
        // instead of baking the tessellated polygon. A circle disk:
        let g = import_ifc_geometry(&closed_circle_ifc(false)).unwrap();
        match &g.elements[0].faces[0].closed_curve {
            Some(AnalyticCurve::Circle { radius, .. }) => {
                assert!((radius - 1500.0).abs() < 1.0, "the circle's radius survives: {radius}")
            }
            other => panic!("expected a Circle, got {other:?}"),
        }

        // A spline disk (rational — an ellipse's form):
        let g = import_ifc_geometry(&closed_spline_ifc(true)).unwrap();
        assert!(
            matches!(g.elements[0].faces[0].closed_curve, Some(AnalyticCurve::NURBS { .. })),
            "a rational spline disk carries a NURBS curve"
        );

        // A box face is a polygon — no exact curve, so the importer keeps its
        // (unchanged) polygon path. This is the guard against the kernel-native
        // path leaking onto ordinary geometry.
        let ifc = emit_box(DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0), "Box");
        let g = import_ifc_geometry(&ifc).unwrap();
        assert!(
            g.elements[0].faces.iter().all(|f| f.closed_curve.is_none()),
            "no box face pretends to be a curve"
        );
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

    /// A member whose body is an `IfcExtrudedAreaSolid` with the given profile,
    /// extruded 3 m up.
    fn extruded_wall_ifc(profile: &str) -> String {
        format!(
            "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
{profile}
#50=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#51=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#50));
#52=IFCPRODUCTDEFINITIONSHAPE($,$,(#51));
#53=IFCWALL('w',$,'Swept',$,$,$,#52,$,$);
ENDSEC;
END-ISO-10303-21;
"
        )
    }

    #[test]
    fn an_extruded_rectangle_becomes_a_prism() {
        // The dominant real-BIM representation: a 2D profile swept a depth. A
        // 4 m x 0.2 m rectangle extruded 3 m is a wall — two caps + four walls.
        let g = import_ifc_geometry(&extruded_wall_ifc("#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);")).unwrap();
        let el = &g.elements[0];
        assert_eq!(el.faces.len(), 6, "rectangle prism = 6 faces");

        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for f in &el.faces {
            for p in &f.outer {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
        // metres → mm: 4000 x 200 footprint, 3000 tall (extruded up +Z).
        assert!((hi.x - lo.x - 4000.0).abs() < 1.0, "x span {}", hi.x - lo.x);
        assert!((hi.y - lo.y - 200.0).abs() < 1.0, "y span {}", hi.y - lo.y);
        assert!((hi.z - lo.z - 3000.0).abs() < 1.0, "extruded 3 m up: {}", hi.z - lo.z);
    }

    #[test]
    fn an_extruded_polyline_becomes_a_prism_and_a_circle_is_round() {
        // Arbitrary closed profile: a triangle → a 3-sided prism (5 faces).
        let g = import_ifc_geometry(&extruded_wall_ifc(
            "#30=IFCCARTESIANPOINT((0.,0.));\n#31=IFCCARTESIANPOINT((4.,0.));\n#32=IFCCARTESIANPOINT((0.,3.));\n\
             #33=IFCPOLYLINE((#30,#31,#32,#30));\n#40=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#33);",
        ))
        .unwrap();
        assert_eq!(g.elements[0].faces.len(), 5, "triangle prism = 2 caps + 3 sides");

        // Circle profile tessellates to many sides — a round column.
        let g = import_ifc_geometry(&extruded_wall_ifc("#40=IFCCIRCLEPROFILEDEF(.AREA.,$,$,0.3);")).unwrap();
        assert!(g.elements[0].faces.len() > 20, "a circle profile is many-sided: {}", g.elements[0].faces.len());
    }

    #[test]
    fn an_extruded_profile_with_a_void_becomes_a_tube() {
        // IfcArbitraryProfileDefWithVoids — a 4 m × 2 m rectangle with a 2 m × 1 m
        // rectangular void, extruded 3 m → a rectangular tube (a through-hole).
        let g = import_ifc_geometry(&extruded_wall_ifc(
            "#30=IFCCARTESIANPOINT((0.,0.));\n#31=IFCCARTESIANPOINT((4.,0.));\n\
             #32=IFCCARTESIANPOINT((4.,2.));\n#39=IFCCARTESIANPOINT((0.,2.));\n\
             #33=IFCPOLYLINE((#30,#31,#32,#39,#30));\n\
             #34=IFCCARTESIANPOINT((1.,0.5));\n#35=IFCCARTESIANPOINT((3.,0.5));\n\
             #36=IFCCARTESIANPOINT((3.,1.5));\n#37=IFCCARTESIANPOINT((1.,1.5));\n\
             #38=IFCPOLYLINE((#34,#35,#36,#37,#34));\n\
             #40=IFCARBITRARYPROFILEDEFWITHVOIDS(.AREA.,$,#33,(#38));",
        ))
        .unwrap();
        let el = &g.elements[0];
        // 2 caps + 4 outer side quads + 4 void side quads.
        assert_eq!(el.faces.len(), 10, "rect tube = 2 caps + 4 outer + 4 void walls");
        // Exactly the two caps carry the void as a 4-point hole.
        let capped: Vec<_> = el.faces.iter().filter(|f| f.inners.len() == 1).collect();
        assert_eq!(capped.len(), 2, "two caps, each with one hole");
        assert!(capped.iter().all(|f| f.inners[0].len() == 4), "the hole is the square void");
    }

    #[test]
    fn an_extruded_composite_curve_profile_is_a_prism() {
        // A profile whose OuterCurve is an IfcCompositeCurve of three straight
        // segments (a triangle) → the same 3-sided prism a polyline triangle gives.
        let g = import_ifc_geometry(&extruded_wall_ifc(
            "#30=IFCCARTESIANPOINT((0.,0.));\n#31=IFCCARTESIANPOINT((4.,0.));\n\
             #32=IFCCARTESIANPOINT((0.,3.));\n\
             #33=IFCPOLYLINE((#30,#31));\n#34=IFCPOLYLINE((#31,#32));\n#35=IFCPOLYLINE((#32,#30));\n\
             #36=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#33);\n\
             #37=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#34);\n\
             #38=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#35);\n\
             #39=IFCCOMPOSITECURVE((#36,#37,#38),.F.);\n\
             #40=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#39);",
        ))
        .unwrap();
        assert_eq!(g.elements[0].faces.len(), 5, "composite triangle prism = 2 caps + 3 sides");
    }

    /// AABB (lo, hi, face count) of an extruded profile in mm.
    fn extruded_section_aabb(profile: &str) -> (DVec3, DVec3, usize) {
        let g = import_ifc_geometry(&extruded_wall_ifc(profile)).unwrap();
        let el = &g.elements[0];
        let (mut lo, mut hi) = (DVec3::splat(f64::INFINITY), DVec3::splat(f64::NEG_INFINITY));
        for f in &el.faces {
            for p in &f.outer {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }
        (lo, hi, el.faces.len())
    }

    #[test]
    fn parametric_structural_sections_import_at_the_bbox_centre() {
        // The named steel sections. Sharp corners → a clean N-gon prism: 2 caps +
        // one side quad per outline vertex. Extruded 3 m up (+Z). The origin is the
        // section's bounding-box centre — cross-checked against web-ifc, so the
        // extent is symmetric in each parametric axis.
        let near = |a: f64, b: f64| (a - b).abs() < 1.0;

        // I / H — 0.2 m wide, 0.4 m deep, doubly symmetric. 12-gon → 14 faces.
        let (lo, hi, faces) = extruded_section_aabb("#40=IFCISHAPEPROFILEDEF(.AREA.,$,$,0.2,0.4,0.01,0.015);");
        assert_eq!(faces, 14, "I = 2 caps + 12 sides");
        assert!(near(lo.x, -100.0) && near(hi.x, 100.0), "I centred in X: {lo:?}..{hi:?}");
        assert!(near(lo.y, -200.0) && near(hi.y, 200.0), "I centred in Y");
        assert!(near(hi.z - lo.z, 3000.0), "extruded 3 m");

        // T — depth 0.4, flange 0.3. Symmetric about Y, bbox-centred: x∈[-150,150],
        // y∈[-200,200]. 8-gon → 10 faces.
        let (lo, hi, faces) = extruded_section_aabb("#40=IFCTSHAPEPROFILEDEF(.AREA.,$,$,0.4,0.3,0.02,0.03);");
        assert_eq!(faces, 10, "T = 2 caps + 8 sides");
        assert!(near(lo.x, -150.0) && near(hi.x, 150.0), "T x: {lo:?}..{hi:?}");
        assert!(near(lo.y, -200.0) && near(hi.y, 200.0), "T y bbox-centred");

        // U / channel — depth 0.3, flange 0.15. bbox-centred: x∈[-75,75], y∈[-150,150].
        let (lo, hi, faces) = extruded_section_aabb("#40=IFCUSHAPEPROFILEDEF(.AREA.,$,$,0.3,0.15,0.02,0.02);");
        assert_eq!(faces, 10, "U = 2 caps + 8 sides");
        assert!(near(lo.x, -75.0) && near(hi.x, 75.0), "U x bbox-centred: {lo:?}..{hi:?}");
        assert!(near(lo.y, -150.0) && near(hi.y, 150.0), "U y");

        // L / angle — depth 0.2, width 0.15. bbox-centred: x∈[-75,75], y∈[-100,100].
        let (lo, hi, faces) = extruded_section_aabb("#40=IFCLSHAPEPROFILEDEF(.AREA.,$,$,0.2,0.15,0.02);");
        assert_eq!(faces, 8, "L = 2 caps + 6 sides");
        assert!(near(lo.x, -75.0) && near(hi.x, 75.0), "L x: {lo:?}..{hi:?}");
        assert!(near(lo.y, -100.0) && near(hi.y, 100.0), "L y");

        // L with a null Width ⇒ equal legs (width = depth = 0.2), x∈[-100,100].
        let (lo, hi, _) = extruded_section_aabb("#40=IFCLSHAPEPROFILEDEF(.AREA.,$,$,0.2,$,0.02);");
        assert!(near(lo.x, -100.0) && near(hi.x, 100.0), "equal-leg L x: {lo:?}..{hi:?}");
        assert!(near(lo.y, -100.0) && near(hi.y, 100.0), "equal-leg L y");
    }

    #[test]
    fn cold_formed_and_asymmetric_sections_import_at_the_bbox_centre() {
        // C/Z were cross-checked vertex-for-vertex against web-ifc (sharp-corner
        // fixtures); the asymmetric I is a web-ifc kernel gap, so it is checked below
        // by the equal-flange ≡ I-shape equivalence instead.
        let near = |a: f64, b: f64| (a - b).abs() < 1.0;

        // C / lipped channel — depth 0.3, width 0.15, wall 0.02, girth 0.05. The
        // outline traces web + flanges + turned-in lips: 12-gon → 14 faces. bbox
        // centred: x∈[-75,75], y∈[-150,150].
        let (lo, hi, faces) = extruded_section_aabb("#40=IFCCSHAPEPROFILEDEF(.AREA.,$,$,0.3,0.15,0.02,0.05);");
        assert_eq!(faces, 14, "C = 2 caps + 12 sides");
        assert!(near(lo.x, -75.0) && near(hi.x, 75.0), "C x: {lo:?}..{hi:?}");
        assert!(near(lo.y, -150.0) && near(hi.y, 150.0), "C y");

        // Z — depth 0.3, overall width 0.1, web 0.02, flange 0.02. FlangeWidth is the
        // tip-to-tip width (web-ifc convention) → x∈[-50,50]. 8-gon → 10 faces.
        let (lo, hi, faces) = extruded_section_aabb("#40=IFCZSHAPEPROFILEDEF(.AREA.,$,$,0.3,0.1,0.02,0.02);");
        assert_eq!(faces, 10, "Z = 2 caps + 8 sides");
        assert!(near(lo.x, -50.0) && near(hi.x, 50.0), "Z x tip-to-tip: {lo:?}..{hi:?}");
        assert!(near(lo.y, -150.0) && near(hi.y, 150.0), "Z y");

        // Asymmetric I — bottom flange 0.3, depth 0.4, web 0.02, bottom flange 0.03,
        // top flange 0.2, top flange 0.025. Symmetric about Y; the widest flange
        // (bottom, 0.3) sets x∈[-150,150], depth centred y∈[-200,200]. 12-gon → 14.
        let g = import_ifc_geometry(&extruded_wall_ifc(
            "#40=IFCASYMMETRICISHAPEPROFILEDEF(.AREA.,$,$,0.3,0.4,0.02,0.03,$,0.2,0.025,$,$,$);",
        ))
        .unwrap();
        let el = &g.elements[0];
        assert_eq!(el.faces.len(), 14, "asymmetric I = 2 caps + 12 sides");
        // The flanges differ: near the bottom (y ≈ −200) the section reaches x = ±150
        // (bottom flange 0.3); near the top (y ≈ +200) only x = ±100 (top flange 0.2).
        let (mut bot_x, mut top_x) = (0.0_f64, 0.0_f64);
        for f in &el.faces {
            for p in &f.outer {
                if (p.y + 200.0).abs() < 1.0 {
                    bot_x = bot_x.max(p.x.abs());
                }
                if (p.y - 200.0).abs() < 1.0 {
                    top_x = top_x.max(p.x.abs());
                }
            }
        }
        assert!(near(bot_x, 150.0), "wide bottom flange reaches ±150, got {bot_x}");
        assert!(near(top_x, 100.0), "narrow top flange reaches ±100, got {top_x}");

        // The equivalence that stands in for web-ifc: an asymmetric I with equal
        // flanges must extrude to exactly the same solid as the plain I-shape.
        let (alo, ahi, af) = extruded_section_aabb(
            "#40=IFCASYMMETRICISHAPEPROFILEDEF(.AREA.,$,$,0.2,0.4,0.01,0.015,$,0.2,0.015,$,$,$);",
        );
        let (ilo, ihi, iff) = extruded_section_aabb("#40=IFCISHAPEPROFILEDEF(.AREA.,$,$,0.2,0.4,0.01,0.015);");
        assert_eq!(af, iff, "equal-flange asymmetric I has the I-shape face count");
        assert!(near(alo.x, ilo.x) && near(ahi.x, ihi.x) && near(alo.y, ilo.y) && near(ahi.y, ihi.y),
            "equal-flange asymmetric I ≡ I-shape extent");
    }

    #[test]
    fn an_extruded_composite_curve_with_an_arc_rounds_a_corner() {
        // A quarter-disk profile: two straight edges + a 90° arc between them, drawn
        // as an IfcCompositeCurve (line + trimmed-circle arc + line). The arc is
        // sampled at ~15°/step, so the outer loop gains several points → more side
        // walls than a straight three-corner profile would.
        let g = import_ifc_geometry(&extruded_wall_ifc(
            "#10=IFCCARTESIANPOINT((0.,0.));\n#11=IFCCARTESIANPOINT((4.,0.));\n\
             #12=IFCCARTESIANPOINT((0.,4.));\n#13=IFCAXIS2PLACEMENT2D(#10,$);\n\
             #14=IFCCIRCLE(#13,4.);\n#15=IFCTRIMMEDCURVE(#14,(#11),(#12),.T.,.CARTESIAN.);\n\
             #16=IFCPOLYLINE((#10,#11));\n#17=IFCPOLYLINE((#12,#10));\n\
             #18=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#16);\n\
             #19=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#15);\n\
             #20=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#17);\n\
             #21=IFCCOMPOSITECURVE((#18,#19,#20),.F.);\n\
             #40=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#21);",
        ))
        .unwrap();
        // 90° arc → 6 (or 7 at the FP boundary) spans, plus the two straight edges:
        // 8–9 outer points → 8–9 side quads + 2 caps.
        let n = g.elements[0].faces.len();
        assert!((10..=11).contains(&n), "quarter-disk prism = arc-tessellated sides + 2 caps, got {n}");
    }

    #[test]
    fn an_extruded_indexed_polycurve_profile_rounds_a_corner() {
        // A profile whose OuterCurve is an IfcIndexedPolyCurve over a 2D point list —
        // straight runs (IfcLineIndex) plus a 3-point arc corner (IfcArcIndex).
        let g = import_ifc_geometry(&extruded_wall_ifc(
            "#10=IFCCARTESIANPOINTLIST2D(((0.,0.),(4.,0.),(4.,4.),(2.,5.5),(0.,4.)));\n\
             #11=IFCINDEXEDPOLYCURVE(#10,(IFCLINEINDEX((1,2,3)),IFCARCINDEX((3,4,5)),IFCLINEINDEX((5,1))),.F.);\n\
             #40=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#11);",
        ))
        .unwrap();
        // Three straight corners + an arc corner → clearly more than the 5 faces a
        // plain triangle gives.
        let n = g.elements[0].faces.len();
        assert!(n >= 8, "indexed profile with an arc corner tessellates to many sides, got {n}");
    }

    /// A cube as an IfcTriangulatedFaceSet (SketchUp / Revit tessellated export):
    /// eight points, twelve triangles indexed 1-based. Each triangle becomes a
    /// face, and out-of-range or zero-area triangles are counted, not imported.
    fn tri_cube_ifc(triangles: &str, close_paren: &str) -> String {
        format!(
            "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#75=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1.,0.,0.),(1.,1.,0.),(0.,1.,0.),(0.,0.,1.),(1.,0.,1.),(1.,1.,1.),(0.,1.,1.)));
#74=IFCTRIANGULATEDFACESET(#75,$,.T.,({triangles}),$);
#78=IFCSHAPEREPRESENTATION($,'Body','Tessellation',(#74));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCWALL('w',$,'TriCube',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
{close_paren}"
        )
    }

    #[test]
    fn a_triangulated_face_set_becomes_one_face_per_triangle() {
        let cube = "(1,2,3),(1,3,4),(5,6,7),(5,7,8),(1,5,6),(1,6,2),\
                    (4,3,7),(4,7,8),(1,4,8),(1,8,5),(2,6,7),(2,7,3)";
        let g = import_ifc_geometry(&tri_cube_ifc(cube, "")).unwrap();
        let el = &g.elements[0];
        assert_eq!(el.faces.len(), 12, "twelve triangles → twelve faces");
        // Each face is a triangle; the shared corners dedup at import (8 points).
        assert!(el.faces.iter().all(|f| f.outer.len() == 3), "every face is a triangle");
    }

    #[test]
    fn a_triangulated_face_set_skips_bad_triangles() {
        // A degenerate (repeated index → zero area) and an out-of-range index are
        // both dropped rather than importing a broken face.
        let tris = "(1,2,3),(1,1,2),(1,2,99)";
        let g = import_ifc_geometry(&tri_cube_ifc(tris, "")).unwrap();
        assert_eq!(g.elements[0].faces.len(), 1, "only the one good triangle survives");
    }

    #[test]
    fn a_polygonal_face_set_keeps_each_face_as_one_n_gon() {
        // A cube as six IfcIndexedPolygonalFace quads — each stays a quad, not
        // two triangles.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#75=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(1.,0.,0.),(1.,1.,0.),(0.,1.,0.),(0.,0.,1.),(1.,0.,1.),(1.,1.,1.),(0.,1.,1.)));
#60=IFCINDEXEDPOLYGONALFACE((1,4,3,2));
#61=IFCINDEXEDPOLYGONALFACE((5,6,7,8));
#62=IFCINDEXEDPOLYGONALFACE((1,2,6,5));
#63=IFCINDEXEDPOLYGONALFACE((2,3,7,6));
#64=IFCINDEXEDPOLYGONALFACE((3,4,8,7));
#65=IFCINDEXEDPOLYGONALFACE((4,1,5,8));
#74=IFCPOLYGONALFACESET(#75,.T.,(#60,#61,#62,#63,#64,#65),$);
#78=IFCSHAPEREPRESENTATION($,'Body','Tessellation',(#74));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCWALL('w',$,'PolyCube',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
"
        .to_string();
        let g = import_ifc_geometry(&src).unwrap();
        let el = &g.elements[0];
        assert_eq!(el.faces.len(), 6, "six quads, not twelve triangles");
        assert!(el.faces.iter().all(|f| f.outer.len() == 4), "every face is a quad");
    }

    #[test]
    fn an_indexed_polygonal_face_with_voids_carries_its_holes() {
        // A 3x3 plate with a 1x1 square hole: one face, one inner loop.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#75=IFCCARTESIANPOINTLIST3D(((0.,0.,0.),(3.,0.,0.),(3.,3.,0.),(0.,3.,0.),(1.,1.,0.),(2.,1.,0.),(2.,2.,0.),(1.,2.,0.)));
#60=IFCINDEXEDPOLYGONALFACEWITHVOIDS((1,2,3,4),((5,6,7,8)));
#74=IFCPOLYGONALFACESET(#75,$,(#60),$);
#78=IFCSHAPEREPRESENTATION($,'Body','Tessellation',(#74));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCSLAB('s',$,'HolePlate',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
"
        .to_string();
        let g = import_ifc_geometry(&src).unwrap();
        let el = &g.elements[0];
        assert_eq!(el.faces.len(), 1, "one face");
        assert_eq!(el.faces[0].outer.len(), 4, "a square outer boundary");
        assert_eq!(el.faces[0].inners.len(), 1, "one hole");
        assert_eq!(el.faces[0].inners[0].len(), 4, "a square hole");
    }

    /// A square profile offset from the axis, revolved a `revolution` around the
    /// Y axis. Full turn → a rectangular-section ring; the `unit`/`angle` slot
    /// lets a test choose radians or a declared degree unit.
    fn revolved_ring_ifc(unit: &str, angle: &str) -> String {
        format!(
            "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
{unit}
#24=IFCCARTESIANPOINT((0.,0.,0.));
#34=IFCCARTESIANPOINT((2.,0.));
#35=IFCCARTESIANPOINT((3.,0.));
#36=IFCCARTESIANPOINT((3.,1.));
#37=IFCCARTESIANPOINT((2.,1.));
#33=IFCPOLYLINE((#34,#35,#36,#37,#34));
#40=IFCARBITRARYCLOSEDPROFILEDEF(.AREA.,$,#33);
#51=IFCAXIS2PLACEMENT3D(#24,$,$);
#53=IFCDIRECTION((0.,1.,0.));
#52=IFCAXIS1PLACEMENT(#24,#53);
#50=IFCREVOLVEDAREASOLID(#40,#51,#52,{angle});
#78=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#50));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCWALL('w',$,'Ring',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
"
        )
    }

    #[test]
    fn a_revolved_area_solid_sweeps_a_closed_ring() {
        // Full 360° (2π rad) → side quads all the way round, no caps.
        let g = import_ifc_geometry(&revolved_ring_ifc("", "6.283185307")).unwrap();
        let el = &g.elements[0];
        assert!(el.faces.len() >= 48, "many side faces around the ring: {}", el.faces.len());
        assert_eq!(el.faces.len() % 4, 0, "four profile edges × N steps, no caps");
        assert!(el.faces.iter().all(|f| f.outer.len() == 4), "every side face is a quad");
    }

    #[test]
    fn a_partial_revolution_adds_end_caps() {
        // A quarter turn is capped at both ends, so it has two more faces than the
        // bare side quads (4 edges × steps).
        let g = import_ifc_geometry(&revolved_ring_ifc("", "1.570796327")).unwrap();
        let count = g.elements[0].faces.len();
        assert_eq!(count % 4, 2, "4×steps side quads + 2 caps: {}", count);
    }

    #[test]
    fn a_revolution_angle_in_degrees_matches_radians() {
        // 360 DEGREE (via IfcConversionBasedUnit) is the same full turn as 2π rad.
        let deg_unit = "#11=IFCCONVERSIONBASEDUNIT(#12,.PLANEANGLEUNIT.,'DEGREE',#13);\n\
                        #12=IFCDIMENSIONALEXPONENTS(0,0,0,0,0,0,0);\n\
                        #13=IFCMEASUREWITHUNIT(IFCPLANEANGLEMEASURE(1.745E-2),#14);\n\
                        #14=IFCSIUNIT(*,.PLANEANGLEUNIT.,$,.RADIAN.);";
        let deg = import_ifc_geometry(&revolved_ring_ifc(deg_unit, "360.")).unwrap();
        let rad = import_ifc_geometry(&revolved_ring_ifc("", "6.283185307")).unwrap();
        assert_eq!(
            deg.elements[0].faces.len(),
            rad.elements[0].faces.len(),
            "360° and 2π revolve to the same ring"
        );
    }

    /// A disk (radius, optional inner radius) swept along a polyline directrix —
    /// a pipe. The `points`/`coords` slots build the directrix, `radius` its
    /// section.
    fn swept_pipe_ifc(points: &str, coords: &str, radius: &str) -> String {
        format!(
            "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
{coords}
#33=IFCPOLYLINE(({points}));
#50=IFCSWEPTDISKSOLID(#33,{radius});
#78=IFCSHAPEREPRESENTATION($,'Body','AdvancedSweptSolid',(#50));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCMEMBER('p',$,'Pipe',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
"
        )
    }

    #[test]
    fn a_swept_disk_solid_is_a_pipe() {
        // A straight directrix → a cylinder: 16 section segments × 1 span of side
        // quads, plus a disk cap at each end.
        let coords = "#34=IFCCARTESIANPOINT((0.,0.,0.));\n#35=IFCCARTESIANPOINT((0.,0.,3.));";
        let g = import_ifc_geometry(&swept_pipe_ifc("#34,#35", coords, "0.5")).unwrap();
        assert_eq!(g.elements[0].faces.len(), 18, "16 side quads + 2 caps");

        // A two-segment directrix (an elbow) → twice the side quads, still 2 caps.
        let bend = "#34=IFCCARTESIANPOINT((0.,0.,0.));\n#35=IFCCARTESIANPOINT((0.,0.,2.));\n\
                    #36=IFCCARTESIANPOINT((2.,0.,2.));";
        let g = import_ifc_geometry(&swept_pipe_ifc("#34,#35,#36", bend, "0.3")).unwrap();
        assert_eq!(g.elements[0].faces.len(), 34, "2×16 side quads + 2 caps");
    }

    #[test]
    fn a_hollow_swept_disk_has_annular_end_caps() {
        // An inner radius makes it a tube: an outer wall, an inner wall, and end
        // caps that are the outer ring with the inner ring as a hole.
        let coords = "#34=IFCCARTESIANPOINT((0.,0.,0.));\n#35=IFCCARTESIANPOINT((0.,0.,3.));";
        let g = import_ifc_geometry(&swept_pipe_ifc("#34,#35", coords, "0.5,0.3")).unwrap();
        let faces = &g.elements[0].faces;
        assert_eq!(faces.len(), 34, "16 outer + 16 inner + 2 annular caps");
        let capped = faces.iter().filter(|f| f.inners.len() == 1).count();
        assert_eq!(capped, 2, "two annular caps, each with an inner hole");
    }

    #[test]
    fn a_swept_disk_follows_an_arc_directrix() {
        // A 90° arc directrix (radius 2 m) → a curved elbow. The arc is sampled at
        // ~15°/step (6 spans), so many more side quads than a straight run — not
        // the hundreds a sub-mm chord tolerance would give.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINT((2.,0.,0.));
#11=IFCCARTESIANPOINT((0.,2.,0.));
#14=IFCCARTESIANPOINT((0.,0.,0.));
#15=IFCAXIS2PLACEMENT3D(#14,$,$);
#16=IFCCIRCLE(#15,2.);
#33=IFCTRIMMEDCURVE(#16,(#10),(#11),.T.,.CARTESIAN.);
#50=IFCSWEPTDISKSOLID(#33,0.2);
#78=IFCSHAPEREPRESENTATION($,'Body','AdvancedSweptSolid',(#50));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCMEMBER('p',$,'ArcPipe',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        // 90° / 15° = 6 spans × 16 section segments + 2 caps.
        assert_eq!(g.elements[0].faces.len(), 98, "6 arc spans × 16 + 2 caps");
    }

    #[test]
    fn a_swept_disk_follows_a_composite_curve() {
        // A composite directrix — the 90° arc, then a straight run — is sampled as
        // one path, its shared junction de-duplicated. More spans than the arc
        // alone (it gains the straight segment).
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINT((2.,0.,0.));
#11=IFCCARTESIANPOINT((0.,2.,0.));
#12=IFCCARTESIANPOINT((0.,5.,0.));
#14=IFCCARTESIANPOINT((0.,0.,0.));
#15=IFCAXIS2PLACEMENT3D(#14,$,$);
#16=IFCCIRCLE(#15,2.);
#20=IFCTRIMMEDCURVE(#16,(#10),(#11),.T.,.CARTESIAN.);
#30=IFCPOLYLINE((#11,#12));
#40=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#20);
#41=IFCCOMPOSITECURVESEGMENT(.CONTINUOUS.,.T.,#30);
#33=IFCCOMPOSITECURVE((#40,#41),.F.);
#50=IFCSWEPTDISKSOLID(#33,0.2);
#78=IFCSHAPEREPRESENTATION($,'Body','AdvancedSweptSolid',(#50));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCMEMBER('p',$,'CompositePipe',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        // 6 arc spans + 1 straight span = 7 × 16 + 2 caps.
        assert_eq!(g.elements[0].faces.len(), 114, "arc + straight, junction merged");
    }

    #[test]
    fn arc_through_three_points_recovers_the_circle() {
        // Three points on a radius-5 circle in the XZ plane, at 0°, 40°, 100°.
        let on = |deg: f64| {
            let r = deg.to_radians();
            DVec3::new(5.0 * r.cos(), 0.0, 5.0 * r.sin())
        };
        let (a, m, b) = (on(0.0), on(40.0), on(100.0));
        let pts = sample_arc_3pt(a, m, b);
        // Endpoints land exactly; the mid decides the way round (the short arc).
        assert!((*pts.first().unwrap() - a).length() < 1e-6);
        assert!((*pts.last().unwrap() - b).length() < 1e-6);
        // Every sample sits on the recovered circle (centre at the origin, r = 5).
        for p in &pts {
            assert!((p.length() - 5.0).abs() < 1e-6, "off the circle: {p:?}");
        }
        // 100° / 15° → 7 spans → 8 points. Collinear points fall back to a line.
        assert_eq!(pts.len(), 8);
        let line = sample_arc_3pt(
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(2.0, 0.0, 0.0),
        );
        assert_eq!(line.len(), 3, "collinear → the straight chain a→m→b");
    }

    #[test]
    fn a_swept_disk_follows_an_indexed_polycurve() {
        // A directrix that is a straight run then a 3-point arc, indexed off one
        // shared point list — the CAD-standard way to author a pipe path.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINTLIST3D(((0.,0.,-3.),(5.,0.,0.),(3.8302,0.,3.2139),(-0.8682,0.,4.9240)));
#11=IFCINDEXEDPOLYCURVE(#10,(IFCLINEINDEX((1,2)),IFCARCINDEX((2,3,4))),.F.);
#50=IFCSWEPTDISKSOLID(#11,0.2);
#78=IFCSHAPEREPRESENTATION($,'Body','AdvancedSweptSolid',(#50));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCMEMBER('p',$,'IndexedPipe',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        // 1 straight span + 7 arc spans (100° / 15°) = 8 × 16 side quads + 2 caps.
        assert_eq!(g.elements[0].faces.len(), 130, "line span + 7 arc spans");
    }

    #[test]
    fn a_swept_disk_follows_a_spline_directrix() {
        // A cubic B-spline directrix — sampled to a polyline (its whole run) at the
        // coarse path tolerance, so it curves without exploding into hundreds of rings.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINT((0.,0.,0.));
#11=IFCCARTESIANPOINT((0.,0.,1.));
#12=IFCCARTESIANPOINT((2.,0.,2.));
#13=IFCCARTESIANPOINT((2.,0.,4.));
#14=IFCBSPLINECURVEWITHKNOTS(3,(#10,#11,#12,#13),.UNSPECIFIED.,.F.,.F.,(4,4),(0.,1.),.UNSPECIFIED.);
#50=IFCSWEPTDISKSOLID(#14,0.2);
#78=IFCSHAPEREPRESENTATION($,'Body','AdvancedSweptSolid',(#50));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCMEMBER('p',$,'SplinePipe',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        let faces = g.elements[0].faces.len();
        // More than a single straight span (18) — the curve was sampled — and bounded
        // by the 64-point decimation cap (64 × 16 + 2 = 1026), never exploded.
        assert!(faces > 18, "curved spline gives several spans, got {faces}");
        assert!(faces <= 1026, "decimation caps the ring count, got {faces}");
        assert_eq!((faces - 2) % 16, 0, "whole side spans + 2 caps");
    }

    #[test]
    fn a_swept_disk_line_directrix_uses_parameter_trims() {
        // A line directrix trimmed by parameter (not cartesian points): the point at
        // parameter u is Pnt + u·Dir. Trims 0 → 3 along a unit +Z vector give a 3 m
        // straight pipe — the same 18 faces a cartesian straight run gives.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#10=IFCCARTESIANPOINT((0.,0.,0.));
#11=IFCDIRECTION((0.,0.,1.));
#12=IFCVECTOR(#11,1.);
#13=IFCLINE(#10,#12);
#14=IFCTRIMMEDCURVE(#13,(IFCPARAMETERVALUE(0.)),(IFCPARAMETERVALUE(3.)),.T.,.PARAMETER.);
#50=IFCSWEPTDISKSOLID(#14,0.5);
#78=IFCSHAPEREPRESENTATION($,'Body','AdvancedSweptSolid',(#50));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#78));
#45=IFCMEMBER('p',$,'ParamLinePipe',$,$,$,#48,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let g = import_ifc_geometry(src).unwrap();
        assert_eq!(g.elements[0].faces.len(), 18, "16 side quads + 2 caps, 3 m run");
    }

    /// A wall (4 x 0.2 x 3 m) with a window cut through it, written the way real
    /// BIM tools write an opening: an `IfcBooleanClippingResult` of the wall solid
    /// minus an opening solid. The opening's own `Position` places it at sill
    /// height and it is thicker than the wall so it punches clean through.
    fn wall_with_window_ifc() -> String {
        "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#50=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#60=IFCCARTESIANPOINT((0.,0.,0.8));
#61=IFCAXIS2PLACEMENT3D(#60,$,$);
#62=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,1.,0.4);
#63=IFCEXTRUDEDAREASOLID(#62,#61,$,1.2);
#70=IFCBOOLEANCLIPPINGRESULT(.DIFFERENCE.,#50,#63);
#51=IFCSHAPEREPRESENTATION($,'Body','CSG',(#70));
#52=IFCPRODUCTDEFINITIONSHAPE($,$,(#51));
#53=IFCWALL('w',$,'CSG',$,$,$,#52,$,$);
ENDSEC;
END-ISO-10303-21;
"
        .to_string()
    }

    #[test]
    fn a_boolean_clipping_result_parses_as_a_subtract_of_two_solids() {
        // The wall-with-opening case. The member carries no plain faces — its
        // shape is the boolean tree, which must survive as a Subtract of two
        // buildable solids (an empty `booleans` would silently drop the wall).
        let g = import_ifc_geometry(&wall_with_window_ifc()).unwrap();
        let el = &g.elements[0];
        assert!(el.faces.is_empty(), "the shape is a boolean, not bare faces");
        assert_eq!(el.booleans.len(), 1, "one IfcBooleanClippingResult");

        let node = &el.booleans[0];
        assert_eq!(node.op, BoolOp::Subtract, ".DIFFERENCE. → Subtract");
        // Both operands are extruded prisms (6 faces each) — real solids, not
        // nested booleans and not half-space clips.
        for operand in [&node.first, &node.second] {
            match operand {
                CsgOperand::Solid(loops) => assert_eq!(loops.len(), 6, "a rectangular prism"),
                _ => panic!("both operands are plain solids here"),
            }
        }
    }

    #[test]
    fn an_unbounded_half_space_operand_parses_as_a_planar_clip() {
        // A wall clipped by an unbounded IfcHalfSpaceSolid: the second operand is
        // now readable — a plane (origin + normal) with an AgreementFlag, and no
        // lateral boundary.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#50=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#60=IFCCARTESIANPOINT((0.,0.,1.5));
#61=IFCDIRECTION((0.,0.,1.));
#62=IFCAXIS2PLACEMENT3D(#60,#61,$);
#63=IFCPLANE(#62);
#64=IFCHALFSPACESOLID(#63,.F.);
#70=IFCBOOLEANCLIPPINGRESULT(.DIFFERENCE.,#50,#64);
#51=IFCSHAPEREPRESENTATION($,'Body','CSG',(#70));
#52=IFCPRODUCTDEFINITIONSHAPE($,$,(#51));
#53=IFCWALL('w',$,'CSG',$,$,$,#52,$,$);
ENDSEC;
END-ISO-10303-21;
"
        .to_string();
        let g = import_ifc_geometry(&src).unwrap();
        let node = &g.elements[0].booleans[0];
        assert_eq!(node.op, BoolOp::Subtract);
        match &node.second {
            CsgOperand::HalfSpace(hs) => {
                assert!(hs.boundary.is_none(), "an unbounded half-space has no polygon");
                assert!(!hs.agreement, ".F. → AgreementFlag false");
                // Plane at z=1500 mm, +Z normal (metres → mm).
                assert!((hs.base_origin.z - 1500.0).abs() < 1.0, "base z {}", hs.base_origin.z);
                assert!((hs.base_normal.z - 1.0).abs() < 1e-6, "normal +Z {:?}", hs.base_normal);
            }
            _ => panic!("second operand is a half-space clip"),
        }
    }

    #[test]
    fn a_polygonal_bounded_half_space_operand_parses_with_its_polygon() {
        // The common real clip (a channel, a sloped cut): IfcPolygonalBoundedHalfSpace
        // carries a base plane *and* a lateral boundary polygon in world space.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#50=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#59=IFCDIRECTION((0.,0.,1.));
#60=IFCCARTESIANPOINT((0.,0.,1.5));
#61=IFCAXIS2PLACEMENT3D(#60,#59,$);
#62=IFCPLANE(#61);
#170=IFCCARTESIANPOINT((-1.,-1.));
#171=IFCCARTESIANPOINT((1.,-1.));
#172=IFCCARTESIANPOINT((1.,1.));
#173=IFCCARTESIANPOINT((-1.,1.));
#174=IFCPOLYLINE((#170,#171,#172,#173,#170));
#175=IFCCARTESIANPOINT((0.,0.,0.));
#176=IFCAXIS2PLACEMENT3D(#175,$,$);
#43=IFCPOLYGONALBOUNDEDHALFSPACE(#62,.F.,#176,#174);
#80=IFCBOOLEANCLIPPINGRESULT(.DIFFERENCE.,#50,#43);
#51=IFCSHAPEREPRESENTATION($,'Body','CSG',(#80));
#52=IFCPRODUCTDEFINITIONSHAPE($,$,(#51));
#53=IFCWALL('w',$,'CSG',$,$,$,#52,$,$);
ENDSEC;
END-ISO-10303-21;
"
        .to_string();
        let g = import_ifc_geometry(&src).unwrap();
        match &g.elements[0].booleans[0].second {
            CsgOperand::HalfSpace(hs) => {
                let (poly, dir) = hs.boundary.as_ref().expect("a bounded half-space has a polygon");
                assert_eq!(poly.len(), 4, "the square boundary has four corners");
                assert!((dir.z - 1.0).abs() < 1e-6, "extrude perpendicular to XY {:?}", dir);
                assert!((hs.base_origin.z - 1500.0).abs() < 1.0);
            }
            _ => panic!("second operand is a bounded half-space"),
        }
    }

    #[test]
    fn a_curved_base_surface_half_space_is_unreadable() {
        // We only clip by a *plane*. A half-space on a curved base surface (here a
        // cylinder) is refused — reported, never guessed at.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#50=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#60=IFCCARTESIANPOINT((0.,0.,0.));
#61=IFCAXIS2PLACEMENT3D(#60,$,$);
#63=IFCCYLINDRICALSURFACE(#61,1.);
#64=IFCHALFSPACESOLID(#63,.F.);
#70=IFCBOOLEANCLIPPINGRESULT(.DIFFERENCE.,#50,#64);
#51=IFCSHAPEREPRESENTATION($,'Body','CSG',(#70));
#52=IFCPRODUCTDEFINITIONSHAPE($,$,(#51));
#53=IFCWALL('w',$,'CSG',$,$,$,#52,$,$);
ENDSEC;
END-ISO-10303-21;
"
        .to_string();
        let g = import_ifc_geometry(&src).unwrap();
        assert!(g.elements.is_empty(), "a curved base surface is not buildable");
        assert!(
            g.warnings.iter().any(|w| w.contains("boolean geometry")),
            "the unreadable boolean is warned about: {:?}",
            g.warnings
        );
    }

    /// A wall with an opening tied to it by IfcRelVoidsElement — the opening is a
    /// separate IfcOpeningElement, placed relative to the wall, that must be cut
    /// out so the wall gets a real hole. The synthesized shape is a Subtract of
    /// the wall solid minus the opening solid, and the wall's plain faces move
    /// into it (so the wall isn't imported solid *and* holed).
    fn wall_voided_by_opening_ifc() -> String {
        "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#24=IFCCARTESIANPOINT((0.,0.,0.));
#47=IFCAXIS2PLACEMENT3D(#24,$,$);
#46=IFCLOCALPLACEMENT($,#47);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#71=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#70=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#71));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#70));
#45=IFCWALL('w',$,'Wall',$,$,#46,#48,$,$);
#83=IFCCARTESIANPOINT((1.,0.,0.8));
#82=IFCAXIS2PLACEMENT3D(#83,$,$);
#81=IFCLOCALPLACEMENT(#46,#82);
#88=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,1.,0.4);
#87=IFCEXTRUDEDAREASOLID(#88,$,$,1.);
#86=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#87));
#84=IFCPRODUCTDEFINITIONSHAPE($,$,(#86));
#80=IFCOPENINGELEMENT('o',$,'Opening',$,$,#81,#84,$,.OPENING.);
#85=IFCRELVOIDSELEMENT('rv',$,$,$,#45,#80);
ENDSEC;
END-ISO-10303-21;
"
        .to_string()
    }

    #[test]
    fn a_rel_voids_element_synthesizes_a_subtract_of_wall_minus_opening() {
        let g = import_ifc_geometry(&wall_voided_by_opening_ifc()).unwrap();
        // Only the wall is a member — the opening is a void, never imported alone.
        assert_eq!(g.elements.len(), 1, "the opening is not a standalone member");
        let el = &g.elements[0];
        assert!(el.faces.is_empty(), "the wall's solid moved into the void boolean");
        assert_eq!(el.booleans.len(), 1, "one synthesized void boolean");

        let node = &el.booleans[0];
        assert_eq!(node.op, BoolOp::Subtract, "a void is a subtraction");
        // wall solid − opening solid, both six-faced prisms.
        match (&node.first, &node.second) {
            (CsgOperand::Solid(wall), CsgOperand::Solid(opening)) => {
                assert_eq!(wall.len(), 6, "the wall prism");
                assert_eq!(opening.len(), 6, "the opening prism");
            }
            _ => panic!("wall minus opening, both plain solids"),
        }
    }

    #[test]
    fn a_window_filling_an_opening_records_the_wall_it_belongs_to() {
        // IfcRelFillsElement(opening, window) ∘ IfcRelVoidsElement(wall, opening)
        // → the window's fills_wall is the wall, so it can be grouped under it.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSIUNIT(*,.LENGTHUNIT.,$,.METRE.);
#24=IFCCARTESIANPOINT((0.,0.,0.));
#47=IFCAXIS2PLACEMENT3D(#24,$,$);
#46=IFCLOCALPLACEMENT($,#47);
#40=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4.,0.2);
#71=IFCEXTRUDEDAREASOLID(#40,$,$,3.);
#70=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#71));
#48=IFCPRODUCTDEFINITIONSHAPE($,$,(#70));
#45=IFCWALL('w',$,'Wall',$,$,#46,#48,$,$);
#83=IFCCARTESIANPOINT((1.,0.,0.8));
#82=IFCAXIS2PLACEMENT3D(#83,$,$);
#81=IFCLOCALPLACEMENT(#46,#82);
#88=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,1.,0.4);
#87=IFCEXTRUDEDAREASOLID(#88,$,$,1.);
#86=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#87));
#84=IFCPRODUCTDEFINITIONSHAPE($,$,(#86));
#80=IFCOPENINGELEMENT('o',$,'Opening',$,$,#81,#84,$,.OPENING.);
#85=IFCRELVOIDSELEMENT('rv',$,$,$,#45,#80);
#103=IFCLOCALPLACEMENT(#81,#82);
#92=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,0.9,0.1);
#91=IFCEXTRUDEDAREASOLID(#92,$,$,0.9);
#90=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#91));
#89=IFCPRODUCTDEFINITIONSHAPE($,$,(#90));
#102=IFCWINDOW('win',$,'Window',$,$,#103,#89,$,0.9,0.9,$,$,$);
#112=IFCRELFILLSELEMENT('rf',$,$,$,#80,#102);
ENDSEC;
END-ISO-10303-21;
"
        .to_string();
        let g = import_ifc_geometry(&src).unwrap();
        // Wall and window import; the opening does not.
        let wall = g.elements.iter().find(|e| e.element_id == 45).expect("wall imported");
        let window = g.elements.iter().find(|e| e.element_id == 102).expect("window imported");
        assert_eq!(wall.fills_wall, None, "the wall fills nothing");
        assert_eq!(window.fills_wall, Some(45), "the window belongs to the wall it fills");
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
            closed_curve: None,
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
            closed_curve: None,
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
            closed_curve: None,
        };
        assert!(line.plane().is_none(), "collinear loop has no plane");

        let two = FaceLoops {
            outer: vec![DVec3::ZERO, DVec3::X],
            inners: vec![],
            closed_curve: None,
        };
        assert!(two.plane().is_none(), "2 points cannot span a plane");
    }
}

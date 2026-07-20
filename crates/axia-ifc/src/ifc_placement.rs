//! `IfcLocalPlacement` chains (ADR-203 I-4) — putting members where they belong.
//!
//! I-3 read B-rep points as world coordinates. That is right for our own files
//! (we bake world coordinates and emit an identity placement) and wrong for
//! every real BIM file: Revit and ArchiCAD write geometry in the member's own
//! coordinate system and locate it with a chain of placements.
//!
//! ```text
//! IfcWall.ObjectPlacement → IfcLocalPlacement ─RelativePlacement→ IfcAxis2Placement3D
//!                                             └PlacementRelTo→ IfcLocalPlacement (storey)
//!                                                              └PlacementRelTo→ … (building, site)
//! ```
//!
//! Each `IfcAxis2Placement3D` gives an origin, a Z axis and a reference X. This
//! module walks the chain to the root and composes them into one transform, so a
//! wall drawn at its own origin lands on the right storey at the right spot.
//!
//! Missing or malformed links resolve to identity rather than an error — a file
//! with a broken placement should still import its geometry, just unplaced.

use axia_foreign::step_parser::{Entity, StepFile};
use glam::DVec3;

/// A rigid placement: an orthonormal basis plus an origin.
///
/// Deliberately not a 4×4 — IFC placements are rigid (no scale, no shear), and
/// keeping the axes explicit makes the composition read like the spec.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placement {
    pub origin: DVec3,
    pub x: DVec3,
    pub y: DVec3,
    pub z: DVec3,
}

impl Default for Placement {
    fn default() -> Self {
        Self::IDENTITY
    }
}

impl Placement {
    pub const IDENTITY: Self = Self {
        origin: DVec3::ZERO,
        x: DVec3::X,
        y: DVec3::Y,
        z: DVec3::Z,
    };

    /// Is this the identity? Used to keep our own (already-world) files on the
    /// zero-cost path and to spot a context that actually needs applying.
    pub fn is_identity(&self) -> bool {
        const EPS: f64 = 1e-12;
        (self.origin - DVec3::ZERO).length_squared() < EPS
            && (self.x - DVec3::X).length_squared() < EPS
            && (self.y - DVec3::Y).length_squared() < EPS
            && (self.z - DVec3::Z).length_squared() < EPS
    }

    /// Local point → parent coordinates.
    pub fn apply(&self, p: DVec3) -> DVec3 {
        self.origin + self.x * p.x + self.y * p.y + self.z * p.z
    }

    /// `self` expressed in `parent`'s coordinates — the chain composition.
    ///
    /// The child's origin is a point (it moves), its axes are directions (they
    /// only rotate), which is why the origin goes through `apply` and the axes
    /// through `rotate`.
    pub fn then(&self, parent: &Placement) -> Placement {
        Placement {
            origin: parent.apply(self.origin),
            x: parent.rotate(self.x),
            y: parent.rotate(self.y),
            z: parent.rotate(self.z),
        }
    }

    fn rotate(&self, d: DVec3) -> DVec3 {
        self.x * d.x + self.y * d.y + self.z * d.z
    }
}

/// Read an `IfcAxis2Placement3D` (or 2D) into a [`Placement`].
///
/// `Axis` (Z) and `RefDirection` (X) are both optional in IFC; either missing
/// means "use the default", and a RefDirection that is not perpendicular to the
/// axis is projected — the spec says the X direction is the RefDirection
/// component orthogonal to the axis, not the RefDirection itself.
pub fn axis_placement(file: &StepFile, id: u32, scale: f64) -> Option<Placement> {
    let e = file.entity(id)?;
    let tag = e.tag.to_ascii_uppercase();
    if tag != "IFCAXIS2PLACEMENT3D" && tag != "IFCAXIS2PLACEMENT2D" {
        return None;
    }

    let origin = e
        .args
        .first()
        .and_then(|v| v.as_ref())
        .and_then(|pid| cartesian_point(file, pid, scale))
        .unwrap_or(DVec3::ZERO);

    // 2D placements carry RefDirection at index 1 and no axis.
    let (axis, ref_dir) = if tag == "IFCAXIS2PLACEMENT2D" {
        (None, e.args.get(1).and_then(|v| v.as_ref()))
    } else {
        (
            e.args.get(1).and_then(|v| v.as_ref()),
            e.args.get(2).and_then(|v| v.as_ref()),
        )
    };

    let z = axis
        .and_then(|d| direction(file, d))
        .unwrap_or(DVec3::Z)
        .normalize_or_zero();
    let z = if z.length_squared() < 1e-18 { DVec3::Z } else { z };

    let raw_x = ref_dir
        .and_then(|d| direction(file, d))
        .unwrap_or(DVec3::X);
    // Project the reference direction into the plane normal to the axis.
    let mut x = raw_x - z * raw_x.dot(z);
    if x.length_squared() < 1e-18 {
        // Degenerate reference (parallel to the axis, or zero) — pick any
        // perpendicular so the basis stays orthonormal instead of collapsing.
        x = if z.x.abs() < 0.9 { DVec3::X } else { DVec3::Y };
        x -= z * x.dot(z);
    }
    let x = x.normalize();
    let y = z.cross(x);

    Some(Placement { origin, x, y, z })
}

/// Walk an `IfcLocalPlacement` chain to the root and compose it.
///
/// Depth is capped: a file with a placement cycle should not hang the import.
pub fn resolve_placement(file: &StepFile, id: u32, scale: f64) -> Placement {
    const MAX_DEPTH: usize = 64;

    let mut chain: Vec<Placement> = Vec::new();
    let mut current = Some(id);
    let mut seen: Vec<u32> = Vec::new();

    while let Some(pid) = current {
        if chain.len() >= MAX_DEPTH || seen.contains(&pid) {
            break; // cycle or absurd depth — stop with what we have
        }
        seen.push(pid);

        let Some(e) = file.entity(pid) else { break };
        if !e.tag.eq_ignore_ascii_case("IFCLOCALPLACEMENT") {
            // IfcGridPlacement and friends are not supported; treat as identity.
            break;
        }
        // IfcLocalPlacement(PlacementRelTo, RelativePlacement)
        let rel = e.args.get(1).and_then(|v| v.as_ref());
        chain.push(
            rel.and_then(|r| axis_placement(file, r, scale))
                .unwrap_or(Placement::IDENTITY),
        );
        current = e.args.first().and_then(|v| v.as_ref());
    }

    // chain[0] is the leaf; fold outward so each is expressed in its parent.
    let mut out = Placement::IDENTITY;
    for p in chain.iter().rev() {
        out = p.then(&out);
    }
    out
}

/// The project's `WorldCoordinateSystem`, if it is not the identity.
///
/// `IfcGeometricRepresentationContext(ContextIdentifier, ContextType,
/// CoordinateSpaceDimension, Precision, WorldCoordinateSystem, TrueNorth)` —
/// index 4. Nearly every exporter writes the identity here; returning `Some`
/// only for a real transform lets the caller warn instead of silently ignoring
/// a file whose whole model is offset.
pub fn world_coordinate_system(file: &StepFile, scale: f64) -> Option<Placement> {
    for (_, e) in file.iter_entities() {
        if !e.tag.eq_ignore_ascii_case("IFCGEOMETRICREPRESENTATIONCONTEXT") {
            continue;
        }
        let wcs = e.args.get(4).and_then(|v| v.as_ref())?;
        let p = axis_placement(file, wcs, scale)?;
        if !p.is_identity() {
            return Some(p);
        }
    }
    None
}

/// `IfcCartesianPoint` → mm.
fn cartesian_point(file: &StepFile, id: u32, scale: f64) -> Option<DVec3> {
    let e = file.entity(id)?;
    if !e.tag.eq_ignore_ascii_case("IFCCARTESIANPOINT") {
        return None;
    }
    coords(e).map(|(x, y, z)| DVec3::new(x * scale, y * scale, z * scale))
}

/// `IfcDirection` → unit-ish vector (direction ratios are unitless).
fn direction(file: &StepFile, id: u32) -> Option<DVec3> {
    let e = file.entity(id)?;
    if !e.tag.eq_ignore_ascii_case("IFCDIRECTION") {
        return None;
    }
    coords(e).map(|(x, y, z)| DVec3::new(x, y, z))
}

fn coords(e: &Entity) -> Option<(f64, f64, f64)> {
    let list = e.args.first()?.as_list()?;
    let mut it = list.iter().filter_map(|v| v.as_f64());
    let x = it.next()?;
    let y = it.next().unwrap_or(0.0);
    let z = it.next().unwrap_or(0.0);
    Some((x, y, z))
}

#[cfg(test)]
mod tests {
    use super::*;
    use axia_foreign::step_parser;

    fn parse(body: &str) -> StepFile {
        let src = format!(
            "ISO-10303-21;\nHEADER;\nFILE_SCHEMA(('IFC4X3'));\nENDSEC;\nDATA;\n{body}ENDSEC;\nEND-ISO-10303-21;\n"
        );
        step_parser::parse(&src).expect("fixture parses")
    }

    #[test]
    fn identity_placement_leaves_points_alone() {
        let f = parse(
            "#1=IFCCARTESIANPOINT((0.,0.,0.));\n\
             #2=IFCAXIS2PLACEMENT3D(#1,$,$);\n\
             #3=IFCLOCALPLACEMENT($,#2);\n",
        );
        let p = resolve_placement(&f, 3, 1000.0);
        assert!(p.is_identity(), "{p:?}");
        assert_eq!(p.apply(DVec3::new(1.0, 2.0, 3.0)), DVec3::new(1.0, 2.0, 3.0));
    }

    #[test]
    fn translation_moves_the_point_and_scales_with_units() {
        // Origin is in file units; metres → mm here.
        let f = parse(
            "#1=IFCCARTESIANPOINT((1.,2.,3.));\n\
             #2=IFCAXIS2PLACEMENT3D(#1,$,$);\n\
             #3=IFCLOCALPLACEMENT($,#2);\n",
        );
        let p = resolve_placement(&f, 3, 1000.0);
        assert_eq!(p.origin, DVec3::new(1000.0, 2000.0, 3000.0));
        // The local point is already in mm — placement only adds the offset.
        assert_eq!(p.apply(DVec3::new(500.0, 0.0, 0.0)), DVec3::new(1500.0, 2000.0, 3000.0));
    }

    #[test]
    fn rotation_about_z_turns_the_axes() {
        // RefDirection +Y ⇒ the local X axis points along world +Y (90° yaw).
        let f = parse(
            "#1=IFCCARTESIANPOINT((0.,0.,0.));\n\
             #2=IFCDIRECTION((0.,0.,1.));\n\
             #3=IFCDIRECTION((0.,1.,0.));\n\
             #4=IFCAXIS2PLACEMENT3D(#1,#2,#3);\n\
             #5=IFCLOCALPLACEMENT($,#4);\n",
        );
        let p = resolve_placement(&f, 5, 1000.0);
        assert!((p.x - DVec3::Y).length() < 1e-12, "x={:?}", p.x);
        assert!((p.y - -DVec3::X).length() < 1e-12, "y={:?}", p.y);
        assert!((p.apply(DVec3::new(10.0, 0.0, 0.0)) - DVec3::new(0.0, 10.0, 0.0)).length() < 1e-12);
    }

    #[test]
    fn a_chain_composes_parent_then_child() {
        // Storey at z=3000 (3 m), wall offset (1,0,0) m within the storey.
        let f = parse(
            "#1=IFCCARTESIANPOINT((0.,0.,3.));\n\
             #2=IFCAXIS2PLACEMENT3D(#1,$,$);\n\
             #3=IFCLOCALPLACEMENT($,#2);\n\
             #4=IFCCARTESIANPOINT((1.,0.,0.));\n\
             #5=IFCAXIS2PLACEMENT3D(#4,$,$);\n\
             #6=IFCLOCALPLACEMENT(#3,#5);\n",
        );
        let p = resolve_placement(&f, 6, 1000.0);
        assert_eq!(p.origin, DVec3::new(1000.0, 0.0, 3000.0), "storey + wall offsets add");
    }

    #[test]
    fn rotated_parent_rotates_the_child_offset() {
        // Parent yawed 90°, child offset +X locally ⇒ world +Y.
        let f = parse(
            "#1=IFCCARTESIANPOINT((0.,0.,0.));\n\
             #2=IFCDIRECTION((0.,0.,1.));\n\
             #3=IFCDIRECTION((0.,1.,0.));\n\
             #4=IFCAXIS2PLACEMENT3D(#1,#2,#3);\n\
             #5=IFCLOCALPLACEMENT($,#4);\n\
             #6=IFCCARTESIANPOINT((2.,0.,0.));\n\
             #7=IFCAXIS2PLACEMENT3D(#6,$,$);\n\
             #8=IFCLOCALPLACEMENT(#5,#7);\n",
        );
        let p = resolve_placement(&f, 8, 1000.0);
        assert!(
            (p.origin - DVec3::new(0.0, 2000.0, 0.0)).length() < 1e-9,
            "child offset is rotated by the parent: {:?}",
            p.origin
        );
    }

    #[test]
    fn non_perpendicular_reference_is_projected_not_trusted() {
        // RefDirection (1,0,1) with axis +Z ⇒ X must come out as +X.
        let f = parse(
            "#1=IFCCARTESIANPOINT((0.,0.,0.));\n\
             #2=IFCDIRECTION((0.,0.,1.));\n\
             #3=IFCDIRECTION((1.,0.,1.));\n\
             #4=IFCAXIS2PLACEMENT3D(#1,#2,#3);\n",
        );
        let p = axis_placement(&f, 4, 1000.0).expect("placement");
        assert!((p.x - DVec3::X).length() < 1e-12, "x={:?}", p.x);
        assert!(p.x.dot(p.z).abs() < 1e-12, "orthonormal");
        assert!((p.y - DVec3::Y).length() < 1e-12, "y={:?}", p.y);
    }

    #[test]
    fn reference_parallel_to_axis_still_yields_an_orthonormal_basis() {
        // Degenerate input: RefDirection == Axis. Must not collapse to zero.
        let f = parse(
            "#1=IFCCARTESIANPOINT((0.,0.,0.));\n\
             #2=IFCDIRECTION((0.,0.,1.));\n\
             #3=IFCDIRECTION((0.,0.,1.));\n\
             #4=IFCAXIS2PLACEMENT3D(#1,#2,#3);\n",
        );
        let p = axis_placement(&f, 4, 1000.0).expect("placement");
        assert!(p.x.length() > 0.99 && p.y.length() > 0.99, "{p:?}");
        assert!(p.x.dot(p.z).abs() < 1e-12 && p.x.dot(p.y).abs() < 1e-12);
    }

    #[test]
    fn broken_or_cyclic_chains_fall_back_to_identity() {
        // Dangling reference.
        let f = parse("#1=IFCLOCALPLACEMENT($,#99);\n");
        assert!(resolve_placement(&f, 1, 1000.0).is_identity());

        // Missing entity entirely.
        assert!(resolve_placement(&f, 42, 1000.0).is_identity());

        // A cycle must terminate rather than hang.
        let f = parse(
            "#1=IFCCARTESIANPOINT((1.,0.,0.));\n\
             #2=IFCAXIS2PLACEMENT3D(#1,$,$);\n\
             #3=IFCLOCALPLACEMENT(#4,#2);\n\
             #4=IFCLOCALPLACEMENT(#3,#2);\n",
        );
        let p = resolve_placement(&f, 3, 1000.0);
        assert!(p.origin.is_finite(), "terminated with a usable result: {p:?}");
    }

    #[test]
    fn non_local_placement_is_treated_as_identity() {
        // IfcGridPlacement is legal IFC we do not support.
        let f = parse("#1=IFCGRIDPLACEMENT($,$);\n");
        assert!(resolve_placement(&f, 1, 1000.0).is_identity());
    }

    #[test]
    fn identity_world_coordinate_system_is_not_reported() {
        let f = parse(
            "#1=IFCCARTESIANPOINT((0.,0.,0.));\n\
             #2=IFCAXIS2PLACEMENT3D(#1,$,$);\n\
             #3=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#2,$);\n",
        );
        assert!(world_coordinate_system(&f, 1000.0).is_none(), "identity is silent");

        let f = parse(
            "#1=IFCCARTESIANPOINT((5.,0.,0.));\n\
             #2=IFCAXIS2PLACEMENT3D(#1,$,$);\n\
             #3=IFCGEOMETRICREPRESENTATIONCONTEXT($,'Model',3,1.E-05,#2,$);\n",
        );
        let wcs = world_coordinate_system(&f, 1000.0).expect("non-identity is reported");
        assert_eq!(wcs.origin, DVec3::new(5000.0, 0.0, 0.0));
    }
}

//! IfcFacetedBrep emitter (ADR-203 β-1 box, β-1.5 live-scene mesh).
//!
//! [`emit_brep`] is the shared core: it emits `points` + polygonal `face_loops`
//! as a true-IFC4X3 `IFCFACETEDBREP` closed shell wrapped in a minimal spatial
//! hierarchy (Project→Site→Building→Storey, one IfcWall). [`emit_box`] feeds it
//! 8 points + 6 quads (β-1 proof); [`emit_faceted_brep`] feeds it a tessellated
//! triangle soup from the live engine (β-1.5). The owner/units/context prologue
//! and product/spatial epilogue are shared with the analytic `IfcAdvancedBrep`
//! emitter (β-2) via [`crate::ifc_common`].

use crate::ifc_common::{emit_owner_units_context, emit_product_and_spatial, pt};
use crate::step_value::{EntityRef, StepValue};
use crate::step_writer::StepWriter;
use glam::DVec3;

/// Shared core: emit `points` + polygonal `face_loops` (each a CCW list of
/// vertex indices into `points`) as a complete IFC4.3 `IFCFACETEDBREP` file.
/// `name` labels the single IfcWall. Coordinates are in the IFC unit (metre);
/// callers convert from engine mm. Shared by [`emit_box`] (6 quads) and
/// [`emit_faceted_brep`] (tessellated triangles, ADR-203 β-1.5).
pub fn emit_brep(points: &[DVec3], face_loops: &[Vec<usize>], name: &str) -> String {
    let mut w = StepWriter::new();
    w.file_description = format!("AXiA IFC4.3 '{}' (IfcFacetedBrep, ADR-203)", name);
    w.file_name = format!("{}.ifc", name);

    // ── Owner / units / context (shared scaffold) ──
    let sc = emit_owner_units_context(&mut w);

    // ── Geometry: points + polygonal faces ──
    let verts: Vec<EntityRef> = points.iter().map(|&p| pt(&mut w, p)).collect();
    let mut faces = Vec::with_capacity(face_loops.len());
    for loop_idx in face_loops {
        let loop_ = w.add(
            "IFCPOLYLOOP",
            vec![StepValue::List(
                loop_idx.iter().map(|&i| StepValue::Ref(verts[i])).collect(),
            )],
        );
        let bound = w.add(
            "IFCFACEOUTERBOUND",
            vec![StepValue::Ref(loop_), StepValue::Enum("T".into())],
        );
        let face = w.add("IFCFACE", vec![StepValue::List(vec![StepValue::Ref(bound)])]);
        faces.push(face);
    }
    let shell = w.add(
        "IFCCLOSEDSHELL",
        vec![StepValue::List(faces.iter().map(|&f| StepValue::Ref(f)).collect())],
    );
    let brep = w.add("IFCFACETEDBREP", vec![StepValue::Ref(shell)]);

    // ── Product + spatial hierarchy (shared scaffold) ──
    emit_product_and_spatial(&mut w, &sc, name, brep, "Brep");

    w.build()
}

/// Emit a box `[min, max]` as a complete IFC4.3 file. `name` labels the wall.
pub fn emit_box(min: DVec3, max: DVec3, name: &str) -> String {
    let (a, b) = (min, max);
    let points = [
        DVec3::new(a.x, a.y, a.z), // 0
        DVec3::new(b.x, a.y, a.z), // 1
        DVec3::new(b.x, b.y, a.z), // 2
        DVec3::new(a.x, b.y, a.z), // 3
        DVec3::new(a.x, a.y, b.z), // 4
        DVec3::new(b.x, a.y, b.z), // 5
        DVec3::new(b.x, b.y, b.z), // 6
        DVec3::new(a.x, b.y, b.z), // 7
    ];
    // 6 faces, CCW outward. (bottom, top, front, back, left, right)
    let face_loops = vec![
        vec![0, 3, 2, 1], // bottom (-Z)
        vec![4, 5, 6, 7], // top (+Z)
        vec![0, 1, 5, 4], // front (-Y)
        vec![2, 3, 7, 6], // back (+Y)
        vec![0, 4, 7, 3], // left (-X)
        vec![1, 2, 6, 5], // right (+X)
    ];
    emit_brep(&points, &face_loops, name)
}

/// Emit a tessellated mesh — `positions` (world verts) + `tris` (triangle index
/// triples into `positions`) — as an IfcFacetedBrep file. This is the live-scene
/// export (ADR-203 β-1.5): the caller passes the engine's render tessellation
/// (curved faces already faceted), converted to the IFC unit (metre).
pub fn emit_faceted_brep(positions: &[DVec3], tris: &[[u32; 3]], name: &str) -> String {
    let face_loops: Vec<Vec<usize>> = tris
        .iter()
        .map(|t| vec![t[0] as usize, t[1] as usize, t[2] as usize])
        .collect();
    emit_brep(positions, &face_loops, name)
}

/// Unit cube `[0,1]³` named "cube".
pub fn emit_unit_cube() -> String {
    emit_box(DVec3::ZERO, DVec3::ONE, "cube")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rebuild(text: &str) -> (usize, usize) {
        // count DATA entities + max #N referenced
        let mut count = 0;
        for line in text.lines() {
            if line.starts_with('#') && line.contains('=') {
                count += 1;
            }
        }
        (count, 0)
    }

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

    #[test]
    fn unit_cube_well_formed_structure() {
        let s = emit_unit_cube();
        // ISO-10303-21 skeleton
        assert!(s.starts_with("ISO-10303-21;"));
        assert!(s.contains("FILE_SCHEMA(('IFC4X3'));"));
        assert!(s.trim_end().ends_with("END-ISO-10303-21;"));
        // true IFC4X3 entity names (not STEP AP203)
        assert!(s.contains("IFCCARTESIANPOINT"));
        assert!(s.contains("IFCFACETEDBREP"));
        assert!(s.contains("IFCWALL"));
        assert!(!s.contains("\nCARTESIAN_POINT"), "must be IFC names, not bare STEP");
        // 8 points, 6 polyloops, 6 faces, 1 shell, 1 brep
        assert_eq!(s.matches("=IFCCARTESIANPOINT(").count(), 8 + 1 /* world origin */);
        assert_eq!(s.matches("=IFCPOLYLOOP(").count(), 6);
        assert_eq!(s.matches("=IFCFACE(").count(), 6);
        assert_eq!(s.matches("=IFCFACEOUTERBOUND(").count(), 6);
        assert_eq!(s.matches("=IFCCLOSEDSHELL(").count(), 1);
        assert_eq!(s.matches("=IFCFACETEDBREP(").count(), 1);
        let (n, _) = rebuild(&s);
        assert!(n >= 40, "≈43 entities, got {}", n);
    }

    #[test]
    fn unit_cube_refs_resolve() {
        assert_refs_resolve(&emit_unit_cube());
    }

    #[test]
    fn unit_cube_byte_identical() {
        assert_eq!(emit_unit_cube(), emit_unit_cube(), "deterministic (L-203-2)");
    }

    #[test]
    fn box_coords_present() {
        let s = emit_box(DVec3::new(0.0, 0.0, 0.0), DVec3::new(2.0, 3.0, 4.0), "b");
        // corner (2,3,4) and (0,0,0) appear as IFCCARTESIANPOINT coords
        assert!(s.contains("IFCCARTESIANPOINT((2.,3.,4.))"));
        assert!(s.contains("IFCCARTESIANPOINT((0.,0.,0.))"));
    }

    // ── β-1.5: live-scene tessellated mesh export ──

    #[test]
    fn faceted_brep_tetrahedron_well_formed() {
        // 4 verts, 4 triangular faces (a tetra soup, indexed like the render buffers).
        let pts = [
            DVec3::new(0.0, 0.0, 0.0),
            DVec3::new(1.0, 0.0, 0.0),
            DVec3::new(0.0, 1.0, 0.0),
            DVec3::new(0.0, 0.0, 1.0),
        ];
        let tris = [[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        let s = emit_faceted_brep(&pts, &tris, "tetra");
        assert!(s.contains("FILE_SCHEMA(('IFC4X3'));"));
        assert!(s.contains("=IFCFACETEDBREP("));
        assert!(s.contains("IFCWALL"));
        // 4 mesh points (+1 world origin), 4 triangle polyloops/faces
        assert_eq!(s.matches("=IFCCARTESIANPOINT(").count(), 4 + 1);
        assert_eq!(s.matches("=IFCPOLYLOOP(").count(), 4);
        assert_eq!(s.matches("=IFCFACE(").count(), 4);
        assert_eq!(s.matches("=IFCCLOSEDSHELL(").count(), 1);
        assert_refs_resolve(&s);
        // triangle vertex coords present
        assert!(s.contains("IFCCARTESIANPOINT((0.,0.,1.))"));
    }

    #[test]
    fn faceted_brep_byte_identical() {
        let pts = [DVec3::ZERO, DVec3::X, DVec3::Y, DVec3::Z];
        let tris = [[0u32, 2, 1], [0, 1, 3], [0, 3, 2], [1, 2, 3]];
        assert_eq!(
            emit_faceted_brep(&pts, &tris, "t"),
            emit_faceted_brep(&pts, &tris, "t"),
            "deterministic (L-203-2)"
        );
    }

    #[test]
    fn box_via_emit_brep_matches_faces() {
        // emit_box still produces exactly 6 quad faces through the shared core.
        let s = emit_box(DVec3::ZERO, DVec3::new(1.0, 1.0, 1.0), "b");
        assert_eq!(s.matches("=IFCPOLYLOOP(").count(), 6);
        assert_eq!(s.matches("=IFCCARTESIANPOINT(").count(), 8 + 1);
    }
}

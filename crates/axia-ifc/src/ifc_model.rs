//! Multi-element IFC model (ADR-203 γ) — one `IfcWall` per semantic member
//! (Xia / Shape) with its own analytic geometry + `IfcMaterial`, all under one
//! `Project → Site → Building → Storey`.
//!
//! Where [`crate::emit_advanced_brep`] emits the whole mesh as a single wall,
//! [`emit_ifc_model`] takes a list of [`IfcElement`]s (name + optional material
//! + owned faces) and emits a named, materialed BIM model. The Scene → element
//! mapping (enumerating Xias/Shapes, resolving material names) lives in the
//! WASM caller; this crate stays Scene-agnostic.

use crate::guid::ifc_guid_for;
use crate::ifc_advancedbrep::{advanced_faces_filtered, emit_advanced_geometry};
use crate::ifc_common::emit_owner_units_context;
use crate::step_value::StepValue;
use crate::step_writer::StepWriter;
use axia_geo::{FaceId, Mesh};
use std::collections::HashSet;

/// One semantic member to export as an `IfcWall`: a display name, an optional
/// material name, and the faces it owns (engine `FaceId`s).
pub struct IfcElement {
    pub name: String,
    pub material_name: Option<String>,
    pub face_ids: Vec<FaceId>,
}

fn next_guid(gi: &mut u64) -> StepValue {
    let v = *gi;
    *gi += 1;
    StepValue::Str(ifc_guid_for(v))
}

/// Emit a complete IFC4.3 file with one `IfcWall` per [`IfcElement`], each
/// carrying an analytic `IfcAdvancedBrep` of its owned faces + (if given) an
/// `IfcMaterial` via `IfcRelAssociatesMaterial`. `scale` converts engine units
/// to metre; `project_name` labels the `IfcProject`.
///
/// **All-or-nothing** (like β-2.5): errors if any element's faces can't form an
/// advanced brep (unsupported surface / curve) → the WASM caller falls back to
/// the faceted single-wall export.
pub fn emit_ifc_model(
    mesh: &Mesh,
    elements: &[IfcElement],
    scale: f64,
    project_name: &str,
) -> Result<String, String> {
    if elements.is_empty() {
        return Err("emit_ifc_model: no elements".into());
    }
    let mut w = StepWriter::new();
    w.file_description = format!("AXiA IFC4.3 '{}' (IfcAdvancedBrep model, ADR-203 γ)", project_name);
    w.file_name = format!("{}.ifc", project_name);

    // ── Owner / units / context ──
    let sc = emit_owner_units_context(&mut w);

    let mut gi: u64 = 0;

    // ── Spatial root: Project → Site → Building → Storey ──
    let project = w.add(
        "IFCPROJECT",
        vec![
            next_guid(&mut gi),
            StepValue::Ref(sc.owner),
            StepValue::Str(project_name.into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset,
            StepValue::List(vec![StepValue::Ref(sc.context)]),
            StepValue::Ref(sc.units),
        ],
    );
    let site_pl = w.add("IFCLOCALPLACEMENT", vec![StepValue::Unset, StepValue::Ref(sc.world)]);
    let site = w.add(
        "IFCSITE",
        vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str("Site".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset,
        ],
    );
    let building = w.add(
        "IFCBUILDING",
        vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str("Building".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset,
        ],
    );
    let storey = w.add(
        "IFCBUILDINGSTOREY",
        vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str("Storey".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()), StepValue::Unset,
        ],
    );
    w.add("IFCRELAGGREGATES", vec![
        next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
        StepValue::Ref(project), StepValue::List(vec![StepValue::Ref(site)])]);
    w.add("IFCRELAGGREGATES", vec![
        next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
        StepValue::Ref(site), StepValue::List(vec![StepValue::Ref(building)])]);
    w.add("IFCRELAGGREGATES", vec![
        next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
        StepValue::Ref(building), StepValue::List(vec![StepValue::Ref(storey)])]);

    // ── One IfcWall per element (geometry + material) ──
    let mut walls = Vec::with_capacity(elements.len());
    // Deduplicate IfcMaterial by name (one entity per distinct material).
    let mut materials: Vec<(String, crate::step_value::EntityRef)> = Vec::new();

    for (ei, el) in elements.iter().enumerate() {
        let allowed: HashSet<FaceId> = el.face_ids.iter().copied().collect();
        let faces = advanced_faces_filtered(mesh, Some(&allowed))
            .map_err(|e| format!("element[{}] '{}': {}", ei, el.name, e))?;
        let brep = emit_advanced_geometry(&mut w, &faces, scale)
            .map_err(|e| format!("element[{}] '{}': {}", ei, el.name, e))?;

        let shape_rep = w.add(
            "IFCSHAPEREPRESENTATION",
            vec![
                StepValue::Ref(sc.context),
                StepValue::Str("Body".into()),
                StepValue::Str("AdvancedBrep".into()),
                StepValue::List(vec![StepValue::Ref(brep)]),
            ],
        );
        let prod_def = w.add(
            "IFCPRODUCTDEFINITIONSHAPE",
            vec![StepValue::Unset, StepValue::Unset, StepValue::List(vec![StepValue::Ref(shape_rep)])],
        );
        let wall = w.add(
            "IFCWALL",
            vec![
                next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str(el.name.clone().into()),
                StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
                StepValue::Ref(prod_def), StepValue::Unset, StepValue::Unset,
            ],
        );
        walls.push(wall);

        if let Some(mat_name) = &el.material_name {
            let mat = match materials.iter().find(|(n, _)| n == mat_name) {
                Some((_, r)) => *r,
                None => {
                    let r = w.add(
                        "IFCMATERIAL",
                        vec![StepValue::Str(mat_name.clone().into()), StepValue::Unset, StepValue::Unset],
                    );
                    materials.push((mat_name.clone(), r));
                    r
                }
            };
            w.add(
                "IFCRELASSOCIATESMATERIAL",
                vec![
                    next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
                    StepValue::List(vec![StepValue::Ref(wall)]), StepValue::Ref(mat),
                ],
            );
        }
    }

    // ── Contain every wall in the storey ──
    w.add(
        "IFCRELCONTAINEDINSPATIALSTRUCTURE",
        vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
            StepValue::List(walls.iter().map(|&x| StepValue::Ref(x)).collect()),
            StepValue::Ref(storey),
        ],
    );

    Ok(w.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

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
            let chars = args.char_indices();
            for (i, c) in chars {
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

    /// Two separate boxes → two IfcWalls, each with its own material.
    fn two_box_mesh() -> (Mesh, Vec<FaceId>, Vec<FaceId>) {
        let mut mesh = Mesh::new();
        let a = mesh
            .create_box(DVec3::new(-2000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0))
            .unwrap();
        let b = mesh
            .create_box(DVec3::new(2000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0))
            .unwrap();
        (mesh, a, b)
    }

    #[test]
    fn two_elements_two_walls_two_materials() {
        let (mesh, a, b) = two_box_mesh();
        let elements = vec![
            IfcElement { name: "Wall A".into(), material_name: Some("Concrete".into()), face_ids: a },
            IfcElement { name: "Wall B".into(), material_name: Some("Steel".into()), face_ids: b },
        ];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "House").unwrap();
        assert!(s.contains("FILE_SCHEMA(('IFC4X3'));"));
        // two named walls
        assert_eq!(s.matches("=IFCWALL(").count(), 2);
        assert!(s.contains("'Wall A'") && s.contains("'Wall B'"));
        // one shared spatial hierarchy
        assert_eq!(s.matches("=IFCPROJECT(").count(), 1);
        assert_eq!(s.matches("=IFCBUILDINGSTOREY(").count(), 1);
        // two materials + two associations
        assert_eq!(s.matches("=IFCMATERIAL(").count(), 2);
        assert!(s.contains("IFCMATERIAL('Concrete'") && s.contains("IFCMATERIAL('Steel'"));
        assert_eq!(s.matches("=IFCRELASSOCIATESMATERIAL(").count(), 2);
        // each box is 6 planar advanced faces
        assert_eq!(s.matches("=IFCADVANCEDBREP(").count(), 2);
        assert_eq!(s.matches("=IFCADVANCEDFACE(").count(), 12);
        // both walls contained in the one storey
        assert_eq!(s.matches("=IFCRELCONTAINEDINSPATIALSTRUCTURE(").count(), 1);
        assert_refs_resolve(&s);
    }

    #[test]
    fn shared_material_deduplicated() {
        let (mesh, a, b) = two_box_mesh();
        let elements = vec![
            IfcElement { name: "A".into(), material_name: Some("Concrete".into()), face_ids: a },
            IfcElement { name: "B".into(), material_name: Some("Concrete".into()), face_ids: b },
        ];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "M").unwrap();
        // one IfcMaterial (deduped), two associations (one per wall)
        assert_eq!(s.matches("=IFCMATERIAL(").count(), 1);
        assert_eq!(s.matches("=IFCRELASSOCIATESMATERIAL(").count(), 2);
    }

    #[test]
    fn element_without_material_has_no_association() {
        let (mesh, a, _b) = two_box_mesh();
        let elements = vec![IfcElement { name: "Form".into(), material_name: None, face_ids: a }];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "F").unwrap();
        assert_eq!(s.matches("=IFCWALL(").count(), 1);
        assert_eq!(s.matches("=IFCMATERIAL(").count(), 0);
        assert_eq!(s.matches("=IFCRELASSOCIATESMATERIAL(").count(), 0);
        assert_refs_resolve(&s);
    }

    #[test]
    fn deterministic_byte_identical() {
        let build = || {
            let (mesh, a, b) = two_box_mesh();
            let elements = vec![
                IfcElement { name: "A".into(), material_name: Some("C".into()), face_ids: a },
                IfcElement { name: "B".into(), material_name: None, face_ids: b },
            ];
            emit_ifc_model(&mesh, &elements, 0.001, "M").unwrap()
        };
        assert_eq!(build(), build(), "deterministic (L-203-2)");
    }

    #[test]
    fn empty_elements_rejected() {
        let (mesh, _a, _b) = two_box_mesh();
        assert!(emit_ifc_model(&mesh, &[], 0.001, "M").is_err());
    }
}

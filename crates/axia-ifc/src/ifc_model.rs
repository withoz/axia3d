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

/// One semantic member to export: a display name, an optional material name,
/// what kind of building element it is, and the faces it owns (engine
/// `FaceId`s).
pub struct IfcElement {
    pub name: String,
    pub material_name: Option<String>,
    /// What this member *is* (ADR-203 δ). Defaults to `Wall` — which is what
    /// every member used to be, so an unassigned model exports unchanged.
    pub kind: crate::IfcElementKind,
    pub face_ids: Vec<FaceId>,
}

/// `OverallHeight` / `OverallWidth` for an opening element, from the faces it
/// owns.
///
/// Height is the Z extent — unambiguous under Z-up (LOCKED #43). Width is the
/// larger of the two horizontal extents, the smaller being the panel's
/// thickness. Both come back `Unset` if the member has no measurable box, so a
/// degenerate element writes `$` instead of a zero that reads as a real size.
fn overall_size(faces: &[crate::ifc_advancedbrep::AdvancedFace], scale: f64) -> (StepValue, StepValue) {
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for f in faces {
        for e in f.outer.iter().chain(f.inners.iter().flatten()) {
            for p in [e.start, e.end] {
                for (i, v) in [p.x, p.y, p.z].into_iter().enumerate() {
                    lo[i] = lo[i].min(v);
                    hi[i] = hi[i].max(v);
                }
            }
        }
    }
    if !lo.iter().chain(hi.iter()).all(|v| v.is_finite()) {
        return (StepValue::Unset, StepValue::Unset);
    }
    let (dx, dy, dz) = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
    let height = dz * scale;
    let width = dx.max(dy) * scale;
    // IfcPositiveLengthMeasure — zero is not a legal value, so omit instead.
    let pos = |v: f64| if v > 0.0 { StepValue::Real(v) } else { StepValue::Unset };
    (pos(height), pos(width))
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
        // The eight attributes every IfcElement has, then whatever this kind
        // adds. A door or a window takes thirteen, not nine — emitting them in
        // the wall-shaped slot produces an entity no reader accepts.
        let mut args = vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str(el.name.clone().into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Ref(prod_def), StepValue::Unset,
        ];
        if el.kind.has_overall_size() {
            // OverallHeight / OverallWidth — BIM tools show these as the
            // opening's size, so fill them from the member's own bounds rather
            // than leaving `$`. Height is the Z extent (Z-up, LOCKED #43);
            // width is the larger horizontal extent, the other one being the
            // panel's thickness. A degenerate box leaves both unset.
            let (h, wd) = overall_size(&faces, scale);
            args.push(h);
            args.push(wd);
        }
        args.push(StepValue::Unset); // PredefinedType
        if el.kind.has_overall_size() {
            // Door: OperationType + UserDefinedOperationType.
            // Window: PartitioningType + UserDefinedPartitioningType.
            args.push(StepValue::Unset);
            args.push(StepValue::Unset);
        }
        debug_assert_eq!(args.len(), el.kind.attribute_count(), "{} arity", el.kind.tag());
        let wall = w.add(el.kind.tag(), args);
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

    /// Count the comma-separated attributes of the first `TAG(` entity.
    fn arity_of(src: &str, tag: &str) -> usize {
        let start = src.find(&format!("={tag}(")).unwrap_or_else(|| panic!("no {tag}"));
        let open = src[start..].find('(').unwrap() + start + 1;
        let mut depth = 0usize;
        let mut n = 1usize;
        let mut in_str = false;
        for c in src[open..].chars() {
            match c {
                '\'' => in_str = !in_str,
                '(' if !in_str => depth += 1,
                ')' if !in_str => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                ',' if !in_str && depth == 0 => n += 1,
                _ => {}
            }
        }
        n
    }

    #[test]
    fn a_door_takes_thirteen_attributes_and_a_wall_still_takes_nine() {
        // δ refused doors because emitting one in the nine-attribute wall slot
        // makes an entity no IFC reader accepts. This is the shape that lets
        // them through.
        let (mesh, a, b) = two_box_mesh();
        let elements = vec![
            IfcElement { name: "Front Door".into(), material_name: None, kind: crate::IfcElementKind::Door, face_ids: a },
            IfcElement { name: "Plain Wall".into(), material_name: None, kind: crate::IfcElementKind::Wall, face_ids: b },
        ];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "House").unwrap();

        assert_eq!(s.matches("=IFCDOOR(").count(), 1);
        assert_eq!(arity_of(&s, "IFCDOOR"), 13, "door: 8 common + height + width + 3");
        assert_eq!(arity_of(&s, "IFCWALL"), 9, "the wall shape must not move");
    }

    #[test]
    fn a_window_carries_its_measured_size() {
        // OverallHeight / OverallWidth are what a BIM tool shows as the
        // opening's size. Leaving them `$` is legal but useless, so they come
        // from the member's own bounds: height is the Z extent, width the
        // larger horizontal one.
        let mut mesh = Mesh::new();
        // create_box(center, width→X, height→Z, depth→Y): 1200 wide, 900 tall,
        // 100 thick — a window panel.
        let f = mesh
            .create_box(DVec3::ZERO, 1200.0, 900.0, 100.0, axia_geo::MaterialId::new(0))
            .unwrap();
        let elements = vec![IfcElement {
            name: "W1".into(),
            material_name: None,
            kind: crate::IfcElementKind::Window,
            face_ids: f,
        }];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "House").unwrap();

        assert_eq!(arity_of(&s, "IFCWINDOW"), 13);
        let line = s.lines().find(|l| l.contains("=IFCWINDOW(")).expect("window line");
        // metres after the 0.001 scale — height then width, in that order.
        assert!(line.contains("0.9"), "height 900mm -> 0.9m: {line}");
        assert!(line.contains("1.2"), "width 1200mm -> 1.2m (not the 0.1m thickness): {line}");
    }

    #[test]
    fn two_elements_two_walls_two_materials() {
        let (mesh, a, b) = two_box_mesh();
        let elements = vec![
            IfcElement { name: "Wall A".into(), material_name: Some("Concrete".into()), kind: crate::IfcElementKind::Wall, face_ids: a },
            IfcElement { name: "Wall B".into(), material_name: Some("Steel".into()), kind: crate::IfcElementKind::Wall, face_ids: b },
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
            IfcElement { name: "A".into(), material_name: Some("Concrete".into()), kind: crate::IfcElementKind::Wall, face_ids: a },
            IfcElement { name: "B".into(), material_name: Some("Concrete".into()), kind: crate::IfcElementKind::Wall, face_ids: b },
        ];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "M").unwrap();
        // one IfcMaterial (deduped), two associations (one per wall)
        assert_eq!(s.matches("=IFCMATERIAL(").count(), 1);
        assert_eq!(s.matches("=IFCRELASSOCIATESMATERIAL(").count(), 2);
    }

    #[test]
    fn element_without_material_has_no_association() {
        let (mesh, a, _b) = two_box_mesh();
        let elements = vec![IfcElement { name: "Form".into(), material_name: None, kind: crate::IfcElementKind::Wall, face_ids: a }];
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
                IfcElement { name: "A".into(), material_name: Some("C".into()), kind: crate::IfcElementKind::Wall, face_ids: a },
                IfcElement { name: "B".into(), material_name: None, kind: crate::IfcElementKind::Wall, face_ids: b },
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

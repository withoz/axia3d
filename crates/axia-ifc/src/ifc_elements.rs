//! IFC element classification (ADR-203 I-2) — the second step of the import
//! track.
//!
//! I-1 answered "what is in this file?" as a histogram. This answers it as a
//! *list of members*: every building element, with its name, material, and the
//! geometry it points at — following the IFC reference chain
//!
//! ```text
//! IfcWall ─Representation→ IfcProductDefinitionShape
//!          ─Representations→ IfcShapeRepresentation
//!                            ─Items→ IfcAdvancedBrep / IfcFacetedBrep / …
//! IfcRelAssociatesMaterial ─RelatedObjects→ the element
//!                          ─RelatingMaterial→ IfcMaterial(Name)
//! ```
//!
//! Turning those geometry references into DCEL faces is I-3; this step decides
//! *what* to convert and reports honestly which items we can handle.

use crate::ifc_analyze::{json_str, opt_json};
use axia_foreign::step_parser::{self, Entity, StepFile, Value};
use std::collections::BTreeMap;

/// IFC product types we treat as building elements, with a short label.
/// (Order is irrelevant — lookup only.)
const ELEMENT_TYPES: &[&str] = &[
    "IFCWALL",
    "IFCWALLSTANDARDCASE",
    "IFCSLAB",
    "IFCBEAM",
    "IFCCOLUMN",
    "IFCDOOR",
    "IFCWINDOW",
    "IFCROOF",
    "IFCSTAIR",
    "IFCRAILING",
    "IFCCOVERING",
    "IFCPLATE",
    "IFCMEMBER",
    "IFCFOOTING",
    "IFCPILE",
    "IFCCURTAINWALL",
    "IFCBUILDINGELEMENTPROXY",
];

/// Geometry representation items I-3 will be able to turn into DCEL faces.
/// Everything else is reported as unsupported rather than silently dropped.
const SUPPORTED_GEOMETRY: &[&str] = &[
    "IFCADVANCEDBREP",
    "IFCFACETEDBREP",
    "IFCEXTRUDEDAREASOLID",
    "IFCBOOLEANRESULT",
    "IFCBOOLEANCLIPPINGRESULT",
    "IFCTRIANGULATEDFACESET",
];

/// One geometry item hanging off an element's shape representation.
#[derive(Clone, Debug, PartialEq)]
pub struct GeometryRef {
    /// `#N` of the representation item.
    pub id: u32,
    /// Entity tag, e.g. `IFCADVANCEDBREP`.
    pub kind: String,
    /// `IfcShapeRepresentation.RepresentationType`, e.g. `AdvancedBrep`.
    pub representation_type: Option<String>,
    /// Whether I-3 can convert this item into DCEL geometry.
    pub supported: bool,
}

/// One building element found in an IFC file.
#[derive(Clone, Debug, PartialEq)]
pub struct ImportedElement {
    /// `#N` of the product entity.
    pub id: u32,
    /// Entity tag, e.g. `IFCWALL`.
    pub ifc_type: String,
    pub name: Option<String>,
    pub global_id: Option<String>,
    /// Material name via `IfcRelAssociatesMaterial` → `IfcMaterial`.
    pub material: Option<String>,
    /// `IfcProduct.ObjectPlacement` (attribute 5) — head of the
    /// `IfcLocalPlacement` chain that locates this member (I-4).
    pub object_placement: Option<u32>,
    /// Geometry items, in file order.
    pub geometry: Vec<GeometryRef>,
}

impl ImportedElement {
    /// True if at least one geometry item is convertible (I-3).
    pub fn has_supported_geometry(&self) -> bool {
        self.geometry.iter().any(|g| g.supported)
    }
}

/// Everything I-2 learned about a file.
#[derive(Clone, Debug, Default)]
pub struct ElementReport {
    /// Elements, ordered by entity id (deterministic).
    pub elements: Vec<ImportedElement>,
    /// Entity tags that look like geometry but that I-3 cannot handle yet,
    /// with occurrence counts — so the gap is visible instead of silent.
    pub unsupported_geometry: BTreeMap<String, usize>,
}

impl ElementReport {
    pub fn convertible_count(&self) -> usize {
        self.elements.iter().filter(|e| e.has_supported_geometry()).count()
    }

    pub fn to_json(&self) -> String {
        let mut s = String::from("{\"ok\":true,\"elementCount\":");
        s.push_str(&self.elements.len().to_string());
        s.push_str(",\"convertible\":");
        s.push_str(&self.convertible_count().to_string());

        s.push_str(",\"elements\":[");
        for (i, e) in self.elements.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!(
                "{{\"id\":{},\"type\":{},\"name\":{},\"material\":{},\"geometry\":[",
                e.id,
                json_str(&e.ifc_type),
                opt_json(e.name.as_deref()),
                opt_json(e.material.as_deref()),
            ));
            for (j, g) in e.geometry.iter().enumerate() {
                if j > 0 {
                    s.push(',');
                }
                s.push_str(&format!(
                    "{{\"id\":{},\"kind\":{},\"representationType\":{},\"supported\":{}}}",
                    g.id,
                    json_str(&g.kind),
                    opt_json(g.representation_type.as_deref()),
                    g.supported,
                ));
            }
            s.push_str("]}");
        }
        s.push(']');

        s.push_str(",\"unsupportedGeometry\":{");
        for (i, (tag, n)) in self.unsupported_geometry.iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("{}:{}", json_str(tag), n));
        }
        s.push_str("}}");
        s
    }
}

/// Parse an `.ifc` and classify its building elements.
pub fn classify_ifc(src: &str) -> Result<ElementReport, String> {
    let file = step_parser::parse(src).map_err(|e| format!("{:?}", e))?;
    Ok(classify(&file))
}

/// Classify an already-parsed file.
pub fn classify(file: &StepFile) -> ElementReport {
    let materials = material_by_element(file);

    let mut elements: Vec<ImportedElement> = Vec::new();
    let mut unsupported: BTreeMap<String, usize> = BTreeMap::new();

    for (&id, ent) in file.iter_entities() {
        let tag = ent.tag.to_ascii_uppercase();
        if !ELEMENT_TYPES.contains(&tag.as_str()) {
            continue;
        }
        let geometry = geometry_of(file, ent, &mut unsupported);
        elements.push(ImportedElement {
            id,
            ifc_type: tag,
            name: arg_str(ent, 2),
            global_id: arg_str(ent, 0),
            material: materials.get(&id).cloned(),
            // IfcProduct: 0 GlobalId, 1 OwnerHistory, 2 Name, 3 Description,
            // 4 ObjectType, 5 ObjectPlacement, 6 Representation.
            object_placement: ent.args.get(5).and_then(|v| v.as_ref()),
            geometry,
        });
    }

    // HashMap iteration is unordered — sort so the report is deterministic.
    elements.sort_by_key(|e| e.id);
    ElementReport { elements, unsupported_geometry: unsupported }
}

/// `element #N → material name`, from every `IfcRelAssociatesMaterial`.
fn material_by_element(file: &StepFile) -> BTreeMap<u32, String> {
    let mut out = BTreeMap::new();
    for (_, rel) in file.iter_entities() {
        if !rel.tag.eq_ignore_ascii_case("IFCRELASSOCIATESMATERIAL") {
            continue;
        }
        // (GlobalId, OwnerHistory, Name, Description, RelatedObjects, RelatingMaterial)
        let Some(name) = rel.args.get(5).and_then(|v| v.as_ref()).and_then(|m| material_name(file, m))
        else {
            continue;
        };
        let Some(related) = rel.args.get(4).and_then(|v| v.as_list()) else { continue };
        for r in related {
            if let Some(target) = r.as_ref() {
                out.insert(target, name.clone());
            }
        }
    }
    out
}

/// `IfcMaterial.Name`, following `IfcMaterialLayerSetUsage`-style wrappers one
/// hop if the relation points at something that itself names a material.
fn material_name(file: &StepFile, id: u32) -> Option<String> {
    let ent = file.entity(id)?;
    if ent.tag.eq_ignore_ascii_case("IFCMATERIAL") {
        return arg_str(ent, 0);
    }
    // Indirect: take the first referenced IfcMaterial we can reach in one hop.
    for a in &ent.args {
        if let Some(r) = a.as_ref() {
            if let Some(inner) = file.entity(r) {
                if inner.tag.eq_ignore_ascii_case("IFCMATERIAL") {
                    return arg_str(inner, 0);
                }
            }
        }
    }
    None
}

/// Walk `element → IfcProductDefinitionShape → IfcShapeRepresentation → Items`.
fn geometry_of(
    file: &StepFile,
    element: &Entity,
    unsupported: &mut BTreeMap<String, usize>,
) -> Vec<GeometryRef> {
    let mut out = Vec::new();
    // IfcProduct.Representation is attribute 6 for the element types above.
    let Some(shape_id) = element.args.get(6).and_then(|v| v.as_ref()) else { return out };
    let Some(shape) = file.entity(shape_id) else { return out };
    if !shape.tag.eq_ignore_ascii_case("IFCPRODUCTDEFINITIONSHAPE") {
        return out;
    }
    // IfcProductDefinitionShape.Representations = attribute 2.
    let Some(reps) = shape.args.get(2).and_then(|v| v.as_list()) else { return out };

    for rep_val in reps {
        let Some(rep) = rep_val.as_ref().and_then(|r| file.entity(r)) else { continue };
        if !rep.tag.eq_ignore_ascii_case("IFCSHAPEREPRESENTATION") {
            continue;
        }
        let rep_type = arg_str(rep, 2);
        // IfcShapeRepresentation.Items = attribute 3.
        let Some(items) = rep.args.get(3).and_then(|v| v.as_list()) else { continue };
        for item_val in items {
            let Some(item_id) = item_val.as_ref() else { continue };
            let Some(item) = file.entity(item_id) else { continue };
            let kind = item.tag.to_ascii_uppercase();
            let supported = SUPPORTED_GEOMETRY.contains(&kind.as_str());
            if !supported {
                *unsupported.entry(kind.clone()).or_insert(0) += 1;
            }
            out.push(GeometryRef {
                id: item_id,
                kind,
                representation_type: rep_type.clone(),
                supported,
            });
        }
    }
    out
}

fn arg_str(ent: &Entity, idx: usize) -> Option<String> {
    match ent.args.get(idx) {
        Some(Value::Str(s)) => Some(s.clone()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{emit_box, emit_ifc_model, IfcElement};
    use axia_geo::{MaterialId, Mesh};
    use glam::DVec3;

    fn two_wall_model() -> String {
        let mut mesh = Mesh::new();
        let a = mesh
            .create_box(DVec3::new(-2000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, MaterialId::new(0))
            .unwrap();
        let b = mesh
            .create_box(DVec3::new(2000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, MaterialId::new(0))
            .unwrap();
        emit_ifc_model(
            &mesh,
            &[
                IfcElement { name: "Wall A".into(), material_name: Some("Concrete".into()), kind: crate::IfcElementKind::Wall, face_ids: a },
                IfcElement { name: "Wall B".into(), material_name: None, kind: crate::IfcElementKind::Wall, face_ids: b },
            ],
            0.001,
            "House",
        )
        .unwrap()
    }

    #[test]
    fn classifies_elements_with_names_materials_and_geometry() {
        let r = classify_ifc(&two_wall_model()).unwrap();
        assert_eq!(r.elements.len(), 2);

        let a = &r.elements[0];
        assert_eq!(a.ifc_type, "IFCWALL");
        assert_eq!(a.name.as_deref(), Some("Wall A"));
        assert_eq!(a.material.as_deref(), Some("Concrete"));
        assert!(a.global_id.is_some(), "GlobalId read back");
        assert_eq!(a.geometry.len(), 1);
        assert_eq!(a.geometry[0].kind, "IFCADVANCEDBREP");
        assert_eq!(a.geometry[0].representation_type.as_deref(), Some("AdvancedBrep"));
        assert!(a.geometry[0].supported);
        assert!(a.has_supported_geometry());

        // Second wall carries no material — reported as None, not invented.
        let b = &r.elements[1];
        assert_eq!(b.name.as_deref(), Some("Wall B"));
        assert_eq!(b.material, None);
        assert!(b.has_supported_geometry());

        assert_eq!(r.convertible_count(), 2);
        assert!(r.unsupported_geometry.is_empty());
    }

    #[test]
    fn faceted_brep_export_is_also_classified() {
        // β-1.5 output: one wall whose geometry is an IfcFacetedBrep.
        let r = classify_ifc(&emit_box(DVec3::ZERO, DVec3::ONE, "Box")).unwrap();
        assert_eq!(r.elements.len(), 1);
        let e = &r.elements[0];
        assert_eq!(e.name.as_deref(), Some("Box"));
        assert_eq!(e.geometry.len(), 1);
        assert_eq!(e.geometry[0].kind, "IFCFACETEDBREP");
        assert_eq!(e.geometry[0].representation_type.as_deref(), Some("Brep"));
        assert!(e.geometry[0].supported);
    }

    #[test]
    fn element_order_is_deterministic() {
        let ifc = two_wall_model();
        let a = classify_ifc(&ifc).unwrap();
        let b = classify_ifc(&ifc).unwrap();
        assert_eq!(a.elements, b.elements, "same file → same order (sorted by id)");
        let ids: Vec<u32> = a.elements.iter().map(|e| e.id).collect();
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        assert_eq!(ids, sorted, "ordered by entity id");
    }

    #[test]
    fn unsupported_geometry_is_reported_not_dropped() {
        // A wall whose body is an IfcSweptDiskSolid (a pipe swept along a
        // curve) — valid IFC that I-3 cannot convert. It must show up as
        // unsupported rather than vanish.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCSWEPTDISKSOLID($,10.,$,$,$);
#2=IFCSHAPEREPRESENTATION($,'Body','AdvancedSweptSolid',(#1));
#3=IFCPRODUCTDEFINITIONSHAPE($,$,(#2));
#4=IFCWALL('2aBcD',$,'Swept Wall',$,$,$,#3,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let r = classify_ifc(src).unwrap();
        assert_eq!(r.elements.len(), 1);
        let e = &r.elements[0];
        assert_eq!(e.name.as_deref(), Some("Swept Wall"));
        assert_eq!(e.global_id.as_deref(), Some("2aBcD"));
        assert_eq!(e.geometry.len(), 1);
        assert_eq!(e.geometry[0].kind, "IFCSWEPTDISKSOLID");
        assert!(!e.geometry[0].supported);
        assert!(!e.has_supported_geometry());
        assert_eq!(r.convertible_count(), 0);
        assert_eq!(r.unsupported_geometry.get("IFCSWEPTDISKSOLID"), Some(&1));
    }

    #[test]
    fn an_extruded_area_solid_is_supported_geometry() {
        // The representation real BIM tools use for almost every wall / slab /
        // column. δ-era it was reported unsupported; I-3 imports it now.
        let src = "\
ISO-10303-21;
HEADER;
FILE_SCHEMA(('IFC4X3'));
ENDSEC;
DATA;
#1=IFCRECTANGLEPROFILEDEF(.AREA.,$,$,4000.,200.);
#2=IFCEXTRUDEDAREASOLID(#1,$,$,3000.);
#3=IFCSHAPEREPRESENTATION($,'Body','SweptSolid',(#2));
#4=IFCPRODUCTDEFINITIONSHAPE($,$,(#3));
#5=IFCWALL('2aBcD',$,'Swept Wall',$,$,$,#4,$,$);
ENDSEC;
END-ISO-10303-21;
";
        let r = classify_ifc(src).unwrap();
        let e = &r.elements[0];
        assert_eq!(e.geometry[0].kind, "IFCEXTRUDEDAREASOLID");
        assert!(e.geometry[0].supported, "extruded area solids are convertible now");
        assert_eq!(r.convertible_count(), 1);
    }

    #[test]
    fn non_element_entities_are_ignored() {
        // The spatial containers and geometry primitives are not elements.
        let r = classify_ifc(&two_wall_model()).unwrap();
        for e in &r.elements {
            assert!(ELEMENT_TYPES.contains(&e.ifc_type.as_str()), "{} is an element type", e.ifc_type);
        }
        assert!(!r.elements.iter().any(|e| e.ifc_type == "IFCPROJECT"));
    }

    #[test]
    fn json_shape_is_stable() {
        let json = classify_ifc(&two_wall_model()).unwrap().to_json();
        assert!(json.starts_with("{\"ok\":true,\"elementCount\":2,\"convertible\":2"));
        assert!(json.contains("\"type\":\"IFCWALL\""));
        assert!(json.contains("\"name\":\"Wall A\""));
        assert!(json.contains("\"material\":\"Concrete\""));
        assert!(json.contains("\"kind\":\"IFCADVANCEDBREP\""));
        assert!(json.contains("\"supported\":true"));
        assert!(json.contains("\"unsupportedGeometry\":{}"));
        // the material-less wall must serialize as null, not ""
        assert!(json.contains("\"name\":\"Wall B\",\"material\":null"));
    }

    #[test]
    fn non_ascii_material_name_round_trips() {
        // We write Korean as the ISO-10303-21 `\X2\…\X0\` directive; reading it
        // back must give the name again, not the raw escape. (This was a real
        // gap — the classifier surfaced `\X2\AC15CCA0\X0\` until the lexer
        // learned the directive.)
        let mut mesh = Mesh::new();
        let faces = mesh
            .create_box(DVec3::ZERO, 1000.0, 1000.0, 1000.0, MaterialId::new(0))
            .unwrap();
        let ifc = emit_ifc_model(
            &mesh,
            &[IfcElement {
                name: "벽체".into(),
                material_name: Some("강철".into()),
                kind: crate::IfcElementKind::Wall, face_ids: faces,
            }],
            0.001,
            "House",
        )
        .unwrap();
        assert!(ifc.contains("\\X2\\"), "the file really is escape-encoded");

        let r = classify_ifc(&ifc).unwrap();
        assert_eq!(r.elements[0].name.as_deref(), Some("벽체"));
        assert_eq!(r.elements[0].material.as_deref(), Some("강철"));
    }

    #[test]
    fn garbage_is_rejected() {
        assert!(classify_ifc("definitely not a step file").is_err());
    }
}

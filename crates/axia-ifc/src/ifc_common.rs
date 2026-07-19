//! Shared IFC scaffold (ADR-203 β-2) — owner/units/context prologue + product/
//! spatial epilogue, plus the geometric placement primitives. SSOT for the parts
//! that `IfcFacetedBrep` (β-1.5) and `IfcAdvancedBrep` (β-2) both need
//! (meta-principle #4). The two emitters differ only in the geometry between the
//! prologue and epilogue and in the `IfcShapeRepresentation.RepresentationType`.

use crate::guid::ifc_guid_for;
use crate::step_value::{EntityRef, StepValue};
use crate::step_writer::StepWriter;
use glam::DVec3;

/// `IFCCARTESIANPOINT((x,y,z))`.
pub(crate) fn pt(w: &mut StepWriter, p: DVec3) -> EntityRef {
    w.add(
        "IFCCARTESIANPOINT",
        vec![StepValue::List(vec![
            StepValue::Real(p.x),
            StepValue::Real(p.y),
            StepValue::Real(p.z),
        ])],
    )
}

/// `IFCDIRECTION((x,y,z))`.
pub(crate) fn dir(w: &mut StepWriter, d: DVec3) -> EntityRef {
    w.add(
        "IFCDIRECTION",
        vec![StepValue::List(vec![
            StepValue::Real(d.x),
            StepValue::Real(d.y),
            StepValue::Real(d.z),
        ])],
    )
}

/// `IFCAXIS2PLACEMENT3D(location, +Z, +X)` — world-aligned frame at `origin`.
pub(crate) fn placement(w: &mut StepWriter, origin: DVec3) -> EntityRef {
    placement_axes(w, origin, DVec3::Z, DVec3::X)
}

/// `IFCAXIS2PLACEMENT3D(location, axis_z, ref_x)` — an oriented frame. Used by
/// analytic surfaces (cylinder/sphere/cone/torus) whose axis is not world-Z.
pub(crate) fn placement_axes(
    w: &mut StepWriter,
    origin: DVec3,
    axis_z: DVec3,
    ref_x: DVec3,
) -> EntityRef {
    let loc = pt(w, origin);
    let z = dir(w, axis_z);
    let x = dir(w, ref_x);
    w.add(
        "IFCAXIS2PLACEMENT3D",
        vec![StepValue::Ref(loc), StepValue::Ref(z), StepValue::Ref(x)],
    )
}

/// Shared header entities returned to the geometry section + epilogue.
pub(crate) struct Scaffold {
    pub owner: EntityRef,
    pub context: EntityRef,
    pub units: EntityRef,
    pub world: EntityRef,
}

/// Emit the owner/application/units/geometric-context prologue (identical to the
/// β-1.5 FacetedBrep scaffold — registration order preserved so both emitters
/// stay byte-deterministic, L-203-2).
pub(crate) fn emit_owner_units_context(w: &mut StepWriter) -> Scaffold {
    let person = w.add(
        "IFCPERSON",
        vec![
            StepValue::Unset,
            StepValue::Str("AXiA".into()),
            StepValue::Unset,
            StepValue::Unset,
            StepValue::Unset,
            StepValue::Unset,
            StepValue::Unset,
            StepValue::Unset,
        ],
    );
    let org = w.add(
        "IFCORGANIZATION",
        vec![
            StepValue::Unset,
            StepValue::Str("AXiA 3D".into()),
            StepValue::Unset,
            StepValue::Unset,
            StepValue::Unset,
        ],
    );
    let person_org = w.add(
        "IFCPERSONANDORGANIZATION",
        vec![StepValue::Ref(person), StepValue::Ref(org), StepValue::Unset],
    );
    let app = w.add(
        "IFCAPPLICATION",
        vec![
            StepValue::Ref(org),
            StepValue::Str("0.1.0".into()),
            StepValue::Str("axia-ifc".into()),
            StepValue::Str("axia-ifc".into()),
        ],
    );
    let owner = w.add(
        "IFCOWNERHISTORY",
        vec![
            StepValue::Ref(person_org),
            StepValue::Ref(app),
            StepValue::Unset,
            StepValue::Enum("ADDED".into()),
            StepValue::Unset,
            StepValue::Unset,
            StepValue::Unset,
            StepValue::Int(0),
        ],
    );

    // SI units (length = METRE, plane angle = RADIAN, solid angle = STERADIAN).
    let unit_len = w.add(
        "IFCSIUNIT",
        vec![StepValue::Derived, StepValue::Enum("LENGTHUNIT".into()), StepValue::Unset, StepValue::Enum("METRE".into())],
    );
    let unit_ang = w.add(
        "IFCSIUNIT",
        vec![StepValue::Derived, StepValue::Enum("PLANEANGLEUNIT".into()), StepValue::Unset, StepValue::Enum("RADIAN".into())],
    );
    let unit_sol = w.add(
        "IFCSIUNIT",
        vec![StepValue::Derived, StepValue::Enum("SOLIDANGLEUNIT".into()), StepValue::Unset, StepValue::Enum("STERADIAN".into())],
    );
    let units = w.add(
        "IFCUNITASSIGNMENT",
        vec![StepValue::List(vec![
            StepValue::Ref(unit_len),
            StepValue::Ref(unit_ang),
            StepValue::Ref(unit_sol),
        ])],
    );
    let world = placement(w, DVec3::ZERO);
    let context = w.add(
        "IFCGEOMETRICREPRESENTATIONCONTEXT",
        vec![
            StepValue::Unset,
            StepValue::Str("Model".into()),
            StepValue::Int(3),
            StepValue::Real(1e-5),
            StepValue::Ref(world),
            StepValue::Unset,
        ],
    );

    Scaffold { owner, context, units, world }
}

/// Emit the product + spatial epilogue: wrap `rep_item` (an `IfcFacetedBrep` or
/// `IfcAdvancedBrep`) in `IfcShapeRepresentation`/`IfcProductDefinitionShape` and
/// a minimal `Project→Site→Building→Storey→Wall` hierarchy. `rep_type` is the
/// `IfcShapeRepresentation.RepresentationType` (`"Brep"` for faceted,
/// `"AdvancedBrep"` for analytic). `name` labels the wall.
pub(crate) fn emit_product_and_spatial(
    w: &mut StepWriter,
    sc: &Scaffold,
    name: &str,
    rep_item: EntityRef,
    rep_type: &str,
) {
    let shape_rep = w.add(
        "IFCSHAPEREPRESENTATION",
        vec![
            StepValue::Ref(sc.context),
            StepValue::Str("Body".into()),
            StepValue::Str(rep_type.into()),
            StepValue::List(vec![StepValue::Ref(rep_item)]),
        ],
    );
    let prod_def = w.add(
        "IFCPRODUCTDEFINITIONSHAPE",
        vec![StepValue::Unset, StepValue::Unset, StepValue::List(vec![StepValue::Ref(shape_rep)])],
    );

    // Deterministic IfcRoot GUIDs by fixed index (L-203-2).
    let g = |i: u64| StepValue::Str(ifc_guid_for(i));

    let project = w.add(
        "IFCPROJECT",
        vec![
            g(0),
            StepValue::Ref(sc.owner),
            StepValue::Str("AXiA Export".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset,
            StepValue::List(vec![StepValue::Ref(sc.context)]),
            StepValue::Ref(sc.units),
        ],
    );
    let site_pl = w.add("IFCLOCALPLACEMENT", vec![StepValue::Unset, StepValue::Ref(sc.world)]);
    let site = w.add(
        "IFCSITE",
        vec![
            g(1), StepValue::Ref(sc.owner), StepValue::Str("Site".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset,
        ],
    );
    let building = w.add(
        "IFCBUILDING",
        vec![
            g(2), StepValue::Ref(sc.owner), StepValue::Str("Building".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset,
        ],
    );
    let storey = w.add(
        "IFCBUILDINGSTOREY",
        vec![
            g(3), StepValue::Ref(sc.owner), StepValue::Str("Storey".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()), StepValue::Unset,
        ],
    );
    let wall = w.add(
        "IFCWALL",
        vec![
            g(4), StepValue::Ref(sc.owner), StepValue::Str(name.into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Ref(prod_def), StepValue::Unset, StepValue::Unset,
        ],
    );

    // Aggregation + spatial containment relationships.
    w.add(
        "IFCRELAGGREGATES",
        vec![g(5), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
             StepValue::Ref(project), StepValue::List(vec![StepValue::Ref(site)])],
    );
    w.add(
        "IFCRELAGGREGATES",
        vec![g(6), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
             StepValue::Ref(site), StepValue::List(vec![StepValue::Ref(building)])],
    );
    w.add(
        "IFCRELAGGREGATES",
        vec![g(7), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
             StepValue::Ref(building), StepValue::List(vec![StepValue::Ref(storey)])],
    );
    w.add(
        "IFCRELCONTAINEDINSPATIALSTRUCTURE",
        vec![g(8), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
             StepValue::List(vec![StepValue::Ref(wall)]), StepValue::Ref(storey)],
    );
}

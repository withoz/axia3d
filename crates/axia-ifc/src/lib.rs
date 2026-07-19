//! IFC4.3 STEP-21 B-Rep exporter (ADR-203).
//!
//! β-1 (this crate's first atomic): the **writer foundation** — a deterministic
//! [`StepWriter`] with a [`StepValue`] formatter SSOT, an [`IfcEntity`] trait,
//! deterministic IFC GlobalIds, and an `IfcFacetedBrep` cube emitter that
//! produces true-IFC4X3 STEP-21 text (validated structurally, self-contained).
//!
//! β-2 (this crate's second geometry emitter): [`emit_advanced_brep`] emits a
//! true `IfcAdvancedBrep` — faces carry their analytic surface (`IfcPlane` /
//! `IfcCylindricalSurface` / ... from axia-geo [`axia_geo::AnalyticSurface`])
//! and `IfcEdgeLoop` boundaries of straight `IfcEdgeCurve(IfcLine)` edges.
//!
//! Later sub-steps: β-3 curved edge curves (`IfcCircle`/`IfcBSplineCurve`) so
//! curved faces get exact trims, γ Xia→IfcWall + material, δ spatial hierarchy,
//! ε external IFC validation (IfcOpenShell/Revit).
//!
//! Key decision (ADR-203): we emit **true IFC** (IFC4X3 entity names) — the
//! axia-foreign STEP parser cannot re-import IFC names, so validation is
//! self-contained structural well-formedness, with external IFC tools deferred
//! to ε.

pub mod guid;
pub mod ifc_advancedbrep;
mod ifc_common;
pub mod ifc_facetedbrep;
pub mod step_value;
pub mod step_writer;

pub use guid::{ifc_guid, ifc_guid_for};
pub use ifc_advancedbrep::{emit_advanced_brep, AdvancedFace};
pub use ifc_facetedbrep::{emit_box, emit_brep, emit_faceted_brep, emit_unit_cube};
pub use step_value::{EntityRef, StepValue};
pub use step_writer::{IfcEntity, StepWriter};

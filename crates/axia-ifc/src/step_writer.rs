//! Deterministic STEP-21 writer (ADR-203 L-203-2/L-203-4).
//!
//! Entities are buffered in an insertion-ordered `Vec` and assigned sequential
//! `#N` ids (1-based) at registration. Output is byte-identical for a given
//! registration order — no `HashMap`, no randomness. Emits true IFC4X3 entity
//! names (`IFCCARTESIANPOINT`, ...) under `FILE_SCHEMA(('IFC4X3'))`.

use crate::step_value::{EntityRef, StepValue};

struct Record {
    id: u32,
    type_name: &'static str,
    args: Vec<StepValue>,
}

/// STEP-21 ISO-10303-21 file builder.
pub struct StepWriter {
    records: Vec<Record>,
    next_id: u32,
    // HEADER fields.
    pub file_description: String,
    pub file_name: String,
    pub time_stamp: String,
    pub author: String,
    pub organization: String,
    pub preprocessor_version: String,
    pub originating_system: String,
    pub schema: &'static str,
}

impl Default for StepWriter {
    fn default() -> Self {
        StepWriter {
            records: Vec::new(),
            next_id: 1,
            file_description: "AXiA IFC4.3 export".to_string(),
            file_name: "model.ifc".to_string(),
            // deterministic default timestamp (no wall-clock — L-203-2). Caller
            // may override for real exports.
            time_stamp: "1970-01-01T00:00:00".to_string(),
            author: "AXiA".to_string(),
            organization: "AXiA 3D".to_string(),
            preprocessor_version: "axia-ifc".to_string(),
            originating_system: "axia-ifc".to_string(),
            schema: "IFC4X3",
        }
    }
}

impl StepWriter {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an entity instance, returning its `#N` ref. The core mechanism:
    /// id assignment is sequential in registration order (deterministic).
    pub fn add(&mut self, type_name: &'static str, args: Vec<StepValue>) -> EntityRef {
        let id = self.next_id;
        self.next_id += 1;
        self.records.push(Record { id, type_name, args });
        EntityRef(id)
    }

    /// Register a typed [`IfcEntity`] (convenience over [`add`]).
    pub fn register<E: IfcEntity>(&mut self, e: &E) -> EntityRef {
        self.add(E::TYPE_NAME, e.args())
    }

    /// Number of registered entities.
    pub fn len(&self) -> usize {
        self.records.len()
    }

    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// True if every `#N` reference appearing in any entity resolves to a
    /// registered entity (1..=len). Structural well-formedness (L-203-5).
    pub fn refs_resolve(&self) -> bool {
        let max = self.next_id; // ids are 1..next_id
        let mut ok = true;
        for r in &self.records {
            for v in &r.args {
                walk_refs(v, &mut |id| {
                    if id == 0 || id >= max {
                        ok = false;
                    }
                });
            }
        }
        ok
    }

    /// Serialize the full ISO-10303-21 file (HEADER + DATA).
    pub fn build(&self) -> String {
        let mut out = String::new();
        out.push_str("ISO-10303-21;\n");
        out.push_str("HEADER;\n");
        out.push_str(&format!(
            "FILE_DESCRIPTION(({}),'2;1');\n",
            StepValue::Str(self.file_description.clone()).fmt()
        ));
        out.push_str(&format!(
            "FILE_NAME({},{},({}),({}),{},{},'');\n",
            StepValue::Str(self.file_name.clone()).fmt(),
            StepValue::Str(self.time_stamp.clone()).fmt(),
            StepValue::Str(self.author.clone()).fmt(),
            StepValue::Str(self.organization.clone()).fmt(),
            StepValue::Str(self.preprocessor_version.clone()).fmt(),
            StepValue::Str(self.originating_system.clone()).fmt(),
        ));
        out.push_str(&format!(
            "FILE_SCHEMA(({}));\n",
            StepValue::Str(self.schema.to_string()).fmt()
        ));
        out.push_str("ENDSEC;\n");
        out.push_str("DATA;\n");
        for r in &self.records {
            let args: Vec<String> = r.args.iter().map(StepValue::fmt).collect();
            out.push_str(&format!("#{}={}({});\n", r.id, r.type_name, args.join(",")));
        }
        out.push_str("ENDSEC;\n");
        out.push_str("END-ISO-10303-21;\n");
        out
    }
}

fn walk_refs(v: &StepValue, f: &mut impl FnMut(u32)) {
    match v {
        StepValue::Ref(r) => f(r.id()),
        StepValue::List(vs) | StepValue::Typed(_, vs) => {
            for x in vs {
                walk_refs(x, f);
            }
        }
        _ => {}
    }
}

/// A typed IFC entity. `args()` returns the STEP attribute values (in schema
/// order); references to sub-entities must already be registered.
pub trait IfcEntity {
    const TYPE_NAME: &'static str;
    fn args(&self) -> Vec<StepValue>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn add_assigns_sequential_ids() {
        let mut w = StepWriter::new();
        let a = w.add("IFCCARTESIANPOINT", vec![StepValue::List(vec![])]);
        let b = w.add("IFCDIRECTION", vec![StepValue::List(vec![])]);
        assert_eq!(a.id(), 1);
        assert_eq!(b.id(), 2);
        assert_eq!(w.len(), 2);
    }

    #[test]
    fn build_emits_iso_10303_21_skeleton() {
        let mut w = StepWriter::new();
        w.add(
            "IFCCARTESIANPOINT",
            vec![StepValue::List(vec![
                StepValue::Real(0.0),
                StepValue::Real(1.0),
                StepValue::Real(0.0),
            ])],
        );
        let s = w.build();
        assert!(s.starts_with("ISO-10303-21;\nHEADER;\n"));
        assert!(s.contains("FILE_SCHEMA(('IFC4X3'));"));
        assert!(s.contains("#1=IFCCARTESIANPOINT((0.,1.,0.));"));
        assert!(s.trim_end().ends_with("END-ISO-10303-21;"));
    }

    #[test]
    fn output_byte_identical_for_same_registration_order() {
        let emit = || {
            let mut w = StepWriter::new();
            let p = w.add("IFCCARTESIANPOINT", vec![StepValue::List(vec![StepValue::Real(2.0)])]);
            w.add("IFCPOLYLOOP", vec![StepValue::List(vec![StepValue::Ref(p)])]);
            w.build()
        };
        assert_eq!(emit(), emit(), "deterministic byte-identical output (L-203-2)");
    }

    #[test]
    fn refs_resolve_detects_dangling() {
        let mut w = StepWriter::new();
        let p = w.add("IFCCARTESIANPOINT", vec![StepValue::List(vec![])]);
        w.add("IFCPOLYLOOP", vec![StepValue::List(vec![StepValue::Ref(p)])]);
        assert!(w.refs_resolve(), "all refs registered");
        // dangling ref → fails
        w.add("IFCFACE", vec![StepValue::List(vec![StepValue::Ref(EntityRef(999))])]);
        assert!(!w.refs_resolve(), "dangling #999 detected");
    }

    #[test]
    fn ifc_entity_trait_via_register() {
        struct IfcCartesianPoint([f64; 3]);
        impl IfcEntity for IfcCartesianPoint {
            const TYPE_NAME: &'static str = "IFCCARTESIANPOINT";
            fn args(&self) -> Vec<StepValue> {
                vec![StepValue::List(self.0.iter().map(|&c| StepValue::Real(c)).collect())]
            }
        }
        let mut w = StepWriter::new();
        let r = w.register(&IfcCartesianPoint([1.0, 2.0, 3.0]));
        assert_eq!(r.id(), 1);
        assert!(w.build().contains("#1=IFCCARTESIANPOINT((1.,2.,3.));"));
    }
}

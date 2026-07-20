//! IFC file analysis (ADR-203 I-1) — the first step of the *import* track.
//!
//! Reading an `.ifc` needs no new parser: `axia-foreign`'s STEP-21 lexer/parser
//! is schema-agnostic (it strips the ISO envelope and parses `#N=TYPE(args);`
//! without caring whether the schema is AP203 or IFC4X3). This module reuses it
//! to answer "what is in this file?" — schema, entity histogram, and the counts
//! a BIM user actually cares about (walls, slabs, materials, breps).
//!
//! Geometry import (IFC B-rep → DCEL) is the next step; this one deliberately
//! stops at *reading*, so the app can report a file's contents honestly instead
//! of showing a "coming soon" placeholder.

use axia_foreign::step_parser::{self, Value};
use std::collections::BTreeMap;

/// What an `.ifc` file contains, as read by an independent parse.
#[derive(Clone, Debug, Default)]
pub struct IfcAnalysis {
    /// `FILE_SCHEMA` (e.g. `IFC4X3`), if the header declares one.
    pub schema: Option<String>,
    /// `FILE_DESCRIPTION` first entry, if present.
    pub description: Option<String>,
    /// Total DATA-section entities.
    pub entity_count: usize,
    /// Entity tag → occurrences. `BTreeMap` keeps the report deterministic.
    pub type_counts: BTreeMap<String, usize>,
}

/// IFC types worth surfacing to a user, in report order.
const NOTABLE: &[(&str, &str)] = &[
    ("IFCWALL", "walls"),
    ("IFCWALLSTANDARDCASE", "wallsStandardCase"),
    ("IFCSLAB", "slabs"),
    ("IFCBEAM", "beams"),
    ("IFCCOLUMN", "columns"),
    ("IFCDOOR", "doors"),
    ("IFCWINDOW", "windows"),
    ("IFCSPACE", "spaces"),
    ("IFCBUILDINGSTOREY", "storeys"),
    ("IFCMATERIAL", "materials"),
    ("IFCADVANCEDBREP", "advancedBreps"),
    ("IFCFACETEDBREP", "facetedBreps"),
    ("IFCEXTRUDEDAREASOLID", "extrudedAreaSolids"),
];

impl IfcAnalysis {
    /// Occurrences of one entity tag (case-insensitive; IFC tags are uppercase).
    pub fn count(&self, tag: &str) -> usize {
        self.type_counts.get(&tag.to_ascii_uppercase()).copied().unwrap_or(0)
    }

    /// The `n` most frequent entity types, most frequent first (ties by name).
    pub fn top_types(&self, n: usize) -> Vec<(String, usize)> {
        let mut v: Vec<(String, usize)> =
            self.type_counts.iter().map(|(k, &c)| (k.clone(), c)).collect();
        v.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
        v.truncate(n);
        v
    }

    /// Report as JSON (hand-rolled — this crate stays dependency-light).
    pub fn to_json(&self) -> String {
        let mut s = String::from("{\"ok\":true");
        s.push_str(&format!(",\"schema\":{}", opt_json(self.schema.as_deref())));
        s.push_str(&format!(",\"description\":{}", opt_json(self.description.as_deref())));
        s.push_str(&format!(",\"entityCount\":{}", self.entity_count));

        s.push_str(",\"notable\":{");
        let mut first = true;
        for (tag, label) in NOTABLE {
            let c = self.count(tag);
            if c == 0 {
                continue;
            }
            if !first {
                s.push(',');
            }
            first = false;
            s.push_str(&format!("{}:{}", json_str(label), c));
        }
        s.push('}');

        s.push_str(",\"topTypes\":[");
        for (i, (tag, c)) in self.top_types(12).iter().enumerate() {
            if i > 0 {
                s.push(',');
            }
            s.push_str(&format!("[{},{}]", json_str(tag), c));
        }
        s.push_str("]}");
        s
    }
}

/// Parse an `.ifc` (STEP-21 physical file) and summarize it.
pub fn analyze_ifc(src: &str) -> Result<IfcAnalysis, String> {
    let file = step_parser::parse(src).map_err(|e| format!("{:?}", e))?;

    let mut type_counts: BTreeMap<String, usize> = BTreeMap::new();
    for (_, ent) in file.iter_entities() {
        *type_counts.entry(ent.tag.to_ascii_uppercase()).or_insert(0) += 1;
    }

    let schema = file
        .header_entity("FILE_SCHEMA")
        .and_then(|e| e.args.first())
        .and_then(first_string);
    let description = file
        .header_entity("FILE_DESCRIPTION")
        .and_then(|e| e.args.first())
        .and_then(first_string);

    Ok(IfcAnalysis { schema, description, entity_count: file.data.len(), type_counts })
}

/// First string inside a value that is either a `Str` or a list of them.
fn first_string(v: &Value) -> Option<String> {
    match v {
        Value::Str(s) => Some(s.clone()),
        Value::List(items) => items.iter().find_map(first_string),
        _ => None,
    }
}

fn json_str(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('"');
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => out.push_str(&format!("\\u{:04x}", c as u32)),
            c => out.push(c),
        }
    }
    out.push('"');
    out
}

fn opt_json(v: Option<&str>) -> String {
    match v {
        Some(s) => json_str(s),
        None => "null".to_string(),
    }
}

/// Error report in the same envelope shape as [`IfcAnalysis::to_json`].
pub fn error_json(msg: &str) -> String {
    format!("{{\"ok\":false,\"error\":{}}}", json_str(msg))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{emit_box, emit_ifc_model, IfcElement};
    use axia_geo::{MaterialId, Mesh};
    use glam::DVec3;

    #[test]
    fn analyzes_our_own_export_round_trip() {
        // The strongest smoke test we have: our exporter's output, read back by
        // our importer's parser.
        let ifc = emit_box(DVec3::ZERO, DVec3::new(1.0, 2.0, 3.0), "Box");
        let a = analyze_ifc(&ifc).expect("our own IFC must parse");
        assert_eq!(a.schema.as_deref(), Some("IFC4X3"));
        assert_eq!(a.count("IFCWALL"), 1);
        assert_eq!(a.count("IFCFACETEDBREP"), 1);
        assert_eq!(a.count("IFCCARTESIANPOINT"), 9); // 8 corners + world origin
        assert!(a.entity_count > 30);
        assert!(a.description.is_some());
    }

    #[test]
    fn analyzes_semantic_model_with_materials() {
        let mut mesh = Mesh::new();
        let faces = mesh
            .create_box(DVec3::ZERO, 1000.0, 1000.0, 1000.0, MaterialId::new(0))
            .unwrap();
        let ifc = emit_ifc_model(
            &mesh,
            &[IfcElement {
                name: "Wall A".into(),
                material_name: Some("Concrete".into()),
                face_ids: faces,
            }],
            0.001,
            "House",
        )
        .unwrap();

        let a = analyze_ifc(&ifc).unwrap();
        assert_eq!(a.count("IFCWALL"), 1);
        assert_eq!(a.count("IFCMATERIAL"), 1);
        assert_eq!(a.count("IFCADVANCEDBREP"), 1);
        assert_eq!(a.count("IFCFACETEDBREP"), 0);

        let json = a.to_json();
        assert!(json.contains("\"ok\":true"));
        assert!(json.contains("\"schema\":\"IFC4X3\""));
        assert!(json.contains("\"walls\":1"), "notable counts: {}", json);
        assert!(json.contains("\"materials\":1"));
        assert!(json.contains("\"topTypes\":["));
    }

    #[test]
    fn top_types_is_frequency_ordered_and_deterministic() {
        let ifc = emit_box(DVec3::ZERO, DVec3::ONE, "b");
        let a = analyze_ifc(&ifc).unwrap();
        let top = a.top_types(3);
        assert!(top.len() <= 3);
        for w in top.windows(2) {
            assert!(w[0].1 >= w[1].1, "descending by count: {:?}", top);
        }
        assert_eq!(a.top_types(3), a.top_types(3), "deterministic");
    }

    #[test]
    fn tag_lookup_is_case_insensitive() {
        let ifc = emit_box(DVec3::ZERO, DVec3::ONE, "b");
        let a = analyze_ifc(&ifc).unwrap();
        assert_eq!(a.count("ifcwall"), a.count("IFCWALL"));
    }

    #[test]
    fn garbage_input_is_rejected_not_panicking() {
        assert!(analyze_ifc("this is not a STEP file at all").is_err());
        let e = error_json("boom");
        assert!(e.contains("\"ok\":false") && e.contains("boom"));
    }

    #[test]
    fn json_escapes_quotes_and_controls() {
        assert_eq!(json_str("a\"b\\c"), "\"a\\\"b\\\\c\"");
        assert_eq!(json_str("line\nbreak"), "\"line\\nbreak\"");
        assert_eq!(opt_json(None), "null");
        // non-ASCII passes through as UTF-8 (valid JSON)
        assert_eq!(json_str("강철"), "\"강철\"");
    }
}

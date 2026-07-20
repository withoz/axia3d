//! The spatial tree (ADR-203 I-5) — site, building, storey, and what sits in them.
//!
//! I-4 put members at the right coordinates. They still arrive as a flat pile of
//! faces: nothing says which storey a wall belongs to, so there is no way to hide
//! a floor or select a whole member. IFC already carries that structure in two
//! relations:
//!
//! ```text
//! IfcProject ─IfcRelAggregates→ IfcSite ─IfcRelAggregates→ IfcBuilding
//!                                                         └→ IfcBuildingStorey
//! IfcBuildingStorey ←IfcRelContainedInSpatialStructure─ IfcWall, IfcSlab, …
//! ```
//!
//! `IfcRelAggregates(…, 4 RelatingObject, 5 RelatedObjects)` nests containers;
//! `IfcRelContainedInSpatialStructure(…, 4 RelatedElements, 5 RelatingStructure)`
//! puts members in one. Both attribute orders were read off our own emitter.
//!
//! A file that omits the relations is not an error — the members simply come in
//! without a container, which the caller reports rather than inventing a tree.

use std::collections::BTreeMap;

use axia_foreign::step_parser::StepFile;

/// Container tags we surface. Ordered coarse → fine, matching IFC nesting.
const CONTAINER_TYPES: &[&str] = &[
    "IFCPROJECT",
    "IFCSITE",
    "IFCBUILDING",
    "IFCBUILDINGSTOREY",
    "IFCSPACE",
];

/// One spatial container.
#[derive(Clone, Debug, PartialEq)]
pub struct SpatialNode {
    /// `#N` of the container entity.
    pub id: u32,
    /// Entity tag, e.g. `IFCBUILDINGSTOREY`.
    pub ifc_type: String,
    pub name: Option<String>,
    /// Parent container, if this one is aggregated into another.
    pub parent: Option<u32>,
}

impl SpatialNode {
    /// What to call this container in the scene tree.
    ///
    /// An unnamed container falls back to its kind, so a storey with no name
    /// reads as "Building Storey" instead of an empty row. Tags are stored
    /// upper-cased, so the readable form is looked up rather than re-derived.
    pub fn label(&self) -> String {
        match &self.name {
            Some(n) if !n.trim().is_empty() => n.clone(),
            _ => match self.ifc_type.as_str() {
                "IFCPROJECT" => "Project",
                "IFCSITE" => "Site",
                "IFCBUILDING" => "Building",
                "IFCBUILDINGSTOREY" => "Building Storey",
                "IFCSPACE" => "Space",
                other => other.trim_start_matches("IFC"),
            }
            .to_string(),
        }
    }
}

/// The spatial structure of a file.
#[derive(Clone, Debug, Default)]
pub struct SpatialTree {
    /// Containers by entity id.
    pub nodes: BTreeMap<u32, SpatialNode>,
    /// Element entity id → the container that holds it.
    pub container_of: BTreeMap<u32, u32>,
}

impl SpatialTree {
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// Containers ordered parents-before-children, so a consumer can create a
    /// scene group and immediately attach it to an already-created parent.
    ///
    /// Cycles cannot starve the output: anything still unemitted after the
    /// parent-respecting passes is appended in id order.
    pub fn topological(&self) -> Vec<&SpatialNode> {
        let mut out: Vec<&SpatialNode> = Vec::with_capacity(self.nodes.len());
        let mut emitted: Vec<u32> = Vec::with_capacity(self.nodes.len());

        loop {
            let mut progressed = false;
            for (id, n) in &self.nodes {
                if emitted.contains(id) {
                    continue;
                }
                let ready = match n.parent {
                    None => true,
                    Some(p) => !self.nodes.contains_key(&p) || emitted.contains(&p),
                };
                if ready {
                    out.push(n);
                    emitted.push(*id);
                    progressed = true;
                }
            }
            if !progressed {
                break;
            }
        }

        // Whatever is left is part of a cycle — emit it rather than drop it.
        for (id, n) in &self.nodes {
            if !emitted.contains(id) {
                out.push(n);
            }
        }
        out
    }

    /// The chain from a container up to the root, nearest first.
    ///
    /// Stops on revisiting a node, so a malformed file that aggregates a cycle
    /// returns the ring once instead of looping.
    pub fn ancestry(&self, mut id: u32) -> Vec<u32> {
        let mut out: Vec<u32> = Vec::new();
        while let Some(n) = self.nodes.get(&id) {
            if out.contains(&n.id) {
                break;
            }
            out.push(n.id);
            match n.parent {
                Some(p) if p != id => id = p,
                _ => break,
            }
        }
        out
    }
}

/// Read the spatial tree from a parsed file.
pub fn spatial_tree(file: &StepFile) -> SpatialTree {
    let mut nodes: BTreeMap<u32, SpatialNode> = BTreeMap::new();

    for (id, e) in file.iter_entities() {
        let tag = e.tag.to_ascii_uppercase();
        if !CONTAINER_TYPES.contains(&tag.as_str()) {
            continue;
        }
        nodes.insert(
            *id,
            SpatialNode {
                id: *id,
                ifc_type: tag,
                // IfcRoot: 0 GlobalId, 1 OwnerHistory, 2 Name.
                name: e.args.get(2).and_then(|v| v.as_str()).map(str::to_string),
                parent: None,
            },
        );
    }

    let mut container_of: BTreeMap<u32, u32> = BTreeMap::new();

    for (_, e) in file.iter_entities() {
        let tag = e.tag.to_ascii_uppercase();
        if tag == "IFCRELAGGREGATES" {
            // 4 RelatingObject, 5 RelatedObjects
            let Some(parent) = e.args.get(4).and_then(|v| v.as_ref()) else { continue };
            let Some(children) = e.args.get(5).and_then(|v| v.as_list()) else { continue };
            for c in children {
                let Some(cid) = c.as_ref() else { continue };
                if let Some(n) = nodes.get_mut(&cid) {
                    // Guard against a file that aggregates a node into itself.
                    if cid != parent {
                        n.parent = Some(parent);
                    }
                }
            }
        } else if tag == "IFCRELCONTAINEDINSPATIALSTRUCTURE" {
            // 4 RelatedElements, 5 RelatingStructure
            let Some(structure) = e.args.get(5).and_then(|v| v.as_ref()) else { continue };
            let Some(elements) = e.args.get(4).and_then(|v| v.as_list()) else { continue };
            for el in elements {
                if let Some(eid) = el.as_ref() {
                    container_of.insert(eid, structure);
                }
            }
        }
    }

    SpatialTree { nodes, container_of }
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

    /// Site → Building → two storeys, one wall in each.
    fn two_storey_file() -> StepFile {
        parse(
            "#1=IFCPROJECT('p',$,'Proj',$,$,$,$,$,$);\n\
             #2=IFCSITE('s',$,'Site',$,$,$,$,$,.ELEMENT.,$,$,$,$,$);\n\
             #3=IFCBUILDING('b',$,'Building',$,$,$,$,$,.ELEMENT.,$,$,$);\n\
             #4=IFCBUILDINGSTOREY('l1',$,'Level 1',$,$,$,$,$,.ELEMENT.,$);\n\
             #5=IFCBUILDINGSTOREY('l2',$,'Level 2',$,$,$,$,$,.ELEMENT.,$);\n\
             #6=IFCRELAGGREGATES('a1',$,$,$,#1,(#2));\n\
             #7=IFCRELAGGREGATES('a2',$,$,$,#2,(#3));\n\
             #8=IFCRELAGGREGATES('a3',$,$,$,#3,(#4,#5));\n\
             #10=IFCWALL('w1',$,'Wall A',$,$,$,$,$,$);\n\
             #11=IFCWALL('w2',$,'Wall B',$,$,$,$,$,$);\n\
             #12=IFCRELCONTAINEDINSPATIALSTRUCTURE('c1',$,$,$,(#10),#4);\n\
             #13=IFCRELCONTAINEDINSPATIALSTRUCTURE('c2',$,$,$,(#11),#5);\n",
        )
    }

    #[test]
    fn reads_the_container_chain_and_membership() {
        let t = spatial_tree(&two_storey_file());
        assert_eq!(t.nodes.len(), 5, "project, site, building, two storeys");
        assert_eq!(t.nodes[&2].parent, Some(1), "site under project");
        assert_eq!(t.nodes[&3].parent, Some(2), "building under site");
        assert_eq!(t.nodes[&4].parent, Some(3), "storey under building");
        assert_eq!(t.nodes[&5].parent, Some(3));
        assert_eq!(t.container_of[&10], 4, "Wall A on Level 1");
        assert_eq!(t.container_of[&11], 5, "Wall B on Level 2");
    }

    #[test]
    fn topological_order_puts_parents_first() {
        let t = spatial_tree(&two_storey_file());
        let order: Vec<u32> = t.topological().iter().map(|n| n.id).collect();
        assert_eq!(order.len(), 5);
        for n in t.topological() {
            if let Some(p) = n.parent {
                let pi = order.iter().position(|x| *x == p).unwrap();
                let ci = order.iter().position(|x| *x == n.id).unwrap();
                assert!(pi < ci, "parent #{p} must precede child #{}", n.id);
            }
        }
    }

    #[test]
    fn ancestry_walks_up_to_the_root() {
        let t = spatial_tree(&two_storey_file());
        assert_eq!(t.ancestry(4), vec![4, 3, 2, 1], "storey → building → site → project");
    }

    #[test]
    fn unnamed_containers_get_a_readable_label() {
        let f = parse("#1=IFCBUILDINGSTOREY('g',$,$,$,$,$,$,$,.ELEMENT.,$);\n");
        let t = spatial_tree(&f);
        assert_eq!(t.nodes[&1].label(), "Building Storey");

        let f = parse("#1=IFCBUILDINGSTOREY('g',$,'2\\X2\\CE35\\X0\\',$,$,$,$,$,.ELEMENT.,$);\n");
        let t = spatial_tree(&f);
        assert_eq!(t.nodes[&1].label(), "2층", "a real name wins, decoded");
    }

    #[test]
    fn a_file_without_the_relations_yields_an_empty_membership() {
        // Containers exist but nothing aggregates or contains — honest empty,
        // not an invented hierarchy.
        let f = parse(
            "#1=IFCSITE('s',$,'Site',$,$,$,$,$,.ELEMENT.,$,$,$,$,$);\n\
             #2=IFCWALL('w',$,'Lonely',$,$,$,$,$,$);\n",
        );
        let t = spatial_tree(&f);
        assert_eq!(t.nodes.len(), 1);
        assert_eq!(t.nodes[&1].parent, None);
        assert!(t.container_of.is_empty(), "no membership invented");
    }

    #[test]
    fn a_cyclic_aggregation_does_not_hang_or_lose_nodes() {
        let f = parse(
            "#1=IFCBUILDING('b',$,'B',$,$,$,$,$,.ELEMENT.,$,$,$);\n\
             #2=IFCBUILDINGSTOREY('s',$,'S',$,$,$,$,$,.ELEMENT.,$);\n\
             #3=IFCRELAGGREGATES('a',$,$,$,#1,(#2));\n\
             #4=IFCRELAGGREGATES('b',$,$,$,#2,(#1));\n",
        );
        let t = spatial_tree(&f);
        assert_eq!(t.topological().len(), 2, "both nodes survive a cycle");
        assert_eq!(t.ancestry(1), vec![1, 2], "the ring is walked once, not forever");
    }

    #[test]
    fn self_aggregation_is_ignored() {
        let f = parse(
            "#1=IFCBUILDING('b',$,'B',$,$,$,$,$,.ELEMENT.,$,$,$);\n\
             #2=IFCRELAGGREGATES('a',$,$,$,#1,(#1));\n",
        );
        let t = spatial_tree(&f);
        assert_eq!(t.nodes[&1].parent, None, "a node is not its own parent");
    }
}

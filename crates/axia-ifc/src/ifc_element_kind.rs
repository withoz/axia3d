//! What a member *is* (ADR-203 δ) — slab, column, beam, not everything a wall.
//!
//! Until now every exported member was an `IfcWall`. Opened in Revit or
//! ArchiCAD that reads as a building made entirely of walls: a floor slab is a
//! wall, a column is a wall. The geometry was right and the meaning was wrong.
//!
//! These are the IFC element types that share `IfcWall`'s attribute shape —
//! `(GlobalId, OwnerHistory, Name, Description, ObjectType, ObjectPlacement,
//! Representation, Tag, PredefinedType)`, nine attributes. `IfcDoor` and
//! `IfcWindow` are deliberately absent: they carry four more attributes
//! (`OverallHeight`, `OverallWidth`, `OperationType`,
//! `UserDefinedOperationType`), so emitting them here would produce a
//! malformed entity. They stay a follow-up rather than a guess.

/// A building element type we can export faithfully.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum IfcElementKind {
    /// The historical default — still what an unassigned member exports as.
    #[default]
    Wall,
    Slab,
    Column,
    Beam,
    Roof,
    Stair,
    Ramp,
    Railing,
    Covering,
    Member,
    Plate,
    Footing,
    /// "A building element we are not classifying" — the IFC-sanctioned way to
    /// say so, rather than mislabelling it as something specific.
    Proxy,
}

impl IfcElementKind {
    /// The STEP entity tag.
    pub fn tag(self) -> &'static str {
        match self {
            Self::Wall => "IFCWALL",
            Self::Slab => "IFCSLAB",
            Self::Column => "IFCCOLUMN",
            Self::Beam => "IFCBEAM",
            Self::Roof => "IFCROOF",
            Self::Stair => "IFCSTAIR",
            Self::Ramp => "IFCRAMP",
            Self::Railing => "IFCRAILING",
            Self::Covering => "IFCCOVERING",
            Self::Member => "IFCMEMBER",
            Self::Plate => "IFCPLATE",
            Self::Footing => "IFCFOOTING",
            Self::Proxy => "IFCBUILDINGELEMENTPROXY",
        }
    }

    /// Parse a tag (or a bare name like `"slab"`) back to a kind.
    ///
    /// Accepts what our own importer reports (`IFCSLAB`) and what a UI would
    /// send (`slab`), so the two never drift into separate vocabularies.
    pub fn from_tag(s: &str) -> Option<Self> {
        let t = s.trim().to_ascii_uppercase();
        let t = t.strip_prefix("IFC").unwrap_or(&t);
        Some(match t {
            "WALL" | "WALLSTANDARDCASE" => Self::Wall,
            "SLAB" | "SLABSTANDARDCASE" => Self::Slab,
            "COLUMN" | "COLUMNSTANDARDCASE" => Self::Column,
            "BEAM" | "BEAMSTANDARDCASE" => Self::Beam,
            "ROOF" => Self::Roof,
            "STAIR" => Self::Stair,
            "RAMP" => Self::Ramp,
            "RAILING" => Self::Railing,
            "COVERING" => Self::Covering,
            "MEMBER" | "MEMBERSTANDARDCASE" => Self::Member,
            "PLATE" | "PLATESTANDARDCASE" => Self::Plate,
            "FOOTING" => Self::Footing,
            "BUILDINGELEMENTPROXY" | "PROXY" => Self::Proxy,
            _ => return None,
        })
    }

    /// Stable short key for bridges and UI (`"slab"`).
    pub fn key(self) -> &'static str {
        match self {
            Self::Wall => "wall",
            Self::Slab => "slab",
            Self::Column => "column",
            Self::Beam => "beam",
            Self::Roof => "roof",
            Self::Stair => "stair",
            Self::Ramp => "ramp",
            Self::Railing => "railing",
            Self::Covering => "covering",
            Self::Member => "member",
            Self::Plate => "plate",
            Self::Footing => "footing",
            Self::Proxy => "proxy",
        }
    }

    /// Every kind, in the order a picker should show them.
    pub const ALL: &'static [IfcElementKind] = &[
        Self::Wall,
        Self::Slab,
        Self::Column,
        Self::Beam,
        Self::Roof,
        Self::Stair,
        Self::Ramp,
        Self::Railing,
        Self::Covering,
        Self::Member,
        Self::Plate,
        Self::Footing,
        Self::Proxy,
    ];
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_kind_round_trips_through_its_tag_and_key() {
        for k in IfcElementKind::ALL {
            assert_eq!(IfcElementKind::from_tag(k.tag()), Some(*k), "tag {}", k.tag());
            assert_eq!(IfcElementKind::from_tag(k.key()), Some(*k), "key {}", k.key());
        }
    }

    #[test]
    fn tags_and_keys_are_unique() {
        // A collision would silently merge two kinds on the way through a bridge.
        let mut tags: Vec<&str> = IfcElementKind::ALL.iter().map(|k| k.tag()).collect();
        let n = tags.len();
        tags.sort_unstable();
        tags.dedup();
        assert_eq!(tags.len(), n, "duplicate tag");

        let mut keys: Vec<&str> = IfcElementKind::ALL.iter().map(|k| k.key()).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), n, "duplicate key");
    }

    #[test]
    fn standard_case_variants_map_to_the_base_kind() {
        // Revit writes IfcWallStandardCase; it is still a wall.
        assert_eq!(IfcElementKind::from_tag("IFCWALLSTANDARDCASE"), Some(IfcElementKind::Wall));
        assert_eq!(IfcElementKind::from_tag("IfcSlabStandardCase"), Some(IfcElementKind::Slab));
    }

    #[test]
    fn unknown_and_unsupported_tags_are_rejected_not_guessed() {
        assert_eq!(IfcElementKind::from_tag("IFCNOSUCHTHING"), None);
        assert_eq!(IfcElementKind::from_tag(""), None);
        // Door and window carry four more attributes; refusing them here is
        // what keeps us from emitting a malformed entity.
        assert_eq!(IfcElementKind::from_tag("IFCDOOR"), None);
        assert_eq!(IfcElementKind::from_tag("IFCWINDOW"), None);
    }

    #[test]
    fn the_default_is_wall_so_existing_files_do_not_change() {
        assert_eq!(IfcElementKind::default(), IfcElementKind::Wall);
        assert_eq!(IfcElementKind::default().tag(), "IFCWALL");
    }
}

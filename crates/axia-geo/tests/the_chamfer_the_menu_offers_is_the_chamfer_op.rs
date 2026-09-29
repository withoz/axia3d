//! THE CHAMFER THE MENU OFFERS MUST BE THE CHAMFER OP.
//!
//! `엣지 챔퍼 (Chamfer)…` is a registered command with a label, a handler, an
//! OperationLog entry and an entry in both catalogs, so every wiring guard in
//! the repo passes on it. The handler called `filletEdge(edge, distance, 1)`
//! with this reasoning, written in the source:
//!
//!   "Chamfer is a degenerate Fillet with only one strip segment — so instead
//!    of an arc between the rolled-back points, a single flat quad connects
//!    them. Delegating to filletEdge(edge, distance, 1) keeps the code path
//!    unified."
//!
//! The reasoning is geometrically sound and the call is not: `fillet_edge`
//! opens with `ensure!(segments >= 2)`. Nothing between the menu and the engine
//! clamps it — not the bridge wrapper (`segments = 8` is only a DEFAULT, and
//! the handler passes 1 explicitly), not the WASM entry. So the menu item
//! failed on every click, and the guards could not see it because they check
//! that a call REACHES the engine, never that the engine accepts its arguments.
//!
//! `chamfer_edge` is the operation it wanted, and it was already there: in
//! axia-geo since ADR-024's sibling work, exported as `chamferEdge` with the
//! same transaction and closure-preserving gate wiring as `filletEdge`, and
//! called by the MCP server. Only the browser was reaching past it.
//!
//! ⚠ The two are NOT interchangeable even where both succeed. `chamfer_edge`
//! rejects a distance that overshoots an incident edge; `fillet_edge` has no
//! such guard, and its own source says why that matters — the facet "folds
//! through a neighbour, a manifold-but-self-intersecting result no downstream
//! check catches". So routing the menu at the fillet was also giving the
//! browser the unguarded path while the MCP agent got the guarded one.

use axia_geo::mesh::Mesh;
use axia_geo::{EdgeId, FaceId, MaterialId};
use glam::DVec3;

fn mat() -> MaterialId {
    MaterialId::new(0)
}

fn box_mesh() -> Mesh {
    let mut m = Mesh::new();
    m.create_box(DVec3::ZERO, 100.0, 100.0, 100.0, mat()).expect("box");
    m
}

fn is_closed(m: &Mesh) -> bool {
    let active: Vec<FaceId> =
        m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
    m.face_set_manifold_info(&active).is_closed_solid
}

/// An edge of the box shared by exactly two active faces — every edge of a box
/// is, so this just takes the first.
fn an_edge(m: &Mesh) -> EdgeId {
    m.edges
        .iter()
        .filter(|(_, e)| e.is_active())
        .map(|(id, _)| id)
        .find(|&id| {
            let (fs, _) = m.get_faces_sharing_edge(id);
            fs.iter().filter(|&&f| m.faces.contains(f) && m.faces[f].is_active()).count() == 2
        })
        .expect("a box edge with two faces")
}

/// What the menu item did until 2026-09-29.
#[test]
fn a_one_segment_fillet_is_refused_outright() {
    let mut m = box_mesh();
    let e = an_edge(&m);
    let r = m.fillet_edge(e, 20.0, 1);
    let msg = r.as_ref().err().map(|x| x.to_string()).unwrap_or_default();
    println!("  fillet_edge(e, 20, 1) -> {msg}");
    assert!(r.is_err(), "a 1-segment fillet must be refused — that is the whole finding");
    assert!(
        msg.contains("segments"),
        "and refused FOR the segment count, so the fix is the call and not the geometry: {msg}"
    );
    assert_eq!(
        m.faces.iter().filter(|(_, f)| f.is_active()).count(),
        6,
        "a refusal leaves the box alone"
    );
}

/// What it does now.
#[test]
fn the_chamfer_op_takes_the_same_edge_and_the_same_distance() {
    let mut m = box_mesh();
    let e = an_edge(&m);
    let before = m.faces.iter().filter(|(_, f)| f.is_active()).count();
    let r = m.chamfer_edge(e, 20.0).expect("chamfer_edge accepts what the menu asks");
    let after = m.faces.iter().filter(|(_, f)| f.is_active()).count();
    println!("  chamfer_edge(e, 20) -> facet {:?}, faces {before} -> {after}", r.chamfer_face);
    assert_eq!(after, before + 1, "one flat facet replaces the edge");
    assert!(is_closed(&m), "and the solid stays closed");
    let inv = m.verify_face_invariants();
    assert!(inv.is_valid(), "and sound: {:?}", inv.violations);
    assert_eq!(m.detect_self_intersections().count(), 0, "and pierces nothing");
}

/// The guard the fillet path does not have, and the reason the two are not
/// interchangeable: a distance longer than the edges it sets back along.
#[test]
fn an_overshooting_distance_is_refused_before_anything_is_torn_down() {
    let mut m = box_mesh();
    let e = an_edge(&m);
    let before = m.faces.iter().filter(|(_, f)| f.is_active()).count();
    let r = m.chamfer_edge(e, 500.0);
    println!("  chamfer_edge(e, 500) -> {:?}", r.as_ref().err().map(|x| x.to_string()));
    assert!(r.is_err(), "500mm cannot be set back along a 100mm edge");
    assert_eq!(
        m.faces.iter().filter(|(_, f)| f.is_active()).count(),
        before,
        "and the refusal leaves the box untouched (ADR-302)"
    );
    assert!(is_closed(&m), "still closed");
}

//! WHAT A FACE IS LEFT NAMING AFTER ITS EDGE IS REMOVED — AND WHY THE OBVIOUS
//! REPAIRS ARE WORSE.
//!
//! `Mesh::remove_edge_and_halfedges` repoints each endpoint vertex's `outgoing`
//! to a surviving half-edge (F1) and does nothing of the sort for FACES. A live
//! face is left with `outer().start` pointing at a half-edge that is gone, and
//! reading it says "cannot collect outer loop: HalfEdge HeId(N) not found".
//!
//! CLAUDE.md LOCKED #88 carried this as a known mechanism whose reach path was
//! unconfirmed. It is reached by widening `face_rederive`'s on-plane volume-edge
//! filter (see `axia-core/tests/what_overlapping_draws_leave_on_the_ground.rs`),
//! and `v_ring_cleans_up_on_edge_removal` pins today's behaviour with a note
//! saying to rewrite it if the function ever starts repairing.
//!
//! ⚠ Three repairs were built and measured, and none is worth shipping. This
//! file exists so the fourth attempt starts from that rather than from scratch.
//!
//! ```text
//!   repoint the loop start to a survivor   works, and is NOT enough — the
//!                                          start becomes live and the loop
//!                                          still does not collect, because
//!                                          nothing splices `next` around the
//!                                          half-edge that went. It buys no
//!                                          measurable behaviour and breaks
//!                                          the pinned test for nothing.
//!   refuse the removal instead             15 tests, 5 suites. The cleanup
//!                                          paths genuinely need those edges
//!                                          gone; refusing is too blunt.
//!   retire the face                        3 suites. Closest of the three,
//!                                          still not free.
//!   splice the loop closed                 not built: a triangle minus an edge
//!                                          is a two-edge ring, which is not a
//!                                          face by any reading.
//! ```
//!
//! The measurement below is what all of that was reasoned from, and it is the
//! part worth keeping: there IS somewhere to repoint to, and repointing there
//! is still not a repair.
use axia_geo::{Mesh, MaterialId};
use glam::DVec3;

#[test]
fn where_the_dead_reference_comes_from() {
    let mut mesh = Mesh::new();
    let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
    let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
    let v2 = mesh.add_vertex(DVec3::new(0.5, 1.0, 0.0));
    let fid = mesh.add_face(&[v0, v1, v2], MaterialId::new(0)).unwrap();

    let start_before = mesh.faces[fid].outer().start;
    let loop_before: Vec<_> = mesh.collect_loop_hes(start_before).unwrap_or_default();
    let eid = mesh.find_edge(v0, v1).unwrap();

    // Which half-edges belong to the edge about to go?
    let mut of_edge = Vec::new();
    for (hid, he) in mesh.hes.iter() {
        if he.edge() == eid {
            of_edge.push(hid);
        }
    }

    println!("  face outer start   {start_before:?}");
    println!("  loop               {loop_before:?}");
    println!("  edge {eid:?} half-edges  {of_edge:?}");
    println!("  start is on that edge?  {}", of_edge.contains(&start_before));

    mesh.remove_edge_and_halfedges(eid).unwrap();

    let start_after = mesh.faces[fid].outer().start;
    println!("  after removal, start {start_after:?}   still alive? {}",
        mesh.hes.contains(start_after));
    println!("  loop collects?       {}", mesh.collect_loop_verts(start_after).is_ok());
    // How much of the old loop survived — is there anything to repoint TO?
    let survivors: Vec<_> = loop_before.iter().copied().filter(|h| mesh.hes.contains(*h)).collect();
    println!("  survivors of the old loop  {survivors:?}");

    // ⚠ Asserted as measured, not as wanted. The day `remove_edge_and_halfedges`
    // starts repairing or retiring, these three lines are what say so — rewrite
    // them to state what it does instead, the way
    // `v_ring_cleans_up_on_edge_removal` asks.
    assert!(
        !mesh.hes.contains(start_after),
        "the loop start survived — the function now repairs; say what it does"
    );
    assert!(
        mesh.collect_loop_verts(start_after).is_err(),
        "the loop collects now — same, rewrite rather than delete"
    );
    assert_eq!(
        survivors.len(),
        2,
        "two of the three half-edges outlive the removal, so a repoint has          somewhere to go — this is what makes the first repair look right"
    );
}

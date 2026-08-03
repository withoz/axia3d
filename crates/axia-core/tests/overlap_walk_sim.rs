//! SIMULATION — is the spare half-edge really what decides it?
//!
//! Measured: a box built by extruding a drawn profile carries 4 half-edges on
//! each ground edge (2 with faces, 2 spare); `create_box` carries 2 and no
//! spare. The region-closing walk collects only face-less half-edges, and only
//! the extruded box lets an overlapping ground draw close the outer region.
//!
//! Two earlier attempts got this wrong and are worth recording. The first
//! hand-built the wire inside axia-geo and never reproduced the failure, so its
//! "spares change nothing" result meant nothing. The second used the real draw
//! path but removed the spares by DEACTIVATING them — and `edge_has_free_he`
//! (mesh.rs:11013) walks the radial ring without checking `is_active`, so it
//! still saw the slot and the removal did nothing. Both looked like clean
//! refutations of a correct hypothesis.
//!
//! Removing them properly — giving each spare the face its edge already borders,
//! so the ring reports the edge as fully used — turns the working box into the
//! failing one. That is the causal link.
//!
//! The gate itself is `detect_loop_by_chain_walk_excluding` (mesh.rs:10810):
//!     if !self.edge_has_free_he(edge_id) { continue; }
//! The chain walk refuses to step along an edge whose half-edge slots are full,
//! so it cannot route around the solid's outline and never closes the region
//! outside it.
//!
//! AND THE OBVIOUS FIX DOES NOT WORK. Deleting that gate — letting the walk step
//! onto a full edge — leaves C exactly as it was. The line after it reads
//!
//!     if neighbors.len() == 1 { …continue… } else { return None; }
//!
//! so the walk only proceeds when a vertex offers exactly ONE way on. The gate is
//! not a restriction to lift; it is what keeps the walk deterministic, by making
//! sure the used-up edges never appear as candidates. Remove it and a vertex
//! offers several, the walk gives up on the spot, and nothing is gained.
//!
//! What a fix needs is therefore a RULE FOR CHOOSING among branches — the
//! leftmost-turn a planar walk normally uses — not a looser filter. That is a
//! real change to the walk, and this simulation exists so nobody spends a week
//! discovering the cheap version does nothing.
//!
//! Nothing here changes production behaviour.

use axia_core::{Command, CommandResult, Scene, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, EdgeId, FaceId};
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

fn faces(s: &Scene) -> usize {
    s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count()
}

/// Faces lying entirely on z=0.
fn ground_faces(s: &Scene) -> Vec<FaceId> {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(i, _)| i)
        .filter(|&i| {
            s.mesh.collect_loop_verts(s.mesh.faces[i].outer().start).map_or(false, |vs| {
                !vs.is_empty()
                    && vs.iter().all(|v| s.mesh.vertex_pos(*v).map_or(false, |p| p.z.abs() < 1e-6))
            })
        })
        .collect()
}

fn ground_edges(s: &Scene) -> Vec<EdgeId> {
    s.mesh
        .edges
        .iter()
        .filter(|(_, e)| e.is_active())
        .map(|(i, _)| i)
        .filter(|&e| {
            let ed = &s.mesh.edges[e];
            matches!(
                (s.mesh.vertex_pos(ed.v_small()), s.mesh.vertex_pos(ed.v_large())),
                (Ok(a), Ok(b)) if a.z.abs() < 1e-6 && b.z.abs() < 1e-6
            )
        })
        .collect()
}

/// (half-edges on this edge, of which face-less)
fn slots(s: &Scene, e: EdgeId) -> (usize, usize) {
    let mut total = 0;
    let mut spare = 0;
    for (_h, he) in s.mesh.hes.iter() {
        if he.edge() != e || !he.is_active() {
            continue;
        }
        total += 1;
        if he.face().is_null() {
            spare += 1;
        }
    }
    (total, spare)
}

fn spare_total(s: &Scene) -> usize {
    ground_edges(s).iter().map(|&e| slots(s, e).1).sum()
}

/// box z in [0,100], footprint [0,200]^2, grown from a drawn rectangle.
fn extruded_box(s: &mut Scene) {
    s.execute(Command::DrawRectAsShape {
        center: DVec3::new(100.0, 100.0, 0.0),
        normal: DVec3::Z,
        up: DVec3::Y,
        width: 200.0,
        height: 200.0,
    });
    let f = s.mesh.faces.iter().filter(|(_, x)| x.is_active()).map(|(i, _)| i).next().unwrap();
    s.execute(Command::CreateSolid {
        face_id: f,
        mode: CreateSolidMode::Extrude { distance: 100.0 },
    });
}

/// same box, built in one call.
fn one_shot_box(s: &mut Scene) {
    let f = s
        .mesh
        .create_box(DVec3::new(100.0, 100.0, 50.0), 200.0, 100.0, 200.0, FORM_MATERIAL)
        .unwrap();
    s.create_xia_with_faces("box".into(), DVec3::ZERO, f);
}

/// THE REMOVAL — take every spare half-edge on the ground edges out of play, as
/// `edge_has_free_he` measures it.
fn strip_spares(s: &mut Scene) -> usize {
    let targets: Vec<_> = {
        let mut v = Vec::new();
        for e in ground_edges(s) {
            for (h, he) in s.mesh.hes.iter() {
                if he.edge() == e && he.is_active() && he.face().is_null() {
                    v.push(h);
                }
            }
        }
        v
    };
    // `edge_has_free_he` walks the radial ring and does NOT check `is_active`,
    // so deactivating is not enough — it still sees the slot. Give each spare a
    // face instead: the ring then reports the edge as fully used, which is what
    // "no spare" has to mean for the walk. Point it at the face the edge already
    // borders, so nothing new is claimed.
    for h in &targets {
        let e = s.mesh.hes[*h].edge();
        let owner = s
            .mesh
            .hes
            .iter()
            .find(|(_, x)| x.edge() == e && !x.face().is_null())
            .map(|(_, x)| x.face());
        if let Some(f) = owner {
            s.mesh.hes[*h].set_face(f);
        }
    }
    targets.len()
}

/// the overlapping ground rectangle, drawn the way the tool draws it —
/// four lines, the last one closing the loop.
fn draw_overlap(s: &mut Scene) -> bool {
    let c = [
        DVec3::new(100.0, 100.0, 0.0),
        DVec3::new(300.0, 100.0, 0.0),
        DVec3::new(300.0, 300.0, 0.0),
        DVec3::new(100.0, 300.0, 0.0),
    ];
    let mut ok = true;
    for i in 0..4 {
        let r = s.execute(Command::DrawLine {
            start: c[i],
            end: c[(i + 1) % 4],
            surface_normal: Some(DVec3::Z),
        });
        if matches!(r, CommandResult::Error(_)) {
            ok = false;
        }
    }
    ok
}

fn row(name: &str, s: &Scene, before_faces: usize, spares_before: usize) {
    let gf = ground_faces(s);
    println!(
        "{:<40} faces {}→{}  ground면 {}  spare(before) {}  nm {}  SI {}  inv {}",
        name,
        before_faces,
        faces(s),
        gf.len(),
        spares_before,
        s.mesh.collect_non_manifold_edges().len(),
        s.mesh.detect_self_intersections().count(),
        s.mesh.verify_face_invariants().violations.len(),
    );
}

#[test]
fn causation_by_removal() {
    println!();

    // A — the case that works, untouched.
    let mut s = prod();
    extruded_box(&mut s);
    let (b, sp) = (faces(&s), spare_total(&s));
    assert!(sp > 0, "an extruded box is supposed to carry spares");
    draw_overlap(&mut s);
    row("A 돌출 박스 (그대로)", &s, b, sp);
    let a_ground = ground_faces(&s).len();
    assert_eq!(a_ground, 3, "with spares the outer region closes → 3 ground faces");

    // B — the same box with its spares removed. It behaves like C, which is what
    //     makes the link causal rather than correlated.
    let mut s = prod();
    extruded_box(&mut s);
    let b = faces(&s);
    let stripped = strip_spares(&mut s);
    let sp = spare_total(&s);
    println!("   (제거한 spare half-edge: {stripped})");
    assert_eq!(sp, 0, "the removal must actually leave no spare");
    draw_overlap(&mut s);
    row("B 돌출 박스 − spare 제거", &s, b, sp);
    let b_ground = ground_faces(&s).len();

    // C — the case that fails.
    let mut s = prod();
    one_shot_box(&mut s);
    let (b, sp) = (faces(&s), spare_total(&s));
    draw_overlap(&mut s);
    row("C 한 번에 찍은 박스", &s, b, sp);
    let c_ground = ground_faces(&s).len();

    // The point of the whole exercise: taking the spares away turns the box that
    // works into the box that does not. Correlation would leave B at 3.
    assert_eq!(
        b_ground, c_ground,
        "stripping the spares must reproduce the one-shot box exactly —          if this ever differs, the spare slot is no longer what decides it"
    );
    assert_eq!(b_ground, 2, "and that means the outer region does not close");
}

/// The removal must not be the thing that broke it — check the mesh is still
/// sound with the spares gone but nothing drawn.
#[test]
fn removal_alone_is_harmless() {
    let mut s = prod();
    extruded_box(&mut s);
    let before = (faces(&s), s.mesh.verify_face_invariants().violations.len());
    let stripped = strip_spares(&mut s);
    let after = (faces(&s), s.mesh.verify_face_invariants().violations.len());
    println!(
        "\nspare {stripped}개 제거만: faces {}→{}  inv {}→{}",
        before.0, after.0, before.1, after.1
    );
}

/// The walk's determinism, pinned. If this ever stops being "exactly one way
/// on", the note at the top of this file — that the gate cannot simply be
/// deleted — needs rechecking, because the reasoning rests on it.
#[test]
fn the_walk_only_proceeds_when_there_is_one_way_on() {
    let src = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../axia-geo/src/mesh.rs"),
    )
    .expect("mesh.rs");
    let at = src.find("fn detect_loop_by_chain_walk_excluding").expect("walk not found");
    let body = &src[at..at + 2500];
    // The walk collects neighbours in two places; both carry the gate, and
    // dropping either one changes what the candidate set can contain.
    assert_eq!(
        body.matches("if !self.edge_has_free_he(edge_id) { continue; }").count(),
        2,
        "a free-half-edge gate is gone — re-run the simulation, its conclusion          assumed the gate is what keeps the candidate set down to one"
    );
    assert!(
        body.contains("if neighbors.len() == 1 {"),
        "the walk no longer requires exactly one continuation — a choosing rule          may now exist, which is what the overlap fix was waiting for"
    );
}

//! What a stale ownership entry can and cannot do.
//!
//! `slice_volume_by_plane` clears `face_to_xia` for the faces it consumes, and
//! removing that line fails nothing — not the ownership grid, not
//! `the_reverse_index_forgets_them_too`, not the workspace. Rather than guess
//! whether that means the line is unnecessary, this measures the property the
//! answer turns on.
//!
//! The hazard a stale reverse entry would pose is MIS-ATTRIBUTION: a later face
//! handed the same raw id, with `get_xia_for_face` still answering for the one
//! that is gone. That requires ids to be reused, and in this engine they are
//! not — `SlotStorage::insert` reads a monotonic `next_id` and increments it,
//! and `remove` only drops the map entry. The one test here says so, and fails
//! when `insert` is changed to recycle the lowest free slot.
//!
//! ⚠ Four attempts were made to write a second test that fails when the slice
//! cleanup is removed, and none of them did — including with id recycling
//! turned on at the same time, which is the only way the entry could ever be
//! consulted for a different face. "The index only answers for live faces" and
//! "the index and the owner list agree" both passed under both mutations.
//! There is no second test here because there is nothing measurable to guard:
//! see the note left at the line itself.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::FaceId;
use glam::DVec3;

const H: f64 = 200.0;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// The property everything else rests on: a face id, once spent, is spent.
///
/// Not a claim about slotmaps in general — `SlotStorage` is this repo's own,
/// and a version that recycled would make every stale reverse entry a
/// mis-attribution waiting to happen.
#[test]
fn a_removed_face_never_gives_its_id_to_a_later_one() {
    let mut s = prod();
    let first = s
        .mesh
        .create_box(DVec3::new(0.0, 0.0, H / 2.0), H, H, H, FORM_MATERIAL)
        .expect("box");
    let spent: Vec<FaceId> = first.clone();
    for f in &spent {
        let _ = s.mesh.remove_face(*f);
    }

    // Build again, several times, so a recycling store would have every chance.
    let mut later: Vec<FaceId> = Vec::new();
    for k in 0..3 {
        let more = s
            .mesh
            .create_box(
                DVec3::new(500.0 * (k as f64 + 1.0), 0.0, H / 2.0),
                H,
                H,
                H,
                FORM_MATERIAL,
            )
            .expect("box");
        later.extend(more);
    }
    println!(
        "SPENT {:?} … LATER {:?}",
        &spent[..spent.len().min(3)],
        &later[..later.len().min(3)]
    );
    for f in &later {
        assert!(
            !spent.contains(f),
            "face {f:?} was handed out again after being removed — every stale \
             ownership entry is then a mis-attribution"
        );
    }
}

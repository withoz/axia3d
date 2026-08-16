//! One question, asked of every operation that changes topology: **does the
//! owner still name only faces that exist?**
//!
//! This is Track E's measurement, and it is narrow on purpose. Ownership can
//! go wrong two ways and only one of them has ever cost anything here:
//!
//!   - An owner naming a DEAD id. This broke slice outright — `slice.rs` takes
//!     the list verbatim and aborts on the first id it cannot find, so a
//!     drilled solid could not be cut at all (PR #117) — and put an orphan
//!     element in an IFC file (PR #118).
//!   - A live face owned by nobody. Real, but a primitive box starts that way,
//!     so it only means something as a difference across an op.
//!
//! Both are measured below. What is NOT measured is which owner *ought* to win
//! when two disagree: that is a product decision, it was taken for merge
//! (first-selected inherits, 사용자 결재 2026-08-13), and it is pinned in
//! `a_merged_face_has_an_owner.rs` rather than guessed at here.
//!
//! ⚠ The grid asks each op on a solid that HAS an owner. A grid run on a bare
//! `create_box` would be silent about all of this, because nothing owns its six
//! faces to begin with — which is exactly why the first assertion in the merge
//! file is that a bare box is already unowned.
//!
//! **What this grid does NOT see**, both found by mutation rather than by
//! reading:
//!
//!  - A stale REVERSE index — `face_to_xia` still answering for a face that is
//!    gone. A third measure for it was written and then removed, because it
//!    never fired: with `adopt_new_active_faces` returning immediately and
//!    reconciling nothing at all, dead ids and orphans both appeared and the
//!    reverse-index count stayed zero for all twelve rows. A check that cannot
//!    fail is the vacuous-guard pattern this repo keeps being caught by, so it
//!    is gone rather than decorative. The question itself lives in
//!    `an_owner_does_not_name_faces_that_are_gone.rs`.
//!  - `slice_volume_by_plane`'s own `face_to_xia.remove(&fid)`. Deleting that
//!    line fails nothing — not this grid, not the reverse-index test, not the
//!    workspace. Whether it matters depends on whether a face slot is ever
//!    reused under a new owner, which has not been measured. Flagged, not
//!    fixed: an unguarded line is worth naming even when its consequence is
//!    unproven.
//!
//! Four mutations. Removing the prune gives dead ids on three rows; removing
//! the adopt gives orphans on the same three; removing both gives both; and
//! the fourth is the reverse-index one that nothing caught. Only the drill and
//! punch rows bite, because only they route through
//! `adopt_new_active_faces` — the others reconcile their own way and are
//! measured by outcome, which is the point of asking by outcome.

use axia_core::scene::Scene;
use axia_core::{Command, FORM_MATERIAL};
use axia_geo::{CreateSolidMode, FaceId};
use glam::DVec3;

const H: f64 = 200.0;

/// A box that belongs to a XIA, built the way the app builds one.
fn owned_solid() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    let faces = s
        .mesh
        .create_box(DVec3::new(0.0, 0.0, H / 2.0), H, H, H, FORM_MATERIAL)
        .expect("box");
    s.create_xia_with_faces("solid".into(), DVec3::ZERO, faces);
    s
}

/// Ids an owner names that are no longer live faces.
fn dead_ids(s: &Scene) -> usize {
    let live = |f: &FaceId| s.mesh.faces.get(*f).map_or(false, |x| x.is_active());
    let xia: usize = s.xias.values().flat_map(|x| x.face_ids.iter()).filter(|f| !live(f)).count();
    let shape: usize = s.shapes.values().flat_map(|x| x.face_ids.iter()).filter(|f| !live(f)).count();
    xia + shape
}

/// Live faces nobody names.
fn unowned(s: &Scene) -> usize {
    s.mesh
        .faces
        .iter()
        .filter(|(fid, f)| {
            f.is_active() && s.get_xia_for_face(*fid).is_none() && !s.face_to_shape.contains_key(fid)
        })
        .count()
}

fn top_face(s: &Scene) -> FaceId {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .max_by(|(_, a), (_, b)| {
            let za = a.normal().normalize_or_zero().z;
            let zb = b.normal().normalize_or_zero().z;
            za.partial_cmp(&zb).unwrap()
        })
        .map(|(fid, _)| fid)
        .expect("a top face")
}

type Op = (&'static str, fn(&mut Scene) -> Result<(), String>);

fn ops() -> Vec<Op> {
    vec![
        ("면 위에 사각형 그리기", |s: &mut Scene| {
            match s.execute(Command::DrawRectAsShape {
                center: DVec3::new(0.0, 0.0, H),
                normal: DVec3::Z,
                up: DVec3::X,
                width: 80.0,
                height: 80.0,
            }) {
                axia_core::CommandResult::Error(e) => Err(e),
                _ => Ok(()),
            }
        }),
        ("면 위에 원 그리기", |s: &mut Scene| {
            match s.execute(Command::DrawCircleAsShape {
                center: DVec3::new(0.0, 0.0, H),
                normal: DVec3::Z,
                radius: 50.0,
                segments: 24,
            }) {
                axia_core::CommandResult::Error(e) => Err(e),
                _ => Ok(()),
            }
        }),
        ("면 가로질러 선 긋기", |s: &mut Scene| {
            match s.execute(Command::DrawLine {
                start: DVec3::new(-120.0, 0.0, H),
                end: DVec3::new(120.0, 0.0, H),
                surface_normal: Some(DVec3::Z),
            }) {
                axia_core::CommandResult::Error(e) => Err(e),
                _ => Ok(()),
            }
        }),
        ("사각 구멍 관통", |s: &mut Scene| {
            s.drill_rect_through_hole(
                DVec3::new(-40.0, -H / 2.0, H / 2.0 - 40.0),
                DVec3::new(40.0, -H / 2.0, H / 2.0 + 40.0),
                -DVec3::Y,
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        }),
        ("원형 구멍 관통", |s: &mut Scene| {
            s.drill_circular_through_hole(DVec3::new(0.0, -H / 2.0, H / 2.0), -DVec3::Y, 40.0, 24)
                .map(|_| ())
                .map_err(|e| e.to_string())
        }),
        // On the TOP, not a wall. The wall placement returned Ok and changed
        // nothing — six faces before and after — so the row said "OK" about an
        // op that had not run.
        ("사각 홈 파기", |s: &mut Scene| {
            s.punch_rect_hole(
                DVec3::new(-40.0, -40.0, H),
                DVec3::new(40.0, 40.0, H),
                DVec3::Z,
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        }),
        // The user path the carve is written for: "draw a rect on a face →
        // push it in → pocket". Two wrong setups came first and both refused
        // while the row read a bare 거부 — a punched hole (the carve wants a
        // drawn SHEET, not a hole), and then the drawn sheet picked with
        // `.next()`, which takes whichever new face comes first and that is
        // the leftover RING, not the rect. The smallest new face is the rect.
        ("포켓 파기 (그린 면 밀어넣기)", |s: &mut Scene| {
            let before: Vec<FaceId> = s
                .mesh
                .faces
                .iter()
                .filter(|(_, f)| f.is_active())
                .map(|(fid, _)| fid)
                .collect();
            if let axia_core::CommandResult::Error(e) = s.execute(Command::DrawRectAsShape {
                center: DVec3::new(0.0, 0.0, H),
                normal: DVec3::Z,
                up: DVec3::X,
                width: 80.0,
                height: 80.0,
            }) {
                return Err(e);
            }
            let fresh: Vec<FaceId> = s
                .mesh
                .faces
                .iter()
                .filter(|(fid, f)| f.is_active() && !before.contains(fid))
                .map(|(fid, _)| fid)
                .collect();
            let drawn = fresh
                .into_iter()
                .min_by(|a, b| {
                    let aa = s.mesh.face_area(*a);
                    let bb = s.mesh.face_area(*b);
                    aa.partial_cmp(&bb).unwrap()
                })
                .ok_or_else(|| "the rect made no new face".to_string())?;
            match s.carve_pocket_from_source_face(drawn, 40.0) {
                axia_core::CommandResult::Error(e) => Err(e),
                _ => Ok(()),
            }
        }),
        ("돌출 (윗면 밀기)", |s: &mut Scene| {
            let f = top_face(s);
            match s.execute(Command::CreateSolid {
                face_id: f,
                mode: CreateSolidMode::Extrude { distance: 60.0 },
            }) {
                axia_core::CommandResult::Error(e) => Err(e),
                _ => Ok(()),
            }
        }),
        ("평면으로 자르기 (양쪽 유지)", |s: &mut Scene| {
            let owned: Vec<FaceId> = s
                .mesh
                .faces
                .iter()
                .filter(|(fid, f)| f.is_active() && s.get_xia_for_face(*fid).is_some())
                .map(|(fid, _)| fid)
                .collect();
            s.slice_volume_by_plane(
                &owned,
                axia_geo::operations::slice::SlicePlane {
                    origin: DVec3::new(0.0, 0.0, H / 2.0),
                    normal: DVec3::Z,
                },
            )
            .map(|_| ())
            .map_err(|e| e.to_string())
        }),
        ("평면으로 잘라내기 (한쪽만)", |s: &mut Scene| {
            let owned: Vec<FaceId> = s
                .mesh
                .faces
                .iter()
                .filter(|(fid, f)| f.is_active() && s.get_xia_for_face(*fid).is_some())
                .map(|(fid, _)| fid)
                .collect();
            s.trim_volume_by_plane(
                &owned,
                axia_geo::operations::slice::SlicePlane {
                    origin: DVec3::new(0.0, 0.0, H / 2.0),
                    normal: DVec3::Z,
                },
                true,
            )
            .map_err(|e| e.to_string())
        }),
        ("비-manifold 복구", |s: &mut Scene| {
            s.repair_non_manifold_edges();
            Ok(())
        }),
        ("고아 면 재합성", |s: &mut Scene| {
            s.resynthesize_orphan_faces();
            Ok(())
        }),
    ]
}

/// The grid.
#[test]
fn no_operation_leaves_an_owner_naming_a_face_that_is_gone() {
    println!("\n연산별 소유권 — 죽은 id 를 남기는가, 살아있는 면을 버리는가\n");
    let mut fails: Vec<String> = Vec::new();
    for (name, op) in ops() {
        let mut s = owned_solid();
        let (dead0, unowned0) = (dead_ids(&s), unowned(&s));
        assert_eq!(dead0, 0, "{name}: the fixture starts clean");
        assert_eq!(unowned0, 0, "{name}: the fixture starts fully owned");

        let outcome = op(&mut s);
        let (dead1, unowned1) = (dead_ids(&s), unowned(&s));
        let live = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();

        let verdict = match &outcome {
            // A refusal is a fine answer — but it must not have cost anything.
            Err(e) => {
                if dead1 != 0 || unowned1 != 0 {
                    fails.push(format!("{name} — 거부했는데 죽은 id {dead1} / 무소유 {unowned1}: {e}"));
                    "거부(손상)".to_string()
                } else {
                    // A refusal is only informative if it says what it refused.
                    // A row reading a bare "거부" cannot be told apart from an
                    // op that never ran.
                    format!("거부: {}", e.chars().take(40).collect::<String>())
                }
            }
            Ok(()) => {
                let mut bad = Vec::new();
                if dead1 != 0 {
                    bad.push(format!("죽은 id {dead1}"));
                }
                if unowned1 != 0 {
                    bad.push(format!("무소유 {unowned1}"));
                }
                if bad.is_empty() {
                    "OK".to_string()
                } else {
                    fails.push(format!("{name} — {}", bad.join(" / ")));
                    bad.join(" / ")
                }
            }
        };
        println!("  {name:<24} {verdict:<22} 면 {live}");
    }
    println!("\n  실패 {}건", fails.len());
    for f in &fails {
        println!("    ✗ {f}");
    }
    assert_eq!(
        fails.len(),
        0,
        "an owner must name only faces that exist, and no live face may be orphaned: {fails:?}"
    );
}

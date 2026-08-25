//! `polygon_difference_by_clip` can hand back a ring that runs over its own
//! vertex — with a small, real input.
//!
//! This is the upstream end of the chain in
//! `a_split_between_collinear_points_leaves_no_area.rs` and
//! `the_door_a_spanning_edge_comes_through.rs`: a face whose boundary steps
//! over one of its own vertices makes a later cut produce a piece with no area,
//! and every attempt to suppress that downstream traded one symptom for a worse
//! one. The ring is malformed before anything touches it.
//!
//! ## Where this input came from
//!
//! Captured at `Scene::subtract_double_covered_faces`, which passes the
//! clipper's output straight to `add_face_with_holes` with only a `len >= 3`
//! check in between. Wide-fuzz session 12: 3 of 10 pieces come back
//! self-overlapping, 8 incidences, and already in 2D — `lift` and `add_vertex`
//! reproduce the counts exactly and `add_vertex` snaps nothing (max 2.8e-14,
//! float noise). The plane basis round-trips its inputs to 2.8e-14 too.
//!
//! ⚠ ADR-101 documents this clipper's MVP as assuming CONVEX input
//! (Sutherland-Hodgman). The base here is a plain rectangle; the clip is a
//! four-point quadrilateral that crosses it twice on each of two opposite
//! sides. That is the shape, not an exotic one.
//!
//! ## ⚠ Not a re-flag of LOCKED #105 — read this before filing it as one
//!
//! CLAUDE.md LOCKED #105 (2026-08-17) already names this function and says
//! "고츠다. 다시 결함으로 올리지 말 것" — fixed, do not re-flag. That is a
//! DIFFERENT shape in the same function, and the distinction is measurable:
//!
//! ```text
//!               LOCKED #105                    this file
//!   shape       a vertex appearing TWICE       distinct vertices, one edge
//!               (31-vert loop, 2 duplicates)   running over another
//!   duplicates  2                              0
//!   crossings   2                              4
//! ```
//!
//! And #105's own closing table records the gap this fills, honestly:
//!
//!   "짝수 교차 게이트 ... 4-교차 겹침을 만들면서 수리도 필요한 장면이 아직 없다"
//!   ("no scene yet makes a 4-crossing overlap that also needs repair")
//!
//! Wide-fuzz session 12 is that scene. So this is not a regression of #105's
//! fix and not a re-flag — it is the case #105 said nothing had reached.
//!
//! The input here also clears both gates #105 left in place: the clip is
//! convex (`is_convex_ccw_2d`, coplanar.rs:200 — all four cross products
//! positive) and the crossing count is even (coplanar.rs:1088). Convex clip,
//! convex subject, and the output still doubles back.
//!
//! ## What this file does NOT claim
//!
//! It does not say dropping such pieces is the fix — measured, and it is not:
//! filtering them where `len >= 3` is checked leaves session 12 at the same 2
//! final violations with the same zero-area face, because that face arrives by
//! the other route (`face_rederive::rebuild_inner`). See the door file.
//!
//! So this pins the DEFECT, not a repair. If someone fixes the clipper, this
//! test fails and tells them the reproducer is gone.

use axia_geo::operations::coplanar::{polygon_difference_by_clip, Crossing2d};

/// Does any edge of the ring run straight over another of its own vertices?
fn steps_over_own_vertex(poly: &[(f64, f64)]) -> Option<(usize, usize)> {
    let n = poly.len();
    for a in 0..n {
        let b = (a + 1) % n;
        let (dx, dy) = (poly[b].0 - poly[a].0, poly[b].1 - poly[a].1);
        let l = (dx * dx + dy * dy).sqrt();
        if l < 1e-9 {
            continue;
        }
        for c in 0..n {
            if c == a || c == b {
                continue;
            }
            let (wx, wy) = (poly[c].0 - poly[a].0, poly[c].1 - poly[a].1);
            let t = (wx * dx + wy * dy) / l;
            if t <= 1e-3 || t >= l - 1e-3 {
                continue;
            }
            let (px, py) = (wx - dx / l * t, wy - dy / l * t);
            if (px * px + py * py).sqrt() <= 1e-6 {
                return Some((a, c));
            }
        }
    }
    None
}

#[test]
fn a_rectangle_minus_a_crossing_quad_comes_back_doubled_over() {
    // Captured verbatim from session 12.
    let base: Vec<(f64, f64)> = vec![
        (0.0, 0.0),
        (50.0, 0.0),
        (50.0, 8.169_999),
        (-0.0, 8.169_999),
    ];
    let clip: Vec<(f64, f64)> = vec![
        (41.567_080, -9.486_393),
        (29.000_017, 74.479_354),
        (20.849_065, 73.921_813),
        (19.714_589, -20.809_441),
    ];
    let crossings = vec![
        Crossing2d { base_edge: 0, base_t: 0.399_276, clip_edge: 2, clip_t: 0.780_332,
                     point: (19.963_797, 0.0) },
        Crossing2d { base_edge: 0, base_t: 0.802_945, clip_edge: 0, clip_t: 0.112_979,
                     point: (40.147_262, 0.0) },
        Crossing2d { base_edge: 2, base_t: 0.221_511, clip_edge: 0, clip_t: 0.210_281,
                     point: (38.924_467, 8.169_999) },
        Crossing2d { base_edge: 2, base_t: 0.598_767, clip_edge: 2, clip_t: 0.694_088,
                     point: (20.061_639, 8.169_999) },
    ];

    // PREMISE: the inputs themselves are sound rings.
    assert_eq!(steps_over_own_vertex(&base), None, "the base is a plain rectangle");
    assert_eq!(steps_over_own_vertex(&clip), None, "the clip is a plain quad");

    let pieces = polygon_difference_by_clip(&base, &clip, &crossings)
        .expect("the clipper accepts this — that is the point");
    assert!(!pieces.is_empty(), "premise: it produced something to inspect");

    let offenders: Vec<(usize, usize, usize)> = pieces
        .iter()
        .enumerate()
        .filter_map(|(i, p)| steps_over_own_vertex(p).map(|(a, c)| (i, a, c)))
        .collect();

    assert!(
        !offenders.is_empty(),
        "this input is the reproducer for a ring that doubles back. If the \
         clipper stopped doing it, that is good news — capture a new reproducer \
         or delete this file. pieces: {:?}",
        pieces.iter().map(|p| p.len()).collect::<Vec<_>>()
    );

    let (i, a, c) = offenders[0];
    let p = &pieces[i];
    println!(
        "\n  piece {i} (len {}) — edge {a}->{} runs over vertex {c}\n    {:?} -> {:?}  over  {:?}\n",
        p.len(), (a + 1) % p.len(), p[a], p[(a + 1) % p.len()], p[c],
    );
}

/// The `len >= 3` check between the clipper and `add_face_with_holes` cannot
/// see this — which is why the ring reaches the door.
#[test]
fn length_alone_does_not_notice() {
    let base: Vec<(f64, f64)> = vec![(0.0, 0.0), (50.0, 0.0), (50.0, 8.169_999), (-0.0, 8.169_999)];
    let clip: Vec<(f64, f64)> = vec![
        (41.567_080, -9.486_393),
        (29.000_017, 74.479_354),
        (20.849_065, 73.921_813),
        (19.714_589, -20.809_441),
    ];
    let crossings = vec![
        Crossing2d { base_edge: 0, base_t: 0.399_276, clip_edge: 2, clip_t: 0.780_332,
                     point: (19.963_797, 0.0) },
        Crossing2d { base_edge: 0, base_t: 0.802_945, clip_edge: 0, clip_t: 0.112_979,
                     point: (40.147_262, 0.0) },
        Crossing2d { base_edge: 2, base_t: 0.221_511, clip_edge: 0, clip_t: 0.210_281,
                     point: (38.924_467, 8.169_999) },
        Crossing2d { base_edge: 2, base_t: 0.598_767, clip_edge: 2, clip_t: 0.694_088,
                     point: (20.061_639, 8.169_999) },
    ];
    let pieces = polygon_difference_by_clip(&base, &clip, &crossings).expect("accepted");
    for p in &pieces {
        if steps_over_own_vertex(p).is_some() {
            assert!(
                p.len() >= 3,
                "the offending ring passes the only check standing between the \
                 clipper and add_face_with_holes — len {} >= 3",
                p.len()
            );
            return;
        }
    }
    panic!("no self-overlapping piece — the reproducer changed, see the sibling test");
}

/// ⚠ Not one unlucky input — every base height tried does it.
///
/// Mutating the base rectangle's height, or moving a clip vertex far away, did
/// NOT stop the sibling test passing. That could mean it is weakly anchored, or
/// that the defect is broad. Measured: broad. Five heights, five
/// self-overlapping pieces. So the sibling's mutation survivors are the defect
/// generalising, not the guard failing to hold.
///
/// (An earlier run of this looked like a hang and was reported as one. It was
/// not: the test did not exist at that moment, so every case produced no output
/// and the long wall-clock was cargo rebuilding. Build first, then run.)
#[test]
fn it_is_not_one_unlucky_input() {
    let clip: Vec<(f64, f64)> = vec![
        (41.567_080, -9.486_393),
        (29.000_017, 74.479_354),
        (20.849_065, 73.921_813),
        (19.714_589, -20.809_441),
    ];
    let crossings = vec![
        Crossing2d { base_edge: 0, base_t: 0.399_276, clip_edge: 2, clip_t: 0.780_332,
                     point: (19.963_797, 0.0) },
        Crossing2d { base_edge: 0, base_t: 0.802_945, clip_edge: 0, clip_t: 0.112_979,
                     point: (40.147_262, 0.0) },
        Crossing2d { base_edge: 2, base_t: 0.221_511, clip_edge: 0, clip_t: 0.210_281,
                     point: (38.924_467, 8.169_999) },
        Crossing2d { base_edge: 2, base_t: 0.598_767, clip_edge: 2, clip_t: 0.694_088,
                     point: (20.061_639, 8.169_999) },
    ];

    let heights = [8.169_999_f64, 12.0, 20.0, 30.0, 50.0];
    let mut accepted = 0;
    let mut overlapping = 0;
    for h in heights {
        let base = vec![(0.0, 0.0), (50.0, 0.0), (50.0, h), (-0.0, h)];
        let Ok(pieces) = polygon_difference_by_clip(&base, &clip, &crossings) else { continue };
        accepted += 1;
        let bad = pieces.iter().filter(|p| steps_over_own_vertex(p).is_some()).count();
        println!("  h={h}  pieces {}  self-overlapping {bad}", pieces.len());
        if bad > 0 { overlapping += 1; }
    }

    assert_eq!(accepted, heights.len(), "premise: the clipper accepted all of them");
    assert_eq!(
        overlapping, heights.len(),
        "every height reproduced when measured. If some stop, the defect narrowed \
         — good news, but this file's breadth claim needs re-measuring"
    );
}

/// The control: the clipper does NOT do this to everything.
///
/// ⚠ Needed because one mutation — moving the base far from the clip — left the
/// tests above passing. That mutation is not fair: it moves the base but keeps
/// the ORIGINAL crossings, so the clipper gets inconsistent input and garbage
/// out proves nothing. This asks the honest version of the question instead.
#[test]
fn a_clean_difference_comes_back_clean() {
    // A square with a smaller square biting one corner, crossings computed by
    // hand so base and clip agree.
    let base = vec![(0.0, 0.0), (100.0, 0.0), (100.0, 100.0), (0.0, 100.0)];
    let clip = vec![(60.0, -20.0), (140.0, -20.0), (140.0, 40.0), (60.0, 40.0)];
    let crossings = vec![
        Crossing2d { base_edge: 0, base_t: 0.6, clip_edge: 3, clip_t: 0.75,
                     point: (60.0, 0.0) },
        Crossing2d { base_edge: 1, base_t: 0.4, clip_edge: 2, clip_t: 0.5,
                     point: (100.0, 40.0) },
    ];

    let pieces = polygon_difference_by_clip(&base, &clip, &crossings)
        .expect("a corner bite is the ordinary case");
    assert!(!pieces.is_empty(), "premise: it produced something");
    for (i, p) in pieces.iter().enumerate() {
        assert_eq!(
            steps_over_own_vertex(p),
            None,
            "piece {i} of an ordinary corner bite must be a sound ring: {p:?}"
        );
    }
}

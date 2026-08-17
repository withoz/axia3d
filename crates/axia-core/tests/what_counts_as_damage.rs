//! Stage 3.5: what counts as damage, said once.
//!
//! Two faces can be in contact three different ways, and only two of them are
//! wrong:
//!
//! ```text
//!   contact           two faces meeting at an ANGLE along a line.
//!                     A sheet drawn past a box's corner rests on the walls
//!                     below it exactly this way. Nothing is enclosed between
//!                     them. NOT damage.
//!   coplanar overlap  two faces on the SAME plane covering the same ground,
//!                     so the region is drawn twice. Damage.
//!   piercing          a face crossing another at an angle, poking through.
//!                     The flap class. Damage.
//! ```
//!
//! `detect_self_intersections` reports all three the same way — a flat
//! `Vec<(FaceId, FaceId)>` with no kind — and every caller re-derives the
//! distinction for itself. Measured: five places do it, four in production and
//! one in a grid, and no two the same way:
//!
//! ```text
//!   subtract_double_covered_faces   skip pairs that already overlapped
//!   split_faces_crossing_other_planes  planar faces only
//!   solid_overlap_pairs             at least one face is not a sheet
//!   guard_imprint                   the above, plus a before-set
//!   draw_freely_matrix (test)       |n_a · n_b| > 0.999
//! ```
//!
//! Before a rollback gate is tightened on top of that (stage 4), the direction
//! has to be settled, and settling it means measuring what the instruments
//! actually say about each of the three — not what their names suggest. This
//! file is that measurement.

use axia_core::scene::Scene;
use axia_core::FORM_MATERIAL;
use axia_geo::FaceId;
use glam::DVec3;

fn prod() -> Scene {
    let mut s = Scene::new();
    s.auto_intersect_on_draw = true;
    s.auto_face_synthesis_on_draw = true;
    s.face_rederive_on_draw = true;
    s.freeform_overlap_on_draw = true;
    s
}

/// The three readings, for one scene.
struct Readings {
    /// Every pair the detector reports.
    all: usize,
    /// Pairs whose two faces lie on the same plane — the region drawn twice.
    coplanar: usize,
    /// Pairs where at least one face belongs to a solid (`solid_overlap_pairs`).
    solid: usize,
}

fn read(s: &Scene) -> Readings {
    let pairs = s.mesh.detect_self_intersections().intersecting_pairs;
    let mut coplanar = 0;
    let mut solid = 0;
    for &(a, b) in &pairs {
        let (Some(fa), Some(fb)) = (s.mesh.faces.get(a), s.mesh.faces.get(b)) else {
            continue;
        };
        let na = fa.normal().normalize_or_zero();
        let nb = fb.normal().normalize_or_zero();
        if na.dot(nb).abs() > 0.999 {
            coplanar += 1;
        }
        if !s.mesh.is_sheet_face(a) || !s.mesh.is_sheet_face(b) {
            solid += 1;
        }
    }
    Readings { all: pairs.len(), coplanar, solid }
}

/// Perpendicular contact: a sheet meeting a wall along a line.
///
/// Built directly, for the same reason as the overlap below — drawing it past
/// a box's corner produces no reported pair at all today, because the
/// arrangement resolves the straddle (Track A). What is wanted here is the
/// bare configuration: two faces at right angles, touching and enclosing
/// nothing.
fn contact() -> Scene {
    let mut s = Scene::new();
    let floor: Vec<_> = [
        DVec3::new(-100.0, -100.0, 0.0),
        DVec3::new(100.0, -100.0, 0.0),
        DVec3::new(100.0, 100.0, 0.0),
        DVec3::new(-100.0, 100.0, 0.0),
    ]
    .iter()
    .map(|p| s.mesh.add_vertex(*p))
    .collect();
    s.mesh.add_face(&floor, FORM_MATERIAL).expect("floor");
    // A wall standing ON the floor: it touches along the line z = 0 and rises.
    let wall: Vec<_> = [
        DVec3::new(-50.0, 0.0, 0.0),
        DVec3::new(50.0, 0.0, 0.0),
        DVec3::new(50.0, 0.0, 100.0),
        DVec3::new(-50.0, 0.0, 100.0),
    ]
    .iter()
    .map(|p| s.mesh.add_vertex(*p))
    .collect();
    s.mesh.add_face(&wall, FORM_MATERIAL).expect("wall");
    s
}

/// Two sheets on one plane covering the same ground.
///
/// Built by adding the faces to the mesh directly, not by drawing them.
/// ⚠ Written first as two `DrawRectAsShape` calls with the auto flags off, and
/// that does NOT produce an overlap: the pair came out as three faces even on
/// `Scene::new()`, where `auto_intersect_on_draw` is false. Something on the
/// draw path divides them regardless of the flag, so the drawn version was
/// measuring the arrangement rather than the configuration. Constructing the
/// geometry is the only way to hold the two faces overlapping long enough to
/// ask what the detector says about them.
fn coplanar_overlap() -> Scene {
    let mut s = Scene::new();
    let quad = |s: &mut Scene, x0: f64, x1: f64| {
        let v: Vec<_> = [
            DVec3::new(x0, -100.0, 0.0),
            DVec3::new(x1, -100.0, 0.0),
            DVec3::new(x1, 100.0, 0.0),
            DVec3::new(x0, 100.0, 0.0),
        ]
        .iter()
        .map(|p| s.mesh.add_vertex(*p))
        .collect();
        s.mesh.add_face(&v, FORM_MATERIAL).expect("face");
    };
    quad(&mut s, -100.0, 100.0);
    quad(&mut s, -40.0, 160.0); // shifted, so they cover the same middle
    s
}

/// A face crossing another at an angle — the flap class.
///
/// A tall wall standing through a horizontal sheet, so the two genuinely cross
/// rather than meeting along an edge.
fn piercing() -> Scene {
    let mut s = Scene::new();
    let floor: Vec<_> = [
        DVec3::new(-100.0, -100.0, 0.0),
        DVec3::new(100.0, -100.0, 0.0),
        DVec3::new(100.0, 100.0, 0.0),
        DVec3::new(-100.0, 100.0, 0.0),
    ]
    .iter()
    .map(|p| s.mesh.add_vertex(*p))
    .collect();
    s.mesh.add_face(&floor, FORM_MATERIAL).expect("floor");
    // The same wall, dropped so it crosses the floor instead of standing on it.
    let wall: Vec<_> = [
        DVec3::new(-50.0, 0.0, -50.0),
        DVec3::new(50.0, 0.0, -50.0),
        DVec3::new(50.0, 0.0, 50.0),
        DVec3::new(-50.0, 0.0, 50.0),
    ]
    .iter()
    .map(|p| s.mesh.add_vertex(*p))
    .collect();
    s.mesh.add_face(&wall, FORM_MATERIAL).expect("wall");
    s
}

#[test]
fn the_three_kinds_read_differently() {
    println!("\n무엇이 손상인가 — 세 구성, 세 계기\n");
    println!("  {:<18} {:>6} {:>10} {:>8}", "구성", "전체", "coplanar", "solid");
    let mut rows = Vec::new();
    for (name, build) in [
        ("접촉 (모서리 걸침)", contact as fn() -> Scene),
        ("coplanar 겹침", coplanar_overlap as fn() -> Scene),
        ("관통 (직교 벽)", piercing as fn() -> Scene),
    ] {
        let s = build();
        let r = read(&s);
        let faces = s.mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let sheets = s
            .mesh
            .faces
            .iter()
            .filter(|(fid, f)| f.is_active() && s.mesh.is_sheet_face(*fid))
            .count();
        println!(
            "  {name:<18} {:>6} {:>10} {:>8}   (면 {faces}, 시트 {sheets})",
            r.all, r.coplanar, r.solid
        );
        rows.push((name, r));
    }

    let (_, contact_r) = &rows[0];
    let (_, overlap_r) = &rows[1];
    let (_, pierce_r) = &rows[2];

    // The direction, stated as the two claims that matter.
    //
    // Contact is not damage. The detector DOES report it — two faces meeting
    // at an angle do share tessellation contact — so a gate that counted every
    // reported pair would reject a shape drawn past a corner, which is a thing
    // users do on purpose.
    assert_eq!(
        contact_r.coplanar, 0,
        "perpendicular contact must not read as the same plane; that is the \
         test that separates it from an overlap"
    );

    // Coplanar overlap is damage, and is what the coplanar reading is for.
    assert!(
        overlap_r.coplanar > 0,
        "two sheets covering the same ground must read as coplanar — this is \
         the reading a gate can act on"
    );

    // Piercing is damage too, and is NOT coplanar — so a gate that only asked
    // "are they on the same plane?" would miss it. Both readings are needed.
    assert!(
        pierce_r.all > 0,
        "a wall standing through a sheet must be reported at all"
    );
    assert_eq!(
        pierce_r.coplanar, 0,
        "and it is not a coplanar overlap, so the coplanar reading alone is \
         not a damage test"
    );
}

/// The reading a gate can act on, as one function rather than five.
///
/// Now `Mesh::damaging_contacts`, so the direction lives in the engine beside
/// the detector rather than being re-derived at each call site. This wrapper
/// stays only to keep the test reading as the claim it is.
fn damaging_pairs(s: &Scene) -> Vec<(FaceId, FaceId)> {
    s.mesh.damaging_contacts()
}

#[test]
fn the_direction_holds_for_all_three() {
    let cases = [
        ("접촉", contact as fn() -> Scene, false),
        ("coplanar 겹침", coplanar_overlap as fn() -> Scene, true),
        ("관통", piercing as fn() -> Scene, true),
    ];
    for (name, build, expect_damage) in cases {
        let s = build();
        let d = damaging_pairs(&s);
        println!("  {name:<14} 손상 쌍 {}", d.len());
        assert_eq!(
            !d.is_empty(),
            expect_damage,
            "{name}: expected damage = {expect_damage}, got {} pair(s)",
            d.len()
        );
    }
}

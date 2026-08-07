//! A CURVED SOLID HAS A VOLUME, AND IT COMES FROM THE RADIUS.
//!
//! `face_set_volume` summed `p0·(pa×pb)` over each face's boundary polygon and
//! skipped anything with fewer than three vertices. Every Path B primitive has
//! exactly that shape — a sphere's boundary is one anchor on a closed curve — so
//! every one measured **0** and `promote_shape_to_xia` refused them all with
//! `ZeroVolume` (사용자 2026-08-07: "구의 부피는 지름 치수만 있으면 되지 원이
//! 있어야 하지 않습니다").
//!
//! The volume now comes from outward flux per face (divergence theorem), read
//! from the SURFACE PARAMETERS where they settle it. Measured 2026-08-07, and
//! the difference between the two ways of reading a surface is the whole point:
//!
//! ```text
//!   cylinder cap, from the parameters       785398.2   exact
//!   cylinder cap, tessellated              2250000.0   the Plane's u×v box is
//!                                                      150×150, not the r=50 disk
//! ```
//!
//! Tessellation stays the fall-back for surfaces with no closed form (cone,
//! torus, the NURBS family), where it measured well — cone 0.030%.
use axia_core::{Scene, FORM_MATERIAL};
use axia_geo::entities::MaterialId;
use glam::DVec3;

const PI: f64 = std::f64::consts::PI;

fn pathb() -> Scene {
    let mut s = Scene::new();
    s.mesh.set_sphere_path_b_default(true);
    s.mesh.set_cone_path_b_default(true);
    s.mesh.set_cylinder_path_b_default(true);
    s
}

fn check(name: &str, faces: Vec<axia_geo::FaceId>, mut s: Scene, truth: f64, tol_pct: f64) {
    let v = axia_core::promote::face_set_volume(&s.mesh, &faces);
    let err = (v - truth).abs() / truth * 100.0;
    assert!(
        err < tol_pct,
        "{name}: volume {v:.1}, expected {truth:.1} (error {err:.3}% ≥ {tol_pct}%)"
    );
    // and the whole point of having a volume: it can become a member
    let shape = s.create_shape(name.into(), faces);
    let ok = s.promote_shape_to_xia(shape, MaterialId::new(1));
    assert!(ok.is_ok(), "{name}: material should make it a XIA, got {:?}", ok.err());
}

/// Exact — the closed form is read straight off `Sphere { radius }`.
#[test]
fn a_sphere_has_the_volume_its_radius_says() {
    let mut s = pathb();
    let f = s.mesh.create_sphere(DVec3::ZERO, 50.0, 16, 12, FORM_MATERIAL).unwrap();
    check("sphere", f, s, 4.0 / 3.0 * PI * 50f64.powi(3), 0.001);
}

/// Exact, and the case that proves the parameters beat the tessellation: its
/// caps are `Plane` faces whose u×v box is much larger than the disk they bound.
#[test]
fn a_cylinder_has_the_volume_its_radius_and_height_say() {
    let mut s = pathb();
    let f = s.mesh.create_cylinder(DVec3::ZERO, 50.0, 100.0, 24, FORM_MATERIAL).unwrap();
    check("cylinder", f, s, PI * 50f64.powi(2) * 100.0, 0.001);
}

/// No closed form here yet, so this one goes through the tessellated fall-back.
/// Pinned at the accuracy that path actually delivers, not at exactness — if a
/// closed form is added later this bound is what will say it improved.
#[test]
fn a_cone_falls_back_to_the_surface_and_lands_within_a_tenth_of_a_percent() {
    let mut s = pathb();
    let f = s.mesh.create_cone(DVec3::ZERO, 50.0, 100.0, 24, FORM_MATERIAL).unwrap();
    check("cone", f, s, PI * 50f64.powi(2) * 100.0 / 3.0, 0.1);
}

/// The control. A flat solid still goes through the boundary polygon, and must
/// be bit-for-bit what it always was — otherwise the change reached further
/// than it should have.
#[test]
fn a_box_is_unchanged() {
    let mut s = Scene::new();
    let f = s.mesh.create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, FORM_MATERIAL).unwrap();
    let v = axia_core::promote::face_set_volume(&s.mesh, &f);
    assert_eq!(v, 1_000_000.0, "a 100mm box is exactly 1e6 mm³");
    check("box", f, s, 1e6, 0.0001);
}

/// The axis definition of a sphere measures the same volume as the shipped one —
/// they are the same solid, however its boundary is written down.
#[test]
fn both_sphere_definitions_agree_on_the_volume() {
    let mut a = pathb();
    let fa = a.mesh.create_sphere(DVec3::ZERO, 50.0, 16, 12, FORM_MATERIAL).unwrap();
    let va = axia_core::promote::face_set_volume(&a.mesh, &fa);

    let mut b = Scene::new();
    let fb = b.mesh.sim_create_sphere_axis_native(DVec3::ZERO, 50.0, DVec3::Z, FORM_MATERIAL).unwrap();
    let vb = axia_core::promote::face_set_volume(&b.mesh, &[fb]);

    assert!((va - vb).abs() < 1e-6, "equator def {va:.6} vs axis def {vb:.6}");
}

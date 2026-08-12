//! What a crossing bore takes out of the wall it crosses, and whether the two
//! walls can be sewn along it.

use axia_geo::operations::cross_bore::crossing_bore_trim;
use glam::DVec3;

const R: f64 = 40.0;

/// The frames the circular drill actually builds its tubes in: `bu` is X for a
/// Z-axis bore and Y for an X-axis bore (`carve.rs`, the `t.cross(n)` fallback).
fn drill_ref(axis: DVec3) -> DVec3 {
    let mut t = DVec3::X;
    if t.cross(axis).length_squared() < 1e-6 {
        t = DVec3::Y;
    }
    (t - axis * t.dot(axis)).normalize()
}

fn perpendicular_bores(segments: u32) -> Option<axia_geo::operations::cross_bore::CrossBore> {
    crossing_bore_trim(
        DVec3::ZERO,
        DVec3::Z,
        drill_ref(DVec3::Z),
        DVec3::X,
        drill_ref(DVec3::X),
        R,
        R,
        segments,
    )
}

#[test]
fn the_missing_band_is_the_other_bore_seen_edge_on() {
    let x = perpendicular_bores(32).expect("two equal bores meeting at right angles");
    for trim in [&x.a, &x.b] {
        for st in &trim.stations {
            // |s| < r|cos θ|, θ from the other axis — widest facing it, pinched
            // to nothing at right angles to it.
            assert!(
                (st.band - R * st.theta.cos().abs()).abs() < 1e-9,
                "band {} at θ={}",
                st.band,
                st.theta
            );
            // and both edges of the band really are on both cylinders
            for p in [st.upper, st.lower] {
                let off = |axis: DVec3| {
                    let d = p - trim.meet;
                    ((d - axis * d.dot(axis)).length() - R).abs()
                };
                assert!(off(DVec3::Z) < 1e-9, "off cyl Z: {p:?}");
                assert!(off(DVec3::X) < 1e-9, "off cyl X: {p:?}");
            }
        }
        // The band is widest where it faces the other bore, and that width is r.
        assert!((trim.max_band() - R).abs() < 1e-9, "max band {}", trim.max_band());
    }
}

#[test]
fn the_two_walls_are_cut_at_the_same_points_or_not_at_all() {
    // The sewing test. A's station θ meets B's at π/2 − θ, so with each tube
    // segmented from its own drill reference the stations coincide only when the
    // count is a multiple of four. Measured at r=40: every station finds an exact
    // partner at 8/12/16/32/64 and NOT ONE does at 6/9/10/14/30 — so this is a
    // sharp condition, not a tolerance.
    for n in [8u32, 12, 16, 32, 64] {
        let x = perpendicular_bores(n).unwrap_or_else(|| panic!("n={n} should sew"));
        assert_eq!(x.a.stations.len(), n as usize);
        for st in &x.a.stations {
            let paired = x.b.stations.iter().any(|o| {
                (o.upper - st.upper).length() < 1e-9 || (o.lower - st.upper).length() < 1e-9
            });
            assert!(paired, "n={n}: a cut with no partner at θ={}", st.theta);
        }
    }
    for n in [6u32, 9, 10, 14, 30] {
        assert!(
            perpendicular_bores(n).is_none(),
            "n={n}: the stations do not meet on the curve, so this must refuse \
             rather than hand back a seam that cannot be closed"
        );
    }
}

#[test]
fn it_refuses_everything_it_is_not() {
    let z = drill_ref(DVec3::Z);
    let x = drill_ref(DVec3::X);

    // Unequal radii — a genuine quartic, no band in closed form.
    assert!(crossing_bore_trim(DVec3::ZERO, DVec3::Z, z, DVec3::X, x, 40.0, 25.0, 32).is_none());

    // Axes that are not perpendicular.
    let oblique = DVec3::new(1.0, 0.0, 1.0).normalize();
    assert!(
        crossing_bore_trim(DVec3::ZERO, DVec3::Z, z, oblique, drill_ref(oblique), R, R, 32)
            .is_none()
    );

    // Degenerate inputs.
    assert!(crossing_bore_trim(DVec3::ZERO, DVec3::Z, z, DVec3::X, x, 0.0, 0.0, 32).is_none());
    assert!(crossing_bore_trim(DVec3::ZERO, DVec3::ZERO, z, DVec3::X, x, R, R, 32).is_none());
    assert!(crossing_bore_trim(DVec3::ZERO, DVec3::Z, z, DVec3::X, x, R, R, 2).is_none());
}

#[test]
fn the_meeting_point_is_where_it_is_told() {
    // Nothing here assumes the origin.
    let meet = DVec3::new(-17.0, 4.0, 33.0);
    let x = crossing_bore_trim(
        meet,
        DVec3::Z,
        drill_ref(DVec3::Z),
        DVec3::X,
        drill_ref(DVec3::X),
        R,
        R,
        32,
    )
    .expect("offset crossing");
    for st in &x.a.stations {
        for p in [st.upper, st.lower] {
            let off = |axis: DVec3| {
                let d = p - meet;
                ((d - axis * d.dot(axis)).length() - R).abs()
            };
            assert!(off(DVec3::Z) < 1e-9 && off(DVec3::X) < 1e-9, "{p:?}");
        }
    }
}

#[test]
fn the_band_agrees_with_the_closed_form_curve() {
    // Independent of this module: the ellipses from `cylinder_cylinder_branches`
    // (#103) must be the same two curves the band's edges trace.
    use axia_geo::surfaces::ssi::analytic::cylinder_cylinder_branches;
    let branches = cylinder_cylinder_branches(
        DVec3::ZERO, DVec3::Z, R,
        DVec3::ZERO, DVec3::X, R,
        128, 10.0,
    );
    assert_eq!(branches.len(), 2);
    let x = perpendicular_bores(32).expect("crossing");
    for st in &x.a.stations {
        for p in [st.upper, st.lower] {
            let near = branches
                .iter()
                .flat_map(|b| b.points.iter())
                .map(|q| (*q - p).length())
                .fold(f64::MAX, f64::min);
            assert!(near < 0.6, "a band edge {p:?} is {near:.4} from the SSI curve");
        }
    }
}

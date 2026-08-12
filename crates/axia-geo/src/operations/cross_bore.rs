//! Where one bore crosses another, and whether the two walls can be sewn along it.
//!
//! Two through-bores of the SAME radius whose axes MEET at right angles cross in
//! two ellipses (the closed form is `ssi::analytic::cylinder_cylinder_branches`).
//! What the surgery needs is not the curve in space but the same fact written in
//! each tube's own parameters: **how much of this wall is standing inside the
//! other bore**, station by station, so the wall can be cut there and the two
//! cuts welded to each other.
//!
//! Put the meeting point at the origin with axes `a` and `b`, and parameterise
//! cylinder A by `(θ, s)` with `θ` measured from **b**:
//!
//! ```text
//!   p = r(cos θ · b + sin θ · (a × b)) + s · a
//!   |p|² = r² + s²          p·b = r cos θ
//!   inside B  ⇔  |p|² − (p·b)² < r²  ⇔  s² < r² cos²θ
//! ```
//!
//! So the wall is gone for `|s| < r|cos θ|`, and the two ellipses are exactly the
//! band's two edges, `s = ±r cos θ`. Nothing here is approximate.
//!
//! ⚠ The two tubes only sew if their stations land on the SAME points of that
//! curve, and that is a property of how each tube happened to be segmented — not
//! something to assume. A's station `θ` meets B's at `θ' = π/2 − θ`, so with both
//! tubes segmented from their drill's own reference direction the stations
//! coincide only when the count is a multiple of four. Measured at r=40: every
//! station finds an exact partner at n = 8, 12, 16, 32, 64, and **not one** does
//! at n = 6, 9, 10, 14, 30. [`crossing_bore_trim`] checks it rather than
//! believing it, and returns `None` when it does not hold — which is what keeps
//! this from producing a seam that cannot be closed.

use glam::DVec3;

use crate::plane::{EPS_PLANE_NORMAL, EPS_PLANE_OFFSET};

/// One angular station of a tube, and what the crossing bore takes out there.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossBoreStation {
    /// Angle around this tube's axis, measured from the OTHER tube's axis.
    pub theta: f64,
    /// Half-height of the missing band, along this tube's axis from the meeting
    /// point: the wall is gone for `|s| < band`.
    pub band: f64,
    /// The point on the `s = +r cos θ` ellipse.
    pub upper: DVec3,
    /// The point on the `s = −r cos θ` ellipse.
    pub lower: DVec3,
}

/// The crossing, written in one tube's parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossBoreTrim {
    /// The meeting point of the two axes.
    pub meet: DVec3,
    /// The axis of the tube being described.
    pub axis: DVec3,
    pub radius: f64,
    /// One entry per station, in increasing `theta`.
    pub stations: Vec<CrossBoreStation>,
}

impl CrossBoreTrim {
    /// The band is widest facing the other bore and pinches to nothing at right
    /// angles to it — those two pinch points are where the ellipses meet.
    pub fn max_band(&self) -> f64 {
        self.stations.iter().fold(0.0_f64, |m, st| m.max(st.band))
    }
}

/// How the two tubes must be described for the crossing to be sewable, and what
/// each of them loses.
#[derive(Debug, Clone, PartialEq)]
pub struct CrossBore {
    pub a: CrossBoreTrim,
    pub b: CrossBoreTrim,
}

/// The crossing of two equal-radius bores whose axes meet at right angles, or
/// `None` when this is not that case — or when it is, but the two tubes were cut
/// at stations that do not meet on the curve.
///
/// `ref_a` / `ref_b` are the directions each tube's own station 0 sits at (the
/// drill's `bu`); they decide whether the stations line up, which is why they are
/// asked for rather than invented here.
#[allow(clippy::too_many_arguments)]
pub fn crossing_bore_trim(
    meet: DVec3,
    axis_a: DVec3,
    ref_a: DVec3,
    axis_b: DVec3,
    ref_b: DVec3,
    radius_a: f64,
    radius_b: f64,
    segments: u32,
) -> Option<CrossBore> {
    if !(radius_a > 0.0) || (radius_a - radius_b).abs() > EPS_PLANE_OFFSET || segments < 3 {
        return None;
    }
    let a = axis_a.normalize_or_zero();
    let b = axis_b.normalize_or_zero();
    if a.length_squared() < 0.5 || b.length_squared() < 0.5 {
        return None;
    }
    // Right angles only: at any other angle the band is still a band but the two
    // tubes' stations no longer pair up one-for-one, and the sewing this exists
    // to serve is a different problem.
    if a.dot(b).abs() > EPS_PLANE_NORMAL {
        return None;
    }
    let r = radius_a;

    let trim = |axis: DVec3, other: DVec3, station_ref: DVec3| -> Option<CrossBoreTrim> {
        // The station reference must lie in the tube's cross-section, and the
        // angle we parameterise by is measured from the OTHER axis.
        let e1 = other; // ⟂ axis, because the axes are perpendicular
        let e2 = axis.cross(e1);
        if e2.length_squared() < 0.5 {
            return None;
        }
        let sr = (station_ref - axis * station_ref.dot(axis)).normalize_or_zero();
        if sr.length_squared() < 0.5 {
            return None;
        }
        // Where station 0 sits, in the (e1, e2) frame.
        let offset = sr.dot(e2).atan2(sr.dot(e1));
        let stations = (0..segments)
            .map(|k| {
                let theta = offset + std::f64::consts::TAU * (k as f64) / (segments as f64);
                let (sin, cos) = theta.sin_cos();
                let ring = e1 * (r * cos) + e2 * (r * sin);
                CrossBoreStation {
                    theta,
                    band: (r * cos).abs(),
                    upper: meet + ring + axis * (r * cos),
                    lower: meet + ring - axis * (r * cos),
                }
            })
            .collect::<Vec<_>>();
        Some(CrossBoreTrim { meet, axis, radius: r, stations })
    };

    let ta = trim(a, b, ref_a)?;
    let tb = trim(b, a, ref_b)?;

    // ⚠ The sewing test, done rather than assumed: every cut point one tube
    // makes must be a cut point the other makes too, or the two walls meet along
    // a seam with no shared vertices and nothing can close it.
    let partners = |from: &CrossBoreTrim, to: &CrossBoreTrim| -> bool {
        from.stations.iter().all(|st| {
            [st.upper, st.lower].iter().all(|p| {
                to.stations
                    .iter()
                    .any(|o| (o.upper - *p).length() < EPS_PLANE_OFFSET
                        || (o.lower - *p).length() < EPS_PLANE_OFFSET)
            })
        })
    };
    if !partners(&ta, &tb) || !partners(&tb, &ta) {
        return None;
    }

    Some(CrossBore { a: ta, b: tb })
}

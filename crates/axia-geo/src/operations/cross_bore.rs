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

use crate::{FaceId, VertId};

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

/// How much of a bore's wall stands inside a crossing bore, at the point whose
/// radial direction is `radial` (unit, ⟂ the wall's own axis).
///
/// This is `r|cos θ|` with θ measured from the other axis — the same band the
/// module header derives, asked one point at a time.
pub fn band_at(radial: DVec3, other_axis: DVec3, radius: f64) -> f64 {
    radius * radial.dot(other_axis).abs()
}

impl crate::Mesh {
    /// Cut a bore's wall where a crossing bore passes through it, and take the
    /// piece that was standing inside the other bore away.
    ///
    /// Each wall quad spans the whole bore, so each becomes two: one above the
    /// band and one below it, with the band itself — the part inside the other
    /// bore — gone. The new corners land ON the crossing curve, which is what
    /// lets the other bore's wall weld to them later: `add_vertex` dedups by
    /// position, so the two walls share these vertices without being told to.
    ///
    /// The quads keep their winding (a bore wall faces the void) and their
    /// `Cylinder`, narrowed to the piece that survives.
    ///
    /// Returns the surviving faces. The solid is OPEN afterwards along the two
    /// crossing curves — closing it is the other wall's job.
    pub fn trim_bore_wall_for_crossing(
        &mut self,
        tube_faces: &[FaceId],
        meet: DVec3,
        axis: DVec3,
        other_axis: DVec3,
        radius: f64,
    ) -> anyhow::Result<Vec<FaceId>> {
        use anyhow::bail;
        let a = axis.normalize_or_zero();
        let b = other_axis.normalize_or_zero();
        if a.length_squared() < 0.5 || b.length_squared() < 0.5 || !(radius > 0.0) {
            bail!("trim bore wall: degenerate axis or radius");
        }

        let mut survivors = Vec::with_capacity(tube_faces.len() * 2);
        for &fid in tube_faces {
            let Some(face) = self.faces.get(fid).filter(|f| f.is_active()) else {
                continue;
            };
            let surface = face.surface().cloned();
            let material = face.material();
            let verts = self.collect_loop_verts(face.outer().start)?;
            if verts.len() != 4 {
                bail!("trim bore wall: expected a quad, got {} corners", verts.len());
            }
            let pts: Vec<DVec3> = verts
                .iter()
                .map(|&v| self.vertex_pos(v))
                .collect::<anyhow::Result<_>>()?;

            // Split the corners into the two axial ends. A tube quad runs the
            // whole bore, so two corners sit at each end.
            let s: Vec<f64> = pts.iter().map(|p| (*p - meet).dot(a)).collect();
            let mid = (s.iter().sum::<f64>()) / 4.0;
            let high: Vec<usize> = (0..4).filter(|&i| s[i] > mid).collect();
            let low: Vec<usize> = (0..4).filter(|&i| s[i] <= mid).collect();
            if high.len() != 2 || low.len() != 2 {
                bail!("trim bore wall: a quad that does not span the bore");
            }

            // `meet` has to be ON this wall's axis, or every radial direction
            // below is measured from the wrong place and the band comes out
            // plausible but wrong. The wall itself says whether it is: every
            // corner of a bore wall stands at exactly `radius` from its axis.
            for (i, p) in pts.iter().enumerate() {
                let d = *p - meet;
                let radial = (d - a * d.dot(a)).length();
                if (radial - radius).abs() > crate::plane::EPS_PLANE_OFFSET {
                    bail!(
                        "trim bore wall: corner {i} stands {radial} from the axis                          through the meeting point, not {radius} — the meeting                          point is not on this wall's axis"
                    );
                }
            }

            // The band, per corner, from that corner's own radial direction.
            let band = |i: usize| -> f64 {
                let d = pts[i] - meet;
                let radial = (d - a * d.dot(a)).normalize_or_zero();
                band_at(radial, b, radius)
            };
            // A quad clear of the band keeps its shape. The quad SPANS the
            // bore, so the question is whether its axial range overlaps the
            // band — not whether a corner sits in it, which is never true for a
            // wall that runs the whole depth.
            let widest = (0..4).map(band).fold(0.0_f64, f64::max);
            let s_min = s.iter().cloned().fold(f64::MAX, f64::min);
            let s_max = s.iter().cloned().fold(f64::MIN, f64::max);
            let overlaps = s_min < widest && s_max > -widest;
            if !overlaps {
                survivors.push(fid);
                continue;
            }

            // Walk the loop and rebuild it twice, swapping the far end's corners
            // for points on the crossing curve. Same order, so same winding.
            let cut = |i: usize, up: bool| -> DVec3 {
                let d = pts[i] - meet;
                let ring = d - a * d.dot(a);
                meet + ring + a * (if up { band(i) } else { -band(i) })
            };
            let upper: Vec<DVec3> = (0..4)
                .map(|i| if high.contains(&i) { pts[i] } else { cut(i, true) })
                .collect();
            let lower: Vec<DVec3> = (0..4)
                .map(|i| if low.contains(&i) { pts[i] } else { cut(i, false) })
                .collect();

            self.remove_face(fid)?;
            for piece in [upper, lower] {
                // A piece with no height left is the pinch, where the two
                // crossing curves touch — nothing to add there.
                let ids: Vec<VertId> = piece.iter().map(|&p| self.add_vertex(p)).collect();
                let mut dedup = ids.clone();
                dedup.sort_by_key(|v| v.raw());
                dedup.dedup();
                if dedup.len() < 3 {
                    continue;
                }
                let new_fid = self.add_face(&ids, material)?;
                if let Some(surf) = surface.clone() {
                    self.set_face_surface(new_fid, Some(surf));
                }
                survivors.push(new_fid);
            }
        }
        Ok(survivors)
    }
}

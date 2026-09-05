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

/// What a crossing bore would cut, and against what — the answer
/// [`crate::Mesh::crossing_bore_plan`] gives before anything is touched.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CrossBorePlan {
    /// Where the two axes meet.
    pub meet: DVec3,
    /// The new bore's axis.
    pub axis: DVec3,
    /// The axis of the bore it crosses.
    pub other_axis: DVec3,
    /// That bore's radius — equal to the new one's, or the plan is refused.
    pub other_radius: f64,
}

/// How much of a bore's wall stands inside a crossing bore, at the point whose
/// radial direction is `radial` (unit, ⟂ the wall's own axis).
///
/// With `θ` measured from the other axis, a point of this wall sits at distance
/// `√(r²sin²θ + s²)` from the other axis, so it is inside the other bore while
///
/// ```text
///   s² < r_other² − r_self² sin²θ
/// ```
///
/// ⚠ Equal radii collapse that to `r|cos θ|`, which is what this used to be and
/// what the module header derives. The general form is no less exact — measured
/// 2026-09-05 at r 40×25, 40×8 and 25×40, every station's point sits on BOTH
/// cylinders to 7.1e-15. Unequal radii were never a geometry problem.
///
/// Returns 0 where the wall is clear of the other bore, which for `r_self >
/// r_other` is most of the turn: the band exists only for `|sin θ| ≤
/// r_other / r_self`.
pub fn band_at(radial: DVec3, other_axis: DVec3, radius: f64, other_radius: f64) -> f64 {
    let cos = radial.dot(other_axis);
    let inside = other_radius * other_radius - radius * radius * (1.0 - cos * cos);
    if inside <= 0.0 {
        0.0
    } else {
        inside.sqrt()
    }
}

/// One point where BOTH walls have to be cut, carrying each wall's own angle
/// for it.
///
/// ⚠ Why this type exists: two tubes of DIFFERENT radii cannot be cut at their
/// own stations and expect to weld. Measured 2026-09-05 at r 40 × 25, only the
/// **four pinch points** ever coincide, at every segment count tried (8, 12, 16,
/// 32, 64, 128 — 4 matches out of 12, 52, 108 …). Equal radii match everywhere
/// because A's station θ lands on B's π/2 − θ; unequal radii map through
/// `sin φ = −(r_a / r_b) sin θ`, which no pair of uniform segmentations respects.
///
/// So neither wall may cut at its own stations alone. Both cut at the UNION, and
/// then the chords between consecutive points are the same on both sides, which
/// is what lets `add_vertex`'s position dedup weld them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SharedCut {
    pub pos: DVec3,
    /// Angle about tube A's axis, measured from B's axis, in `[0, τ)`.
    pub theta_a: f64,
    /// Angle about tube B's axis, measured from A's axis, in `[0, τ)`.
    pub theta_b: f64,
    /// Signed distance along A's axis from the meeting point — which branch of
    /// the curve this is, seen from A.
    pub side_a: f64,
    /// The same, along B's axis.
    pub side_b: f64,
}

/// Where a tube's station sits on its own ring, and the frame it is measured in.
fn ring_frame(axis: DVec3, other: DVec3) -> Option<(DVec3, DVec3)> {
    let e1 = (other - axis * other.dot(axis)).normalize_or_zero();
    if e1.length_squared() < 0.5 {
        return None;
    }
    let e2 = axis.cross(e1);
    if e2.length_squared() < 0.5 {
        return None;
    }
    Some((e1, e2))
}

/// The angle of `station_ref` in the `(e1, e2)` frame — where station 0 sits.
fn station_offset(axis: DVec3, station_ref: DVec3, e1: DVec3, e2: DVec3) -> Option<f64> {
    let sr = (station_ref - axis * station_ref.dot(axis)).normalize_or_zero();
    if sr.length_squared() < 0.5 {
        return None;
    }
    Some(sr.dot(e2).atan2(sr.dot(e1)))
}

/// The signed angle from `from` to `to`, taking the short way round: in
/// `(-π, π]`.
fn angle_gap(to: f64, from: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut d = (to - from) % tau;
    if d > std::f64::consts::PI {
        d -= tau;
    } else if d <= -std::f64::consts::PI {
        d += tau;
    }
    d
}

fn wrap_tau(t: f64) -> f64 {
    let tau = std::f64::consts::TAU;
    let r = t % tau;
    if r < 0.0 { r + tau } else { r }
}

/// Every point at which both walls must be cut for the two bores to open into
/// each other and still close.
///
/// The curve itself is closed form for any two radii — a wall point of A at
/// angle θ and axial offset `s` is inside B exactly while
/// `s² < r_b² − r_a² sin²θ` — so this generates each tube's own cut points
/// from that and merges the two sets, deduplicating by position at the same
/// 1.5 μm the sewing check uses.
///
/// Returns `None` for degenerate input. Right angles only: at other angles the
/// band is still a band, but `p · b` picks up an `s` term and the two branches
/// stop being symmetric about the meeting plane, which the trim assumes.
#[allow(clippy::too_many_arguments)]
pub fn shared_crossing_cuts(
    meet: DVec3,
    axis_a: DVec3,
    ref_a: DVec3,
    radius_a: f64,
    axis_b: DVec3,
    ref_b: DVec3,
    radius_b: f64,
    segments_a: u32,
    segments_b: u32,
) -> Option<Vec<SharedCut>> {
    let a = axis_a.normalize_or_zero();
    let b = axis_b.normalize_or_zero();
    let (e1a, e2a) = ring_frame(a, b)?;
    let (e1b, e2b) = ring_frame(b, a)?;
    let off_a = station_offset(a, ref_a, e1a, e2a)?;
    let off_b = station_offset(b, ref_b, e1b, e2b)?;
    let ring = |off: f64, n: u32| -> Vec<f64> {
        (0..n).map(|k| off + std::f64::consts::TAU * (k as f64) / (n as f64)).collect()
    };
    shared_crossing_cuts_from_angles(
        meet,
        axis_a, radius_a, &ring(off_a, segments_a),
        axis_b, radius_b, &ring(off_b, segments_b),
    )
}

/// The same, given each tube's station angles rather than a segment count.
///
/// The existing bore's wall is already in the mesh and nobody recorded which
/// reference direction it was drilled from, so reading its angles off its
/// corners is both easier and truer than guessing them back.
#[allow(clippy::too_many_arguments)]
pub fn shared_crossing_cuts_from_angles(
    meet: DVec3,
    axis_a: DVec3,
    radius_a: f64,
    angles_a: &[f64],
    axis_b: DVec3,
    radius_b: f64,
    angles_b: &[f64],
) -> Option<Vec<SharedCut>> {
    let a = axis_a.normalize_or_zero();
    let b = axis_b.normalize_or_zero();
    if a.length_squared() < 0.5
        || b.length_squared() < 0.5
        || !(radius_a > 0.0)
        || !(radius_b > 0.0)
        || angles_a.len() < 3
        || angles_b.len() < 3
        || a.dot(b).abs() > EPS_PLANE_NORMAL
    {
        return None;
    }

    let (e1a, e2a) = ring_frame(a, b)?;
    let (e1b, e2b) = ring_frame(b, a)?;

    // Each tube's own cut points, from the closed-form band.
    let mut pts: Vec<DVec3> = Vec::new();
    let mut emit = |axis: DVec3, e1: DVec3, e2: DVec3, r_self: f64, r_other: f64,
                    stations: &[f64], out: &mut Vec<DVec3>| {
        for &t in stations {
            let (sin, cos) = t.sin_cos();
            let inside = r_other * r_other - r_self * r_self * sin * sin;
            if inside <= 0.0 {
                continue;
            }
            let band = inside.sqrt();
            let ring = e1 * (r_self * cos) + e2 * (r_self * sin);
            out.push(meet + ring + axis * band);
            out.push(meet + ring - axis * band);
        }
    };
    emit(a, e1a, e2a, radius_a, radius_b, angles_a, &mut pts);
    emit(b, e1b, e2b, radius_b, radius_a, angles_b, &mut pts);

    // ⚠ And the four points where a wall's band VANISHES, which are stations of
    // nothing. `sin θ = ±r_other / r_self` has a solution only for the wider tube,
    // and there the two branches of the curve meet: the arc of wall it loses ends
    // exactly there. Equal radii put them at ±π/2 — which is why the old code
    // needed `segments % 4 == 0`, though it never said so in those terms. Leave
    // them out and the quad straddling the arc's end has nothing to be cut at.
    let mut pinch = |axis: DVec3, e1: DVec3, e2: DVec3, r_self: f64, r_other: f64,
                     out: &mut Vec<DVec3>| {
        if r_other > r_self {
            return; // this wall is cut all the way round; its band never vanishes
        }
        let sin = r_other / r_self;
        let base = sin.asin();
        for t in [base, std::f64::consts::PI - base, -base, std::f64::consts::PI + base] {
            let (st, ct) = t.sin_cos();
            let _ = axis;
            out.push(meet + e1 * (r_self * ct) + e2 * (r_self * st));
        }
    };
    pinch(a, e1a, e2a, radius_a, radius_b, &mut pts);
    pinch(b, e1b, e2b, radius_b, radius_a, &mut pts);

    // Merge. Equal radii make the two sets coincide almost everywhere, so this
    // is not an optimisation — without it the walls would carry each shared
    // point twice and the second `add_vertex` would find the first anyway, but
    // the boundary would name it twice and the face would be degenerate.
    let mut merged: Vec<SharedCut> = Vec::with_capacity(pts.len());
    for pos in pts {
        if merged.iter().any(|c| (c.pos - pos).length() < EPS_PLANE_OFFSET) {
            continue;
        }
        let d = pos - meet;
        let ra = d - a * d.dot(a);
        let rb = d - b * d.dot(b);
        merged.push(SharedCut {
            pos,
            theta_a: wrap_tau(ra.dot(e2a).atan2(ra.dot(e1a))),
            theta_b: wrap_tau(rb.dot(e2b).atan2(rb.dot(e1b))),
            side_a: d.dot(a),
            side_b: d.dot(b),
        });
    }
    Some(merged)
}

impl crate::Mesh {
    /// The angles at which a bore's wall already has corners, measured about its
    /// own axis from `other_axis`.
    ///
    /// Read from the mesh rather than reconstructed from a segment count: the
    /// reference direction a bore was drilled from is not recorded anywhere, and
    /// a shared cut set built on the wrong one matches nothing.
    pub fn bore_wall_station_angles(
        &self,
        tube_faces: &[FaceId],
        meet: DVec3,
        axis: DVec3,
        other_axis: DVec3,
    ) -> anyhow::Result<Vec<f64>> {
        use anyhow::bail;
        let a = axis.normalize_or_zero();
        let Some((e1, e2)) = ring_frame(a, other_axis) else {
            bail!("bore wall angles: the two axes do not span a ring frame");
        };
        let mut out: Vec<f64> = Vec::new();
        for &fid in tube_faces {
            let Some(face) = self.faces.get(fid).filter(|f| f.is_active()) else {
                continue;
            };
            for v in self.collect_loop_verts(face.outer().start)? {
                let d = self.vertex_pos(v)? - meet;
                let r = d - a * d.dot(a);
                if r.length_squared() < 1e-18 {
                    continue;
                }
                let t = wrap_tau(r.dot(e2).atan2(r.dot(e1)));
                if !out.iter().any(|o| angle_gap(t, *o).abs() < 1e-9) {
                    out.push(t);
                }
            }
        }
        out.sort_by(|x, y| x.partial_cmp(y).unwrap());
        Ok(out)
    }

    /// Give a bore's wall a station at every angle in `angles`, so that a later
    /// trim cuts it exactly where the other wall will be cut.
    ///
    /// ⚠ Why this is needed at all: two tubes of different radii cut each other
    /// at points that are stations of NEITHER. Measured at r 40 × 25, only the
    /// four pinch points coincide, at every segment count tried (8, 12, 16, 32,
    /// 64, 128). Split both walls at the union first and every cut edge
    /// afterwards runs between the same two shared points on both sides, which
    /// is what lets `add_vertex`'s position dedup weld them.
    ///
    /// ⚠ It splits EDGES, not faces. Rebuilding the quad from new corners was
    /// tried first and opened the solid: a tube quad's two angular edges are
    /// shared with the shell's hole loop, so a new corner that only the wall
    /// knows about leaves the shell still spanning the old chord — measured, 8
    /// tube faces became 12 with `valid = true` and `closed = FALSE`.
    /// `split_edge` updates both incident faces, so the rim follows.
    ///
    /// The new rim points sit on the true circle rather than on the chord they
    /// replace, which makes the hole slightly rounder — the direction a finer
    /// tessellation would move it anyway.
    pub fn split_bore_wall_at_angles(
        &mut self,
        tube_faces: &[FaceId],
        meet: DVec3,
        axis: DVec3,
        other_axis: DVec3,
        radius: f64,
        angles: &[f64],
    ) -> anyhow::Result<Vec<FaceId>> {
        use anyhow::bail;
        let a = axis.normalize_or_zero();
        if a.length_squared() < 0.5 || !(radius > 0.0) {
            bail!("split bore wall: degenerate axis or radius");
        }
        let Some((e1, e2)) = ring_frame(a, other_axis) else {
            bail!("split bore wall: the two axes do not span a ring frame");
        };

        let mut work: Vec<FaceId> = tube_faces.to_vec();
        for &target in angles {
            let mut next = Vec::with_capacity(work.len() + 1);
            for fid in work {
                match self.split_quad_at_angle(fid, meet, a, e1, e2, radius, target)? {
                    Some((f1, f2)) => {
                        next.push(f1);
                        next.push(f2);
                    }
                    None => next.push(fid),
                }
            }
            work = next;
        }
        Ok(work)
    }

    /// One quad, one angle. `Ok(None)` means the angle is not strictly inside
    /// this quad's span — someone else's business.
    #[allow(clippy::too_many_arguments)]
    fn split_quad_at_angle(
        &mut self,
        fid: FaceId,
        meet: DVec3,
        a: DVec3,
        e1: DVec3,
        e2: DVec3,
        radius: f64,
        target: f64,
    ) -> anyhow::Result<Option<(FaceId, FaceId)>> {
        use anyhow::bail;
        const ANG_EPS: f64 = 1e-9;

        let Some(face) = self.faces.get(fid).filter(|f| f.is_active()) else {
            return Ok(None);
        };
        let verts = self.collect_loop_verts(face.outer().start)?;
        if verts.len() != 4 {
            return Ok(None); // already split, or not a plain tube quad
        }
        let pts: Vec<DVec3> =
            verts.iter().map(|&v| self.vertex_pos(v)).collect::<anyhow::Result<_>>()?;
        let ang: Vec<f64> = pts
            .iter()
            .map(|p| {
                let d = *p - meet;
                let r = d - a * d.dot(a);
                wrap_tau(r.dot(e2).atan2(r.dot(e1)))
            })
            .collect();
        let s: Vec<f64> = pts.iter().map(|p| (*p - meet).dot(a)).collect();

        let col0 = ang[0];
        let same0: Vec<usize> =
            (0..4).filter(|&i| angle_gap(ang[i], col0).abs() < ANG_EPS).collect();
        let other: Vec<usize> = (0..4).filter(|i| !same0.contains(i)).collect();
        if same0.len() != 2 || other.len() != 2 {
            return Ok(None);
        }
        let col1 = ang[other[0]];
        if angle_gap(ang[other[1]], col1).abs() >= ANG_EPS {
            return Ok(None);
        }

        let sweep = angle_gap(col1, col0);
        let gap = angle_gap(target, col0);
        let inside = gap.abs() > ANG_EPS
            && (sweep - gap).abs() > ANG_EPS
            && gap.signum() == sweep.signum()
            && gap.abs() < sweep.abs();
        if !inside {
            return Ok(None);
        }

        let mid = s.iter().sum::<f64>() / 4.0;
        let high: Vec<usize> = (0..4).filter(|&i| s[i] > mid).collect();
        let low: Vec<usize> = (0..4).filter(|&i| s[i] <= mid).collect();
        if high.len() != 2 || low.len() != 2 {
            return Ok(None);
        }

        // The two angular edges, one at each end of the bore. A quad rebuilt
        // from corners would leave these in place and open the solid.
        let (Some(hi_edge), Some(lo_edge)) = (
            self.find_edge(verts[high[0]], verts[high[1]]),
            self.find_edge(verts[low[0]], verts[low[1]]),
        ) else {
            bail!("split bore wall: a quad whose two ends are not edges");
        };

        let f = (gap / sweep).clamp(0.0, 1.0);
        let (sin, cos) = target.sin_cos();
        let ring = meet + e1 * (radius * cos) + e2 * (radius * sin);
        let at = |idx: &[usize]| -> DVec3 {
            let (p, q) = if same0.contains(&idx[0]) { (idx[0], idx[1]) } else { (idx[1], idx[0]) };
            ring + a * (s[p] + (s[q] - s[p]) * f)
        };

        // ⚠ Read the barrel BEFORE splitting. `split_face` hands each child the
        // parent's surface unchanged (ADR-089 A-χ), and a bore quad's `u_range`
        // is its OWN angular slice — so both halves would claim the whole slice
        // and every volume reader would count it twice. Measured before this
        // line existed: a r 40 × 25 crossing reported 891,210 removed by the
        // second bore when the whole of that bore is only 392,699. The same
        // shape of error #112 found, one level down.
        let barrel = self.face_surface(fid).and_then(|s| match s {
            crate::surfaces::AnalyticSurface::Cylinder {
                axis_origin, axis_dir, radius, ref_dir, ..
            } => Some((*axis_origin, *axis_dir, *radius, *ref_dir)),
            _ => None,
        });

        let (v_hi, _, _) = self.split_edge(hi_edge, at(&high))?;
        let (v_lo, _, _) = self.split_edge(lo_edge, at(&low))?;
        let (f1, f2) = self.split_face(fid, v_hi, v_lo)?;

        if let Some((origin, dir, r, e)) = barrel {
            self.attach_bore_cylinder(&[f1, f2], origin, dir, r, e);
        }
        Ok(Some((f1, f2)))
    }

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
        other_radius: f64,
    ) -> anyhow::Result<Vec<FaceId>> {
        use anyhow::bail;
        let a = axis.normalize_or_zero();
        let b = other_axis.normalize_or_zero();
        if a.length_squared() < 0.5 || b.length_squared() < 0.5 || !(radius > 0.0)
            || !(other_radius > 0.0)
        {
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
                band_at(radial, b, radius, other_radius)
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
                // ⚠ Narrow the surface to the piece. Cloning it unchanged
                // leaves BOTH halves claiming the whole barrel, and each is then
                // read as the whole barrel: measured, two crossing Ø80 bores in a
                // 200³ box took out 3,351,032 where the two cylinders' union is
                // 1,669,286 — the crossing region removed twice. The volume also
                // stopped depending on the segment count, which is what gave it
                // away, and I nearly read that as a good sign.
                if let Some(mut surf) = surface.clone() {
                    if let crate::surfaces::AnalyticSurface::Cylinder {
                        axis_origin, axis_dir, ref mut v_range, ..
                    } = surf
                    {
                        let axis_u = axis_dir.normalize_or_zero();
                        let lo = piece.iter().map(|p| (*p - axis_origin).dot(axis_u)).fold(f64::MAX, f64::min);
                        let hi = piece.iter().map(|p| (*p - axis_origin).dot(axis_u)).fold(f64::MIN, f64::max);
                        if hi > lo {
                            *v_range = (lo, hi);
                        }
                    }
                    self.set_face_surface(new_fid, Some(surf));
                }
                survivors.push(new_fid);
            }
        }
        Ok(survivors)
    }

    /// Drill a bore that CROSSES an existing one, so the two open into each
    /// other with nothing standing in between.
    ///
    /// The ordinary drill refuses this: its straight tube cannot bridge the void
    /// the first bore left, and it would leave two walls running through each
    /// other. This builds that straight tube anyway and then cuts BOTH walls
    /// back to the curve where they cross — after which neither is standing
    /// inside the other, and the two cuts weld because they were made at the
    /// same points (`add_vertex` dedups by position).
    ///
    /// Only the case [`crossing_bore_trim`] accepts: equal radii, axes meeting
    /// at right angles, and stations that land on the same points of the
    /// crossing curve. Anything else is refused with the mesh untouched — the
    /// caller should fall back to the ordinary refusal, or to Boolean.
    /// The bore already lying ACROSS `axis`, if there is exactly one.
    ///
    /// A wall says which cylinder it stands on (#105), so this is a read of what
    /// the mesh already knows rather than a search for geometry. `Ok(None)` means
    /// nothing crosses; `Err` means more than one does, which is a different
    /// problem than this op solves.
    pub fn bore_across_axis(
        &self,
        axis: DVec3,
    ) -> anyhow::Result<Option<(DVec3, DVec3, f64)>> {
        use anyhow::bail;
        let n = axis.normalize_or_zero();
        if n.length_squared() < 0.5 {
            return Ok(None);
        }
        let mut found: Option<(DVec3, DVec3, f64)> = None;
        for (fid, f) in self.faces.iter() {
            if !f.is_active() {
                continue;
            }
            let Some(crate::surfaces::AnalyticSurface::Cylinder {
                axis_origin, axis_dir, radius: r, ..
            }) = self.face_surface(fid)
            else {
                continue;
            };
            let a = axis_dir.normalize_or_zero();
            if a.dot(n).abs() > EPS_PLANE_NORMAL {
                continue; // parallel-ish bores do not cross
            }
            match found {
                None => found = Some((*axis_origin, a, *r)),
                Some((_, a0, r0)) => {
                    if (r - r0).abs() > EPS_PLANE_OFFSET
                        || a0.dot(a).abs() < 1.0 - EPS_PLANE_NORMAL
                    {
                        bail!("crossing bore: more than one bore lies across this one");
                    }
                }
            }
        }
        Ok(found)
    }

    /// What a crossing bore would need, worked out WITHOUT touching anything.
    ///
    /// The same checks [`Mesh::drill_crossing_bore`] makes before it cuts, asked
    /// on their own so a caller can find out whether the kernel can do this
    /// crossing before offering it to anyone. The `Err` is the reason, in the
    /// words the user will see.
    pub fn crossing_bore_plan(
        &self,
        center: DVec3,
        normal: DVec3,
        radius: f64,
        segments: u32,
    ) -> anyhow::Result<CrossBorePlan> {
        use anyhow::bail;
        let n = normal.normalize_or_zero();
        if n.length_squared() < 0.5 || !(radius > 0.0) || segments < 3 {
            bail!("crossing bore: degenerate normal, radius or segment count");
        }

        let n = normal.normalize_or_zero();
        if n.length_squared() < 0.5 || !(radius > 0.0) || segments < 3 {
            bail!("crossing bore: degenerate normal, radius or segment count");
        }

        let Some((other_origin, other_axis, other_radius)) = self.bore_across_axis(n)? else {
            bail!("crossing bore: nothing here to cross");
        };

        // Where the two axes meet, and whether the walls can be sewn there.
        let Some(meet) =
            crate::surfaces::ssi::analytic::axes_meeting_point(
                center, n, other_origin, other_axis, EPS_PLANE_OFFSET,
            )
        else {
            bail!("crossing bore: the axes are skew, so they do not cross at a point");
        };
        let drill_ref = |axis: DVec3| {
            let mut t = DVec3::X;
            if t.cross(axis).length_squared() < 1e-6 {
                t = DVec3::Y;
            }
            (t - axis * t.dot(axis)).normalize_or_zero()
        };
        // ⚠ This used to demand EQUAL radii and a segment count that lands on the
        // curve, because both walls were cut at their own stations and only then
        // do those stations coincide. Measured 2026-09-05 at r 40 × 25: exactly
        // four points ever matched, at every count tried. The geometry was never
        // the obstacle — the band is closed form for any two radii, to 7.1e-15 —
        // the SEGMENTATION was. Both walls are now split at the union first
        // (`split_bore_wall_at_angles`), so what is left to require is that the
        // shared set exists at all: axes meeting at right angles.
        if shared_crossing_cuts(
            meet,
            n,
            drill_ref(n),
            radius,
            other_axis,
            drill_ref(other_axis),
            other_radius,
            segments,
            segments,
        )
        .is_none()
        {
            bail!(
                "crossing bore: these two bores do not cut each other at shared \
                 points — the axes must meet at right angles"
            );
        }

        Ok(CrossBorePlan { meet, axis: n, other_axis, other_radius })
    }

    /// Drill a bore that CROSSES an existing one, so the two open into each
    /// other with nothing standing in between.
    ///
    /// The ordinary drill refuses this: its straight tube cannot bridge the void
    /// the first bore left, and it would leave two walls running through each
    /// other. This builds that straight tube anyway and then cuts BOTH walls
    /// back to the curve where they cross — after which neither is standing
    /// inside the other, and the two cuts weld because they were made at the
    /// same points (`add_vertex` dedups by position).
    ///
    /// Only the case [`Mesh::crossing_bore_plan`] accepts. Anything else is
    /// refused with the mesh untouched — the caller should fall back to the
    /// ordinary refusal, or to Boolean.
    pub fn drill_crossing_bore(
        &mut self,
        center: DVec3,
        normal: DVec3,
        radius: f64,
        segments: u32,
    ) -> anyhow::Result<Vec<FaceId>> {
        let CrossBorePlan { meet, axis: n, other_axis, other_radius } =
            self.crossing_bore_plan(center, normal, radius, segments)?;

        // ⚠ The plan cannot promise the seam will close. It knows the two axes
        // meet at right angles and that the shared cut set exists, but whether
        // both walls end up carrying every point of it depends on where the new
        // tube's stations actually land, which is the drill's business and not
        // always what predicting them says — measured 2026-09-05, odd segment
        // counts (9, 11, 13, 15) come out open while 8, 10, 14, 16, 30, 32, 64
        // close. So the answer is checked rather than predicted, on a copy, and
        // the contract that a refusal leaves the mesh untouched is kept by
        // restoring it.
        let backup = self.clone();
        match self.drill_crossing_bore_inner(center, meet, n, other_axis, radius, other_radius, segments) {
            Ok(kept) if self.verify_outward_normals().is_closed_solid => Ok(kept),
            Ok(_) => {
                *self = backup;
                anyhow::bail!(
                    "crossing bore: the two walls did not close along the seam at                      {segments} segments — try an even count, or use Boolean"
                )
            }
            Err(e) => {
                *self = backup;
                Err(e)
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn drill_crossing_bore_inner(
        &mut self,
        center: DVec3,
        meet: DVec3,
        n: DVec3,
        other_axis: DVec3,
        radius: f64,
        other_radius: f64,
        segments: u32,
    ) -> anyhow::Result<Vec<FaceId>> {

        // ⚠ Cut the EXISTING wall first, then drill.
        //
        // The other order fails, and not always: the drill measures its depth by
        // raycasting for the nearest opposite face, and with the crossed bore's
        // wall still whole that ray hits the WALL rather than the far shell. It
        // then tries to punch the exit inside the bore and finds no host. Whether
        // it does depends on where the wall's vertices happen to fall, which is
        // the same density-dependence the cross-drill guard was fixed for —
        // measured here at 8 segments failing while 16 and 32 went through.
        //
        // Cutting first opens the band the new bore passes through, so the ray
        // meets nothing until the shell and the depth is right at any density.
        let others: Vec<FaceId> = self
            .faces
            .iter()
            .filter(|(fid, f)| {
                f.is_active()
                    && matches!(
                        self.face_surface(*fid),
                        Some(crate::surfaces::AnalyticSurface::Cylinder { axis_dir, .. })
                            if axis_dir.normalize_or_zero().dot(other_axis).abs()
                                > 1.0 - EPS_PLANE_NORMAL
                    )
            })
            .map(|(fid, _)| fid)
            .collect();
        // The two walls must be cut at the SAME points or the seam has nothing to
        // weld along. With unequal radii neither tube's own stations are those
        // points, so both get split at the union first.
        //
        // ⚠ The existing wall's stations are read off its corners rather than
        // reconstructed: nobody recorded which reference direction it was drilled
        // from, and a guess that is 1° out yields a shared set that matches
        // nothing.
        let drill_ref = |axis: DVec3| {
            let mut t = DVec3::X;
            if t.cross(axis).length_squared() < 1e-6 {
                t = DVec3::Y;
            }
            (t - axis * t.dot(axis)).normalize_or_zero()
        };
        let existing_angles =
            self.bore_wall_station_angles(&others, meet, other_axis, n)?;
        let new_angles: Vec<f64> = {
            let (e1, e2) = ring_frame(n, other_axis)
                .ok_or_else(|| anyhow::anyhow!("crossing bore: no ring frame"))?;
            let off = station_offset(n, drill_ref(n), e1, e2)
                .ok_or_else(|| anyhow::anyhow!("crossing bore: no station offset"))?;
            (0..segments)
                .map(|k| off + std::f64::consts::TAU * (k as f64) / (segments as f64))
                .collect()
        };
        let cuts = shared_crossing_cuts_from_angles(
            meet,
            other_axis, other_radius, &existing_angles,
            n, radius, &new_angles,
        )
        .ok_or_else(|| anyhow::anyhow!("crossing bore: the shared cut set is empty"))?;
        let angles_existing: Vec<f64> = cuts.iter().map(|c| c.theta_a).collect();
        let angles_new: Vec<f64> = cuts.iter().map(|c| c.theta_b).collect();

        let others =
            self.split_bore_wall_at_angles(&others, meet, other_axis, n, other_radius, &angles_existing)?;
        let mut kept =
            self.trim_bore_wall_for_crossing(&others, meet, other_axis, n, other_radius, radius)?;

        let drilled = self.drill_circular_through_hole_inner(center, n, radius, segments, true)?;
        let new_wall = self.split_bore_wall_at_angles(
            &drilled.tube_faces, meet, n, other_axis, radius, &angles_new)?;
        kept.extend(self.trim_bore_wall_for_crossing(&new_wall, meet, n, other_axis, radius, other_radius)?);
        Ok(kept)
    }
}

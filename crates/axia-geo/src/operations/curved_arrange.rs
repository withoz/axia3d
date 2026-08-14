//! C-2 — two shapes that meet on a curved host, resolved in the host's own
//! parameter space.
//!
//! Measured 2026-08-14 (`two_circles_on_a_curved_host_share_ground.rs`): a
//! second circle drawn across a first does not notice it. Cap A's boundary is
//! 31 vertices before and 31 after, invariants valid, self-intersections 0 —
//! and the ground under the lens belongs to two faces at once. Nothing catches
//! it, because the pair share no edge and pierce nothing.
//!
//! On a plane the same pair becomes three regions, and the machinery that does
//! it — `PlanarGraph` → `bentley_ottmann_resolve` → `extract_regions` — is not
//! plane-specific. What is plane-specific is the CHART: a planar face projects
//! onto its own plane, exactly, by an affine map.
//!
//! The cylinder has one too. It is DEVELOPABLE, so unrolling it,
//!
//! ```text
//!     (u, v)  ->  (u·R, v)
//! ```
//!
//! is a local isometry — lengths, angles and areas are all preserved. A
//! geodesic circle on the wall is therefore a Euclidean circle on the chart,
//! and the arrangement computed there is the arrangement on the wall, not an
//! approximation of it. The cone is developable too and gets the same
//! treatment; the sphere and torus are not, and are left for later rather than
//! approximated quietly.
//!
//! This module is the GEOMETRY half: it says what the regions are. Putting
//! them back into the mesh is the other half.

use crate::boundary_kernel::bentley_ottmann::bentley_ottmann_resolve;
use crate::boundary_kernel::geom2::Vec2;
use crate::boundary_kernel::planar::{Lineage, PlanarGraph};
use crate::boundary_kernel::region::extract_regions;
use crate::surfaces::{cylinder, AnalyticSurface};
use glam::DVec3;

/// Where a region sits relative to the two loops it came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Share {
    /// Inside the first loop only.
    FirstOnly,
    /// Inside both — the lens.
    Both,
    /// Inside the second loop only.
    SecondOnly,
}

/// One region of the arrangement, back in world space.
#[derive(Clone, Debug)]
pub struct CurvedRegion {
    /// Boundary points on the host surface, in order.
    pub boundary: Vec<DVec3>,
    /// Which of the two loops this region belongs to.
    pub share: Share,
    /// Area in the unrolled chart — exact for a developable host.
    pub area: f64,
}

/// The unroll chart for a cylinder: world → (arc length, axial).
///
/// `u_seam` is where the chart is cut. It has to miss both loops, or a loop
/// that straddles it comes back as two pieces with a jump between them — the
/// same trap the render's band clipper has.
struct Unroll {
    axis_origin: DVec3,
    axis_dir: DVec3,
    radius: f64,
    ref_dir: DVec3,
    u_seam: f64,
}

impl Unroll {
    fn to_2d(&self, p: DVec3) -> Option<Vec2> {
        let (_sp, u, v) = cylinder::project_to_cylinder(
            self.axis_origin,
            self.axis_dir,
            self.radius,
            self.ref_dir,
            p,
        )?;
        let mut s = u - self.u_seam;
        while s < 0.0 {
            s += std::f64::consts::TAU;
        }
        while s >= std::f64::consts::TAU {
            s -= std::f64::consts::TAU;
        }
        Some(Vec2 { x: s * self.radius, y: v })
    }

    fn to_3d(&self, q: Vec2) -> DVec3 {
        cylinder::evaluate(
            self.axis_origin,
            self.axis_dir,
            self.radius,
            self.ref_dir,
            q.x / self.radius + self.u_seam,
            q.y,
        )
    }
}

/// The seam furthest from every point of both loops.
///
/// Same question the render's band asks, and answered the same obvious way:
/// try a few hundred angles and keep the one whose nearest loop point is
/// furthest. Sorting interval ends and pairing them by index looked like it
/// worked and did not — measured 2026-08-14, it put the seam inside a hole.
fn seam_missing_both(a: &[(f64, f64)], b: &[(f64, f64)]) -> f64 {
    use std::f64::consts::{PI, TAU};
    const CANDIDATES: usize = 360;
    let mut best = (0.0, -1.0_f64);
    for c in 0..CANDIDATES {
        let cand = TAU * (c as f64) / (CANDIDATES as f64);
        let mut nearest = f64::MAX;
        for &(u, _) in a.iter().chain(b.iter()) {
            let mut d = (u - cand).abs() % TAU;
            if d > PI {
                d = TAU - d;
            }
            nearest = nearest.min(d);
        }
        if nearest > best.1 {
            best = (cand, nearest);
        }
    }
    best.0
}

fn point_in_polygon(p: Vec2, poly: &[Vec2]) -> bool {
    let mut inside = false;
    let n = poly.len();
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a.y > p.y) != (b.y > p.y) {
            let t = (p.y - a.y) / (b.y - a.y);
            if p.x < a.x + t * (b.x - a.x) {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Resolve two closed loops that meet on a cylinder into the regions they
/// divide it into.
///
/// Both loops are given as world points ON the cylinder, in order, without a
/// repeated closing point. Returns `None` when the host is not a cylinder,
/// when a point is not on it, or when the loops do not in fact overlap — the
/// caller keeps what it had rather than being handed a rearrangement of
/// nothing.
pub fn arrange_two_loops_on_cylinder(
    surface: &AnalyticSurface,
    loop_a: &[DVec3],
    loop_b: &[DVec3],
) -> Option<Vec<CurvedRegion>> {
    let (axis_origin, axis_dir, radius, ref_dir) = match surface {
        AnalyticSurface::Cylinder { axis_origin, axis_dir, radius, ref_dir, .. } => {
            (*axis_origin, *axis_dir, *radius, *ref_dir)
        }
        _ => return None,
    };
    if loop_a.len() < 3 || loop_b.len() < 3 {
        return None;
    }
    // Raw (u, v) first, so the seam can be chosen before anything is unrolled.
    let raw = |p: DVec3| -> Option<(f64, f64)> {
        let (_sp, mut u, v) = cylinder::project_to_cylinder(axis_origin, axis_dir, radius, ref_dir, p)?;
        if u < 0.0 {
            u += std::f64::consts::TAU;
        }
        Some((u, v))
    };
    let ra: Vec<(f64, f64)> = loop_a.iter().map(|&p| raw(p)).collect::<Option<_>>()?;
    let rb: Vec<(f64, f64)> = loop_b.iter().map(|&p| raw(p)).collect::<Option<_>>()?;
    let chart = Unroll {
        axis_origin,
        axis_dir,
        radius,
        ref_dir,
        u_seam: seam_missing_both(&ra, &rb),
    };
    let pa: Vec<Vec2> = loop_a.iter().map(|&p| chart.to_2d(p)).collect::<Option<_>>()?;
    let pb: Vec<Vec2> = loop_b.iter().map(|&p| chart.to_2d(p)).collect::<Option<_>>()?;

    // Both loops into one planar graph; the sweep finds where they cross and
    // splits the edges there, which is the whole reason this is reusable.
    let eps = 1e-9;
    let mut g: PlanarGraph<u32> = PlanarGraph::new(eps);
    let mut next_vid = 0_u32;
    let mut make = |_p: Vec2| -> u32 {
        let v = next_vid;
        next_vid += 1;
        v
    };
    for poly in [&pa, &pb] {
        let n = poly.len();
        let mut ids = Vec::with_capacity(n);
        for &q in poly.iter() {
            ids.push(g.get_or_create_vertex(q, &mut make));
        }
        for i in 0..n {
            let (x, y) = (ids[i], ids[(i + 1) % n]);
            if x != y {
                g.create_edge(x, y, None);
            }
        }
    }
    let mut lineage = Lineage::default();
    for e in g.edges.values() {
        lineage.ensure_root(e.root);
    }
    bentley_ottmann_resolve(&mut g, &mut lineage, &mut make);
    g.dedup_parallel_edges(&mut lineage);

    let eps_area = (eps * eps).max(1e-18);
    let mut out = Vec::new();
    for r in extract_regions(&g, eps_area) {
        if r.signed_area <= eps_area {
            continue; // CW outer region, or a sliver
        }
        let inside_a = point_in_polygon(r.centroid, &pa);
        let inside_b = point_in_polygon(r.centroid, &pb);
        let share = match (inside_a, inside_b) {
            (true, true) => Share::Both,
            (true, false) => Share::FirstOnly,
            (false, true) => Share::SecondOnly,
            // Outside both: the rest of the wall, which is the remainder's
            // business and not a region of this arrangement.
            (false, false) => continue,
        };
        let positions: Vec<Vec2> = r
            .verts
            .iter()
            .filter_map(|v| g.vertices.get(v).map(|x| x.p))
            .collect();
        if positions.len() != r.verts.len() {
            return None;
        }
        out.push(CurvedRegion {
            boundary: positions.iter().map(|&q| chart.to_3d(q)).collect(),
            share,
            area: r.signed_area,
        });
    }
    // Two loops that genuinely cross give exactly three regions. Anything else
    // is disjoint, nested, or degenerate — all of which the caller already
    // handles, and none of which this should pretend to have resolved.
    if out.len() != 3 {
        return None;
    }
    let mut kinds: Vec<Share> = out.iter().map(|r| r.share).collect();
    kinds.sort_by_key(|k| *k as u8);
    kinds.dedup();
    if kinds.len() != 3 {
        return None;
    }
    Some(out)
}

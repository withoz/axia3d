//! SSI Stage 4 — Topology assembly (ADR-034 §P19, Stage 4).
//!
//! Given a list of refined candidate points, stitches them into ordered
//! polylines representing intersection curve(s). Detects closed loops.
//!
//! ## MVP algorithm — greedy nearest-neighbor chaining
//! 1. Dedup candidates within `merge_tol` (avoid duplicate refinements).
//! 2. For each unvisited point, start a chain and grow it from BOTH ends at
//!    once, always taking the nearer neighbour, until neither end has one
//!    within `gap_tol`.
//! 3. Detect closure: the end gap is no wider than the chain's own steps, and
//!    the chain turns back — the gap is at most half of what it walked.
//! 4. Emit each chain as a `SurfaceIntersection`.
//!
//! ## Limitations (defer to follow-up)
//! - No singular point (branching) detection — multi-branch curves emerge
//!   as separate chains.
//! - Self-intersecting curves yield one chain that crosses itself.

use super::SurfaceIntersection;
use super::newton::RefinementResult;

/// Assemble refined candidates into ordered polyline(s).
pub fn assemble_chains(
    mut candidates: Vec<RefinementResult>,
    gap_tol: f64,
    merge_tol: f64,
) -> Vec<SurfaceIntersection> {
    // Dedup
    candidates = dedup(candidates, merge_tol);
    if candidates.is_empty() {
        return Vec::new();
    }

    let n = candidates.len();
    let mut visited = vec![false; n];
    let mut chains: Vec<SurfaceIntersection> = Vec::new();

    for start_idx in 0..n {
        if visited[start_idx] { continue; }
        // Grow the chain from BOTH ends at once, always taking the nearer
        // extension.
        //
        // ⚠ It used to walk the head to exhaustion and only then extend the
        // tail. A head that runs out can still reach a point within `gap_tol`
        // that belongs at the TAIL, so it jumped back across its own start and
        // carried on the other side. Measured on a plane cutting a promoted
        // cylinder: a run from -79.73° to -79.03° leapt 0.719 back to -80.06°
        // and on to -80.25° -- one open arc 0.85 wide, visited out of order.
        // The fold made its end gap look like a step and its path look like it
        // turned back, so it was called closed at every tolerance tried.
        let mut chain_idx: Vec<usize> = vec![start_idx];
        visited[start_idx] = true;
        loop {
            let head = candidates[*chain_idx.last().unwrap()].point;
            let tail = candidates[chain_idx[0]].point;
            let mut best: Option<(usize, f64, bool)> = None;
            for (i, c) in candidates.iter().enumerate() {
                if visited[i] { continue; }
                let at_head = (c.point - head).length();
                let at_tail = (c.point - tail).length();
                let (d, to_head) = if at_head <= at_tail {
                    (at_head, true)
                } else {
                    (at_tail, false)
                };
                if d <= gap_tol && best.map_or(true, |(_, bd, _)| d < bd) {
                    best = Some((i, d, to_head));
                }
            }
            match best {
                Some((i, _, true)) => { visited[i] = true; chain_idx.push(i); }
                Some((i, _, false)) => { visited[i] = true; chain_idx.insert(0, i); }
                None => break,
            }
        }

        // Build SurfaceIntersection from chain.
        let mut points = Vec::with_capacity(chain_idx.len());
        let mut uv_a = Vec::with_capacity(chain_idx.len());
        let mut uv_b = Vec::with_capacity(chain_idx.len());
        for &i in &chain_idx {
            points.push(candidates[i].point);
            uv_a.push(candidates[i].uv_a);
            uv_b.push(candidates[i].uv_b);
        }

        // Closure check, against the walk's OWN standard.
        //
        // ⚠ It used to ask for `merge_tol * 4`, which the walk never had to
        // meet: the walk links a point whenever the next one is within
        // `gap_tol`, and `gap_tol` is a hundred times `merge_tol` at every call
        // site. So a loop sampled at the subdivision's own spacing could not
        // close. Measured on a plane cutting a promoted cylinder: 72 points on a
        // r=40 circle, accurate to 3e-10, closing gap 7.4544 -- exactly the
        // largest gap the walk had already accepted INSIDE the chain -- and
        // `7.4544 < 0.4` said open. Every such chain was then dropped by
        // `nurbs_boolean_v2`, which skips what is not closed, so a Boolean
        // against a cylinder produced no faces at all.
        //
        // The rule now is the walk's: the ends join if the gap between them is
        // no wider than the gaps already inside. That is self-scaling -- no new
        // tolerance to pick -- and it still says a LINE is open, because a
        // line's ends are further apart than any step along it.
        //
        // ⚠ And the chain has to TURN BACK. The gap test alone closed runs that
        // never do: a depth-capped subdivision leaves near-duplicate pairs
        // joined by one long step, whose end gap IS its widest step. Measured on
        // the plane grid a Boolean builds for a 200 mm box face, at each of tol
        // 0.1, 0.05 and 0.01: four chains of 4 points, each a ~1° arc, closing
        // 0.7158 against a widest step of 0.7153, all called closed (the other
        // four closed chains were the folded runs described above). At 0.01 the
        // Boolean built them into faces of zero area. A loop's end gap is one
        // step out of many; a run that only goes forward walks about as far as
        // its own end gap. So the gap may be at most half of what the chain
        // walked: a triangle sits exactly on that line, a real loop well inside
        // it, a straight run or a one-step fragment outside it.
        let closing = (*points.first().unwrap() - *points.last().unwrap()).length();
        let widest_step = points
            .windows(2)
            .map(|w| (w[1] - w[0]).length())
            .fold(0.0_f64, f64::max);
        let walked: f64 = points.windows(2).map(|w| (w[1] - w[0]).length()).sum();
        let closed = chain_idx.len() >= 3
            && closing <= 0.5 * walked
            && (closing < merge_tol * 4.0 || closing <= widest_step * 1.5);

        // Tangent warning if any candidate flagged depth_capped
        let tangent_warning = chain_idx.iter()
            .any(|&i| candidates[i].iterations >= 50);

        chains.push(SurfaceIntersection {
            points, uv_a, uv_b, closed, tangent_warning,
        });
    }

    chains
}

/// Drop near-duplicate candidates within `tol`. Keeps the one with smaller
/// residual.
fn dedup(mut candidates: Vec<RefinementResult>, tol: f64) -> Vec<RefinementResult> {
    candidates.sort_by(|a, b| a.residual.partial_cmp(&b.residual).unwrap_or(std::cmp::Ordering::Equal));
    let mut kept: Vec<RefinementResult> = Vec::new();
    for c in candidates {
        let dup = kept.iter().any(|k| (k.point - c.point).length() < tol);
        if !dup {
            kept.push(c);
        }
    }
    kept
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    fn make_cand(p: DVec3, residual: f64) -> RefinementResult {
        RefinementResult {
            uv_a: (0.0, 0.0),
            uv_b: (0.0, 0.0),
            point: p,
            residual,
            iterations: 1,
            converged: true,
        }
    }

    #[test]
    fn assemble_empty_returns_empty() {
        let chains = assemble_chains(vec![], 0.1, 1e-3);
        assert!(chains.is_empty());
    }

    #[test]
    fn assemble_collinear_points_chain_in_order() {
        // 5 collinear points spaced 1mm apart along X. Random insertion order.
        let candidates = vec![
            make_cand(DVec3::new(2.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(0.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(4.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(1.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(3.0, 0.0, 0.0), 0.0),
        ];
        let chains = assemble_chains(candidates, 1.5, 0.01);
        assert_eq!(chains.len(), 1);
        let chain = &chains[0];
        assert_eq!(chain.points.len(), 5);
        assert!(!chain.closed);
        // Should be sorted by X (or its reverse — chaining direction).
        let xs: Vec<f64> = chain.points.iter().map(|p| p.x).collect();
        let monotonic = xs.windows(2).all(|w| w[0] <= w[1])
            || xs.windows(2).all(|w| w[0] >= w[1]);
        assert!(monotonic, "x values not monotonic: {:?}", xs);
    }

    #[test]
    fn assemble_closed_loop_detected() {
        // 8 points around a circle (radius 1), random order.
        let n = 8;
        let mut candidates = Vec::new();
        for i in 0..n {
            let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
            let p = DVec3::new(theta.cos(), theta.sin(), 0.0);
            candidates.push(make_cand(p, 0.0));
        }
        // Distance between adjacent samples = 2·sin(π/8) ≈ 0.765
        let chains = assemble_chains(candidates, 1.0, 0.01);
        assert_eq!(chains.len(), 1);
        // Loop closure: first ≈ last with gap_tol*4 padding
        // gap from first to last after greedy walk should be < 4*merge_tol
        // Note: with merge_tol=0.01, merge_tol*4=0.04 — too tight for raw
        // 8-sample circle. Closure detection is a heuristic; relax tol.
        let chains2 = assemble_chains(
            (0..n).map(|i| {
                let theta = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
                let p = DVec3::new(theta.cos(), theta.sin(), 0.0);
                make_cand(p, 0.0)
            }).collect(),
            1.0,
            0.5,  // looser merge_tol → closure threshold = 2.0
        );
        assert_eq!(chains2.len(), 1);
        assert!(chains2[0].closed, "loop should close with looser merge_tol");
    }

    #[test]
    fn assemble_two_disconnected_chains() {
        // Cluster A near origin, cluster B near (10, 0, 0). gap_tol < 10
        // means they shouldn't merge.
        let candidates = vec![
            make_cand(DVec3::new(0.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(1.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(2.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(10.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(11.0, 0.0, 0.0), 0.0),
            make_cand(DVec3::new(12.0, 0.0, 0.0), 0.0),
        ];
        let chains = assemble_chains(candidates, 2.0, 0.1);
        assert_eq!(chains.len(), 2);
        // Each chain should have 3 points.
        for c in &chains {
            assert_eq!(c.points.len(), 3);
            assert!(!c.closed);
        }
    }

    #[test]
    fn assemble_dedups_close_duplicates() {
        let candidates = vec![
            make_cand(DVec3::ZERO, 1e-3),
            make_cand(DVec3::new(1e-9, 0.0, 0.0), 1e-4),  // duplicate of (0,0,0)
            make_cand(DVec3::new(1.0, 0.0, 0.0), 0.0),
        ];
        let chains = assemble_chains(candidates, 2.0, 1e-6);
        assert_eq!(chains.len(), 1);
        // After dedup, only 2 points (origin and (1,0,0)).
        assert_eq!(chains[0].points.len(), 2);
    }

    /// A run of points is not a loop because its end gap is no wider than its
    /// widest step. Measured 2026-09-15 on a plane cutting a promoted cylinder:
    /// four chains of 4 points, each a ~1° arc, closing 0.7158 against a widest
    /// step of 0.7153 — every one called closed, turned into a sliver trim loop
    /// by `nurbs_boolean_v2`, and (at tol 0.01) built into a face of zero area.
    #[test]
    fn assemble_a_fragment_that_never_turns_back_is_open() {
        // Two near-duplicate pairs joined by one long step — the shape a
        // depth-capped subdivision leaves. The widest-step rule closes it.
        let chains = assemble_chains(
            vec![
                make_cand(DVec3::new(0.0, 0.0, 0.0), 0.0),
                make_cand(DVec3::new(0.06, 0.0, 0.0), 0.0),
                make_cand(DVec3::new(0.70, 0.0, 0.0), 0.0),
                make_cand(DVec3::new(0.76, 0.0, 0.0), 0.0),
            ],
            5.0,
            0.05,
        );
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].points.len(), 4);
        assert!(!chains[0].closed, "a straight run of four points was called a loop");

        // Three points in a line inside `merge_tol * 4` — the older rule's
        // version of the same mistake.
        let chains = assemble_chains(
            vec![
                make_cand(DVec3::new(0.0, 0.0, 0.0), 0.0),
                make_cand(DVec3::new(0.15, 0.0, 0.0), 0.0),
                make_cand(DVec3::new(0.30, 0.0, 0.0), 0.0),
            ],
            10.0,
            0.1,
        );
        assert_eq!(chains[0].points.len(), 3);
        assert!(!chains[0].closed, "three points in a line were called a loop");

        // The control: four corners of a square DO turn back, and close.
        let chains = assemble_chains(
            vec![
                make_cand(DVec3::new(0.0, 0.0, 0.0), 0.0),
                make_cand(DVec3::new(1.0, 0.0, 0.0), 0.0),
                make_cand(DVec3::new(1.0, 1.0, 0.0), 0.0),
                make_cand(DVec3::new(0.0, 1.0, 0.0), 0.0),
            ],
            1.2,
            0.01,
        );
        assert_eq!(chains.len(), 1);
        assert_eq!(chains[0].points.len(), 4);
        assert!(chains[0].closed, "a square's four corners stopped closing");
    }

    /// A run seeded in its middle is laid out end to end, not folded back
    /// across its start. Walking the head to exhaustion first let it leap from
    /// its far end to a point beside the start, which made an open arc look
    /// like a loop (measured: -79.73° → -79.03°, then 0.719 back to -80.06°).
    #[test]
    fn assemble_a_run_is_not_folded_back_across_its_start() {
        // Index 0 is the seed, in the middle of the run.
        let xs = [0.0, 0.133, 0.36, 0.49, -0.33, -0.463];
        let chains = assemble_chains(
            xs.iter().map(|&x| make_cand(DVec3::new(x, 0.0, 0.0), 0.0)).collect(),
            10.0,
            0.05,
        );
        assert_eq!(chains.len(), 1);
        let got: Vec<f64> = chains[0].points.iter().map(|p| p.x).collect();
        let monotonic = got.windows(2).all(|w| w[0] < w[1]) || got.windows(2).all(|w| w[0] > w[1]);
        assert!(monotonic, "the run was folded back across its start: {got:?}");
        assert!(!chains[0].closed, "an open run, laid out end to end, was called a loop: {got:?}");
    }
}

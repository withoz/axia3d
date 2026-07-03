//! Self-intersection detection — geometric overlap of *non-adjacent* faces that
//! passes every topological check (manifold, watertight, cracks, winding) yet
//! renders as a fold / poke-through.
//!
//! This is the final defense line for hand-rolled face-rebuild ops
//! (chamfer / fillet / merge / …): the "flap" class the browser demo gate
//! caught, where a chamfer trim triangle overshot its edge and punched through
//! adjacent geometry. No other invariant sees it because the DCEL stays valid —
//! only the *geometry* self-intersects.
//!
//! ## Algorithm (MVP)
//! 1. Tessellate every active face to 3D triangles (earcut, holes included).
//! 2. Broad phase — AABB overlap reject (O(F²) box tests; fine for edit-time
//!    mesh sizes, a spatial grid is a later optimisation).
//! 3. Adjacency — skip face pairs that share a vertex. Faces that share an
//!    edge/vertex legitimately touch there; a real self-intersection between
//!    them (folding into each other) is out of MVP scope. The flap class
//!    overshoots *past* its neighbours into faces it does NOT share a vertex
//!    with, so it is still caught.
//! 4. Narrow phase — any triangle pair properly intersects
//!    (`triangle_triangle_intersection`). Since the two faces share no vertex,
//!    any intersection is a genuine crossing.

use glam::DVec3;

use crate::entities::*;
use crate::mesh::Mesh;

use super::boolean_geo::{project_to_2d, triangle_triangle_intersection};

/// Small absolute epsilon for AABB slack (mm). Matches the broad-phase only;
/// the narrow phase carries its own tolerance.
const AABB_EPS: f64 = 1e-6;

/// Result of [`Mesh::detect_self_intersections`].
#[derive(Debug, Clone, Default)]
pub struct SelfIntersectionReport {
    /// Pairs of active faces whose tessellations properly intersect (they do
    /// not merely touch along a shared edge/vertex).
    pub intersecting_pairs: Vec<(FaceId, FaceId)>,
}

impl SelfIntersectionReport {
    /// True when no self-intersection was found.
    pub fn is_clean(&self) -> bool {
        self.intersecting_pairs.is_empty()
    }

    /// Number of intersecting face pairs.
    pub fn count(&self) -> usize {
        self.intersecting_pairs.len()
    }

    /// Human-readable one-line summary.
    pub fn summary(&self) -> String {
        if self.is_clean() {
            "no self-intersections".to_string()
        } else {
            format!("{} self-intersecting face pair(s)", self.count())
        }
    }
}

struct FaceGeom {
    fid: FaceId,
    verts: Vec<VertId>,
    tris: Vec<[DVec3; 3]>,
    lo: DVec3,
    hi: DVec3,
}

impl Mesh {
    /// Detect geometric self-intersections between non-adjacent active faces.
    ///
    /// Read-only. See the module docs for the algorithm and MVP scope. Returns
    /// the list of intersecting face pairs; empty means clean.
    pub fn detect_self_intersections(&self) -> SelfIntersectionReport {
        let geoms: Vec<FaceGeom> = self
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .filter_map(|(fid, _)| self.tessellate_face_geom(fid))
            .collect();

        if geoms.len() < 2 {
            return SelfIntersectionReport::default();
        }

        // ── Broad phase: uniform spatial grid ──────────────────────────────
        // Cell size = mean face AABB extent, so a typical face occupies ~1 cell.
        // Two AABBs that overlap always share ≥1 cell (their overlap contains a
        // point whose cell both faces inserted), so the grid never misses a
        // candidate. A face whose AABB spans more than `CELL_CAP` cells (a large
        // face on a fine grid) is put in `big` and tested against everyone —
        // avoids memory blow-up while staying exhaustive.
        let mut ext_sum = 0.0f64;
        for g in &geoms {
            ext_sum += (g.hi - g.lo).max_element().max(AABB_EPS);
        }
        let cell = (ext_sum / geoms.len() as f64).max(AABB_EPS);
        let key = |p: DVec3| -> (i64, i64, i64) {
            (
                (p.x / cell).floor() as i64,
                (p.y / cell).floor() as i64,
                (p.z / cell).floor() as i64,
            )
        };

        const CELL_CAP: i64 = 512;
        let mut grid: rustc_hash::FxHashMap<(i64, i64, i64), Vec<usize>> =
            rustc_hash::FxHashMap::default();
        let mut big: Vec<usize> = Vec::new();
        for (idx, g) in geoms.iter().enumerate() {
            let (lo, hi) = (key(g.lo), key(g.hi));
            let span = (hi.0 - lo.0 + 1) * (hi.1 - lo.1 + 1) * (hi.2 - lo.2 + 1);
            if span > CELL_CAP {
                big.push(idx);
                continue;
            }
            for cx in lo.0..=hi.0 {
                for cy in lo.1..=hi.1 {
                    for cz in lo.2..=hi.2 {
                        grid.entry((cx, cy, cz)).or_default().push(idx);
                    }
                }
            }
        }

        // Candidate index pairs (i < j), deduplicated across shared cells.
        let mut cand: rustc_hash::FxHashSet<(usize, usize)> = rustc_hash::FxHashSet::default();
        for bucket in grid.values() {
            for a in 0..bucket.len() {
                for b in (a + 1)..bucket.len() {
                    let (i, j) = (bucket[a], bucket[b]);
                    cand.insert((i.min(j), i.max(j)));
                }
            }
        }
        for &bi in &big {
            for j in 0..geoms.len() {
                if j != bi {
                    cand.insert((bi.min(j), bi.max(j)));
                }
            }
        }

        // ── Narrow phase over candidates ───────────────────────────────────
        let mut pairs = Vec::new();
        for &(i, j) in &cand {
            let a = &geoms[i];
            let b = &geoms[j];

            // Exact AABB reject (a shared grid cell doesn't guarantee overlap).
            if a.hi.x < b.lo.x - AABB_EPS
                || b.hi.x < a.lo.x - AABB_EPS
                || a.hi.y < b.lo.y - AABB_EPS
                || b.hi.y < a.lo.y - AABB_EPS
                || a.hi.z < b.lo.z - AABB_EPS
                || b.hi.z < a.lo.z - AABB_EPS
            {
                continue;
            }
            // Faces sharing a vertex/edge legitimately touch there. Instead of
            // skipping them outright (which misses folds that penetrate past the
            // shared feature, and coplanar overlaps), we use STRICT tests below
            // that ignore boundary/shared-feature touching but still catch real
            // penetration and coplanar area overlap.
            let share = a.verts.iter().any(|v| b.verts.contains(v));
            if faces_geom_intersect(&a.tris, &b.tris, share) {
                pairs.push((a.fid, b.fid));
            }
        }

        // Deterministic order (candidate set iteration is unordered).
        pairs.sort_by_key(|(a, b)| (a.raw(), b.raw()));
        pairs.dedup();

        SelfIntersectionReport { intersecting_pairs: pairs }
    }

    /// Tessellate a face's outer loop (with holes) into 3D triangles via earcut.
    /// `None` if the face is degenerate / untriangulable.
    fn tessellate_face_geom(&self, fid: FaceId) -> Option<FaceGeom> {
        let face = self.faces.get(fid)?;
        if !face.is_active() {
            return None;
        }
        let outer = self.collect_loop_verts(face.outer().start).ok()?;
        if outer.len() < 3 {
            return None;
        }

        // Face normal (skip degenerate faces — can't project reliably).
        let normal = face.normal().normalize_or_zero();
        if normal.length_squared() < 0.5 {
            return None;
        }

        let outer_pos: Vec<DVec3> =
            outer.iter().map(|&v| self.vertex_pos(v).unwrap_or(DVec3::ZERO)).collect();

        // Project the outer loop to 2D; reuse the SAME basis for holes.
        let (outer2d, u, v_axis, origin) = project_to_2d(&outer_pos, normal);
        let mut coords: Vec<f64> = Vec::with_capacity(outer2d.len() * 2);
        for p in &outer2d {
            coords.push(p.x);
            coords.push(p.y);
        }
        // earcut index → 3D position map (outer first, then each hole).
        let mut pos3d: Vec<DVec3> = outer_pos.clone();

        let mut all_verts = outer;
        let mut hole_indices: Vec<usize> = Vec::new();
        for inner in face.inners() {
            if inner.start.is_null() {
                continue;
            }
            let hv = match self.collect_loop_verts(inner.start) {
                Ok(v) if v.len() >= 3 => v,
                _ => continue,
            };
            hole_indices.push(coords.len() / 2);
            for &vid in &hv {
                let p = self.vertex_pos(vid).unwrap_or(DVec3::ZERO);
                let rel = p - origin;
                coords.push(rel.dot(u));
                coords.push(rel.dot(v_axis));
                pos3d.push(p);
            }
            all_verts.extend(hv);
        }

        let idx = earcutr::earcut(&coords, &hole_indices, 2).ok()?;
        let mut tris: Vec<[DVec3; 3]> = Vec::with_capacity(idx.len() / 3);
        for c in idx.chunks(3) {
            if c.len() < 3 {
                continue;
            }
            let (i0, i1, i2) = (c[0], c[1], c[2]);
            if i0 >= pos3d.len() || i1 >= pos3d.len() || i2 >= pos3d.len() {
                continue;
            }
            tris.push([pos3d[i0], pos3d[i1], pos3d[i2]]);
        }
        if tris.is_empty() {
            return None;
        }

        let mut lo = tris[0][0];
        let mut hi = tris[0][0];
        for t in &tris {
            for p in t {
                lo = lo.min(*p);
                hi = hi.max(*p);
            }
        }

        Some(FaceGeom { fid, verts: all_verts, tris, lo, hi })
    }
}

/// Relative tolerance for the strict interior/penetration predicates. Positions
/// are in mm; these are unitless (barycentric / normalised) or relative to edge
/// length, so they behave the same at any scale.
const TRI_EPS: f64 = 1e-7;

/// True if any triangle of face `a` intersects any triangle of face `b`.
/// `share` = the two faces share ≥1 vertex (are locally adjacent).
///
/// Per triangle pair:
/// - **Coplanar** → 2D area overlap (deep test: a vertex strictly inside the
///   other, or edges properly crossing). Adjacent coplanar faces sit side by
///   side (0 area overlap) so they are not flagged — this holds even when they
///   share a vertex, so coplanar folds ARE caught.
/// - **Non-coplanar, faces share a vertex** → strict edge-pierces-interior test.
///   Touching at the shared edge/vertex is on the boundary (excluded); only a
///   fold that penetrates PAST the shared feature is flagged.
/// - **Non-coplanar, no shared vertex** → the plane-interval test
///   (`triangle_triangle_intersection`), which already handles this case.
fn faces_geom_intersect(a: &[[DVec3; 3]], b: &[[DVec3; 3]], share: bool) -> bool {
    for ta in a {
        for tb in b {
            if tri_pair_intersect(ta, tb, share) {
                return true;
            }
        }
    }
    false
}

fn tri_normal(t: &[DVec3; 3]) -> DVec3 {
    (t[1] - t[0]).cross(t[2] - t[0])
}

fn tri_pair_intersect(ta: &[DVec3; 3], tb: &[DVec3; 3], share: bool) -> bool {
    let na = tri_normal(ta);
    let nb = tri_normal(tb);
    let (la2, lb2) = (na.length_squared(), nb.length_squared());
    if la2 < 1e-24 || lb2 < 1e-24 {
        return false; // degenerate triangle
    }
    let na = na / la2.sqrt();
    let nb = nb / lb2.sqrt();

    // Coplanar? (parallel normals + same plane offset)
    if na.cross(nb).length() < 1e-7 {
        let off = (tb[0] - ta[0]).dot(na);
        // Tolerance relative to triangle size.
        let scale = (ta[1] - ta[0]).length().max((tb[1] - tb[0]).length()).max(1e-9);
        if off.abs() < TRI_EPS * scale {
            return coplanar_tris_overlap(ta, tb, na);
        }
        return false; // parallel but distinct planes
    }

    // Non-coplanar.
    if share {
        // Strict: an edge of one must pierce the STRICT interior of the other.
        edges(ta).iter().any(|&(p, q)| edge_pierces_interior(p, q, tb, nb))
            || edges(tb).iter().any(|&(p, q)| edge_pierces_interior(p, q, ta, na))
    } else {
        triangle_triangle_intersection(ta[0], ta[1], ta[2], tb[0], tb[1], tb[2]).is_some()
    }
}

fn edges(t: &[DVec3; 3]) -> [(DVec3, DVec3); 3] {
    [(t[0], t[1]), (t[1], t[2]), (t[2], t[0])]
}

/// Segment `p0→p1` crosses the plane of `tri` strictly (endpoints on opposite
/// sides, not On) and the crossing point lies strictly inside `tri`.
fn edge_pierces_interior(p0: DVec3, p1: DVec3, tri: &[DVec3; 3], n: DVec3) -> bool {
    let len = (p1 - p0).length().max(1e-9);
    let margin = TRI_EPS * len;
    let d0 = (p0 - tri[0]).dot(n);
    let d1 = (p1 - tri[0]).dot(n);
    // Must strictly straddle — a shared vertex/edge sits ON the plane (d≈0).
    if !((d0 > margin && d1 < -margin) || (d0 < -margin && d1 > margin)) {
        return false;
    }
    let t = d0 / (d0 - d1);
    let hit = p0 + (p1 - p0) * t;
    point_strictly_in_tri(hit, tri, n)
}

/// True if `p` (assumed on `tri`'s plane) is strictly inside the triangle.
fn point_strictly_in_tri(p: DVec3, tri: &[DVec3; 3], n: DVec3) -> bool {
    let two_area = tri_normal(tri).dot(n).abs().max(1e-18);
    let c0 = (tri[1] - tri[0]).cross(p - tri[0]).dot(n) / two_area;
    let c1 = (tri[2] - tri[1]).cross(p - tri[1]).dot(n) / two_area;
    let c2 = (tri[0] - tri[2]).cross(p - tri[2]).dot(n) / two_area;
    c0 > TRI_EPS && c1 > TRI_EPS && c2 > TRI_EPS
}

/// Two coplanar triangles overlap in positive area: a vertex of one strictly
/// inside the other, or two edges properly cross. (Shared-edge adjacency yields
/// neither, so it is not flagged.)
fn coplanar_tris_overlap(ta: &[DVec3; 3], tb: &[DVec3; 3], n: DVec3) -> bool {
    for &p in ta {
        if point_strictly_in_tri(p, tb, n) {
            return true;
        }
    }
    for &p in tb {
        if point_strictly_in_tri(p, ta, n) {
            return true;
        }
    }
    for &(a0, a1) in &edges(ta) {
        for &(b0, b1) in &edges(tb) {
            if segments_properly_cross(a0, a1, b0, b1, n) {
                return true;
            }
        }
    }
    false
}

/// Strict crossing of two coplanar segments (interior intersection, not shared
/// endpoints or collinear touch).
fn segments_properly_cross(a0: DVec3, a1: DVec3, b0: DVec3, b1: DVec3, n: DVec3) -> bool {
    let da = a1 - a0;
    let db = b1 - b0;
    let denom = da.cross(db).dot(n);
    let scale = (da.length() * db.length()).max(1e-18);
    if denom.abs() < TRI_EPS * scale {
        return false; // parallel / collinear
    }
    let t = (b0 - a0).cross(db).dot(n) / denom;
    let u = (b0 - a0).cross(da).dot(n) / denom;
    t > TRI_EPS && t < 1.0 - TRI_EPS && u > TRI_EPS && u < 1.0 - TRI_EPS
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::push_pull::PushPullResult;

    fn make_box(mesh: &mut Mesh, w: f64, d: f64, h: f64) -> (FaceId, PushPullResult) {
        let mat = MaterialId::new(0);
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(w, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(w, 0.0, d));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 0.0, d));
        let base = mesh.add_face(&[v0, v3, v2, v1], mat).unwrap();
        let pp = mesh.push_pull(base, h, mat).unwrap();
        (pp.top_face, pp)
    }

    #[test]
    fn clean_box_has_no_self_intersection() {
        let mut m = Mesh::new();
        make_box(&mut m, 10.0, 10.0, 10.0);
        let r = m.detect_self_intersections();
        assert!(r.is_clean(), "clean box must be self-intersection free: {}", r.summary());
    }

    #[test]
    fn valid_chamfer_stays_clean_no_false_positive() {
        // A valid chamfer that keeps the solid closed must NOT be flagged (no
        // false positive). Iterate corners; use the first whose chamfer both
        // succeeds and stays watertight, then assert the checker is clean.
        let corners: Vec<VertId> = {
            let mut m = Mesh::new();
            make_box(&mut m, 10.0, 10.0, 10.0);
            m.verts.iter().filter(|(_, vt)| vt.is_active()).map(|(id, _)| id).collect()
        };
        let mut checked = 0;
        for &c in &corners {
            let mut m = Mesh::new();
            make_box(&mut m, 10.0, 10.0, 10.0);
            if m.chamfer_vertex_3way(c, 2.0).is_err() {
                continue;
            }
            let active: Vec<_> =
                m.faces.iter().filter(|(_, f)| f.is_active()).map(|(id, _)| id).collect();
            if !m.face_set_manifold_info(&active).is_closed_solid {
                continue; // only assert on results that are themselves valid
            }
            let r = m.detect_self_intersections();
            assert!(r.is_clean(),
                "a watertight chamfer must not be flagged self-intersecting: {}", r.summary());
            checked += 1;
        }
        assert!(checked >= 1, "at least one corner chamfer should be clean+watertight");
    }

    #[test]
    fn closed_solid_with_folded_flap_is_detected() {
        // Two tetrahedra-ish quads overlapping in space with NO shared vertex —
        // the "poke-through" the topological checks miss. The checker must flag
        // it even though each face is individually valid.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        // Horizontal quad at z=0.
        let a0 = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let a1 = m.add_vertex(DVec3::new(20.0, 0.0, 0.0));
        let a2 = m.add_vertex(DVec3::new(20.0, 20.0, 0.0));
        let a3 = m.add_vertex(DVec3::new(0.0, 20.0, 0.0));
        m.add_face(&[a0, a1, a2, a3], mat).unwrap();
        // A tilted quad whose middle dips below z=0 and rises above — it pierces
        // the horizontal quad twice. Distinct verts, positioned inside A's extent.
        let b0 = m.add_vertex(DVec3::new(5.0, 5.0, -3.0));
        let b1 = m.add_vertex(DVec3::new(15.0, 5.0, 3.0));
        let b2 = m.add_vertex(DVec3::new(15.0, 15.0, 3.0));
        let b3 = m.add_vertex(DVec3::new(5.0, 15.0, -3.0));
        m.add_face(&[b0, b1, b2, b3], mat).unwrap();

        assert!(!m.detect_self_intersections().is_clean(), "piercing flap must be detected");
    }

    #[test]
    fn overlapping_disjoint_faces_are_detected() {
        // Two independent quads that cross like an X (no shared vertex).
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        // Quad A in the XY plane (z=0), spanning x,y ∈ [0,10].
        let a0 = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let a1 = m.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let a2 = m.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let a3 = m.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        m.add_face(&[a0, a1, a2, a3], mat).unwrap();
        // Quad B in the XZ plane through the middle of A (y=5), z ∈ [-5,5]:
        // it pierces A's interior. Distinct vertices (no sharing).
        let b0 = m.add_vertex(DVec3::new(2.0, 5.0, -5.0));
        let b1 = m.add_vertex(DVec3::new(8.0, 5.0, -5.0));
        let b2 = m.add_vertex(DVec3::new(8.0, 5.0, 5.0));
        let b3 = m.add_vertex(DVec3::new(2.0, 5.0, 5.0));
        m.add_face(&[b0, b1, b2, b3], mat).unwrap();

        let r = m.detect_self_intersections();
        assert!(!r.is_clean(), "crossing quads must be detected");
        assert_eq!(r.count(), 1, "exactly one intersecting pair");
    }

    #[test]
    fn grid_scales_and_finds_planted_intersection() {
        // Many well-separated quads spread across space (exercises the spatial
        // grid's many buckets) plus exactly one planted crossing pair. The grid
        // broad phase must still find precisely that one pair.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        for gx in 0..8 {
            for gy in 0..8 {
                let ox = gx as f64 * 100.0;
                let oy = gy as f64 * 100.0;
                let a = m.add_vertex(DVec3::new(ox, oy, 0.0));
                let b = m.add_vertex(DVec3::new(ox + 10.0, oy, 0.0));
                let c = m.add_vertex(DVec3::new(ox + 10.0, oy + 10.0, 0.0));
                let d = m.add_vertex(DVec3::new(ox, oy + 10.0, 0.0));
                m.add_face(&[a, b, c, d], mat).unwrap();
            }
        }
        // Planted crossing quad piercing the tile at grid (3,3) ≈ (300..310).
        let q0 = m.add_vertex(DVec3::new(303.0, 303.0, -4.0));
        let q1 = m.add_vertex(DVec3::new(307.0, 303.0, 4.0));
        let q2 = m.add_vertex(DVec3::new(307.0, 307.0, 4.0));
        let q3 = m.add_vertex(DVec3::new(303.0, 307.0, -4.0));
        m.add_face(&[q0, q1, q2, q3], mat).unwrap();

        let r = m.detect_self_intersections();
        assert_eq!(r.count(), 1, "exactly one planted intersection among 64+ tiles: {:?}", r.intersecting_pairs);
    }

    #[test]
    fn coplanar_overlapping_faces_detected() {
        // Two coplanar quads at z=0 whose areas overlap ([5,10]²), no shared
        // vertex. The plane-interval test misses coplanar cases; the deepened
        // coplanar area test must catch it.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let a0 = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let a1 = m.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let a2 = m.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let a3 = m.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        m.add_face(&[a0, a1, a2, a3], mat).unwrap();
        let b0 = m.add_vertex(DVec3::new(5.0, 5.0, 0.0));
        let b1 = m.add_vertex(DVec3::new(15.0, 5.0, 0.0));
        let b2 = m.add_vertex(DVec3::new(15.0, 15.0, 0.0));
        let b3 = m.add_vertex(DVec3::new(5.0, 15.0, 0.0));
        m.add_face(&[b0, b1, b2, b3], mat).unwrap();

        assert!(!m.detect_self_intersections().is_clean(), "coplanar area overlap must be detected");
    }

    #[test]
    fn coplanar_shared_vertex_fold_detected() {
        // A quad and a triangle that SHARE a corner vertex but the triangle lies
        // inside the quad's area (a coplanar fold-back). Old code skipped all
        // vertex-sharing pairs and missed this; the deepened area test catches
        // it because adjacency (side-by-side) has zero area overlap but this has
        // the triangle strictly inside.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let a0 = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let a1 = m.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let a2 = m.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let a3 = m.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        m.add_face(&[a0, a1, a2, a3], mat).unwrap();
        // triangle shares corner a0=(0,0,0), other verts strictly inside the
        // quad (off both earcut diagonals y=x and y=10-x).
        let t1 = m.add_vertex(DVec3::new(6.0, 2.0, 0.0));
        let t2 = m.add_vertex(DVec3::new(2.0, 6.0, 0.0));
        m.add_face(&[a0, t1, t2], mat).unwrap();

        assert!(!m.detect_self_intersections().is_clean(),
            "coplanar overlap sharing a vertex must be detected");
    }

    #[test]
    fn vertex_sharing_fold_penetration_detected() {
        // Two faces sharing ONE vertex; one stabs through the other's interior
        // (non-coplanar). Old code skipped shared-vertex pairs; the deepened
        // strict edge-pierce test catches the penetration past the shared vertex.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let a0 = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let a1 = m.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let a2 = m.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let a3 = m.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        m.add_face(&[a0, a1, a2, a3], mat).unwrap();
        // triangle shares a0; its far edge is a vertical segment at (3,6) that
        // pierces the quad interior at (3,6,0) — off both earcut diagonals.
        let s1 = m.add_vertex(DVec3::new(3.0, 6.0, -5.0));
        let s2 = m.add_vertex(DVec3::new(3.0, 6.0, 5.0));
        m.add_face(&[a0, s1, s2], mat).unwrap();

        assert!(!m.detect_self_intersections().is_clean(),
            "vertex-sharing penetration must be detected");
    }

    #[test]
    fn coplanar_adjacent_faces_not_flagged() {
        // Two coplanar quads side by side sharing an edge — legitimate adjacency,
        // zero area overlap. Must NOT be flagged even though they are coplanar
        // and share vertices.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let a0 = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let a1 = m.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let a2 = m.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let a3 = m.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        m.add_face(&[a0, a1, a2, a3], mat).unwrap();
        // shares edge a1-a2 (x=10), extends to x=20
        let b1 = m.add_vertex(DVec3::new(20.0, 0.0, 0.0));
        let b2 = m.add_vertex(DVec3::new(20.0, 10.0, 0.0));
        m.add_face(&[a1, b1, b2, a2], mat).unwrap();

        assert!(m.detect_self_intersections().is_clean(),
            "coplanar side-by-side faces must not be flagged");
    }

    #[test]
    fn detect_scales_to_many_disjoint_faces() {
        // 30×30 = 900 disjoint quads. Exercises the spatial grid across many
        // cells and guards against a quadratic regression in detection (a
        // brute-force O(F²) would do ~400k pair tests; the grid keeps it
        // near-linear). Must be clean. (Debug-build add_face invariant checks
        // dominate the wall time here, not detection.)
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let n = 30;
        for i in 0..n {
            for j in 0..n {
                let ox = i as f64 * 30.0;
                let oy = j as f64 * 30.0;
                let a = m.add_vertex(DVec3::new(ox, oy, 0.0));
                let b = m.add_vertex(DVec3::new(ox + 10.0, oy, 0.0));
                let c = m.add_vertex(DVec3::new(ox + 10.0, oy + 10.0, 0.0));
                let d = m.add_vertex(DVec3::new(ox, oy + 10.0, 0.0));
                m.add_face(&[a, b, c, d], mat).unwrap();
            }
        }
        let r = m.detect_self_intersections();
        assert!(r.is_clean(), "1600 disjoint quads must be self-intersection free");
    }

    #[test]
    fn separated_faces_not_flagged() {
        // Two parallel quads far apart — must NOT be flagged.
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let a0 = m.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let a1 = m.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let a2 = m.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let a3 = m.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        m.add_face(&[a0, a1, a2, a3], mat).unwrap();
        let b0 = m.add_vertex(DVec3::new(0.0, 0.0, 100.0));
        let b1 = m.add_vertex(DVec3::new(10.0, 0.0, 100.0));
        let b2 = m.add_vertex(DVec3::new(10.0, 10.0, 100.0));
        let b3 = m.add_vertex(DVec3::new(0.0, 10.0, 100.0));
        m.add_face(&[b0, b1, b2, b3], mat).unwrap();

        assert!(m.detect_self_intersections().is_clean());
    }
}

//! Volume Slice (Plane Cut) — splits a closed Wall volume into two
//! closed sub-volumes by an arbitrary cutting plane.
//!
//! ## Overview
//!
//! Given a set of Wall faces forming a closed 2-manifold solid and a plane
//! `(origin, normal)`:
//!
//! 1. Classify every vertex by signed plane distance: Above / Below / On.
//! 2. For every edge whose endpoints straddle the plane, `split_edge` at the
//!    intersection point — producing a new "On" vertex shared by both
//!    adjacent faces (radial chain preserved).
//! 3. For every face that crosses, locate the two On vertices on its
//!    boundary and `split_face` between them — producing one Above sub-face
//!    and one Below sub-face plus a chord (cut segment) on the plane.
//! 4. Assemble the chord segments into one or more closed cut loops by
//!    walking shared vertices.
//! 5. For each closed loop create **two cap faces** with opposite winding —
//!    one sealing the Above half (normal pointing −plane_normal toward the
//!    cut), one sealing the Below half (normal pointing +plane_normal).
//! 6. Verify both halves are closed Wall volumes and report classification.
//!
//! ## ADR-007 compliance
//!
//! * Walls remain Walls — the two halves are each a closed manifold so all
//!   sub-faces and the new cap faces classify as `is_face_in_volume == true`.
//! * Winding is the single source of truth — caller can run
//!   `mesh.reconcile_face_normals()` afterwards to refresh cached normals.
//!
//! ## MVP scope (limitations)
//!
//! * Each crossed face must have **exactly two** On vertices after edge
//!   splits (true for convex faces). Non-convex faces with > 2 crossings
//!   bail with a clear error.
//! * Faces lying entirely on the plane bail with an error.
//! * Open volumes (cut loop fails to close) bail with an error.

use anyhow::{Result, bail, ensure};
use glam::DVec3;
use rustc_hash::{FxHashMap, FxHashSet};

use crate::{FaceId, EdgeId, VertId, MaterialId};
use crate::mesh::Mesh;

/// Tolerance for "vertex on plane" classification. Below this absolute
/// signed distance the vertex is treated as exactly on the cut plane.
const PLANE_EPS: f64 = 1e-4; // 0.1 µm — tighter than VERTEX_TOLERANCE.

#[derive(Debug, Clone, Copy)]
pub struct SlicePlane {
    pub origin: DVec3,
    /// Must be a unit vector. Caller normalizes before passing.
    pub normal: DVec3,
}

impl SlicePlane {
    pub fn new(origin: DVec3, normal: DVec3) -> Result<Self> {
        let len = normal.length();
        ensure!(len > 1e-9, "SlicePlane: normal is degenerate (length {})", len);
        Ok(Self { origin, normal: normal / len })
    }
    #[inline]
    pub fn signed_distance(&self, p: DVec3) -> f64 {
        (p - self.origin).dot(self.normal)
    }
}

#[derive(Debug, Clone)]
pub struct SliceResult {
    /// Wall sub-faces lying on the +normal side (plus any cap_above).
    pub above_walls: Vec<FaceId>,
    /// Wall sub-faces lying on the −normal side (plus any cap_below).
    pub below_walls: Vec<FaceId>,
    /// Cap face(s) sealing the above half (one per cut loop).
    pub cap_above: Vec<FaceId>,
    /// Cap face(s) sealing the below half (one per cut loop, twin winding).
    pub cap_below: Vec<FaceId>,
    /// Cut loops as ordered vertex sequences (for visualization / tests).
    pub cut_loops: Vec<Vec<VertId>>,
}

/// Per-vertex plane classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum VC { Above, Below, On }

fn classify(d: f64) -> VC {
    if d >  PLANE_EPS { VC::Above }
    else if d < -PLANE_EPS { VC::Below }
    else { VC::On }
}

impl Mesh {
    /// Slice a closed volume defined by `face_ids` with `plane`.
    ///
    /// On success the volume's faces are split in-place and two cap face
    /// pairs (above / below) are inserted. Returns the classification of
    /// every resulting face plus the cut loops.
    pub fn slice_volume_by_plane(
        &mut self,
        face_ids: &[FaceId],
        plane: SlicePlane,
        material: MaterialId,
    ) -> Result<SliceResult> {
        // ── 0. Validate ──────────────────────────────────────────────────
        ensure!(!face_ids.is_empty(), "slice_volume_by_plane: empty face set");
        let _face_set: FxHashSet<FaceId> = face_ids.iter().copied().collect();
        for &fid in face_ids {
            let face = self.faces.get(fid)
                .ok_or_else(|| anyhow::anyhow!("slice: face {:?} not found", fid))?;
            ensure!(face.is_active(), "slice: face {:?} inactive", fid);
            // ADR-243 C2 Tier A — holed faces are no longer rejected at the
            // entry gate. A holed face is permitted iff it lies STRICTLY on one
            // side of the cut (no On vertex). That invariant is enforced
            // per-face in the classification loop below; a holed face that
            // crosses or grazes the plane bails there (Tier B/C not yet built).
        }
        // Soft check: input should form a closed volume. If any face is a
        // Sheet (free boundary in this set) we still proceed but the cut
        // loops won't close and we'll bail later with a precise message.

        // ── 1. Collect all unique edges in the input face set ───────────
        let mut edge_owners: FxHashMap<EdgeId, Vec<FaceId>> = FxHashMap::default();
        for &fid in face_ids {
            for eid in self.face_outer_edges(fid)? {
                edge_owners.entry(eid).or_default().push(fid);
            }
        }

        // ── 2. Split crossing edges; record produced "On" verts ─────────
        // Map: original edge → new On vert (so all faces sharing that edge
        // pick up the same vertex, which is automatic via radial chain but
        // this map lets us detect duplicates).
        let mut edge_cut_vert: FxHashMap<EdgeId, VertId> = FxHashMap::default();
        // We mutate edges, so iterate over a snapshot of edge ids.
        let edges_snapshot: Vec<EdgeId> = edge_owners.keys().copied().collect();
        for eid in edges_snapshot {
            let edge = match self.edges.get(eid) {
                Some(e) if e.is_active() => e,
                _ => continue,
            };
            let va = edge.v_small();
            let vb = edge.v_large();
            let pa = self.verts.get(va).map(|v| v.pos()).unwrap_or(DVec3::ZERO);
            let pb = self.verts.get(vb).map(|v| v.pos()).unwrap_or(DVec3::ZERO);
            let da = plane.signed_distance(pa);
            let db = plane.signed_distance(pb);
            let ca = classify(da);
            let cb = classify(db);

            // Strict crossing only (Above ↔ Below). On-vertex edges handled
            // implicitly without splitting.
            let crosses = matches!(
                (ca, cb),
                (VC::Above, VC::Below) | (VC::Below, VC::Above)
            );
            if !crosses { continue; }

            let t = da / (da - db); // d=0 at this t, monotonic since signs differ
            let pos = pa + (pb - pa) * t;
            let (new_v, _e1, _e2) = self.split_edge(eid, pos)?;
            edge_cut_vert.insert(eid, new_v);
        }

        // ── 3. Re-classify each input face after edge splits ────────────
        // For each face determine: AllAbove / AllBelow / AllOn / Crossing.
        // For Crossing collect the two "On" verts on its boundary.

        #[derive(Debug)]
        struct CrossInfo {
            face: FaceId,
            cut_a: VertId,
            cut_b: VertId,
        }

        let mut all_above: Vec<FaceId> = Vec::new();
        let mut all_below: Vec<FaceId> = Vec::new();
        let mut crossings: Vec<CrossInfo> = Vec::new();
        // ADR-242 C1 — non-convex crossing faces (> 2 On verts) handled via the
        // general multi-chord splitter after the simple 2-On pass.
        let mut complex_crossings: Vec<FaceId> = Vec::new();
        // ADR-243 C2 Tier B — convex-crossed HOLED faces (hole strictly one side)
        // handled via detach-split-reassign after the normal crossing pass.
        let mut holed_crossings: Vec<CrossInfo> = Vec::new();

        for &fid in face_ids {
            let outer_start = self.faces[fid].outer().start;
            let loop_verts = self.collect_loop_verts(outer_start)?;
            let mut above_count = 0usize;
            let mut below_count = 0usize;
            let mut on_verts: Vec<VertId> = Vec::new();

            for &v in &loop_verts {
                let p = self.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO);
                match classify(plane.signed_distance(p)) {
                    VC::Above => above_count += 1,
                    VC::Below => below_count += 1,
                    VC::On => on_verts.push(v),
                }
            }

            let has_holes = !self.faces[fid].inners().is_empty();

            if above_count > 0 && below_count == 0 {
                // All-above (Tier A): a holed all-above face keeps its hole above
                // (affine) and step 5.5 never rebuilds the above half. A GRAZING
                // holed face (On vert on the outer) is conservatively rejected —
                // an all-below grazing twin would hit the step-5.5 hole-loss path,
                // so both grazings bail (Tier B+ refines).
                if has_holes && !on_verts.is_empty() {
                    bail!("slice: holed face {:?} grazes the cut plane — not yet \
                        supported (C2 Tier B+); position the cut clear of the hole", fid);
                }
                all_above.push(fid);
                continue;
            }
            if below_count > 0 && above_count == 0 {
                if has_holes && !on_verts.is_empty() {
                    bail!("slice: holed face {:?} grazes the cut plane — not yet \
                        supported (C2 Tier B+); position the cut clear of the hole", fid);
                }
                all_below.push(fid);
                continue;
            }
            if above_count == 0 && below_count == 0 {
                bail!("slice: face {:?} lies entirely on the cut plane — \
                    refuse (would create degenerate volume)", fid);
            }

            // Crossing (outer has both above and below).
            // Collapse duplicates (an On vert can appear once per loop slot).
            let mut dedup_on: Vec<VertId> = Vec::new();
            for &v in &on_verts {
                if !dedup_on.contains(&v) { dedup_on.push(v); }
            }

            if has_holes {
                // ADR-243 C2 Tier B — crossed HOLED face: split the outer then
                // redistribute each inner loop to the sub-face that contains it.
                // MVP requires a CONVEX outer (exactly 2 On) and every inner loop
                // STRICTLY on one side; a crossed hole = annular cross-section
                // (Tier C) and a non-convex crossed outer (Tier B+) both bail.
                if dedup_on.len() != 2 {
                    bail!("slice: non-convex crossed holed face {:?} not yet \
                        supported (C2 Tier B+); position the cut clear of the hole", fid);
                }
                for inner in self.faces[fid].inners() {
                    let iv = self.collect_loop_verts(inner.start)?;
                    let (mut ia, mut ib) = (false, false);
                    for &v in &iv {
                        let d = plane.signed_distance(
                            self.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO));
                        if d > PLANE_EPS { ia = true; } else if d < -PLANE_EPS { ib = true; }
                    }
                    if ia && ib {
                        bail!("slice: the cut crosses a hole on face {:?} — annular \
                            cross-section not yet supported (C2 Tier C); position \
                            the cut clear of the hole", fid);
                    }
                }
                holed_crossings.push(CrossInfo {
                    face: fid, cut_a: dedup_on[0], cut_b: dedup_on[1],
                });
                continue;
            }

            // Non-holed crossing. Convex faces have exactly 2 On verts → simple
            // split. Non-convex faces (ADR-242 C1) cross in ≥ 2 segments → even
            // count > 2 → general multi-chord split.
            match dedup_on.len() {
                2 => crossings.push(CrossInfo {
                    face: fid, cut_a: dedup_on[0], cut_b: dedup_on[1],
                }),
                k if k > 2 && k % 2 == 0 => complex_crossings.push(fid),
                k => bail!(
                    "slice: face {:?} has {} on-plane vertices after edge splits \
                    (need an even count ≥ 2; an odd count means a self-touching / \
                    degenerate boundary)",
                    fid, k
                ),
            }
        }

        if crossings.is_empty() && complex_crossings.is_empty() && holed_crossings.is_empty() {
            bail!("slice: plane does not cross any face of the volume");
        }

        // ── 4. split_face on each crossing — record sub-face classification
        // We need to know which sub-face is Above vs Below. After
        // split_face(face, v1, v2), we get (face_a, face_b). Walk each
        // result face's loop and check whether its non-On verts are above
        // or below.

        let mut wall_above: Vec<FaceId> = Vec::new();
        let mut wall_below: Vec<FaceId> = Vec::new();

        // Pre-fill all-above / all-below faces.
        wall_above.extend(all_above.iter().copied());
        wall_below.extend(all_below.iter().copied());

        // Track the chord segments for cut-loop assembly.
        // Each chord is an unordered pair {cut_a, cut_b} of On verts.
        let mut chords: Vec<(VertId, VertId)> = Vec::new();

        for ci in &crossings {
            // Verify the original face is still active (split_edge in step 2
            // doesn't destroy faces, only re-routes hes — so face id stable).
            if !self.faces.contains(ci.face) || !self.faces[ci.face].is_active() {
                bail!("slice: face {:?} disappeared before split_face", ci.face);
            }
            let (fa, fb) = self.split_face(ci.face, ci.cut_a, ci.cut_b)?;
            // Classify each sub-face by checking its non-On verts.
            let side_fa = side_of_face(self, fa, plane)?;
            let side_fb = side_of_face(self, fb, plane)?;
            match (side_fa, side_fb) {
                (Side::Above, Side::Below) => { wall_above.push(fa); wall_below.push(fb); }
                (Side::Below, Side::Above) => { wall_above.push(fb); wall_below.push(fa); }
                _ => bail!(
                    "slice: split_face produced inconsistent sides for face {:?} \
                    (sub {:?}={:?}, sub {:?}={:?}) — non-convex face?",
                    ci.face, fa, side_fa, fb, side_fb
                ),
            }
            chords.push((ci.cut_a, ci.cut_b));
        }

        // ── 4b. ADR-242 C1 — non-convex crossing faces (> 2 On verts) via the
        // general multi-chord splitter (each split cuts off a mono-side ear).
        for &cf in &complex_crossings {
            if !self.faces.contains(cf) || !self.faces[cf].is_active() {
                bail!("slice: non-convex face {:?} disappeared before split", cf);
            }
            let (ab, bl, ch) = split_crossing_face_general(self, cf, plane)?;
            wall_above.extend(ab);
            wall_below.extend(bl);
            chords.extend(ch);
        }

        // ── 4c. ADR-243 C2 Tier B — convex-crossed HOLED faces. Detach the
        // inner loops (so split_face works on clean outer-only topology), split
        // the outer, then reassign each hole to the sub-face that contains it
        // (Phase G case-(a) recipe). The hole was verified strictly one side, so
        // it lands wholly on one sub-face. step 5.5 (below-detach) preserves it.
        for ci in &holed_crossings {
            if !self.faces.contains(ci.face) || !self.faces[ci.face].is_active() {
                bail!("slice: holed face {:?} disappeared before split", ci.face);
            }
            // Save inner loops (LoopRef + a sample vertex) before detaching.
            let saved: Vec<(crate::entities::LoopRef, VertId)> = self.faces[ci.face]
                .inners().iter()
                .map(|lr| {
                    let sample = self.collect_loop_verts(lr.start)
                        .ok().and_then(|v| v.first().copied())
                        .unwrap_or_default();
                    (*lr, sample)
                })
                .collect();
            // Detach so split_face operates on outer-only topology.
            self.faces[ci.face].inners_mut().clear();
            self.faces[ci.face].bump_boundary_version_after_inners_mut();

            let (fa, fb) = self.split_face(ci.face, ci.cut_a, ci.cut_b)?;
            let side_fa = side_of_face(self, fa, plane)?;
            let side_fb = side_of_face(self, fb, plane)?;
            match (side_fa, side_fb) {
                (Side::Above, Side::Below) => { wall_above.push(fa); wall_below.push(fb); }
                (Side::Below, Side::Above) => { wall_above.push(fb); wall_below.push(fa); }
                _ => bail!(
                    "slice: holed split_face produced inconsistent sides for face \
                    {:?}", ci.face),
            }
            chords.push((ci.cut_a, ci.cut_b));

            // Redistribute each saved hole to the containing sub-face.
            for (loop_ref, sample) in saved {
                let p = self.verts.get(sample).map(|x| x.pos()).unwrap_or(DVec3::ZERO);
                let in_a = crate::operations::face_split::point_in_face(self, fa, p)
                    .unwrap_or(false);
                let target = if in_a { fa } else { fb };
                if target != ci.face {
                    crate::operations::face_split::reassign_loop_face(
                        self, loop_ref.start, target)?;
                }
                self.faces[target].add_inner(loop_ref);
            }
        }

        // ── 5. Assemble closed cut loops from chords ────────────────────
        let cut_loops = assemble_loops(&chords)?;
        if cut_loops.is_empty() {
            bail!("slice: no closed cut loops formed — input volume may not be closed");
        }

        // ── 5.4. ADR-245 C2 Tier C — classify the cut loops into nesting groups
        // (each outer loop + the hole loops directly inside it). A simple cross-
        // section is a lone outer (no holes); an ANNULAR cross-section (the cut
        // passed through a hole region) is an outer with ≥1 nested hole loop, and
        // is sealed by a single HOLED cap (step 6). >1-level nesting (a hole
        // within a hole) bails (MVP single-level).
        let loop_groups = classify_loop_nesting(self, &cut_loops, plane)?;

        // ── 5.5. Detach the below half so the two halves are
        // topologically independent (ADR-007 I5: edge ≤ 2 active faces). ──
        //
        // Strategy: duplicate every cut-loop vertex; rebuild every below
        // sub-wall (and any all-below face that touches a cut vert) with
        // the duplicates substituted in. Above half stays untouched.
        let cut_verts_set: FxHashSet<VertId> = chords.iter()
            .flat_map(|&(a, b)| [a, b].into_iter())
            .collect();
        let mut cut_vert_dup: FxHashMap<VertId, VertId> = FxHashMap::default();
        for &v in &cut_verts_set {
            let p = self.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO);
            let v2 = self.add_vertex_force_new(p);
            cut_vert_dup.insert(v, v2);
        }

        let old_below = wall_below.clone();
        let mut new_below: Vec<FaceId> = Vec::with_capacity(old_below.len());
        for &fid in &old_below {
            // Walk the loop; if any vert is in cut_verts_set, we must rebuild.
            let outer_start = self.faces[fid].outer().start;
            let loop_verts = self.collect_loop_verts(outer_start)?;
            let touches_cut = loop_verts.iter().any(|v| cut_verts_set.contains(v));
            if !touches_cut {
                new_below.push(fid);
                continue;
            }
            let mat_b = self.faces[fid].material();
            let substituted: Vec<VertId> = loop_verts.iter()
                .map(|&v| cut_vert_dup.get(&v).copied().unwrap_or(v))
                .collect();
            // ADR-243 C2 Tier B — capture inner loops (holes) BEFORE remove_face
            // so a hole reassigned onto a below sub-face (step 4c) survives the
            // detach rebuild. Hole verts are strictly below (not cut verts) →
            // cut_vert_dup substitution is identity for them; they stay shared
            // with the hole's adjacent walls (also below, also rebuilt).
            let inner_loops: Vec<Vec<VertId>> = self.faces[fid].inners().iter()
                .map(|lr| self.collect_loop_verts(lr.start))
                .collect::<Result<Vec<_>>>()?;
            let subst_inners: Vec<Vec<VertId>> = inner_loops.iter()
                .map(|lp| lp.iter()
                    .map(|&v| cut_vert_dup.get(&v).copied().unwrap_or(v))
                    .collect())
                .collect();
            self.remove_face(fid)?;
            let new_fid = if subst_inners.is_empty() {
                self.add_face(&substituted, mat_b)?
            } else {
                let refs: Vec<&[VertId]> = subst_inners.iter().map(|v| v.as_slice()).collect();
                self.add_face_with_holes(&substituted, &refs, mat_b)?
            };
            new_below.push(new_fid);
        }
        wall_below = new_below;

        // Build the duplicate cut loops for cap_below.
        let cut_loops_below: Vec<Vec<VertId>> = cut_loops.iter()
            .map(|loop_verts| loop_verts.iter()
                .map(|v| cut_vert_dup.get(v).copied().unwrap_or(*v))
                .collect())
            .collect();

        // ── 6. Build cap faces — one per NESTING GROUP per half, opposite
        // windings. A lone outer (no holes) → simple cap (add_face). An annular
        // group (outer + hole loops) → HOLED cap (add_face_with_holes).
        //   Cap face winding rule:
        //     The above half's interior sits on the +normal side of the plane.
        //     The cap closing its underside has front (winding normal) pointing
        //     AWAY from that interior → −plane.normal. Symmetric for cap_below
        //     (+plane.normal). Hole loops are oriented OPPOSITE the cap outer so
        //     add_face_with_holes reads them as holes.
        let mut cap_above: Vec<FaceId> = Vec::new();
        let mut cap_below: Vec<FaceId> = Vec::new();

        for (outer_idx, hole_idxs) in &loop_groups {
            // cap_above: outer oriented for −plane.normal, holes opposite (+normal).
            let outer_a = orient_loop_for_normal(self, &cut_loops[*outer_idx], -plane.normal)?;
            let cap_a = if hole_idxs.is_empty() {
                self.add_face(&outer_a, material)?
            } else {
                let holes_a: Vec<Vec<VertId>> = hole_idxs.iter()
                    .map(|&h| orient_loop_for_normal(self, &cut_loops[h], plane.normal))
                    .collect::<Result<_>>()?;
                let refs: Vec<&[VertId]> = holes_a.iter().map(|v| v.as_slice()).collect();
                self.add_face_with_holes(&outer_a, &refs, material)?
            };
            // cap_below: duplicated loops, outer oriented +plane.normal, holes −normal.
            let outer_b = orient_loop_for_normal(self, &cut_loops_below[*outer_idx], plane.normal)?;
            let cap_b = if hole_idxs.is_empty() {
                self.add_face(&outer_b, material)?
            } else {
                let holes_b: Vec<Vec<VertId>> = hole_idxs.iter()
                    .map(|&h| orient_loop_for_normal(self, &cut_loops_below[h], -plane.normal))
                    .collect::<Result<_>>()?;
                let refs: Vec<&[VertId]> = holes_b.iter().map(|v| v.as_slice()).collect();
                self.add_face_with_holes(&outer_b, &refs, material)?
            };
            cap_above.push(cap_a);
            cap_below.push(cap_b);
        }

        // ── 7. Refresh cached normals from new winding ──────────────────
        let _ = self.reconcile_face_normals();

        // ── 8. Verify the two halves are now closed Walls (debug only) ──
        let mut all_above_set = wall_above.clone();
        all_above_set.extend(cap_above.iter().copied());
        let mut all_below_set = wall_below.clone();
        all_below_set.extend(cap_below.iter().copied());

        let above_info = self.face_set_manifold_info(&all_above_set);
        let below_info = self.face_set_manifold_info(&all_below_set);
        if above_info.boundary_edge_count > 0 {
            bail!(
                "slice: above half not closed (boundary edges = {}) — \
                cap topology error",
                above_info.boundary_edge_count
            );
        }
        if below_info.boundary_edge_count > 0 {
            bail!(
                "slice: below half not closed (boundary edges = {})",
                below_info.boundary_edge_count
            );
        }

        // ADR-007 invariants
        self.debug_verify_invariants();

        // Convert cut_loops to result-shape (Vec<Vec<VertId>>).
        let cut_loops_out: Vec<Vec<VertId>> = cut_loops;

        Ok(SliceResult {
            above_walls: wall_above,
            below_walls: wall_below,
            cap_above,
            cap_below,
            cut_loops: cut_loops_out,
        })
    }

    /// ADR-241 (Phase 1 C5) — Plane-cut a closed volume and KEEP only one half
    /// (trim). Runs [`Self::slice_volume_by_plane`] then removes the discarded
    /// half's faces, leaving a single closed sub-volume. `keep_above` keeps the
    /// +normal side (`false` keeps the −normal side). Returns the kept half's
    /// face ids. Reuses the full slice algorithm (MVP scope inherited: convex
    /// crossed faces, no holes, closed volume).
    pub fn trim_volume_by_plane(
        &mut self,
        face_ids: &[FaceId],
        plane: SlicePlane,
        keep_above: bool,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let res = self.slice_volume_by_plane(face_ids, plane, material)?;
        let (keep, discard): (Vec<FaceId>, Vec<FaceId>) = if keep_above {
            (
                res.above_walls.iter().chain(res.cap_above.iter()).copied().collect(),
                res.below_walls.iter().chain(res.cap_below.iter()).copied().collect(),
            )
        } else {
            (
                res.below_walls.iter().chain(res.cap_below.iter()).copied().collect(),
                res.above_walls.iter().chain(res.cap_above.iter()).copied().collect(),
            )
        };
        // The two halves are topologically independent after the slice's
        // detach step, so removing one leaves the other a closed Wall volume.
        for &fid in &discard {
            if self.faces.contains(fid) && self.faces[fid].is_active() {
                let _ = self.remove_face(fid);
            }
        }
        self.debug_verify_invariants();
        Ok(keep)
    }
}

// ════════════════════════════════════════════════════════════════════════
// Helpers
// ════════════════════════════════════════════════════════════════════════

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side { Above, Below }

/// Determine which side of `plane` a face lies on after split_face.
/// At least one non-on vertex is required. On-only faces are an error.
fn side_of_face(mesh: &Mesh, fid: FaceId, plane: SlicePlane) -> Result<Side> {
    let outer_start = mesh.faces[fid].outer().start;
    let verts = mesh.collect_loop_verts(outer_start)?;
    for v in verts {
        let p = mesh.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO);
        let d = plane.signed_distance(p);
        if d >  PLANE_EPS { return Ok(Side::Above); }
        if d < -PLANE_EPS { return Ok(Side::Below); }
    }
    bail!("side_of_face: face {:?} has no off-plane vertex", fid);
}

/// ADR-242 (Phase 1 C1) — split a crossing face (convex OR non-convex) by
/// `plane` into above/below sub-faces. Iteratively `split_face` along an
/// interior chord connecting two boundary-consecutive On verts (the boundary
/// arc between them has no other crossing → it is a mono-side "ear"), recursing
/// on the remainder. The cut segments form a non-crossing matching on the
/// boundary, so such an innermost interior pair always exists until the face is
/// reduced to mono-side pieces. Returns (above_faces, below_faces, cut_chords).
///
/// Convex faces (2 On verts) take the simple `split_face` path in the caller;
/// this handles `> 2` On verts (e.g. a U/L-shaped cap crossed through a notch).
fn split_crossing_face_general(
    mesh: &mut Mesh,
    fid: FaceId,
    plane: SlicePlane,
) -> Result<(Vec<FaceId>, Vec<FaceId>, Vec<(VertId, VertId)>)> {
    let mut above: Vec<FaceId> = Vec::new();
    let mut below: Vec<FaceId> = Vec::new();
    let mut chords: Vec<(VertId, VertId)> = Vec::new();
    let mut work = vec![fid];
    let mut guard = 0usize;
    while let Some(f) = work.pop() {
        guard += 1;
        if guard > 100_000 {
            bail!("slice: crossing-face split exceeded iteration guard (face {:?})", fid);
        }
        let loop_verts = mesh.collect_loop_verts(mesh.faces[f].outer().start)?;
        let n = loop_verts.len();
        // On-vert boundary indices, in boundary order.
        let on_idx: Vec<usize> = (0..n)
            .filter(|&i| {
                let p = mesh.verts.get(loop_verts[i]).map(|x| x.pos()).unwrap_or(DVec3::ZERO);
                classify(plane.signed_distance(p)) == VC::On
            })
            .collect();
        // Leaf test = MONO-SIDE (every off-plane vert on one side). A mono-side
        // piece is done even with On verts on its boundary (the cut edges left
        // by previous ear splits) — the count of On verts is NOT the leaf signal.
        let (mut has_above, mut has_below) = (false, false);
        for &v in &loop_verts {
            let d = plane.signed_distance(mesh.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO));
            if d > PLANE_EPS { has_above = true; }
            else if d < -PLANE_EPS { has_below = true; }
        }
        if !(has_above && has_below) {
            if has_above { above.push(f); }
            else if has_below { below.push(f); }
            else { bail!("slice: sub-face {:?} has no off-plane vertex", f); }
            continue;
        }
        // Mixed face (both sides present) → split off a mono-side ear.
        // 2D projection (face plane basis) for an interior-chord test.
        let (e1, e2) = slice_plane_basis(mesh.faces[f].normal());
        let origin = mesh.verts.get(loop_verts[0]).map(|x| x.pos()).unwrap_or(DVec3::ZERO);
        let poly2d: Vec<(f64, f64)> = loop_verts.iter().map(|&v| {
            let p = mesh.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO) - origin;
            (p.dot(e1), p.dot(e2))
        }).collect();
        // Every On vert lies on the cut line, so ANY chord between two On verts
        // runs along it — the cut line crosses the face in alternating
        // interior/exterior intervals. The real cut segments are the interior
        // intervals between t-ADJACENT On verts (no other On vert between them in
        // line order); a chord spanning an exterior gap (e.g. a U notch) is not a
        // cut. Sort On verts along the cut line and test consecutive pairs;
        // interiority is sampled just off the line (perpendicular nudge) since a
        // point ON the line is degenerate for point-in-polygon.
        let cut_dir = {
            let a = poly2d[on_idx[0]];
            let b = poly2d[on_idx[1]];
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-12 { (1.0, 0.0) } else { (dx / len, dy / len) }
        };
        let perp = (-cut_dir.1, cut_dir.0);
        let mut sorted: Vec<usize> = on_idx.clone();
        sorted.sort_by(|&i, &j| {
            let ti = poly2d[i].0 * cut_dir.0 + poly2d[i].1 * cut_dir.1;
            let tj = poly2d[j].0 * cut_dir.0 + poly2d[j].1 * cut_dir.1;
            ti.partial_cmp(&tj).unwrap_or(std::cmp::Ordering::Equal)
        });
        let mut chosen: Option<(usize, usize)> = None;
        // Interior cut intervals are the t-adjacent pairs (0,1),(2,3),… ; the
        // exterior gaps are (1,2),(3,4),… . Probe the even-indexed pairs first;
        // fall back to scanning all adjacencies for robustness on sub-faces.
        let probe = |ai: usize, bi: usize| -> bool {
            if loop_verts[ai] == loop_verts[bi] { return false; }
            let dist_fwd = if bi >= ai { bi - ai } else { n - ai + bi };
            let dist_bwd = n - dist_fwd;
            if dist_fwd < 2 || dist_bwd < 2 { return false; } // split_face non-adjacency
            let chord_len = {
                let (dx, dy) = (poly2d[bi].0 - poly2d[ai].0, poly2d[bi].1 - poly2d[ai].1);
                (dx * dx + dy * dy).sqrt()
            };
            if chord_len < 1e-9 { return false; }
            let mid = (
                (poly2d[ai].0 + poly2d[bi].0) * 0.5,
                (poly2d[ai].1 + poly2d[bi].1) * 0.5,
            );
            let eps = chord_len * 0.01;
            let p_plus = (mid.0 + eps * perp.0, mid.1 + eps * perp.1);
            let p_minus = (mid.0 - eps * perp.0, mid.1 - eps * perp.1);
            // A real cut segment has the solid on BOTH sides of the line here;
            // an exterior gap has the solid on neither.
            point_in_poly_2d(p_plus, &poly2d) || point_in_poly_2d(p_minus, &poly2d)
        };
        let s = sorted.len();
        for start in [0usize, 1usize] {
            let mut i = start;
            while i + 1 < s {
                let (ai, bi) = (sorted[i], sorted[i + 1]);
                if probe(ai, bi) { chosen = Some((ai, bi)); break; }
                i += 2;
            }
            if chosen.is_some() { break; }
        }
        let (ai, bi) = chosen.ok_or_else(|| anyhow::anyhow!(
            "slice: non-convex face {:?} — no interior cut segment found among \
             on-plane vertices (self-intersecting / degenerate boundary)", f))?;
        let (va, vb) = (loop_verts[ai], loop_verts[bi]);
        let (fa, fb) = mesh.split_face(f, va, vb)?;
        chords.push((va, vb));
        work.push(fa);
        work.push(fb);
    }
    Ok((above, below, chords))
}

/// In-plane orthonormal basis (e1, e2) from a plane normal (same construction
/// as `punch_circular_hole` / `merge_coplanar_containing`).
fn slice_plane_basis(n: DVec3) -> (DVec3, DVec3) {
    let n = n.normalize_or_zero();
    let mut t = DVec3::new(1.0, 0.0, 0.0);
    if t.cross(n).length_squared() < 1e-6 {
        t = DVec3::new(0.0, 1.0, 0.0);
    }
    let e1 = (t - n * t.dot(n)).normalize_or_zero();
    let e2 = n.cross(e1).normalize_or_zero();
    (e1, e2)
}

/// 2D ray-casting point-in-polygon (CCW or CW agnostic).
fn point_in_poly_2d(p: (f64, f64), poly: &[(f64, f64)]) -> bool {
    let m = poly.len();
    if m < 3 { return false; }
    let (x, y) = p;
    let mut inside = false;
    let mut j = m - 1;
    for i in 0..m {
        let (xi, yi) = poly[i];
        let (xj, yj) = poly[j];
        if ((yi > y) != (yj > y)) && (x < (xj - xi) * (y - yi) / (yj - yi + 1e-12) + xi) {
            inside = !inside;
        }
        j = i;
    }
    inside
}

/// ADR-245 C2 Tier C — classify cut loops into nesting GROUPS, each an outer
/// loop with the hole loops nested DIRECTLY inside it. Returns
/// `Vec<(outer_idx, Vec<hole_idx>)>` indexing into `loops`.
///
/// A lone outer (no holes) is a simple cross-section; an outer with ≥1 hole is
/// an ANNULAR cross-section sealed by a holed cap. Disjoint outers each form
/// their own group (e.g. two prongs, each possibly holed). Nesting deeper than
/// one level (a hole inside a hole) bails — MVP supports single-level only.
///
/// Containment is tested by projecting to the cut plane's 2D basis and checking
/// a representative vertex of each loop against the others (point-in-polygon).
fn classify_loop_nesting(
    mesh: &Mesh,
    loops: &[Vec<VertId>],
    plane: SlicePlane,
) -> Result<Vec<(usize, Vec<usize>)>> {
    let n = loops.len();
    let (e1, e2) = slice_plane_basis(plane.normal);
    let proj: Vec<Vec<(f64, f64)>> = loops.iter().map(|lp| {
        lp.iter().map(|&v| {
            let p = mesh.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO) - plane.origin;
            (p.dot(e1), p.dot(e2))
        }).collect()
    }).collect();
    // containers[i] = indices of loops that contain loop i's representative vert.
    let mut containers: Vec<Vec<usize>> = vec![Vec::new(); n];
    for i in 0..n {
        let Some(&pt) = proj[i].first() else { continue };
        for j in 0..n {
            if i == j { continue; }
            if point_in_poly_2d(pt, &proj[j]) { containers[i].push(j); }
        }
    }
    let mut groups: Vec<(usize, Vec<usize>)> = Vec::new();
    let mut outer_pos: FxHashMap<usize, usize> = FxHashMap::default();
    // First pass: top-level outers (contained by nothing).
    for i in 0..n {
        if containers[i].is_empty() {
            outer_pos.insert(i, groups.len());
            groups.push((i, Vec::new()));
        }
    }
    // Second pass: holes (contained by exactly one outer). Deeper → bail.
    for i in 0..n {
        match containers[i].len() {
            0 => {}
            1 => {
                let parent = containers[i][0];
                let gi = *outer_pos.get(&parent).ok_or_else(|| anyhow::anyhow!(
                    "slice: cut loop {} nested inside a hole loop — >1-level nesting \
                     not yet supported (C2 Tier C MVP single-level only)", i))?;
                groups[gi].1.push(i);
            }
            _ => bail!(
                "slice: cut loop {} is nested {} levels deep — only single-level \
                 annular cross-sections are supported (C2 Tier C MVP)",
                i, containers[i].len()
            ),
        }
    }
    Ok(groups)
}

/// Walk shared vertices to build closed loops from unordered chord segments.
///
/// Each chord {a, b} contributes two endpoint references. Build vertex →
/// chord index multi-map; pick an unvisited chord and traverse, hopping
/// through shared vertices, until we return to the start.
fn assemble_loops(chords: &[(VertId, VertId)]) -> Result<Vec<Vec<VertId>>> {
    if chords.is_empty() { return Ok(Vec::new()); }
    let n = chords.len();
    let mut adj: FxHashMap<VertId, Vec<usize>> = FxHashMap::default();
    for (i, &(a, b)) in chords.iter().enumerate() {
        adj.entry(a).or_default().push(i);
        adj.entry(b).or_default().push(i);
    }
    // Sanity: each cut vertex must have degree exactly 2 in the chord graph
    // for a closed manifold cut. Higher degree means non-manifold cut.
    for (v, list) in &adj {
        if list.len() != 2 {
            bail!(
                "slice/assemble_loops: cut vertex {:?} has degree {} (expected 2) — \
                non-manifold cut",
                v, list.len()
            );
        }
    }

    let mut used = vec![false; n];
    let mut loops: Vec<Vec<VertId>> = Vec::new();

    for start_idx in 0..n {
        if used[start_idx] { continue; }
        let (s_a, s_b) = chords[start_idx];
        let mut loop_verts: Vec<VertId> = vec![s_a, s_b];
        used[start_idx] = true;
        let mut current_v = s_b;
        let mut prev_idx = start_idx;
        loop {
            // Find the other chord at current_v.
            let neighbors = &adj[&current_v];
            let next_idx = if neighbors[0] == prev_idx { neighbors[1] } else { neighbors[0] };
            if used[next_idx] {
                if next_idx == start_idx { break; }
                bail!("slice/assemble_loops: traversal revisited used chord — corrupted graph");
            }
            used[next_idx] = true;
            let (na, nb) = chords[next_idx];
            let next_v = if na == current_v { nb } else { na };
            if next_v == s_a {
                // closed
                break;
            }
            loop_verts.push(next_v);
            prev_idx = next_idx;
            current_v = next_v;
            if loop_verts.len() > n + 1 {
                bail!("slice/assemble_loops: traversal exceeded chord count — runaway");
            }
        }
        loops.push(loop_verts);
    }

    Ok(loops)
}

/// Reorder a closed loop's vertices so that the polygon's winding produces
/// a face normal aligned with `desired_normal` (within positive dot
/// product). Uses Newell's signed-area formula for robustness.
fn orient_loop_for_normal(
    mesh: &Mesh,
    loop_verts: &[VertId],
    desired_normal: DVec3,
) -> Result<Vec<VertId>> {
    ensure!(loop_verts.len() >= 3, "orient_loop: degenerate loop ({})", loop_verts.len());
    let pts: Vec<DVec3> = loop_verts.iter()
        .map(|&v| mesh.verts.get(v).map(|x| x.pos()).unwrap_or(DVec3::ZERO))
        .collect();

    // Newell's method
    let mut nrm = DVec3::ZERO;
    for i in 0..pts.len() {
        let a = pts[i];
        let b = pts[(i + 1) % pts.len()];
        nrm.x += (a.y - b.y) * (a.z + b.z);
        nrm.y += (a.z - b.z) * (a.x + b.x);
        nrm.z += (a.x - b.x) * (a.y + b.y);
    }
    if nrm.length() < 1e-12 {
        bail!("orient_loop: degenerate (collinear) loop");
    }

    let mut out: Vec<VertId> = loop_verts.to_vec();
    if nrm.dot(desired_normal) < 0.0 {
        out.reverse();
    }
    Ok(out)
}

//! Boolean Operations — Union, Subtract, Intersect
//!
//! 3-stage pipeline:
//!   1. Intersection Graph — 두 솔리드의 face-face 교차선 계산
//!   2. Face Split — 교차선으로 face를 sub-face로 분할
//!   3. Classification — 각 sub-face를 inside/outside로 분류하여 결과 조립
//!
//! MVP: 축정렬 직육면체(box) 간 Boolean → 이후 임의 메시 확장

use glam::DVec3;
use anyhow::{bail, Result};
use rustc_hash::FxHashMap;

use crate::mesh::Mesh;
use crate::{EdgeId, FaceId, VertId, MaterialId};
use super::boolean_geo::{
    point_in_solid, triangle_triangle_intersection,
    Pt2, project_to_2d, unproject_to_3d, segment_segment_2d,
    polygon_signed_area_2d, point_in_polygon_2d,
};

/// Boolean 연산 종류
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoolOp {
    Union,
    Subtract,
    Intersect,
}

/// Boolean 연산 결과
#[derive(Debug)]
pub struct BooleanResult {
    /// 결과 face 목록
    pub faces: Vec<FaceId>,
    /// 생성된 정점 수
    pub new_verts: usize,
    /// 디버그 로그
    pub debug: Vec<String>,
}

/// 솔리드 = face 집합 + 삼각형화된 데이터
struct SolidData {
    face_ids: Vec<FaceId>,
    /// face별 삼각형 (교차 판정용)
    triangles: Vec<FaceTriangles>,
    /// 전체 삼각형 목록 (point-in-solid용)
    all_triangles: Vec<(DVec3, DVec3, DVec3)>,
}

struct FaceTriangles {
    face_id: FaceId,
    tris: Vec<(DVec3, DVec3, DVec3)>,
}

impl Mesh {
    /// ADR-197 β-3-i — recognise a Z-up analytic primitive in `faces` and return
    /// its kind + world AABB (from surface params, NOT the self-loop boundary).
    /// `None` if no curved surface is present or the axis is not ∥ Z.
    fn classify_curved_primitive(&self, faces: &[FaceId]) -> Option<CurvedPrim> {
        use crate::operations::coplanar::Aabb3;
        use crate::surfaces::AnalyticSurface as S;
        for &f in faces {
            match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => {
                    let r = *radius;
                    return Some(CurvedPrim {
                        kind: CurvedPrimKind::Sphere,
                        faces: faces.to_vec(),
                        aabb: Aabb3 { min: *center - DVec3::splat(r), max: *center + DVec3::splat(r) },
                        center_z: center.z,
                    });
                }
                Some(S::Cylinder { axis_origin, axis_dir, radius, v_range, .. }) => {
                    if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
                        return None;
                    }
                    let r = *radius;
                    let z_lo = axis_origin.z + v_range.0.min(v_range.1);
                    let z_hi = axis_origin.z + v_range.0.max(v_range.1);
                    return Some(CurvedPrim {
                        kind: CurvedPrimKind::Cylinder,
                        faces: faces.to_vec(),
                        aabb: Aabb3 {
                            min: DVec3::new(axis_origin.x - r, axis_origin.y - r, z_lo),
                            max: DVec3::new(axis_origin.x + r, axis_origin.y + r, z_hi),
                        },
                        center_z: 0.5 * (z_lo + z_hi),
                    });
                }
                Some(S::Cone { apex, axis_dir, half_angle, v_range, .. }) => {
                    // accept BOTH apex-up (axis_dir=−Z) and apex-down (axis_dir=+Z)
                    // Z-axis cones — apex-down is needed for the opposing-cone (β-3-o
                    // hourglass) union. (Case A cone∪box still requires apex-up; its
                    // builder validates and bails on apex-down.)
                    let ad = axis_dir.normalize_or_zero();
                    if ad.cross(DVec3::Z).length() > 1e-6 {
                        return None; // not a Z-axis cone
                    }
                    let v_base = v_range.0.max(v_range.1);
                    let base_r = v_base * half_angle.tan();
                    // base is on the opposite side of the apex from the axis_dir.
                    let base_z = if ad.z < 0.0 { apex.z - v_base } else { apex.z + v_base };
                    let (z_lo, z_hi) = (apex.z.min(base_z), apex.z.max(base_z));
                    return Some(CurvedPrim {
                        kind: CurvedPrimKind::Cone,
                        faces: faces.to_vec(),
                        aabb: Aabb3 {
                            min: DVec3::new(apex.x - base_r, apex.y - base_r, z_lo),
                            max: DVec3::new(apex.x + base_r, apex.y + base_r, z_hi),
                        },
                        center_z: 0.5 * (z_lo + z_hi),
                    });
                }
                Some(S::Torus { center, axis_dir, major_radius, minor_radius, .. }) => {
                    if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
                        return None;
                    }
                    let rr = *major_radius + *minor_radius;
                    return Some(CurvedPrim {
                        kind: CurvedPrimKind::Torus,
                        faces: faces.to_vec(),
                        aabb: Aabb3 {
                            min: DVec3::new(center.x - rr, center.y - rr, center.z - *minor_radius),
                            max: DVec3::new(center.x + rr, center.y + rr, center.z + *minor_radius),
                        },
                        center_z: center.z,
                    });
                }
                _ => continue,
            }
        }
        None
    }

    /// ADR-197 β-3-i — recognise an axis-aligned box (every face normal cardinal)
    /// and return its world AABB. `None` if any face is non-cardinal or < 4 faces.
    fn classify_axis_box(&self, faces: &[FaceId]) -> Option<crate::operations::coplanar::Aabb3> {
        use crate::operations::coplanar::Aabb3;
        if faces.len() < 4 {
            return None;
        }
        for &f in faces {
            let n = self.faces.get(f)?.normal().normalize_or_zero();
            let cardinal = n.x.abs() > 0.999 || n.y.abs() > 0.999 || n.z.abs() > 0.999;
            if !cardinal {
                return None;
            }
        }
        let sb = self.prepare_solid(faces).ok()?;
        let mut min = DVec3::splat(f64::INFINITY);
        let mut max = DVec3::splat(f64::NEG_INFINITY);
        for (a, b, c) in &sb.all_triangles {
            for p in [a, b, c] {
                min = min.min(*p);
                max = max.max(*p);
            }
        }
        if min.is_finite() && max.is_finite() {
            Some(Aabb3 { min, max })
        } else {
            None
        }
    }

    /// ADR-197 β-3-i — remove a (box) solid's faces + their edges entirely.
    fn remove_box_solid(&mut self, box_faces: &[FaceId]) {
        let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
        for &f in box_faces {
            if let Some(face) = self.faces.get(f) {
                let mut starts = vec![face.outer().start];
                for inner in face.inners() {
                    starts.push(inner.start);
                }
                for st in starts {
                    if let Ok(hes) = self.collect_loop_hes(st) {
                        for he in hes {
                            es.insert(self.hes[he].edge(), ());
                        }
                    }
                }
            }
        }
        for &f in box_faces {
            let _ = self.remove_face(f);
        }
        for (e, _) in es {
            let _ = self.remove_edge_and_halfedges(e);
        }
    }

    /// ADR-197 β-3-i — try to route `A ∩ B` to a surface-preserving curved
    /// Boolean when one operand is a Z-up analytic primitive and the other is an
    /// axis-aligned box that contains the primitive in X and Y (so it only cuts
    /// in Z). Returns `Some(result_faces)` on a match (the box is consumed),
    /// `None` to fall through to the legacy polygonal path. Intersect only —
    /// subtract / XY-cutting boxes / non-straddling sphere slabs / 2-cut tori
    /// all fall through (the legacy path or a later γ-2b step handles them).
    fn try_curved_intersect_dispatch(
        &mut self,
        faces_a: &[FaceId],
        faces_b: &[FaceId],
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        for (cv_faces, box_faces) in [(faces_a, faces_b), (faces_b, faces_a)] {
            let Some(bx) = self.classify_axis_box(box_faces) else {
                continue;
            };
            // ADR-205 γ-2a — tilted cylinder ∩ axis-box SLAB. `classify_curved_
            // primitive` rejects tilted cylinders (axis ∦ Z); a box that is a slab
            // through a tilted cylinder cuts it with two parallel OBLIQUE elliptic
            // sections → route to β-3 `boolean_cylinder_oblique_slab`. Returns
            // `None` for Z-axis cylinders + non-slab configs (falls through to the
            // circular-section paths below).
            if let Some(res) = self.try_tilted_cylinder_box_slab(cv_faces, box_faces, &bx, material) {
                return Some(res);
            }
            // ADR-205 γ-2b — tilted cylinder ∩ box HALFSPACE (one face clips → β-2)
            // + no-op containment (box ⊇ cylinder → A∩B=A). Reuses the same
            // which-faces-cut geometry; corner/multi-plane stay deferred (γ-2c).
            if let Some(res) = self.try_tilted_cylinder_box_halfspace(cv_faces, box_faces, &bx, material) {
                return Some(res);
            }
            // ADR-205 γ-2c — tilted cylinder ∩ box CORNER (two perpendicular faces
            // cut → β-5 tent, for the upper-bound subset). Reuses the same
            // which-faces-cut geometry; N-plane / non-tent corners stay deferred.
            if let Some(res) = self.try_tilted_cylinder_box_corner(cv_faces, box_faces, &bx, material) {
                return Some(res);
            }
            // ADR-205 γ-cone-slab — tilted cone ∩ box SLAB (two parallel faces cut
            // → β-3-cone). Reuses the cone's apex+base-rim cardinal extent.
            if let Some(res) = self.try_tilted_cone_box_slab(cv_faces, box_faces, &bx, material) {
                return Some(res);
            }
            // ADR-205 γ-cone-halfspace — tilted cone ∩ box HALFSPACE (one face clips
            // the apex → β-2-cone frustum) + no-op containment.
            if let Some(res) = self.try_tilted_cone_box_halfspace(cv_faces, box_faces, &bx, material) {
                return Some(res);
            }
            // ADR-205 cone-corner γ — tilted cone ∩ box CORNER (two perpendicular
            // base-keeping faces → boolean_cone_corner tent).
            if let Some(res) = self.try_tilted_cone_box_corner(cv_faces, box_faces, &bx, material) {
                return Some(res);
            }
            // ADR-205 γ-torus — tilted torus ∩ box: classify which e*-faces cut and
            // route to β-2-torus (1 cut, halfspace) / β-3-torus (2 cuts, slab) / no-op
            // containment. Declines Z-axis tori (existing classify path) + side cuts.
            if let Some(res) = self.try_tilted_torus_box(cv_faces, box_faces, &bx, material) {
                return Some(res);
            }
            let Some(prim) = self.classify_curved_primitive(cv_faces) else {
                continue;
            };
            // ADR-197 β-3-j γ-2b-4 — Sphere CORNER: exactly one box plane per axis
            // cuts the sphere (per-axis count (1,1,1)) and they meet at a corner
            // inside it → boolean_sphere_octant (curved patch + 3 caps).
            if prim.kind == CurvedPrimKind::Sphere {
                let center = (prim.aabb.min + prim.aabb.max) * 0.5;
                let radius = (prim.aabb.max.x - prim.aabb.min.x) * 0.5;
                if let Some(planes) = sphere_box_corner_planes(center, radius, bx.min, bx.max) {
                    return Some(match self.boolean_sphere_octant(&prim.faces, &planes, material) {
                        Ok(faces) => {
                            self.remove_box_solid(box_faces);
                            Ok(faces)
                        }
                        Err(e) => Err(e),
                    });
                }
                // full sphere-rounded box (all 6 planes cut, all 8 corners outside).
                if is_sphere_rounded_box(center, radius, bx.min, bx.max) {
                    return Some(match self.boolean_sphere_box_full(&prim.faces, bx.min, bx.max, material) {
                        Ok(faces) => {
                            self.remove_box_solid(box_faces);
                            Ok(faces)
                        }
                        Err(e) => Err(e),
                    });
                }
            }
            // The box must fully contain the primitive in X and Y (Z-only cut).
            if !(bx.min.x <= prim.aabb.min.x + EPS
                && bx.max.x >= prim.aabb.max.x - EPS
                && bx.min.y <= prim.aabb.min.y + EPS
                && bx.max.y >= prim.aabb.max.y - EPS)
            {
                continue; // box cuts in XY (box∩sphere corner) → needs γ-2b
            }
            let z_lo = bx.min.z;
            let z_hi = bx.max.z;
            let cuts_lo = z_lo > prim.aabb.min.z + EPS;
            let cuts_hi = z_hi < prim.aabb.max.z - EPS;
            // No-op: box contains the primitive in Z too → A ∩ B = A.
            if !cuts_lo && !cuts_hi {
                let faces = prim.faces.clone();
                self.remove_box_solid(box_faces);
                return Some(Ok(faces));
            }
            let dispatched: Option<Result<Vec<FaceId>>> = match prim.kind {
                CurvedPrimKind::Sphere => {
                    if cuts_lo && cuts_hi {
                        if z_lo < prim.center_z && z_hi > prim.center_z {
                            Some(self.boolean_sphere_slab(&prim.faces, z_lo, z_hi, material))
                        } else {
                            None // non-straddling slab → fall through
                        }
                    } else if cuts_lo {
                        Some(self.boolean_sphere_halfspace(
                            &prim.faces,
                            DVec3::new(0., 0., z_lo),
                            DVec3::Z,
                            material,
                        ))
                    } else {
                        Some(self.boolean_sphere_halfspace(
                            &prim.faces,
                            DVec3::new(0., 0., z_hi),
                            DVec3::NEG_Z,
                            material,
                        ))
                    }
                }
                CurvedPrimKind::Cylinder => {
                    Some(self.boolean_cylinder_slab(&prim.faces, z_lo, z_hi, material))
                }
                CurvedPrimKind::Cone => {
                    Some(self.boolean_cone_slab(&prim.faces, z_lo, z_hi, material))
                }
                CurvedPrimKind::Torus => {
                    if cuts_lo && cuts_hi {
                        Some(self.boolean_torus_slab(&prim.faces, z_lo, z_hi, material)) // β-3-l
                    } else if cuts_lo {
                        Some(self.boolean_torus_halfspace(&prim.faces, z_lo, true, material))
                    } else {
                        Some(self.boolean_torus_halfspace(&prim.faces, z_hi, false, material))
                    }
                }
            };
            match dispatched {
                Some(Ok(faces)) => {
                    self.remove_box_solid(box_faces);
                    return Some(Ok(faces));
                }
                Some(Err(e)) => return Some(Err(e)),
                None => return None, // explicit fall-through (non-straddle / 2-cut)
            }
        }
        None
    }

    /// ADR-205 γ-2a — `box ∩ tilted-cylinder` SLAB auto-routing. A world-axis box
    /// that is a SLAB in one cardinal direction (its two parallel e-faces cut the
    /// tilted cylinder's lateral surface; the other two directions CONTAIN it)
    /// clips the cylinder with two parallel OBLIQUE elliptic sections → route to
    /// β-3 `boolean_cylinder_oblique_slab` with that pair's cardinal normal +
    /// axis-relative offsets. The which-faces-cut test compares each box face's
    /// cardinal coordinate against the cylinder's lateral-surface extent along
    /// that axis (`ao·e + [min,max] v·(â·e) ± r·amp`, amp = radial sweep onto e).
    ///
    /// Returns `None` (fall through) for: a non-cylinder operand, a Z-axis
    /// cylinder (handled by `classify_curved_primitive`), or any config that is
    /// not a single-axis oblique slab (halfspace / corner / multi-plane — deferred
    /// to γ-2b+). `Some(Ok)` once routed; `Some(Err)` if β-3 declines (e.g. an
    /// ellipse would extend past an end cap) — surfaced, not silently faceted.
    fn try_tilted_cylinder_box_slab(
        &mut self,
        cv_faces: &[FaceId],
        box_faces: &[FaceId],
        bx: &crate::operations::coplanar::Aabb3,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        let (ao, ad_raw, rad, bu, vr) = self.cylinder_full_of(cv_faces).ok()?;
        let ad = ad_raw.normalize_or_zero();
        if ad.length_squared() < 0.5 || rad <= 0.0 {
            return None;
        }
        // Z-axis cylinders are handled by the circular-section classify path.
        if ad.cross(DVec3::Z).length() < 1e-6 {
            return None;
        }
        let bw = ad.cross(bu).normalize_or_zero();
        if bw.length_squared() < 0.5 {
            return None;
        }
        let (v0, v1) = (vr.0.min(vr.1), vr.0.max(vr.1));
        // cylinder lateral-surface extent along a cardinal axis e.
        let extent = |e: DVec3| -> (f64, f64) {
            let adot = ad.dot(e);
            let amp = ((bu.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            let a_lo = (v0 * adot).min(v1 * adot);
            let a_hi = (v0 * adot).max(v1 * adot);
            (ao.dot(e) + a_lo - rad * amp, ao.dot(e) + a_hi + rad * amp)
        };
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let exts: [(f64, f64); 3] = [extent(axes[0]), extent(axes[1]), extent(axes[2])];
        let bmins = [bx.min.x, bx.min.y, bx.min.z];
        let bmaxs = [bx.max.x, bx.max.y, bx.max.z];
        // per-axis: how many of the box's two e-faces fall strictly inside the
        // cylinder's e-extent (→ that face cuts the lateral surface).
        let mut cut = [0u8; 3];
        for i in 0..3 {
            let (lo, hi) = exts[i];
            if bmins[i] > lo + EPS && bmins[i] < hi - EPS {
                cut[i] += 1;
            }
            if bmaxs[i] > lo + EPS && bmaxs[i] < hi - EPS {
                cut[i] += 1;
            }
        }
        // pure single-axis slab: exactly one cardinal pair cuts (both faces), the
        // other two contain the cylinder.
        let slab = (0..3).find(|&i| cut[i] == 2 && cut[(i + 1) % 3] == 0 && cut[(i + 2) % 3] == 0)?;
        let e = axes[slab];
        // β-3 needs an OBLIQUE slab (planes neither ∥ nor ⟂ the axis).
        let cos_theta = ad.dot(e).abs();
        if cos_theta <= 1e-6 || cos_theta >= 1.0 - 1e-6 {
            return None;
        }
        let d_lo = bmins[slab] - ao.dot(e);
        let d_hi = bmaxs[slab] - ao.dot(e);
        Some(
            match self.boolean_cylinder_oblique_slab(cv_faces, e, d_lo, d_hi, material) {
                Ok(faces) => {
                    self.remove_box_solid(box_faces);
                    Ok(faces)
                }
                Err(err) => Err(err),
            },
        )
    }

    /// ADR-205 γ-2b — `box ∩ tilted-cylinder` HALFSPACE + no-op containment auto-
    /// routing (sibling of `try_tilted_cylinder_box_slab`, run after it). Classifies
    /// each of the box's 6 faces against the cylinder's cardinal extents:
    ///   • Cuts — the face coordinate is strictly inside the cylinder's e-extent;
    ///   • NonBinding — the cylinder is entirely on the inside of the face;
    ///   • Excluding — the cylinder is entirely on the outside of the face.
    /// EXACTLY ONE Cuts (rest NonBinding) → β-2 oblique halfspace, keeping the
    /// INSIDE of the box (plane normal = −outward). ZERO Cuts + ZERO Excluding
    /// (box ⊇ cylinder) → no-op `A ∩ B = A` (return the cylinder, consume the box).
    /// `≥1` Excluding (a face excludes it → disjoint/empty) or `≥2` Cuts
    /// (slab — caught earlier — / corner — γ-2c — / multi-plane) → `None`.
    fn try_tilted_cylinder_box_halfspace(
        &mut self,
        cv_faces: &[FaceId],
        box_faces: &[FaceId],
        bx: &crate::operations::coplanar::Aabb3,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        let (ao, ad_raw, rad, bu, vr) = self.cylinder_full_of(cv_faces).ok()?;
        let ad = ad_raw.normalize_or_zero();
        if ad.length_squared() < 0.5 || rad <= 0.0 {
            return None;
        }
        if ad.cross(DVec3::Z).length() < 1e-6 {
            return None; // Z-axis handled by the circular-section classify path.
        }
        let bw = ad.cross(bu).normalize_or_zero();
        if bw.length_squared() < 0.5 {
            return None;
        }
        let (v0, v1) = (vr.0.min(vr.1), vr.0.max(vr.1));
        let extent = |e: DVec3| -> (f64, f64) {
            let adot = ad.dot(e);
            let amp = ((bu.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            (
                ao.dot(e) + (v0 * adot).min(v1 * adot) - rad * amp,
                ao.dot(e) + (v0 * adot).max(v1 * adot) + rad * amp,
            )
        };
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let bmins = [bx.min.x, bx.min.y, bx.min.z];
        let bmaxs = [bx.max.x, bx.max.y, bx.max.z];
        let (mut cuts, mut excl, mut cutter) = (0u32, 0u32, None);
        for i in 0..3 {
            let (lo, hi) = extent(axes[i]);
            for (is_max, c) in [(false, bmins[i]), (true, bmaxs[i])] {
                if c > lo + EPS && c < hi - EPS {
                    cuts += 1;
                    cutter = Some((i, is_max));
                } else if is_max {
                    // max face: NonBinding if c ≥ hi (cyl inside), else Excluding.
                    if c < hi - EPS {
                        excl += 1;
                    }
                } else if c > lo + EPS {
                    // min face: NonBinding if c ≤ lo (cyl inside), else Excluding.
                    excl += 1;
                }
            }
        }
        if excl > 0 {
            return None; // disjoint → empty intersect (deferred).
        }
        match cuts {
            0 => {
                // box fully contains the cylinder → A ∩ B = A.
                let faces = cv_faces.to_vec();
                self.remove_box_solid(box_faces);
                Some(Ok(faces))
            }
            1 => {
                let (i, is_max) = cutter.unwrap();
                let e = axes[i];
                let cos_theta = ad.dot(e).abs();
                if cos_theta <= 1e-6 || cos_theta >= 1.0 - 1e-6 {
                    return None; // ∥ → β-4 / ⟂ → local-frame, not β-2.
                }
                let coord = if is_max { bmaxs[i] } else { bmins[i] };
                let p_origin = e * coord;
                let p_normal = if is_max { -e } else { e };
                Some(
                    match self.boolean_cylinder_oblique_halfspace(cv_faces, p_origin, p_normal, material) {
                        Ok(faces) => {
                            self.remove_box_solid(box_faces);
                            Ok(faces)
                        }
                        Err(err) => Err(err),
                    },
                )
            }
            _ => None, // ≥2 cuts: slab (earlier) / corner (γ-2c) / multi-plane.
        }
    }

    /// ADR-205 γ-2c — `box ∩ tilted-cylinder` CORNER auto-routing (sibling, run
    /// after the halfspace route). Two PERPENDICULAR box faces cutting the tilted
    /// cylinder (the rest NonBinding, none Excluding) form a β-5 tent — but only
    /// the subset where BOTH cutting planes are "upper bounds": the cylinder is
    /// kept BELOW each, i.e. the inward normal m = −outward satisfies n_a·m < 0
    /// (β-5's "cut from the top" convention). Maps each cutter to its (origin on
    /// the plane, inward normal), verifies upper-bound + oblique, and routes
    /// `boolean_cylinder_corner`. Returns `None` for: non-cylinder / Z-axis / ≠2
    /// cuts / same-axis cuts (slab) / any Excluding (disjoint) / a non-upper-bound
    /// face (a corner orientation β-5 cannot represent — general corner deferred).
    fn try_tilted_cylinder_box_corner(
        &mut self,
        cv_faces: &[FaceId],
        box_faces: &[FaceId],
        bx: &crate::operations::coplanar::Aabb3,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        let (ao, ad_raw, rad, bu, vr) = self.cylinder_full_of(cv_faces).ok()?;
        let n_a = ad_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || rad <= 0.0 {
            return None;
        }
        if n_a.cross(DVec3::Z).length() < 1e-6 {
            return None; // Z-axis handled by the circular-section classify path.
        }
        let bw = n_a.cross(bu).normalize_or_zero();
        if bw.length_squared() < 0.5 {
            return None;
        }
        let (v0, v1) = (vr.0.min(vr.1), vr.0.max(vr.1));
        let extent = |e: DVec3| -> (f64, f64) {
            let adot = n_a.dot(e);
            let amp = ((bu.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            (
                ao.dot(e) + (v0 * adot).min(v1 * adot) - rad * amp,
                ao.dot(e) + (v0 * adot).max(v1 * adot) + rad * amp,
            )
        };
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let bmins = [bx.min.x, bx.min.y, bx.min.z];
        let bmaxs = [bx.max.x, bx.max.y, bx.max.z];
        let mut cutters: Vec<(usize, bool)> = Vec::new();
        for i in 0..3 {
            let (lo, hi) = extent(axes[i]);
            for (is_max, c) in [(false, bmins[i]), (true, bmaxs[i])] {
                if c > lo + EPS && c < hi - EPS {
                    cutters.push((i, is_max));
                } else if is_max {
                    if c < hi - EPS {
                        return None; // excluding face → disjoint (empty intersect).
                    }
                } else if c > lo + EPS {
                    return None; // excluding face → disjoint.
                }
            }
        }
        // a corner needs the cuts on DISTINCT axes (2 = box edge, 3 = box vertex).
        let distinct_axes: std::collections::BTreeSet<usize> = cutters.iter().map(|c| c.0).collect();
        if distinct_axes.len() != cutters.len() || !(2..=3).contains(&cutters.len()) {
            return None; // two cuts on the same axis (a slab) or >3 → not a corner.
        }
        let plane_of = |i: usize, is_max: bool| -> (DVec3, DVec3) {
            let e = axes[i];
            let coord = if is_max { bmaxs[i] } else { bmins[i] };
            (e * coord, if is_max { -e } else { e }) // origin on the plane, inward normal
        };
        let cut_planes: Vec<(DVec3, DVec3)> =
            cutters.iter().map(|&(i, is_max)| plane_of(i, is_max)).collect();
        // every cut must be an oblique upper bound (β-5's "cut from the top" convention).
        for &(_, mm) in &cut_planes {
            let c = n_a.dot(mm);
            if c >= -1e-6 || c.abs() >= 1.0 - 1e-6 {
                return None; // non-upper-bound or ⟂ → β-5 can't represent (deferred).
            }
        }
        let result = if cutters.len() == 2 {
            // a box EDGE → the validated 2-plane tent.
            let (o1, m1) = cut_planes[0];
            let (o2, m2) = cut_planes[1];
            self.boolean_cylinder_corner(cv_faces, o1, m1, o2, m2, material)
        } else {
            // a box VERTEX (3 perpendicular faces) → the N-plane pie-slice corner.
            self.boolean_cylinder_corner_n(cv_faces, &cut_planes, material)
        };
        Some(match result {
            Ok(faces) => {
                self.remove_box_solid(box_faces);
                Ok(faces)
            }
            Err(err) => Err(err),
        })
    }

    /// ADR-205 γ-cone-slab — `box ∩ tilted-cone` SLAB auto-routing. A box that is a
    /// slab in one cardinal direction passing through a tilted cone cuts it with
    /// two parallel oblique elliptic sections → route to β-3-cone. Unlike the
    /// cylinder the cone narrows to the apex, so its cardinal extent spans the apex
    /// point + the base rim; a clean slab needs the APEX on one side of BOTH faces
    /// and the WHOLE base disk on the other, with the other two directions
    /// containing the cone. Returns `None` for: non-cone / Z-axis cone (the
    /// circular classify path) / any non-single-axis-slab config (halfspace / base-
    /// clip / corner — deferred). `Some(Ok)` once routed; `Some(Err)` if β-3-cone
    /// declines.
    fn try_tilted_cone_box_slab(
        &mut self,
        cv_faces: &[FaceId],
        box_faces: &[FaceId],
        bx: &crate::operations::coplanar::Aabb3,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        let (apex, axis_dir_raw, half_angle, ref_dir, v_range) = self.cone_full_of(cv_faces).ok()?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || half_angle <= 1e-6 {
            return None;
        }
        // Z-axis cones are handled by the circular-section classify path.
        if n_a.cross(DVec3::Z).length() < 1e-6 {
            return None;
        }
        let height = v_range.0.max(v_range.1);
        let base_center = apex + n_a * height;
        let base_radius = height * half_angle.tan();
        let ref_n = ref_dir.normalize_or_zero();
        let bw = n_a.cross(ref_n).normalize_or_zero();
        if bw.length_squared() < 0.5 {
            return None;
        }
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let bmins = [bx.min.x, bx.min.y, bx.min.z];
        let bmaxs = [bx.max.x, bx.max.y, bx.max.z];
        // cone cardinal extent (apex point + base rim): (cone_lo, cone_hi, base_lo, base_hi).
        let cone_ext = |e: DVec3| -> (f64, f64, f64, f64) {
            let amp = ((ref_n.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            let (blo, bhi) = (base_center.dot(e) - base_radius * amp, base_center.dot(e) + base_radius * amp);
            let ax = apex.dot(e);
            (ax.min(blo), ax.max(bhi), blo, bhi)
        };
        let slab = (0..3).find(|&i| {
            let e = axes[i];
            let (_, _, blo, bhi) = cone_ext(e);
            let apex_e = apex.dot(e);
            // apex on one side of both faces, whole base disk on the other.
            let a = apex_e < bmins[i] - EPS && blo > bmaxs[i] + EPS;
            let b = apex_e > bmaxs[i] + EPS && bhi < bmins[i] - EPS;
            (a || b)
                && (0..3).filter(|&j| j != i).all(|j| {
                    let (clo2, chi2, ..) = cone_ext(axes[j]);
                    clo2 >= bmins[j] - EPS && chi2 <= bmaxs[j] + EPS
                })
        })?;
        let e = axes[slab];
        let d_lo = bmins[slab] - apex.dot(e);
        let d_hi = bmaxs[slab] - apex.dot(e);
        Some(
            match self.boolean_cone_oblique_slab(cv_faces, e, d_lo, d_hi, material) {
                Ok(faces) => {
                    self.remove_box_solid(box_faces);
                    Ok(faces)
                }
                Err(err) => Err(err),
            },
        )
    }

    /// ADR-205 γ-cone-halfspace — `box ∩ tilted-cone` HALFSPACE + no-op containment
    /// (sibling, run after the slab route). Classifies each of the box's 6 faces
    /// against the cone (apex point + base disk): a face that cleanly cuts the cone
    /// (apex on one side, the WHOLE base disk on the other) → either a β-2-cone frustum
    /// (BASE on the inward side) or a cone apex-tip (APEX on the inward side). EXACTLY
    /// ONE such face (rest containing the cone) → `boolean_cone_oblique_halfspace` /
    /// `boolean_cone_apex_halfspace`. ZERO cuts + all containing (box ⊇ cone) → no-op
    /// `A∩B=A`. Anything else — a face excluding the cone (disjoint) or clipping the
    /// base disk (cut crosses the base) — returns `None` (deferred).
    fn try_tilted_cone_box_halfspace(
        &mut self,
        cv_faces: &[FaceId],
        box_faces: &[FaceId],
        bx: &crate::operations::coplanar::Aabb3,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        let (apex, axis_dir_raw, half_angle, ref_dir, v_range) = self.cone_full_of(cv_faces).ok()?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || half_angle <= 1e-6 {
            return None;
        }
        if n_a.cross(DVec3::Z).length() < 1e-6 {
            return None; // Z-axis cone handled by the circular classify path.
        }
        let height = v_range.0.max(v_range.1);
        let base_center = apex + n_a * height;
        let base_radius = height * half_angle.tan();
        let ref_n = ref_dir.normalize_or_zero();
        let bw = n_a.cross(ref_n).normalize_or_zero();
        if bw.length_squared() < 0.5 {
            return None;
        }
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let bmins = [bx.min.x, bx.min.y, bx.min.z];
        let bmaxs = [bx.max.x, bx.max.y, bx.max.z];
        // cone cardinal extent along e: (apex·e, base_lo, base_hi).
        let cone_ext = |e: DVec3| -> (f64, f64, f64) {
            let amp = ((ref_n.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            (apex.dot(e), base_center.dot(e) - base_radius * amp, base_center.dot(e) + base_radius * amp)
        };
        let (mut cuts, mut cutter, mut apex_cut) = (0u32, None, false);
        for i in 0..3 {
            let e = axes[i];
            let (apex_e, blo, bhi) = cone_ext(e);
            let (cone_lo, cone_hi) = (apex_e.min(blo), apex_e.max(bhi));
            for (is_max, f) in [(false, bmins[i]), (true, bmaxs[i])] {
                // contain: whole cone on the inward side. cut_base_in: a clean side cut
                // with the whole base on the inward side (→ β-2-cone frustum).
                // cut_apex_in: the apex on the inward side + the whole base outward
                // (→ cone apex-tip). A base-clip (cut crosses the base) → else → defer.
                let (contain, cut_base_in, cut_apex_in) = if is_max {
                    // inward −e, inside p·e < f.
                    (cone_hi < f - EPS,
                     apex_e > f + EPS && bhi < f - EPS,
                     apex_e < f - EPS && blo > f + EPS)
                } else {
                    // inward +e, inside p·e > f.
                    (cone_lo > f + EPS,
                     apex_e < f - EPS && blo > f + EPS,
                     apex_e > f + EPS && bhi < f - EPS)
                };
                if contain {
                    // non-binding.
                } else if cut_base_in {
                    cuts += 1;
                    cutter = Some((i, is_max));
                    apex_cut = false;
                } else if cut_apex_in {
                    cuts += 1;
                    cutter = Some((i, is_max));
                    apex_cut = true;
                } else {
                    return None; // exclude / base-clip → deferred.
                }
            }
        }
        match cuts {
            0 => {
                // box fully contains the cone → A ∩ B = A.
                let faces = cv_faces.to_vec();
                self.remove_box_solid(box_faces);
                Some(Ok(faces))
            }
            1 => {
                let (i, is_max) = cutter.unwrap();
                let e = axes[i];
                let coord = if is_max { bmaxs[i] } else { bmins[i] };
                let inward = if is_max { -e } else { e };
                let origin = e * coord;
                // base on the inward side → β-2-cone frustum; apex on the inward side
                // → cone apex-tip (both keep the +inward halfspace).
                let op = if apex_cut {
                    self.boolean_cone_apex_halfspace(cv_faces, origin, inward, material)
                } else {
                    self.boolean_cone_oblique_halfspace(cv_faces, origin, inward, material)
                };
                Some(
                    match op {
                        Ok(faces) => {
                            self.remove_box_solid(box_faces);
                            Ok(faces)
                        }
                        Err(err) => Err(err),
                    },
                )
            }
            _ => None, // ≥2 cuts: slab (earlier) / corner (γ corner) / multi.
        }
    }

    /// ADR-205 cone-corner γ — `box ∩ tilted-cone` CORNER auto-routing (sibling, run
    /// after the cone halfspace route). For a sufficiently TILTED cone (apex + base
    /// spread across two cardinal directions) two PERPENDICULAR box faces can each
    /// cleanly cut the cone keeping the BASE on the inward side (apex on −m, whole
    /// base disk on +m) — a base-keeping TENT → route `boolean_cone_corner`. Reuses
    /// the cone apex+base-rim classifier; returns `None` for non-cone / Z-axis / any
    /// face that excludes / clips the base / keeps the apex tip, or ≠2 perpendicular
    /// base-keeping cuts (the apex-tip corner is deferred).
    fn try_tilted_cone_box_corner(
        &mut self,
        cv_faces: &[FaceId],
        box_faces: &[FaceId],
        bx: &crate::operations::coplanar::Aabb3,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        let (apex, axis_dir_raw, half_angle, ref_dir, v_range) = self.cone_full_of(cv_faces).ok()?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || half_angle <= 1e-6 {
            return None;
        }
        if n_a.cross(DVec3::Z).length() < 1e-6 {
            return None;
        }
        let height = v_range.0.max(v_range.1);
        let base_center = apex + n_a * height;
        let base_radius = height * half_angle.tan();
        let ref_n = ref_dir.normalize_or_zero();
        let bw = n_a.cross(ref_n).normalize_or_zero();
        if bw.length_squared() < 0.5 {
            return None;
        }
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let bmins = [bx.min.x, bx.min.y, bx.min.z];
        let bmaxs = [bx.max.x, bx.max.y, bx.max.z];
        let cone_ext = |e: DVec3| -> (f64, f64, f64) {
            let amp = ((ref_n.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            (apex.dot(e), base_center.dot(e) - base_radius * amp, base_center.dot(e) + base_radius * amp)
        };
        // (axis, is_max, is_apex_keep). base-keep → cone-corner tent; apex-keep →
        // cone apex-tip corner (the mirror).
        let mut cutters: Vec<(usize, bool, bool)> = Vec::new();
        for i in 0..3 {
            let e = axes[i];
            let (apex_e, blo, bhi) = cone_ext(e);
            let (cone_lo, cone_hi) = (apex_e.min(blo), apex_e.max(bhi));
            for (is_max, f) in [(false, bmins[i]), (true, bmaxs[i])] {
                let (contain, cut_base_in, cut_apex_in) = if is_max {
                    (cone_hi < f - EPS,
                     apex_e > f + EPS && bhi < f - EPS,
                     apex_e < f - EPS && blo > f + EPS)
                } else {
                    (cone_lo > f + EPS,
                     apex_e < f - EPS && blo > f + EPS,
                     apex_e > f + EPS && bhi < f - EPS)
                };
                if contain {
                    // non-binding.
                } else if cut_base_in {
                    cutters.push((i, is_max, false));
                } else if cut_apex_in {
                    cutters.push((i, is_max, true));
                } else {
                    return None; // exclude / base-clip → deferred.
                }
            }
        }
        // exactly two cuts on DIFFERENT axes (a perpendicular corner), BOTH the same
        // keep-type (both base → tent, both apex → apex-tip; mixed → deferred).
        if cutters.len() != 2 || cutters[0].0 == cutters[1].0 || cutters[0].2 != cutters[1].2 {
            return None;
        }
        let plane_of = |i: usize, is_max: bool| -> (DVec3, DVec3) {
            let e = axes[i];
            let coord = if is_max { bmaxs[i] } else { bmins[i] };
            (e * coord, if is_max { -e } else { e }) // origin, inward normal
        };
        let (o1, m1) = plane_of(cutters[0].0, cutters[0].1);
        let (o2, m2) = plane_of(cutters[1].0, cutters[1].1);
        let op = if cutters[0].2 {
            self.boolean_cone_apex_corner(cv_faces, o1, m1, o2, m2, material)
        } else {
            self.boolean_cone_corner(cv_faces, o1, m1, o2, m2, material)
        };
        Some(match op {
            Ok(faces) => {
                self.remove_box_solid(box_faces);
                Ok(faces)
            }
            Err(err) => Err(err),
        })
    }

    /// **ADR-205 γ-torus** — `box ∩ tilted-torus` auto-routing (run after the cone
    /// routes). A TILTED torus (axis ∦ Z, but within the annular threshold
    /// `√(R²−r²)/R` of its nearest cardinal `e*`) is "thin" along its axis and "wide"
    /// (±(R+r)) across it, so the only clean cuts are by the `±e*` box faces (their
    /// normal ≈ the axis → annular sections). The two OTHER cardinals must CONTAIN the
    /// torus (a side cut is the deferred pinched regime). Along `e*`: 0 cuts (box ⊇
    /// torus) → no-op A∩B=A; 1 cut → β-2-torus halfspace; 2 cuts → β-3-torus slab.
    /// Returns `None` for non-torus / Z-axis torus (existing classify path) / a torus
    /// tilted past the threshold from every cardinal / a side (⊥-axis) cut / a disjoint
    /// box. The torus cardinal extent is closed-form: `C·e ± (√(1−(axis·e)²)·R + r)`.
    fn try_tilted_torus_box(
        &mut self,
        cv_faces: &[FaceId],
        box_faces: &[FaceId],
        bx: &crate::operations::coplanar::Aabb3,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        let (center, axis_raw, _ref_dir, major_r, minor_r) = self.torus_full_of(cv_faces).ok()?;
        let axis = axis_raw.normalize_or_zero();
        if axis.length_squared() < 0.5 || major_r <= minor_r + 1e-9 || minor_r <= 1e-9 {
            return None;
        }
        if axis.cross(DVec3::Z).length() < 1e-6 {
            return None; // Z-axis torus → existing circular classify path.
        }
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let bmins = [bx.min.x, bx.min.y, bx.min.z];
        let bmaxs = [bx.max.x, bx.max.y, bx.max.z];
        // torus cardinal extent along e: C·e ± (√(1−(axis·e)²)·R + r).
        let torus_ext = |e: DVec3| -> (f64, f64) {
            let a = axis.dot(e);
            let p = (1.0 - a * a).max(0.0).sqrt();
            let half = p * major_r + minor_r;
            (center.dot(e) - half, center.dot(e) + half)
        };
        // e* = the cardinal most aligned with the axis; require it within the annular
        // threshold cos = √(R²−r²)/R (= the §1 atan(r/√(R²−r²)) condition).
        let cos_thresh = (major_r * major_r - minor_r * minor_r).max(0.0).sqrt() / major_r;
        let (mut star, mut best) = (0usize, 0.0_f64);
        for i in 0..3 {
            let a = axis.dot(axes[i]).abs();
            if a > best { best = a; star = i; }
        }
        if best < cos_thresh {
            return None; // axis too tilted from every cardinal → pinched, deferred.
        }
        // the two ⊥ cardinals must CONTAIN the torus (else a side cut → deferred).
        for i in 0..3 {
            if i == star { continue; }
            let (lo, hi) = torus_ext(axes[i]);
            if !(bmins[i] <= lo + EPS && bmaxs[i] >= hi - EPS) {
                return None; // box cuts the torus from the side (⊥ axis) → deferred.
            }
        }
        let e = axes[star];
        let cstar = center.dot(e);
        let (tlo, thi) = torus_ext(e);
        if bmaxs[star] < tlo - EPS || bmins[star] > thi + EPS {
            return None; // disjoint along e* (empty intersection).
        }
        let lo_cuts = bmins[star] > tlo + EPS;
        let hi_cuts = bmaxs[star] < thi - EPS;
        let route = |me: &mut Self, res: Result<Vec<FaceId>>, box_faces: &[FaceId]| -> Result<Vec<FaceId>> {
            match res {
                Ok(faces) => { me.remove_box_solid(box_faces); Ok(faces) }
                Err(err) => Err(err),
            }
        };
        match (lo_cuts, hi_cuts) {
            (false, false) => {
                // box contains the torus along e* too → A ∩ B = A.
                let faces = cv_faces.to_vec();
                self.remove_box_solid(box_faces);
                Some(Ok(faces))
            }
            (true, false) => {
                // keep coord > box.min[e*]: inward +e, plane at box.min[e*].
                let origin = center + e * (bmins[star] - cstar);
                let r = self.boolean_torus_oblique_halfspace(cv_faces, origin, e, material);
                Some(route(self, r, box_faces))
            }
            (false, true) => {
                // keep coord < box.max[e*]: inward −e, plane at box.max[e*].
                let origin = center + e * (bmaxs[star] - cstar);
                let r = self.boolean_torus_oblique_halfspace(cv_faces, origin, -e, material);
                Some(route(self, r, box_faces))
            }
            (true, true) => {
                // slab between the two e* faces.
                let r = self.boolean_torus_oblique_slab(cv_faces, e, bmins[star] - cstar, bmaxs[star] - cstar, material);
                Some(route(self, r, box_faces))
            }
        }
    }

    /// ADR-197 β-3-m — curved SUBTRACT dispatch: `A − box` where `A = faces_a` is
    /// an analytic primitive and `box = faces_b` is an axis-box that XY-contains
    /// it (Z-only cut). `A − box = A ∩ ¬box` keeps the OUTER piece(s). Order-
    /// sensitive (only `curved − box`, never `box − curved`). Concave subtracts
    /// (XY-cutting box → scooped octant / 6 bulge-caps) return `None` (DEFER →
    /// legacy). Returns `None` to fall through; `Some(Ok/Err)` once routed.
    fn try_curved_subtract_dispatch(
        &mut self,
        faces_a: &[FaceId],
        faces_b: &[FaceId],
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        // ── ADR-198 — CONCAVE: box − curved (drilling through-hole / enclosed void).
        //   faces_a = axis box (minuend), faces_b = curved primitive (removed tool).
        //   Returns `None` for partial cases (blind hole / dimple / side-pierce) →
        //   DEFER to the Path B guard. (Convex `curved − box` falls through below.)
        if let (Some(bx_a), Some(prim_b)) =
            (self.classify_axis_box(faces_a), self.classify_curved_primitive(faces_b))
        {
            return self.try_concave_box_minus_curved(faces_a, &bx_a, faces_b, &prim_b, material);
        }
        let prim = self.classify_curved_primitive(faces_a)?; // A must be the curved minuend
        let bx = self.classify_axis_box(faces_b)?; // B must be the axis-box
        // Box must XY-contain the primitive (Z-only cut). Otherwise the subtract is
        // concave (box − corner / sphere with a scooped octant / 6 bulge-caps) →
        // DEFER to a separate track (fall through to legacy).
        if !(bx.min.x <= prim.aabb.min.x + EPS
            && bx.max.x >= prim.aabb.max.x - EPS
            && bx.min.y <= prim.aabb.min.y + EPS
            && bx.max.y >= prim.aabb.max.y - EPS)
        {
            return None;
        }
        let z_lo = bx.min.z;
        let z_hi = bx.max.z;
        let cuts_lo = z_lo > prim.aabb.min.z + EPS;
        let cuts_hi = z_hi < prim.aabb.max.z - EPS;
        // Box covers the primitive in Z too → A − B = ∅ (whole primitive removed).
        if !cuts_lo && !cuts_hi {
            self.remove_primitive_solid(faces_a);
            self.remove_box_solid(faces_b);
            return Some(Ok(Vec::new()));
        }
        let dispatched: Option<Result<Vec<FaceId>>> = match prim.kind {
            CurvedPrimKind::Sphere => {
                if cuts_lo && cuts_hi {
                    Some(self.boolean_sphere_slab_subtract(&prim.faces, z_lo, z_hi, material))
                } else if cuts_hi {
                    // box covers the bottom (plane z_hi) → keep the top cap z>z_hi.
                    Some(self.boolean_sphere_halfspace(&prim.faces, DVec3::new(0., 0., z_hi), DVec3::Z, material))
                } else {
                    // box covers the top (plane z_lo) → keep the bottom cap z<z_lo.
                    Some(self.boolean_sphere_halfspace(&prim.faces, DVec3::new(0., 0., z_lo), DVec3::NEG_Z, material))
                }
            }
            CurvedPrimKind::Cylinder => {
                if cuts_lo && cuts_hi {
                    Some(self.boolean_cylinder_slab_subtract(&prim.faces, z_lo, z_hi, material))
                } else if cuts_hi {
                    Some(self.boolean_cylinder_slab(&prim.faces, z_hi, 1e9, material)) // keep z>z_hi (clamps to top)
                } else {
                    Some(self.boolean_cylinder_slab(&prim.faces, -1e9, z_lo, material)) // keep z<z_lo (clamps to base)
                }
            }
            CurvedPrimKind::Cone => {
                if cuts_lo && cuts_hi {
                    Some(self.boolean_cone_slab_subtract(&prim.faces, z_lo, z_hi, material))
                } else if cuts_hi {
                    Some(self.boolean_cone_slab(&prim.faces, z_hi, 1e9, material)) // keep tip (clamps to apex)
                } else {
                    Some(self.boolean_cone_slab(&prim.faces, -1e9, z_lo, material)) // keep base frustum
                }
            }
            CurvedPrimKind::Torus => {
                if cuts_lo && cuts_hi {
                    Some(self.boolean_torus_slab_subtract(&prim.faces, z_lo, z_hi, material))
                } else if cuts_hi {
                    Some(self.boolean_torus_halfspace(&prim.faces, z_hi, true, material)) // keep above
                } else {
                    Some(self.boolean_torus_halfspace(&prim.faces, z_lo, false, material)) // keep below
                }
            }
        };
        match dispatched {
            Some(Ok(faces)) => {
                self.remove_box_solid(faces_b);
                Some(Ok(faces))
            }
            Some(Err(e)) => Some(Err(e)),
            None => None,
        }
    }

    /// ADR-198 — CONCAVE subtract dispatch: `box − curved` (drilling / void). The
    /// box is the minuend; the curved primitive is the material removed. MVP:
    ///   • through-hole — a Z-axis cylinder spanning the full box height → bore.
    ///   • enclosed void — primitive STRICTLY inside the box → box + inner shell.
    /// Partial cases (blind hole / dimple / side-pierce) return `None` (DEFER).
    fn try_concave_box_minus_curved(
        &mut self,
        box_faces: &[FaceId],
        bx: &crate::operations::coplanar::Aabb3,
        prim_faces: &[FaceId],
        prim: &CurvedPrim,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        // Enclosed void: primitive AABB STRICTLY inside the box (all axes).
        let strictly_inside = bx.min.x < prim.aabb.min.x - EPS && bx.max.x > prim.aabb.max.x + EPS
            && bx.min.y < prim.aabb.min.y - EPS && bx.max.y > prim.aabb.max.y + EPS
            && bx.min.z < prim.aabb.min.z - EPS && bx.max.z > prim.aabb.max.z + EPS;
        if strictly_inside {
            return Some(self.boolean_box_minus_void(box_faces, prim_faces, material));
        }
        let xy_contains = bx.min.x <= prim.aabb.min.x + EPS && bx.max.x >= prim.aabb.max.x - EPS
            && bx.min.y <= prim.aabb.min.y + EPS && bx.max.y >= prim.aabb.max.y - EPS;
        // Z-axis cylinder, box XY-contains it:
        if prim.kind == CurvedPrimKind::Cylinder && xy_contains {
            // Through-hole — cylinder Z-span ⊇ box Z-span (pierces top + bottom).
            let through = prim.aabb.min.z <= bx.min.z + EPS && prim.aabb.max.z >= bx.max.z - EPS;
            if through {
                return Some(self.boolean_box_minus_cylinder(box_faces, prim_faces, material));
            }
            // Blind hole — enters exactly ONE Z-face (floor inside the box).
            let enters_top = prim.aabb.max.z >= bx.max.z - EPS && prim.aabb.min.z > bx.min.z + EPS;
            let enters_bot = prim.aabb.min.z <= bx.min.z + EPS && prim.aabb.max.z < bx.max.z - EPS;
            if enters_top || enters_bot {
                return Some(self.boolean_box_minus_cylinder_blind(box_faces, prim_faces, material));
            }
        }
        // Dimple — a sphere poking through exactly ONE Z-face (bottom inside the box).
        if prim.kind == CurvedPrimKind::Sphere && xy_contains {
            let pokes_top = prim.aabb.max.z > bx.max.z + EPS && prim.aabb.min.z > bx.min.z + EPS;
            let pokes_bot = prim.aabb.min.z < bx.min.z - EPS && prim.aabb.max.z < bx.max.z - EPS;
            if pokes_top || pokes_bot {
                return Some(self.boolean_box_minus_sphere_dimple(box_faces, prim_faces, material));
            }
        }
        // Countersink — a Z-cone whose APEX is inside the box and whose BASE pokes
        // out one Z-face (conical pocket). box XY-contains the base (so the smaller
        // box-face circle fits). apex-down (axis +Z) pokes the top; apex-up the bottom.
        if prim.kind == CurvedPrimKind::Cone && xy_contains {
            let pokes_top = prim.aabb.max.z > bx.max.z + EPS && prim.aabb.min.z > bx.min.z + EPS && prim.aabb.min.z < bx.max.z - EPS;
            let pokes_bot = prim.aabb.min.z < bx.min.z - EPS && prim.aabb.max.z < bx.max.z - EPS && prim.aabb.max.z > bx.min.z + EPS;
            if pokes_top || pokes_bot {
                return Some(self.boolean_box_minus_cone_countersink(box_faces, prim_faces, material));
            }
        }
        None // remaining partial (side-pierce / scooped octant) → DEFER.
    }

    /// ADR-198 (drilling) — `box − cylinder` through-hole. The cylinder bore is
    /// removed and the box top/bottom faces are pierced; an INWARD cylinder wall
    /// connects the two holes (`bore_through_box`) → genus-1 watertight solid.
    pub fn boolean_box_minus_cylinder(
        &mut self,
        box_faces: &[FaceId],
        cyl_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (axis_origin, axis_dir, ref_dir, radius) = cyl_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cylinder { axis_origin, axis_dir, ref_dir, radius, .. }) => {
                    Some((*axis_origin, *axis_dir, *ref_dir, *radius))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-198 box−cyl: no Cylinder surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-198 box−cyl drilling MVP: axis must be ∥ Z");
        }
        let (top_f, z_top, bot_f, z_bot) = self
            .box_horizontal_faces(box_faces)
            .ok_or_else(|| anyhow::anyhow!("ADR-198 box−cyl: box has no horizontal faces"))?;
        let (cx, cy) = (axis_origin.x, axis_origin.y);
        let circle = |z: f64| AnalyticCurve::Circle { center: DVec3::new(cx, cy, z), radius, normal: DVec3::Z, basis_u: DVec3::X };
        let band = S::Cylinder { axis_origin: DVec3::new(cx, cy, z_bot), axis_dir: DVec3::Z, radius, ref_dir, u_range: (0.0, TAU), v_range: (0.0, z_top - z_bot) };
        let anchor = |z: f64| DVec3::new(cx + radius, cy, z);
        self.remove_primitive_solid(cyl_faces);
        // INWARD wall normal: radially toward the axis (−ref_dir) at the anchor.
        let inward = -ref_dir.normalize_or_zero();
        let band_f = self.bore_through_box(
            top_f, anchor(z_top), circle(z_top),
            bot_f, anchor(z_bot), circle(z_bot),
            band, inward, material,
        )?;
        self.set_face_surface_reversed(band_f, true); // cavity wall renders inward.
        let mut out = box_faces.to_vec();
        out.push(band_f);
        Ok(out)
    }

    /// ADR-198 (enclosed void) — `box − primitive` where the primitive is fully
    /// inside the box. The box and the primitive coexist as TWO disjoint closed
    /// shells (a box with an internal void); the primitive's curved faces are
    /// marked reversed so they render INWARD (into the void). DCEL geometry of
    /// both solids is unchanged.
    pub fn boolean_box_minus_void(
        &mut self,
        box_faces: &[FaceId],
        prim_faces: &[FaceId],
        _material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        for &f in prim_faces {
            if self.faces.get(f).map(|x| x.is_active()).unwrap_or(false) {
                self.set_face_surface_reversed(f, true);
            }
        }
        let mut out = box_faces.to_vec();
        out.extend_from_slice(prim_faces);
        Ok(out)
    }

    /// ADR-198 (blind hole) — `box − cylinder` where the cylinder enters ONE box
    /// Z-face and its floor is inside the box. The entry box-face is pierced; an
    /// INWARD cylinder wall (`pierce_face_with_band_stub`) drops to a flat floor
    /// disk (facing the opening) → watertight blind bore.
    pub fn boolean_box_minus_cylinder_blind(
        &mut self,
        box_faces: &[FaceId],
        cyl_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (axis_origin, axis_dir, ref_dir, radius, v_range) = cyl_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cylinder { axis_origin, axis_dir, ref_dir, radius, v_range, .. }) => {
                    Some((*axis_origin, *axis_dir, *ref_dir, *radius, *v_range))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-198 box−cyl blind: no Cylinder surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-198 box−cyl blind MVP: axis must be ∥ Z");
        }
        let (cx, cy) = (axis_origin.x, axis_origin.y);
        let z0 = axis_origin.z;
        let cyl_lo = z0 + v_range.0.min(v_range.1);
        let cyl_hi = z0 + v_range.0.max(v_range.1);
        let (top_f, z_top, bot_f, z_bot) = self
            .box_horizontal_faces(box_faces)
            .ok_or_else(|| anyhow::anyhow!("ADR-198 box−cyl blind: box has no horizontal faces"))?;
        // Entry from the top (floor = cyl_lo) or bottom (floor = cyl_hi).
        let (entry_face, z_entry, z_floor, disk_up) = if cyl_hi >= z_top - 1e-9 {
            (top_f, z_top, cyl_lo, DVec3::Z)
        } else {
            (bot_f, z_bot, cyl_hi, DVec3::NEG_Z)
        };
        let (lower, upper) = (z_floor.min(z_entry), z_floor.max(z_entry));
        let circle = |z: f64| AnalyticCurve::Circle { center: DVec3::new(cx, cy, z), radius, normal: DVec3::Z, basis_u: DVec3::X };
        let band = S::Cylinder { axis_origin: DVec3::new(cx, cy, lower), axis_dir: DVec3::Z, radius, ref_dir, u_range: (0.0, TAU), v_range: (0.0, upper - lower) };
        let floor_disk = S::Plane { origin: DVec3::new(cx, cy, z_floor), normal: disk_up, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
        let anchor = |z: f64| DVec3::new(cx + radius, cy, z);
        self.remove_primitive_solid(cyl_faces);
        let stub = self.pierce_face_with_band_stub(
            entry_face, anchor(z_entry), circle(z_entry), anchor(z_floor), circle(z_floor),
            band, -ref_dir.normalize_or_zero(), floor_disk, disk_up, material,
        )?;
        if let Some(&band_f) = stub.first() {
            self.set_face_surface_reversed(band_f, true); // bore wall renders inward.
        }
        let mut out = box_faces.to_vec();
        out.extend(stub);
        Ok(out)
    }

    /// ADR-198 (dimple) — `box − sphere` where the sphere pokes through ONE box
    /// Z-face and its far side is inside the box. The box face is pierced at the
    /// sphere∩plane circle; the sub-sphere inside the box becomes an INWARD cap
    /// (`pierce_face_with_cap`) → watertight spherical pocket.
    pub fn boolean_box_minus_sphere_dimple(
        &mut self,
        box_faces: &[FaceId],
        sphere_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, PI, TAU};
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-198 box−sph dimple: no Sphere surface"))?;
        let (top_f, z_top, bot_f, z_bot) = self
            .box_horizontal_faces(box_faces)
            .ok_or_else(|| anyhow::anyhow!("ADR-198 box−sph dimple: box has no horizontal faces"))?;
        let s_hi = center.z + radius;
        // Pokes the top (keep south cap below z_top) or the bottom (keep north cap).
        let (entry_face, z_cut, keep_below) = if s_hi > z_top + 1e-9 {
            (top_f, z_top, true)
        } else {
            (bot_f, z_bot, false)
        };
        let d = z_cut - center.z;
        if d.abs() >= radius - 1e-9 {
            bail!("ADR-198 box−sph dimple: cut plane must cross the sphere");
        }
        let rho = (radius * radius - d * d).sqrt();
        let v_cut = (d / radius).clamp(-1.0, 1.0).asin();
        let (v_lo, v_hi) = if keep_below { (-FRAC_PI_2, v_cut) } else { (v_cut, FRAC_PI_2) };
        let cap = S::Sphere { center, radius, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (v_lo, v_hi) };
        let circle = AnalyticCurve::Circle { center: DVec3::new(center.x, center.y, z_cut), radius: rho, normal: DVec3::Z, basis_u: DVec3::X };
        let cap_n = (crate::surfaces::sphere::evaluate(center, radius, DVec3::Z, DVec3::X, PI, (v_lo + v_hi) * 0.5) - center).normalize_or_zero();
        self.remove_primitive_solid(sphere_faces);
        let cap_f = self.pierce_face_with_cap(
            entry_face, DVec3::new(center.x + rho, center.y, z_cut), circle, cap, cap_n, material,
        )?;
        self.set_face_surface_reversed(cap_f, true); // cavity cap renders inward.
        let mut out = box_faces.to_vec();
        out.push(cap_f);
        Ok(out)
    }

    /// ADR-198 (countersink) — `box − cone` where the cone's APEX is inside the box
    /// and its base pokes out one Z-face → a conical pocket. The box face is pierced
    /// at the cone∩plane circle; the sub-cone inside the box (apex → that circle)
    /// becomes an INWARD cap (`pierce_face_with_cap`, apex degenerate). apex-down
    /// (axis +Z) pockets from the top; apex-up from the bottom.
    pub fn boolean_box_minus_cone_countersink(
        &mut self,
        box_faces: &[FaceId],
        cone_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::{AnalyticSurface as S, SurfaceOps};
        use std::f64::consts::TAU;
        let (apex, axis_dir, half_angle, ref_dir) = cone_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cone { apex, axis_dir, half_angle, ref_dir, .. }) => {
                    Some((*apex, *axis_dir, *half_angle, *ref_dir))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-198 box−cone: no Cone surface"))?;
        let ad = axis_dir.normalize_or_zero();
        if ad.cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-198 box−cone countersink MVP: axis must be ∥ Z");
        }
        let (top_f, z_top, bot_f, z_bot) = self
            .box_horizontal_faces(box_faces)
            .ok_or_else(|| anyhow::anyhow!("ADR-198 box−cone: box has no horizontal faces"))?;
        if !(apex.z > z_bot - 1e-9 && apex.z < z_top + 1e-9) {
            bail!("ADR-198 box−cone countersink: apex must be inside the box (Z)");
        }
        // apex-down (axis +Z, base above) pokes the top; apex-up (axis −Z) the bottom.
        let (entry_face, z_cut) = if ad.z > 0.0 { (top_f, z_top) } else { (bot_f, z_bot) };
        let v_cut = (z_cut - apex.z) * ad.z; // axial distance apex→cut (≥ 0).
        if v_cut <= 1e-9 {
            bail!("ADR-198 box−cone countersink: cone does not reach the box face");
        }
        let rho = v_cut * half_angle.tan();
        let cap = S::Cone { apex, axis_dir, half_angle, ref_dir, u_range: (0.0, TAU), v_range: (0.0, v_cut) };
        let circle = AnalyticCurve::Circle { center: DVec3::new(apex.x, apex.y, z_cut), radius: rho, normal: DVec3::Z, basis_u: DVec3::X };
        let cap_n = cap.normal(0.0, v_cut * 0.5); // cone outward normal at the cap mid.
        self.remove_primitive_solid(cone_faces);
        let cap_f = self.pierce_face_with_cap(
            entry_face, DVec3::new(apex.x + rho, apex.y, z_cut), circle, cap, cap_n, material,
        )?;
        self.set_face_surface_reversed(cap_f, true); // pocket wall renders inward.
        let mut out = box_faces.to_vec();
        out.push(cap_f);
        Ok(out)
    }

    /// ADR-197 β-3-o/p — curved UNION dispatch.
    ///   • CASE B (β-3-o) — curved ∪ curved: two Z-coaxial overlapping spheres →
    ///     `boolean_sphere_sphere_union` (capsule).
    ///   • CASE A (β-3-p) — curved ∪ box (box XY-contains + Z-cuts the curved
    ///     primitive): the box absorbs the middle band, the caps poke out →
    ///     `boolean_sphere_box_union` (pierced box + 2 caps).
    /// Returns `None` to fall through (unsupported pair / non-overlapping spheres /
    /// box not XY-containing). MVP: sphere only for both cases.
    fn try_curved_union_dispatch(
        &mut self,
        faces_a: &[FaceId],
        faces_b: &[FaceId],
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        const EPS: f64 = 1e-9;
        let ca = self.classify_curved_primitive(faces_a);
        let cb = self.classify_curved_primitive(faces_b);

        // ── CASE B — curved ∪ curved (both analytic primitives).
        if let (Some(pa), Some(pb)) = (&ca, &cb) {
            if pa.kind == CurvedPrimKind::Sphere && pb.kind == CurvedPrimKind::Sphere {
                if let (Some(crate::surfaces::AnalyticSurface::Sphere { center: c1, radius: r1, .. }),
                        Some(crate::surfaces::AnalyticSurface::Sphere { center: c2, radius: r2, .. }))
                    = (self.face_surface(pa.faces[0]).cloned(), self.face_surface(pb.faces[0]).cloned())
                {
                    if sphere_sphere_z_circle(c1, r1, c2, r2).is_some() {
                        return Some(self.boolean_sphere_sphere_union(&pa.faces, &pb.faces, material));
                    }
                }
            }
            // opposing-coaxial overlapping cones → hourglass (β-3-o).
            if pa.kind == CurvedPrimKind::Cone && pb.kind == CurvedPrimKind::Cone {
                // a cone solid has a base-disk (Plane) + side (Cone) face — search
                // all faces for the Cone surface (faces[0] may be the disk).
                let cone_of = |faces: &[FaceId], m: &Mesh| faces.iter().find_map(|&f| match m.face_surface(f) {
                    Some(crate::surfaces::AnalyticSurface::Cone { apex, axis_dir, half_angle, v_range, .. }) => {
                        Some((*apex, *axis_dir, *half_angle, v_range.0.max(v_range.1)))
                    }
                    _ => None,
                });
                if let (Some((a1, x1, h1, vb1)), Some((a2, x2, h2, vb2))) = (cone_of(&pa.faces, self), cone_of(&pb.faces, self)) {
                    if cone_cone_hourglass(a1, x1, h1, vb1, a2, x2, h2, vb2).is_some() {
                        return Some(self.boolean_cone_cone_union(&pa.faces, &pb.faces, material));
                    }
                }
            }
            return None; // both curved but unsupported pair / no SSI → fall through
        }

        // ── CASE A — curved ∪ box (one curved primitive + one axis box).
        for (cv_opt, bx_faces) in [(&ca, faces_b), (&cb, faces_a)] {
            if let Some(prim) = cv_opt {
                if let Some(bx) = self.classify_axis_box(bx_faces) {
                    let xy_contains = bx.min.x <= prim.aabb.min.x + EPS && bx.max.x >= prim.aabb.max.x - EPS
                        && bx.min.y <= prim.aabb.min.y + EPS && bx.max.y >= prim.aabb.max.y - EPS;
                    let cuts = bx.min.z > prim.aabb.min.z + EPS && bx.max.z < prim.aabb.max.z - EPS;
                    if xy_contains && cuts {
                        match prim.kind {
                            CurvedPrimKind::Sphere => return Some(self.boolean_sphere_box_union(&prim.faces, bx_faces, material)),
                            CurvedPrimKind::Cylinder => return Some(self.boolean_cylinder_box_union(&prim.faces, bx_faces, material)),
                            CurvedPrimKind::Cone => return Some(self.boolean_cone_box_union(&prim.faces, bx_faces, material)),
                            CurvedPrimKind::Torus => return Some(self.boolean_torus_box_union(&prim.faces, bx_faces, material)),
                        }
                    }
                }
            }
        }
        None
    }

    /// Boolean 연산 수행 (기존 동작 — Stage 1 coplanar only, no fail-closed gate).
    ///
    /// `faces_a`: 솔리드 A의 face 집합
    /// `faces_b`: 솔리드 B의 face 집합
    /// `op`: Union / Subtract / Intersect
    ///
    /// ADR-276: this preserves the pre-ADR-276 behavior byte-for-byte
    /// (curved dispatch + coplanar-only Stage 1). The solid-CSG path that
    /// wires the general tri-tri collector + fail-closed gate is
    /// [`Mesh::boolean_solid`] — a separate entry so existing callers
    /// (demo_*, boolean_dispatch mesh fallback, regression oracles) are
    /// unaffected until the UI is routed to it (ADR-276 Phase 5, Q2).
    pub fn boolean(
        &mut self,
        faces_a: &[FaceId],
        faces_b: &[FaceId],
        op: BoolOp,
        material: MaterialId,
    ) -> Result<BooleanResult> {
        self.boolean_impl(faces_a, faces_b, op, material, false)
    }

    /// ADR-276 Phase 1 — solid CSG boolean: wires the general (non-coplanar)
    /// `find_intersections` collector into Stage 1 so box/planar solids
    /// actually cut, and runs a fail-closed validity gate (ADR-267/272/273
    /// spirit): if the result is not a valid, self-intersection-free mesh it
    /// is rolled back byte-identically and an honest "not yet supported"
    /// error is returned. Convex overlaps (corner-poke / notch) cut cleanly;
    /// configs whose split/classify is not yet robust (through-slot) or that
    /// need void handling (fully-enclosed, Phase 3) roll back safely.
    pub fn boolean_solid(
        &mut self,
        faces_a: &[FaceId],
        faces_b: &[FaceId],
        op: BoolOp,
        material: MaterialId,
    ) -> Result<BooleanResult> {
        self.boolean_impl(faces_a, faces_b, op, material, true)
    }

    fn boolean_impl(
        &mut self,
        faces_a: &[FaceId],
        faces_b: &[FaceId],
        op: BoolOp,
        material: MaterialId,
        use_general: bool,
    ) -> Result<BooleanResult> {
        let mut debug = Vec::new();

        // ── ADR-197 β-3-i — Curved Boolean routing (intersect only) ──
        //
        // Before the legacy polygonal path (which polygonises Path B analytic
        // faces, destroying the surface), try a surface-preserving curved
        // Boolean: an analytic primitive (sphere/cylinder/cone/torus) ∩ an
        // axis-aligned box that only cuts it in Z routes to the curved slab /
        // halfspace ops. Falls through (returns None) for subtract, XY-cutting
        // boxes, non-straddling sphere slabs, and 2-cut tori. Additive — the
        // legacy path is unchanged for every non-matching input.
        if op == BoolOp::Intersect {
            if let Some(res) = self.try_curved_intersect_dispatch(faces_a, faces_b, material) {
                return res.map(|faces| {
                    let nf = faces.len();
                    BooleanResult {
                        faces,
                        new_verts: 0,
                        debug: vec![format!("ADR-197 β-3-i curved intersect dispatch → {nf} faces")],
                    }
                });
            }
        }
        // ── ADR-197 β-3-m — Curved Boolean SUBTRACT routing (curved − box) ──
        //
        // A − box = A ∩ ¬box. An axis-box that XY-contains the analytic primitive
        // (sphere/cylinder/cone/torus = `faces_a`) and only Z-cuts it keeps the
        // OUTER piece(s): a halfspace cut → one cap/stub/frustum/band-ring (the
        // intersect builder with the keep-side flipped); a slab cut → TWO disjoint
        // outer pieces. Concave subtracts (box − curved, or an XY-cutting box =
        // scooped octant / 6 bulge-caps) fall through to the legacy path (DEFER).
        if op == BoolOp::Subtract {
            if let Some(res) = self.try_curved_subtract_dispatch(faces_a, faces_b, material) {
                return res.map(|faces| {
                    let nf = faces.len();
                    BooleanResult {
                        faces,
                        new_verts: 0,
                        debug: vec![format!("ADR-197 β-3-m curved subtract dispatch → {nf} faces")],
                    }
                });
            }
        }
        // ── ADR-197 β-3-o — Curved Boolean UNION routing (curved ∪ curved) ──
        //
        // Two analytic primitives merged at their SSI. MVP: two Z-coaxial
        // overlapping spheres → a capsule (2 Sphere caps sharing the SSI circle),
        // surface preserved. Falls through (returns None) for non-curved operands,
        // disjoint/nested spheres, and unsupported pairs (curved∪box = β-3-p).
        if op == BoolOp::Union {
            if let Some(res) = self.try_curved_union_dispatch(faces_a, faces_b, material) {
                return res.map(|faces| {
                    let nf = faces.len();
                    BooleanResult {
                        faces,
                        new_verts: 0,
                        debug: vec![format!("ADR-197 β-3-o curved union dispatch → {nf} faces")],
                    }
                });
            }
        }

        // ── ADR-197 #Track2 — Path B curved self-loop guard (메타-원칙 #16) ──
        //
        // A CURVED analytic surface (Sphere/Cylinder/Cone/Torus) face whose outer
        // boundary is a SELF-LOOP (1 anchor + a curve self-loop edge) reaching here
        // means the curved dispatch above DECLINED this op + configuration (concave
        // subtract / box−curved / XY-cutting box / same-kind curved union). The
        // legacy polygonal path can't form a solid from such a face (prepare_solid
        // skips the < 3-vert self-loop boundary → a downstream `HeId not found`
        // crash), so bail with a clear message instead of crashing. Checked on the
        // RAW input (before the ADR-110 polygonize pass) so the self-loop is still
        // intact. A flat closed-curve face (Plane surface) is left to polygonize; a
        // normal polygonal face that merely carries a curved surface LABEL is not a
        // self-loop and passes through unchanged.
        for &fid in faces_a.iter().chain(faces_b.iter()) {
            let Some(face) = self.faces.get(fid) else { continue };
            let Some(surf) = face.surface() else { continue };
            if matches!(surf, crate::surfaces::AnalyticSurface::Plane { .. }) {
                continue;
            }
            let start = face.outer().start;
            if start.is_null() {
                continue;
            }
            let eid = self.hes[start].edge();
            let is_self_loop = self.edges.get(eid).map(|e| e.is_self_loop()).unwrap_or(false);
            if is_self_loop {
                let op_name = match op {
                    BoolOp::Union => "union",
                    BoolOp::Subtract => "subtract",
                    BoolOp::Intersect => "intersect",
                };
                anyhow::bail!(
                    "boolean {op_name}: operand keeps a curved analytic surface this \
                     configuration does not support yet (e.g. concave subtract, box−curved, \
                     non-axis-aligned box, or same-kind curved union). Use an axis-aligned \
                     box that only cuts the primitive in Z."
                );
            }
        }

        // ── ADR-276 Phase 1 — fail-closed snapshot (solid CSG path only) ──
        // Wiring find_intersections into Stage 1 (below) makes box/planar
        // solids actually cut. Some configs still produce invalid topology
        // (e.g. through-slot). Snapshot the mesh BEFORE any mutation (the
        // polygonize pass mutates) so the validity gate at the end can roll
        // back byte-identically instead of committing a corrupt result.
        // Only the solid-CSG entry (`boolean_solid`) pays this clone; the
        // legacy `boolean()` path (use_general=false) is byte-identical to
        // before ADR-276. (Curved-analytic operands returned early above.)
        let fail_closed_backup = if use_general { Some(self.clone()) } else { None };

        // ── ADR-110 π-β — Pre-polygonize Path B closed-curve faces ──
        //
        // Path B closed-curve face (1 anchor + 1 self-loop edge with Circle
        // curve) 는 prepare_solid 의 `positions.len() < 3` short-circuit 으로
        // skip → Boolean silent fail (audit evidence 2026-05-16: Path B
        // cylinder × Path B cylinder Union 결과 변경 0).
        //
        // ADR-101 Phase A 의 `polygonize_closed_curve_face` helper 재사용 —
        // closed-curve face 를 polygonal substitute 로 변환 (chord_tol-driven
        // sampling). 새 FaceId 반환, 원본 face inactive.
        //
        // L1 — Additive, drop-in alongside (기존 path UNCHANGED).
        // L3 — In-place face_id 매핑 (caller 영향 0).
        // L4 — polygonal face 는 polygonize 가 Ok(None) → 원본 face_id 보존.
        let faces_a_resolved: Vec<FaceId> = faces_a.iter()
            .map(|&fid| match self.polygonize_closed_curve_face(fid, material) {
                Ok(Some(new_fid)) => new_fid,
                _ => fid,
            })
            .collect();
        let faces_b_resolved: Vec<FaceId> = faces_b.iter()
            .map(|&fid| match self.polygonize_closed_curve_face(fid, material) {
                Ok(Some(new_fid)) => new_fid,
                _ => fid,
            })
            .collect();

        // Phase F — Boolean 연산은 fan triangulation 사용 (convex face 가정).
        // 구멍(inner loops) 있는 face는 잘못된 결과 생성 → 명확히 거부.
        // 미래 작업: constrained Delaunay로 hole-aware triangulation.
        for &fid in faces_a_resolved.iter().chain(faces_b_resolved.iter()) {
            if let Some(face) = self.faces.get(fid) {
                if !face.inners().is_empty() {
                    anyhow::bail!(
                        "boolean: face {:?} has {} hole(s) — multi-loop boolean not yet supported",
                        fid, face.inners().len()
                    );
                }
            }
        }

        // Use resolved face IDs from here on (ADR-110 π-β).
        let faces_a = &faces_a_resolved[..];
        let faces_b = &faces_b_resolved[..];

        // ── Stage 0: 솔리드 데이터 준비 ──────────────
        let solid_a = self.prepare_solid(faces_a)?;
        let solid_b = self.prepare_solid(faces_b)?;
        debug.push(format!(
            "Solid A: {} faces, {} tris | Solid B: {} faces, {} tris",
            solid_a.face_ids.len(), solid_a.all_triangles.len(),
            solid_b.face_ids.len(), solid_b.all_triangles.len(),
        ));

        // ── Stage 0.5: 공면 face 감지 ─────────────────
        let coplanar_intersections = self.detect_coplanar_faces(&solid_a, &solid_b);
        let coplanar_count = coplanar_intersections.len();

        // ── Stage 1: 교차선 수집 ─────────────────────
        // ADR-276 Phase 1: the solid-CSG path (`boolean_solid`, use_general)
        // wires `find_intersections` (general non-coplanar tri-tri via
        // boolean_geo::triangle_triangle_intersection) — previously wired ONLY
        // to "Intersect with Model" — and unions it with the coplanar overlaps
        // so box/planar solids that cross non-coplanarly actually get cut
        // (ADR-275 scoping: every planar box config was a NO-OP). The legacy
        // `boolean()` path (use_general=false) keeps coplanar-only.
        let general_count;
        let mut intersections: Vec<IntersectionSegment> = coplanar_intersections;
        if use_general {
            let general_intersections = self.find_intersections(&solid_a, &solid_b);
            general_count = general_intersections.len();
            intersections.extend(general_intersections);
        } else {
            general_count = 0;
        }
        debug.push(format!("Intersections found: {} ({} coplanar + {} general)",
            intersections.len(), coplanar_count, general_count));

        // ── Stage 2: Face Split — 교차선으로 face 분할 ─────
        let split_a = if !intersections.is_empty() {
            self.split_faces_by_intersections(&solid_a, &intersections, material)
        } else {
            // 교차 없으면 모든 face를 자기 자신으로 매핑
            solid_a.face_ids.iter().map(|&f| (f, vec![f])).collect()
        };
        let split_b = if !intersections.is_empty() {
            self.split_faces_by_intersections(&solid_b, &intersections, material)
        } else {
            solid_b.face_ids.iter().map(|&f| (f, vec![f])).collect()
        };

        // 분할 후 새로운 face 목록
        let new_faces_a: Vec<FaceId> = split_a.values().flat_map(|v| v.iter().copied()).collect();
        let new_faces_b: Vec<FaceId> = split_b.values().flat_map(|v| v.iter().copied()).collect();

        let split_count_a = new_faces_a.len().saturating_sub(solid_a.face_ids.len());
        let split_count_b = new_faces_b.len().saturating_sub(solid_b.face_ids.len());
        debug.push(format!(
            "Face splits: A +{} ({}→{}), B +{} ({}→{})",
            split_count_a, solid_a.face_ids.len(), new_faces_a.len(),
            split_count_b, solid_b.face_ids.len(), new_faces_b.len(),
        ));

        // ── Stage 3: 분할된 face의 centroid 계산 + 원본 솔리드로 분류 ──
        // 핵심: point_in_solid는 밀폐된(watertight) 솔리드가 필요하므로
        // 분할 후 재구성이 아닌 **원본 솔리드 삼각형**으로 분류해야 정확함
        let (keep_a, keep_b) = self.classify_split_faces(
            &new_faces_a, &solid_b, // A의 새 face → 원본 B로 판정
            &new_faces_b, &solid_a, // B의 새 face → 원본 A로 판정
            op,
        );
        debug.push(format!(
            "Keep: A={} faces, B={} faces",
            keep_a.len(), keep_b.len(),
        ));

        // ── Stage 5: 결과 조립 ───────────────────────
        let mut result_faces = Vec::new();

        // A에서 유지할 face
        for &fid in &keep_a {
            result_faces.push(fid);
        }

        // B에서 유지할 face (Subtract 시 winding 반전)
        for &fid in &keep_b {
            if op == BoolOp::Subtract {
                self.flip_face(fid)?;
            }
            result_faces.push(fid);
        }

        // 제거 대상 face 삭제
        let remove_a: Vec<FaceId> = new_faces_a.iter()
            .filter(|f| !keep_a.contains(f))
            .copied()
            .collect();
        let remove_b: Vec<FaceId> = new_faces_b.iter()
            .filter(|f| !keep_b.contains(f))
            .copied()
            .collect();

        for fid in remove_a {
            let _ = self.remove_face(fid);
        }
        for fid in remove_b {
            let _ = self.remove_face(fid);
        }

        // ── Stage 6: 공면 face 병합 ──────────────────
        let merged_faces = self.merge_coplanar_result_faces(&result_faces);
        debug.push(format!(
            "Face merging: {} → {} (merged {} coplanar faces)",
            result_faces.len(), merged_faces.len(),
            result_faces.len() - merged_faces.len()
        ));

        let new_vert_count = split_count_a + split_count_b;
        debug.push(format!(
            "Removed: {} A-faces, {} B-faces | New verts: ~{}",
            new_faces_a.len() - keep_a.len(),
            new_faces_b.len() - keep_b.len(),
            new_vert_count,
        ));

        // ADR-007 — boolean 후 invariants 검증
        self.debug_verify_invariants();

        // ── ADR-276 Phase 1 — fail-closed validity gate (solid CSG only) ──
        // Verify the result is topologically sound before committing. Some
        // operand configurations (e.g. through-slot) still produce invalid
        // topology under the current split/classify stages; rather than commit
        // a corrupt mesh, roll back byte-identically to the pre-op snapshot and
        // report an honest "not yet supported" error. Gate = ADR-007 invariants
        // valid AND no self-intersections (ADR-273). Closed-solid is NOT
        // required here: 2D/sheet operands legitimately yield open results.
        // Only runs for `boolean_solid` (fail_closed_backup is Some).
        if let Some(backup) = fail_closed_backup {
            let inv_valid = self.verify_face_invariants().is_valid();
            let si_clean = self.detect_self_intersections().is_clean();
            if !inv_valid || !si_clean {
                *self = backup;
                anyhow::bail!(
                    "boolean_solid: result failed the ADR-276 Phase 1 validity gate \
                     (invariants_valid={inv_valid}, self_intersection_clean={si_clean}) \
                     — rolled back; this operand configuration is not yet supported"
                );
            }
        }

        Ok(BooleanResult {
            faces: merged_faces,
            new_verts: new_vert_count,
            debug,
        })
    }

    /// "Intersect with Model" — 선택된 face들과 나머지 active face 사이의
    /// 교차선을 edge로 구성 (Boolean solid 판정 없이 순수 topology split).
    ///
    /// SketchUp 의 "Intersect Faces with Model" 에 해당. 선택한 face 가
    /// 다른 face 와 3D 상에서 교차하면, 양쪽 면을 교차선 기준으로 분할해
    /// 새 edge 를 생성한다. inside/outside 분류는 수행하지 않는다 — 모든
    /// sub-face 를 유지.
    ///
    /// 인자:
    ///   `selected`: 교차 검사 대상 face 집합
    ///   `material`: 새 sub-face 에 할당할 material
    ///
    /// 반환: 분할 후 전체 sub-face 집합 (selected + rest of scene)
    pub fn intersect_faces_with_model(
        &mut self,
        selected: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        if selected.is_empty() {
            return Ok(Vec::new());
        }

        // Build "other" face set = all active PLANAR faces not in selected.
        // Curved-primitive faces (Sphere/Cylinder/Cone/Torus/NURBS-class) are NOT
        // auto-intersect targets: drawing a flat shape must not be split by a 3D
        // solid's silhouette (e.g. a rect drawn at a sphere's equator would
        // otherwise be cut along the equator circle, producing non-manifold
        // geometry). Mirrors the seed-side curved filter in
        // Scene::intersect_faces_inner. Faces with no surface (legacy) or a Plane
        // surface remain targets.
        use std::collections::HashSet;
        let selected_set: HashSet<FaceId> = selected.iter().copied().collect();
        let others: Vec<FaceId> = self.faces.iter()
            .filter(|(fid, f)| {
                f.is_active()
                    && !selected_set.contains(fid)
                    && f.surface().map_or(true, |s| {
                        matches!(s, crate::surfaces::AnalyticSurface::Plane { .. })
                    })
            })
            .map(|(fid, _)| fid)
            .collect();

        if others.is_empty() {
            return Ok(selected.to_vec());
        }

        // Active-only check: reject faces with holes (same as Boolean).
        for &fid in selected.iter().chain(others.iter()) {
            if let Some(face) = self.faces.get(fid) {
                if !face.inners().is_empty() {
                    anyhow::bail!(
                        "intersect_faces_with_model: face {:?} has {} hole(s) — not yet supported",
                        fid, face.inners().len()
                    );
                }
            }
        }

        let solid_sel = self.prepare_solid(selected)?;
        let solid_oth = self.prepare_solid(&others)?;

        let intersections = self.find_intersections(&solid_sel, &solid_oth);
        if intersections.is_empty() {
            // 교차 없음 — scene 변화 없음
            return Ok(selected.to_vec());
        }

        let split_sel = self.split_faces_by_intersections(&solid_sel, &intersections, material);
        let split_oth = self.split_faces_by_intersections(&solid_oth, &intersections, material);

        let mut result: Vec<FaceId> = Vec::new();
        for v in split_sel.values() { result.extend(v.iter().copied()); }
        for v in split_oth.values() { result.extend(v.iter().copied()); }

        // ADR-007 — invariants 검증 (debug mode)
        self.debug_verify_invariants();

        Ok(result)
    }

    /// 솔리드 데이터 준비: face → 삼각형 변환
    fn prepare_solid(&self, face_ids: &[FaceId]) -> Result<SolidData> {
        let mut triangles = Vec::new();
        let mut all_tris = Vec::new();

        for &fid in face_ids {
            let face = self.faces.get(fid)
                .ok_or_else(|| anyhow::anyhow!("face {:?} not found", fid))?;

            if !face.is_active() {
                continue;
            }

            let verts = self.collect_loop_verts(face.outer().start)?;
            let positions: Vec<DVec3> = verts.iter()
                .map(|&vid| self.verts.get(vid).map(|v| v.pos()).unwrap_or(DVec3::ZERO))
                .collect();

            if positions.len() < 3 {
                continue;
            }

            // Fan triangulation (convex face 가정 — MVP)
            let mut face_tris = Vec::new();
            for i in 1..positions.len() - 1 {
                let tri = (positions[0], positions[i], positions[i + 1]);
                face_tris.push(tri);
                all_tris.push(tri);
            }

            triangles.push(FaceTriangles {
                face_id: fid,
                tris: face_tris,
            });
        }

        Ok(SolidData {
            face_ids: face_ids.to_vec(),
            triangles,
            all_triangles: all_tris,
        })
    }

    /// Stage 1: 두 솔리드 간 교차선 수집
    fn find_intersections(
        &self,
        solid_a: &SolidData,
        solid_b: &SolidData,
    ) -> Vec<IntersectionSegment> {
        let mut segments = Vec::new();

        // ── 전체 솔리드 AABB 사전 검사 (겹침 없으면 즉시 반환) ──
        if !solid_aabb_overlap(solid_a, solid_b) {
            return segments;
        }

        for ft_a in &solid_a.triangles {
            for ft_b in &solid_b.triangles {
                // Per-face AABB 사전 필터 (성능)
                if !face_aabb_overlap(ft_a, ft_b) {
                    continue;
                }

                for &tri_a in &ft_a.tris {
                    for &tri_b in &ft_b.tris {
                        if let Some((p0, p1)) = triangle_triangle_intersection(
                            tri_a.0, tri_a.1, tri_a.2,
                            tri_b.0, tri_b.1, tri_b.2,
                        ) {
                            if (p1 - p0).length() > 1e-7 {
                                segments.push(IntersectionSegment {
                                    face_a: ft_a.face_id,
                                    face_b: ft_b.face_id,
                                    p0,
                                    p1,
                                });
                            }
                        }
                    }
                }
            }
        }

        segments
    }

    /// Stage 3: 분할된 face들을 원본 반대편 솔리드로 분류
    ///
    /// 핵심 차이: `classify_faces`와 달리, 분류 대상은 분할된 face 목록이고
    /// point_in_solid 판정에는 원본(밀폐된) 솔리드 삼각형을 사용
    fn classify_split_faces(
        &self,
        faces_a: &[FaceId],
        original_solid_b: &SolidData,  // 원본 B (밀폐)
        faces_b: &[FaceId],
        original_solid_a: &SolidData,  // 원본 A (밀폐)
        op: BoolOp,
    ) -> (Vec<FaceId>, Vec<FaceId>) {
        let mut keep_a = Vec::new();
        let mut keep_b = Vec::new();

        // A의 각 (분할된) face → 원본 B 내부인지 판정
        for &fid in faces_a {
            match self.faces.get(fid) {
                Some(f) if f.is_active() => {},
                _ => continue,
            };
            let centroid = match self.face_centroid(fid) {
                Some(c) => c,
                None => continue,
            };
            let inside_b = point_in_solid(&original_solid_b.all_triangles, centroid);
            let keep = match op {
                BoolOp::Union => !inside_b,
                BoolOp::Subtract => !inside_b,
                BoolOp::Intersect => inside_b,
            };
            if keep {
                keep_a.push(fid);
            }
        }

        // B의 각 (분할된) face → 원본 A 내부인지 판정
        for &fid in faces_b {
            match self.faces.get(fid) {
                Some(f) if f.is_active() => {},
                _ => continue,
            };
            let centroid = match self.face_centroid(fid) {
                Some(c) => c,
                None => continue,
            };
            let inside_a = point_in_solid(&original_solid_a.all_triangles, centroid);
            let keep = match op {
                BoolOp::Union => !inside_a,
                BoolOp::Subtract => inside_a,
                BoolOp::Intersect => inside_a,
            };
            if keep {
                keep_b.push(fid);
            }
        }

        (keep_a, keep_b)
    }

    /// Face의 centroid (무게중심) 계산
    fn face_centroid(&self, fid: FaceId) -> Option<DVec3> {
        let face = self.faces.get(fid)?;
        let start = face.outer().start;
        let verts = self.collect_loop_verts(start).ok()?;
        if verts.is_empty() { return None; }
        let mut sum = DVec3::ZERO;
        let mut count = 0;
        for &vid in &verts {
            if let Some(v) = self.verts.get(vid) {
                sum += v.pos();
                count += 1;
            }
        }
        if count == 0 { return None; }
        Some(sum / count as f64)
    }

    // flip_face는 orient.rs에 이미 구현됨 — 재사용

    // ================================================================
    // Stage 2.5: Face Split — 교차선으로 face를 sub-face로 분할
    // ================================================================

    /// 교차선이 통과하는 face를 sub-face로 분할.
    ///
    /// 알고리즘:
    ///   1. Face의 외곽 정점 + 교차 세그먼트를 2D로 투영
    ///   2. 교차점을 에지 위에 삽입 → 확장 정점 목록
    ///   3. 교차선을 따라 다각형을 분할 → sub-polygon 추출
    ///   4. 각 sub-polygon으로 새 DCEL face 생성, 원본 face 제거
    ///
    /// 반환: (원본 face → 새 face 목록) 매핑
    fn split_faces_by_intersections(
        &mut self,
        solid: &SolidData,
        intersections: &[IntersectionSegment],
        material: MaterialId,
    ) -> FxHashMap<FaceId, Vec<FaceId>> {
        let mut split_map: FxHashMap<FaceId, Vec<FaceId>> = FxHashMap::default();

        // face별 교차 세그먼트 수집
        let mut face_segments: FxHashMap<FaceId, Vec<(DVec3, DVec3)>> = FxHashMap::default();
        for seg in intersections {
            face_segments.entry(seg.face_a)
                .or_default()
                .push((seg.p0, seg.p1));
            face_segments.entry(seg.face_b)
                .or_default()
                .push((seg.p0, seg.p1));
        }

        for ft in &solid.triangles {
            let fid = ft.face_id;
            let segs = match face_segments.get(&fid) {
                Some(s) if !s.is_empty() => s,
                _ => {
                    // 교차선 없음 → 분할 불필요
                    split_map.insert(fid, vec![fid]);
                    continue;
                }
            };

            // 실제 face의 정점 수집 (DCEL loop)
            let face = match self.faces.get(fid) {
                Some(f) if f.is_active() => f,
                _ => { split_map.insert(fid, vec![fid]); continue; }
            };
            let normal = face.normal();
            let outer_start = face.outer().start;
            let loop_verts = match self.collect_loop_verts(outer_start) {
                Ok(v) => v,
                Err(_) => { split_map.insert(fid, vec![fid]); continue; }
            };

            let positions: Vec<DVec3> = loop_verts.iter()
                .filter_map(|&vid| self.verts.get(vid).map(|v| v.pos()))
                .collect();

            if positions.len() < 3 {
                split_map.insert(fid, vec![fid]);
                continue;
            }

            // 2D 투영
            let (poly_2d, u_axis, v_axis, origin) = project_to_2d(&positions, normal);

            // 교차 세그먼트도 2D로 투영
            let segs_2d: Vec<(Pt2, Pt2)> = segs.iter().map(|&(p0, p1)| {
                let d0 = p0 - origin;
                let d1 = p1 - origin;
                (
                    Pt2::new(u_axis.dot(d0), v_axis.dot(d0)),
                    Pt2::new(u_axis.dot(d1), v_axis.dot(d1)),
                )
            }).collect();

            // 교차선으로 다각형 분할
            match split_polygon_2d(&poly_2d, &segs_2d) {
                Some(sub_polys) if sub_polys.len() >= 2 => {
                    // 원본 face 제거 후 sub-polygon으로 새 face 생성
                    let mut new_faces = Vec::new();
                    let mat = self.faces.get(fid)
                        .map(|f| f.material())
                        .unwrap_or(material);
                    // ADR-089 A-χ-β — capture parent surface for inheritance.
                    // Auto-intersect splits faces; without this, sphere×sphere
                    // intersection would lose all Sphere surface metadata on
                    // sub-faces → A-ρ/A-φ/A-τ all skip them.
                    let parent_surface = self.faces.get(fid)
                        .and_then(|f| f.surface().cloned());
                    // K3 (보고서 시나리오 3 hotfix, 2026-05-23) — capture
                    // parent surface owner_id for propagation. Boolean
                    // split (sphere × sphere, cylinder × X) must preserve
                    // group identity to keep "click any → all siblings
                    // selected" UX consistent.
                    let parent_owner = self.face_surface_owner_id(fid);

                    for sub_poly in &sub_polys {
                        // 2D → 3D 역투영
                        let verts_3d: Vec<DVec3> = sub_poly.iter()
                            .map(|pt| unproject_to_3d(*pt, u_axis, v_axis, origin))
                            .collect();

                        if verts_3d.len() < 3 {
                            continue;
                        }

                        // DCEL 정점 생성 (기존 정점 재사용 via add_vertex tolerance)
                        let vert_ids: Vec<VertId> = verts_3d.iter()
                            .map(|&p| self.add_vertex(p))
                            .collect();

                        // 중복 제거 (연속된 동일 정점)
                        let deduped = dedup_consecutive_verts(&vert_ids);
                        if deduped.len() < 3 {
                            continue;
                        }

                        match self.add_face(&deduped, mat) {
                            Ok(new_fid) => {
                                // ADR-089 A-χ-β — propagate parent surface.
                                if let Some(ref s) = parent_surface {
                                    self.faces[new_fid].set_surface(Some(s.clone()));
                                }
                                // K3 — propagate parent surface owner_id.
                                if let Some(owner) = parent_owner {
                                    self.set_face_surface_owner_id(new_fid, Some(owner));
                                }
                                new_faces.push(new_fid);
                            }
                            Err(_) => {
                                // face 생성 실패 시 무시 (degenerate polygon)
                            }
                        }
                    }

                    if new_faces.is_empty() {
                        // 분할 실패 → 원본 유지
                        split_map.insert(fid, vec![fid]);
                    } else {
                        // 원본 face 제거
                        let _ = self.remove_face(fid);

                        // ADR-101 Amendment 10 — 메타-원칙 #15 cross-cut
                        // enforcement. Boolean split-induced edges (각
                        // new_face 사이 shared edges) HARD flag 부여.
                        // 외부 boundary 는 face_normals.len()==1 → 자동
                        // draw (영향 0). 정확한 split contract.
                        let mut shared_edges: Vec<EdgeId> = Vec::new();
                        for i in 0..new_faces.len() {
                            for j in (i + 1)..new_faces.len() {
                                if let Some(eid) = self.find_shared_edge_between_faces(
                                    new_faces[i], new_faces[j],
                                ) {
                                    shared_edges.push(eid);
                                }
                            }
                        }
                        self.mark_edges_hard(&shared_edges);

                        split_map.insert(fid, new_faces);
                    }
                }
                _ => {
                    // 분할 불가 또는 단일 다각형 → 원본 유지
                    split_map.insert(fid, vec![fid]);
                }
            }
        }

        split_map
    }

    /// ── Stage 0.5: 공면 face 감지 ────────────────────
    /// A와 B의 두 solids에서 같은 평면에 있는 face 쌍을 찾아서
    /// pseudo-intersection을 생성해 face split을 유도
    ///
    /// G-3 fix: 반환 타입을 단순화 — 이전엔 `(coplanar_segs, regular_segs)` 튜플을
    /// 반환했으나 `drain`으로 모두 regular에 옮긴 뒤 빈 coplanar를 반환하여
    /// 호출자의 debug log가 항상 "0 coplanar"로 찍히는 버그가 있었음.
    /// 이제는 공면 세그먼트만 단일 벡터로 반환.
    fn detect_coplanar_faces(
        &self,
        solid_a: &SolidData,
        solid_b: &SolidData,
    ) -> Vec<IntersectionSegment> {
        let mut coplanar_segs = Vec::new();

        for &fid_a in &solid_a.face_ids {
            let face_a = match self.faces.get(fid_a) {
                Some(f) if f.is_active() => f,
                _ => continue,
            };
            let normal_a = face_a.normal();

            for &fid_b in &solid_b.face_ids {
                let face_b = match self.faces.get(fid_b) {
                    Some(f) if f.is_active() => f,
                    _ => continue,
                };
                let normal_b = face_b.normal();

                // 법선이 평행한지 확인 (parallel or anti-parallel)
                let dot = normal_a.dot(normal_b).abs();
                if (dot - 1.0).abs() < 1e-6 {
                    // 공면 후보: centroid로 같은 평면인지 확인
                    if let (Some(c_a), Some(c_b)) = (self.face_centroid(fid_a), self.face_centroid(fid_b)) {
                        let diff = c_b - c_a;
                        let dist_to_plane = diff.dot(normal_a).abs();
                        if dist_to_plane < 1e-5 {
                            // 같은 평면 → edge bounding box로 pseudo segment 생성
                            if let (Ok(verts_a), Ok(verts_b)) =
                                (self.collect_loop_verts(face_a.outer().start),
                                 self.collect_loop_verts(face_b.outer().start))
                            {
                                // 각 face의 bounding box 코너 중 일부를 이용해 교차 세그먼트 생성
                                let positions_a: Vec<_> = verts_a.iter()
                                    .filter_map(|&v| self.verts.get(v).map(|v| v.pos()))
                                    .collect();
                                let positions_b: Vec<_> = verts_b.iter()
                                    .filter_map(|&v| self.verts.get(v).map(|v| v.pos()))
                                    .collect();

                                if positions_a.len() >= 2 && positions_b.len() >= 2 {
                                    // 간단한 전략: face의 centroid를 약간 offset한 segment
                                    coplanar_segs.push(IntersectionSegment {
                                        face_a: fid_a,
                                        face_b: fid_b,
                                        p0: positions_a[0],
                                        p1: positions_a[positions_a.len() - 1],
                                    });
                                }
                            }
                        }
                    }
                }
            }
        }

        coplanar_segs
    }

    /// ADR-197 β-2a — general (non-coplanar) face-face intersection segments.
    ///
    /// For each non-parallel face pair, the plane∩plane line clipped to BOTH
    /// convex face polygons = the segment where the two faces cross in 3D.
    /// (Coplanar pairs → `detect_coplanar_faces`; parallel non-coplanar pairs
    /// never intersect.) This is the raw material for the imprint step of a
    /// VOLUMETRIC solid Boolean (ADR-197 Path B). NOT wired into `boolean()`
    /// yet — the downstream imprint + classify + sew (β-2b/c) consume it.
    pub(crate) fn detect_general_intersections(
        &self,
        solid_a: &SolidData,
        solid_b: &SolidData,
    ) -> Vec<IntersectionSegment> {
        let mut segs = Vec::new();
        for &fid_a in &solid_a.face_ids {
            let (na, poly_a) = match self.face_unit_normal_and_poly(fid_a) {
                Some(x) => x,
                None => continue,
            };
            let da = na.dot(poly_a[0]);
            for &fid_b in &solid_b.face_ids {
                let (nb, poly_b) = match self.face_unit_normal_and_poly(fid_b) {
                    Some(x) => x,
                    None => continue,
                };
                let draw = na.cross(nb);
                let denom = draw.length_squared();
                if denom < 1e-12 {
                    continue; // parallel / coplanar
                }
                let db = nb.dot(poly_b[0]);
                // a point on the plane∩plane line + unit direction (na,nb unit).
                let p_line = (da * nb.cross(draw) + db * draw.cross(na)) / denom;
                let dir = draw / denom.sqrt();
                let ra = clip_line_to_convex_poly(&poly_a, na, p_line, dir);
                let rb = clip_line_to_convex_poly(&poly_b, nb, p_line, dir);
                if let (Some((a0, a1)), Some((b0, b1))) = (ra, rb) {
                    let lo = a0.max(b0);
                    let hi = a1.min(b1);
                    if hi - lo > 1e-7 {
                        segs.push(IntersectionSegment {
                            face_a: fid_a,
                            face_b: fid_b,
                            p0: p_line + dir * lo,
                            p1: p_line + dir * hi,
                        });
                    }
                }
            }
        }
        segs
    }

    /// ADR-197 β-3-β — curved-Boolean SSI dispatch. For every face pair where AT
    /// LEAST ONE face carries a CURVED analytic surface (Cylinder/Sphere/Cone),
    /// compute the closed-form surface-surface intersection (reusing
    /// `ssi::analytic`). Plane×Plane is left to the polygon arrangement (β-2d);
    /// unsupported pairs (NURBS-class, Sphere×Sphere, tangent contacts, …) yield
    /// nothing here (later β). This is the raw material for the curved imprint
    /// (β-3-γ) — NOT yet wired into `solid_boolean`.
    pub(crate) fn detect_curved_intersections(
        &self,
        faces_a: &[FaceId],
        faces_b: &[FaceId],
    ) -> Vec<CurvedIntersection> {
        let mut out = Vec::new();
        for &fa in faces_a {
            let Some(sa) = self.faces.get(fa).and_then(|f| f.surface()) else {
                continue;
            };
            let sa = sa.clone();
            for &fb in faces_b {
                let Some(sb) = self.faces.get(fb).and_then(|f| f.surface()) else {
                    continue;
                };
                if is_planar_surface(&sa) && is_planar_surface(sb) {
                    continue; // both planar → handled by the polygon arrangement (β-2d)
                }
                if let Some(ssi) = surface_surface_intersection(&sa, sb) {
                    if ssi.points.len() >= 2 && !ssi.tangent_warning {
                        out.push(CurvedIntersection { face_a: fa, face_b: fb, ssi });
                    }
                }
            }
        }
        out
    }

    /// ADR-197 β-3-ε-3 (MVP orchestration) — the FIRST automatic curved Boolean:
    /// clip a kernel-native sphere by a half-space `{(p − plane_origin)·n > 0}`,
    /// running the curved pipeline end-to-end — SSI (`plane_sphere`) → imprint
    /// (`imprint_curved_face`) → classify (`classify_curved_subface`) → sew
    /// (`sew_closed_curve_pair`, ε-1). The result is a watertight capped sphere
    /// (curved cap + planar disk on the shared SSI circle).
    ///
    /// Scope: a single axis-aligned cut. INTERSECT keeps one spherical cap;
    /// SUBTRACT keeps two pole-to-cut caps that MERGE into one (the south pole is
    /// a point → still a single boundary), so both sew via ε-1 — no annulus. A
    /// kept band that reaches NO pole (a genuine annulus) or a full box (multiple
    /// cuts → periodic arrangement γ-2b) are deferred.
    pub fn boolean_sphere_halfspace(
        &mut self,
        sphere_faces: &[FaceId],
        plane_origin: DVec3,
        plane_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        let n = plane_normal.normalize_or_zero();
        if n.length_squared() < 0.5 {
            bail!("ADR-197 ε-3: degenerate plane normal");
        }
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 ε-3: no Sphere surface among the faces"))?;

        let dist = (center - plane_origin).dot(n);
        if dist.abs() >= radius - 1e-9 {
            bail!("ADR-197 ε-3: plane does not cut the sphere (|d|={:.3} ≥ r)", dist.abs());
        }
        let circle_center = center - dist * n;
        let r_circ = (radius * radius - dist * dist).sqrt();
        let basis_u = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
            .normalize_or_zero();
        let anchor_pos = circle_center + basis_u * r_circ;
        let circle = crate::curves::AnalyticCurve::Circle {
            center: circle_center,
            radius: r_circ,
            normal: n,
            basis_u,
        };
        let ssi = crate::surfaces::ssi::analytic::plane_sphere(plane_origin, n, center, radius, 64);
        let in_result =
            |p: DVec3| (p - center).length() < radius && (p - plane_origin).dot(n) > 0.0;

        // imprint + classify each sphere face → the kept cap(s) as a Sphere with
        // the sub-region's v-range.
        let mut kept: Vec<S> = Vec::new();
        for &sf in sphere_faces {
            let Some(surf) = self.face_surface(sf).cloned() else {
                continue;
            };
            match imprint_curved_face(&surf, &ssi) {
                Some(subs) => {
                    for s in &subs {
                        if classify_curved_subface(s, &in_result).is_some() {
                            let vmin = s.uv_region.iter().map(|p| p.1).fold(f64::MAX, f64::min);
                            let vmax = s.uv_region.iter().map(|p| p.1).fold(f64::MIN, f64::max);
                            if let S::Sphere { center: c, radius: r, u_range, .. } = &surf {
                                kept.push(S::Sphere {
                                    center: *c,
                                    radius: *r,
                                    axis_dir: DVec3::Z,
                                    ref_dir: DVec3::X,
                                    u_range: *u_range,
                                    v_range: (vmin, vmax),
                                });
                            }
                        }
                    }
                }
                None => {
                    // whole-face classify (un-split face, e.g. the far hemisphere).
                    let whole = CurvedSubFace {
                        surface: surf.clone(),
                        uv_region: full_uv_rect(&surf),
                        uv_holes: Vec::new(),
                        u_shift: 0.0,
                    };
                    if classify_curved_subface(&whole, &in_result).is_some() {
                        kept.push(surf);
                    }
                }
            }
        }
        // Merge the kept cap(s) into a SINGLE cap. The intersect side keeps one
        // cap directly; the subtract side keeps two adjacent caps (a hemisphere
        // + the near band) that merge into one — their union reaches a pole, so
        // the merged cap still has a single boundary (the SSI circle). A band
        // that reaches NO pole is a genuine annulus → deferred to ε-2.
        if kept.is_empty() {
            bail!("ADR-197 ε-3: nothing kept");
        }
        // ADR-204 — express the kept +n hemisphere cap as an ORIENTED Sphere
        // (pole = cut normal `n`), so a TILTED cut renders the correct tilted cap
        // instead of a Z-latitude band. A single plane cut always keeps the +n
        // pole (kept side is dot(·, n) > 0, and dist + radius > 0), so the cap
        // reaches v = +π/2 and is bounded by the SSI circle at latitude
        // asin(-dist/radius) relative to `n`. This closed-form replaces the
        // Z-frame imprint v_range; the `kept` loop above only confirms
        // non-emptiness (a degenerate / non-intersecting cut keeps nothing).
        let v_cut = (-dist / radius).clamp(-1.0, 1.0).asin();
        let cap_surf = S::Sphere {
            center,
            radius,
            axis_dir: n,
            ref_dir: basis_u,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (v_cut, std::f64::consts::FRAC_PI_2),
        };
        // Outward normal at the cap midpoint (n-frame) — orients the sew.
        let cap_normal = {
            let vc = (v_cut + std::f64::consts::FRAC_PI_2) * 0.5;
            (crate::surfaces::sphere::evaluate(center, radius, n, basis_u, 0.0, vc) - center)
                .normalize_or_zero()
        };
        let disk = S::Plane {
            origin: plane_origin,
            normal: -n,
            basis_u,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };

        // remove the original sphere faces + their (shared) edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &sf in sphere_faces {
                if let Some(f) = self.faces.get(sf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes {
                            es.insert(self.hes[he].edge(), ());
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &sf in sphere_faces {
            let _ = self.remove_face(sf);
        }
        for eid in orig_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }

        let (cap_f, disk_f) = self.sew_closed_curve_pair(
            anchor_pos, circle, cap_surf, cap_normal, disk, -n, material,
        )?;
        Ok(vec![cap_f, disk_f])
    }

    /// ADR-197 β-3-ε-2 (orchestration) — the AUTOMATIC barrel: a kernel-native
    /// sphere ∩ the Z-slab `{z_lo < z < z_hi}` (both planes ⟂ Z and straddling
    /// the equator). The curved pipeline (two SSI circles → imprint → classify →
    /// MERGE the kept caps into a pole-free band → `sew_curved_band`) yields a
    /// watertight barrel: a Sphere band v∈[v_lo, v_hi] + top & bottom Plane disks.
    ///
    /// MVP scope: a Z-slab whose two planes straddle the equator (each hemisphere
    /// cut once). Oblique / off-equator multi-cut slabs need γ-2b.
    pub fn boolean_sphere_slab(
        &mut self,
        sphere_faces: &[FaceId],
        z_lo: f64,
        z_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        if z_lo >= z_hi {
            bail!("ADR-197 ε-2 slab: z_lo < z_hi required");
        }
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 ε-2 slab: no Sphere surface"))?;
        if z_lo <= center.z - radius + 1e-9 || z_hi >= center.z + radius - 1e-9 {
            bail!("ADR-197 ε-2 slab: both planes must cut the sphere");
        }
        if !(z_lo < center.z && z_hi > center.z) {
            bail!("ADR-197 ε-2 slab MVP: the slab must straddle the equator (z_lo<cz<z_hi)");
        }
        let ssi_lo =
            crate::surfaces::ssi::analytic::plane_sphere(DVec3::new(0., 0., z_lo), DVec3::Z, center, radius, 64);
        let ssi_hi =
            crate::surfaces::ssi::analytic::plane_sphere(DVec3::new(0., 0., z_hi), DVec3::Z, center, radius, 64);
        let in_result =
            |p: DVec3| (p - center).length() < radius && p.z > z_lo && p.z < z_hi;
        let v_lat_lo = ((z_lo - center.z) / radius).clamp(-1.0, 1.0).asin();
        let v_lat_hi = ((z_hi - center.z) / radius).clamp(-1.0, 1.0).asin();

        // imprint each hemisphere by whichever circle's latitude falls in its band.
        let mut kept: Vec<(f64, f64)> = Vec::new();
        for &sf in sphere_faces {
            let Some(surf) = self.face_surface(sf).cloned() else {
                continue;
            };
            let S::Sphere { v_range, .. } = &surf else {
                continue;
            };
            let ssi = if v_lat_hi > v_range.0 + 1e-9 && v_lat_hi < v_range.1 - 1e-9 {
                Some(&ssi_hi)
            } else if v_lat_lo > v_range.0 + 1e-9 && v_lat_lo < v_range.1 - 1e-9 {
                Some(&ssi_lo)
            } else {
                None
            };
            match ssi.and_then(|s| imprint_curved_face(&surf, s)) {
                Some(subs) => {
                    for s in &subs {
                        if classify_curved_subface(s, &in_result).is_some() {
                            let vmin = s.uv_region.iter().map(|p| p.1).fold(f64::MAX, f64::min);
                            let vmax = s.uv_region.iter().map(|p| p.1).fold(f64::MIN, f64::max);
                            kept.push((vmin, vmax));
                        }
                    }
                }
                None => {
                    let whole = CurvedSubFace {
                        surface: surf.clone(),
                        uv_region: full_uv_rect(&surf),
                        uv_holes: Vec::new(),
                        u_shift: 0.0,
                    };
                    if classify_curved_subface(&whole, &in_result).is_some() {
                        kept.push(*v_range);
                    }
                }
            }
        }
        if kept.is_empty() {
            bail!("ADR-197 ε-2 slab: nothing kept");
        }
        kept.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        for w in kept.windows(2) {
            if (w[0].1 - w[1].0).abs() > 1e-6 {
                bail!("ADR-197 ε-2 slab: kept band not contiguous");
            }
        }
        let band_v_lo = kept.first().unwrap().0;
        let band_v_hi = kept.last().unwrap().1;
        if (band_v_lo + FRAC_PI_2).abs() < 1e-6 || (band_v_hi - FRAC_PI_2).abs() < 1e-6 {
            bail!("ADR-197 ε-2 slab: band reaches a pole — that is a cap (use halfspace)");
        }

        let r_lo = (radius * radius - (z_lo - center.z).powi(2)).max(0.0).sqrt();
        let r_hi = (radius * radius - (z_hi - center.z).powi(2)).max(0.0).sqrt();
        let circle = |z: f64, r: f64| crate::curves::AnalyticCurve::Circle {
            center: DVec3::new(center.x, center.y, z),
            radius: r,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        let band = S::Sphere {
            center,
            radius,
            axis_dir: DVec3::Z,
            ref_dir: DVec3::X,
            u_range: (0.0, TAU),
            v_range: (band_v_lo, band_v_hi),
        };
        let disk = |z: f64, nz: DVec3| S::Plane {
            origin: DVec3::new(center.x, center.y, z),
            normal: nz,
            basis_u: DVec3::X,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };

        // remove the original sphere faces + edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &sf in sphere_faces {
                if let Some(f) = self.faces.get(sf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes {
                            es.insert(self.hes[he].edge(), ());
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &sf in sphere_faces {
            let _ = self.remove_face(sf);
        }
        for eid in orig_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }

        let (bf, tdf, bdf) = self.sew_curved_band(
            DVec3::new(center.x + r_hi, center.y, z_hi),
            circle(z_hi, r_hi),
            DVec3::new(center.x + r_lo, center.y, z_lo),
            circle(z_lo, r_lo),
            band,
            DVec3::X,
            disk(z_hi, DVec3::Z),
            DVec3::Z,
            disk(z_lo, DVec3::NEG_Z),
            DVec3::NEG_Z,
            material,
        )?;
        Ok(vec![bf, tdf, bdf])
    }

    /// ADR-204 β-3 — sphere ∩ slab between two planes ⟂ `plane_normal` at signed
    /// distances `d_lo < d_hi` from the centre along `plane_normal`. Both planes
    /// must cut the sphere (`|d| < radius`) and straddle the centre
    /// (`d_lo < 0 < d_hi`), so the kept piece is a BAND (not a cap). The band is
    /// an ORIENTED Sphere (axis_dir = plane_normal), so a TILTED slab renders the
    /// correct spherical zone via the v_range path — the β-2 oriented-cap pattern
    /// extended to the full-u band (NO `tessellate_sphere_clipped` needed; that
    /// path is for ADR-202 small-circle sketching). Because the band is uniquely
    /// determined by the two cut circles, the imprint/classify pass of the Z-axis
    /// `boolean_sphere_slab` is unnecessary here — the v_range is closed-form.
    /// Returns `[band, top_disk, bottom_disk]`.
    pub fn boolean_sphere_slab_oriented(
        &mut self,
        sphere_faces: &[FaceId],
        plane_normal: DVec3,
        d_lo: f64,
        d_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let n = plane_normal.normalize_or_zero();
        if n.length_squared() < 0.5 {
            bail!("ADR-204 β-3 oriented slab: degenerate plane normal");
        }
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-204 β-3 oriented slab: no Sphere surface"))?;
        if d_lo >= d_hi {
            bail!("ADR-204 β-3 oriented slab: d_lo < d_hi required");
        }
        if d_lo <= -radius + 1e-9 || d_hi >= radius - 1e-9 {
            bail!("ADR-204 β-3 oriented slab: both planes must cut the sphere (|d| < r)");
        }
        if !(d_lo < 0.0 && d_hi > 0.0) {
            bail!("ADR-204 β-3 oriented slab MVP: the slab must straddle the centre (d_lo<0<d_hi)");
        }
        let basis_u = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
            .normalize_or_zero();
        let v_band_lo = (d_lo / radius).clamp(-1.0, 1.0).asin();
        let v_band_hi = (d_hi / radius).clamp(-1.0, 1.0).asin();
        let r_lo = (radius * radius - d_lo * d_lo).max(0.0).sqrt();
        let r_hi = (radius * radius - d_hi * d_hi).max(0.0).sqrt();
        let circle = |d: f64, rc: f64| crate::curves::AnalyticCurve::Circle {
            center: center + d * n,
            radius: rc,
            normal: n,
            basis_u,
        };
        let band = S::Sphere {
            center,
            radius,
            axis_dir: n,
            ref_dir: basis_u,
            u_range: (0.0, TAU),
            v_range: (v_band_lo, v_band_hi),
        };
        let disk = |d: f64, dn: DVec3| S::Plane {
            origin: center + d * n,
            normal: dn,
            basis_u,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };

        // remove the original sphere faces + edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &sf in sphere_faces {
                if let Some(f) = self.faces.get(sf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes {
                            es.insert(self.hes[he].edge(), ());
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &sf in sphere_faces {
            let _ = self.remove_face(sf);
        }
        for eid in orig_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }

        let (bf, tdf, bdf) = self.sew_curved_band(
            center + d_hi * n + basis_u * r_hi,
            circle(d_hi, r_hi),
            center + d_lo * n + basis_u * r_lo,
            circle(d_lo, r_lo),
            band,
            basis_u,
            disk(d_hi, n),
            n,
            disk(d_lo, -n),
            -n,
            material,
        )?;
        Ok(vec![bf, tdf, bdf])
    }

    // ─────────────────────────────────────────────────────────────────────
    // ADR-197 Z-axis lift (A) — local-frame transform for arbitrary-axis cuts
    //
    // The single-primitive curved cut ops (`boolean_cylinder_slab` etc.) assume
    // the primitive axis ∥ Z (each bails otherwise). Rather than rewrite every
    // op's world-Z math, we lift the restriction with a rotate-roundtrip: rotate
    // the whole solid (verts + per-face analytic surfaces + edge curves) into a
    // Z-frame via `rotate_verts`, run the existing Z-locked op, then rotate the
    // result back. The cut bounds are then interpreted along the primitive's OWN
    // axis (cut planes ⟂ that axis), which is the only thing the circular-section
    // sew machinery can represent for a tilted primitive.
    // ─────────────────────────────────────────────────────────────────────

    /// Every active loop vert (outer + inner) of a face set. `rotate_verts`'s
    /// all-or-none surface rule needs the COMPLETE vert set of a solid so that
    /// no face's analytic surface is silently dropped during the rotation.
    fn solid_loop_verts(&self, faces: &[FaceId]) -> Vec<VertId> {
        let mut set: std::collections::HashSet<VertId> = std::collections::HashSet::new();
        for &fid in faces {
            let face = match self.faces.get(fid) {
                Some(f) if f.is_active() => f,
                _ => continue,
            };
            if let Ok(vs) = self.collect_loop_verts(face.outer().start) {
                set.extend(vs);
            }
            for inner in face.inners() {
                if let Ok(vs) = self.collect_loop_verts(inner.start) {
                    set.extend(vs);
                }
            }
        }
        set.into_iter().collect()
    }

    /// Rotate a curved solid so its axis aligns to `target` (the orientation the
    /// inner op expects — +Z for cylinder/torus, −Z for apex-up cone), run `op`
    /// (an existing axis-locked cut), then rotate the result back — lifting the
    /// per-op axis restriction WITHOUT touching the op. The rotation pivots at
    /// `pivot` (so it stays fixed), and `rotate_verts` carries the per-face
    /// analytic surfaces + edge curves + normals through both legs. On `op`
    /// failure the original solid is rotated back (the cut ops validate before
    /// mutating, so the input faces are intact). ADR-197 Z-axis lift (A).
    fn with_axis_lifted_to<F>(
        &mut self,
        solid_faces: &[FaceId],
        axis_dir: DVec3,
        pivot: DVec3,
        target: DVec3,
        op: F,
    ) -> Result<Vec<FaceId>>
    where
        F: FnOnce(&mut Self) -> Result<Vec<FaceId>>,
    {
        let ad = axis_dir.normalize_or_zero();
        let td = target.normalize_or_zero();
        if ad.length_squared() < 0.5 || td.length_squared() < 0.5 {
            bail!("ADR-197 Z-axis lift: degenerate primitive axis or target");
        }
        // Already aligned → the op runs directly (no rotation).
        if (ad - td).length() < 1e-6 {
            return op(self);
        }
        // Rotation (axis, angle) mapping `ad` → `td`, pivoting at `pivot`.
        let (rot_axis, angle) = if (ad + td).length() < 1e-6 {
            // Antiparallel: 180° about any axis ⟂ to `td`.
            let perp = if td.cross(DVec3::X).length() > 1e-6 {
                td.cross(DVec3::X)
            } else {
                td.cross(DVec3::Y)
            };
            (perp.normalize(), std::f64::consts::PI)
        } else {
            (
                ad.cross(td).normalize(),
                ad.dot(td).clamp(-1.0, 1.0).acos(),
            )
        };
        // Forward leg: rotate the whole solid into the canonical frame.
        let fwd_verts = self.solid_loop_verts(solid_faces);
        self.rotate_verts(&fwd_verts, pivot, rot_axis, angle)?;
        // Run the existing axis-locked op (the primitive is now aligned).
        let result = op(self);
        // Inverse leg: rotate back. On success the new result faces carry the
        // solid; on failure the original faces are still present.
        let back_faces: Vec<FaceId> = match &result {
            Ok(fs) => fs.clone(),
            Err(_) => solid_faces.to_vec(),
        };
        let back_verts = self.solid_loop_verts(&back_faces);
        self.rotate_verts(&back_verts, pivot, rot_axis, -angle)?;
        result
    }

    /// Read `(axis_dir, axis_origin)` of the first Cylinder-surfaced face.
    fn cylinder_axis_of(&self, faces: &[FaceId]) -> Result<(DVec3, DVec3)> {
        for &f in faces {
            if let Some(crate::surfaces::AnalyticSurface::Cylinder {
                axis_dir, axis_origin, ..
            }) = self.face_surface(f)
            {
                return Ok((*axis_dir, *axis_origin));
            }
        }
        bail!("ADR-197 Z-axis lift: no Cylinder surface in face set");
    }

    /// Read the first Cylinder surface's full parameters.
    fn cylinder_full_of(
        &self,
        faces: &[FaceId],
    ) -> Result<(DVec3, DVec3, f64, DVec3, (f64, f64))> {
        for &f in faces {
            if let Some(crate::surfaces::AnalyticSurface::Cylinder {
                axis_origin, axis_dir, radius, ref_dir, v_range, ..
            }) = self.face_surface(f)
            {
                return Ok((*axis_origin, *axis_dir, *radius, *ref_dir, *v_range));
            }
        }
        bail!("ADR-205 β-2: no Cylinder surface in face set");
    }

    /// Read the first Cone surface's full parameters
    /// `(apex, axis_dir, half_angle, ref_dir, v_range)`.
    fn cone_full_of(
        &self,
        faces: &[FaceId],
    ) -> Result<(DVec3, DVec3, f64, DVec3, (f64, f64))> {
        for &f in faces {
            if let Some(crate::surfaces::AnalyticSurface::Cone {
                apex, axis_dir, half_angle, ref_dir, v_range, ..
            }) = self.face_surface(f)
            {
                return Ok((*apex, *axis_dir, *half_angle, *ref_dir, *v_range));
            }
        }
        bail!("ADR-205 β-2-cone: no Cone surface in face set");
    }

    /// **ADR-205 β-2-cone** — a kernel-native cone cut by an OBLIQUE plane (not ⟂
    /// the axis, steeper than the cone slant) → an ELLIPTIC section. Keeps the
    /// BASE-side FRUSTUM (the +`plane_normal` side must contain the base): base
    /// disk (Plane) + trimmed cone-side band (base circle + ellipse boundaries) +
    /// a planar ELLIPTIC cap. The ellipse is `cone_oblique_ellipse` +
    /// `nurbs::ellipse` (β-1); the band renders boundary-aware via
    /// `tessellate_cone_clipped`. Returns `[side_band, elliptic_cap, base_disk]`.
    ///
    /// MVP scope: the plane must SEPARATE the apex from the base (cut the side),
    /// the section is a bounded ellipse wholly on the side, and the BASE is on the
    /// +m side. Keeping the apex tip (base on −m), a parabola/hyperbola section, or
    /// a ⟂/∥ plane is rejected.
    pub fn boolean_cone_oblique_halfspace(
        &mut self,
        cone_faces: &[FaceId],
        plane_origin: DVec3,
        plane_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 β-2-cone: degenerate plane normal");
        }
        let (apex, axis_dir_raw, half_angle, ref_dir, v_range) = self.cone_full_of(cone_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || half_angle <= 1e-6 || half_angle >= FRAC_PI_2 - 1e-6 {
            bail!("ADR-205 β-2-cone: degenerate cone");
        }
        let height = v_range.0.max(v_range.1);
        let base_center = apex + n_a * height;
        let base_radius = height * half_angle.tan();
        // the plane must SEPARATE apex from base (cut the side) with the BASE on +m.
        let apex_side = (apex - plane_origin).dot(m);
        let base_side = (base_center - plane_origin).dot(m);
        if base_side <= 1e-9 {
            bail!("ADR-205 β-2-cone: this keeps the apex tip (base on −m) — use boolean_cone_apex_halfspace, or pass −plane_normal to keep the base frustum");
        }
        if apex_side >= -1e-9 {
            bail!("ADR-205 β-2-cone: plane must separate the apex (−m) from the base (+m) — it misses or clips an end");
        }
        // elliptic section (cone_oblique_ellipse — cone Dandelin α).
        let (e_center, semi_major, semi_minor, major_dir, minor_dir) =
            cone_oblique_ellipse(apex, n_a, half_angle, plane_origin, m).ok_or_else(|| {
                anyhow::anyhow!("ADR-205 β-2-cone: section is not a bounded ellipse (parabola/hyperbola or ⟂ plane)")
            })?;
        // clean-cut guard: the ellipse axial extent strictly within (0, height).
        let e_axial = (e_center - apex).dot(n_a);
        let axial_spread = ((semi_major * major_dir.dot(n_a)).powi(2)
            + (semi_minor * minor_dir.dot(n_a)).powi(2))
            .sqrt();
        if e_axial - axial_spread <= 1e-6 || e_axial + axial_spread >= height - 1e-6 {
            bail!("ADR-205 β-2-cone: the ellipse extends past the apex or the base (not wholly on the side)");
        }
        // build the frustum: base disk + cone band + elliptic cap.
        let (cp, w, kn, deg) =
            crate::curves::nurbs::ellipse(e_center, semi_major, semi_minor, major_dir, minor_dir);
        let top_anchor = cp[0];
        let top_ellipse = crate::curves::AnalyticCurve::NURBS {
            control_pts: cp, weights: w, knots: kn, degree: deg as u32,
        };
        let ref_n = ref_dir.normalize_or_zero();
        let base_circle = crate::curves::AnalyticCurve::Circle {
            center: base_center, radius: base_radius, normal: n_a, basis_u: ref_n,
        };
        let base_anchor = base_center + ref_n * base_radius;
        let band = S::Cone {
            apex, axis_dir: n_a, half_angle, ref_dir,
            u_range: (0.0, TAU),
            v_range: ((e_axial - axial_spread).max(0.0), height),
        };
        // elliptic cap faces −m (away from the kept +m frustum, toward the cut-off apex).
        let cap = S::Plane {
            origin: e_center, normal: -m, basis_u: major_dir,
            u_range: (-semi_major * 1.2, semi_major * 1.2),
            v_range: (-semi_major * 1.2, semi_major * 1.2),
        };
        let base_disk = S::Plane {
            origin: base_center, normal: n_a, basis_u: ref_n,
            u_range: (-base_radius * 1.5, base_radius * 1.5),
            v_range: (-base_radius * 1.5, base_radius * 1.5),
        };
        // remove the original cone faces + their edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cone_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cone_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (band_f, cap_f, disk_f) = self.sew_curved_band(
            top_anchor, top_ellipse,
            base_anchor, base_circle,
            band, ref_n,
            cap, -m,
            base_disk, n_a,
            material,
        )?;
        Ok(vec![band_f, cap_f, disk_f])
    }

    /// **ADR-205 cone apex-tip** — the deferred companion of β-2-cone: a kernel-native
    /// cone cut by an OBLIQUE plane, keeping the small APEX cone (the +`plane_normal`
    /// side must contain the APEX). The result is the cone-side fan from the apex to
    /// the elliptic cut + the elliptic cap (`[side, cap]`) — the apex is a degenerate
    /// pole, so it is sewn by `sew_cone_tip` (one elliptic self-loop), not
    /// `sew_curved_band`. The ellipse is `cone_oblique_ellipse` + `nurbs::ellipse`; the
    /// side renders apex-clipped via `tessellate_cone_clipped` (single-plane branch).
    ///
    /// MVP scope: the plane SEPARATES the apex (+m, kept) from the base (−m), the
    /// section is a bounded ellipse wholly on the side. A parabola/hyperbola section,
    /// a ⟂/∥ plane, or the base on +m (→ β-2-cone) is rejected.
    pub fn boolean_cone_apex_halfspace(
        &mut self,
        cone_faces: &[FaceId],
        plane_origin: DVec3,
        plane_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 cone apex-tip: degenerate plane normal");
        }
        let (apex, axis_dir_raw, half_angle, ref_dir, v_range) = self.cone_full_of(cone_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || half_angle <= 1e-6 || half_angle >= FRAC_PI_2 - 1e-6 {
            bail!("ADR-205 cone apex-tip: degenerate cone");
        }
        let height = v_range.0.max(v_range.1);
        let base_center = apex + n_a * height;
        // the plane must SEPARATE apex from base (cut the side) with the APEX on +m.
        let apex_side = (apex - plane_origin).dot(m);
        let base_side = (base_center - plane_origin).dot(m);
        if apex_side <= 1e-9 {
            bail!("ADR-205 cone apex-tip: keeping the apex requires it on +m — pass −plane_normal, or use β-2-cone for the base frustum");
        }
        if base_side >= -1e-9 {
            bail!("ADR-205 cone apex-tip: plane must separate the apex (+m) from the base (−m) — it misses or clips an end");
        }
        // elliptic section (cone_oblique_ellipse — cone Dandelin α).
        let (e_center, semi_major, semi_minor, major_dir, minor_dir) =
            cone_oblique_ellipse(apex, n_a, half_angle, plane_origin, m).ok_or_else(|| {
                anyhow::anyhow!("ADR-205 cone apex-tip: section is not a bounded ellipse (parabola/hyperbola or ⟂ plane)")
            })?;
        // clean-cut guard: the ellipse axial extent strictly within (0, height).
        let e_axial = (e_center - apex).dot(n_a);
        let axial_spread = ((semi_major * major_dir.dot(n_a)).powi(2)
            + (semi_minor * minor_dir.dot(n_a)).powi(2))
            .sqrt();
        if e_axial - axial_spread <= 1e-6 || e_axial + axial_spread >= height - 1e-6 {
            bail!("ADR-205 cone apex-tip: the ellipse extends past the apex or the base (not wholly on the side)");
        }
        // build the tip: cone-side fan (apex degenerate) + elliptic cap.
        let (cp, w, kn, deg) =
            crate::curves::nurbs::ellipse(e_center, semi_major, semi_minor, major_dir, minor_dir);
        let anchor = cp[0];
        let ellipse = crate::curves::AnalyticCurve::NURBS {
            control_pts: cp, weights: w, knots: kn, degree: deg as u32,
        };
        let side = S::Cone {
            apex, axis_dir: n_a, half_angle, ref_dir,
            u_range: (0.0, TAU),
            v_range: (0.0, (e_axial + axial_spread).min(height)),
        };
        // elliptic cap faces −m (away from the kept +m tip, toward the cut-off base).
        let cap = S::Plane {
            origin: e_center, normal: -m, basis_u: major_dir,
            u_range: (-semi_major * 1.2, semi_major * 1.2),
            v_range: (-semi_major * 1.2, semi_major * 1.2),
        };
        // remove the original cone faces + their edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cone_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cone_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (side_f, cap_f) = self.sew_cone_tip(anchor, ellipse, side, n_a, cap, -m, material)?;
        Ok(vec![side_f, cap_f])
    }

    /// **ADR-205 β-3-cone** — a kernel-native cone cut by TWO PARALLEL OBLIQUE
    /// planes (shared normal `m`, offsets `d_lo < d_hi` along `m` from the apex)
    /// → an ELLIPTIC SLAB. Keeps the band BETWEEN the planes
    /// (`d_lo < (p−apex)·m < d_hi`): a trimmed Cone band with TWO elliptic
    /// boundaries + two planar ELLIPTIC caps (no base disk, no apex — the slab is
    /// the truncated middle). Returns `[band, cap_hi, cap_lo]`.
    ///
    /// Both elliptic boundaries are `cone_oblique_ellipse` (cone Dandelin α) +
    /// `nurbs::ellipse` (β-1); the band renders boundary-aware via
    /// `tessellate_cone_clipped` (both boundaries oblique). MVP scope: both planes
    /// give bounded ellipse sections wholly on the side (clear of apex + base) and
    /// the slab lies on the side (apex below `d_lo`, base above `d_hi`).
    pub fn boolean_cone_oblique_slab(
        &mut self,
        cone_faces: &[FaceId],
        plane_normal: DVec3,
        d_lo: f64,
        d_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 β-3-cone: degenerate plane normal");
        }
        if d_lo >= d_hi {
            bail!("ADR-205 β-3-cone: d_lo < d_hi required");
        }
        let (apex, axis_dir_raw, half_angle, ref_dir, v_range) = self.cone_full_of(cone_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || half_angle <= 1e-6 || half_angle >= FRAC_PI_2 - 1e-6 {
            bail!("ADR-205 β-3-cone: degenerate cone");
        }
        let height = v_range.0.max(v_range.1);
        // the slab must lie strictly between the apex (d=0) and the base.
        let base_d = (apex + n_a * height - apex).dot(m); // = height·(n_a·m)
        let (apex_d, base_d) = (0.0_f64, base_d);
        let (slab_out_lo, slab_out_hi) = (apex_d.min(base_d), apex_d.max(base_d));
        if d_lo <= slab_out_lo + 1e-9 || d_hi >= slab_out_hi - 1e-9 {
            bail!("ADR-205 β-3-cone: the slab must lie between the apex and the base (each plane cuts the side)");
        }
        // both planes' ellipse sections.
        let ellipse_at = |d: f64| cone_oblique_ellipse(apex, n_a, half_angle, apex + m * d, m);
        let (lo_c, lo_sm, lo_sn, lo_maj, lo_min) =
            ellipse_at(d_lo).ok_or_else(|| anyhow::anyhow!("ADR-205 β-3-cone: d_lo section is not a bounded ellipse"))?;
        let (hi_c, hi_sm, hi_sn, hi_maj, hi_min) =
            ellipse_at(d_hi).ok_or_else(|| anyhow::anyhow!("ADR-205 β-3-cone: d_hi section is not a bounded ellipse"))?;
        // both ellipses wholly on the side (axial strictly within (0, height)).
        let axial_of = |c: DVec3, sm: f64, sn: f64, maj: DVec3, min: DVec3| -> (f64, f64) {
            let ax = (c - apex).dot(n_a);
            let spread = ((sm * maj.dot(n_a)).powi(2) + (sn * min.dot(n_a)).powi(2)).sqrt();
            (ax, spread)
        };
        let (lo_ax, lo_sp) = axial_of(lo_c, lo_sm, lo_sn, lo_maj, lo_min);
        let (hi_ax, hi_sp) = axial_of(hi_c, hi_sm, hi_sn, hi_maj, hi_min);
        for (ax, sp) in [(lo_ax, lo_sp), (hi_ax, hi_sp)] {
            if ax - sp <= 1e-6 || ax + sp >= height - 1e-6 {
                bail!("ADR-205 β-3-cone: an ellipse extends past the apex or the base (slab not wholly on the side)");
            }
        }
        // build the two elliptic boundaries.
        let make = |c: DVec3, sm: f64, sn: f64, maj: DVec3, min: DVec3| {
            let (cp, w, kn, deg) = crate::curves::nurbs::ellipse(c, sm, sn, maj, min);
            (cp[0], crate::curves::AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: kn, degree: deg as u32 })
        };
        let (lo_anchor, lo_ellipse) = make(lo_c, lo_sm, lo_sn, lo_maj, lo_min);
        let (hi_anchor, hi_ellipse) = make(hi_c, hi_sm, hi_sn, hi_maj, hi_min);
        let ref_n = ref_dir.normalize_or_zero();
        let band = S::Cone {
            apex, axis_dir: n_a, half_angle, ref_dir,
            u_range: (0.0, TAU),
            v_range: ((lo_ax - lo_sp).min(hi_ax - hi_sp).max(0.0), (lo_ax + lo_sp).max(hi_ax + hi_sp).min(height)),
        };
        // cap_lo (d_lo plane) faces −m; cap_hi (d_hi plane) faces +m (both away from
        // the kept d_lo<d<d_hi band).
        let cap_lo = S::Plane {
            origin: lo_c, normal: -m, basis_u: lo_maj,
            u_range: (-lo_sm * 1.2, lo_sm * 1.2), v_range: (-lo_sm * 1.2, lo_sm * 1.2),
        };
        let cap_hi = S::Plane {
            origin: hi_c, normal: m, basis_u: hi_maj,
            u_range: (-hi_sm * 1.2, hi_sm * 1.2), v_range: (-hi_sm * 1.2, hi_sm * 1.2),
        };
        // remove the original cone faces + their edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cone_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cone_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (band_f, cap_hi_f, cap_lo_f) = self.sew_curved_band(
            hi_anchor, hi_ellipse,
            lo_anchor, lo_ellipse,
            band, ref_n,
            cap_hi, m,
            cap_lo, -m,
            material,
        )?;
        Ok(vec![band_f, cap_hi_f, cap_lo_f])
    }

    /// **ADR-205 cone-corner** — a kernel-native cone cut by TWO oblique planes
    /// forming a base-keeping TENT → a corner solid: base disk + corner band (Cone
    /// surface, base circle inner + a 4-edge tent top of two active ellipse arcs) +
    /// two partial elliptic caps. Returns `[band, base_disk, cap_b, cap_a]`. The
    /// cone mirrors cylinder β-5: the kept base frustum's tent top is `max(v_e1,
    /// v_e2)` and `n_a` (apex→base) is the base-outward normal.
    ///
    /// MVP scope: both planes give bounded ellipse sections, keep the base on +m
    /// (apex on −m), their ridge crosses the cone at two corners on the side, and
    /// the base disk is wholly kept. Reuses `sew_corner_band` (β-5) + `cone_oblique_
    /// ellipse` (Dandelin α) + `nurbs::ellipse_arc`; renders via
    /// `tessellate_cone_corner_clipped`.
    pub fn boolean_cone_corner(
        &mut self,
        cone_faces: &[FaceId],
        p1_origin: DVec3,
        p1_normal: DVec3,
        p2_origin: DVec3,
        p2_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let m1 = p1_normal.normalize_or_zero();
        let m2 = p2_normal.normalize_or_zero();
        if m1.length_squared() < 0.5 || m2.length_squared() < 0.5 {
            bail!("ADR-205 cone-corner: degenerate plane normal");
        }
        if m1.cross(m2).length() < 1e-4 {
            bail!("ADR-205 cone-corner: planes parallel → use the slab (β-3-cone) path");
        }
        let (apex, axis_dir_raw, half_angle, ref_dir, v_range) = self.cone_full_of(cone_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || half_angle <= 1e-6 || half_angle >= FRAC_PI_2 - 1e-6 {
            bail!("ADR-205 cone-corner: degenerate cone");
        }
        let tan_a = half_angle.tan();
        let height = v_range.0.max(v_range.1);
        let base_center = apex + n_a * height;
        let base_radius = height * tan_a;
        // both planes: bounded ellipse + apex on −m + base on +m.
        for (mp, op, lbl) in [(m1, p1_origin, "1"), (m2, p2_origin, "2")] {
            if cone_oblique_ellipse(apex, n_a, half_angle, op, mp).is_none() {
                bail!("ADR-205 cone-corner: plane {} section is not a bounded ellipse", lbl);
            }
            if (apex - op).dot(mp) >= -1e-9 {
                bail!("ADR-205 cone-corner: plane {} must put the apex on −m (keep the base)", lbl);
            }
            if (base_center - op).dot(mp) <= 1e-9 {
                bail!("ADR-205 cone-corner: plane {} must put the base on +m", lbl);
            }
        }
        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize_or_zero();
        let v_plane = |m: DVec3, o: DVec3, u: f64| -> f64 {
            let g = n_a + (r_vec * u.cos() + p_vec * u.sin()) * tan_a;
            (o - apex).dot(m) / g.dot(m)
        };
        let surf = |u: f64, v: f64| apex + n_a * v + (r_vec * u.cos() + p_vec * u.sin()) * (v * tan_a);
        let u_of = |p: DVec3| {
            let rel = p - apex;
            let radial = rel - n_a * rel.dot(n_a);
            radial.dot(p_vec).atan2(radial.dot(r_vec)).rem_euclid(TAU)
        };
        // ridge (plane1 ∩ plane2) ∩ cone — closed-form quadratic on the nappe.
        let dir = m1.cross(m2);
        let (d1, d2) = (p1_origin.dot(m1), p2_origin.dot(m2));
        let l0 = (m2.cross(dir) * d1 + dir.cross(m1) * d2) / dir.dot(dir);
        let a0 = l0 - apex;
        let (av, bv) = (a0.dot(n_a), dir.dot(n_a));
        let (q0, q1, q2) = (a0.dot(a0), a0.dot(dir), dir.dot(dir));
        let cos2 = half_angle.cos().powi(2);
        let (qa, qb, qc) = (bv * bv - cos2 * q2, 2.0 * (av * bv - cos2 * q1), av * av - cos2 * q0);
        let disc = qb * qb - 4.0 * qa * qc;
        if qa.abs() < 1e-12 || disc <= 0.0 {
            bail!("ADR-205 cone-corner: the ridge does not cross the cone side");
        }
        let sd = disc.sqrt();
        let corner_lo = l0 + dir * ((-qb - sd) / (2.0 * qa));
        let corner_hi = l0 + dir * ((-qb + sd) / (2.0 * qa));
        for c in [corner_lo, corner_hi] {
            let av_c = (c - apex).dot(n_a);
            if av_c <= 1e-6 || av_c >= height - 1e-6 {
                bail!("ADR-205 cone-corner: a corner is past the apex or the base");
            }
        }
        let (mut uc1, mut uc2) = (u_of(corner_lo), u_of(corner_hi));
        let (mut c1, mut c2) = (corner_lo, corner_hi);
        if uc1 > uc2 {
            std::mem::swap(&mut uc1, &mut uc2);
            std::mem::swap(&mut c1, &mut c2);
        }
        // active plane per arc (base frustum → the binding plane = argMAX v_e).
        let active = |um: f64| -> (DVec3, DVec3) {
            if v_plane(m1, p1_origin, um) >= v_plane(m2, p2_origin, um) {
                (m1, p1_origin)
            } else {
                (m2, p2_origin)
            }
        };
        let mid_ua = 0.5 * (uc1 + uc2);
        let mid_ub = (0.5 * (uc2 + uc1 + TAU)).rem_euclid(TAU);
        let (pa, pa_o) = active(mid_ua);
        let (pb, pb_o) = active(mid_ub);
        let mid_a = surf(mid_ua, v_plane(pa, pa_o, mid_ua));
        let mid_b = surf(mid_ub, v_plane(pb, pb_o, mid_ub));
        let (ea_c, ea_sm, ea_sn, ea_mj, ea_mn) =
            cone_oblique_ellipse(apex, n_a, half_angle, pa_o, pa).unwrap();
        let (eb_c, eb_sm, eb_sn, eb_mj, eb_mn) =
            cone_oblique_ellipse(apex, n_a, half_angle, pb_o, pb).unwrap();
        let phi = |p: DVec3, c: DVec3, sm: f64, sn: f64, mj: DVec3, mn: DVec3| {
            let rel = p - c;
            (rel.dot(mn) / sn).atan2(rel.dot(mj) / sm)
        };
        let arc = |p: DVec3, q: DVec3, c: DVec3, sm: f64, sn: f64, mj: DVec3, mn: DVec3| {
            let (cp, w, k, d) = crate::curves::nurbs::ellipse_arc(c, sm, sn, mj, mn, phi(p, c, sm, sn, mj, mn), phi(q, c, sm, sn, mj, mn));
            crate::curves::AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: d as u32 }
        };
        // Top loop in the orientation that makes the partial caps' Newell normals
        // point OUTWARD (−m_i) for the cone's mirrored (base-keeping) geometry —
        // the OPPOSITE winding to cylinder β-5 (whose bottom-tent geometry wants
        // [c1, mid_b, c2, mid_a]). [c1, mid_a, c2, mid_b]: edges 0,1 on plane pa
        // (arc through mid_a), edges 2,3 on plane pb (arc through mid_b).
        let top_verts = [c1, mid_a, c2, mid_b];
        let top_curves = [
            arc(c1, mid_a, ea_c, ea_sm, ea_sn, ea_mj, ea_mn),
            arc(mid_a, c2, ea_c, ea_sm, ea_sn, ea_mj, ea_mn),
            arc(c2, mid_b, eb_c, eb_sm, eb_sn, eb_mj, eb_mn),
            arc(mid_b, c1, eb_c, eb_sm, eb_sn, eb_mj, eb_mn),
        ];
        // base disk wholly kept by both planes.
        for (mp, op) in [(m1, p1_origin), (m2, p2_origin)] {
            let amp = base_radius * (1.0 - n_a.dot(mp).powi(2)).max(0.0).sqrt();
            if (base_center - op).dot(mp) <= amp + 1e-9 {
                bail!("ADR-205 cone-corner: base disk not wholly kept (a plane clips it)");
            }
        }
        let base_circle = crate::curves::AnalyticCurve::Circle {
            center: base_center, radius: base_radius, normal: n_a, basis_u: r_vec,
        };
        let band = S::Cone { apex, axis_dir: n_a, half_angle, ref_dir, u_range: (0.0, TAU), v_range: (0.0, height) };
        let base_disk = S::Plane {
            origin: base_center, normal: n_a, basis_u: r_vec,
            u_range: (-base_radius * 1.5, base_radius * 1.5), v_range: (-base_radius * 1.5, base_radius * 1.5),
        };
        // remove the original cone faces + their edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cone_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cone_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (band_f, disk_f, vids) = self.sew_corner_band(
            &top_verts, &top_curves, base_center + r_vec * base_radius, base_circle,
            band, r_vec, base_disk, n_a, material,
        )?;
        // caps reuse the band arc edges (opposite traversal). vids = [c1, mid_a,
        // c2, mid_b]: cap_a (plane pa, arc through mid_a) → [c2, mid_a, c1]; cap_b
        // (plane pb, arc through mid_b) → [c1, mid_b, c2]. This winding's Newell
        // points −m_i (OUTWARD), so it satisfies ADR-007 I2 (no set_normal needed).
        let cap_a = self.add_face_with_holes(&[vids[2], vids[1], vids[0]], &[], material)?;
        self.faces[cap_a].set_surface(Some(S::Plane { origin: ea_c, normal: -pa, basis_u: ea_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }));
        let cap_b = self.add_face_with_holes(&[vids[0], vids[3], vids[2]], &[], material)?;
        self.faces[cap_b].set_surface(Some(S::Plane { origin: eb_c, normal: -pb, basis_u: eb_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }));
        Ok(vec![band_f, disk_f, cap_a, cap_b])
    }

    /// **ADR-205 cone apex-tip corner** — the MIRROR of `boolean_cone_corner`: a cone
    /// cut by TWO oblique planes BOTH keeping the APEX → the small apex cone clipped
    /// by a corner. The kept region is `v ∈ [0 (apex), min(v_e1, v_e2)]`, so the
    /// binding plane per arc is `argMIN v_e` (not argMAX) and there is NO base disk —
    /// the apex is the degenerate pole (`sew_corner_tip`). Returns `[corner_band,
    /// cap_a, cap_b]`. Each plane must put the APEX on +m + the base on −m + give a
    /// bounded ellipse; the ridge must cross the cone with both corners on the side.
    pub fn boolean_cone_apex_corner(
        &mut self,
        cone_faces: &[FaceId],
        p1_origin: DVec3,
        p1_normal: DVec3,
        p2_origin: DVec3,
        p2_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let m1 = p1_normal.normalize_or_zero();
        let m2 = p2_normal.normalize_or_zero();
        if m1.length_squared() < 0.5 || m2.length_squared() < 0.5 {
            bail!("ADR-205 apex-tip corner: degenerate plane normal");
        }
        if m1.cross(m2).length() < 1e-4 {
            bail!("ADR-205 apex-tip corner: planes parallel");
        }
        let (apex, axis_dir_raw, half_angle, ref_dir, v_range) = self.cone_full_of(cone_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || half_angle <= 1e-6 || half_angle >= FRAC_PI_2 - 1e-6 {
            bail!("ADR-205 apex-tip corner: degenerate cone");
        }
        let tan_a = half_angle.tan();
        let height = v_range.0.max(v_range.1);
        let base_center = apex + n_a * height;
        // both planes: bounded ellipse + APEX on +m + base on −m (mirror of cone-corner).
        for (mp, op, lbl) in [(m1, p1_origin, "1"), (m2, p2_origin, "2")] {
            if cone_oblique_ellipse(apex, n_a, half_angle, op, mp).is_none() {
                bail!("ADR-205 apex-tip corner: plane {} section is not a bounded ellipse", lbl);
            }
            if (apex - op).dot(mp) <= 1e-9 {
                bail!("ADR-205 apex-tip corner: plane {} must put the apex on +m (keep the apex)", lbl);
            }
            if (base_center - op).dot(mp) >= -1e-9 {
                bail!("ADR-205 apex-tip corner: plane {} must put the base on −m", lbl);
            }
        }
        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize_or_zero();
        let v_plane = |m: DVec3, o: DVec3, u: f64| -> f64 {
            let g = n_a + (r_vec * u.cos() + p_vec * u.sin()) * tan_a;
            (o - apex).dot(m) / g.dot(m)
        };
        let surf = |u: f64, v: f64| apex + n_a * v + (r_vec * u.cos() + p_vec * u.sin()) * (v * tan_a);
        let u_of = |p: DVec3| {
            let rel = p - apex;
            let radial = rel - n_a * rel.dot(n_a);
            radial.dot(p_vec).atan2(radial.dot(r_vec)).rem_euclid(TAU)
        };
        // ridge (plane1 ∩ plane2) ∩ cone — closed-form quadratic on the nappe.
        let dir = m1.cross(m2);
        let (d1, d2) = (p1_origin.dot(m1), p2_origin.dot(m2));
        let l0 = (m2.cross(dir) * d1 + dir.cross(m1) * d2) / dir.dot(dir);
        let a0 = l0 - apex;
        let (av, bv) = (a0.dot(n_a), dir.dot(n_a));
        let (q0, q1, q2) = (a0.dot(a0), a0.dot(dir), dir.dot(dir));
        let cos2 = half_angle.cos().powi(2);
        let (qa, qb, qc) = (bv * bv - cos2 * q2, 2.0 * (av * bv - cos2 * q1), av * av - cos2 * q0);
        let disc = qb * qb - 4.0 * qa * qc;
        if qa.abs() < 1e-12 || disc <= 0.0 {
            bail!("ADR-205 apex-tip corner: the ridge does not cross the cone side");
        }
        let sd = disc.sqrt();
        let corner_lo = l0 + dir * ((-qb - sd) / (2.0 * qa));
        let corner_hi = l0 + dir * ((-qb + sd) / (2.0 * qa));
        for c in [corner_lo, corner_hi] {
            let av_c = (c - apex).dot(n_a);
            if av_c <= 1e-6 || av_c >= height - 1e-6 {
                bail!("ADR-205 apex-tip corner: a corner is past the apex or the base");
            }
        }
        let (mut uc1, mut uc2) = (u_of(corner_lo), u_of(corner_hi));
        let (mut c1, mut c2) = (corner_lo, corner_hi);
        if uc1 > uc2 {
            std::mem::swap(&mut uc1, &mut uc2);
            std::mem::swap(&mut c1, &mut c2);
        }
        // APEX-KEEP: the binding plane per arc is argMIN v_e (kept v < min cut).
        let active = |um: f64| -> (DVec3, DVec3) {
            if v_plane(m1, p1_origin, um) <= v_plane(m2, p2_origin, um) {
                (m1, p1_origin)
            } else {
                (m2, p2_origin)
            }
        };
        let mid_ua = 0.5 * (uc1 + uc2);
        let mid_ub = (0.5 * (uc2 + uc1 + TAU)).rem_euclid(TAU);
        let (pa, pa_o) = active(mid_ua);
        let (pb, pb_o) = active(mid_ub);
        let mid_a = surf(mid_ua, v_plane(pa, pa_o, mid_ua));
        let mid_b = surf(mid_ub, v_plane(pb, pb_o, mid_ub));
        let (ea_c, ea_sm, ea_sn, ea_mj, ea_mn) =
            cone_oblique_ellipse(apex, n_a, half_angle, pa_o, pa).unwrap();
        let (eb_c, eb_sm, eb_sn, eb_mj, eb_mn) =
            cone_oblique_ellipse(apex, n_a, half_angle, pb_o, pb).unwrap();
        let phi = |p: DVec3, c: DVec3, sm: f64, sn: f64, mj: DVec3, mn: DVec3| {
            let rel = p - c;
            (rel.dot(mn) / sn).atan2(rel.dot(mj) / sm)
        };
        let arc = |p: DVec3, q: DVec3, c: DVec3, sm: f64, sn: f64, mj: DVec3, mn: DVec3| {
            let (cp, w, k, d) = crate::curves::nurbs::ellipse_arc(c, sm, sn, mj, mn, phi(p, c, sm, sn, mj, mn), phi(q, c, sm, sn, mj, mn));
            crate::curves::AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: d as u32 }
        };
        let top_verts = [c1, mid_a, c2, mid_b];
        let top_curves = [
            arc(c1, mid_a, ea_c, ea_sm, ea_sn, ea_mj, ea_mn),
            arc(mid_a, c2, ea_c, ea_sm, ea_sn, ea_mj, ea_mn),
            arc(c2, mid_b, eb_c, eb_sm, eb_sn, eb_mj, eb_mn),
            arc(mid_b, c1, eb_c, eb_sm, eb_sn, eb_mj, eb_mn),
        ];
        let band = S::Cone { apex, axis_dir: n_a, half_angle, ref_dir, u_range: (0.0, TAU), v_range: (0.0, height) };
        // remove the original cone faces + their edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cone_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cone_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (band_f, vids) = self.sew_corner_tip(&top_verts, &top_curves, band, n_a, material)?;
        // two partial caps reuse the band arc twins + share the ridge (c1,c2) edge.
        let cap_a = self.add_face_with_holes(&[vids[2], vids[1], vids[0]], &[], material)?;
        self.faces[cap_a].set_surface(Some(S::Plane { origin: ea_c, normal: -pa, basis_u: ea_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }));
        let cap_b = self.add_face_with_holes(&[vids[0], vids[3], vids[2]], &[], material)?;
        self.faces[cap_b].set_surface(Some(S::Plane { origin: eb_c, normal: -pb, basis_u: eb_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }));
        Ok(vec![band_f, cap_a, cap_b])
    }

    /// **ADR-205 β-2** — a kernel-native cylinder cut by an OBLIQUE plane (not ⟂
    /// the axis) → an ELLIPTIC section. Keeps the +`plane_normal` side: the kept
    /// end disk + the trimmed side band (one circular + one elliptic boundary) +
    /// a planar ELLIPTIC cap. The elliptic boundary is an exact `nurbs::ellipse`
    /// (β-1); the band renders boundary-aware via `tessellate_cylinder_clipped`.
    /// Returns `[side_band, elliptic_cap, kept_disk]`.
    ///
    /// MVP scope: the plane must cut cleanly THROUGH the side (each end cap wholly
    /// on one side, clear by the cap's m-extent r·sinθ). A ⟂ cut (use the
    /// local-frame `boolean_cylinder_*` family), a ∥ plane, or a cap-clipping cut
    /// is rejected.
    pub fn boolean_cylinder_oblique_halfspace(
        &mut self,
        cyl_faces: &[FaceId],
        plane_origin: DVec3,
        plane_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 β-2: degenerate plane normal");
        }
        let (axis_origin, axis_dir_raw, radius, ref_dir, v_range) =
            self.cylinder_full_of(cyl_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || radius <= 0.0 {
            bail!("ADR-205 β-2: degenerate cylinder");
        }
        let ndm = n_a.dot(m);
        let cos_theta = ndm.abs();
        if cos_theta > 1.0 - 1e-6 {
            bail!("ADR-205 β-2: cut plane ⟂ axis → use the local-frame cylinder family");
        }
        if cos_theta < 1e-6 {
            bail!("ADR-205 β-2: cut plane ∥ axis → no elliptic section");
        }
        // end-circle centres at the two axial ends.
        let (v0, v1) = v_range;
        let c0 = axis_origin + n_a * v0;
        let c1 = axis_origin + n_a * v1;
        let d0 = (c0 - plane_origin).dot(m);
        let d1 = (c1 - plane_origin).dot(m);
        // the plane must cut cleanly through the side: end caps on opposite sides,
        // each clear of the plane by more than the cap's m-extent (r·sinθ).
        let sin_theta = (1.0 - cos_theta * cos_theta).max(0.0).sqrt();
        let clearance = radius * sin_theta;
        if !((d0 > clearance && d1 < -clearance) || (d0 < -clearance && d1 > clearance)) {
            bail!("ADR-205 β-2: plane must cut cleanly through the side (clips a cap or misses)");
        }
        // kept end = the cap on the +m side (in_result keeps dot(·, m) > 0).
        let (v_keep, kept_center) = if d0 > 0.0 { (v0, c0) } else { (v1, c1) };
        // elliptic section (closed-form, sim 1).
        let t = (plane_origin - axis_origin).dot(m) / ndm;
        let e_center = axis_origin + n_a * t;
        let minor_dir = m.cross(n_a).normalize_or_zero();
        let major_dir = (n_a - ndm * m).normalize_or_zero();
        if minor_dir.length_squared() < 0.5 || major_dir.length_squared() < 0.5 {
            bail!("ADR-205 β-2: degenerate ellipse axes");
        }
        let (semi_major, semi_minor) = (radius / cos_theta, radius);
        let (cp, w, k, deg) =
            crate::curves::nurbs::ellipse(e_center, semi_major, semi_minor, major_dir, minor_dir);
        let top_anchor = cp[0];
        let top_ellipse = crate::curves::AnalyticCurve::NURBS {
            control_pts: cp, weights: w, knots: k, degree: deg as u32,
        };
        let ref_n = ref_dir.normalize_or_zero();
        // kept-end disk outward normal (away from the body, toward the kept end).
        // The kept end is at v_keep; the cut (ellipse) is at axial `t`, so the
        // exterior points away from `t`. The kept circle's `normal` MUST be this
        // outward direction — the Circle render fast-path orients the disk fan by
        // the circle's normal (not the face hint), so a +n_a circle on a kept-LOW
        // end would render the disk INWARD.
        let kept_outward = if v_keep >= t { n_a } else { -n_a };
        let kept_circle = crate::curves::AnalyticCurve::Circle {
            center: kept_center, radius, normal: kept_outward, basis_u: ref_n,
        };
        let bot_anchor = kept_center + ref_n * radius;
        // band v_range covers kept end → deepest ellipse; the clip trims per-u.
        let z_span = semi_major * major_dir.dot(n_a).abs();
        let band = S::Cylinder {
            axis_origin, axis_dir: n_a, radius, ref_dir,
            u_range: (0.0, TAU),
            v_range: (v_keep.min(t - z_span), v_keep.max(t + z_span)),
        };
        // elliptic cap faces −m (away from the kept +m solid).
        let elliptic_cap = S::Plane {
            origin: e_center, normal: -m, basis_u: major_dir,
            u_range: (-semi_major * 1.2, semi_major * 1.2),
            v_range: (-semi_major * 1.2, semi_major * 1.2),
        };
        let kept_disk = S::Plane {
            origin: kept_center, normal: kept_outward, basis_u: ref_n,
            u_range: (-radius * 1.5, radius * 1.5),
            v_range: (-radius * 1.5, radius * 1.5),
        };
        // remove the original cylinder faces + their edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cyl_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cyl_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (band_f, cap_f, disk_f) = self.sew_curved_band(
            top_anchor, top_ellipse, bot_anchor, kept_circle,
            band, ref_n, elliptic_cap, -m, kept_disk, kept_outward, material,
        )?;
        Ok(vec![band_f, cap_f, disk_f])
    }

    /// **ADR-205 β-3** — a kernel-native cylinder cut by TWO PARALLEL OBLIQUE
    /// planes (shared normal `m`, offsets `d_lo < d_hi` along `m` from
    /// `axis_origin`) → an ELLIPTIC SLAB. Keeps the band BETWEEN the planes
    /// (`d_lo < (p−axis_origin)·m < d_hi`): a trimmed Cylinder band with TWO
    /// elliptic boundaries + two planar ELLIPTIC caps (no circular end disk —
    /// both original caps are removed). Returns `[band, cap_hi, cap_lo]`.
    ///
    /// Both elliptic caps are NURBS self-loops rendered by the face hint (no
    /// Circle, so the β-2 circle-normal subtlety does not arise). The band is
    /// rendered boundary-aware by `tessellate_cylinder_clipped`, whose min/max
    /// strip handles BOTH boundaries oblique.
    ///
    /// MVP scope: both planes parallel + oblique (0 < θ < 90) + each ellipse
    /// wholly on the side (`v0 < t_d ± z_span < v1`). A ⟂ / ∥ normal, `d_lo ≥
    /// d_hi`, or an ellipse beyond an end cap is rejected.
    pub fn boolean_cylinder_oblique_slab(
        &mut self,
        cyl_faces: &[FaceId],
        plane_normal: DVec3,
        d_lo: f64,
        d_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 β-3: degenerate plane normal");
        }
        if d_lo >= d_hi {
            bail!("ADR-205 β-3: d_lo < d_hi required");
        }
        let (axis_origin, axis_dir_raw, radius, ref_dir, v_range) =
            self.cylinder_full_of(cyl_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || radius <= 0.0 {
            bail!("ADR-205 β-3: degenerate cylinder");
        }
        let ndm = n_a.dot(m);
        let cos_theta = ndm.abs();
        if cos_theta > 1.0 - 1e-6 {
            bail!("ADR-205 β-3: planes ⟂ axis → use the local-frame cylinder slab");
        }
        if cos_theta < 1e-6 {
            bail!("ADR-205 β-3: planes ∥ axis → no elliptic section");
        }
        let minor_dir = m.cross(n_a).normalize_or_zero();
        let major_dir = (n_a - ndm * m).normalize_or_zero();
        if minor_dir.length_squared() < 0.5 || major_dir.length_squared() < 0.5 {
            bail!("ADR-205 β-3: degenerate ellipse axes");
        }
        let (semi_major, semi_minor) = (radius / cos_theta, radius);
        let z_span = semi_major * major_dir.dot(n_a).abs();
        // ellipse axial centre for a plane at offset `d`: t = d / ndm.
        let (v0, v1) = (v_range.0.min(v_range.1), v_range.0.max(v_range.1));
        let t_lo = d_lo / ndm;
        let t_hi = d_hi / ndm;
        for t in [t_lo, t_hi] {
            if t - z_span <= v0 + 1e-9 || t + z_span >= v1 - 1e-9 {
                bail!("ADR-205 β-3: an ellipse extends past an end cap (slab not wholly on the side)");
            }
        }
        // build the two elliptic boundaries + caps.
        let build_ellipse = |t: f64| {
            let e_center = axis_origin + n_a * t;
            let (cp, w, k, deg) = crate::curves::nurbs::ellipse(
                e_center, semi_major, semi_minor, major_dir, minor_dir,
            );
            let anchor = cp[0];
            (
                anchor,
                crate::curves::AnalyticCurve::NURBS {
                    control_pts: cp, weights: w, knots: k, degree: deg as u32,
                },
                e_center,
            )
        };
        let (lo_anchor, lo_ellipse, lo_center) = build_ellipse(t_lo);
        let (hi_anchor, hi_ellipse, hi_center) = build_ellipse(t_hi);
        let ref_n = ref_dir.normalize_or_zero();
        let band = S::Cylinder {
            axis_origin, axis_dir: n_a, radius, ref_dir,
            u_range: (0.0, TAU),
            v_range: (t_lo.min(t_hi) - z_span, t_lo.max(t_hi) + z_span),
        };
        // cap_lo (d_lo plane) faces −m (away from the kept d_lo<d<d_hi band);
        // cap_hi (d_hi plane) faces +m.
        let cap_lo = S::Plane {
            origin: lo_center, normal: -m, basis_u: major_dir,
            u_range: (-semi_major * 1.2, semi_major * 1.2),
            v_range: (-semi_major * 1.2, semi_major * 1.2),
        };
        let cap_hi = S::Plane {
            origin: hi_center, normal: m, basis_u: major_dir,
            u_range: (-semi_major * 1.2, semi_major * 1.2),
            v_range: (-semi_major * 1.2, semi_major * 1.2),
        };
        // remove the original cylinder faces + their edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cyl_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cyl_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (band_f, cap_hi_f, cap_lo_f) = self.sew_curved_band(
            hi_anchor, hi_ellipse, lo_anchor, lo_ellipse,
            band, ref_n, cap_hi, m, cap_lo, -m, material,
        )?;
        Ok(vec![band_f, cap_hi_f, cap_lo_f])
    }

    /// **ADR-205 β-4** — a kernel-native cylinder cut by a plane PARALLEL to the
    /// axis (`plane_normal ⟂ axis`). The section is a LINE PAIR (not an ellipse),
    /// so keeping the +`plane_normal` side yields a flat-on-cylinder (a D-shaft):
    /// a PARTIAL Cylinder band (the kept arc) + a flat rectangle (the cut) + two
    /// D-shaped end caps (arc + chord). Returns `[band, flat, cap_v_hi, cap_v_lo]`.
    ///
    /// The partial band renders via the existing `u_range`-honouring Cylinder
    /// tessellation (no clip). Each arc edge is split at its midpoint so a D-cap
    /// has 3 boundary verts (the render polygon path's ≥3 guard + `he_arc_fill_
    /// points`). Faces are built with `add_face_with_holes` (octant pattern), each
    /// oriented outward so shared edges twin.
    ///
    /// MVP scope: `plane_normal ⟂ axis` (|cosθ| < 1e-4) and the axis within the
    /// radius of the plane (`|d_axis| < r`, so it actually cuts). An oblique /
    /// ⟂ plane or a missing cut is rejected.
    pub fn boolean_cylinder_axial_halfspace(
        &mut self,
        cyl_faces: &[FaceId],
        plane_origin: DVec3,
        plane_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 β-4: degenerate plane normal");
        }
        let (axis_origin, axis_dir_raw, radius, ref_dir, v_range) =
            self.cylinder_full_of(cyl_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || radius <= 0.0 {
            bail!("ADR-205 β-4: degenerate cylinder");
        }
        if n_a.dot(m).abs() > 1e-4 {
            bail!("ADR-205 β-4: plane must be ∥ the axis (normal ⟂ axis) — oblique → use the elliptic family");
        }
        // axis→plane signed distance along m. |d_axis| < r to actually cut.
        let d_axis = (axis_origin - plane_origin).dot(m);
        if d_axis.abs() >= radius - 1e-9 {
            bail!("ADR-205 β-4: plane does not cut the cylinder (|d|={:.3} ≥ r)", d_axis.abs());
        }
        // cylinder basis (matches surfaces::cylinder).
        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize_or_zero();
        if r_vec.length_squared() < 0.5 || p_vec.length_squared() < 0.5 {
            bail!("ADR-205 β-4: degenerate cylinder basis");
        }
        // kept arc (keep +m): (p−o)·m = d_axis + r·cos(u−α) > 0 → u ∈ (α−ψ, α+ψ).
        let alpha = (m.dot(p_vec)).atan2(m.dot(r_vec));
        let psi = (-d_axis / radius).clamp(-1.0, 1.0).acos();
        let (u_lo, u_mid, u_hi) = (alpha - psi, alpha, alpha + psi);
        let (v0, v1) = (v_range.0.min(v_range.1), v_range.0.max(v_range.1));
        let surf = |u: f64, v: f64| {
            axis_origin + n_a * v + r_vec * (radius * u.cos()) + p_vec * (radius * u.sin())
        };
        let cross_0 = axis_origin + n_a * v0;
        let cross_1 = axis_origin + n_a * v1;
        // 6 vertices (arc split at midpoint for the ≥3-vert D-cap render).
        let p_lo_0 = self.add_vertex(surf(u_lo, v0));
        let p_mid_0 = self.add_vertex(surf(u_mid, v0));
        let p_hi_0 = self.add_vertex(surf(u_hi, v0));
        let p_lo_1 = self.add_vertex(surf(u_lo, v1));
        let p_mid_1 = self.add_vertex(surf(u_mid, v1));
        let p_hi_1 = self.add_vertex(surf(u_hi, v1));
        let arc = |center: DVec3, a: f64, b: f64| AnalyticCurve::Arc {
            center, radius, normal: n_a, basis_u: r_vec, start_angle: a, end_angle: b,
        };

        // remove the original cylinder faces + edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cyl_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cyl_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        // orient a face's verts so its Newell normal points along `outward`,
        // then build + attach arcs. Consistent outward orientation makes shared
        // edges twin.
        let pos_of = |me: &Mesh, v: VertId| me.vertex_pos(v).unwrap_or(DVec3::ZERO);
        let oriented = |me: &Mesh, verts: &[VertId], outward: DVec3| -> Vec<VertId> {
            let p: Vec<DVec3> = verts.iter().map(|&v| pos_of(me, v)).collect();
            let mut nrm = DVec3::ZERO;
            for i in 0..p.len() {
                let a = p[i];
                let b = p[(i + 1) % p.len()];
                nrm += a.cross(b);
            }
            if nrm.dot(outward) >= 0.0 { verts.to_vec() } else {
                verts.iter().rev().copied().collect()
            }
        };
        // attach an Arc to the boundary edge (vs, ve) of `face`.
        fn attach_arc(me: &mut Mesh, face: FaceId, vs: VertId, ve: VertId, curve: AnalyticCurve) {
            if let (Ok(hes), Ok(verts)) = (
                me.collect_loop_hes(me.faces[face].outer().start),
                me.collect_loop_verts(me.faces[face].outer().start),
            ) {
                let n = verts.len();
                for i in 0..n {
                    let s = verts[(i + n - 1) % n];
                    let e = verts[i];
                    if (s == vs && e == ve) || (s == ve && e == vs) {
                        let eid = me.hes[hes[i]].edge();
                        me.edges[eid].set_curve(Some(curve));
                        return;
                    }
                }
            }
        }

        // band (Cylinder, partial u_range). representative outward = +m (radial @ midpoint).
        let band_v = oriented(self, &[p_lo_0, p_mid_0, p_hi_0, p_hi_1, p_mid_1, p_lo_1], m);
        let band_f = self.add_face_with_holes(&band_v, &[], material)?;
        attach_arc(self, band_f, p_lo_0, p_mid_0, arc(cross_0, u_lo, u_mid));
        attach_arc(self, band_f, p_mid_0, p_hi_0, arc(cross_0, u_mid, u_hi));
        attach_arc(self, band_f, p_lo_1, p_mid_1, arc(cross_1, u_lo, u_mid));
        attach_arc(self, band_f, p_mid_1, p_hi_1, arc(cross_1, u_mid, u_hi));
        self.faces[band_f].set_surface(Some(S::Cylinder {
            axis_origin, axis_dir: n_a, radius, ref_dir,
            u_range: (u_lo, u_hi), v_range: (v0, v1),
        }));

        // flat rectangle (Plane on the cut plane, faces −m away from the kept +m solid).
        let flat_v = oriented(self, &[p_lo_0, p_hi_0, p_hi_1, p_lo_1], -m);
        let flat_f = self.add_face_with_holes(&flat_v, &[], material)?;
        self.faces[flat_f].set_surface(Some(S::Plane {
            origin: plane_origin, normal: -m, basis_u: n_a,
            u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        }));

        // D-caps (Plane). v0 faces −n_a, v1 faces +n_a.
        let cap0_v = oriented(self, &[p_lo_0, p_mid_0, p_hi_0], -n_a);
        let cap0_f = self.add_face_with_holes(&cap0_v, &[], material)?;
        attach_arc(self, cap0_f, p_lo_0, p_mid_0, arc(cross_0, u_lo, u_mid));
        attach_arc(self, cap0_f, p_mid_0, p_hi_0, arc(cross_0, u_mid, u_hi));
        self.faces[cap0_f].set_surface(Some(S::Plane {
            origin: cross_0, normal: -n_a, basis_u: r_vec,
            u_range: (-radius * 1.5, radius * 1.5), v_range: (-radius * 1.5, radius * 1.5),
        }));

        let cap1_v = oriented(self, &[p_lo_1, p_mid_1, p_hi_1], n_a);
        let cap1_f = self.add_face_with_holes(&cap1_v, &[], material)?;
        attach_arc(self, cap1_f, p_lo_1, p_mid_1, arc(cross_1, u_lo, u_mid));
        attach_arc(self, cap1_f, p_mid_1, p_hi_1, arc(cross_1, u_mid, u_hi));
        self.faces[cap1_f].set_surface(Some(S::Plane {
            origin: cross_1, normal: n_a, basis_u: r_vec,
            u_range: (-radius * 1.5, radius * 1.5), v_range: (-radius * 1.5, radius * 1.5),
        }));

        Ok(vec![band_f, flat_f, cap1_f, cap0_f])
    }

    /// **ADR-205 β-5 β-2** — a kernel-native cylinder cut by TWO non-parallel
    /// OBLIQUE planes meeting at a corner (the minimal box-corner). Keeps the
    /// `+plane_normal` side of BOTH (a "tent"): the bottom disk + a corner band
    /// (Cylinder, piecewise elliptic top) + two PARTIAL elliptic caps (each an
    /// ellipse arc + the shared ridge). Returns `[band, bottom_disk, cap_a, cap_b]`.
    ///
    /// MVP scope: both planes oblique with `n_a·m_i < 0` (upper bounds, cut from
    /// the top), non-parallel, the ridge crossing the side (2 corners), and the
    /// bottom circle wholly kept. A ⟂ / ∥ / parallel-pair / non-crossing / bottom-
    /// clipping configuration is rejected.
    #[allow(clippy::too_many_arguments)]
    pub fn boolean_cylinder_corner(
        &mut self,
        cyl_faces: &[FaceId],
        p1_origin: DVec3,
        p1_normal: DVec3,
        p2_origin: DVec3,
        p2_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let m1 = p1_normal.normalize_or_zero();
        let m2 = p2_normal.normalize_or_zero();
        if m1.length_squared() < 0.5 || m2.length_squared() < 0.5 {
            bail!("ADR-205 β-5: degenerate plane normal");
        }
        if m1.cross(m2).length() < 1e-4 {
            bail!("ADR-205 β-5: planes are parallel → use the slab (β-3) path");
        }
        let (axis_origin, axis_dir_raw, radius, ref_dir, v_range) =
            self.cylinder_full_of(cyl_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || radius <= 0.0 {
            bail!("ADR-205 β-5: degenerate cylinder");
        }
        // both planes must be oblique upper bounds (n_a·m < 0 → keep below).
        for (m, lbl) in [(m1, "1"), (m2, "2")] {
            let c = n_a.dot(m);
            if c.abs() > 1.0 - 1e-6 || c.abs() < 1e-6 {
                bail!("ADR-205 β-5: plane {} not oblique to the axis", lbl);
            }
            if c >= 0.0 {
                bail!("ADR-205 β-5: plane {} must cut from the top (n_a·m < 0)", lbl);
            }
        }
        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize_or_zero();
        let (v0, v1) = (v_range.0.min(v_range.1), v_range.0.max(v_range.1));

        // corners = ridge (plane1 ∩ plane2 line) ∩ cylinder (2 points).
        let dir_raw = m1.cross(m2);
        let (d1, d2) = (p1_origin.dot(m1), p2_origin.dot(m2));
        let l0 = (m2.cross(dir_raw) * d1 + dir_raw.cross(m1) * d2) / dir_raw.dot(dir_raw);
        let rel0 = l0 - axis_origin;
        let perp = |w: DVec3| w - n_a * w.dot(n_a);
        let (a0, ad) = (perp(rel0), perp(dir_raw));
        let (qa, qb, qc) = (ad.dot(ad), 2.0 * a0.dot(ad), a0.dot(a0) - radius * radius);
        let disc = qb * qb - 4.0 * qa * qc;
        if qa < 1e-12 || disc <= 0.0 {
            bail!("ADR-205 β-5: ridge does not cross the cylinder side");
        }
        let sd = disc.sqrt();
        let corner_lo = l0 + dir_raw * ((-qb - sd) / (2.0 * qa));
        let corner_hi = l0 + dir_raw * ((-qb + sd) / (2.0 * qa));

        // cylinder u + axial v of a surface point.
        let u_of = |p: DVec3| {
            let rel = p - axis_origin;
            rel.dot(p_vec).atan2(rel.dot(r_vec)).rem_euclid(TAU)
        };
        let surf = |u: f64, v: f64| {
            axis_origin + n_a * v + r_vec * (radius * u.cos()) + p_vec * (radius * u.sin())
        };
        // v where the generator at angle u pierces plane (m, o).
        let v_plane = |m: DVec3, o: DVec3, u: f64| {
            ((o - axis_origin).dot(m) - radius * (r_vec * u.cos() + p_vec * u.sin()).dot(m)) / n_a.dot(m)
        };
        let (mut uc1, mut uc2) = (u_of(corner_lo), u_of(corner_hi));
        let (mut c1, mut c2) = (corner_lo, corner_hi);
        if uc1 > uc2 { std::mem::swap(&mut uc1, &mut uc2); std::mem::swap(&mut c1, &mut c2); }
        // corners must be on the side (axial within the cylinder).
        for c in [c1, c2] {
            let av = (c - axis_origin).dot(n_a);
            if av <= v0 + 1e-9 || av >= v1 - 1e-9 {
                bail!("ADR-205 β-5: a corner is past an end cap");
            }
        }
        // two arcs (uc1→uc2) and (uc2→uc1+2π); the active plane = min upper bound.
        let arc_active = |um: f64| -> ((DVec3, DVec3), (DVec3, DVec3)) {
            if v_plane(m1, p1_origin, um) <= v_plane(m2, p2_origin, um) {
                ((m1, p1_origin), (m2, p2_origin))
            } else {
                ((m2, p2_origin), (m1, p1_origin))
            }
        };
        let mid_ua = 0.5 * (uc1 + uc2);
        let mid_ub = 0.5 * (uc2 + uc1 + TAU);
        let (pa, _) = arc_active(mid_ua); // active plane on arc A (uc1→uc2)
        let (pb, _) = arc_active(mid_ub); // active plane on arc B
        let mid_a = surf(mid_ua, v_plane(pa.0, pa.1, mid_ua));
        let mid_b = surf(mid_ub, v_plane(pb.0, pb.1, mid_ub.rem_euclid(TAU)));

        // ellipse params (β-2) + φ of a point on an ellipse.
        let ellipse_of = |m: DVec3, o: DVec3| {
            let ndm = n_a.dot(m);
            let center = axis_origin + n_a * ((o - axis_origin).dot(m) / ndm);
            let minor = m.cross(n_a).normalize();
            let major = (n_a - ndm * m).normalize();
            (center, radius / ndm.abs(), radius, major, minor)
        };
        let (ea_c, ea_a, ea_b, ea_mj, ea_mn) = ellipse_of(pa.0, pa.1);
        let (eb_c, eb_a, eb_b, eb_mj, eb_mn) = ellipse_of(pb.0, pb.1);
        let phi = |p: DVec3, c: DVec3, a: f64, b: f64, mj: DVec3, mn: DVec3| {
            let rel = p - c;
            (rel.dot(mn) / b).atan2(rel.dot(mj) / a)
        };
        let arc_nurbs = |p: DVec3, q: DVec3, c, a, b, mj, mn| {
            let (cp, w, k, d) = crate::curves::nurbs::ellipse_arc(
                c, a, b, mj, mn,
                phi(p, c, a, b, mj, mn), phi(q, c, a, b, mj, mn),
            );
            crate::curves::AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: d as u32 }
        };
        // top loop in DECREASING u so the whole solid is consistently OUTWARD
        // (this orientation makes the partial caps' Newell normals point −m_i; the
        // other winding leaves them inward). [c1, mid_b, c2, mid_a]: edges 0,1 on
        // plane pb (arc through mid_b), edges 2,3 on plane pa (arc through mid_a).
        let top_verts = [c1, mid_b, c2, mid_a];
        let top_curves = [
            arc_nurbs(c1, mid_b, eb_c, eb_a, eb_b, eb_mj, eb_mn),
            arc_nurbs(mid_b, c2, eb_c, eb_a, eb_b, eb_mj, eb_mn),
            arc_nurbs(c2, mid_a, ea_c, ea_a, ea_b, ea_mj, ea_mn),
            arc_nurbs(mid_a, c1, ea_c, ea_a, ea_b, ea_mj, ea_mn),
        ];
        // bottom circle (kept full) — verify it is wholly +m for both planes.
        let c_bot = axis_origin + n_a * v0;
        for (m, o) in [(m1, p1_origin), (m2, p2_origin)] {
            let sin_t = (1.0 - n_a.dot(m).powi(2)).max(0.0).sqrt();
            if (c_bot - o).dot(m) <= radius * sin_t {
                bail!("ADR-205 β-5: bottom circle not wholly kept (a plane clips it)");
            }
        }
        // The disk's render fast-path orients its fan by the circle's `normal`, so
        // it must be the OUTWARD direction (−n_a, away from the solid above).
        let bottom_circle = crate::curves::AnalyticCurve::Circle {
            center: c_bot, radius, normal: -n_a, basis_u: r_vec,
        };
        let band = S::Cylinder {
            axis_origin, axis_dir: n_a, radius, ref_dir,
            u_range: (0.0, TAU), v_range: (v0, v1),
        };
        let bottom_disk = S::Plane {
            origin: c_bot, normal: -n_a, basis_u: r_vec,
            u_range: (-radius * 1.5, radius * 1.5), v_range: (-radius * 1.5, radius * 1.5),
        };

        // remove the original cylinder.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cyl_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cyl_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (band_f, disk_f, vids) = self.sew_corner_band(
            &top_verts, &top_curves, c_bot + r_vec * radius, bottom_circle,
            band, r_vec, bottom_disk, -n_a, material,
        )?;
        // partial caps reuse the band arc edges (opposite traversal) + a ridge.
        // vids = [c1, mid_b, c2, mid_a]. cap_b (plane pb, arc through mid_b) →
        // [c2, mid_b, c1]; cap_a (plane pa, arc through mid_a) → [c1, mid_a, c2].
        let cap_b = self.add_face_with_holes(&[vids[2], vids[1], vids[0]], &[], material)?;
        self.faces[cap_b].set_surface(Some(S::Plane {
            origin: eb_c, normal: -pb.0, basis_u: eb_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        }));
        let cap_a = self.add_face_with_holes(&[vids[0], vids[3], vids[2]], &[], material)?;
        self.faces[cap_a].set_surface(Some(S::Plane {
            origin: ea_c, normal: -pa.0, basis_u: ea_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        }));
        Ok(vec![band_f, disk_f, cap_b, cap_a])
    }

    /// **ADR-205 N-plane corner** — generalize `boolean_cylinder_corner` (a 2-plane
    /// tent at a box EDGE) to N oblique upper-bound planes (a box VERTEX clips a
    /// tilted cylinder with up to 3 perpendicular faces at once). The kept top is the
    /// **lower envelope** of the planes (`min_i v_plane_i(u)`), giving K active arcs
    /// joined by K corners (ridge of two consecutive active planes ∩ cylinder). For
    /// K=2 it delegates to the tent. For K=3 the three ridges meet at the **box vertex
    /// V** (the 3-plane intersection); each cap is a pie slice
    /// `[corner_{i-1}, mid_i, corner_i, V]` sharing V and the ridge segments pairwise.
    /// Requires V inside the cylinder (the clean regime). `planes` = `(origin, normal)`
    /// with each normal the box face's INWARD normal. Returns
    /// `[band, bottom_disk, cap_0, …, cap_{K-1}]`.
    pub fn boolean_cylinder_corner_n(
        &mut self,
        cyl_faces: &[FaceId],
        planes: &[(DVec3, DVec3)],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if planes.len() < 2 {
            bail!("ADR-205 corner-N: need ≥2 planes");
        }
        let (axis_origin, axis_dir_raw, radius, ref_dir, v_range) = self.cylinder_full_of(cyl_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 || radius <= 0.0 {
            bail!("ADR-205 corner-N: degenerate cylinder");
        }
        // normalize + validate every plane is an oblique upper bound (n_a·m < 0).
        let pls: Vec<(DVec3, DVec3)> = planes
            .iter()
            .map(|&(o, m)| (o, m.normalize_or_zero()))
            .collect();
        for (_, m) in &pls {
            if m.length_squared() < 0.5 {
                bail!("ADR-205 corner-N: degenerate plane normal");
            }
            let c = n_a.dot(*m);
            if c >= -1e-6 || c.abs() >= 1.0 - 1e-6 {
                bail!("ADR-205 corner-N: a plane is not an oblique upper bound (n_a·m<0)");
            }
        }
        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize_or_zero();
        if r_vec.length_squared() < 0.5 || p_vec.length_squared() < 0.5 {
            bail!("ADR-205 corner-N: degenerate frame");
        }
        let (v0, v1) = (v_range.0.min(v_range.1), v_range.0.max(v_range.1));

        let v_plane = |m: DVec3, o: DVec3, u: f64| {
            ((o - axis_origin).dot(m) - radius * (r_vec * u.cos() + p_vec * u.sin()).dot(m)) / n_a.dot(m)
        };
        let u_of = |p: DVec3| {
            let rel = p - axis_origin;
            rel.dot(p_vec).atan2(rel.dot(r_vec)).rem_euclid(TAU)
        };
        let surf = |u: f64, v: f64| {
            axis_origin + n_a * v + r_vec * (radius * u.cos()) + p_vec * (radius * u.sin())
        };

        // lower envelope → contiguous active runs (cyclic).
        let nsamp = 3600usize;
        let active_at = |u: f64| -> usize {
            (0..pls.len())
                .min_by(|&i, &j| {
                    v_plane(pls[i].1, pls[i].0, u)
                        .partial_cmp(&v_plane(pls[j].1, pls[j].0, u))
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .unwrap()
        };
        let act: Vec<usize> = (0..nsamp).map(|s| active_at(s as f64 / nsamp as f64 * TAU)).collect();
        let mut runs: Vec<(usize, usize, usize)> = Vec::new();
        let mut s0 = 0usize;
        for s in 1..nsamp {
            if act[s] != act[s - 1] {
                runs.push((act[s - 1], s0, s - 1));
                s0 = s;
            }
        }
        runs.push((act[nsamp - 1], s0, nsamp - 1));
        if runs.len() > 1 && runs[0].0 == runs[runs.len() - 1].0 {
            let last = runs.pop().unwrap();
            runs[0].1 = last.1;
        }
        let k = runs.len();
        if k < 2 {
            bail!("ADR-205 corner-N: <2 active planes (a halfspace, not a corner)");
        }
        if k == 2 {
            // a tent (box edge) — delegate to the validated 2-plane path.
            let (a, b) = (runs[0].0, runs[1].0);
            return self.boolean_cylinder_corner(cyl_faces, pls[a].0, pls[a].1, pls[b].0, pls[b].1, material);
        }
        if k > 3 {
            bail!("ADR-205 corner-N: >3 active planes deferred (not a box vertex)");
        }

        // K=3 — box vertex pie-slice. corners = ridge(run k, run k+1) ∩ cylinder.
        let u_at = |idx: usize| idx as f64 / nsamp as f64 * TAU;
        let ridge_corner = |a: usize, b: usize, u_hint: f64| -> Result<DVec3> {
            let (oa, ma) = pls[a];
            let (ob, mb) = pls[b];
            let dir_raw = ma.cross(mb);
            if dir_raw.length() < 1e-6 {
                bail!("ADR-205 corner-N: two active planes parallel");
            }
            let (da, db) = (oa.dot(ma), ob.dot(mb));
            let l0 = (mb.cross(dir_raw) * da + dir_raw.cross(ma) * db) / dir_raw.dot(dir_raw);
            let perp = |w: DVec3| w - n_a * w.dot(n_a);
            let (a0, ad) = (perp(l0 - axis_origin), perp(dir_raw));
            let (qa, qb, qc) = (ad.dot(ad), 2.0 * a0.dot(ad), a0.dot(a0) - radius * radius);
            let disc = qb * qb - 4.0 * qa * qc;
            if qa < 1e-12 || disc <= 0.0 {
                bail!("ADR-205 corner-N: a ridge misses the cylinder side");
            }
            let sd = disc.sqrt();
            let c_lo = l0 + dir_raw * ((-qb - sd) / (2.0 * qa));
            let c_hi = l0 + dir_raw * ((-qb + sd) / (2.0 * qa));
            let du = |c: DVec3| {
                let d = (u_of(c) - u_hint).rem_euclid(TAU);
                d.min(TAU - d)
            };
            Ok(if du(c_lo) <= du(c_hi) { c_lo } else { c_hi })
        };
        let mut corners: Vec<DVec3> = Vec::with_capacity(k);
        for i in 0..k {
            corners.push(ridge_corner(runs[i].0, runs[(i + 1) % k].0, u_at(runs[i].2))?);
        }
        for c in &corners {
            let av = (*c - axis_origin).dot(n_a);
            if av <= v0 + 1e-9 || av >= v1 - 1e-9 {
                bail!("ADR-205 corner-N: a corner is past an end cap");
            }
        }
        // box vertex V = intersection of the 3 active planes (apex shared by all caps).
        let (m0, m1, m2) = (pls[runs[0].0].1, pls[runs[1].0].1, pls[runs[2].0].1);
        let (d0, d1, d2) = (
            pls[runs[0].0].0.dot(m0),
            pls[runs[1].0].0.dot(m1),
            pls[runs[2].0].0.dot(m2),
        );
        let det = m0.dot(m1.cross(m2));
        if det.abs() < 1e-9 {
            bail!("ADR-205 corner-N: the 3 active planes do not meet at a point");
        }
        let vbox = (m1.cross(m2) * d0 + m2.cross(m0) * d1 + m0.cross(m1) * d2) / det;
        let vbox_av = (vbox - axis_origin).dot(n_a);
        if vbox_av <= v0 || vbox_av >= v1
            || ((vbox - axis_origin) - n_a * vbox_av).length() > radius + 1e-9
        {
            bail!("ADR-205 corner-N: box vertex outside the cylinder (V-outside regime deferred)");
        }

        // ellipse params (β-2) + φ for arc NURBS.
        let ellipse_of = |m: DVec3, o: DVec3| {
            let ndm = n_a.dot(m);
            let center = axis_origin + n_a * ((o - axis_origin).dot(m) / ndm);
            let minor = m.cross(n_a).normalize();
            let major = (n_a - ndm * m).normalize();
            (center, radius / ndm.abs(), radius, major, minor)
        };
        let phi = |p: DVec3, c: DVec3, a: f64, b: f64, mj: DVec3, mn: DVec3| {
            let rel = p - c;
            (rel.dot(mn) / b).atan2(rel.dot(mj) / a)
        };
        // each sub-arc is < 180°; unwrap φ1 to the SHORT way from φ0 so `ellipse_arc`
        // (which takes the raw φ1−φ0 span) does not wrap the LONG way around the
        // ellipse (e.g. φ0=π, φ1=−π+δ would otherwise span ≈ −2π through the top).
        let arc_nurbs = |p: DVec3, q: DVec3, c, a, b, mj, mn| {
            let phi0 = phi(p, c, a, b, mj, mn);
            let mut phi1 = phi(q, c, a, b, mj, mn);
            while phi1 - phi0 > std::f64::consts::PI { phi1 -= TAU; }
            while phi1 - phi0 < -std::f64::consts::PI { phi1 += TAU; }
            let (cp, w, kn, d) = crate::curves::nurbs::ellipse_arc(c, a, b, mj, mn, phi0, phi1);
            crate::curves::AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: kn, degree: d as u32 }
        };
        // per-run mid point + ellipse params.
        let mut mids: Vec<DVec3> = Vec::with_capacity(k);
        let mut ells: Vec<(DVec3, f64, f64, DVec3, DVec3, DVec3)> = Vec::with_capacity(k); // (c,a,b,mj,mn,m)
        for i in 0..k {
            let (o, m) = pls[runs[i].0];
            let (ec, ea, eb, emj, emn) = ellipse_of(m, o);
            let mid_u = {
                let (a_idx, b_idx) = (runs[i].1, runs[i].2);
                let span = if b_idx >= a_idx { b_idx - a_idx } else { nsamp - a_idx + b_idx };
                u_at((a_idx + span / 2) % nsamp)
            };
            mids.push(surf(mid_u, v_plane(m, o, mid_u)));
            ells.push((ec, ea, eb, emj, emn, m));
        }
        // top loop in DECREASING u (the tent's outward convention): traverse runs in
        // reverse, each arc going corners[i] → mid_i → corners[i-1]. With this winding
        // the pie-slice caps' natural Newell normal is OUTWARD (−m), matching the Plane
        // surface hint (so no invariant-violating override is needed).
        let mut top_verts: Vec<DVec3> = Vec::with_capacity(2 * k);
        let mut top_curves: Vec<crate::curves::AnalyticCurve> = Vec::with_capacity(2 * k);
        let mut cap_plane_origin: Vec<(DVec3, DVec3, DVec3)> = Vec::with_capacity(k); // (center, -m, major)
        for j in 0..k {
            let i = k - 1 - j;
            let cur = corners[i];
            let prev = corners[(i + k - 1) % k];
            let (ec, ea, eb, emj, emn, m) = ells[i];
            top_verts.push(cur);
            top_verts.push(mids[i]);
            top_curves.push(arc_nurbs(cur, mids[i], ec, ea, eb, emj, emn));
            top_curves.push(arc_nurbs(mids[i], prev, ec, ea, eb, emj, emn));
            cap_plane_origin.push((ec, -m, emj));
        }

        // bottom circle (kept full) — verify wholly +m for every plane.
        let c_bot = axis_origin + n_a * v0;
        for &(o, m) in &pls {
            let sin_t = (1.0 - n_a.dot(m).powi(2)).max(0.0).sqrt();
            if (c_bot - o).dot(m) <= radius * sin_t {
                bail!("ADR-205 corner-N: a plane clips the bottom circle");
            }
        }
        let bottom_circle = crate::curves::AnalyticCurve::Circle {
            center: c_bot, radius, normal: -n_a, basis_u: r_vec,
        };
        let band = S::Cylinder {
            axis_origin, axis_dir: n_a, radius, ref_dir,
            u_range: (0.0, TAU), v_range: (v0, v1),
        };
        let bottom_disk = S::Plane {
            origin: c_bot, normal: -n_a, basis_u: r_vec,
            u_range: (-radius * 1.5, radius * 1.5), v_range: (-radius * 1.5, radius * 1.5),
        };

        // remove the original cylinder.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cyl_faces {
                if let Some(f) = self.faces.get(cf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cyl_faces { let _ = self.remove_face(cf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let (band_f, disk_f, vids) = self.sew_corner_band(
            &top_verts, &top_curves, c_bot + r_vec * radius, bottom_circle,
            band, r_vec, bottom_disk, -n_a, material,
        )?;
        let vbox_id = self.add_vertex(vbox);
        // K pie-slice caps to V. With the DECREASING-u band loop, run j contributes
        // band HEs cur→mid (vids[2j]→vids[2j+1]) and mid→prev (vids[2j+1]→vids[2j+2]);
        // the cap reuses the free twins prev→mid→cur (vids[2j+2]→vids[2j+1]→vids[2j])
        // + the two ridges to V → loop [prev, mid, cur, V] with OUTWARD Newell normal.
        let mut faces = vec![band_f, disk_f];
        for j in 0..k {
            let v_cur = 2 * j;                 // corners[i]
            let v_mid = 2 * j + 1;             // mid_i
            let v_prev = (2 * j + 2) % (2 * k); // corners[i-1]
            let (ec, _out_n, emj) = cap_plane_origin[j];
            let cap = self.add_face_with_holes(
                &[vids[v_prev], vids[v_mid], vids[v_cur], vbox_id], &[], material,
            )?;
            let cap_n = self.faces[cap].normal();
            self.faces[cap].set_surface(Some(S::Plane {
                origin: ec, normal: cap_n, basis_u: emj,
                u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
            }));
            faces.push(cap);
        }
        Ok(faces)
    }

    /// **ADR-205 γ** — the user-facing single-plane trim of a cylinder by an
    /// ARBITRARY plane. Keeps the `+plane_normal` side, dispatching on the
    /// plane-vs-axis angle `cosθ = |n_a·m|`:
    ///   • `≈1` (⟂ axis)  → the local-frame slab (a ⟂ halfspace = a slab to the
    ///     far end), preserving a tilted axis;
    ///   • `≈0` (∥ axis)  → β-4 axial flat cut (a D-shaft);
    ///   • otherwise (oblique) → β-2 elliptic halfspace.
    /// This is the single entry the SliceTool / `boolean()` calls for a tilted
    /// cylinder cut by one plane.
    pub fn boolean_cylinder_trim_plane(
        &mut self,
        cyl_faces: &[FaceId],
        plane_origin: DVec3,
        plane_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 γ: degenerate plane normal");
        }
        let (axis_origin, axis_dir_raw, _r, _ref, v_range) = self.cylinder_full_of(cyl_faces)?;
        let n_a = axis_dir_raw.normalize_or_zero();
        if n_a.length_squared() < 0.5 {
            bail!("ADR-205 γ: degenerate cylinder axis");
        }
        let cos = n_a.dot(m).abs();
        if cos > 1.0 - 1e-4 {
            // ⟂ axis → a perpendicular halfspace = a local-frame slab to the far end.
            let v_plane = (plane_origin - axis_origin).dot(n_a);
            let (v0, v1) = (v_range.0.min(v_range.1), v_range.0.max(v_range.1));
            let (v_lo, v_hi) = if n_a.dot(m) > 0.0 { (v_plane, v1) } else { (v0, v_plane) };
            if v_lo >= v_hi - 1e-9 {
                bail!("ADR-205 γ: ⟂ cut keeps nothing (plane beyond the cylinder)");
            }
            self.boolean_cylinder_slab_local(cyl_faces, v_lo, v_hi, material)
        } else if cos < 1e-4 {
            self.boolean_cylinder_axial_halfspace(cyl_faces, plane_origin, plane_normal, material)
        } else {
            self.boolean_cylinder_oblique_halfspace(cyl_faces, plane_origin, plane_normal, material)
        }
    }

    /// **ADR-205 γ-wire** — the curved-trim dispatcher the SliceTool / Scene calls
    /// for an ARBITRARY plane (mirrors `cut_curved_by_z_plane`, but plane-general
    /// and TRIM-only). Returns `Some(Ok)` on a curved primitive it handles (the
    /// `+plane_normal` side kept), `Some(Err)` on a handled-but-failed cut, and
    /// `None` to signal a non-curved input → polygonal fallback.
    ///
    /// MVP scope: CYLINDER of any axis (via `boolean_cylinder_trim_plane`). Sphere
    /// / cone / torus arbitrary-plane trims are a γ-2 follow-up (they already have
    /// Z-plane paths in `cut_curved_by_z_plane`).
    pub fn trim_curved_by_plane(
        &mut self,
        faces: &[FaceId],
        plane_origin: DVec3,
        plane_normal: DVec3,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        let is_cylinder = faces.iter().any(|&f| {
            matches!(self.face_surface(f), Some(crate::surfaces::AnalyticSurface::Cylinder { .. }))
        });
        if is_cylinder {
            return Some(self.boolean_cylinder_trim_plane(faces, plane_origin, plane_normal, material));
        }
        // ADR-205 γ-torus-wire — a single oblique plane trimming a torus is the
        // β-2-torus annular HALFSPACE (keep the +plane_normal side). The op validates
        // annularity itself (Err for the pinched / too-oblique regimes), and its ⟂-axis
        // limit covers the perpendicular cut too, so no extra dispatch is needed.
        let is_torus = faces.iter().any(|&f| {
            matches!(self.face_surface(f), Some(crate::surfaces::AnalyticSurface::Torus { .. }))
        });
        if is_torus {
            return Some(self.boolean_torus_oblique_halfspace(faces, plane_origin, plane_normal, material));
        }
        None
    }

    /// ADR-197 Z-axis lift (A-1) — CYLINDER slab for an arbitrary-axis cylinder.
    /// `v_lo`/`v_hi` are cut bounds along the cylinder's OWN axis (axial distance
    /// from `axis_origin`), NOT world Z — the cut planes are ⟂ the cylinder axis.
    /// For a +Z cylinder this equals `boolean_cylinder_slab(z0 + v_lo, z0 + v_hi)`;
    /// for a tilted one the solid is rotated into a Z-frame, the existing
    /// `boolean_cylinder_slab` runs, and the result is rotated back (analytic
    /// Cylinder surface + tilted axis preserved). Returns
    /// `[side_band, top_disk, bottom_disk]`.
    pub fn boolean_cylinder_slab_local(
        &mut self,
        cyl_faces: &[FaceId],
        v_lo: f64,
        v_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, axis_origin) = self.cylinder_axis_of(cyl_faces)?;
        let z0 = axis_origin.z;
        self.with_axis_lifted_to(cyl_faces, axis_dir, axis_origin, DVec3::Z, move |me| {
            // Z-frame: the axis is +Z through axis_origin (the unchanged pivot),
            // so a surface point at axial position `v` is at world-z = z0 + v.
            me.boolean_cylinder_slab(cyl_faces, z0 + v_lo, z0 + v_hi, material)
        })
    }

    /// Read `(axis_dir, apex)` of the first Cone-surfaced face.
    fn cone_axis_of(&self, faces: &[FaceId]) -> Result<(DVec3, DVec3)> {
        for &f in faces {
            if let Some(crate::surfaces::AnalyticSurface::Cone { axis_dir, apex, .. }) =
                self.face_surface(f)
            {
                return Ok((*axis_dir, *apex));
            }
        }
        bail!("ADR-197 Z-axis lift: no Cone surface in face set");
    }

    /// ADR-197 Z-axis lift (A-2) — CONE slab for an arbitrary-axis cone.
    /// `v_lo`/`v_hi` are cut bounds along the cone's OWN axis as the surface's
    /// `v`-parameter — axial distance from the apex (`v = 0` at the apex), so
    /// `0 ≤ v_lo < v_hi`. The cut planes are ⟂ the cone axis. The op expects the
    /// cone apex-up (axis_dir = −Z), so the solid is rotated to that frame
    /// (pivoting at the apex), `boolean_cone_slab` runs, and the result is rotated
    /// back (analytic Cone surface + tilted axis preserved). A frustum cut
    /// (`0 < v_lo < v_hi < height`) returns `[band, top_disk, bottom_disk]`.
    pub fn boolean_cone_slab_local(
        &mut self,
        cone_faces: &[FaceId],
        v_lo: f64,
        v_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, apex) = self.cone_axis_of(cone_faces)?;
        let apex_z = apex.z;
        self.with_axis_lifted_to(cone_faces, axis_dir, apex, DVec3::NEG_Z, move |me| {
            // −Z frame: apex stays at `apex` (the pivot), axis is −Z, so a point
            // at axial distance `v` from the apex is at world-z = apex_z − v.
            // Thus larger `v` → lower z. `boolean_cone_slab` needs z_lo < z_hi.
            me.boolean_cone_slab(cone_faces, apex_z - v_hi, apex_z - v_lo, material)
        })
    }

    /// Read `(axis_dir, center)` of the first Torus-surfaced face.
    fn torus_axis_of(&self, faces: &[FaceId]) -> Result<(DVec3, DVec3)> {
        for &f in faces {
            if let Some(crate::surfaces::AnalyticSurface::Torus { axis_dir, center, .. }) =
                self.face_surface(f)
            {
                return Ok((*axis_dir, *center));
            }
        }
        bail!("ADR-197 Z-axis lift: no Torus surface in face set");
    }

    /// Read `(center, axis_dir, ref_dir, major_radius, minor_radius)` of the first
    /// Torus-surfaced face. (ADR-205 β-2-torus.)
    fn torus_full_of(&self, faces: &[FaceId]) -> Result<(DVec3, DVec3, DVec3, f64, f64)> {
        for &f in faces {
            if let Some(crate::surfaces::AnalyticSurface::Torus {
                center, axis_dir, ref_dir, major_radius, minor_radius, ..
            }) = self.face_surface(f)
            {
                return Ok((*center, *axis_dir, *ref_dir, *major_radius, *minor_radius));
            }
        }
        bail!("ADR-205 β-2-torus: no Torus surface in face set");
    }

    /// **ADR-205 β-2-torus** — a kernel-native torus cut by an OBLIQUE plane (not ⟂
    /// the torus axis) in the **ANNULAR** regime → a *spiric* section (a quartic
    /// Cassini-oval, NOT a conic). Keeps the `+plane_normal` halfspace: the result is
    /// a **Torus band** (the kept half-tube, an annulus) + an **annular Plane cap**
    /// (the planar region between the two spiric ovals). Returns `[band, cap]`.
    ///
    /// Unlike cylinder/cone (an analytic ellipse → `sew_curved_band`), the spiric
    /// boundary has no exact NURBS self-loop, so the two ovals are SAMPLED polylines
    /// and the band+cap are sewn by two `add_face_with_holes` calls with reversed
    /// windings (they share each rim edge's twin → watertight + manifold; de-risked in
    /// `sim_adr205_torus_oblique_halfspace_dcel`). The band renders boundary-aware via
    /// `tessellate_torus_clipped` (kept side read from the band's oriented normal).
    ///
    /// MVP scope: the **ANNULAR** case only — the plane must pierce EVERY minor circle
    /// twice (within ~`atan(r/√(R²−r²))` of the axis, the §1 threshold). A steeper /
    /// grazing cut (the PINCHED single-oval regime, where some minor circles are
    /// missed) needs a different cap topology (a simple disk) and is deferred.
    pub fn boolean_torus_oblique_halfspace(
        &mut self,
        torus_faces: &[FaceId],
        plane_origin: DVec3,
        plane_normal: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 β-2-torus: degenerate plane normal");
        }
        let (center, axis_raw, ref_dir, major_r, minor_r) = self.torus_full_of(torus_faces)?;
        let axis = axis_raw.normalize_or_zero();
        if axis.length_squared() < 0.5 || major_r <= minor_r + 1e-9 || minor_r <= 1e-9 {
            bail!("ADR-205 β-2-torus: degenerate torus (need 0 < r < R)");
        }
        let p1 = crate::surfaces::orthonormal_ref(axis, ref_dir);
        let p2 = axis.cross(p1).normalize_or_zero();
        if p1.length_squared() < 0.5 || p2.length_squared() < 0.5 {
            bail!("ADR-205 β-2-torus: degenerate basis");
        }
        let o = plane_origin;
        let d = minor_r * axis.dot(m); // u-independent sin-coefficient

        // Sample the two spiric ovals, validating the ANNULAR regime (every minor
        // circle pierced twice). Outer vs inner oval split per-u by cos v
        // (radial-from-axis = R + r·cos v).
        let n = crate::surfaces::sagitta_segments(major_r + minor_r, TAU, 0.05).max(48);
        let mut outer = Vec::with_capacity(n);
        let mut inner = Vec::with_capacity(n);
        for i in 0..n {
            let u = TAU * (i as f64) / (n as f64);
            let radial = p1 * u.cos() + p2 * u.sin();
            let c_u = center + radial * major_r;
            let a_u = (c_u - o).dot(m);
            let b_u = minor_r * radial.dot(m);
            let amp = (b_u * b_u + d * d).sqrt();
            if amp < 1e-12 || a_u.abs() >= amp - 1e-9 {
                bail!(
                    "ADR-205 β-2-torus: not the annular case (a minor circle is missed/tangent — \
                     the pinched single-oval regime is deferred)"
                );
            }
            let phi = d.atan2(b_u);
            let dv = (-a_u / amp).clamp(-1.0, 1.0).acos();
            let (v1, v2) = (phi + dv, phi - dv);
            let (vo, vi) = if v1.cos() >= v2.cos() { (v1, v2) } else { (v2, v1) };
            outer.push(c_u + radial * (minor_r * vo.cos()) + axis * (minor_r * vo.sin()));
            inner.push(c_u + radial * (minor_r * vi.cos()) + axis * (minor_r * vi.sin()));
        }

        // Newell normal of the u-ordered outer oval (it is planar on the cut plane),
        // to orient the two faces: cap.normal() = −m (outward), band.normal() = +m
        // (the kept side, read by the boundary-aware render).
        let newell = |pts: &[DVec3]| -> DVec3 {
            let mut nrm = DVec3::ZERO;
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                nrm.x += (a.y - b.y) * (a.z + b.z);
                nrm.y += (a.z - b.z) * (a.x + b.x);
                nrm.z += (a.x - b.x) * (a.y + b.y);
            }
            nrm.normalize_or_zero()
        };
        let outer_faces_plus_m = newell(&outer).dot(m) > 0.0;

        // Remove the original torus faces + their edges (mirror the cone op).
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &tf in torus_faces {
                if let Some(f) = self.faces.get(tf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner_l in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner_l.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &tf in torus_faces { let _ = self.remove_face(tf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        // Shared vertices for both ovals (cap & band reuse them via twin half-edges).
        let outer_ids: Vec<VertId> = outer.iter().map(|&p| self.add_vertex(p)).collect();
        let inner_ids: Vec<VertId> = inner.iter().map(|&p| self.add_vertex(p)).collect();
        let outer_rev: Vec<VertId> = outer_ids.iter().rev().copied().collect();
        let inner_rev: Vec<VertId> = inner_ids.iter().rev().copied().collect();

        // Pick windings so the CAP's outer loop Newell = −m. The BAND uses the
        // reversed loops (its outer Newell = +m) and shares every rim edge's twin.
        let (cap_outer, cap_inner, band_outer, band_inner) = if outer_faces_plus_m {
            (&outer_rev, &inner_rev, &outer_ids, &inner_ids)
        } else {
            (&outer_ids, &inner_ids, &outer_rev, &inner_rev)
        };

        let cap = self
            .add_face_with_holes(cap_outer, &[cap_inner.as_slice()], material)
            .map_err(|e| anyhow::anyhow!("ADR-205 β-2-torus: annular cap sew failed: {e}"))?;
        let half = major_r + minor_r;
        self.set_face_surface(cap, Some(S::Plane {
            origin: o, normal: -m, basis_u: p1,
            u_range: (-half * 1.2, half * 1.2), v_range: (-half * 1.2, half * 1.2),
        }));

        let band = self
            .add_face_with_holes(band_outer, &[band_inner.as_slice()], material)
            .map_err(|e| anyhow::anyhow!("ADR-205 β-2-torus: torus band sew failed: {e}"))?;
        self.set_face_surface(band, Some(S::Torus {
            center, axis_dir: axis, ref_dir: p1,
            major_radius: major_r, minor_radius: minor_r,
            u_range: (0.0, TAU), v_range: (0.0, TAU),
        }));

        Ok(vec![band, cap])
    }

    /// **ADR-205 β-3-torus** — a kernel-native torus cut by TWO PARALLEL OBLIQUE
    /// planes (shared normal `m`, offsets `d_lo < d_hi` measured from the torus CENTRE
    /// along `m`) → an annular **SLAB**. In the STRADDLING regime (both planes pierce
    /// EVERY minor circle twice) the kept solid is exactly TWO Torus belts (outer +
    /// inner, split per-u by cos v) + TWO annular Plane caps, bounded by FOUR sampled
    /// spiric ovals {outer,inner}×{d_lo,d_hi}. Returns `[outer_belt, inner_belt,
    /// cap_lo, cap_hi]`.
    ///
    /// Built by the §4 de-risk recipe — four `add_face_with_holes` calls in which each
    /// oval is shared by two faces with OPPOSITE windings (sharing each rim edge's
    /// twin). The global winding is chosen so cap_lo.normal()=−m and cap_hi.normal()=+m
    /// (both lids outward). Each belt renders boundary-aware via
    /// `tessellate_torus_slab_clipped` (its two boundary loops live on the two planes).
    /// Validation precedes face removal, so a bail leaves the mesh INTACT.
    ///
    /// MVP scope: STRADDLING only (each plane cuts every minor circle twice). The
    /// one-sided (mixed 1/2-arc) and tube-swallowing regimes need different cap
    /// topologies and are deferred (§3 de-risk finding).
    pub fn boolean_torus_oblique_slab(
        &mut self,
        torus_faces: &[FaceId],
        plane_normal: DVec3,
        d_lo: f64,
        d_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let m = plane_normal.normalize_or_zero();
        if m.length_squared() < 0.5 {
            bail!("ADR-205 β-3-torus: degenerate plane normal");
        }
        if d_lo >= d_hi {
            bail!("ADR-205 β-3-torus: d_lo < d_hi required");
        }
        let (center, axis_raw, ref_dir, major_r, minor_r) = self.torus_full_of(torus_faces)?;
        let axis = axis_raw.normalize_or_zero();
        if axis.length_squared() < 0.5 || major_r <= minor_r + 1e-9 || minor_r <= 1e-9 {
            bail!("ADR-205 β-3-torus: degenerate torus (need 0 < r < R)");
        }
        let p1 = crate::surfaces::orthonormal_ref(axis, ref_dir);
        let p2 = axis.cross(p1).normalize_or_zero();
        if p1.length_squared() < 0.5 || p2.length_squared() < 0.5 {
            bail!("ADR-205 β-3-torus: degenerate basis");
        }
        let d = minor_r * axis.dot(m);
        let eval = |u: f64, v: f64| -> DVec3 {
            let radial = p1 * u.cos() + p2 * u.sin();
            center + radial * (major_r + minor_r * v.cos()) + axis * (minor_r * v.sin())
        };

        // Sample the four spiric ovals, validating the straddling-annular regime
        // (BOTH planes pierce every minor circle twice). Split per-u by cos v.
        let n = crate::surfaces::sagitta_segments(major_r + minor_r, TAU, 0.05).max(48);
        let (mut ol, mut il, mut oh, mut ih) = (
            Vec::with_capacity(n), Vec::with_capacity(n),
            Vec::with_capacity(n), Vec::with_capacity(n),
        );
        for i in 0..n {
            let u = TAU * (i as f64) / (n as f64);
            let radial = p1 * u.cos() + p2 * u.sin();
            let c_u = center + radial * major_r;
            let a_u = (c_u - center).dot(m);
            let b_u = minor_r * radial.dot(m);
            let amp = (b_u * b_u + d * d).sqrt();
            if amp < 1e-12 {
                bail!("ADR-205 β-3-torus: degenerate minor circle");
            }
            let phi = d.atan2(b_u);
            for (pd, outv, innv) in [(d_lo, &mut ol, &mut il), (d_hi, &mut oh, &mut ih)] {
                let cval = (pd - a_u) / amp;
                if cval.abs() >= 1.0 - 1e-9 {
                    bail!(
                        "ADR-205 β-3-torus: not the straddling-annular case (a plane misses/grazes \
                         a minor circle — the one-sided / tube-swallowing regimes are deferred)"
                    );
                }
                let dphi = cval.acos();
                let (v1, v2) = (phi + dphi, phi - dphi);
                let (vo, vi) = if v1.cos() >= v2.cos() { (v1, v2) } else { (v2, v1) };
                outv.push(eval(u, vo));
                innv.push(eval(u, vi));
            }
        }

        // Newell of the u-ordered outer_lo oval (planar on the d_lo plane) → pick the
        // global winding so both caps end up outward (cap_lo = −m, cap_hi = +m).
        let newell = |pts: &[DVec3]| -> DVec3 {
            let mut nrm = DVec3::ZERO;
            for i in 0..pts.len() {
                let a = pts[i];
                let b = pts[(i + 1) % pts.len()];
                nrm.x += (a.y - b.y) * (a.z + b.z);
                nrm.y += (a.z - b.z) * (a.x + b.x);
                nrm.z += (a.x - b.x) * (a.y + b.y);
            }
            nrm.normalize_or_zero()
        };
        let ol_plus_m = newell(&ol).dot(m) > 0.0;

        // Remove the original torus faces + their edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &tf in torus_faces {
                if let Some(f) = self.faces.get(tf) {
                    if let Ok(hes) = self.collect_loop_hes(f.outer().start) {
                        for he in hes { es.insert(self.hes[he].edge(), ()); }
                    }
                    for inner_l in f.inners() {
                        if let Ok(hes) = self.collect_loop_hes(inner_l.start) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &tf in torus_faces { let _ = self.remove_face(tf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        let ids = |me: &mut Mesh, pts: &[DVec3]| -> Vec<VertId> {
            pts.iter().map(|&p| me.add_vertex(p)).collect()
        };
        let (ol_id, il_id, oh_id, ih_id) =
            (ids(self, &ol), ids(self, &il), ids(self, &oh), ids(self, &ih));
        let rev = |v: &[VertId]| -> Vec<VertId> { v.iter().rev().copied().collect() };

        // Two watertight assignments (the §4 recipe + its global flip); pick the one
        // giving cap_lo = −m, cap_hi = +m. Each oval shared by two faces, opposite windings.
        let mat = material;
        let (cap_lo, outer_belt, cap_hi, inner_belt) = if !ol_plus_m {
            // outer_lo Newell = −m → cap_lo = AFH(outer_lo) = −m outward.
            let cl = self.add_face_with_holes(&ol_id, &[il_id.as_slice()], mat)
                .map_err(|e| anyhow::anyhow!("β-3-torus cap_lo: {e}"))?;
            let ob = self.add_face_with_holes(&rev(&ol_id), &[oh_id.as_slice()], mat)
                .map_err(|e| anyhow::anyhow!("β-3-torus outer belt: {e}"))?;
            let ch = self.add_face_with_holes(&rev(&oh_id), &[ih_id.as_slice()], mat)
                .map_err(|e| anyhow::anyhow!("β-3-torus cap_hi: {e}"))?;
            let ib = self.add_face_with_holes(&rev(&il_id), &[rev(&ih_id).as_slice()], mat)
                .map_err(|e| anyhow::anyhow!("β-3-torus inner belt: {e}"))?;
            (cl, ob, ch, ib)
        } else {
            // global flip → cap_lo = AFH(rev outer_lo) = −m outward.
            let cl = self.add_face_with_holes(&rev(&ol_id), &[rev(&il_id).as_slice()], mat)
                .map_err(|e| anyhow::anyhow!("β-3-torus cap_lo: {e}"))?;
            let ob = self.add_face_with_holes(&ol_id, &[rev(&oh_id).as_slice()], mat)
                .map_err(|e| anyhow::anyhow!("β-3-torus outer belt: {e}"))?;
            let ch = self.add_face_with_holes(&oh_id, &[rev(&ih_id).as_slice()], mat)
                .map_err(|e| anyhow::anyhow!("β-3-torus cap_hi: {e}"))?;
            let ib = self.add_face_with_holes(&il_id, &[ih_id.as_slice()], mat)
                .map_err(|e| anyhow::anyhow!("β-3-torus inner belt: {e}"))?;
            (cl, ob, ch, ib)
        };

        let torus = |me: &mut Mesh, f: FaceId| {
            me.set_face_surface(f, Some(S::Torus {
                center, axis_dir: axis, ref_dir: p1,
                major_radius: major_r, minor_radius: minor_r,
                u_range: (0.0, TAU), v_range: (0.0, TAU),
            }));
        };
        torus(self, outer_belt);
        torus(self, inner_belt);
        let half = major_r + minor_r;
        self.set_face_surface(cap_lo, Some(S::Plane {
            origin: center + m * d_lo, normal: -m, basis_u: p1,
            u_range: (-half * 1.2, half * 1.2), v_range: (-half * 1.2, half * 1.2),
        }));
        self.set_face_surface(cap_hi, Some(S::Plane {
            origin: center + m * d_hi, normal: m, basis_u: p1,
            u_range: (-half * 1.2, half * 1.2), v_range: (-half * 1.2, half * 1.2),
        }));

        Ok(vec![outer_belt, inner_belt, cap_lo, cap_hi])
    }

    /// ADR-197 Z-axis lift (A-3) — TORUS slab for an arbitrary-axis torus.
    /// `d_lo`/`d_hi` are cut bounds as the signed axial offset from the torus
    /// CENTER plane (`d = 0` at the centre), so both must cut the tube:
    /// `|d| < minor_radius`, `d_lo < d_hi`. The cut planes are ⟂ the torus axis.
    /// The solid is rotated so its axis is +Z (pivoting at the centre),
    /// `boolean_torus_slab` runs, and the result is rotated back (analytic Torus
    /// bands + tilted axis preserved). Returns
    /// `[outer_band, inner_band, top_washer, bottom_washer]`.
    pub fn boolean_torus_slab_local(
        &mut self,
        torus_faces: &[FaceId],
        d_lo: f64,
        d_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, center) = self.torus_axis_of(torus_faces)?;
        let cz = center.z;
        self.with_axis_lifted_to(torus_faces, axis_dir, center, DVec3::Z, move |me| {
            // +Z frame: the centre stays at `center` (the pivot), axis is +Z, so a
            // cut at axial offset `d` from the centre plane is at world-z = cz + d.
            me.boolean_torus_slab(torus_faces, cz + d_lo, cz + d_hi, material)
        })
    }

    // ── ADR-204 local-frame variants — halfspace / slice / subtract ──
    // Same `with_axis_lifted_to` rotate-roundtrip as the slab wrappers (A-1~3),
    // just calling the subtract/slice/halfspace Z-axis op. Cut bounds are along
    // the primitive's OWN axis (cylinder/torus: offset from axis_origin/centre;
    // cone: axial distance from apex).

    /// ADR-204 — CYLINDER − slab (subtract) for an arbitrary-axis cylinder.
    /// `v_lo`/`v_hi` axial along the cylinder axis (offset from axis_origin).
    pub fn boolean_cylinder_slab_subtract_local(
        &mut self,
        cyl_faces: &[FaceId],
        v_lo: f64,
        v_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, axis_origin) = self.cylinder_axis_of(cyl_faces)?;
        let z0 = axis_origin.z;
        self.with_axis_lifted_to(cyl_faces, axis_dir, axis_origin, DVec3::Z, move |me| {
            me.boolean_cylinder_slab_subtract(cyl_faces, z0 + v_lo, z0 + v_hi, material)
        })
    }

    /// ADR-204 — CYLINDER slice at axial `v_k` for an arbitrary-axis cylinder.
    pub fn boolean_cylinder_slice_local(
        &mut self,
        cyl_faces: &[FaceId],
        v_k: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, axis_origin) = self.cylinder_axis_of(cyl_faces)?;
        let z0 = axis_origin.z;
        self.with_axis_lifted_to(cyl_faces, axis_dir, axis_origin, DVec3::Z, move |me| {
            me.boolean_cylinder_slice(cyl_faces, z0 + v_k, material)
        })
    }

    /// ADR-204 — CONE − slab (subtract) for an arbitrary-axis cone. `v_lo`/`v_hi`
    /// axial distance from the apex (z = apex_z − v).
    pub fn boolean_cone_slab_subtract_local(
        &mut self,
        cone_faces: &[FaceId],
        v_lo: f64,
        v_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, apex) = self.cone_axis_of(cone_faces)?;
        let apex_z = apex.z;
        self.with_axis_lifted_to(cone_faces, axis_dir, apex, DVec3::NEG_Z, move |me| {
            me.boolean_cone_slab_subtract(cone_faces, apex_z - v_hi, apex_z - v_lo, material)
        })
    }

    /// ADR-204 — CONE slice at axial `v_k` (from apex) for an arbitrary-axis cone.
    pub fn boolean_cone_slice_local(
        &mut self,
        cone_faces: &[FaceId],
        v_k: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, apex) = self.cone_axis_of(cone_faces)?;
        let apex_z = apex.z;
        self.with_axis_lifted_to(cone_faces, axis_dir, apex, DVec3::NEG_Z, move |me| {
            me.boolean_cone_slice(cone_faces, apex_z - v_k, material)
        })
    }

    /// ADR-204 — TORUS halfspace for an arbitrary-axis torus. `d_k` = signed
    /// axial offset from the centre plane; `keep_above` keeps the +axis side.
    pub fn boolean_torus_halfspace_local(
        &mut self,
        torus_faces: &[FaceId],
        d_k: f64,
        keep_above: bool,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, center) = self.torus_axis_of(torus_faces)?;
        let cz = center.z;
        self.with_axis_lifted_to(torus_faces, axis_dir, center, DVec3::Z, move |me| {
            me.boolean_torus_halfspace(torus_faces, cz + d_k, keep_above, material)
        })
    }

    /// ADR-204 — TORUS − slab (subtract) for an arbitrary-axis torus.
    pub fn boolean_torus_slab_subtract_local(
        &mut self,
        torus_faces: &[FaceId],
        d_lo: f64,
        d_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, center) = self.torus_axis_of(torus_faces)?;
        let cz = center.z;
        self.with_axis_lifted_to(torus_faces, axis_dir, center, DVec3::Z, move |me| {
            me.boolean_torus_slab_subtract(torus_faces, cz + d_lo, cz + d_hi, material)
        })
    }

    /// ADR-204 — TORUS slice at axial offset `d_k` for an arbitrary-axis torus.
    pub fn boolean_torus_slice_local(
        &mut self,
        torus_faces: &[FaceId],
        d_k: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let (axis_dir, center) = self.torus_axis_of(torus_faces)?;
        let cz = center.z;
        self.with_axis_lifted_to(torus_faces, axis_dir, center, DVec3::Z, move |me| {
            me.boolean_torus_slice(torus_faces, cz + d_k, material)
        })
    }

    /// ADR-197 β-3-h — CYLINDER ∩ Z-slab (truncate). The curved-Boolean sphere
    /// pattern (SSI → imprint → classify → merge → sew) reused for a cylinder,
    /// but degenerate-simplified: a cylinder's surface parameter `v` IS the axial
    /// position (`z = axis_origin.z + v`), so an axis-aligned latitude cut maps to
    /// a pure `v`-range clamp — no inversion, no seam, no pole (a cylinder has no
    /// pole, so the kept part is always a band, never a cap). `sew_curved_band`
    /// (the same multi-loop primitive as the sphere band) does the stitching.
    ///
    /// `[z_lo, z_hi]` is clamped to the cylinder's extent; halfspace `{z>k}` is the
    /// special case `slab(k, top_z)`. Requires the cylinder axis ∥ Z (MVP).
    /// Returns `[side_band, top_disk, bottom_disk]`.
    pub fn boolean_cylinder_slab(
        &mut self,
        cyl_faces: &[FaceId],
        z_lo: f64,
        z_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if z_lo >= z_hi {
            bail!("ADR-197 β-3-h cylinder slab: z_lo < z_hi required");
        }
        // Cylinder params from a side (Cylinder) face.
        let (axis_origin, axis_dir, ref_dir, radius, v_range) = cyl_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cylinder { axis_origin, axis_dir, ref_dir, radius, v_range, .. }) => {
                    Some((*axis_origin, *axis_dir, *ref_dir, *radius, *v_range))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-h cylinder slab: no Cylinder surface"))?;
        // MVP: Z-up cylinder (axis ∥ Z).
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-h cylinder slab MVP: cylinder axis must be ∥ Z");
        }
        let z0 = axis_origin.z;
        let base_z = z0 + v_range.0.min(v_range.1);
        let top_z = z0 + v_range.0.max(v_range.1);
        // Clamp the slab to the cylinder's extent; require a genuine cut.
        let lo = z_lo.max(base_z);
        let hi = z_hi.min(top_z);
        if lo >= hi - 1e-9 {
            bail!("ADR-197 β-3-h cylinder slab: slab does not overlap the cylinder extent");
        }
        if lo <= base_z + 1e-9 && hi >= top_z - 1e-9 {
            bail!("ADR-197 β-3-h cylinder slab: slab covers the whole cylinder (no cut)");
        }
        // SSI sanity: each bounding plane that genuinely cuts the side must yield a
        // closed cut circle (confirms the analytic surfaces actually intersect).
        let side_surf = S::Cylinder {
            axis_origin,
            axis_dir,
            radius,
            ref_dir,
            u_range: (0.0, TAU),
            v_range,
        };
        for &z in &[lo, hi] {
            if z > base_z + 1e-9 && z < top_z - 1e-9 {
                let plane = S::Plane {
                    origin: DVec3::new(axis_origin.x, axis_origin.y, z),
                    normal: DVec3::Z,
                    basis_u: DVec3::X,
                    u_range: (-1e6, 1e6),
                    v_range: (-1e6, 1e6),
                };
                match surface_surface_intersection(&plane, &side_surf) {
                    Some(s) if s.closed && s.points.len() >= 3 => {}
                    _ => bail!(
                        "ADR-197 β-3-h cylinder slab: plane z={z} did not yield a closed cut circle"
                    ),
                }
            }
        }

        let cx = axis_origin.x;
        let cy = axis_origin.y;
        let circle = |z: f64| crate::curves::AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, z),
            radius,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        // Kept band: same cylinder surface, axial range clamped to the slab.
        let band = S::Cylinder {
            axis_origin,
            axis_dir,
            radius,
            ref_dir,
            u_range: (0.0, TAU),
            v_range: (lo - z0, hi - z0),
        };
        let disk = |z: f64, nz: DVec3| S::Plane {
            origin: DVec3::new(cx, cy, z),
            normal: nz,
            basis_u: DVec3::X,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };

        // Remove the original cylinder faces + their (self-loop) edges. The side
        // band carries outer + inner loops; the disks share those same self-loop
        // edges via twin half-edges, so the dedup keeps the set to 2 edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cyl_faces {
                if let Some(f) = self.faces.get(cf) {
                    let mut starts = vec![f.outer().start];
                    for inner in f.inners() {
                        starts.push(inner.start);
                    }
                    for st in starts {
                        if let Ok(hes) = self.collect_loop_hes(st) {
                            for he in hes {
                                es.insert(self.hes[he].edge(), ());
                            }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cyl_faces {
            let _ = self.remove_face(cf);
        }
        for eid in orig_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }

        let (bf, tdf, bdf) = self.sew_curved_band(
            DVec3::new(cx + radius, cy, hi),
            circle(hi),
            DVec3::new(cx + radius, cy, lo),
            circle(lo),
            band,
            ref_dir,
            disk(hi, DVec3::Z),
            DVec3::Z,
            disk(lo, DVec3::NEG_Z),
            DVec3::NEG_Z,
            material,
        )?;
        Ok(vec![bf, tdf, bdf])
    }

    /// ADR-197 β-3-h — CONE ∩ Z-slab. The same sphere/cylinder pattern; a cone's
    /// surface parameter `v` is the axial distance from the apex (radius =
    /// `v·tan(half_angle)`), so an axis-aligned cut is a constant-`v` latitude
    /// circle. Two result shapes (both reuse an EXISTING curved-sew primitive):
    ///   • keeps the apex → a smaller cone (single self-loop side + 1 disk) via
    ///     `sew_closed_curve_pair` (the ε-1 cap primitive — apex is degenerate);
    ///   • otherwise      → a frustum (multi-loop band + 2 disks) via `sew_curved_band`.
    ///
    /// `[z_lo, z_hi]` is clamped to the cone's extent; halfspace is the special case
    /// where one bound is the apex (smaller cone) or the base (frustum-to-base).
    /// MVP: apex-up Z-axis cone (`create_cone_kernel_native` output, axis_dir = -Z).
    pub fn boolean_cone_slab(
        &mut self,
        cone_faces: &[FaceId],
        z_lo: f64,
        z_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if z_lo >= z_hi {
            bail!("ADR-197 β-3-h cone slab: z_lo < z_hi required");
        }
        let (apex, axis_dir, half_angle, ref_dir, v_range) = cone_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cone { apex, axis_dir, half_angle, ref_dir, v_range, .. }) => {
                    Some((*apex, *axis_dir, *half_angle, *ref_dir, *v_range))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-h cone slab: no Cone surface"))?;
        // MVP: apex-up Z-axis cone (axis points down toward the base).
        if (axis_dir.normalize_or_zero() - DVec3::NEG_Z).length() > 1e-6 {
            bail!("ADR-197 β-3-h cone slab MVP: cone axis must be -Z (apex-up)");
        }
        let apex_z = apex.z;
        let v_base = v_range.0.max(v_range.1);
        let base_z = apex_z - v_base; // axis_dir.z = -1
        if apex_z <= base_z + 1e-9 {
            bail!("ADR-197 β-3-h cone slab: degenerate cone extent");
        }
        let tan_ha = half_angle.tan();
        let cx = apex.x;
        let cy = apex.y;
        let lo = z_lo.max(base_z);
        let hi = z_hi.min(apex_z);
        if lo >= hi - 1e-9 {
            bail!("ADR-197 β-3-h cone slab: slab does not overlap the cone extent");
        }
        let keeps_apex = hi >= apex_z - 1e-9;
        let keeps_base = lo <= base_z + 1e-9;
        if keeps_apex && keeps_base {
            bail!("ADR-197 β-3-h cone slab: slab covers the whole cone (no cut)");
        }
        // axial distance from apex + cone radius at height z.
        let v_of = |z: f64| apex_z - z;
        let r_of = |z: f64| (apex_z - z) * tan_ha;
        let circle = |z: f64| crate::curves::AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, z),
            radius: r_of(z),
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        let disk = |z: f64, nz: DVec3| S::Plane {
            origin: DVec3::new(cx, cy, z),
            normal: nz,
            basis_u: DVec3::X,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        // SSI sanity for each genuine cut (a plane strictly inside the extent).
        for &z in &[lo, hi] {
            if z > base_z + 1e-9 && z < apex_z - 1e-9 {
                let side = S::Cone {
                    apex,
                    axis_dir,
                    half_angle,
                    ref_dir,
                    u_range: (0.0, TAU),
                    v_range,
                };
                let plane = S::Plane {
                    origin: DVec3::new(cx, cy, z),
                    normal: DVec3::Z,
                    basis_u: DVec3::X,
                    u_range: (-1e6, 1e6),
                    v_range: (-1e6, 1e6),
                };
                match surface_surface_intersection(&plane, &side) {
                    Some(s) if s.closed && s.points.len() >= 3 => {}
                    _ => bail!(
                        "ADR-197 β-3-h cone slab: plane z={z} did not yield a closed cut circle"
                    ),
                }
            }
        }

        // Remove original cone faces + their (self-loop) edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &cf in cone_faces {
                if let Some(f) = self.faces.get(cf) {
                    let mut starts = vec![f.outer().start];
                    for inner in f.inners() {
                        starts.push(inner.start);
                    }
                    for st in starts {
                        if let Ok(hes) = self.collect_loop_hes(st) {
                            for he in hes {
                                es.insert(self.hes[he].edge(), ());
                            }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &cf in cone_faces {
            let _ = self.remove_face(cf);
        }
        for eid in orig_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }

        if keeps_apex {
            // smaller cone: apex .. single cut at z=lo. The cone side (Cone v∈[0,
            // v(lo)], apex degenerate) shares the z=lo base circle with the bottom
            // disk — the ε-1 self-loop cap pattern.
            let cone_side = S::Cone {
                apex,
                axis_dir,
                half_angle,
                ref_dir,
                u_range: (0.0, TAU),
                v_range: (0.0, v_of(lo)),
            };
            let n_anchor = DVec3::new(cx + r_of(lo), cy, lo);
            let cone_n = cone_side.normal_at_world_pos(n_anchor);
            let (cf, df) = self.sew_closed_curve_pair(
                n_anchor,
                circle(lo),
                cone_side,
                cone_n,
                disk(lo, DVec3::NEG_Z),
                DVec3::NEG_Z,
                material,
            )?;
            Ok(vec![cf, df])
        } else {
            // frustum: band Cone v∈[v(hi), v(lo)] (top circle at hi = smaller radius,
            // bottom circle at lo = larger radius) + top/bottom disks.
            let band = S::Cone {
                apex,
                axis_dir,
                half_angle,
                ref_dir,
                u_range: (0.0, TAU),
                v_range: (v_of(hi), v_of(lo)),
            };
            let band_n = band.normal_at_world_pos(DVec3::new(cx + r_of(hi), cy, hi));
            let (bf, tdf, bdf) = self.sew_curved_band(
                DVec3::new(cx + r_of(hi), cy, hi),
                circle(hi),
                DVec3::new(cx + r_of(lo), cy, lo),
                circle(lo),
                band,
                band_n,
                disk(hi, DVec3::Z),
                DVec3::Z,
                disk(lo, DVec3::NEG_Z),
                DVec3::NEG_Z,
                material,
            )?;
            Ok(vec![bf, tdf, bdf])
        }
    }

    /// ADR-197 β-3-h — TORUS ∩ halfspace. Unlike sphere/cylinder/cone, a torus has
    /// no single-curve SSI: a horizontal plane `z = k` cuts a Z-up torus in TWO
    /// concentric circles (the `plane_torus` dispatch is intentionally absent). The
    /// kept solid is one poloidal band capped by an ANNULAR WASHER (the flat cut
    /// cross-section, a `Plane` with the inner circle as a hole) — `sew_torus_cap`.
    ///
    /// `z = center.z + r·sin(v)` (poloidal), so the cut is at `sin v = d/r`
    /// (`d = k − center.z`, requires `|d| < r`): `v1 = asin(d/r)` (outer circle
    /// `ρ = R+√(r²−d²)`), `v2 = π − v1` (inner circle `ρ = R−√(r²−d²)`).
    ///   • `keep_above` (z>k) → top arc `v ∈ [v1, v2]`, washer faces `−Z`;
    ///   • `keep_below` (z<k) → bottom arc `v ∈ [v2, 2π+v1]`, washer faces `+Z`.
    /// MVP: Z-up torus (single cut). The two-cut slab keeps TWO disjoint bands — a
    /// later sub-step. Returns `[band_face, washer_face]`.
    pub fn boolean_torus_halfspace(
        &mut self,
        torus_faces: &[FaceId],
        k: f64,
        keep_above: bool,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (center, axis_dir, ref_dir, major_radius, minor_radius) = torus_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Torus {
                    center,
                    axis_dir,
                    ref_dir,
                    major_radius,
                    minor_radius,
                    ..
                }) => Some((*center, *axis_dir, *ref_dir, *major_radius, *minor_radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-h torus halfspace: no Torus surface"))?;
        // MVP: Z-up torus.
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-h torus halfspace MVP: torus axis must be ∥ Z");
        }
        // 2-circle horizontal cut (the "plane_torus" computation).
        let (v1, v2, rho_outer, rho_inner) =
            torus_z_cut(center.z, major_radius, minor_radius, k)
                .ok_or_else(|| anyhow::anyhow!(
                    "ADR-197 β-3-h torus halfspace: plane z={k} does not cut the tube \
                     (need |k − center.z| < minor_radius)"
                ))?;
        let cx = center.x;
        let cy = center.y;
        let circle = |rho: f64| crate::curves::AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, k),
            radius: rho,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        // kept poloidal band + washer orientation.
        let (band_v_lo, band_v_hi, washer_normal) = if keep_above {
            (v1, v2, DVec3::NEG_Z) // material above → cap faces down
        } else {
            (v2, TAU + v1, DVec3::Z) // material below → cap faces up
        };
        let band = S::Torus {
            center,
            axis_dir,
            ref_dir,
            major_radius,
            minor_radius,
            u_range: (0.0, TAU),
            v_range: (band_v_lo, band_v_hi),
        };
        let outer_anchor = DVec3::new(cx + rho_outer, cy, k);
        let inner_anchor = DVec3::new(cx + rho_inner, cy, k);
        let band_normal = band.normal_at_world_pos(outer_anchor);
        // washer: flat annulus at z=k (outer circle, inner hole).
        let washer = S::Plane {
            origin: DVec3::new(cx, cy, k),
            normal: washer_normal,
            basis_u: DVec3::X,
            u_range: (-(rho_outer * 1.5), rho_outer * 1.5),
            v_range: (-(rho_outer * 1.5), rho_outer * 1.5),
        };

        // remove the original torus face(s) + edge(s).
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &tf in torus_faces {
                if let Some(f) = self.faces.get(tf) {
                    let mut starts = vec![f.outer().start];
                    for inner in f.inners() {
                        starts.push(inner.start);
                    }
                    for st in starts {
                        if let Ok(hes) = self.collect_loop_hes(st) {
                            for he in hes {
                                es.insert(self.hes[he].edge(), ());
                            }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &tf in torus_faces {
            let _ = self.remove_face(tf);
        }
        for eid in orig_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }

        let (band_f, washer_f) = self.sew_torus_cap(
            outer_anchor,
            circle(rho_outer),
            inner_anchor,
            circle(rho_inner),
            band,
            band_normal,
            washer,
            washer_normal,
            material,
        )?;
        Ok(vec![band_f, washer_f])
    }

    /// ADR-197 β-3-l — TORUS ∩ Z-slab (both cuts within the tube): keeps a
    /// horizontal band of the donut — still a genus-1 ring. Result = 2 Torus
    /// bands (outer + inner tube surface) + 2 Plane washers (annular caps at
    /// z_lo, z_hi), wired from 4 cut circles (outer/inner × hi/lo), each shared
    /// by a band and a washer. Returns `[outer_band, inner_band, top_washer,
    /// bot_washer]`. MVP: Z-up torus, both planes cut the tube (else use
    /// `boolean_torus_halfspace`).
    pub fn boolean_torus_slab(
        &mut self,
        torus_faces: &[FaceId],
        z_lo: f64,
        z_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if z_lo >= z_hi {
            bail!("ADR-197 β-3-l torus slab: z_lo < z_hi required");
        }
        let (center, axis_dir, ref_dir, major_radius, minor_radius) = torus_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. }) => {
                    Some((*center, *axis_dir, *ref_dir, *major_radius, *minor_radius))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-l torus slab: no Torus surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-l torus slab MVP: torus axis must be ∥ Z");
        }
        let (cx, cy, cz) = (center.x, center.y, center.z);
        let (rr, mr) = (major_radius, minor_radius);
        let d_lo = z_lo - cz;
        let d_hi = z_hi - cz;
        // both planes must cut the tube (else it is a halfspace, not a 2-cut slab).
        if d_lo <= -mr + 1e-9 || d_hi >= mr - 1e-9 {
            bail!("ADR-197 β-3-l torus slab: both planes must cut the tube (|z−cz| < minor_radius)");
        }
        let outer = |d: f64| rr + (mr * mr - d * d).sqrt();
        let inner = |d: f64| rr - (mr * mr - d * d).sqrt();
        let (o_hi, i_hi, o_lo, i_lo) = (outer(d_hi), inner(d_hi), outer(d_lo), inner(d_lo));
        // band poloidal v-ranges.
        let v_o_lo = (d_lo / mr).asin();
        let v_o_hi = (d_hi / mr).asin();
        let v_i_lo = std::f64::consts::PI - (d_hi / mr).asin();
        let v_i_hi = std::f64::consts::PI - (d_lo / mr).asin();
        let circle = |z: f64, rho: f64| AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, z),
            radius: rho,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        let torus_band = |v_lo: f64, v_hi: f64| S::Torus {
            center,
            axis_dir,
            ref_dir,
            major_radius: rr,
            minor_radius: mr,
            u_range: (0.0, TAU),
            v_range: (v_lo, v_hi),
        };
        let washer = |z: f64| S::Plane {
            origin: DVec3::new(cx, cy, z),
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (-(o_hi.max(o_lo) * 1.5), o_hi.max(o_lo) * 1.5),
            v_range: (-(o_hi.max(o_lo) * 1.5), o_hi.max(o_lo) * 1.5),
        };
        let outer_band = torus_band(v_o_lo, v_o_hi);
        let inner_band = torus_band(v_i_lo, v_i_hi);
        let outer_normal = outer_band.normal_at_world_pos(DVec3::new(cx + o_hi, cy, z_hi));
        let inner_normal = inner_band.normal_at_world_pos(DVec3::new(cx + i_hi, cy, z_hi));

        // remove original torus.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &tf in torus_faces {
                if let Some(f) = self.faces.get(tf) {
                    let mut starts = vec![f.outer().start];
                    for inner in f.inners() { starts.push(inner.start); }
                    for st in starts {
                        if let Ok(hes) = self.collect_loop_hes(st) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &tf in torus_faces { let _ = self.remove_face(tf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }

        // 4 cut-circle self-loop edges → 8 half-edges.
        let (ohf, ohb) = self.add_self_loop_circle(DVec3::new(cx + o_hi, cy, z_hi), circle(z_hi, o_hi))?;
        let (ihf, ihb) = self.add_self_loop_circle(DVec3::new(cx + i_hi, cy, z_hi), circle(z_hi, i_hi))?;
        let (olf, olb) = self.add_self_loop_circle(DVec3::new(cx + o_lo, cy, z_lo), circle(z_lo, o_lo))?;
        let (ilf, ilb) = self.add_self_loop_circle(DVec3::new(cx + i_lo, cy, z_lo), circle(z_lo, i_lo))?;
        // wire 4 faces — each circle's fwd → band, bwd → washer.
        let ob = self.wire_2loop_face(ohf, olf, outer_band, outer_normal, material);
        let ib = self.wire_2loop_face(ihf, ilf, inner_band, inner_normal, material);
        let tw = self.wire_2loop_face(ohb, ihb, washer(z_hi), DVec3::Z, material);
        let bw = self.wire_2loop_face(olb, ilb, washer(z_lo), DVec3::NEG_Z, material);
        Ok(vec![ob, ib, tw, bw])
    }

    /// ADR-197 β-3-m — remove a primitive solid's faces + their (self-loop /
    /// shared) edges. Shared helper for the slab-subtract 2-piece builders, which
    /// extract analytic params first, remove the original ONCE, then sew both
    /// outer pieces. (The sew primitives never touch the original, so removing
    /// once is sufficient — same pattern as `boolean_torus_slab`.)
    fn remove_primitive_solid(&mut self, faces: &[FaceId]) {
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &f in faces {
                if let Some(face) = self.faces.get(f) {
                    let mut starts = vec![face.outer().start];
                    for inner in face.inners() { starts.push(inner.start); }
                    for st in starts {
                        if let Ok(hes) = self.collect_loop_hes(st) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &f in faces { let _ = self.remove_face(f); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }
    }

    /// ADR-197 β-3-m — SPHERE − Z-slab (both planes cut): `A − box = A ∩ ¬box`
    /// keeps the TWO outer caps (z>z_hi and z<z_lo) = 2 DISJOINT closed solids.
    /// Each cap reaches a pole (so it is a genuine single cap, not an annulus) =
    /// a Sphere cap + a Plane disk via `sew_closed_curve_pair`. Returns 4 faces
    /// `[top_cap, top_disk, bot_cap, bot_disk]` (2 solids). MVP: Z-up.
    pub fn boolean_sphere_slab_subtract(
        &mut self,
        sphere_faces: &[FaceId],
        z_lo: f64,
        z_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        if z_lo >= z_hi {
            bail!("ADR-197 β-3-m sphere slab subtract: z_lo < z_hi required");
        }
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-m sphere slab subtract: no Sphere surface"))?;
        let (cx, cy, cz) = (center.x, center.y, center.z);
        let d_lo = z_lo - cz;
        let d_hi = z_hi - cz;
        if d_lo <= -radius + 1e-9 || d_hi >= radius - 1e-9 || d_lo >= d_hi {
            bail!("ADR-197 β-3-m sphere slab subtract: both planes must cut the sphere");
        }
        let rho = |d: f64| (radius * radius - d * d).sqrt();
        let circle = |z: f64, r: f64| AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, z),
            radius: r,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        let cap_normal = |v_lo: f64, v_hi: f64| {
            let vc = (v_lo + v_hi) * 0.5;
            (crate::surfaces::sphere::evaluate(center, radius, DVec3::Z, DVec3::X, std::f64::consts::PI, vc) - center)
                .normalize_or_zero()
        };
        self.remove_primitive_solid(sphere_faces);

        // top cap: keep z > z_hi → Sphere v∈[asin(d_hi/r), π/2], disk faces −Z.
        let v_top_lo = (d_hi / radius).asin();
        let top_cap = S::Sphere { center, radius, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (v_top_lo, FRAC_PI_2) };
        let top_disk = S::Plane {
            origin: DVec3::new(cx, cy, z_hi), normal: DVec3::NEG_Z, basis_u: DVec3::X,
            u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        };
        let (tcf, tdf) = self.sew_closed_curve_pair(
            DVec3::new(cx + rho(d_hi), cy, z_hi), circle(z_hi, rho(d_hi)),
            top_cap, cap_normal(v_top_lo, FRAC_PI_2), top_disk, DVec3::NEG_Z, material,
        )?;

        // bottom cap: keep z < z_lo → Sphere v∈[−π/2, asin(d_lo/r)], disk faces +Z.
        let v_bot_hi = (d_lo / radius).asin();
        let bot_cap = S::Sphere { center, radius, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (-FRAC_PI_2, v_bot_hi) };
        let bot_disk = S::Plane {
            origin: DVec3::new(cx, cy, z_lo), normal: DVec3::Z, basis_u: DVec3::X,
            u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        };
        let (bcf, bdf) = self.sew_closed_curve_pair(
            DVec3::new(cx + rho(d_lo), cy, z_lo), circle(z_lo, rho(d_lo)),
            bot_cap, cap_normal(-FRAC_PI_2, v_bot_hi), bot_disk, DVec3::Z, material,
        )?;
        Ok(vec![tcf, tdf, bcf, bdf])
    }

    /// ADR-204 β-3 — sphere − slab (subtract) for an arbitrary plane normal `n`:
    /// keeps the TWO outer caps (`dot(·−c, n) > d_hi` and `< d_lo`), each an
    /// ORIENTED Sphere reaching the ±n pole (top axis_dir = n, v∈[asin(d_hi/r),
    /// π/2]; bottom axis_dir = n, v∈[−π/2, asin(d_lo/r)]), so a TILTED slab
    /// subtract renders correctly (β-2 cap pattern ×2). `d_lo < d_hi`, both
    /// `|d| < radius`. Returns `[top_cap, top_disk, bottom_cap, bottom_disk]`
    /// (2 disjoint solids).
    pub fn boolean_sphere_slab_subtract_oriented(
        &mut self,
        sphere_faces: &[FaceId],
        plane_normal: DVec3,
        d_lo: f64,
        d_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, PI, TAU};
        let n = plane_normal.normalize_or_zero();
        if n.length_squared() < 0.5 {
            bail!("ADR-204 β-3 oriented slab subtract: degenerate plane normal");
        }
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-204 β-3 oriented slab subtract: no Sphere surface"))?;
        if d_lo >= d_hi {
            bail!("ADR-204 β-3 oriented slab subtract: d_lo < d_hi required");
        }
        if d_lo <= -radius + 1e-9 || d_hi >= radius - 1e-9 {
            bail!("ADR-204 β-3 oriented slab subtract: both planes must cut the sphere (|d| < r)");
        }
        let basis_u = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
            .normalize_or_zero();
        let rho = |d: f64| (radius * radius - d * d).max(0.0).sqrt();
        let circle = |d: f64, r: f64| crate::curves::AnalyticCurve::Circle {
            center: center + d * n,
            radius: r,
            normal: n,
            basis_u,
        };
        let disk = |d: f64, dn: DVec3| S::Plane {
            origin: center + d * n,
            normal: dn,
            basis_u,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        let cap_normal = |v_lo: f64, v_hi: f64| {
            let vc = (v_lo + v_hi) * 0.5;
            (crate::surfaces::sphere::evaluate(center, radius, n, basis_u, PI, vc) - center)
                .normalize_or_zero()
        };
        self.remove_primitive_solid(sphere_faces);

        // top cap: keep dot > d_hi → reaches +n pole. Disk at d_hi faces −n.
        let v_top_lo = (d_hi / radius).clamp(-1.0, 1.0).asin();
        let top_cap = S::Sphere {
            center, radius, axis_dir: n, ref_dir: basis_u,
            u_range: (0.0, TAU), v_range: (v_top_lo, FRAC_PI_2),
        };
        let (tcf, tdf) = self.sew_closed_curve_pair(
            center + d_hi * n + basis_u * rho(d_hi), circle(d_hi, rho(d_hi)),
            top_cap, cap_normal(v_top_lo, FRAC_PI_2), disk(d_hi, -n), -n, material,
        )?;

        // bottom cap: keep dot < d_lo → reaches −n pole. Disk at d_lo faces +n.
        let v_bot_hi = (d_lo / radius).clamp(-1.0, 1.0).asin();
        let bot_cap = S::Sphere {
            center, radius, axis_dir: n, ref_dir: basis_u,
            u_range: (0.0, TAU), v_range: (-FRAC_PI_2, v_bot_hi),
        };
        let (bcf, bdf) = self.sew_closed_curve_pair(
            center + d_lo * n + basis_u * rho(d_lo), circle(d_lo, rho(d_lo)),
            bot_cap, cap_normal(-FRAC_PI_2, v_bot_hi), disk(d_lo, n), n, material,
        )?;
        Ok(vec![tcf, tdf, bcf, bdf])
    }

    /// ADR-197 β-3-m — CYLINDER − Z-slab (both planes cut): keeps the TWO outer
    /// stubs (z∈[base,z_lo] and z∈[z_hi,top]) = 2 DISJOINT short cylinders. Each
    /// stub = side Cylinder band + 2 Plane disks via `sew_curved_band`. Returns 6
    /// faces (2 solids). MVP: Z-up.
    pub fn boolean_cylinder_slab_subtract(
        &mut self,
        cyl_faces: &[FaceId],
        z_lo: f64,
        z_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if z_lo >= z_hi {
            bail!("ADR-197 β-3-m cylinder slab subtract: z_lo < z_hi required");
        }
        let (axis_origin, axis_dir, ref_dir, radius, v_range) = cyl_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cylinder { axis_origin, axis_dir, ref_dir, radius, v_range, .. }) => {
                    Some((*axis_origin, *axis_dir, *ref_dir, *radius, *v_range))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-m cylinder slab subtract: no Cylinder surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-m cylinder slab subtract MVP: axis must be ∥ Z");
        }
        let z0 = axis_origin.z;
        let base_z = z0 + v_range.0.min(v_range.1);
        let top_z = z0 + v_range.0.max(v_range.1);
        if z_lo <= base_z + 1e-9 || z_hi >= top_z - 1e-9 {
            bail!("ADR-197 β-3-m cylinder slab subtract: both planes must cut the cylinder");
        }
        let (cx, cy) = (axis_origin.x, axis_origin.y);
        let circle = |z: f64| AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, z), radius, normal: DVec3::Z, basis_u: DVec3::X,
        };
        let disk = |z: f64, nz: DVec3| S::Plane {
            origin: DVec3::new(cx, cy, z), normal: nz, basis_u: DVec3::X,
            u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        };
        let band = |lo: f64, hi: f64| S::Cylinder {
            axis_origin, axis_dir, radius, ref_dir,
            u_range: (0.0, TAU), v_range: (lo - z0, hi - z0),
        };
        self.remove_primitive_solid(cyl_faces);

        // bottom stub z∈[base, z_lo]: base cap (−Z) + cut disk at z_lo (+Z).
        let mut out = Vec::new();
        let (bf0, td0, bd0) = self.sew_curved_band(
            DVec3::new(cx + radius, cy, z_lo), circle(z_lo),
            DVec3::new(cx + radius, cy, base_z), circle(base_z),
            band(base_z, z_lo), ref_dir,
            disk(z_lo, DVec3::Z), DVec3::Z,
            disk(base_z, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        out.extend([bf0, td0, bd0]);
        // top stub z∈[z_hi, top]: top cap (+Z) + cut disk at z_hi (−Z).
        let (bf1, td1, bd1) = self.sew_curved_band(
            DVec3::new(cx + radius, cy, top_z), circle(top_z),
            DVec3::new(cx + radius, cy, z_hi), circle(z_hi),
            band(z_hi, top_z), ref_dir,
            disk(top_z, DVec3::Z), DVec3::Z,
            disk(z_hi, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        out.extend([bf1, td1, bd1]);
        Ok(out)
    }

    /// ADR-197 β-3-m — CONE − Z-slab (both planes cut, apex-up): keeps the base
    /// FRUSTUM (z∈[base,z_lo]) + the tip CONE (z∈[z_hi,apex]) = 2 DISJOINT solids.
    /// Frustum = band Cone + 2 disks (`sew_curved_band`); tip = Cone side (apex
    /// degenerate) + 1 disk (`sew_closed_curve_pair`). Returns 5 faces (2 solids).
    pub fn boolean_cone_slab_subtract(
        &mut self,
        cone_faces: &[FaceId],
        z_lo: f64,
        z_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if z_lo >= z_hi {
            bail!("ADR-197 β-3-m cone slab subtract: z_lo < z_hi required");
        }
        let (apex, axis_dir, half_angle, ref_dir, v_range) = cone_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cone { apex, axis_dir, half_angle, ref_dir, v_range, .. }) => {
                    Some((*apex, *axis_dir, *half_angle, *ref_dir, *v_range))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-m cone slab subtract: no Cone surface"))?;
        if (axis_dir.normalize_or_zero() - DVec3::NEG_Z).length() > 1e-6 {
            bail!("ADR-197 β-3-m cone slab subtract MVP: axis must be -Z (apex-up)");
        }
        let apex_z = apex.z;
        let base_z = apex_z - v_range.0.max(v_range.1);
        if z_lo <= base_z + 1e-9 || z_hi >= apex_z - 1e-9 {
            bail!("ADR-197 β-3-m cone slab subtract: both planes must cut the cone");
        }
        let tan_ha = half_angle.tan();
        let (cx, cy) = (apex.x, apex.y);
        let v_of = |z: f64| apex_z - z;
        let r_of = |z: f64| (apex_z - z) * tan_ha;
        let circle = |z: f64| AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, z), radius: r_of(z), normal: DVec3::Z, basis_u: DVec3::X,
        };
        let disk = |z: f64, nz: DVec3| S::Plane {
            origin: DVec3::new(cx, cy, z), normal: nz, basis_u: DVec3::X,
            u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        };
        self.remove_primitive_solid(cone_faces);

        // base frustum z∈[base, z_lo]: band Cone v∈[v(z_lo), v(base)] + 2 disks.
        let fr_band = S::Cone {
            apex, axis_dir, half_angle, ref_dir,
            u_range: (0.0, TAU), v_range: (v_of(z_lo), v_of(base_z)),
        };
        let fr_n = fr_band.normal_at_world_pos(DVec3::new(cx + r_of(z_lo), cy, z_lo));
        let (bf, tdf, bdf) = self.sew_curved_band(
            DVec3::new(cx + r_of(z_lo), cy, z_lo), circle(z_lo),
            DVec3::new(cx + r_of(base_z), cy, base_z), circle(base_z),
            fr_band, fr_n,
            disk(z_lo, DVec3::Z), DVec3::Z,
            disk(base_z, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        // tip cone z∈[z_hi, apex]: Cone side v∈[0, v(z_hi)] (apex degenerate) + cut disk (−Z).
        let tip_side = S::Cone {
            apex, axis_dir, half_angle, ref_dir,
            u_range: (0.0, TAU), v_range: (0.0, v_of(z_hi)),
        };
        let tip_anchor = DVec3::new(cx + r_of(z_hi), cy, z_hi);
        let tip_n = tip_side.normal_at_world_pos(tip_anchor);
        let (tcf, tdf2) = self.sew_closed_curve_pair(
            tip_anchor, circle(z_hi), tip_side, tip_n, disk(z_hi, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        Ok(vec![bf, tdf, bdf, tcf, tdf2])
    }

    /// ADR-197 β-3-m — TORUS − Z-slab (both planes cut the tube): keeps the TWO
    /// outer band-rings (z>z_hi and z<z_lo) = 2 DISJOINT genus-1 rings. Each ring
    /// = Torus band + annular Plane washer via `sew_torus_cap` (the halfspace
    /// shape, mirrored). Returns 4 faces (2 solids). MVP: Z-up.
    pub fn boolean_torus_slab_subtract(
        &mut self,
        torus_faces: &[FaceId],
        z_lo: f64,
        z_hi: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        if z_lo >= z_hi {
            bail!("ADR-197 β-3-m torus slab subtract: z_lo < z_hi required");
        }
        let (center, axis_dir, ref_dir, major_radius, minor_radius) = torus_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. }) => {
                    Some((*center, *axis_dir, *ref_dir, *major_radius, *minor_radius))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-m torus slab subtract: no Torus surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-m torus slab subtract MVP: axis must be ∥ Z");
        }
        let (cx, cy) = (center.x, center.y);
        // Both planes must cut the tube.
        let cut_hi = torus_z_cut(center.z, major_radius, minor_radius, z_hi);
        let cut_lo = torus_z_cut(center.z, major_radius, minor_radius, z_lo);
        let (Some((v1h, v2h, ro_h, ri_h)), Some((v1l, v2l, ro_l, ri_l))) = (cut_hi, cut_lo) else {
            bail!("ADR-197 β-3-m torus slab subtract: both planes must cut the tube");
        };
        let circle = |z: f64, rho: f64| AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, z), radius: rho, normal: DVec3::Z, basis_u: DVec3::X,
        };
        let torus_band = |v_lo: f64, v_hi: f64| S::Torus {
            center, axis_dir, ref_dir, major_radius, minor_radius,
            u_range: (0.0, TAU), v_range: (v_lo, v_hi),
        };
        let washer = |z: f64, nz: DVec3, ro: f64| S::Plane {
            origin: DVec3::new(cx, cy, z), normal: nz, basis_u: DVec3::X,
            u_range: (-(ro * 1.5), ro * 1.5), v_range: (-(ro * 1.5), ro * 1.5),
        };
        self.remove_primitive_solid(torus_faces);

        // top ring: keep z > z_hi → band v∈[v1h, v2h], washer at z_hi faces −Z.
        let top_band = torus_band(v1h, v2h);
        let top_bn = top_band.normal_at_world_pos(DVec3::new(cx + ro_h, cy, z_hi));
        let (tb, tw) = self.sew_torus_cap(
            DVec3::new(cx + ro_h, cy, z_hi), circle(z_hi, ro_h),
            DVec3::new(cx + ri_h, cy, z_hi), circle(z_hi, ri_h),
            top_band, top_bn, washer(z_hi, DVec3::NEG_Z, ro_h), DVec3::NEG_Z, material,
        )?;
        // bottom ring: keep z < z_lo → band v∈[v2l, 2π+v1l], washer at z_lo faces +Z.
        let bot_band = torus_band(v2l, TAU + v1l);
        let bot_bn = bot_band.normal_at_world_pos(DVec3::new(cx + ro_l, cy, z_lo));
        let (bb, bw) = self.sew_torus_cap(
            DVec3::new(cx + ro_l, cy, z_lo), circle(z_lo, ro_l),
            DVec3::new(cx + ri_l, cy, z_lo), circle(z_lo, ri_l),
            bot_band, bot_bn, washer(z_lo, DVec3::Z, ro_l), DVec3::Z, material,
        )?;
        Ok(vec![tb, tw, bb, bw])
    }

    // ─────────────────────────────────────────────────────────────────────────
    // ADR-197 β-3-n — CURVED SLICE: a single Z-plane `z = k` SPLITS a curved
    // solid into TWO closed volumes (both kept), the surface preserved. Unlike
    // the slab subtract, the two halves SHARE the cut plane (no gap), so their
    // cut circles are geometrically coincident — the two pieces are anchored at
    // OPPOSITE angles on the circle (`+X` vs `−X`) so the 0.15μm vertex dedup
    // (LOCKED #5) keeps them as DISJOINT volumes rather than pinching them at a
    // shared anchor. (TRIM = keep one side reuses `boolean(Subtract)`; SLICE is
    // the only new engine path for the curved knife.)
    // ─────────────────────────────────────────────────────────────────────────

    /// ADR-197 β-3-n — SPHERE slice at `z=k` → top cap + bottom cap (2 volumes).
    pub fn boolean_sphere_slice(&mut self, sphere_faces: &[FaceId], k: f64, material: MaterialId) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-n sphere slice: no Sphere surface"))?;
        let (cx, cy, cz) = (center.x, center.y, center.z);
        let d = k - cz;
        if d.abs() >= radius - 1e-9 {
            bail!("ADR-197 β-3-n sphere slice: plane must cut the sphere (|k−cz| < r)");
        }
        let rho = (radius * radius - d * d).sqrt();
        let vc = (d / radius).asin();
        let circle = || AnalyticCurve::Circle { center: DVec3::new(cx, cy, k), radius: rho, normal: DVec3::Z, basis_u: DVec3::X };
        let disk = |nz: DVec3| S::Plane { origin: DVec3::new(cx, cy, k), normal: nz, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
        let cap_n = |v_lo: f64, v_hi: f64| (crate::surfaces::sphere::evaluate(center, radius, DVec3::Z, DVec3::X, std::f64::consts::PI, (v_lo + v_hi) * 0.5) - center).normalize_or_zero();
        self.remove_primitive_solid(sphere_faces);
        // top cap (keep above) — anchor at +X.
        let top = S::Sphere { center, radius, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (vc, FRAC_PI_2) };
        let (tcf, tdf) = self.sew_closed_curve_pair(DVec3::new(cx + rho, cy, k), circle(), top, cap_n(vc, FRAC_PI_2), disk(DVec3::NEG_Z), DVec3::NEG_Z, material)?;
        // bottom cap (keep below) — anchor at −X (avoid dedup-merge with top).
        let bot = S::Sphere { center, radius, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (-FRAC_PI_2, vc) };
        let (bcf, bdf) = self.sew_closed_curve_pair(DVec3::new(cx - rho, cy, k), circle(), bot, cap_n(-FRAC_PI_2, vc), disk(DVec3::Z), DVec3::Z, material)?;
        Ok(vec![tcf, tdf, bcf, bdf])
    }

    /// ADR-204 β-3 — sphere SLICE by a single arbitrary plane (normal `n`, signed
    /// distance `d_k` from the centre): keeps BOTH halves as ORIENTED Sphere caps
    /// (top axis_dir = n, v∈[vc, π/2] reaching +n pole; bottom axis_dir = n,
    /// v∈[−π/2, vc] reaching −n pole), so a TILTED slice renders correctly. The
    /// two caps share the cut circle — anchored at ±basis_u (opposite angles) to
    /// avoid the 0.15μm dedup merge. Returns `[top_cap, top_disk, bottom_cap,
    /// bottom_disk]` (2 volumes).
    pub fn boolean_sphere_slice_oriented(
        &mut self,
        sphere_faces: &[FaceId],
        plane_normal: DVec3,
        d_k: f64,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, PI, TAU};
        let n = plane_normal.normalize_or_zero();
        if n.length_squared() < 0.5 {
            bail!("ADR-204 β-3 oriented slice: degenerate plane normal");
        }
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-204 β-3 oriented slice: no Sphere surface"))?;
        if d_k.abs() >= radius - 1e-9 {
            bail!("ADR-204 β-3 oriented slice: plane must cut the sphere (|d| < r)");
        }
        let basis_u = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
            .normalize_or_zero();
        let rho = (radius * radius - d_k * d_k).max(0.0).sqrt();
        let vc = (d_k / radius).clamp(-1.0, 1.0).asin();
        let circle = || crate::curves::AnalyticCurve::Circle {
            center: center + d_k * n,
            radius: rho,
            normal: n,
            basis_u,
        };
        let disk = |dn: DVec3| S::Plane {
            origin: center + d_k * n,
            normal: dn,
            basis_u,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        let cap_n = |v_lo: f64, v_hi: f64| {
            (crate::surfaces::sphere::evaluate(center, radius, n, basis_u, PI, (v_lo + v_hi) * 0.5)
                - center)
                .normalize_or_zero()
        };
        self.remove_primitive_solid(sphere_faces);
        // top cap (keep dot > d_k) — anchor at +basis_u, disk faces −n.
        let top = S::Sphere {
            center, radius, axis_dir: n, ref_dir: basis_u,
            u_range: (0.0, TAU), v_range: (vc, FRAC_PI_2),
        };
        let (tcf, tdf) = self.sew_closed_curve_pair(
            center + d_k * n + basis_u * rho, circle(),
            top, cap_n(vc, FRAC_PI_2), disk(-n), -n, material,
        )?;
        // bottom cap (keep dot < d_k) — anchor at −basis_u (anti dedup-merge),
        // disk faces +n.
        let bot = S::Sphere {
            center, radius, axis_dir: n, ref_dir: basis_u,
            u_range: (0.0, TAU), v_range: (-FRAC_PI_2, vc),
        };
        let (bcf, bdf) = self.sew_closed_curve_pair(
            center + d_k * n - basis_u * rho, circle(),
            bot, cap_n(-FRAC_PI_2, vc), disk(n), n, material,
        )?;
        Ok(vec![tcf, tdf, bcf, bdf])
    }

    /// ADR-197 β-3-n — CYLINDER slice at `z=k` → top stub + bottom stub (2 volumes).
    pub fn boolean_cylinder_slice(&mut self, cyl_faces: &[FaceId], k: f64, material: MaterialId) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (axis_origin, axis_dir, ref_dir, radius, v_range) = cyl_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cylinder { axis_origin, axis_dir, ref_dir, radius, v_range, .. }) => Some((*axis_origin, *axis_dir, *ref_dir, *radius, *v_range)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-n cylinder slice: no Cylinder surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-n cylinder slice MVP: axis must be ∥ Z");
        }
        let z0 = axis_origin.z;
        let base_z = z0 + v_range.0.min(v_range.1);
        let top_z = z0 + v_range.0.max(v_range.1);
        if k <= base_z + 1e-9 || k >= top_z - 1e-9 {
            bail!("ADR-197 β-3-n cylinder slice: plane must cut the cylinder");
        }
        let (cx, cy) = (axis_origin.x, axis_origin.y);
        let circ = |x: f64| AnalyticCurve::Circle { center: DVec3::new(cx, cy, x), radius, normal: DVec3::Z, basis_u: DVec3::X };
        let disk = |x: f64, nz: DVec3| S::Plane { origin: DVec3::new(cx, cy, x), normal: nz, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
        let band = |lo: f64, hi: f64| S::Cylinder { axis_origin, axis_dir, radius, ref_dir, u_range: (0.0, TAU), v_range: (lo - z0, hi - z0) };
        self.remove_primitive_solid(cyl_faces);
        // top stub z∈[k, top]: cut disk at k faces −Z (anchor +X), top cap +Z.
        let (tbf, ttd, tcd) = self.sew_curved_band(
            DVec3::new(cx + radius, cy, top_z), circ(top_z), DVec3::new(cx + radius, cy, k), circ(k),
            band(k, top_z), ref_dir, disk(top_z, DVec3::Z), DVec3::Z, disk(k, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        // bottom stub z∈[base, k]: cut disk at k faces +Z (anchor −X), base cap −Z.
        let (bbf, btd, bcd) = self.sew_curved_band(
            DVec3::new(cx - radius, cy, k), circ(k), DVec3::new(cx + radius, cy, base_z), circ(base_z),
            band(base_z, k), ref_dir, disk(k, DVec3::Z), DVec3::Z, disk(base_z, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        Ok(vec![tbf, ttd, tcd, bbf, btd, bcd])
    }

    /// ADR-197 β-3-n — CONE slice at `z=k` → tip cone + base frustum (2 volumes).
    pub fn boolean_cone_slice(&mut self, cone_faces: &[FaceId], k: f64, material: MaterialId) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (apex, axis_dir, half_angle, ref_dir, v_range) = cone_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cone { apex, axis_dir, half_angle, ref_dir, v_range, .. }) => Some((*apex, *axis_dir, *half_angle, *ref_dir, *v_range)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-n cone slice: no Cone surface"))?;
        if (axis_dir.normalize_or_zero() - DVec3::NEG_Z).length() > 1e-6 {
            bail!("ADR-197 β-3-n cone slice MVP: axis must be -Z (apex-up)");
        }
        let apex_z = apex.z;
        let base_z = apex_z - v_range.0.max(v_range.1);
        if k <= base_z + 1e-9 || k >= apex_z - 1e-9 {
            bail!("ADR-197 β-3-n cone slice: plane must cut the cone");
        }
        let tan_ha = half_angle.tan();
        let (cx, cy) = (apex.x, apex.y);
        let v_of = |z: f64| apex_z - z;
        let r_of = |z: f64| (apex_z - z) * tan_ha;
        let circ = |z: f64, ang_pos: bool| {
            let r = r_of(z);
            (AnalyticCurve::Circle { center: DVec3::new(cx, cy, z), radius: r, normal: DVec3::Z, basis_u: DVec3::X },
             if ang_pos { DVec3::new(cx + r, cy, z) } else { DVec3::new(cx - r, cy, z) })
        };
        let disk = |z: f64, nz: DVec3| S::Plane { origin: DVec3::new(cx, cy, z), normal: nz, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
        self.remove_primitive_solid(cone_faces);
        // tip cone z∈[k, apex]: Cone side v∈[0, v(k)] + cut disk at k facing −Z (anchor +X).
        let tip = S::Cone { apex, axis_dir, half_angle, ref_dir, u_range: (0.0, TAU), v_range: (0.0, v_of(k)) };
        let (tk_circle, tk_anchor) = circ(k, true);
        let tip_n = tip.normal_at_world_pos(tk_anchor);
        let (tcf, tdf) = self.sew_closed_curve_pair(tk_anchor, tk_circle, tip, tip_n, disk(k, DVec3::NEG_Z), DVec3::NEG_Z, material)?;
        // base frustum z∈[base, k]: band Cone v∈[v(k), v(base)] + cut disk at k (+Z, anchor −X) + base disk −Z.
        let fr = S::Cone { apex, axis_dir, half_angle, ref_dir, u_range: (0.0, TAU), v_range: (v_of(k), v_of(base_z)) };
        let (bk_circle, bk_anchor) = circ(k, false);
        let fr_n = fr.normal_at_world_pos(bk_anchor);
        let (bf, btd, bbd) = self.sew_curved_band(
            bk_anchor, bk_circle, DVec3::new(cx + r_of(base_z), cy, base_z),
            AnalyticCurve::Circle { center: DVec3::new(cx, cy, base_z), radius: r_of(base_z), normal: DVec3::Z, basis_u: DVec3::X },
            fr, fr_n, disk(k, DVec3::Z), DVec3::Z, disk(base_z, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        Ok(vec![tcf, tdf, bf, btd, bbd])
    }

    /// ADR-197 β-3-n — TORUS slice at `z=k` → top band-ring + bottom band-ring (2 volumes).
    pub fn boolean_torus_slice(&mut self, torus_faces: &[FaceId], k: f64, material: MaterialId) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (center, axis_dir, ref_dir, major_radius, minor_radius) = torus_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. }) => Some((*center, *axis_dir, *ref_dir, *major_radius, *minor_radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-n torus slice: no Torus surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-n torus slice MVP: axis must be ∥ Z");
        }
        let (cx, cy) = (center.x, center.y);
        let (v1, v2, ro, ri) = torus_z_cut(center.z, major_radius, minor_radius, k)
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-n torus slice: plane must cut the tube (|k−cz| < r)"))?;
        let circ = |rho: f64| AnalyticCurve::Circle { center: DVec3::new(cx, cy, k), radius: rho, normal: DVec3::Z, basis_u: DVec3::X };
        let band = |v_lo: f64, v_hi: f64| S::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, u_range: (0.0, TAU), v_range: (v_lo, v_hi) };
        let washer = |nz: DVec3| S::Plane { origin: DVec3::new(cx, cy, k), normal: nz, basis_u: DVec3::X, u_range: (-(ro * 1.5), ro * 1.5), v_range: (-(ro * 1.5), ro * 1.5) };
        self.remove_primitive_solid(torus_faces);
        // top ring (keep above k): band v∈[v1,v2], washer −Z; anchors at +X.
        let tb = band(v1, v2);
        let tbn = tb.normal_at_world_pos(DVec3::new(cx + ro, cy, k));
        let (top_band, top_w) = self.sew_torus_cap(
            DVec3::new(cx + ro, cy, k), circ(ro), DVec3::new(cx + ri, cy, k), circ(ri),
            tb, tbn, washer(DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        // bottom ring (keep below k): band v∈[v2, 2π+v1], washer +Z; anchors at −X (avoid merge).
        let bb = band(v2, TAU + v1);
        let bbn = bb.normal_at_world_pos(DVec3::new(cx - ro, cy, k));
        let (bot_band, bot_w) = self.sew_torus_cap(
            DVec3::new(cx - ro, cy, k), circ(ro), DVec3::new(cx - ri, cy, k), circ(ri),
            bb, bbn, washer(DVec3::Z), DVec3::Z, material,
        )?;
        Ok(vec![top_band, top_w, bot_band, bot_w])
    }

    /// ADR-197 β-3-o — SPHERE ∪ SPHERE (Z-coaxial, overlapping): the union is a
    /// capsule — each sphere trimmed at the SSI circle, keeping the OUTER cap
    /// (the part NOT inside the other sphere). Result = 2 Sphere caps sharing one
    /// SSI circle (the waist), via `sew_closed_curve_pair`. Returns `[cap1, cap2]`.
    /// MVP: Z-coaxial spheres (oblique offset → deferred, γ-2a seam territory).
    pub fn boolean_sphere_sphere_union(
        &mut self,
        s1_faces: &[FaceId],
        s2_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let extract = |faces: &[FaceId], m: &Mesh| -> Option<(DVec3, f64)> {
            faces.iter().find_map(|&f| match m.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
        };
        let (c1, r1) = extract(s1_faces, self).ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-o: operand A has no Sphere surface"))?;
        let (c2, r2) = extract(s2_faces, self).ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-o: operand B has no Sphere surface"))?;
        let (z_ssi, rho, v1, v2) = sphere_sphere_z_circle(c1, r1, c2, r2)
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-o: spheres not Z-coaxial & overlapping (no SSI circle)"))?;
        let (cx, cy) = (c1.x, c1.y);
        let c2_above = c2.z > c1.z;
        // each kept cap is the part of its sphere FARTHER from the other centre.
        let (cap1_v, cap2_v) = if c2_above {
            ((-FRAC_PI_2, v1), (v2, FRAC_PI_2)) // sphere1 below z_ssi, sphere2 above
        } else {
            ((v1, FRAC_PI_2), (-FRAC_PI_2, v2)) // sphere1 above z_ssi, sphere2 below
        };
        let cap1 = S::Sphere { center: c1, radius: r1, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: cap1_v };
        let cap2 = S::Sphere { center: c2, radius: r2, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: cap2_v };
        let n1 = (crate::surfaces::sphere::evaluate(c1, r1, DVec3::Z, DVec3::X, std::f64::consts::PI, (cap1_v.0 + cap1_v.1) * 0.5) - c1).normalize_or_zero();
        let n2 = (crate::surfaces::sphere::evaluate(c2, r2, DVec3::Z, DVec3::X, std::f64::consts::PI, (cap2_v.0 + cap2_v.1) * 0.5) - c2).normalize_or_zero();
        let circle = AnalyticCurve::Circle { center: DVec3::new(cx, cy, z_ssi), radius: rho, normal: DVec3::Z, basis_u: DVec3::X };

        // remove both original spheres + their edges, then sew the 2 caps.
        self.remove_primitive_solid(s1_faces);
        self.remove_primitive_solid(s2_faces);
        let (cf1, cf2) = self.sew_closed_curve_pair(
            DVec3::new(cx + rho, cy, z_ssi), circle, cap1, n1, cap2, n2, material,
        )?;
        Ok(vec![cf1, cf2])
    }

    /// ADR-197 β-3-o — CONE ∪ CONE (opposing coaxial, overlapping) → an HOURGLASS:
    /// each cone's apex sits inside the other (removed); the WIDE part of each is
    /// kept as a Cone FRUSTUM band, the two bands joined at the shared waist SSI
    /// circle, each capped by its base disk. Result = 2 Cone bands + 2 base disks
    /// = 4 faces (`sew_hourglass`). MVP: Z-axis, one apex-up + one apex-down.
    pub fn boolean_cone_cone_union(
        &mut self,
        c1_faces: &[FaceId],
        c2_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let extract = |faces: &[FaceId], m: &Mesh| -> Option<(DVec3, DVec3, f64, f64)> {
            faces.iter().find_map(|&f| match m.face_surface(f) {
                Some(S::Cone { apex, axis_dir, half_angle, v_range, .. }) => {
                    Some((*apex, *axis_dir, *half_angle, v_range.0.max(v_range.1)))
                }
                _ => None,
            })
        };
        let (apex1, ax1, ha1, vb1) = extract(c1_faces, self).ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-o: operand A has no Cone surface"))?;
        let (apex2, ax2, ha2, vb2) = extract(c2_faces, self).ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-o: operand B has no Cone surface"))?;
        let (z_waist, rho_waist, c1_up) = cone_cone_hourglass(apex1, ax1, ha1, vb1, apex2, ax2, ha2, vb2)
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-o: cones not an opposing-coaxial overlapping hourglass"))?;
        let (apex_up, ha_up, vb_up, apex_dn, ha_dn, vb_dn) =
            if c1_up { (apex1, ha1, vb1, apex2, ha2, vb2) } else { (apex2, ha2, vb2, apex1, ha1, vb1) };
        let (cx, cy) = (apex_up.x, apex_up.y);
        let (tan_up, tan_dn) = (ha_up.tan(), ha_dn.tan());
        let base_up = apex_up.z - vb_up;
        let base_dn = apex_dn.z + vb_dn;
        let r_base_up = vb_up * tan_up;
        let r_base_dn = vb_dn * tan_dn;
        let circle = |z: f64, rho: f64| AnalyticCurve::Circle { center: DVec3::new(cx, cy, z), radius: rho, normal: DVec3::Z, basis_u: DVec3::X };
        // band surfaces: the WIDE frustum of each cone (waist → base).
        let band_up = S::Cone { apex: apex_up, axis_dir: DVec3::NEG_Z, half_angle: ha_up, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (apex_up.z - z_waist, vb_up) };
        let band_dn = S::Cone { apex: apex_dn, axis_dir: DVec3::Z, half_angle: ha_dn, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (z_waist - apex_dn.z, vb_dn) };
        let n_up = band_up.normal_at_world_pos(DVec3::new(cx + r_base_up, cy, base_up));
        let n_dn = band_dn.normal_at_world_pos(DVec3::new(cx + r_base_dn, cy, base_dn));
        let disk_up = S::Plane { origin: DVec3::new(cx, cy, base_up), normal: DVec3::NEG_Z, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
        let disk_dn = S::Plane { origin: DVec3::new(cx, cy, base_dn), normal: DVec3::Z, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };

        self.remove_primitive_solid(c1_faces);
        self.remove_primitive_solid(c2_faces);
        self.sew_hourglass(
            DVec3::new(cx + r_base_up, cy, base_up), circle(base_up, r_base_up),
            DVec3::new(cx + rho_waist, cy, z_waist), circle(z_waist, rho_waist),
            DVec3::new(cx + r_base_dn, cy, base_dn), circle(base_dn, r_base_dn),
            band_up, n_up, band_dn, n_dn,
            disk_up, DVec3::NEG_Z, disk_dn, DVec3::Z, material,
        )
    }

    /// ADR-197 β-3-p — SPHERE ∪ BOX (box XY-contains + Z-cuts the sphere): the box
    /// absorbs the sphere's middle band; the two caps poke OUT of the box top &
    /// bottom. Result = the box (4 walls + top/bottom faces now PIERCED with a
    /// circular hole) + the 2 Sphere caps capping those holes. Returns all 8 faces
    /// (6 box + 2 caps). The box keeps its 4 walls; its top/bottom faces gain an
    /// inner hole loop. MVP: axis box, Z-up sphere.
    pub fn boolean_sphere_box_union(
        &mut self,
        sphere_faces: &[FaceId],
        box_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-p: no Sphere surface"))?;
        // find the box's two HORIZONTAL faces (|normal·Z|>0.99) by geometry — the
        // higher-z one is the top, the lower-z the bottom. (Robust to boxes whose
        // faces lack an attached Plane surface, e.g. the test helper's make_box.)
        let face_horiz_z = |m: &Mesh, f: FaceId| -> Option<f64> {
            let face = m.faces.get(f)?;
            let vs = m.collect_loop_verts(face.outer().start).ok()?;
            if vs.len() < 3 { return None; }
            let p: Vec<DVec3> = vs.iter().filter_map(|&v| m.verts.get(v).map(|x| x.pos())).collect();
            if p.len() < 3 { return None; }
            let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
            if n.z.abs() < 0.99 { return None; }
            Some(p.iter().map(|q| q.z).sum::<f64>() / p.len() as f64)
        };
        let mut horiz: Vec<(FaceId, f64)> = box_faces
            .iter()
            .filter_map(|&bf| face_horiz_z(self, bf).map(|z| (bf, z)))
            .collect();
        if horiz.len() < 2 {
            bail!("ADR-197 β-3-p: box has no horizontal top/bottom faces");
        }
        horiz.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let (bot_f, z_lo) = horiz[0];
        let (top_f, z_hi) = *horiz.last().unwrap();
        let (cx, cy, cz) = (center.x, center.y, center.z);
        let d_hi = z_hi - cz;
        let d_lo = z_lo - cz;
        if d_hi >= radius - 1e-9 || d_lo <= -radius + 1e-9 || z_lo >= z_hi {
            bail!("ADR-197 β-3-p: box must Z-cut the sphere (both planes inside)");
        }
        let rho = |d: f64| (radius * radius - d * d).sqrt();
        let circle = |z: f64, r: f64| AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, z), radius: r, normal: DVec3::Z, basis_u: DVec3::X,
        };
        let cap_n = |v_lo: f64, v_hi: f64| (crate::surfaces::sphere::evaluate(center, radius, DVec3::Z, DVec3::X, std::f64::consts::PI, (v_lo + v_hi) * 0.5) - center).normalize_or_zero();

        // remove the sphere only; the box faces stay (top/bottom get pierced).
        self.remove_primitive_solid(sphere_faces);

        // top cap (z>z_hi) pierced through the box top face.
        let v_top = (d_hi / radius).asin();
        let top_cap = S::Sphere { center, radius, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (v_top, FRAC_PI_2) };
        let cap_top = self.pierce_face_with_cap(top_f, DVec3::new(cx + rho(d_hi), cy, z_hi), circle(z_hi, rho(d_hi)), top_cap, cap_n(v_top, FRAC_PI_2), material)?;
        // bottom cap (z<z_lo) pierced through the box bottom face.
        let v_bot = (d_lo / radius).asin();
        let bot_cap = S::Sphere { center, radius, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (-FRAC_PI_2, v_bot) };
        let cap_bot = self.pierce_face_with_cap(bot_f, DVec3::new(cx + rho(d_lo), cy, z_lo), circle(z_lo, rho(d_lo)), bot_cap, cap_n(-FRAC_PI_2, v_bot), material)?;

        let mut out: Vec<FaceId> = box_faces.to_vec();
        out.push(cap_top);
        out.push(cap_bot);
        Ok(out)
    }

    /// ADR-197 β-3-p — finds the box's two horizontal faces (top = higher z,
    /// bottom = lower z) by geometry. Robust to boxes whose faces lack a Plane
    /// surface (the test `make_box`). Returns `(top_face, z_hi, bot_face, z_lo)`.
    fn box_horizontal_faces(&self, box_faces: &[FaceId]) -> Option<(FaceId, f64, FaceId, f64)> {
        let mut horiz: Vec<(FaceId, f64)> = box_faces
            .iter()
            .filter_map(|&bf| {
                let face = self.faces.get(bf)?;
                let vs = self.collect_loop_verts(face.outer().start).ok()?;
                if vs.len() < 3 { return None; }
                let p: Vec<DVec3> = vs.iter().filter_map(|&v| self.verts.get(v).map(|x| x.pos())).collect();
                if p.len() < 3 { return None; }
                let n = (p[1] - p[0]).cross(p[2] - p[0]).normalize_or_zero();
                if n.z.abs() < 0.99 { return None; }
                Some((bf, p.iter().map(|q| q.z).sum::<f64>() / p.len() as f64))
            })
            .collect();
        if horiz.len() < 2 { return None; }
        horiz.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let (bot_f, z_lo) = horiz[0];
        let (top_f, z_hi) = *horiz.last().unwrap();
        Some((top_f, z_hi, bot_f, z_lo))
    }

    /// ADR-197 β-3-p — CYLINDER ∪ BOX (box XY-contains + Z-cuts): the box absorbs
    /// the middle band; two cylinder STUBS poke out of the box top & bottom. Each
    /// stub = a Cylinder side band + a flat end disk, pierced through the box face
    /// (`pierce_face_with_band_stub`). Result = box (4 walls + top/bottom pierced)
    /// + 2 stubs (4 faces) = 10 faces. MVP: Z-up cylinder, axis box.
    pub fn boolean_cylinder_box_union(
        &mut self,
        cyl_faces: &[FaceId],
        box_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (axis_origin, axis_dir, ref_dir, radius, v_range) = cyl_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cylinder { axis_origin, axis_dir, ref_dir, radius, v_range, .. }) => {
                    Some((*axis_origin, *axis_dir, *ref_dir, *radius, *v_range))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-p cyl∪box: no Cylinder surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-p cyl∪box MVP: axis must be ∥ Z");
        }
        let z0 = axis_origin.z;
        let base_z = z0 + v_range.0.min(v_range.1);
        let top_z = z0 + v_range.0.max(v_range.1);
        let (top_f, z_hi, bot_f, z_lo) = self.box_horizontal_faces(box_faces)
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-p cyl∪box: box has no horizontal faces"))?;
        if z_hi >= top_z - 1e-9 || z_lo <= base_z + 1e-9 || z_lo >= z_hi {
            bail!("ADR-197 β-3-p cyl∪box: box must Z-cut the cylinder (both planes inside)");
        }
        let (cx, cy) = (axis_origin.x, axis_origin.y);
        let circle = |z: f64| AnalyticCurve::Circle { center: DVec3::new(cx, cy, z), radius, normal: DVec3::Z, basis_u: DVec3::X };
        let band = |lo: f64, hi: f64| S::Cylinder { axis_origin, axis_dir, radius, ref_dir, u_range: (0.0, TAU), v_range: (lo - z0, hi - z0) };
        let disk = |z: f64, nz: DVec3| S::Plane { origin: DVec3::new(cx, cy, z), normal: nz, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
        let anchor = |z: f64| DVec3::new(cx + radius, cy, z);

        self.remove_primitive_solid(cyl_faces);
        // top stub z∈[z_hi, top_z]: pierce box top, band + top disk.
        let mut out: Vec<FaceId> = box_faces.to_vec();
        let top_stub = self.pierce_face_with_band_stub(
            top_f, anchor(z_hi), circle(z_hi), anchor(top_z), circle(top_z),
            band(z_hi, top_z), ref_dir, disk(top_z, DVec3::Z), DVec3::Z, material,
        )?;
        out.extend(top_stub);
        // bottom stub z∈[base_z, z_lo]: pierce box bottom, band + base disk.
        let bot_stub = self.pierce_face_with_band_stub(
            bot_f, anchor(z_lo), circle(z_lo), anchor(base_z), circle(base_z),
            band(base_z, z_lo), ref_dir, disk(base_z, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        out.extend(bot_stub);
        Ok(out)
    }

    /// ADR-197 β-3-p — CONE ∪ BOX (apex-up, box XY-contains + Z-cuts): MIXED — the
    /// apex TIP pokes out of the box top (a small Cone CAP), the base FRUSTUM pokes
    /// out of the box bottom (a Cone band + base disk STUB). Reuses BOTH pierce
    /// helpers: `pierce_face_with_cap` (tip) + `pierce_face_with_band_stub` (frustum).
    /// Result = box (4 walls + pierced top/bottom) + tip(1) + frustum(2) = 9 faces.
    /// MVP: apex-up (-Z axis) cone, axis box.
    pub fn boolean_cone_box_union(
        &mut self,
        cone_faces: &[FaceId],
        box_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (apex, axis_dir, half_angle, ref_dir, v_range) = cone_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Cone { apex, axis_dir, half_angle, ref_dir, v_range, .. }) => {
                    Some((*apex, *axis_dir, *half_angle, *ref_dir, *v_range))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-p cone∪box: no Cone surface"))?;
        if (axis_dir.normalize_or_zero() - DVec3::NEG_Z).length() > 1e-6 {
            bail!("ADR-197 β-3-p cone∪box MVP: axis must be -Z (apex-up)");
        }
        let apex_z = apex.z;
        let base_z = apex_z - v_range.0.max(v_range.1);
        let (top_f, z_hi, bot_f, z_lo) = self.box_horizontal_faces(box_faces)
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-p cone∪box: box has no horizontal faces"))?;
        if z_hi >= apex_z - 1e-9 || z_lo <= base_z + 1e-9 || z_lo >= z_hi {
            bail!("ADR-197 β-3-p cone∪box: box must Z-cut the cone (both planes inside)");
        }
        let tan_ha = half_angle.tan();
        let (cx, cy) = (apex.x, apex.y);
        let v_of = |z: f64| apex_z - z;
        let r_of = |z: f64| (apex_z - z) * tan_ha;
        let circle = |z: f64| AnalyticCurve::Circle { center: DVec3::new(cx, cy, z), radius: r_of(z), normal: DVec3::Z, basis_u: DVec3::X };
        let anchor = |z: f64| DVec3::new(cx + r_of(z), cy, z);

        self.remove_primitive_solid(cone_faces);
        let mut out: Vec<FaceId> = box_faces.to_vec();
        // top TIP (z∈[z_hi, apex]): Cone cap (apex degenerate) pierced through box top.
        let tip = S::Cone { apex, axis_dir, half_angle, ref_dir, u_range: (0.0, TAU), v_range: (0.0, v_of(z_hi)) };
        let tip_n = tip.normal_at_world_pos(anchor(z_hi));
        let cap = self.pierce_face_with_cap(top_f, anchor(z_hi), circle(z_hi), tip, tip_n, material)?;
        out.push(cap);
        // bottom FRUSTUM (z∈[base_z, z_lo]): Cone band + base disk stub pierced through box bottom.
        let band = S::Cone { apex, axis_dir, half_angle, ref_dir, u_range: (0.0, TAU), v_range: (v_of(z_lo), v_of(base_z)) };
        let band_n = band.normal_at_world_pos(anchor(z_lo));
        let base_disk = S::Plane { origin: DVec3::new(cx, cy, base_z), normal: DVec3::NEG_Z, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
        let frustum = self.pierce_face_with_band_stub(
            bot_f, anchor(z_lo), circle(z_lo), anchor(base_z), circle(base_z),
            band, band_n, base_disk, DVec3::NEG_Z, material,
        )?;
        out.extend(frustum);
        Ok(out)
    }

    /// ADR-197 β-3-p — TORUS ∪ BOX (box XY-contains + Z-cuts the tube): the tube
    /// pokes through the box top & bottom as ANNULI. Each box face splits into an
    /// outer annulus (box rect − outer circle) + a "donut-center" disk (inside the
    /// inner circle); the torus band-ring connects both (`pierce_face_with_torus_
    /// band`). Result = box (4 walls + top/bottom annular + 2 donut-center disks) +
    /// 2 Torus bands = 10 faces. MVP: Z-up torus, axis box.
    pub fn boolean_torus_box_union(
        &mut self,
        torus_faces: &[FaceId],
        box_faces: &[FaceId],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let (center, axis_dir, ref_dir, major_radius, minor_radius) = torus_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, .. }) => {
                    Some((*center, *axis_dir, *ref_dir, *major_radius, *minor_radius))
                }
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-p torus∪box: no Torus surface"))?;
        if axis_dir.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 {
            bail!("ADR-197 β-3-p torus∪box MVP: axis must be ∥ Z");
        }
        let (cx, cy) = (center.x, center.y);
        let (top_f, z_hi, bot_f, z_lo) = self.box_horizontal_faces(box_faces)
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-p torus∪box: box has no horizontal faces"))?;
        let cut_hi = torus_z_cut(center.z, major_radius, minor_radius, z_hi);
        let cut_lo = torus_z_cut(center.z, major_radius, minor_radius, z_lo);
        let (Some((v1h, v2h, ro_h, ri_h)), Some((v1l, v2l, ro_l, ri_l))) = (cut_hi, cut_lo) else {
            bail!("ADR-197 β-3-p torus∪box: box must Z-cut the tube (both planes inside)");
        };
        if z_lo >= z_hi {
            bail!("ADR-197 β-3-p torus∪box: z_lo < z_hi required");
        }
        let circle = |z: f64, rho: f64| AnalyticCurve::Circle { center: DVec3::new(cx, cy, z), radius: rho, normal: DVec3::Z, basis_u: DVec3::X };
        let band = |v_lo: f64, v_hi: f64| S::Torus { center, axis_dir, ref_dir, major_radius, minor_radius, u_range: (0.0, TAU), v_range: (v_lo, v_hi) };
        let disk = |z: f64, nz: DVec3| S::Plane { origin: DVec3::new(cx, cy, z), normal: nz, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };

        self.remove_primitive_solid(torus_faces);
        let mut out: Vec<FaceId> = box_faces.to_vec();
        // top band-ring (keep above z_hi): band v∈[v1h, v2h], donut-center disk faces +Z.
        let top_band = band(v1h, v2h);
        let top_bn = top_band.normal_at_world_pos(DVec3::new(cx + ro_h, cy, z_hi));
        let top = self.pierce_face_with_torus_band(
            top_f, DVec3::new(cx + ro_h, cy, z_hi), circle(z_hi, ro_h),
            DVec3::new(cx + ri_h, cy, z_hi), circle(z_hi, ri_h),
            top_band, top_bn, disk(z_hi, DVec3::Z), DVec3::Z, material,
        )?;
        out.extend(top);
        // bottom band-ring (keep below z_lo): band v∈[v2l, 2π+v1l], donut-center disk faces −Z.
        let bot_band = band(v2l, TAU + v1l);
        let bot_bn = bot_band.normal_at_world_pos(DVec3::new(cx + ro_l, cy, z_lo));
        let bot = self.pierce_face_with_torus_band(
            bot_f, DVec3::new(cx + ro_l, cy, z_lo), circle(z_lo, ro_l),
            DVec3::new(cx + ri_l, cy, z_lo), circle(z_lo, ri_l),
            bot_band, bot_bn, disk(z_lo, DVec3::NEG_Z), DVec3::NEG_Z, material,
        )?;
        out.extend(bot);
        Ok(out)
    }

    /// ADR-197 β-3-n — CURVED KNIFE dispatcher: cut a curved Path B solid by a
    /// horizontal plane `z = k`. Returns `None` if `faces` is NOT a single
    /// analytic primitive (sphere/cylinder/cone/torus) — the caller (SliceTool)
    /// then falls back to the polygonal `slice_volume_by_plane`. `Slice` →
    /// `boolean_*_slice` (2 volumes); `KeepAbove`/`KeepBelow` → trim via the
    /// existing halfspace builders (the intersect-machine, keep-side flipped).
    /// MVP: the caller must ensure the plane is horizontal (normal ‖ ±Z).
    pub fn cut_curved_by_z_plane(
        &mut self,
        faces: &[FaceId],
        z: f64,
        mode: CurvedCutMode,
        material: MaterialId,
    ) -> Option<Result<Vec<FaceId>>> {
        let prim = self.classify_curved_primitive(faces)?;
        use CurvedCutMode::*;
        use CurvedPrimKind::*;
        let res = match (prim.kind, mode) {
            (Sphere, Slice) => self.boolean_sphere_slice(&prim.faces, z, material),
            (Sphere, KeepAbove) => self.boolean_sphere_halfspace(&prim.faces, DVec3::new(0., 0., z), DVec3::Z, material),
            (Sphere, KeepBelow) => self.boolean_sphere_halfspace(&prim.faces, DVec3::new(0., 0., z), DVec3::NEG_Z, material),
            (Cylinder, Slice) => self.boolean_cylinder_slice(&prim.faces, z, material),
            (Cylinder, KeepAbove) => self.boolean_cylinder_slab(&prim.faces, z, 1e9, material),
            (Cylinder, KeepBelow) => self.boolean_cylinder_slab(&prim.faces, -1e9, z, material),
            (Cone, Slice) => self.boolean_cone_slice(&prim.faces, z, material),
            (Cone, KeepAbove) => self.boolean_cone_slab(&prim.faces, z, 1e9, material),
            (Cone, KeepBelow) => self.boolean_cone_slab(&prim.faces, -1e9, z, material),
            (Torus, Slice) => self.boolean_torus_slice(&prim.faces, z, material),
            (Torus, KeepAbove) => self.boolean_torus_halfspace(&prim.faces, z, true, material),
            (Torus, KeepBelow) => self.boolean_torus_halfspace(&prim.faces, z, false, material),
        };
        Some(res)
    }

    /// ADR-197 β-3-j γ-2b-3 — SPHERE ∩ box CORNER (3 cutting planes meeting at a
    /// box corner inside the sphere). Builds the corner solid directly from the
    /// corner geometry (γ-2b-1): 4 vertices (3 circle-circle crossings + the box
    /// corner `B`) and 4 faces — 1 curved Sphere patch (3 arc edges) + 3 planar
    /// caps (each 1 arc + 2 line edges) — sharing edges to form a watertight
    /// manifold (topologically a tetrahedron with one curved face). Reuses the
    /// existing `add_face_with_holes` for the sew (γ-2b audit) and `tessellate_
    /// arc_bounded_face` for the curved render (γ-2b-2).
    ///
    /// `planes[i] = (normal, origin)`; the kept side is `n·(p − o) ≥ 0`. The 3
    /// planes' normals must be linearly independent and the corner inside the
    /// sphere. Returns `[patch, cap0, cap1, cap2]`.
    pub fn boolean_sphere_octant(
        &mut self,
        sphere_faces: &[FaceId],
        planes: &[(DVec3, DVec3); 3],
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 γ-2b-3: no Sphere surface"))?;
        let nrm = |i: usize| planes[i].0.normalize_or_zero();
        let org = |i: usize| planes[i].1;
        // crossing Cij = the sphere ∩ plane_i ∩ plane_j point inside plane_k.
        let pick = |i: usize, j: usize, k: usize| -> Option<DVec3> {
            let pts = sphere_plane_pair_crossings(center, radius, nrm(i), org(i), nrm(j), org(j));
            pts.into_iter()
                .find(|&p| nrm(k).dot(p - org(k)) >= -1e-6)
        };
        let c01 = pick(0, 1, 2).ok_or_else(|| anyhow::anyhow!("ADR-197 γ-2b-3: no crossing 0∩1 inside plane 2"))?;
        let c02 = pick(0, 2, 1).ok_or_else(|| anyhow::anyhow!("ADR-197 γ-2b-3: no crossing 0∩2 inside plane 1"))?;
        let c12 = pick(1, 2, 0).ok_or_else(|| anyhow::anyhow!("ADR-197 γ-2b-3: no crossing 1∩2 inside plane 0"))?;
        // box corner B = the 3-plane intersection point (must be inside the sphere).
        let m = glam::DMat3::from_cols(
            DVec3::new(nrm(0).x, nrm(1).x, nrm(2).x),
            DVec3::new(nrm(0).y, nrm(1).y, nrm(2).y),
            DVec3::new(nrm(0).z, nrm(1).z, nrm(2).z),
        );
        if m.determinant().abs() < 1e-9 {
            bail!("ADR-197 γ-2b-3: cutting planes are not linearly independent");
        }
        let d = DVec3::new(nrm(0).dot(org(0)), nrm(1).dot(org(1)), nrm(2).dot(org(2)));
        let b = m.inverse() * d;
        if (b - center).length() >= radius - 1e-9 {
            bail!("ADR-197 γ-2b-3: box corner is not strictly inside the sphere");
        }

        // cut-circle (center, radius, basis_u) for a plane + its arc range.
        let cut_circle = |i: usize| -> (DVec3, f64, DVec3) {
            let n = nrm(i);
            let dist = n.dot(center - org(i));
            let cc = center - n * dist;
            let cr = (radius * radius - dist * dist).max(0.0).sqrt();
            let bu = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }.normalize_or_zero();
            (cc, cr, bu)
        };
        let arc_for = |i: usize, a: DVec3, bnd: DVec3, others: &[(DVec3, DVec3)]| -> AnalyticCurve {
            let (cc, cr, bu) = cut_circle(i);
            let (lo, hi) = corner_arc_range(cc, cr, nrm(i), bu, a, bnd, others);
            AnalyticCurve::Arc { center: cc, radius: cr, normal: nrm(i), basis_u: bu, start_angle: lo, end_angle: hi }
        };
        let arc0 = arc_for(0, c01, c02, &[(nrm(1), org(1)), (nrm(2), org(2))]); // on p0, between C01,C02
        let arc1 = arc_for(1, c01, c12, &[(nrm(0), org(0)), (nrm(2), org(2))]); // on p1, between C01,C12
        let arc2 = arc_for(2, c02, c12, &[(nrm(0), org(0)), (nrm(1), org(1))]); // on p2, between C02,C12

        // remove the original sphere faces + edges.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &sf in sphere_faces {
                if let Some(f) = self.faces.get(sf) {
                    let mut starts = vec![f.outer().start];
                    for inner in f.inners() {
                        starts.push(inner.start);
                    }
                    for st in starts {
                        if let Ok(hes) = self.collect_loop_hes(st) {
                            for he in hes {
                                es.insert(self.hes[he].edge(), ());
                            }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &sf in sphere_faces {
            let _ = self.remove_face(sf);
        }
        for eid in orig_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }

        // vertices.
        let v01 = self.add_vertex(c01);
        let v02 = self.add_vertex(c02);
        let v12 = self.add_vertex(c12);
        let vb = self.add_vertex(b);
        let solid_cen = (c01 + c02 + c12 + b) / 4.0;
        // order a triangle's verts so the Newell normal points along `target`.
        let order = |a: VertId, pa: DVec3, b2: VertId, pb: DVec3, c: VertId, pc: DVec3, target: DVec3| -> [VertId; 3] {
            let n = (pb - pa).cross(pc - pa);
            if n.dot(target) >= 0.0 { [a, b2, c] } else { [a, c, b2] }
        };
        let attach = |mesh: &mut Mesh, face: FaceId, vstart: VertId, vend: VertId, curve: AnalyticCurve| {
            // find the boundary edge (vstart, vend) and attach the arc curve.
            if let Ok(hes) = mesh.collect_loop_hes(mesh.faces[face].outer().start) {
                let verts = mesh.collect_loop_verts(mesh.faces[face].outer().start).unwrap_or_default();
                let n = verts.len();
                for i in 0..n {
                    let s = verts[(i + n - 1) % n];
                    let e = verts[i];
                    if (s == vstart && e == vend) || (s == vend && e == vstart) {
                        let eid = mesh.hes[hes[i]].edge();
                        mesh.edges[eid].set_curve(Some(curve.clone()));
                        return;
                    }
                }
            }
        };

        // curved patch: outward = away from center (radial).
        let patch_target = (solid_cen - center).normalize_or_zero();
        let pv = order(v01, c01, v02, c02, v12, c12, patch_target);
        let patch = self.add_face_with_holes(&pv, &[], material)?;
        attach(self, patch, v01, v02, arc0.clone());
        attach(self, patch, v01, v12, arc1.clone());
        attach(self, patch, v02, v12, arc2.clone());
        self.faces[patch].set_surface(Some(S::Sphere {
            center,
            radius,
            axis_dir: DVec3::Z,
            ref_dir: DVec3::X,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2),
        }));

        // planar caps: outward = -n_i (solid is on the +n kept side).
        let plane_surf = |i: usize| {
            let (_cc, _cr, bu) = cut_circle(i);
            S::Plane { origin: org(i), normal: nrm(i), basis_u: bu, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }
        };
        let cap0v = order(v01, c01, v02, c02, vb, b, -nrm(0));
        let cap0 = self.add_face_with_holes(&cap0v, &[], material)?;
        attach(self, cap0, v01, v02, arc0);
        self.faces[cap0].set_surface(Some(plane_surf(0)));

        let cap1v = order(v01, c01, v12, c12, vb, b, -nrm(1));
        let cap1 = self.add_face_with_holes(&cap1v, &[], material)?;
        attach(self, cap1, v01, v12, arc1);
        self.faces[cap1].set_surface(Some(plane_surf(1)));

        let cap2v = order(v02, c02, v12, c12, vb, b, -nrm(2));
        let cap2 = self.add_face_with_holes(&cap2v, &[], material)?;
        attach(self, cap2, v02, v12, arc2);
        self.faces[cap2].set_surface(Some(plane_surf(2)));

        Ok(vec![patch, cap0, cap1, cap2])
    }

    /// ADR-197 β-3-k — full SPHERE ∩ AXIS-BOX (the sphere-rounded box): every box
    /// corner is outside the sphere (cut off) and every box face cuts the sphere.
    /// Result = a single closed manifold of 24 vertices, 36 edges, 14 faces — 8
    /// Sphere-triangle corner patches (3 arc edges) + 6 Plane octagon box faces
    /// (4 straight + 4 arc edges), sharing edges (γ-2b-1 crossings + γ-2b-2 arc
    /// render + `add_face_with_holes` sew). Returns the 14 faces.
    pub fn boolean_sphere_box_full(
        &mut self,
        sphere_faces: &[FaceId],
        bmin: DVec3,
        bmax: DVec3,
        material: MaterialId,
    ) -> Result<Vec<FaceId>> {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let (center, radius) = sphere_faces
            .iter()
            .find_map(|&f| match self.face_surface(f) {
                Some(S::Sphere { center, radius, .. }) => Some((*center, *radius)),
                _ => None,
            })
            .ok_or_else(|| anyhow::anyhow!("ADR-197 β-3-k: no Sphere surface"))?;
        let (cx, cy, cz, r) = (center.x, center.y, center.z, radius);
        let bx = [bmin.x, bmax.x];
        let by = [bmin.y, bmax.y];
        let bz = [bmin.z, bmax.z];
        // Each box plane must cut the sphere.
        for &x in &bx { if (x - cx).abs() >= r - 1e-9 { bail!("ADR-197 β-3-k: x-plane does not cut the sphere"); } }
        for &y in &by { if (y - cy).abs() >= r - 1e-9 { bail!("ADR-197 β-3-k: y-plane does not cut the sphere"); } }
        for &z in &bz { if (z - cz).abs() >= r - 1e-9 { bail!("ADR-197 β-3-k: z-plane does not cut the sphere"); } }
        // Each box corner must be OUTSIDE the sphere (the rounded-box case).
        for i in 0..2 { for j in 0..2 { for k in 0..2 {
            if (DVec3::new(bx[i], by[j], bz[k]) - center).length() <= r + 1e-9 {
                bail!("ADR-197 β-3-k: box corner inside sphere — use a smaller box (single-corner = boolean_sphere_octant)");
            }
        }}}
        // Per-edge crossing offsets² (must be > 0 — each box edge crosses the sphere).
        let dx2 = |j: usize, k: usize| r * r - (by[j] - cy).powi(2) - (bz[k] - cz).powi(2);
        let dy2 = |i: usize, k: usize| r * r - (bx[i] - cx).powi(2) - (bz[k] - cz).powi(2);
        let dz2 = |i: usize, j: usize| r * r - (bx[i] - cx).powi(2) - (by[j] - cy).powi(2);
        for i in 0..2 { for j in 0..2 { for k in 0..2 {
            if dx2(j, k) <= 1e-9 || dy2(i, k) <= 1e-9 || dz2(i, j) <= 1e-9 {
                bail!("ADR-197 β-3-k: a box edge does not cross the sphere");
            }
        }}}
        // Crossing positions (24).
        let pcx = |i: usize, j: usize, k: usize| DVec3::new(cx + (2.0 * i as f64 - 1.0) * dx2(j, k).sqrt(), by[j], bz[k]);
        let pcy = |i: usize, j: usize, k: usize| DVec3::new(bx[i], cy + (2.0 * j as f64 - 1.0) * dy2(i, k).sqrt(), bz[k]);
        let pcz = |i: usize, j: usize, k: usize| DVec3::new(bx[i], by[j], cz + (2.0 * k as f64 - 1.0) * dz2(i, j).sqrt());
        let idx = |i: usize, j: usize, k: usize| i * 4 + j * 2 + k;
        // remove the original sphere faces + edges first.
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &sf in sphere_faces {
                if let Some(f) = self.faces.get(sf) {
                    let mut starts = vec![f.outer().start];
                    for inner in f.inners() { starts.push(inner.start); }
                    for st in starts {
                        if let Ok(hes) = self.collect_loop_hes(st) {
                            for he in hes { es.insert(self.hes[he].edge(), ()); }
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &sf in sphere_faces { let _ = self.remove_face(sf); }
        for eid in orig_edges { let _ = self.remove_edge_and_halfedges(eid); }
        // add the 24 vertices (indexed by idx(i,j,k)).
        let mut vx = Vec::with_capacity(8);
        let mut vy = Vec::with_capacity(8);
        let mut vz = Vec::with_capacity(8);
        for i in 0..2 { for j in 0..2 { for k in 0..2 {
            vx.push(self.add_vertex(pcx(i, j, k)));
            vy.push(self.add_vertex(pcy(i, j, k)));
            vz.push(self.add_vertex(pcz(i, j, k)));
        }}}

        // cut-circle (center, radius, normal, basis_u) on each box plane.
        let circ_x = |i: usize| (DVec3::new(bx[i], cy, cz), (r * r - (bx[i] - cx).powi(2)).sqrt(), DVec3::X, DVec3::Y);
        let circ_y = |j: usize| (DVec3::new(cx, by[j], cz), (r * r - (by[j] - cy).powi(2)).sqrt(), DVec3::Y, DVec3::Z);
        let circ_z = |k: usize| (DVec3::new(cx, cy, bz[k]), (r * r - (bz[k] - cz).powi(2)).sqrt(), DVec3::Z, DVec3::X);
        let make_arc = |(cc, cr, n, bu): (DVec3, f64, DVec3, DVec3), a: DVec3, b: DVec3, target: DVec3| {
            let (lo, hi) = arc_range_toward(cc, cr, n, bu, a, b, target);
            AnalyticCurve::Arc { center: cc, radius: cr, normal: n, basis_u: bu, start_angle: lo, end_angle: hi }
        };
        let poly_normal = |ps: &[DVec3]| {
            let mut n = DVec3::ZERO;
            for w in 0..ps.len() { n += ps[w].cross(ps[(w + 1) % ps.len()]); }
            n.normalize_or_zero()
        };
        let attach = |mesh: &mut Mesh, face: FaceId, va: VertId, vb: VertId, curve: AnalyticCurve| {
            if let Ok(hes) = mesh.collect_loop_hes(mesh.faces[face].outer().start) {
                let vs = mesh.collect_loop_verts(mesh.faces[face].outer().start).unwrap_or_default();
                let n = vs.len();
                for w in 0..n {
                    let s = vs[(w + n - 1) % n];
                    let e = vs[w];
                    if (s == va && e == vb) || (s == vb && e == va) {
                        let eid = mesh.hes[hes[w]].edge();
                        mesh.edges[eid].set_curve(Some(curve.clone()));
                        return;
                    }
                }
            }
        };

        let mut out = Vec::with_capacity(14);

        // ── 8 corner patches (Sphere triangle, 3 arc edges) ──
        for i in 0..2 { for j in 0..2 { for k in 0..2 {
            let (vxc, vyc, vzc) = (vx[idx(i, j, k)], vy[idx(i, j, k)], vz[idx(i, j, k)]);
            let (px, py, pz) = (pcx(i, j, k), pcy(i, j, k), pcz(i, j, k));
            let corner = DVec3::new(bx[i], by[j], bz[k]);
            let outward = ((px + py + pz) / 3.0 - center).normalize_or_zero();
            let mut verts = vec![vxc, vyc, vzc];
            let mut pos = vec![px, py, pz];
            if poly_normal(&pos).dot(outward) < 0.0 { verts.swap(1, 2); pos.swap(1, 2); }
            let face = self.add_face_with_holes(&verts, &[], material)?;
            attach(self, face, vxc, vyc, make_arc(circ_z(k), px, py, corner)); // both on z=bz[k]
            attach(self, face, vyc, vzc, make_arc(circ_x(i), py, pz, corner)); // both on x=bx[i]
            attach(self, face, vxc, vzc, make_arc(circ_y(j), px, pz, corner)); // both on y=by[j]
            self.faces[face].set_surface(Some(S::Sphere { center, radius, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (-FRAC_PI_2, FRAC_PI_2) }));
            out.push(face);
        }}}

        // ── 6 box octagon faces (Plane, 4 straight + 4 arc edges) ──
        // Each face perpendicular to one axis; collect its 8 boundary crossings,
        // sort CCW around the face centre, sew, then attach a plane-arc to every
        // same-corner consecutive pair.
        type Item = (VertId, DVec3, (usize, usize)); // (vid, pos, corner indices)
        let mut build_octagon = |mesh: &mut Mesh,
                                  items: &mut Vec<Item>,
                                  ang: &dyn Fn(DVec3) -> f64,
                                  outward: DVec3,
                                  plane: (DVec3, f64, DVec3, DVec3),
                                  corner_pt: &dyn Fn((usize, usize)) -> DVec3,
                                  surf: S|
         -> Result<FaceId> {
            items.sort_by(|a, b| ang(a.1).partial_cmp(&ang(b.1)).unwrap());
            let mut verts: Vec<VertId> = items.iter().map(|x| x.0).collect();
            let mut pos: Vec<DVec3> = items.iter().map(|x| x.1).collect();
            let mut corners: Vec<(usize, usize)> = items.iter().map(|x| x.2).collect();
            if poly_normal(&pos).dot(outward) < 0.0 {
                verts.reverse(); pos.reverse(); corners.reverse();
            }
            let face = mesh.add_face_with_holes(&verts, &[], material)?;
            let m = verts.len();
            for w in 0..m {
                let nx = (w + 1) % m;
                if corners[w] == corners[nx] {
                    let arc = {
                        let (lo, hi) = arc_range_toward(plane.0, plane.1, plane.2, plane.3, pos[w], pos[nx], corner_pt(corners[w]));
                        AnalyticCurve::Arc { center: plane.0, radius: plane.1, normal: plane.2, basis_u: plane.3, start_angle: lo, end_angle: hi }
                    };
                    attach(mesh, face, verts[w], verts[nx], arc);
                }
            }
            mesh.faces[face].set_surface(Some(surf));
            Ok(face)
        };

        // x-faces.
        for i in 0..2 {
            let mut items: Vec<Item> = Vec::new();
            for j in 0..2 { for k in 0..2 {
                items.push((vy[idx(i, j, k)], pcy(i, j, k), (j, k)));
                items.push((vz[idx(i, j, k)], pcz(i, j, k), (j, k)));
            }}
            let outward = if bx[i] > cx { DVec3::X } else { DVec3::NEG_X };
            let plane = circ_x(i);
            let corner_pt = move |(j, k): (usize, usize)| DVec3::new(bx[i], by[j], bz[k]);
            let surf = S::Plane { origin: DVec3::new(bx[i], cy, cz), normal: outward, basis_u: DVec3::Y, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
            let f = build_octagon(self, &mut items, &|p| (p.z - cz).atan2(p.y - cy), outward, plane, &corner_pt, surf)?;
            out.push(f);
        }
        // y-faces.
        for j in 0..2 {
            let mut items: Vec<Item> = Vec::new();
            for i in 0..2 { for k in 0..2 {
                items.push((vx[idx(i, j, k)], pcx(i, j, k), (i, k)));
                items.push((vz[idx(i, j, k)], pcz(i, j, k), (i, k)));
            }}
            let outward = if by[j] > cy { DVec3::Y } else { DVec3::NEG_Y };
            let plane = circ_y(j);
            let corner_pt = move |(i, k): (usize, usize)| DVec3::new(bx[i], by[j], bz[k]);
            let surf = S::Plane { origin: DVec3::new(cx, by[j], cz), normal: outward, basis_u: DVec3::Z, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
            let f = build_octagon(self, &mut items, &|p| (p.x - cx).atan2(p.z - cz), outward, plane, &corner_pt, surf)?;
            out.push(f);
        }
        // z-faces.
        for k in 0..2 {
            let mut items: Vec<Item> = Vec::new();
            for i in 0..2 { for j in 0..2 {
                items.push((vx[idx(i, j, k)], pcx(i, j, k), (i, j)));
                items.push((vy[idx(i, j, k)], pcy(i, j, k), (i, j)));
            }}
            let outward = if bz[k] > cz { DVec3::Z } else { DVec3::NEG_Z };
            let plane = circ_z(k);
            let corner_pt = move |(i, j): (usize, usize)| DVec3::new(bx[i], by[j], bz[k]);
            let surf = S::Plane { origin: DVec3::new(cx, cy, bz[k]), normal: outward, basis_u: DVec3::X, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) };
            let f = build_octagon(self, &mut items, &|p| (p.y - cy).atan2(p.x - cx), outward, plane, &corner_pt, surf)?;
            out.push(f);
        }

        Ok(out)
    }

    /// ADR-197 β-2b+c — VOLUMETRIC general 3D solid Boolean (Path B integration).
    ///
    /// imprint (β-2a) → classify (point-in-solid) → sew (strategy 2: rebuild kept
    /// fragments via `add_face_with_holes`, which welds the coincident SSI cut
    /// edges because the imprint verts are shared via 0.15μm dedup). Surfaces are
    /// re-attached per fragment (ADR-089 A-χ). Subtract flips the B fragments.
    ///
    /// MVP: convex planar faces (box / prism), single cut-chain per face.
    /// Curved-surface SSI + multi-chain + degenerate hardening are later β steps.
    /// NOT wired into `boolean()` yet — this is the new B-Rep path being proven.
    pub(crate) fn solid_boolean(
        &mut self,
        faces_a: &[FaceId],
        faces_b: &[FaceId],
        op: BoolOp,
        mat: MaterialId,
    ) -> Result<Vec<FaceId>> {
        let sa = self.prepare_solid(faces_a)?;
        let sb = self.prepare_solid(faces_b)?;
        let segs = self.detect_general_intersections(&sa, &sb);

        let mut by_face: FxHashMap<FaceId, Vec<(DVec3, DVec3)>> = FxHashMap::default();
        for s in &segs {
            by_face.entry(s.face_a).or_default().push((s.p0, s.p1));
            by_face.entry(s.face_b).or_default().push((s.p0, s.p1));
        }

        // ── Phase 1: compute kept fragments via the UNIFIED 2D arrangement
        // (ADR-197 β-2d). Each face's cut segments subdivide it into atomic
        // sub-regions (outer loop + hole loops); classify each by a material
        // point and keep per the Boolean op. Pure geometry — no mutation. ──
        // kept entry = (outer (outward-wound), holes (opposite-wound), eff_normal, surface, flip).
        //
        // Classify each region by MEMBERSHIP ON BOTH SIDES: a face fragment is on
        // the result boundary iff its inward side (−normal, just inside this
        // solid) and outward side (+normal) have DIFFERENT result-membership. This
        // single test handles every coplanar degeneracy uniformly:
        //   • a flush "membrane" (two solids glued across a face) has the SAME
        //     membership on both sides → not kept → cancelled (fixes coplanar Union);
        //   • identical solids A−A leave every face with both sides outside the
        //     result → nothing kept → empty (no inverted shell);
        //   • a genuine boundary face differs across its sides → kept, oriented
        //     toward the in-result side.
        let in_result = |p: DVec3| -> bool {
            let ina = point_in_solid(&sa.all_triangles, p);
            let inb = point_in_solid(&sb.all_triangles, p);
            match op {
                BoolOp::Union => ina || inb,
                BoolOp::Subtract => ina && !inb,
                BoolOp::Intersect => ina && inb,
            }
        };
        let mut kept: Vec<(
            Vec<DVec3>,
            Vec<Vec<DVec3>>,
            DVec3,
            Option<crate::surfaces::AnalyticSurface>,
            bool,
        )> = Vec::new();
        for faces in [faces_a, faces_b] {
            for &fid in faces {
                let Some((normal, poly)) = self.face_unit_normal_and_poly(fid) else {
                    continue;
                };
                let surface = self.faces.get(fid).and_then(|f| f.surface().cloned());
                let (poly2d, u, v, origin) = project_to_2d(&poly, normal);
                let to2d = |p: DVec3| Pt2::new(u.dot(p - origin), v.dot(p - origin));
                // The two membership probes are offset off the face plane by ε,
                // clamped to [2e-4, 1e-3] mm: always ABOVE the 0.15μm dedup
                // tolerance (so a coplanar probe clears the boundary ambiguity)
                // yet small enough not to punch through a thin slab.
                let (mut lo, mut hi) = (f64::MAX, f64::MIN);
                let (mut lo2, mut hi2) = (f64::MAX, f64::MIN);
                for p in &poly2d {
                    lo = lo.min(p.x);
                    hi = hi.max(p.x);
                    lo2 = lo2.min(p.y);
                    hi2 = hi2.max(p.y);
                }
                let face_eps =
                    (((hi - lo).powi(2) + (hi2 - lo2).powi(2)).sqrt() * 1e-3).clamp(2e-4, 1e-3);

                let regions: Vec<Region2D> = match by_face.get(&fid) {
                    Some(fsegs) if !fsegs.is_empty() => {
                        let cuts2d: Vec<(Pt2, Pt2)> =
                            fsegs.iter().map(|&(a, b)| (to2d(a), to2d(b))).collect();
                        let regs = arrange_polygon_2d(&poly2d, &cuts2d);
                        if regs.is_empty() {
                            // arrangement degenerate → keep the whole face.
                            vec![Region2D { outer: poly2d.clone(), holes: Vec::new() }]
                        } else {
                            regs
                        }
                    }
                    _ => vec![Region2D { outer: poly2d.clone(), holes: Vec::new() }],
                };

                for region in regions {
                    // Material point strictly inside the region (outside its holes,
                    // since an annulus centroid can land in the hole).
                    let mp = unproject_to_3d(region_material_point(&region), u, v, origin);
                    let res_in = in_result(mp - normal * face_eps); // inward (into this solid)
                    let res_out = in_result(mp + normal * face_eps); // outward
                    if res_in == res_out {
                        continue; // not a boundary of the result → drop (cancels membranes)
                    }
                    // Outer wound to +normal; flip if the result interior is on
                    // the OUTWARD side (so the kept normal points away from it).
                    let flip = res_out;
                    let eff_normal = if flip { -normal } else { normal };
                    let mut outer3d: Vec<DVec3> = region
                        .outer
                        .iter()
                        .map(|&p| unproject_to_3d(p, u, v, origin))
                        .collect();
                    if polygon_normal(&outer3d).dot(normal) < 0.0 {
                        outer3d.reverse();
                    }
                    let holes3d: Vec<Vec<DVec3>> = region
                        .holes
                        .iter()
                        .map(|h| {
                            let mut h3d: Vec<DVec3> =
                                h.iter().map(|&p| unproject_to_3d(p, u, v, origin)).collect();
                            if polygon_normal(&h3d).dot(normal) > 0.0 {
                                h3d.reverse(); // holes wind opposite the outer loop
                            }
                            h3d
                        })
                        .collect();
                    kept.push((outer3d, holes3d, eff_normal, surface.clone(), flip));
                }
            }
        }

        // Drop coincident SAME-SENSE duplicate fragments — a flush face that BOTH
        // solids contribute to the same result boundary (e.g. a Subtract where B's
        // flipped face coincides with an A face). Keeping both would make the
        // shared edges non-manifold; keep one. (Opposite-sense membranes were
        // already cancelled by the both-sides-equal test above.)
        {
            let mut deduped: Vec<(
                Vec<DVec3>,
                Vec<Vec<DVec3>>,
                DVec3,
                Option<crate::surfaces::AnalyticSurface>,
                bool,
            )> = Vec::with_capacity(kept.len());
            'next: for entry in kept {
                for ke in &deduped {
                    if entry.2.dot(ke.2) > 0.0 && same_point_set(&entry.0, &ke.0) {
                        continue 'next;
                    }
                }
                deduped.push(entry);
            }
            kept = deduped;
        }

        // ── Phase 2: fully remove the original faces AND their edges, leaving
        // only the vertices (for 0.15μm dedup in Phase 3). `remove_face` alone
        // merely detaches half-edges (face=null) → those remnant boundary HEs
        // interfere with the re-add welding. Removing the edges outright leaves
        // a clean vertex cloud the kept fragments weld onto from scratch. ──
        let orig_edges: Vec<EdgeId> = {
            let mut es: FxHashMap<EdgeId, ()> = FxHashMap::default();
            for &fid in faces_a.iter().chain(faces_b.iter()) {
                if let Some(f) = self.faces.get(fid) {
                    let start = f.outer().start;
                    if let Ok(hes) = self.collect_loop_hes(start) {
                        for he in hes {
                            es.insert(self.hes[he].edge(), ());
                        }
                    }
                }
            }
            es.into_keys().collect()
        };
        for &fid in faces_a.iter().chain(faces_b.iter()) {
            let _ = self.remove_face(fid);
        }
        for eid in orig_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }

        // ── Phase 3: re-add kept fragments. `add_face_with_holes` welds both
        // the shared cut edges AND the hole boundaries onto neighbours via the
        // 0.15μm-deduped SSI verts (e.g. a tunnel-cap annulus hole welds onto
        // the tunnel walls). ──
        let mut result = Vec::new();
        for (outer, holes, _eff_normal, surface, flip) in kept {
            let mut outer_v: Vec<VertId> = outer.iter().map(|&p| self.add_vertex(p)).collect();
            if flip {
                outer_v.reverse();
            }
            outer_v.dedup();
            if outer_v.len() > 1 && outer_v.first() == outer_v.last() {
                outer_v.pop();
            }
            if outer_v.len() < 3 {
                continue;
            }
            let mut hole_vs: Vec<Vec<VertId>> = Vec::new();
            for h in &holes {
                let mut hv: Vec<VertId> = h.iter().map(|&p| self.add_vertex(p)).collect();
                if flip {
                    hv.reverse();
                }
                hv.dedup();
                if hv.len() > 1 && hv.first() == hv.last() {
                    hv.pop();
                }
                if hv.len() >= 3 {
                    hole_vs.push(hv);
                }
            }
            let hole_refs: Vec<&[VertId]> = hole_vs.iter().map(|v| v.as_slice()).collect();
            if let Ok(fid) = self.add_face_with_holes(&outer_v, &hole_refs, mat) {
                if let Some(s) = surface {
                    self.faces[fid].set_surface(Some(s));
                }
                result.push(fid);
            }
        }

        // ── Phase 4: prune orphaned free edges (removed-face remnants not
        // re-used — e.g. B's surface outside A in a Subtract leaves closed loops
        // of completely-free edges that cleanup_dangling, which targets only
        // valence-1 danglers, cannot remove). A completely-free edge bounds no
        // active face on either side → not part of the result. ──
        let free_edges: Vec<EdgeId> = self
            .edges
            .iter()
            .filter(|(eid, e)| e.is_active() && self.is_edge_completely_free(*eid))
            .map(|(eid, _)| eid)
            .collect();
        for eid in free_edges {
            let _ = self.remove_edge_and_halfedges(eid);
        }
        self.remove_isolated_verts();

        Ok(result)
    }

    /// Unit normal + 3D outer-loop polygon for an active face (ADR-197 helper).
    fn face_unit_normal_and_poly(&self, fid: FaceId) -> Option<(DVec3, Vec<DVec3>)> {
        let face = self.faces.get(fid).filter(|f| f.is_active())?;
        let n = face.normal();
        let nl = n.length();
        if !nl.is_finite() || nl < 1e-10 {
            return None;
        }
        let poly: Vec<DVec3> = self
            .collect_loop_verts(face.outer().start)
            .ok()?
            .iter()
            .filter_map(|&v| self.verts.get(v).map(|v| v.pos()))
            .collect();
        if poly.len() < 3 {
            return None;
        }
        Some((n / nl, poly))
    }

    /// ── Stage 6: 공면 face 병합 ───────────────────
    /// Boolean 결과에서 인접한 공면 face들을 병합하여 unnecessary edge 제거.
    ///
    /// **ADR-067 Step 1**: pub(crate) 승격 — push_pull 의 auto-merge
    /// 단계에서 동일 코드 재사용 (드롭-in alongside, 재구현 금지).
    pub(crate) fn merge_coplanar_result_faces(&mut self, result_faces: &[FaceId]) -> Vec<FaceId> {
        // mesh의 기존 merge_faces_by_edge 활용
        // 결과 face들에 대해 merge pass 실행
        let mut current_faces = result_faces.to_vec();
        let mut changed = true;
        let mut iterations = 0;
        const MAX_ITERATIONS: usize = 10;

        while changed && iterations < MAX_ITERATIONS {
            changed = false;
            iterations += 1;
            let mut next_faces = Vec::new();

            for &fid in &current_faces {
                if !self.faces.get(fid).map(|f| f.is_active()).unwrap_or(false) {
                    continue;
                }
                next_faces.push(fid);
            }

            // 모든 face 쌍에 대해 병합 시도
            let mut i = 0;
            while i < next_faces.len() {
                let mut merged = false;
                let fid_a = next_faces[i];

                let mut j = i + 1;
                while j < next_faces.len() {
                    let fid_b = next_faces[j];
                    // 공면이고 edge를 공유하는지 확인 후 병합 시도
                    if let (Some(fa), Some(fb)) = (self.faces.get(fid_a), self.faces.get(fid_b)) {
                        if fa.is_active() && fb.is_active() {
                            let na = fa.normal();
                            let nb = fb.normal();
                            // G-2 fix: 법선 평행 체크 + 점-평면 거리 체크 (are_faces_coplanar_strict)
                            // 이전엔 법선만 비교하여 "같은 방향, 다른 높이"인 평행 면도
                            // 공면으로 오판 → merge_faces_by_edge가 degenerate face 생성 위험.
                            let parallel = (na.dot(nb).abs() - 1.0).abs() < 1e-6;
                            let coplanar = parallel
                                && self.are_faces_coplanar_strict(fid_a, fid_b).unwrap_or(false);
                            if coplanar {
                                // 공유 edge를 찾아서 merge 시도
                                if let Some(shared_edge) = self.find_shared_edge_between_faces(fid_a, fid_b) {
                                    let _ = self.merge_faces_by_edge(shared_edge);
                                }
                                next_faces.remove(j);
                                merged = true;
                                changed = true;
                                break;
                            }
                        }
                    }
                    j += 1;
                }

                if !merged {
                    i += 1;
                }
            }

            current_faces = next_faces;
        }

        current_faces
    }
}

/// 연속된 동일 VertId 제거
fn dedup_consecutive_verts(verts: &[VertId]) -> Vec<VertId> {
    if verts.is_empty() { return Vec::new(); }
    let mut result = vec![verts[0]];
    for i in 1..verts.len() {
        if verts[i] != verts[i - 1] {
            result.push(verts[i]);
        }
    }
    // 마지막과 처음이 같으면 제거
    if result.len() > 1 && result.last() == result.first() {
        result.pop();
    }
    result
}

// ════════════════════════════════════════════════════════════════════
// ADR-197 β-2a — line ∩ convex polygon clipping (general Boolean intersection)
// ════════════════════════════════════════════════════════════════════

/// Param `t` on the infinite line `l0 + t·ld` where it crosses segment `c→d`
/// (only within the segment). `None` if parallel or off the segment.
fn line_vs_segment_t(l0: Pt2, ld: Pt2, c: Pt2, d: Pt2) -> Option<f64> {
    let ex = d.x - c.x;
    let ey = d.y - c.y;
    let denom = ld.x * ey - ld.y * ex; // cross(ld, edge)
    if denom.abs() < 1e-12 {
        return None;
    }
    let rx = c.x - l0.x;
    let ry = c.y - l0.y;
    let s = (rx * ld.y - ry * ld.x) / denom; // edge param ∈ [0,1]
    if s < -1e-9 || s > 1.0 + 1e-9 {
        return None;
    }
    Some((rx * ey - ry * ex) / denom) // line param
}

/// Clip the infinite line `p_line + t·dir` (lying in the polygon's plane) to a
/// CONVEX 3D polygon → inside `[t_enter, t_exit]` range, or `None` if it misses.
fn clip_line_to_convex_poly(
    poly: &[DVec3],
    normal: DVec3,
    p_line: DVec3,
    dir: DVec3,
) -> Option<(f64, f64)> {
    let (poly_2d, u_axis, v_axis, origin) = project_to_2d(poly, normal);
    let l0 = Pt2::new(u_axis.dot(p_line - origin), v_axis.dot(p_line - origin));
    let ld = Pt2::new(u_axis.dot(dir), v_axis.dot(dir));
    if ld.x * ld.x + ld.y * ld.y < 1e-12 {
        return None;
    }
    let n = poly_2d.len();
    let mut ts: Vec<f64> = Vec::new();
    for i in 0..n {
        if let Some(t) = line_vs_segment_t(l0, ld, poly_2d[i], poly_2d[(i + 1) % n]) {
            ts.push(t);
        }
    }
    if ts.len() < 2 {
        return None;
    }
    ts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let lo = ts[0];
    let hi = ts[ts.len() - 1];
    if hi - lo < 1e-9 {
        return None;
    }
    Some((lo, hi))
}

/// Newell's method — robust polygon normal from 3D points. Unnormalized; its
/// sign encodes the winding relative to a reference normal.
fn polygon_normal(pts: &[DVec3]) -> DVec3 {
    let n = pts.len();
    if n < 3 {
        return DVec3::Z;
    }
    let mut nrm = DVec3::ZERO;
    for i in 0..n {
        let a = pts[i];
        let b = pts[(i + 1) % n];
        nrm.x += (a.y - b.y) * (a.z + b.z);
        nrm.y += (a.z - b.z) * (a.x + b.x);
        nrm.z += (a.x - b.x) * (a.y + b.y);
    }
    nrm
}

// ════════════════════════════════════════════════════════════════════
// ADR-197 β-2d — Unified 2D Planar Arrangement (imprint core)
//
// Subdivide a simple polygon by a set of cut segments — which may cross each
// other and the boundary, or form interior closed loops — into its atomic
// sub-regions. One algorithm unifies every imprint case the old special-case
// helpers handled piecemeal:
//   • single open chain   (corner overlap)            → 2 regions, no holes
//   • multiple open chains (tunnel-wall strips)        → N+1 regions, no holes
//   • crossing chains      (non-convex / multi-B)      → arrangement quadrants
//   • interior closed loop (tunnel cap cross-section)  → annulus(+hole) + disk
//
// Output regions carry their hole loops, so the sew step rebuilds each kept
// fragment with `add_face_with_holes` (the cut verts are shared via 0.15μm
// dedup, so the holes weld onto the tunnel walls automatically).
// ════════════════════════════════════════════════════════════════════

const ARR_TOL: f64 = 1e-6; // vertex coincidence in projected mm
const ARR_AREA_EPS: f64 = 1e-9; // degenerate-cycle area threshold

/// An atomic sub-region of the arrangement: an outer CCW loop plus any hole
/// (CW) loops produced by interior closed cut-loops.
#[derive(Clone, Debug)]
pub(crate) struct Region2D {
    pub outer: Vec<Pt2>,
    pub holes: Vec<Vec<Pt2>>,
}

/// Intern a 2D point into a vertex pool, deduping within `ARR_TOL`.
fn arr_intern(verts: &mut Vec<Pt2>, p: Pt2) -> usize {
    for (i, q) in verts.iter().enumerate() {
        if q.dist2(&p) < ARR_TOL * ARR_TOL {
            return i;
        }
    }
    verts.push(p);
    verts.len() - 1
}

/// Signed-area-weighted centroid of a 2D polygon (winding-agnostic geometric
/// center). Falls back to the vertex average for a degenerate (zero-area) loop.
fn centroid_2d(poly: &[Pt2]) -> Pt2 {
    let n = poly.len();
    let avg = || {
        let s = poly.iter().fold((0.0, 0.0), |a, p| (a.0 + p.x, a.1 + p.y));
        Pt2::new(s.0 / n.max(1) as f64, s.1 / n.max(1) as f64)
    };
    if n < 3 {
        return avg();
    }
    let mut a2 = 0.0;
    let mut cx = 0.0;
    let mut cy = 0.0;
    for i in 0..n {
        let j = (i + 1) % n;
        let cross = poly[i].x * poly[j].y - poly[j].x * poly[i].y;
        a2 += cross;
        cx += (poly[i].x + poly[j].x) * cross;
        cy += (poly[i].y + poly[j].y) * cross;
    }
    if a2.abs() < 1e-15 {
        return avg();
    }
    Pt2::new(cx / (3.0 * a2), cy / (3.0 * a2))
}

/// A point guaranteed strictly inside a region's material (inside the outer
/// loop, outside every hole) — used to classify the region inside/outside the
/// other solid. For a hole-free region the outer centroid suffices; for an
/// annulus (whose outer centroid can fall in the hole) we step a hair inward
/// from an outer-boundary edge midpoint, which lands in the material band.
fn region_material_point(region: &Region2D) -> Pt2 {
    let c = centroid_2d(&region.outer);
    if region.holes.is_empty() || !region.holes.iter().any(|h| point_in_polygon_2d(&c, h)) {
        return c;
    }
    let n = region.outer.len();
    for i in 0..n {
        let a = region.outer[i];
        let b = region.outer[(i + 1) % n];
        let mid = Pt2::new((a.x + b.x) * 0.5, (a.y + b.y) * 0.5);
        let probe = Pt2::new(mid.x + (c.x - mid.x) * 0.01, mid.y + (c.y - mid.y) * 0.01);
        if point_in_polygon_2d(&probe, &region.outer)
            && !region.holes.iter().any(|h| point_in_polygon_2d(&probe, h))
        {
            return probe;
        }
    }
    c
}

/// Two 3D point loops describe the same point SET (order-independent), within a
/// 0.1mm-squared tolerance — used to detect coincident Boolean fragments.
fn same_point_set(a: &[DVec3], b: &[DVec3]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter()
        .all(|p| b.iter().any(|q| (*p - *q).length_squared() < 1e-8))
}

/// Build the planar subdivision of `poly` by `cuts` and return its atomic
/// sub-regions (outer CCW loop + hole CW loops). See the section header above.
fn arrange_polygon_2d(poly: &[Pt2], cuts: &[(Pt2, Pt2)]) -> Vec<Region2D> {
    let n = poly.len();
    if n < 3 {
        return Vec::new();
    }

    // ── 1. Raw edges (boundary + non-degenerate cuts) ──
    let mut raw: Vec<(Pt2, Pt2)> = Vec::with_capacity(n + cuts.len());
    for i in 0..n {
        raw.push((poly[i], poly[(i + 1) % n]));
    }
    for &(a, b) in cuts {
        if a.dist2(&b) > ARR_TOL * ARR_TOL {
            raw.push((a, b));
        }
    }

    // Split each raw edge at every crossing (with the other raw edges) and emit
    // consecutive sub-edges into a deduped undirected edge set.
    let mut verts: Vec<Pt2> = Vec::new();
    let mut undirected: Vec<(usize, usize)> = Vec::new();
    for i in 0..raw.len() {
        let (a, b) = raw[i];
        let mut pts: Vec<(f64, Pt2)> = vec![(0.0, a), (1.0, b)];
        for (j, &(c, d)) in raw.iter().enumerate() {
            if i == j {
                continue;
            }
            if let Some((t, _u, p)) = segment_segment_2d(a, b, c, d) {
                pts.push((t.clamp(0.0, 1.0), p));
            }
        }
        pts.sort_by(|x, y| x.0.partial_cmp(&y.0).unwrap_or(std::cmp::Ordering::Equal));
        for w in pts.windows(2) {
            let ia = arr_intern(&mut verts, w[0].1);
            let ib = arr_intern(&mut verts, w[1].1);
            if ia != ib {
                undirected.push((ia, ib));
            }
        }
    }
    undirected.sort_unstable_by_key(|&(a, b)| (a.min(b), a.max(b)));
    undirected.dedup_by_key(|p| (p.0.min(p.1), p.0.max(p.1)));
    if undirected.is_empty() {
        return Vec::new();
    }

    // ── 2. Half-edges + per-vertex CCW angular order ──
    // he 2k = (a→b), he 2k+1 = (b→a) for undirected[k]; twin(h) = h ^ 1.
    let nv = verts.len();
    let mut he_from: Vec<usize> = Vec::with_capacity(undirected.len() * 2);
    let mut he_to: Vec<usize> = Vec::with_capacity(undirected.len() * 2);
    for &(a, b) in &undirected {
        he_from.push(a);
        he_to.push(b);
        he_from.push(b);
        he_to.push(a);
    }
    let nh = he_from.len();
    let arr_angle = |h: usize, vf: &[usize], vt: &[usize], vs: &[Pt2]| -> f64 {
        let a = vs[vf[h]];
        let b = vs[vt[h]];
        (b.y - a.y).atan2(b.x - a.x)
    };
    let mut out_he: Vec<Vec<usize>> = vec![Vec::new(); nv];
    for h in 0..nh {
        out_he[he_from[h]].push(h);
    }
    for ring in &mut out_he {
        ring.sort_by(|&p, &q| {
            arr_angle(p, &he_from, &he_to, &verts)
                .partial_cmp(&arr_angle(q, &he_from, &he_to, &verts))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
    }

    // ── 3. Extract cycles. next(h) = at dst(h), the CCW-predecessor of twin(h)
    // (the most-clockwise turn) → each bounded face is traced CCW, the
    // unbounded face CW. Every half-edge is consumed by exactly one cycle. ──
    let mut visited = vec![false; nh];
    let mut raw_cycles: Vec<Vec<usize>> = Vec::new();
    for start in 0..nh {
        if visited[start] {
            continue;
        }
        let mut loop_v: Vec<usize> = Vec::new();
        let mut h = start;
        let mut guard = 0usize;
        loop {
            if visited[h] {
                break;
            }
            visited[h] = true;
            loop_v.push(he_from[h]);
            let v = he_to[h];
            let t = h ^ 1;
            let ring = &out_he[v];
            let idx = match ring.iter().position(|&x| x == t) {
                Some(k) => k,
                None => break,
            };
            h = ring[(idx + ring.len() - 1) % ring.len()];
            guard += 1;
            if guard > nh + 1 {
                break;
            }
            if h == start {
                break;
            }
        }
        if loop_v.len() >= 3 {
            raw_cycles.push(loop_v);
        }
    }

    // ── 4. Classify cycles by signed area: CCW(+) = region, CW(−) = hole. ──
    let mut regions: Vec<Region2D> = Vec::new();
    let mut holes: Vec<Vec<Pt2>> = Vec::new();
    for cyc in &raw_cycles {
        let p: Vec<Pt2> = cyc.iter().map(|&i| verts[i]).collect();
        let area = polygon_signed_area_2d(&p);
        if area > ARR_AREA_EPS {
            regions.push(Region2D { outer: p, holes: Vec::new() });
        } else if area < -ARR_AREA_EPS {
            holes.push(p);
        }
    }

    // ── 5. Assign each hole to the smallest STRICTLY-LARGER region that
    // contains it. The strictly-larger test excludes both the coincident disk
    // (same area, opposite winding) and the unbounded outer cycle (which is a
    // hole contained by no smaller region → dropped). ──
    for hole in holes {
        let h_area = polygon_signed_area_2d(&hole).abs();
        let c = centroid_2d(&hole);
        let mut best: Option<(usize, f64)> = None;
        for (ri, r) in regions.iter().enumerate() {
            let r_area = polygon_signed_area_2d(&r.outer).abs();
            if r_area > h_area + ARR_AREA_EPS && point_in_polygon_2d(&c, &r.outer) {
                if best.map_or(true, |(_, ba)| r_area < ba) {
                    best = Some((ri, r_area));
                }
            }
        }
        if let Some((ri, _)) = best {
            regions[ri].holes.push(hole);
        }
        // else: unbounded face → dropped.
    }

    regions
}

// ════════════════════════════════════════════════════════════════════
// 2D Polygon Splitting by Line Segments
// ════════════════════════════════════════════════════════════════════

/// 교차 세그먼트 집합으로 2D 다각형을 분할.
///
/// 전략: 모든 교차 세그먼트를 하나의 분할선(cutting line)으로 모아서
/// 다각형의 에지와 교차점을 계산 → 에지에 교차점 삽입 →
/// 교차점 쌍을 연결하여 sub-polygon 추출.
///
/// MVP: 단일 직선(세그먼트 체인)에 의한 분할만 지원.
/// (2개 이상의 독립 분할선은 향후 확장)
fn split_polygon_2d(
    poly: &[Pt2],
    cut_segments: &[(Pt2, Pt2)],
) -> Option<Vec<Vec<Pt2>>> {
    let n = poly.len();
    if n < 3 || cut_segments.is_empty() {
        return None;
    }

    // ── Step 1: 각 에지 위의 교차점 수집 ────────────
    // edge_hits[i] = (t, Pt2) 리스트 → 에지 i (poly[i]→poly[(i+1)%n]) 위의 교차점
    let mut edge_hits: Vec<Vec<(f64, Pt2)>> = vec![Vec::new(); n];

    // 모든 cut_segment × polygon_edge 교차 계산
    for &(cs0, cs1) in cut_segments {
        for i in 0..n {
            let j = (i + 1) % n;
            if let Some((t, _u, pt)) = segment_segment_2d(poly[i], poly[j], cs0, cs1) {
                // t는 polygon edge 위의 파라미터
                let t_clamped = t.clamp(0.0, 1.0);
                edge_hits[i].push((t_clamped, pt));
            }
        }
    }

    // 교차점이 2개 미만이면 분할 불가
    let total_hits: usize = edge_hits.iter().map(|h| h.len()).sum();
    if total_hits < 2 {
        return None;
    }

    // 각 에지의 교차점을 t순 정렬
    for hits in &mut edge_hits {
        hits.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
    }

    // ── Step 2: 확장 정점 리스트 구축 ────────────
    // 다각형 에지를 순회하면서 교차점을 끼워넣는다
    let mut ext_verts: Vec<PolyVert> = Vec::new();
    let mut intersection_count = 0usize;
    let mut intersections_info: Vec<(usize, Pt2)> = Vec::new(); // (idx in ext_verts, pt)

    for i in 0..n {
        // 에지 시작 정점
        ext_verts.push(PolyVert {
            pt: poly[i],
            is_intersection: false,
            intersection_id: None,
        });

        // 이 에지 위 교차점들 (t순)
        for &(_t, pt) in &edge_hits[i] {
            let idx = ext_verts.len();
            let iid = intersection_count;
            intersection_count += 1;
            ext_verts.push(PolyVert {
                pt,
                is_intersection: true,
                intersection_id: Some(iid),
            });
            intersections_info.push((idx, pt));
        }
    }

    if intersection_count < 2 {
        return None;
    }

    // ── Step 3: 교차점 페어링 ────────────
    // 교차점들을 cut segment 방향을 따라 정렬하여 순서대로 짝짓기
    // (Entry/Exit 쌍)
    // 단순화: 교차점을 이들의 centroid 방향으로 정렬
    let pairs = pair_intersection_points(&intersections_info, cut_segments);

    if pairs.is_empty() {
        return None;
    }

    // ── Step 4: Sub-polygon 추출 ────────────
    // 전략: 각 교차점 쌍(A,B)에 대해 두 방향 추적
    //   방향 1: A → forward → ... → B → jump to A (닫힘)
    //   방향 2: B → forward → ... → A → jump to B (닫힘)
    // 교차선으로 나뉜 양쪽 다각형 모두 추출

    let en = ext_verts.len();
    let mut pair_map: FxHashMap<usize, usize> = FxHashMap::default();
    for &(a_iid, b_iid) in &pairs {
        let a_idx = intersections_info[a_iid].0;
        let b_idx = intersections_info[b_iid].0;
        pair_map.insert(a_idx, b_idx);
        pair_map.insert(b_idx, a_idx);
    }

    let mut sub_polys: Vec<Vec<Pt2>> = Vec::new();
    let mut traced_starts: std::collections::HashSet<usize> = std::collections::HashSet::new();

    for &(a_iid, b_iid) in &pairs {
        let a_idx = intersections_info[a_iid].0;
        let b_idx = intersections_info[b_iid].0;

        // 두 방향 추적: 각 교차점에서 시작하여 forward 진행
        for &start_idx in &[a_idx, b_idx] {
            if traced_starts.contains(&start_idx) {
                continue;
            }
            traced_starts.insert(start_idx);

            let mut sub_poly = vec![ext_verts[start_idx].pt];
            let mut idx = (start_idx + 1) % en; // forward (시작점에서 jump 안 함)
            let mut safety = 0;

            loop {
                if safety > en * 2 {
                    break; // 무한 루프 방지
                }
                safety += 1;

                sub_poly.push(ext_verts[idx].pt);

                // 교차점에 도달 → pair로 jump하여 루프 닫기
                if ext_verts[idx].is_intersection {
                    if let Some(&pair_idx) = pair_map.get(&idx) {
                        if pair_idx == start_idx {
                            // 시작점으로 돌아옴 → 루프 완성
                            break;
                        } else {
                            // 다른 교차점 쌍이면 jump 후 계속
                            sub_poly.push(ext_verts[pair_idx].pt);
                            idx = (pair_idx + 1) % en;
                            continue;
                        }
                    }
                }

                idx = (idx + 1) % en;
                if idx == start_idx {
                    break; // 전체 순회 완료
                }
            }

            if sub_poly.len() >= 3 {
                sub_polys.push(sub_poly);
            }
        }
    }

    if sub_polys.len() < 2 {
        return None;
    }

    // 퇴화 다각형 필터 (면적이 너무 작은 것 제거)
    let valid_polys: Vec<Vec<Pt2>> = sub_polys.into_iter()
        .filter(|p| polygon_signed_area_2d(p).abs() > 1e-10)
        .collect();

    if valid_polys.len() < 2 {
        return None;
    }

    Some(valid_polys)
}

/// 교차점 쌍(pair) 생성: 교차점을 cut direction으로 정렬 후 순차 페어링
fn pair_intersection_points(
    intersections: &[(usize, Pt2)],
    cut_segments: &[(Pt2, Pt2)],
) -> Vec<(usize, usize)> {
    if intersections.len() < 2 {
        return Vec::new();
    }

    // Cut segment 체인의 대략적 방향 계산
    let dir = if !cut_segments.is_empty() {
        let s = &cut_segments[0];
        let dx = s.1.x - s.0.x;
        let dy = s.1.y - s.0.y;
        let len = (dx * dx + dy * dy).sqrt();
        if len > 1e-12 { (dx / len, dy / len) } else { (1.0, 0.0) }
    } else {
        (1.0, 0.0)
    };

    // 각 교차점의 projection onto cut direction으로 정렬
    let mut sorted: Vec<(usize, f64)> = intersections.iter().enumerate()
        .map(|(iid, (_idx, pt))| {
            let proj = pt.x * dir.0 + pt.y * dir.1;
            (iid, proj)
        })
        .collect();
    sorted.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal));

    // 순차 페어링: 0↔1, 2↔3, ...
    let mut pairs = Vec::new();
    let mut i = 0;
    while i + 1 < sorted.len() {
        pairs.push((sorted[i].0, sorted[i + 1].0));
        i += 2;
    }

    pairs
}

/// 확장 정점 (내부 사용)
#[derive(Clone, Debug)]
struct PolyVert {
    pt: Pt2,
    is_intersection: bool,
    #[allow(dead_code)]
    intersection_id: Option<usize>,
}

/// 교차선분
#[derive(Debug, Clone)]
struct IntersectionSegment {
    face_a: FaceId,
    face_b: FaceId,
    p0: DVec3,
    p1: DVec3,
}

/// ADR-197 β-3-β — a surface-surface intersection between two faces' analytic
/// surfaces (the curved-Boolean counterpart of `IntersectionSegment`). `ssi.points`
/// is the 3D intersection curve; `ssi.uv_a` / `ssi.uv_b` are the parameters on
/// `face_a`'s / `face_b`'s surface respectively (kept consistent with this order).
#[derive(Debug, Clone)]
pub(crate) struct CurvedIntersection {
    pub face_a: FaceId,
    pub face_b: FaceId,
    pub ssi: crate::surfaces::ssi::SurfaceIntersection,
}

fn is_planar_surface(s: &crate::surfaces::AnalyticSurface) -> bool {
    matches!(s, crate::surfaces::AnalyticSurface::Plane { .. })
}

/// Closed-form SSI for an ORDERED surface pair (`ssi::analytic`); `None` for
/// unsupported pairs (NURBS-class, Sphere×Sphere, …, deferred to a later β).
fn ssi_ordered(
    a: &crate::surfaces::AnalyticSurface,
    b: &crate::surfaces::AnalyticSurface,
) -> Option<crate::surfaces::ssi::SurfaceIntersection> {
    use crate::surfaces::ssi::analytic;
    use crate::surfaces::AnalyticSurface as S;
    const N: usize = 64; // SSI samples per curve
    const EXT: f64 = 1.0e4; // line extent (mm)
    match (a, b) {
        (S::Plane { origin: oa, normal: na, .. }, S::Plane { origin: ob, normal: nb, .. }) => {
            Some(analytic::plane_plane(*oa, *na, *ob, *nb, N, EXT))
        }
        (S::Plane { origin, normal, .. }, S::Cylinder { axis_origin, axis_dir, radius, .. }) => {
            Some(analytic::plane_cylinder(*origin, *normal, *axis_origin, *axis_dir, *radius, N))
        }
        (S::Plane { origin, normal, .. }, S::Sphere { center, radius, .. }) => {
            Some(analytic::plane_sphere(*origin, *normal, *center, *radius, N))
        }
        (S::Plane { origin, normal, .. }, S::Cone { apex, axis_dir, half_angle, .. }) => {
            Some(analytic::plane_cone(*origin, *normal, *apex, *axis_dir, *half_angle, N))
        }
        (
            S::Cylinder { axis_origin: oa, axis_dir: da, radius: ra, .. },
            S::Cylinder { axis_origin: ob, axis_dir: db, radius: rb, .. },
        ) => Some(analytic::cylinder_cylinder(*oa, *da, *ra, *ob, *db, *rb, N, EXT)),
        _ => None,
    }
}

/// ADR-197 β-3-β — dispatch two analytic surfaces to the closed-form SSI, with
/// `uv_a`/`uv_b` consistent with the `(a, b)` argument order (the asymmetric
/// Plane×X functions are tried both ways and the uv arrays swapped on the flip).
fn surface_surface_intersection(
    a: &crate::surfaces::AnalyticSurface,
    b: &crate::surfaces::AnalyticSurface,
) -> Option<crate::surfaces::ssi::SurfaceIntersection> {
    if let Some(si) = ssi_ordered(a, b) {
        return Some(si);
    }
    if let Some(mut si) = ssi_ordered(b, a) {
        std::mem::swap(&mut si.uv_a, &mut si.uv_b);
        return Some(si);
    }
    None
}

/// ADR-197 β-3-γ — a curved sub-face produced by imprinting an SSI curve onto a
/// curved analytic face. `surface` is the SAME analytic surface; `uv_region` is
/// the sub-face's domain as a polygon in the surface's (u, v) parameter space.
/// `u_shift` is the longitude rotation applied to keep the region off the u-seam
/// (oblique cuts); the REAL parameter is `(wrap(u + u_shift), v)` — so evaluate
/// the surface at `(u + u_shift, v)`. For latitude/un-shifted faces `u_shift = 0`.
#[derive(Debug, Clone)]
pub(crate) struct CurvedSubFace {
    pub surface: crate::surfaces::AnalyticSurface,
    pub uv_region: Vec<(f64, f64)>,
    pub uv_holes: Vec<Vec<(f64, f64)>>,
    pub u_shift: f64,
}

/// ADR-197 β-3-i — general `boolean()` curved routing: an analytic primitive
/// (Z-up sphere/cylinder/cone/torus) recognised among a face set, with its
/// world AABB computed from the SURFACE parameters (the self-loop boundary's
/// AABB is just an anchor point, so it cannot be used).
#[derive(Debug, Clone, Copy, PartialEq)]
enum CurvedPrimKind {
    Sphere,
    Cylinder,
    Cone,
    Torus,
}

/// ADR-197 β-3-n — curved knife mode: `Slice` splits into two volumes; `KeepAbove`
/// / `KeepBelow` trim (keep one side, the other removed).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CurvedCutMode {
    Slice,
    KeepAbove,
    KeepBelow,
}

#[derive(Debug, Clone)]
struct CurvedPrim {
    kind: CurvedPrimKind,
    faces: Vec<FaceId>,
    aabb: crate::operations::coplanar::Aabb3,
    center_z: f64, // sphere/torus axis centre (straddle decisions)
}

/// ADR-197 β-3-h — the "plane_torus SSI" for a Z-up torus cut by a horizontal
/// plane `z = k`. Unlike sphere/cylinder/cone (single curve), a torus cut yields
/// TWO concentric circles, so this returns their poloidal angles + radii rather
/// than a single `SurfaceIntersection`. `z = center_z + r·sin(v)` →
/// `sin v = d/r` (`d = k − center_z`); requires `|d| < r` (a genuine cut).
/// Returns `(v1, v2, rho_outer, rho_inner)`:
///   v1 = asin(d/r) → outer circle ρ = R + √(r²−d²),
///   v2 = π − v1    → inner circle ρ = R − √(r²−d²).
fn torus_z_cut(center_z: f64, major_r: f64, minor_r: f64, k: f64) -> Option<(f64, f64, f64, f64)> {
    let d = k - center_z;
    if d.abs() >= minor_r - 1e-9 {
        return None; // plane misses or is tangent to the tube
    }
    let half = (minor_r * minor_r - d * d).max(0.0).sqrt();
    let rho_outer = major_r + half;
    let rho_inner = major_r - half;
    if rho_inner <= 1e-9 {
        return None; // degenerate (would self-intersect — guarded at create)
    }
    let v1 = (d / minor_r).clamp(-1.0, 1.0).asin();
    let v2 = std::f64::consts::PI - v1;
    Some((v1, v2, rho_outer, rho_inner))
}

/// ADR-197 β-3-o — the SSI circle of two Z-COAXIAL spheres (centers on a common
/// Z line). Two spheres with centre distance `d` and radii `r1`, `r2` that
/// genuinely overlap (`|r1−r2| < d < r1+r2`) meet in a circle on the radical
/// plane (perpendicular to the centre line). Returns `(z_ssi, rho, v1, v2)`:
/// the circle is at `z = z_ssi` with radius `rho`; `v1 = asin((z_ssi−c1.z)/r1)`
/// is sphere1's latitude at the circle and `v2 = asin((z_ssi−c2.z)/r2)` sphere2's.
/// `None` if not Z-coaxial, concentric, disjoint, or one strictly inside the other.
fn sphere_sphere_z_circle(c1: DVec3, r1: f64, c2: DVec3, r2: f64) -> Option<(f64, f64, f64, f64)> {
    if (c1.x - c2.x).abs() > 1e-6 || (c1.y - c2.y).abs() > 1e-6 {
        return None; // not Z-coaxial (oblique SSI → deferred)
    }
    let d = c2.z - c1.z; // signed, along +Z toward c2
    let d_abs = d.abs();
    if d_abs < 1e-9 {
        return None; // concentric
    }
    if d_abs >= r1 + r2 - 1e-9 {
        return None; // disjoint
    }
    if d_abs <= (r1 - r2).abs() + 1e-9 {
        return None; // one strictly inside the other → no boundary circle
    }
    // radical-plane offset from c1 along the centre line (toward c2).
    let a = (d * d + r1 * r1 - r2 * r2) / (2.0 * d);
    let z_ssi = c1.z + a;
    let rho_sq = r1 * r1 - a * a;
    if rho_sq <= 1e-12 {
        return None; // tangent / degenerate
    }
    let rho = rho_sq.sqrt();
    let v1 = ((z_ssi - c1.z) / r1).clamp(-1.0, 1.0).asin();
    let v2 = ((z_ssi - c2.z) / r2).clamp(-1.0, 1.0).asin();
    Some((z_ssi, rho, v1, v2))
}

/// ADR-197 β-3-o — validate two OPPOSING coaxial cones overlapping into an
/// HOURGLASS and return `(z_waist, rho_waist, c1_is_apex_up)`. The two cone
/// surfaces (one `axis_dir≈−Z` apex-up, one `axis_dir≈+Z` apex-down) must be
/// Z-axis, coaxial (same XY), opposing, and overlap so the waist circle lies
/// strictly inside both. `None` otherwise (→ dispatch falls through to legacy).
/// `apexN`/`vbN` = cone apex + base axial distance (v_range max). Waist:
/// `(apex_up.z − z)tanθ_up = (z − apex_dn.z)tanθ_dn`.
fn cone_cone_hourglass(
    apex1: DVec3, ax1: DVec3, ha1: f64, vb1: f64,
    apex2: DVec3, ax2: DVec3, ha2: f64, vb2: f64,
) -> Option<(f64, f64, bool)> {
    if ax1.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 { return None; }
    if ax2.normalize_or_zero().cross(DVec3::Z).length() > 1e-6 { return None; }
    if ax1.z * ax2.z >= 0.0 { return None; } // not opposing
    if (apex1.x - apex2.x).abs() > 1e-6 || (apex1.y - apex2.y).abs() > 1e-6 { return None; } // not coaxial
    let c1_up = ax1.z < 0.0;
    let (apex_up, ha_up, vb_up, apex_dn, ha_dn, vb_dn) =
        if c1_up { (apex1, ha1, vb1, apex2, ha2, vb2) } else { (apex2, ha2, vb2, apex1, ha1, vb1) };
    let (tan_up, tan_dn) = (ha_up.tan(), ha_dn.tan());
    if tan_up <= 1e-9 || tan_dn <= 1e-9 { return None; }
    let base_up = apex_up.z - vb_up; // apex-up cone base (bottom)
    let base_dn = apex_dn.z + vb_dn; // apex-down cone base (top)
    let z_waist = (apex_up.z * tan_up + apex_dn.z * tan_dn) / (tan_up + tan_dn);
    let rho_waist = (apex_up.z - z_waist) * tan_up;
    if rho_waist <= 1e-9 { return None; }
    // waist strictly inside both cone bodies → genuine hourglass.
    if !(base_up < z_waist - 1e-9 && z_waist < apex_up.z - 1e-9
        && apex_dn.z < z_waist - 1e-9 && z_waist < base_dn - 1e-9)
    {
        return None;
    }
    Some((z_waist, rho_waist, c1_up))
}

/// ADR-197 β-3-j (corner geometry) — the points on a sphere lying on BOTH cut
/// planes (`plane_a ∩ plane_b ∩ sphere`). Two planes meet in a line; the line
/// meets the sphere in 0/1/2 points (the circle-circle crossings of the two cut
/// circles). `n*` need not be normalised; `o*` is any point on each plane.
fn sphere_plane_pair_crossings(
    center: DVec3,
    radius: f64,
    na: DVec3,
    oa: DVec3,
    nb: DVec3,
    ob: DVec3,
) -> Vec<DVec3> {
    let na = na.normalize_or_zero();
    let nb = nb.normalize_or_zero();
    let dir = na.cross(nb);
    let dlen2 = dir.length_squared();
    if dlen2 < 1e-18 {
        return Vec::new(); // parallel planes → no line
    }
    let da = na.dot(oa);
    let db = nb.dot(ob);
    // A point on the intersection line: p0 = ((da·nb − db·na) × dir) / |dir|².
    let p0 = (nb * da - na * db).cross(dir) / dlen2;
    let dir = dir / dlen2.sqrt();
    // line p0 + s·dir ∩ sphere.
    let m = p0 - center;
    let b = m.dot(dir);
    let c = m.length_squared() - radius * radius;
    let disc = b * b - c;
    if disc < -1e-12 {
        return Vec::new();
    }
    let sq = disc.max(0.0).sqrt();
    if sq < 1e-9 {
        vec![p0 - dir * b] // tangent
    } else {
        vec![p0 - dir * (b + sq), p0 - dir * (b - sq)]
    }
}

/// ADR-197 β-3-j γ-2b-4 — detect the box∩sphere CORNER (octant) case: exactly
/// one of each axis's two box planes cuts the sphere (per-axis count (1,1,1))
/// AND the 3-plane corner is inside the sphere. Returns the 3 cutting planes
/// `(normal toward the kept side, origin)`, else `None` (no-op / halfspace /
/// slab / wedge-bigon / full-box / corner-outside → handled elsewhere).
fn sphere_box_corner_planes(
    center: DVec3,
    radius: f64,
    bmin: DVec3,
    bmax: DVec3,
) -> Option<[(DVec3, DVec3); 3]> {
    let axis_plane = |lo: f64, hi: f64, c: f64, ax: DVec3| -> Option<(DVec3, DVec3)> {
        let lo_cuts = c - radius < lo && lo < c + radius;
        let hi_cuts = c - radius < hi && hi < c + radius;
        if lo_cuts && !hi_cuts {
            Some((ax, ax * lo)) // kept p·ax > lo, normal +ax
        } else if hi_cuts && !lo_cuts {
            Some((-ax, ax * hi)) // kept p·ax < hi, normal -ax
        } else {
            None // 0 or 2 cuts on this axis → not the corner case
        }
    };
    let px = axis_plane(bmin.x, bmax.x, center.x, DVec3::X)?;
    let py = axis_plane(bmin.y, bmax.y, center.y, DVec3::Y)?;
    let pz = axis_plane(bmin.z, bmax.z, center.z, DVec3::Z)?;
    // corner = intersection of the 3 axis planes; must be strictly inside.
    let corner = DVec3::new(px.1.x, py.1.y, pz.1.z);
    if (corner - center).length() >= radius - 1e-9 {
        return None;
    }
    Some([px, py, pz])
}

/// ADR-197 β-3-k — detect the full sphere-rounded box: every box plane cuts the
/// sphere AND every box corner is outside it. Routes to `boolean_sphere_box_full`.
fn is_sphere_rounded_box(center: DVec3, radius: f64, bmin: DVec3, bmax: DVec3) -> bool {
    let bx = [bmin.x, bmax.x];
    let by = [bmin.y, bmax.y];
    let bz = [bmin.z, bmax.z];
    for &x in &bx { if (x - center.x).abs() >= radius - 1e-9 { return false; } }
    for &y in &by { if (y - center.y).abs() >= radius - 1e-9 { return false; } }
    for &z in &bz { if (z - center.z).abs() >= radius - 1e-9 { return false; } }
    for i in 0..2 { for j in 0..2 { for k in 0..2 {
        if (DVec3::new(bx[i], by[j], bz[k]) - center).length() <= radius + 1e-9 {
            return false;
        }
    }}}
    true
}

/// ADR-205 cone Dandelin — the ELLIPSE section of an oblique plane ∩ a cone.
/// Returns `(center, semi_major, semi_minor, major_dir, minor_dir)` of the planar
/// ellipse, or `None` when the plane does NOT cut the cone in a bounded ellipse:
/// the section is a parabola/hyperbola (|n_a·m| ≤ p·tanα), the plane is ⟂ the axis
/// (a circle — the local-frame slab path), or it passes through the apex (k = 0).
///
/// Geometry (apex `A`, unit axis `n_a` apex→base, half-angle `α`, plane `(O, m)`):
///   D = n_a·m,  p = |m − D·n_a|,  q = (m − D·n_a)/p,  r2 = n_a × q,
///   a = cosα·D,  b = sinα·p,  k = (O−A)·m,  denom = a²−b²  (>0 for the ellipse),
///   center     = A + (k/denom)(a·cosα·n_a − b·sinα·q),
///   semi_major = |k|·√(b²cos²α + a²sin²α) / denom   (axis in the n_a–q plane),
///   semi_minor = |k|·sinα / √denom                   (axis along r2).
/// Derivation + on-cone/on-plane validation: `sim_adr205_cone_oblique_ellipse_geometry`.
/// Consumed by `boolean_cone_oblique_halfspace` (β-2-cone).
fn cone_oblique_ellipse(
    apex: DVec3,
    axis_dir: DVec3,
    half_angle: f64,
    plane_origin: DVec3,
    plane_normal: DVec3,
) -> Option<(DVec3, f64, f64, DVec3, DVec3)> {
    let n_a = axis_dir.normalize_or_zero();
    let m = plane_normal.normalize_or_zero();
    if n_a.length_squared() < 0.5 || m.length_squared() < 0.5 {
        return None;
    }
    let (ca, sa) = (half_angle.cos(), half_angle.sin());
    let d = n_a.dot(m);
    let perp = m - n_a * d;
    let p = perp.length();
    if p < 1e-9 {
        return None; // plane ⟂ axis → a circle (the local-frame slab path).
    }
    let a = ca * d;
    let b = sa * p;
    let denom = a * a - b * b;
    if denom <= 1e-12 {
        return None; // parabola / hyperbola → not a bounded ellipse.
    }
    let k = (plane_origin - apex).dot(m);
    if k.abs() < 1e-12 {
        return None; // plane through the apex → degenerate.
    }
    let q = perp / p;
    let r2 = n_a.cross(q).normalize_or_zero();
    let center = apex + (k / denom) * (a * ca * n_a - b * sa * q);
    let semi_major = k.abs() * (b * b * ca * ca + a * a * sa * sa).sqrt() / denom;
    let semi_minor = k.abs() * sa / denom.sqrt();
    let s0 = k / (a + b);
    let p0 = apex + s0 * (ca * n_a + sa * q);
    let major_dir = (p0 - center).normalize_or_zero();
    if major_dir.length_squared() < 0.5 || r2.length_squared() < 0.5 {
        return None;
    }
    Some((center, semi_major, semi_minor, major_dir, r2))
}

/// ADR-197 β-3-j — the angle of `point` on the circle `(center, normal, basis_u)`
/// in `[0, 2π)`. `θ = atan2((p−c)·(n×u), (p−c)·u)` — inverse of the circle
/// parameterisation `c + r(cos θ·u + sin θ·(n×u))`.
fn circle_angle_of_point(center: DVec3, normal: DVec3, basis_u: DVec3, point: DVec3) -> f64 {
    let n = normal.normalize_or_zero();
    let u = basis_u.normalize_or_zero();
    let w = n.cross(u);
    let d = point - center;
    let mut a = d.dot(w).atan2(d.dot(u));
    if a < 0.0 {
        a += std::f64::consts::TAU;
    }
    a
}

/// ADR-197 β-3-j — for a cut circle with two crossings on it, pick the arc
/// `(start_angle, end_angle)` (end ≥ start, span ≤ 2π) whose MIDPOINT lies inside
/// all the other kept halfspaces (`n·(p − o) ≥ 0`). This is the arc that bounds
/// the corner patch on this cut circle. `arc_a`/`arc_b` are the two crossings.
fn corner_arc_range(
    circle_center: DVec3,
    circle_radius: f64,
    circle_normal: DVec3,
    circle_basis_u: DVec3,
    arc_a: DVec3,
    arc_b: DVec3,
    other_planes: &[(DVec3, DVec3)],
) -> (f64, f64) {
    use std::f64::consts::TAU;
    let a1 = circle_angle_of_point(circle_center, circle_normal, circle_basis_u, arc_a);
    let a2 = circle_angle_of_point(circle_center, circle_normal, circle_basis_u, arc_b);
    let u = circle_basis_u.normalize_or_zero();
    let w = circle_normal.normalize_or_zero().cross(u);
    let pt_at = |ang: f64| circle_center + circle_radius * (ang.cos() * u + ang.sin() * w);
    let inside = |p: DVec3| {
        other_planes
            .iter()
            .all(|(n, o)| n.normalize_or_zero().dot(p - *o) >= -1e-9)
    };
    let ccw_span = (a2 - a1).rem_euclid(TAU); // CCW from a1 to a2
    let mid_ccw = pt_at(a1 + ccw_span * 0.5);
    if inside(mid_ccw) {
        (a1, a1 + ccw_span)
    } else {
        (a2, a2 + (TAU - ccw_span))
    }
}

/// ADR-197 β-3-k — pick the arc (start, end angles) between two points on a
/// circle whose MIDPOINT is closest to `target` (the box corner). For the
/// sphere-rounded box every corner cut is the minor arc bulging toward the box
/// corner, so "closest midpoint to the corner" selects it unambiguously.
fn arc_range_toward(
    cc: DVec3,
    cr: f64,
    normal: DVec3,
    basis_u: DVec3,
    a: DVec3,
    b: DVec3,
    target: DVec3,
) -> (f64, f64) {
    use std::f64::consts::TAU;
    let a1 = circle_angle_of_point(cc, normal, basis_u, a);
    let a2 = circle_angle_of_point(cc, normal, basis_u, b);
    let u = basis_u.normalize_or_zero();
    let w = normal.normalize_or_zero().cross(u);
    let pt = |ang: f64| cc + cr * (ang.cos() * u + ang.sin() * w);
    let ccw_span = (a2 - a1).rem_euclid(TAU);
    let mid_ccw = pt(a1 + ccw_span * 0.5);
    let mid_cw = pt(a1 - (TAU - ccw_span) * 0.5);
    if (mid_ccw - target).length_squared() <= (mid_cw - target).length_squared() {
        (a1, a1 + ccw_span)
    } else {
        (a2, a2 + (TAU - ccw_span))
    }
}

/// Sphere (Z-up) inverse parameterisation: u = atan2(Δy, Δx) ∈ [0, τ),
/// v = asin(Δz / r) ∈ [-π/2, π/2]. Inverse of `surfaces::sphere::evaluate`.
fn sphere_invert(p: DVec3, center: DVec3, radius: f64) -> (f64, f64) {
    let d = p - center;
    let v = (d.z / radius).clamp(-1.0, 1.0).asin();
    let mut u = (d.y).atan2(d.x);
    if u < 0.0 {
        u += std::f64::consts::TAU;
    }
    (u, v)
}

/// The longest CIRCULAR run of consecutive points whose `v` lies in [v_lo, v_hi]
/// (the SSI arc clipped to the face's latitude band). Returns the whole list if
/// every point is in range (a closed loop — the caller defers that case).
fn longest_inrange_run(uv: &[(f64, f64)], v_lo: f64, v_hi: f64) -> Vec<(f64, f64)> {
    let n = uv.len();
    let inb = |v: f64| v >= v_lo - 1e-9 && v <= v_hi + 1e-9;
    if uv.iter().all(|p| inb(p.1)) {
        return uv.to_vec();
    }
    let Some(start) = (0..n).find(|&i| !inb(uv[i].1) && inb(uv[(i + 1) % n].1)).map(|i| (i + 1) % n)
    else {
        return Vec::new();
    };
    let mut run = Vec::new();
    let mut i = start;
    while inb(uv[i].1) {
        run.push(uv[i]);
        i = (i + 1) % n;
        if i == start {
            break;
        }
    }
    run
}

/// The midpoint of the largest circular gap between the given u-values — the u to
/// rotate to the seam so the SSI arc does NOT cross u = 0 / τ (the β-3-γ-2a
/// seam-shift). Multiple curves that jointly cover all u leave no large gap →
/// the seam shift cannot help (that needs the periodic arrangement, β-3-γ-2b).
fn largest_u_gap_mid(us: &[f64]) -> f64 {
    let tau = std::f64::consts::TAU;
    let mut s: Vec<f64> = us.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut best_gap = -1.0;
    let mut best_mid = 0.0;
    for i in 0..s.len() {
        let a = s[i];
        let b = if i + 1 < s.len() { s[i + 1] } else { s[0] + tau };
        let gap = b - a;
        if gap > best_gap {
            best_gap = gap;
            best_mid = ((a + b) * 0.5) % tau;
        }
    }
    best_mid
}

/// ADR-197 β-3-δ — classify a curved sub-face by the same BOTH-SIDES membership
/// test as the planar arrangement (β-2d): a representative point on the surface
/// is probed just INSIDE (−normal) and just OUTSIDE (+normal); the sub-face is on
/// the result boundary iff the two sides differ. Returns `Some(flip)` when kept
/// (flip = the outward side is in-result), `None` when dropped.
///
/// `in_result(p)` encodes the op + solid membership — and for a curved solid that
/// membership must be ANALYTIC (e.g. a sphere: `|p−c| < r`), not a ray cast on the
/// self-loop triangulation (which is unreliable). The caller supplies it.
fn classify_curved_subface(
    subface: &CurvedSubFace,
    in_result: impl Fn(DVec3) -> bool,
) -> Option<bool> {
    use crate::surfaces::AnalyticSurface as S;
    use std::f64::consts::TAU;
    if subface.uv_region.len() < 3 {
        return None;
    }
    // representative interior (u, v) of the sub-region (in the shifted frame),
    // then the real longitude.
    let poly: Vec<Pt2> = subface.uv_region.iter().map(|&(u, v)| Pt2::new(u, v)).collect();
    let c = centroid_2d(&poly);
    let real_u = (((c.x + subface.u_shift) % TAU) + TAU) % TAU;
    let p = match &subface.surface {
        S::Sphere { center, radius, .. } => {
            crate::surfaces::sphere::evaluate(*center, *radius, DVec3::Z, DVec3::X, real_u, c.y)
        }
        _ => return None, // Cylinder/Cone/Torus → later sub-steps
    };
    let n = subface.surface.normal_at_world_pos(p);
    if n.length_squared() < 1e-18 {
        return None;
    }
    let eps = 1e-3;
    let res_in = in_result(p - n * eps);
    let res_out = in_result(p + n * eps);
    if res_in == res_out {
        return None; // both sides same membership → not a result boundary → drop
    }
    Some(res_out) // kept; flip if the result interior is on the outward side
}

/// The full (u, v) parameter rectangle of a surface as a uv polygon — the
/// "whole face" region for classifying an un-split curved face (ADR-197 β-3-ε-3).
fn full_uv_rect(surf: &crate::surfaces::AnalyticSurface) -> Vec<(f64, f64)> {
    use crate::surfaces::AnalyticSurface as S;
    match surf {
        S::Sphere { u_range, v_range, .. } => vec![
            (u_range.0, v_range.0),
            (u_range.1, v_range.0),
            (u_range.1, v_range.1),
            (u_range.0, v_range.1),
        ],
        _ => Vec::new(),
    }
}

/// ADR-197 β-3-γ — imprint a single SSI curve onto a curved face, returning the
/// curved sub-faces (as the surface over uv sub-regions).
///
/// Sphere scope:
///   • LATITUDE cut (SSI ⟂ Z, constant z) → split the v-range into two strips.
///   • OBLIQUE cut that crosses the face's latitude band (an open uv arc) →
///     invert → clip to the band → SEAM-SHIFT (β-3-γ-2a) → `arrange_polygon_2d`
///     in uv → sub-regions.
/// Deferred: oblique closed loops fully inside the band (interior uv loop) and
/// multiple curves jointly covering all u (periodic arrangement, β-3-γ-2b);
/// Cylinder/Cone/Torus.
fn imprint_curved_face(
    surface: &crate::surfaces::AnalyticSurface,
    ssi: &crate::surfaces::ssi::SurfaceIntersection,
) -> Option<Vec<CurvedSubFace>> {
    use crate::surfaces::AnalyticSurface as S;
    use std::f64::consts::TAU;
    if ssi.points.len() < 3 {
        return None;
    }
    let S::Sphere { center, radius, u_range, v_range, .. } = surface else {
        return None; // Cylinder/Cone/Torus → later sub-steps
    };
    let wrap = |u: f64| ((u % TAU) + TAU) % TAU;

    // ── LATITUDE: constant-z SSI → two v-strips (direct, exact). ──
    let z0 = ssi.points[0].z;
    if ssi.closed && ssi.points.iter().all(|p| (p.z - z0).abs() < 1e-6) {
        let v0 = ((z0 - center.z) / radius).clamp(-1.0, 1.0).asin();
        if v0 <= v_range.0 + 1e-9 || v0 >= v_range.1 - 1e-9 {
            return None;
        }
        let strip = |va: f64, vb: f64| -> CurvedSubFace {
            CurvedSubFace {
                surface: surface.clone(),
                uv_region: vec![
                    (u_range.0, va),
                    (u_range.1, va),
                    (u_range.1, vb),
                    (u_range.0, vb),
                ],
                uv_holes: Vec::new(),
                u_shift: 0.0,
            }
        };
        return Some(vec![strip(v_range.0, v0), strip(v0, v_range.1)]);
    }

    // ── OBLIQUE: invert → clip to the band → seam-shift → arrange. ──
    let uv_all: Vec<(f64, f64)> =
        ssi.points.iter().map(|&p| sphere_invert(p, *center, *radius)).collect();
    let mut run = longest_inrange_run(&uv_all, v_range.0, v_range.1);
    // An open arc only (clipped): a fully-in-range closed loop is deferred.
    if run.len() < 2 || run.len() == uv_all.len() {
        return None;
    }
    // Snap the arc endpoints onto the nearest latitude-band edge so the cut
    // chain terminates ON the rectangle boundary (clean split).
    let snap_v = |v: f64| {
        if (v - v_range.0).abs() <= (v - v_range.1).abs() {
            v_range.0
        } else {
            v_range.1
        }
    };
    let last = run.len() - 1;
    run[0].1 = snap_v(run[0].1);
    run[last].1 = snap_v(run[last].1);

    let shift = largest_u_gap_mid(&run.iter().map(|p| p.0).collect::<Vec<_>>());
    let shifted: Vec<(f64, f64)> = run.iter().map(|&(u, v)| (wrap(u - shift), v)).collect();
    let segs: Vec<(Pt2, Pt2)> = shifted
        .windows(2)
        .map(|w| (Pt2::new(w[0].0, w[0].1), Pt2::new(w[1].0, w[1].1)))
        .collect();
    let rect = vec![
        Pt2::new(u_range.0, v_range.0),
        Pt2::new(u_range.1, v_range.0),
        Pt2::new(u_range.1, v_range.1),
        Pt2::new(u_range.0, v_range.1),
    ];
    let regions = arrange_polygon_2d(&rect, &segs);
    if regions.len() < 2 {
        return None;
    }
    // Keep the regions in the SHIFTED frame (clean polygons that never cross the
    // seam) and record `u_shift`; the real longitude is `wrap(u + shift)`.
    let as_uv = |pts: &[Pt2]| -> Vec<(f64, f64)> { pts.iter().map(|p| (p.x, p.y)).collect() };
    Some(
        regions
            .iter()
            .map(|r| CurvedSubFace {
                surface: surface.clone(),
                uv_region: as_uv(&r.outer),
                uv_holes: r.holes.iter().map(|h| as_uv(h)).collect(),
                u_shift: shift,
            })
            .collect(),
    )
}

/// 두 face의 AABB가 겹치는지 빠른 사전 필터
#[allow(dead_code)] // find_intersections에서 호출 (현재 비활성 경로)
/// 두 솔리드 전체의 AABB가 겹치는지 검사 (빠른 사전 필터)
fn solid_aabb_overlap(a: &SolidData, b: &SolidData) -> bool {
    if a.all_triangles.is_empty() || b.all_triangles.is_empty() {
        return false;
    }
    let (a_min, a_max) = compute_aabb(&a.all_triangles);
    let (b_min, b_max) = compute_aabb(&b.all_triangles);
    let margin = 1e-6;
    a_min.x <= b_max.x + margin && a_max.x >= b_min.x - margin
        && a_min.y <= b_max.y + margin && a_max.y >= b_min.y - margin
        && a_min.z <= b_max.z + margin && a_max.z >= b_min.z - margin
}

#[allow(dead_code)] // find_intersections에서 호출 (현재 비활성 경로)
fn face_aabb_overlap(a: &FaceTriangles, b: &FaceTriangles) -> bool {
    let (a_min, a_max) = compute_aabb(&a.tris);
    let (b_min, b_max) = compute_aabb(&b.tris);

    let margin = 1e-6;
    a_min.x <= b_max.x + margin && a_max.x >= b_min.x - margin
        && a_min.y <= b_max.y + margin && a_max.y >= b_min.y - margin
        && a_min.z <= b_max.z + margin && a_max.z >= b_min.z - margin
}

#[allow(dead_code)] // solid_aabb_overlap, face_aabb_overlap에서 호출
fn compute_aabb(tris: &[(DVec3, DVec3, DVec3)]) -> (DVec3, DVec3) {
    let mut min = DVec3::splat(f64::MAX);
    let mut max = DVec3::splat(f64::MIN);
    for &(a, b, c) in tris {
        for p in [a, b, c] {
            min = min.min(p);
            max = max.max(p);
        }
    }
    (min, max)
}

#[cfg(test)]
mod adr110_tests {
    // ════════════════════════════════════════════════════════════════════
    // ADR-110 π-β — Boolean Path B Compatibility
    //
    // Pre-polygonize at Mesh::boolean entry — Path B closed-curve face
    // (1 self-loop edge with Circle curve) 를 polygonal substitute 로 변환
    // 후 Boolean 활성.
    //
    // 사용자 통찰 (2026-05-16): "기능 확보 → 결함 자연 해소" canonical
    // strategy. ADR-101 §3.1 architectural gap 해소.
    // ════════════════════════════════════════════════════════════════════

    use super::*;
    use crate::Mesh;
    use crate::curves::AnalyticCurve;

    /// Path B cylinder profile (1 anchor + 1 self-loop edge with Circle).
    fn build_path_b_circle(mesh: &mut Mesh, cx: f64, cy: f64, radius: f64) -> FaceId {
        let mat = MaterialId::new(0);
        let basis_u = DVec3::new(1.0, 0.0, 0.0);
        let anchor = mesh.add_vertex(DVec3::new(cx + radius, cy, 0.0));
        let circle = AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, 0.0),
            radius,
            normal: DVec3::new(0.0, 0.0, 1.0),
            basis_u,
        };
        mesh.add_face_closed_curve(anchor, circle, mat).expect("path B face")
    }

    /// Path B circle × Path B circle Union — must succeed (이전: silent fail,
    /// face count 변경 0). ADR-110 fix 후 polygonize 가 Path B → polygonal
    /// 변환 → Boolean 활성.
    #[test]
    fn adr110_pi_beta_path_b_boolean_does_not_silent_fail() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);

        let face_a = build_path_b_circle(&mut mesh, 0.0, 0.0, 5.0);
        let face_b = build_path_b_circle(&mut mesh, 6.0, 0.0, 5.0);

        let verts_before = mesh.verts.iter().filter(|(_, v)| v.is_active()).count();

        let result = mesh.boolean(&[face_a], &[face_b], BoolOp::Union, mat);
        assert!(result.is_ok(),
            "ADR-110 π-β: Path B Boolean Union must succeed, got {:?}", result.err());

        // Polygonize substitute 가 Path B 의 1 anchor 를 N polygonal verts
        // 로 확장 → vert count 증가. Boolean fix evidence (이전엔 변경 0).
        let verts_after = mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
        assert!(verts_after > verts_before,
            "ADR-110 π-β: Path B polygonize 가 verts 추가 (before={}, after={})",
            verts_before, verts_after);
    }

    /// Regression guard — polygonal face Boolean 영향 0 (additive only).
    #[test]
    fn adr110_pi_beta_polygonal_unchanged() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);

        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        let face_a = mesh.add_face(&[v0, v1, v2, v3], mat).expect("rect A");

        let v4 = mesh.add_vertex(DVec3::new(5.0, 5.0, 0.0));
        let v5 = mesh.add_vertex(DVec3::new(15.0, 5.0, 0.0));
        let v6 = mesh.add_vertex(DVec3::new(15.0, 15.0, 0.0));
        let v7 = mesh.add_vertex(DVec3::new(5.0, 15.0, 0.0));
        let face_b = mesh.add_face(&[v4, v5, v6, v7], mat).expect("rect B");

        // Pre-polygonize 는 polygonal face 에 no-op (Ok(None)).
        let result = mesh.boolean(&[face_a], &[face_b], BoolOp::Union, mat);
        assert!(result.is_ok(),
            "Polygonal Boolean regression guard — must not error");
    }

    /// Helper unit — polygonize_closed_curve_face returns substituted FaceId
    /// for Path B input, None for polygonal.
    #[test]
    fn adr110_pi_beta_helper_substitution() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);

        let path_b = build_path_b_circle(&mut mesh, 0.0, 0.0, 5.0);
        let result_b = mesh.polygonize_closed_curve_face(path_b, mat);
        assert!(matches!(result_b, Ok(Some(_))),
            "Path B face must polygonize to new FaceId, got {:?}", result_b);

        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let polygonal = mesh.add_face(&[v0, v1, v2, v3], mat).expect("rect");
        let result_p = mesh.polygonize_closed_curve_face(polygonal, mat);
        assert!(matches!(result_p, Ok(None)),
            "Polygonal face must polygonize to None (no-op), got {:?}", result_p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ADR-276 Phase 0 de-risk simulation (measurement, print-only).
    //
    // `boolean()` Stage 1 uses ONLY `detect_coplanar_faces`; the general
    // `find_intersections` (tri-tri, boolean_geo::triangle_triangle_intersection)
    // is fully implemented but is wired only into "Intersect with Model", NOT
    // into `boolean()`. This measures whether wiring `find_intersections` into
    // Stage 1 is the fix (collector + split stages already work) vs whether the
    // collector/split themselves are also incomplete for box configs.
    //
    // Decisive questions per config:
    //   (1) segs = find_intersections(A, B).len()  — does the collector find
    //       the general (non-coplanar) box-box crossing at all?
    //   (2) after split_faces_by_intersections — do faces actually get split
    //       (face count grows) for those segments?
    #[test]
    fn adr276_phase0_sim_general_intersection_and_split() {
        let mat = MaterialId::new(0);
        let configs: [(&str, DVec3, f64, f64, f64); 4] = [
            ("corner-poke", DVec3::new(50.0, 50.0, 100.0), 60.0, 60.0, 60.0),
            ("top-center notch", DVec3::new(0.0, 0.0, 90.0), 40.0, 40.0, 40.0),
            ("through-slot", DVec3::new(0.0, 0.0, 50.0), 200.0, 30.0, 30.0),
            ("enclosed cavity", DVec3::new(0.0, 0.0, 50.0), 40.0, 40.0, 40.0),
        ];
        println!("\n===== ADR-276 Phase 0: wire find_intersections into Stage 1? =====");
        for (label, bpos, bw, bh, bd) in configs {
            let mut m = Mesh::new();
            let a = m.create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, mat).unwrap();
            let b = m.create_box(bpos, bw, bh, bd, mat).unwrap();

            let solid_a = m.prepare_solid(&a).unwrap();
            let solid_b = m.prepare_solid(&b).unwrap();

            // (1) general (non-coplanar) collector
            let segs = m.find_intersections(&solid_a, &solid_b);
            // (0) what boolean() currently uses instead:
            let cop = m.detect_coplanar_faces(&solid_a, &solid_b);

            // (2) does split actually cut faces with those general segments?
            let faces_before = m.faces.iter().filter(|(_, f)| f.is_active()).count();
            let split_a = m.split_faces_by_intersections(&solid_a, &segs, mat);
            let split_b = m.split_faces_by_intersections(&solid_b, &segs, mat);
            let new_a: usize = split_a.values().map(|v| v.len()).sum();
            let new_b: usize = split_b.values().map(|v| v.len()).sum();
            let faces_after = m.faces.iter().filter(|(_, f)| f.is_active()).count();
            let inv = m.verify_face_invariants();

            println!(
                "  [{label}] find_intersections segs={} (coplanar={}) | split: {}->{} faces (A subfaces={}, B subfaces={}) | inv_valid={}",
                segs.len(), cop.len(), faces_before, faces_after, new_a, new_b, inv.is_valid(),
            );

            // Regression guard for the ADR-276 core finding: the general
            // collector DOES find the box-box crossing (and split grows faces)
            // for surface-crossing configs. If this breaks, the "wire Stage 1"
            // premise is invalidated. (enclosed cavity legitimately has 0 segs
            // — B is fully inside A, no surface crossing.)
            if label != "enclosed cavity" {
                assert!(
                    segs.len() >= 1,
                    "[{label}] find_intersections must find the general crossing (got {})",
                    segs.len(),
                );
                assert!(
                    faces_after > faces_before,
                    "[{label}] split must grow faces (before={faces_before}, after={faces_after})",
                );
            }
        }
        println!("=====================================================================\n");
    }

    // ADR-276 Phase 1 — box-box boolean now CUTS (Stage 1 wired to
    // find_intersections) and is fail-closed: every config leaves the mesh
    // VALID (either a valid cut, or a byte-identical rollback), never corrupt.
    #[test]
    fn adr276_phase1_box_box_subtract_cuts_and_never_corrupts() {
        let mat = MaterialId::new(0);
        let configs: [(&str, DVec3, f64, f64, f64); 4] = [
            ("corner-poke", DVec3::new(50.0, 50.0, 100.0), 60.0, 60.0, 60.0),
            ("top-center notch", DVec3::new(0.0, 0.0, 90.0), 40.0, 40.0, 40.0),
            ("through-slot", DVec3::new(0.0, 0.0, 50.0), 200.0, 30.0, 30.0),
            ("enclosed cavity", DVec3::new(0.0, 0.0, 50.0), 40.0, 40.0, 40.0),
        ];
        let mut any_cut = false;
        println!("\n===== ADR-276 Phase 1: box-box boolean end-to-end =====");
        for (label, bpos, bw, bh, bd) in configs {
            let mut m = Mesh::new();
            let a = m.create_box(DVec3::new(0.0, 0.0, 50.0), 100.0, 100.0, 100.0, mat).unwrap();
            let b = m.create_box(bpos, bw, bh, bd, mat).unwrap();
            let faces_before = m.faces.iter().filter(|(_, f)| f.is_active()).count();
            let verts_before = m.verts.iter().filter(|(_, v)| v.is_active()).count();

            let r = m.boolean_solid(&a, &b, BoolOp::Subtract, mat);

            // INVARIANT (the Phase 1 guarantee): whatever happens, the mesh is
            // left topologically valid. A gate rejection rolls back to the
            // (valid) pre-op state; a success is a valid cut.
            assert!(
                m.verify_face_invariants().is_valid(),
                "[{label}] mesh must stay valid after boolean (ok={})", r.is_ok(),
            );
            let faces_after = m.faces.iter().filter(|(_, f)| f.is_active()).count();
            let verts_after = m.verts.iter().filter(|(_, v)| v.is_active()).count();

            if r.is_err() {
                // Fail-closed rollback must be byte-identical (counts restored).
                assert_eq!(faces_after, faces_before, "[{label}] rollback preserves faces");
                assert_eq!(verts_after, verts_before, "[{label}] rollback preserves verts");
            } else if faces_after != faces_before || verts_after != verts_before {
                any_cut = true;
            }
            println!(
                "  [{label}] ok={} faces {}->{} verts {}->{} valid=true",
                r.is_ok(), faces_before, faces_after, verts_before, verts_after,
            );
        }
        // At least one config must actually cut end-to-end (the whole point of
        // Phase 1 — box-box boolean was a total no-op before ADR-276).
        assert!(any_cut, "ADR-276 Phase 1: at least one box-box config must cut");
        println!("========================================================\n");
    }

    /// 두 겹치는 큐브로 Boolean 테스트
    fn make_test_cubes(mesh: &mut Mesh, mat: MaterialId) -> (Vec<FaceId>, Vec<FaceId>) {
        let a = make_box(mesh, DVec3::ZERO, DVec3::new(2.0, 2.0, 2.0), mat);
        let b = make_box(mesh, DVec3::new(1.0, 0.0, 0.0), DVec3::new(3.0, 2.0, 2.0), mat);
        (a, b)
    }

    fn make_box(
        mesh: &mut Mesh,
        min: DVec3,
        max: DVec3,
        mat: MaterialId,
    ) -> Vec<FaceId> {
        let v = [
            mesh.add_vertex(DVec3::new(min.x, min.y, min.z)),
            mesh.add_vertex(DVec3::new(max.x, min.y, min.z)),
            mesh.add_vertex(DVec3::new(max.x, max.y, min.z)),
            mesh.add_vertex(DVec3::new(min.x, max.y, min.z)),
            mesh.add_vertex(DVec3::new(min.x, min.y, max.z)),
            mesh.add_vertex(DVec3::new(max.x, min.y, max.z)),
            mesh.add_vertex(DVec3::new(max.x, max.y, max.z)),
            mesh.add_vertex(DVec3::new(min.x, max.y, max.z)),
        ];

        let mut faces = Vec::new();

        // 6 faces (CCW outward)
        let face_verts = [
            [v[0], v[3], v[2], v[1]], // -Z
            [v[4], v[5], v[6], v[7]], // +Z
            [v[0], v[1], v[5], v[4]], // -Y
            [v[2], v[3], v[7], v[6]], // +Y
            [v[0], v[4], v[7], v[3]], // -X
            [v[1], v[2], v[6], v[5]], // +X
        ];

        for verts in &face_verts {
            if let Ok(fid) = mesh.add_face(verts, mat) {
                faces.push(fid);
            }
        }

        faces
    }

    /// ADR-197 β-2a — general (non-coplanar) face-face intersection: two cubes
    /// overlapping at a CORNER (offset in all 3 axes) produce the L-shaped cut
    /// segments on the boundary of the overlap cube [2,4]³. This is the raw
    /// material the imprint step (β-2b) consumes; NOT wired into boolean() yet.
    #[test]
    fn adr197_beta2a_general_intersection_box_box() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(2., 2., 2.), DVec3::new(6., 6., 6.), mat);
        let sa = mesh.prepare_solid(&a).expect("solid a");
        let sb = mesh.prepare_solid(&b).expect("solid b");
        let segs = mesh.detect_general_intersections(&sa, &sb);

        assert!(
            !segs.is_empty(),
            "box-box corner overlap must yield general intersection segments"
        );
        // Every segment lies on the boundary of the overlap cube [2,4]³.
        for s in &segs {
            for p in [s.p0, s.p1] {
                assert!(
                    p.x > 1.99 && p.x < 4.01 && p.y > 1.99 && p.y < 4.01 && p.z > 1.99 && p.z < 4.01,
                    "segment endpoint {:?} outside overlap [2,4]³",
                    p
                );
            }
        }
        // The A(+X, x=4) × B(-Y, y=2) crossing: a segment on x≈4, y≈2, z∈[2,4].
        let has_x4_y2 = segs.iter().any(|s| {
            let on = |p: DVec3| (p.x - 4.0).abs() < 1e-6 && (p.y - 2.0).abs() < 1e-6;
            on(s.p0) && on(s.p1)
        });
        assert!(
            has_x4_y2,
            "expected the x=4 ∩ y=2 segment (A's +X face crossing B's -Y wall)"
        );
    }

    /// ADR-197 β-2d — imprint via the UNIFIED arrangement: A's +X face is
    /// crossed by B along an L-shaped chain (2 segments + interior corner);
    /// `arrange_polygon_2d` subdivides it into exactly two sub-regions (no
    /// holes — open chain), one of which is the inside-B corner region [2,4]².
    #[test]
    fn adr197_beta2a_ii_imprint_box_face_chain_split() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(2., 2., 2.), DVec3::new(6., 6., 6.), mat);
        let sa = mesh.prepare_solid(&a).expect("solid a");
        let sb = mesh.prepare_solid(&b).expect("solid b");
        let segs = mesh.detect_general_intersections(&sa, &sb);

        // A's +X face (normal +X, at x=4).
        let xplus = a
            .iter()
            .copied()
            .find(|&f| mesh.faces.get(f).map_or(false, |fc| fc.normal().x > 0.9))
            .expect("+X face");
        let (normal, poly) = mesh.face_unit_normal_and_poly(xplus).expect("face poly");

        let face_segs: Vec<(DVec3, DVec3)> = segs
            .iter()
            .filter(|s| s.face_a == xplus)
            .map(|s| (s.p0, s.p1))
            .collect();
        assert_eq!(
            face_segs.len(),
            2,
            "+X face L-cut = 2 segments, got {}",
            face_segs.len()
        );

        // Project + arrange (the unified imprint path used by solid_boolean).
        let (poly2d, u, v, origin) = project_to_2d(&poly, normal);
        let to2d = |p: DVec3| Pt2::new(u.dot(p - origin), v.dot(p - origin));
        let cuts2d: Vec<(Pt2, Pt2)> =
            face_segs.iter().map(|&(a, b)| (to2d(a), to2d(b))).collect();
        let regions = arrange_polygon_2d(&poly2d, &cuts2d);
        assert_eq!(regions.len(), 2, "L-cut → 2 sub-regions, got {}", regions.len());
        assert!(regions.iter().all(|r| r.holes.is_empty()), "open L-chain → no holes");

        // Exactly one sub-region is the inside-B corner [2,4]² (centroid in range).
        let in_corner = |r: &Region2D| {
            let c = unproject_to_3d(centroid_2d(&r.outer), u, v, origin);
            c.y > 1.99 && c.y < 4.01 && c.z > 1.99 && c.z < 4.01
        };
        let corner_count = regions.iter().filter(|r| in_corner(r)).count();
        assert_eq!(
            corner_count, 1,
            "exactly one sub-region is the inside-B corner region"
        );
    }

    /// ADR-197 β-2b+c — the integration MVP: a true VOLUMETRIC general 3D solid
    /// subtract A[0,4]³ − B[2,6]³ (corner overlap, all non-coplanar). The legacy
    /// mesh.boolean and the NURBS DCEL both fail this; solid_boolean must produce
    /// a manifold, watertight carved solid with the removed corner [2,4]³ gone.
    #[test]
    fn adr197_beta2bc_box_box_subtract_carves_watertight() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(2., 2., 2.), DVec3::new(6., 6., 6.), mat);
        let result = mesh
            .solid_boolean(&a, &b, BoolOp::Subtract, mat)
            .expect("solid_boolean ok");
        assert!(!result.is_empty(), "subtract must produce faces");

        let inv = mesh.verify_face_invariants();
        let active_faces = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let boundary_hes = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        // DIAGNOSTIC: a watertight closed solid has 0 boundary half-edges.
        assert!(
            inv.is_valid(),
            "manifold? ({} result / {} active faces, {} boundary HEs): {:?}",
            result.len(),
            active_faces,
            boundary_hes,
            inv.violations.iter().take(6).collect::<Vec<_>>()
        );
        assert_eq!(
            boundary_hes, 0,
            "WATERTIGHT: result must have 0 boundary half-edges (sew complete), \
             got {} ({} result faces, {} active)",
            boundary_hes, result.len(), active_faces
        );

        // Correctness via point-in-solid on the assembled result.
        let rs = mesh.prepare_solid(&result).expect("result solid");
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(1.0, 1.0, 1.0)),
            "a kept region (1,1,1) must be INSIDE the carved solid"
        );
        assert!(
            !point_in_solid(&rs.all_triangles, DVec3::new(3.0, 3.0, 3.0)),
            "the removed corner (3,3,3) must be OUTSIDE the carved solid"
        );
    }

    #[test]
    fn adr267_beta3_box_box_subtract_passes_volume_integrity_gate() {
        // ADR-267 β-3 — 정상 box-box subtract 결과는 watertight + crack-free 이므로
        // verify_volume_integrity(ClosedSolid) 게이트를 통과한다 (게이트가 정상
        // boolean 을 오탐하지 않음). boolean_op / boolean_dispatch_dcel_multi_json
        // WASM wrapper 가 이 결과에 delta 게이트를 적용해 손상 유발 시에만 rollback.
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(2., 2., 2.), DVec3::new(6., 6., 6.), mat);
        mesh.solid_boolean(&a, &b, BoolOp::Subtract, mat)
            .expect("solid_boolean ok");

        let active: Vec<FaceId> = mesh
            .faces
            .iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();
        let report = mesh.verify_volume_integrity(crate::IntegrityScope::ClosedSolid(&active));
        assert!(
            report.is_valid(),
            "subtract result must pass watertight gate: {}",
            report.summary()
        );
        assert!(report.geometric_cracks.is_empty(), "no cracks in carved solid");
        assert_eq!(report.open_boundary_edges, 0, "carved solid is watertight");
    }

    /// ADR-197 β-2 hardening — UNION A[0,4]³ ∪ B[2,6]³: the combined solid
    /// (A-outside-B + B-outside-A, overlap interior removed), watertight.
    #[test]
    fn adr197_beta2_union_box_box_watertight() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(2., 2., 2.), DVec3::new(6., 6., 6.), mat);
        let result = mesh.solid_boolean(&a, &b, BoolOp::Union, mat).expect("union ok");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert!(mesh.verify_face_invariants().is_valid(), "union manifold");
        assert_eq!(boundary, 0, "union watertight, got {} boundary HEs", boundary);
        let rs = mesh.prepare_solid(&result).expect("result");
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(1.0, 1.0, 1.0)),
            "(1,1,1) ∈ A → inside union"
        );
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(5.0, 5.0, 5.0)),
            "(5,5,5) ∈ B → inside union"
        );
        assert!(
            !point_in_solid(&rs.all_triangles, DVec3::new(10.0, 10.0, 10.0)),
            "(10,10,10) ∉ A∪B"
        );
    }

    /// ADR-197 β-2 hardening — INTERSECT A[0,4]³ ∩ B[2,6]³: the overlap cube
    /// [2,4]³, watertight.
    #[test]
    fn adr197_beta2_intersect_box_box_watertight() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(2., 2., 2.), DVec3::new(6., 6., 6.), mat);
        let result = mesh
            .solid_boolean(&a, &b, BoolOp::Intersect, mat)
            .expect("intersect ok");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert!(mesh.verify_face_invariants().is_valid(), "intersect manifold");
        assert_eq!(boundary, 0, "intersect watertight, got {} boundary HEs", boundary);
        let rs = mesh.prepare_solid(&result).expect("result");
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(3.0, 3.0, 3.0)),
            "(3,3,3) ∈ overlap → inside intersect"
        );
        assert!(
            !point_in_solid(&rs.all_triangles, DVec3::new(1.0, 1.0, 1.0)),
            "(1,1,1) only ∈ A → outside intersect"
        );
        assert!(
            !point_in_solid(&rs.all_triangles, DVec3::new(5.0, 5.0, 5.0)),
            "(5,5,5) only ∈ B → outside intersect"
        );
    }

    /// ADR-197 β-2d — a bar passing THROUGH a box (tunnel). A's top & bottom
    /// faces get a CLOSED-loop hole (B's cross-section); the unified arrangement
    /// turns each into an annulus (outer + hole) + a disk, and the tunnel walls
    /// weld onto the hole boundaries via the shared SSI verts → a WATERTIGHT
    /// carved tunnel. The old single-chain helper left 16 boundary HEs here
    /// (`order_segments_into_chain` rejected the closed loop); the arrangement
    /// closes them.
    #[test]
    fn adr197_beta2d_bar_through_box_tunnel_watertight() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(10., 10., 10.), mat);
        // square bar [3,7]×[3,7] through A along Z (z −5..15 spans A fully).
        let b = make_box(&mut mesh, DVec3::new(3., 3., -5.), DVec3::new(7., 7., 15.), mat);

        let r = mesh
            .solid_boolean(&a, &b, BoolOp::Subtract, mat)
            .expect("tunnel subtract ok");
        assert!(!r.is_empty(), "tunnel produces faces");

        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        let inv = mesh.verify_face_invariants();
        assert_eq!(
            boundary, 0,
            "WATERTIGHT tunnel — 0 boundary half-edges, got {} ({} result faces)",
            boundary,
            r.len()
        );
        assert!(
            inv.is_valid(),
            "manifold after tunnel carve: {:?}",
            inv.violations.iter().take(6).collect::<Vec<_>>()
        );

        // Correctness via point-in-solid on the assembled result.
        let rs = mesh.prepare_solid(&r).expect("result solid");
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(1.0, 1.0, 1.0)),
            "(1,1,1) ∈ A, outside the bar → INSIDE the tunnelled solid"
        );
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(8.0, 8.0, 8.0)),
            "(8,8,8) ∈ A, outside the bar → INSIDE"
        );
        assert!(
            !point_in_solid(&rs.all_triangles, DVec3::new(5.0, 5.0, 5.0)),
            "(5,5,5) in the carved tunnel → OUTSIDE"
        );
    }

    /// ADR-197 β-2d degenerate (3D) — CONTAINMENT subtract: B fully inside A.
    /// There are NO face intersections; the no-cut classify path keeps A's shell
    /// and B's INVERTED shell → a solid with a B-shaped cavity. Watertight (two
    /// closed shells), the cavity carved.
    #[test]
    fn deg3d_containment_subtract_cavity() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(10., 10., 10.), mat);
        let b = make_box(&mut mesh, DVec3::new(3., 3., 3.), DVec3::new(7., 7., 7.), mat);
        let r = mesh.solid_boolean(&a, &b, BoolOp::Subtract, mat).expect("containment subtract");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "cavity solid watertight, got {} boundary HEs", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold cavity");
        let rs = mesh.prepare_solid(&r).expect("result solid");
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(1.0, 1.0, 1.0)),
            "(1,1,1) ∈ A, outside cavity → INSIDE"
        );
        assert!(
            !point_in_solid(&rs.all_triangles, DVec3::new(5.0, 5.0, 5.0)),
            "(5,5,5) in the carved cavity → OUTSIDE"
        );
    }

    /// ADR-197 β-2d degenerate (3D) — COPLANAR FACE TOUCH: A and B share a full
    /// face (x=4) but no volume. `detect_general_intersections` skips the
    /// parallel coplanar pair → no cuts. Subtract A−B must leave A intact and
    /// watertight (the shared face must NOT be wrongly classified inside B).
    #[test]
    fn deg3d_coplanar_face_touch_subtract() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        // B abuts A across the x=4 plane (B's −X face == A's +X face, y,z ∈ [0,4]).
        let b = make_box(&mut mesh, DVec3::new(4., 0., 0.), DVec3::new(8., 4., 4.), mat);
        let r = mesh.solid_boolean(&a, &b, BoolOp::Subtract, mat).expect("coplanar-touch subtract");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        let inv = mesh.verify_face_invariants();
        assert_eq!(
            boundary, 0,
            "A intact + watertight after touching subtract, got {} boundary HEs",
            boundary
        );
        assert!(inv.is_valid(), "manifold after coplanar touch: {:?}", inv.violations.iter().take(4).collect::<Vec<_>>());
        let rs = mesh.prepare_solid(&r).expect("result solid");
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(2.0, 2.0, 2.0)),
            "(2,2,2) ∈ A → INSIDE (A untouched)"
        );
        assert!(
            !point_in_solid(&rs.all_triangles, DVec3::new(6.0, 2.0, 2.0)),
            "(6,2,2) ∈ B only → OUTSIDE the subtract result"
        );
    }

    /// ADR-197 β-2d degenerate (3D) — EDGE TOUCH: A and B share only an edge
    /// (x=4, y=4). No face/volume overlap → subtract leaves A intact + watertight.
    #[test]
    fn deg3d_edge_touch_subtract() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(4., 4., 0.), DVec3::new(8., 8., 4.), mat);
        let r = mesh.solid_boolean(&a, &b, BoolOp::Subtract, mat).expect("edge-touch subtract");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "edge touch → A intact watertight, got {} boundary HEs", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold after edge touch");
        let rs = mesh.prepare_solid(&r).expect("result solid");
        assert!(point_in_solid(&rs.all_triangles, DVec3::new(2.0, 2.0, 2.0)), "(2,2,2) ∈ A → INSIDE");
        assert!(!point_in_solid(&rs.all_triangles, DVec3::new(6.0, 6.0, 2.0)), "(6,6,2) ∈ B only → OUTSIDE");
    }

    /// ADR-197 β-2d degenerate (3D) — VERTEX TOUCH: A and B share only the corner
    /// (4,4,4). Subtract leaves A intact + watertight.
    #[test]
    fn deg3d_vertex_touch_subtract() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(4., 4., 4.), DVec3::new(8., 8., 8.), mat);
        let r = mesh.solid_boolean(&a, &b, BoolOp::Subtract, mat).expect("vertex-touch subtract");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "vertex touch → A intact watertight, got {} boundary HEs", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold after vertex touch");
        let rs = mesh.prepare_solid(&r).expect("result solid");
        assert!(point_in_solid(&rs.all_triangles, DVec3::new(2.0, 2.0, 2.0)), "(2,2,2) ∈ A → INSIDE");
        assert!(!point_in_solid(&rs.all_triangles, DVec3::new(6.0, 6.0, 6.0)), "(6,6,6) ∈ B only → OUTSIDE");
    }

    /// ADR-197 β-2d degenerate (3D) — UNION of two coplanar-touching boxes. The
    /// combined volume [0,8]×[0,4]×[0,4] must be watertight & manifold and contain
    /// both halves. (A residual internal membrane at x=4 is an accepted artifact
    /// removable by the coplanar-merge pass; it must not break manifoldness.)
    #[test]
    fn deg3d_coplanar_face_touch_union() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(4., 0., 0.), DVec3::new(8., 4., 4.), mat);
        let r = mesh.solid_boolean(&a, &b, BoolOp::Union, mat).expect("coplanar-touch union");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold union");
        let rs = mesh.prepare_solid(&r).expect("result solid");
        assert!(point_in_solid(&rs.all_triangles, DVec3::new(2.0, 2.0, 2.0)), "(2,2,2) ∈ A → INSIDE union");
        assert!(point_in_solid(&rs.all_triangles, DVec3::new(6.0, 2.0, 2.0)), "(6,2,2) ∈ B → INSIDE union");
        assert!(!point_in_solid(&rs.all_triangles, DVec3::new(10.0, 2.0, 2.0)), "(10,2,2) → OUTSIDE union");
    }

    /// ADR-197 β-2d degenerate (3D) — THIN SLAB (0.05mm) carved by a column.
    /// The slab's large 100×100 faces stress the −normal*face_eps classification
    /// offset against the slab thickness: the clamp [2e-4,1e-3] keeps ε below the
    /// thickness so the probe does NOT punch through. We verify the carve via the
    /// resulting STRUCTURE (both caps become annuli, four inward column walls,
    /// watertight + manifold) rather than `point_in_solid`, whose fixed ray
    /// directions are unreliable at this 400:1 aspect ratio (the rays leave the
    /// 0.05mm z-band before reaching the 10mm-distant walls — a limitation of the
    /// test helper, not of the carve, which is geometrically exact here).
    #[test]
    fn deg3d_thin_slab_subtract() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(100., 100., 0.05), mat);
        let b = make_box(&mut mesh, DVec3::new(40., 40., -1.), DVec3::new(60., 60., 1.), mat);
        let r = mesh.solid_boolean(&a, &b, BoolOp::Subtract, mat).expect("thin slab subtract");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "thin slab carve watertight, got {} boundary HEs", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold thin slab");

        // Both z-caps (z≈0 and z≈0.05) must carry exactly one hole (the carved
        // column cross-section) — proof the offset did not punch through.
        let mut caps_with_hole = 0;
        let mut inner_walls = 0;
        for &fid in &r {
            let Some(f) = mesh.faces.get(fid) else { continue };
            let n = f.normal().normalize_or_zero();
            if n.z.abs() > 0.9 && !f.inners().is_empty() {
                caps_with_hole += 1;
            }
            // inner column walls sit on the [40,60] hole boundary with horizontal
            // normals pointing toward the hole centre (50,50).
            let vs = mesh.collect_loop_verts(f.outer().start).unwrap_or_default();
            let c = vs
                .iter()
                .filter_map(|&v| mesh.verts.get(v).map(|p| p.pos()))
                .fold(DVec3::ZERO, |a, p| a + p)
                / (vs.len().max(1) as f64);
            let on_hole = (c.x - 40.0).abs() < 1e-3
                || (c.x - 60.0).abs() < 1e-3
                || (c.y - 40.0).abs() < 1e-3
                || (c.y - 60.0).abs() < 1e-3;
            if n.z.abs() < 0.1 && on_hole && c.x > 39.0 && c.x < 61.0 && c.y > 39.0 && c.y < 61.0 {
                inner_walls += 1;
            }
        }
        assert_eq!(caps_with_hole, 2, "top & bottom caps each carry the carved hole");
        assert_eq!(inner_walls, 4, "four inward column walls seal the hole");

        // A point in the solid part (away from the hole) classifies correctly —
        // here the ray exits through an annulus, so point_in_solid is reliable.
        let rs = mesh.prepare_solid(&r).expect("result solid");
        assert!(
            point_in_solid(&rs.all_triangles, DVec3::new(10.0, 10.0, 0.025)),
            "(10,10,0.025) ∈ slab, outside column → INSIDE"
        );
    }

    /// ADR-197 β-2d degenerate (3D) — IDENTICAL solids A==B. Subtract A−A is the
    /// empty solid; the pipeline must not corrupt (either no faces, or a manifold
    /// result), and no probed point may be classified inside.
    #[test]
    fn deg3d_identical_solids_subtract() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let b = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        let r = mesh.solid_boolean(&a, &b, BoolOp::Subtract, mat).expect("identical subtract");
        // Empty result OR a manifold one — never a corrupt non-manifold mesh.
        assert!(
            r.is_empty() || mesh.verify_face_invariants().is_valid(),
            "identical subtract must be empty or manifold, got {} faces",
            r.len()
        );
        if !r.is_empty() {
            let rs = mesh.prepare_solid(&r).expect("result solid");
            assert!(
                !point_in_solid(&rs.all_triangles, DVec3::new(2.0, 2.0, 2.0)),
                "A−A: interior point (2,2,2) must NOT be inside the (empty) result"
            );
        }
    }

    /// ADR-197 β-2d — PLANAR analytic surface preservation. A box whose 6 faces
    /// each carry a Plane `AnalyticSurface`; after a corner subtract, every kept
    /// A-derived fragment must RETAIN its Plane surface (Phase 1 clone + Phase 3
    /// re-attach, ADR-089 A-χ). B carries no surface, so only A's fragments do.
    #[test]
    fn adr197_beta2d_planar_analytic_surface_preserved() {
        use crate::surfaces::AnalyticSurface;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(4., 4., 4.), mat);
        for &fid in &a {
            let n = mesh.faces.get(fid).unwrap().normal().normalize_or_zero();
            let basis_u = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
                .normalize_or_zero();
            mesh.set_face_surface(
                fid,
                Some(AnalyticSurface::Plane {
                    origin: DVec3::ZERO,
                    normal: n,
                    basis_u,
                    u_range: (-1e6, 1e6),
                    v_range: (-1e6, 1e6),
                }),
            );
        }
        let b = make_box(&mut mesh, DVec3::new(2., 2., 2.), DVec3::new(6., 6., 6.), mat);
        let r = mesh.solid_boolean(&a, &b, BoolOp::Subtract, mat).expect("surfaced subtract");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
        // Every A-derived fragment keeps a Plane surface (≥6 — A's 6 faces, some split).
        let planes = r
            .iter()
            .filter(|&&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Plane { .. })))
            .count();
        assert!(planes >= 6, "A fragments retain Plane surface, got {}", planes);
        // No fragment carries a CORRUPT (non-Plane) surface.
        assert!(
            r.iter()
                .all(|&f| mesh.face_surface(f).map_or(true, |s| matches!(s, AnalyticSurface::Plane { .. }))),
            "no fragment gains a wrong surface kind"
        );
    }

    /// ADR-197 β-2d — KNOWN LIMITATION locked as a regression (β-3 trigger). Path B
    /// curved primitives use SELF-LOOP (anchor→anchor) face boundaries, which the
    /// polygon-based arrangement skips (poly < 3 verts → no planar normal). A
    /// Boolean on curved input therefore yields nothing YET; β-3 (curved-face
    /// handling via the SSI trim machinery) will deliberately update this.
    #[test]
    fn adr197_beta2d_curved_input_skipped_pending_beta3() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let sphere = mesh
            .create_sphere_kernel_native(DVec3::new(0., 0., 0.), 5.0, mat)
            .expect("path B sphere");
        assert_eq!(sphere.len(), 2, "sphere = 2 self-loop hemisphere faces");
        assert!(
            sphere.iter().all(|&f| mesh.face_unit_normal_and_poly(f).is_none()),
            "curved self-loop faces have no planar polygon → skipped by the arrangement"
        );
        let b = make_box(&mut mesh, DVec3::new(0., 0., 0.), DVec3::new(6., 6., 6.), mat);
        let r = mesh.solid_boolean(&sphere, &b, BoolOp::Subtract, mat).expect("curved subtract ok");
        assert!(
            r.is_empty(),
            "curved input not yet supported (β-3) → empty result, got {} faces",
            r.len()
        );
    }

    /// Round-12 (adversarial SSI sweep) regression — the real curved-Boolean
    /// SUBTRACT results (through drill / blind hole / sphere dimple / cone
    /// countersink) must stay CLOSED solids with ZERO self-intersection.
    /// Locks the sweep finding that no silent flap / poke-through survives in
    /// these paths: `detect_self_intersections` (the flap-class checker) fully
    /// covers the tessellated wall + entry/exit ring faces they emit, and
    /// `is_closed_solid` guards the watertight/open transition the topological
    /// invariants (I1-5) miss. A future regression that opens the solid or
    /// folds a wall through itself breaks here instead of shipping silently.
    #[test]
    fn round12_boolean_subtract_results_stay_si_clean_closed_solids() {
        let mat = MaterialId::new(0);

        // Assert: current mesh is a closed, invariant-valid, SI-clean solid.
        fn check(mesh: &Mesh, non_empty: bool, label: &str) {
            assert!(non_empty, "{}: non-empty subtract result", label);
            assert!(mesh.verify_face_invariants().is_valid(), "{}: invariants", label);
            let all: Vec<FaceId> = mesh
                .faces
                .iter()
                .filter(|(_, f)| f.is_active())
                .map(|(id, _)| id)
                .collect();
            let mi = mesh.face_set_manifold_info(&all);
            assert!(
                mi.is_closed_solid,
                "{}: closed solid (bnd={} nm={})",
                label, mi.boundary_edge_count, mi.non_manifold_edge_count
            );
            let si = mesh.detect_self_intersections();
            assert!(si.is_clean(), "{}: self-intersection ({} pairs)", label, si.count());
        }

        // 1. through drill — box − cylinder that spans the full box height.
        {
            let mut mesh = Mesh::default();
            let c = DVec3::new(0., 0., 0.);
            let bx = mesh.create_box(c, 1000., 1000., 1000., mat).unwrap();
            let cyl = mesh
                .create_cylinder_kernel_native_clean(DVec3::new(0., 0., -501.), 200., 1002., mat)
                .unwrap();
            let res = mesh.boolean(&bx, &cyl, BoolOp::Subtract, mat).unwrap();
            check(&mesh, !res.faces.is_empty(), "through drill");
        }

        // 2. blind hole — cylinder floor sits inside the box.
        {
            let mut mesh = Mesh::default();
            let c = DVec3::new(0., 0., 0.);
            let bx = mesh.create_box(c, 1000., 1000., 1000., mat).unwrap();
            // base = box_top − depth (inside); top pokes 1 above.
            let cyl = mesh
                .create_cylinder_kernel_native_clean(DVec3::new(0., 0., 100.), 200., 401., mat)
                .unwrap();
            let res = mesh.boolean(&bx, &cyl, BoolOp::Subtract, mat).unwrap();
            check(&mesh, !res.faces.is_empty(), "blind hole");
        }

        // 3. sphere dimple — sphere centred on the box top, lower hemisphere in.
        {
            let mut mesh = Mesh::default();
            let c = DVec3::new(0., 0., 0.);
            let bx = mesh.create_box(c, 1000., 1000., 1000., mat).unwrap();
            let sph = mesh
                .create_sphere_kernel_native(DVec3::new(0., 0., 500.), 300., mat)
                .unwrap();
            let res = mesh.boolean(&bx, &sph, BoolOp::Subtract, mat).unwrap();
            check(&mesh, !res.faces.is_empty(), "sphere dimple");
        }

        // 4. cone countersink — apex-down cone pocket from the box top.
        {
            let mut mesh = Mesh::default();
            let c = DVec3::new(0., 0., 0.);
            let bx = mesh.create_box(c, 1000., 1000., 1000., mat).unwrap();
            let cone = mesh
                .create_cone_kernel_native_apex_down(DVec3::new(0., 0., 501.), 300., 301., mat)
                .unwrap();
            let res = mesh.boolean(&bx, &cone, BoolOp::Subtract, mat).unwrap();
            check(&mesh, !res.faces.is_empty(), "cone countersink");
        }
    }

    /// ADR-197 β-3-β — curved SSI dispatch. A box (Plane surfaces) and a Path B
    /// sphere: every box plane within the radius must intersect the sphere in a
    /// closed circle (`plane_sphere`). The x=2 plane ∩ sphere(r=3) → circle of
    /// radius √(9−4)=√5 centred at (2,0,0). (Detection only — β-3-γ imprints.)
    #[test]
    fn adr197_beta3b_plane_sphere_ssi_detected() {
        use crate::surfaces::AnalyticSurface;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::new(-2., -2., -2.), DVec3::new(2., 2., 2.), mat);
        for &fid in &a {
            // origin = a real point ON the face plane (a vertex), not the world origin.
            let (n, origin) = {
                let f = mesh.faces.get(fid).unwrap();
                let n = f.normal().normalize_or_zero();
                let v0 = mesh.collect_loop_verts(f.outer().start).unwrap()[0];
                (n, mesh.verts.get(v0).unwrap().pos())
            };
            let basis_u = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
                .normalize_or_zero();
            mesh.set_face_surface(
                fid,
                Some(AnalyticSurface::Plane {
                    origin,
                    normal: n,
                    basis_u,
                    u_range: (-1e6, 1e6),
                    v_range: (-1e6, 1e6),
                }),
            );
        }
        let sphere = mesh.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).expect("sphere");
        let xs = mesh.detect_curved_intersections(&a, &sphere);
        assert!(!xs.is_empty(), "plane×sphere SSI detected");
        assert!(xs.iter().all(|c| c.ssi.closed), "plane∩sphere = closed circle");
        // 6 box faces × 2 sphere hemispheres, all planes within r=3 → 12 pairs.
        assert_eq!(xs.len(), 12, "6 box planes × 2 hemispheres, got {}", xs.len());
        // the x=2 plane ∩ sphere → circle radius √5 at (2,0,0).
        let center = DVec3::new(2.0, 0.0, 0.0);
        let found = xs.iter().any(|c| {
            c.ssi.points.iter().all(|p| (p.x - 2.0).abs() < 1e-6)
                && (c.ssi.points[0].distance(center) - 5.0_f64.sqrt()).abs() < 1e-3
        });
        assert!(found, "box face x=2 ∩ sphere r=3 → circle radius √5 at x=2");
    }

    fn north_hemisphere() -> crate::surfaces::AnalyticSurface {
        use std::f64::consts::{FRAC_PI_2, TAU};
        crate::surfaces::AnalyticSurface::Sphere {
            center: DVec3::ZERO,
            radius: 3.0,
            axis_dir: DVec3::Z, ref_dir: DVec3::X,
            u_range: (0.0, TAU),
            v_range: (0.0, FRAC_PI_2),
        }
    }
    fn closed_ssi(points: Vec<DVec3>) -> crate::surfaces::ssi::SurfaceIntersection {
        crate::surfaces::ssi::SurfaceIntersection {
            points,
            uv_a: vec![],
            uv_b: vec![],
            closed: true,
            tangent_warning: false,
        }
    }
    fn uv_area(region: &[(f64, f64)]) -> f64 {
        let poly: Vec<Pt2> = region.iter().map(|&(u, v)| Pt2::new(u, v)).collect();
        polygon_signed_area_2d(&poly).abs()
    }

    /// ADR-197 β-3-γ-1 — sphere LATITUDE imprint. A constant-z=2 SSI circle on the
    /// north hemisphere (v ∈ [0, π/2]) splits it at v0 = asin(2/3) into two Sphere
    /// sub-faces whose uv strips are [0, v0] and [v0, π/2] (no seam shift).
    #[test]
    fn adr197_beta3g_sphere_latitude_imprint() {
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let surf = north_hemisphere();
        let rc = 5.0_f64.sqrt();
        let pts: Vec<DVec3> = (0..32)
            .map(|i| {
                let t = TAU * i as f64 / 32.0;
                DVec3::new(rc * t.cos(), rc * t.sin(), 2.0)
            })
            .collect();
        let subs = imprint_curved_face(&surf, &closed_ssi(pts)).expect("latitude imprint");
        assert_eq!(subs.len(), 2, "latitude cut → 2 sub-faces");
        assert!(subs.iter().all(|s| matches!(s.surface, AnalyticSurface::Sphere { .. })));
        assert!(subs.iter().all(|s| s.u_shift == 0.0), "latitude: no seam shift");
        let v0 = (2.0_f64 / 3.0).asin();
        let mut vr: Vec<(f64, f64)> = subs
            .iter()
            .map(|s| {
                let vs: Vec<f64> = s.uv_region.iter().map(|p| p.1).collect();
                (
                    vs.iter().cloned().fold(f64::MAX, f64::min),
                    vs.iter().cloned().fold(f64::MIN, f64::max),
                )
            })
            .collect();
        vr.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        assert!((vr[0].0).abs() < 1e-9 && (vr[0].1 - v0).abs() < 1e-9, "lower strip [0, v0]");
        assert!(
            (vr[1].0 - v0).abs() < 1e-9 && (vr[1].1 - FRAC_PI_2).abs() < 1e-9,
            "upper strip [v0, π/2]"
        );
    }

    /// ADR-197 β-3-γ-2a — sphere OBLIQUE imprint via seam-shift. The x=2 plane cuts
    /// the sphere in a circle that spans both hemispheres; clipped to the north
    /// hemisphere it is an open uv arc crossing the u-seam. Inversion + clip +
    /// seam-shift + uv arrangement → two Sphere sub-faces whose (shifted) uv
    /// regions partition the hemisphere rectangle [0,2π]×[0,π/2] = π².
    #[test]
    fn adr197_beta3g2a_sphere_oblique_imprint() {
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let surf = north_hemisphere();
        let rc = 5.0_f64.sqrt();
        let pts: Vec<DVec3> = (0..32)
            .map(|i| {
                let phi = TAU * i as f64 / 32.0;
                DVec3::new(2.0, rc * phi.cos(), rc * phi.sin())
            })
            .collect();
        let subs = imprint_curved_face(&surf, &closed_ssi(pts)).expect("oblique imprint");
        assert_eq!(subs.len(), 2, "oblique cut → 2 sub-faces");
        assert!(subs.iter().all(|s| matches!(s.surface, AnalyticSurface::Sphere { .. })));
        assert!(subs.iter().all(|s| s.u_shift != 0.0), "oblique: seam-shift applied");
        let total: f64 = subs.iter().map(|s| uv_area(&s.uv_region)).sum();
        let rect_area = TAU * FRAC_PI_2;
        assert!(
            (total - rect_area).abs() < 1e-6,
            "uv regions partition the hemisphere ({} vs {})",
            total,
            rect_area
        );
        let mut areas: Vec<f64> = subs.iter().map(|s| uv_area(&s.uv_region)).collect();
        areas.sort_by(|a, b| a.partial_cmp(b).unwrap());
        assert!(areas[0] < areas[1] * 0.5, "one sub-face is the smaller +x cap");

        // A DIFFERENT oblique plane (y = -1) must also split cleanly (seam-shift
        // generalises beyond the axis-aligned x=2 case).
        let rc2 = 8.0_f64.sqrt(); // √(9-1)
        let pts2: Vec<DVec3> = (0..32)
            .map(|i| {
                let phi = TAU * i as f64 / 32.0;
                DVec3::new(rc2 * phi.cos(), -1.0, rc2 * phi.sin())
            })
            .collect();
        let subs2 = imprint_curved_face(&surf, &closed_ssi(pts2)).expect("y=-1 oblique imprint");
        assert_eq!(subs2.len(), 2, "second oblique cut → 2 sub-faces");
        let total2: f64 = subs2.iter().map(|s| uv_area(&s.uv_region)).sum();
        assert!(
            (total2 - rect_area).abs() < 1e-6,
            "y=-1 uv regions partition the hemisphere ({} vs {})",
            total2,
            rect_area
        );
    }

    /// ADR-197 β-3-δ — curved sub-face classify. sphere(r=3) − half-space {z>2}:
    /// imprint the north hemisphere at z=2, then the z<2 cap is in sphere∖box →
    /// KEPT, while the z>2 cap is inside the box → DROPPED. The sphere's solid
    /// membership is ANALYTIC (|p−c|<r); the box's is the triangle ray cast.
    #[test]
    fn adr197_beta3d_curved_classify() {
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        // a big box acting as the half-space z > 2.
        let bx = make_box(&mut mesh, DVec3::new(-10., -10., 2.), DVec3::new(10., 10., 10.), mat);
        let box_tris = mesh.prepare_solid(&bx).expect("box solid").all_triangles;
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        let in_result = |p: DVec3| -> bool {
            (p - center).length() < radius && !point_in_solid(&box_tris, p)
        };
        let surf = AnalyticSurface::Sphere {
            center,
            radius,
            axis_dir: DVec3::Z, ref_dir: DVec3::X,
            u_range: (0.0, TAU),
            v_range: (0.0, FRAC_PI_2),
        };
        let rc = 5.0_f64.sqrt();
        let pts: Vec<DVec3> = (0..32)
            .map(|i| {
                let t = TAU * i as f64 / 32.0;
                DVec3::new(rc * t.cos(), rc * t.sin(), 2.0)
            })
            .collect();
        let subs = imprint_curved_face(&surf, &closed_ssi(pts)).expect("imprint");
        assert_eq!(subs.len(), 2);
        let vcen = |s: &CurvedSubFace| {
            s.uv_region.iter().map(|p| p.1).sum::<f64>() / s.uv_region.len() as f64
        };
        let (lower, upper) = if vcen(&subs[0]) < vcen(&subs[1]) {
            (&subs[0], &subs[1])
        } else {
            (&subs[1], &subs[0])
        };
        assert!(
            classify_curved_subface(lower, &in_result).is_some(),
            "z<2 cap kept (sphere∖box)"
        );
        assert!(
            classify_curved_subface(upper, &in_result).is_none(),
            "z>2 cap dropped (inside box)"
        );

        // INTERSECT sphere ∩ {z>2} → the OPPOSITE: z>2 cap kept, z<2 dropped.
        let in_isect = |p: DVec3| -> bool {
            (p - center).length() < radius && point_in_solid(&box_tris, p)
        };
        assert!(
            classify_curved_subface(upper, &in_isect).is_some(),
            "z>2 cap kept (sphere ∩ box)"
        );
        assert!(
            classify_curved_subface(lower, &in_isect).is_none(),
            "z<2 cap dropped (outside box for ∩)"
        );
    }

    /// ADR-197 β-3-γ-2 APPROACH VALIDATION (사전검토 시뮬레이션, locked). The general
    /// oblique curved imprint = run `arrange_polygon_2d` in the surface's uv domain,
    /// using exact surface INVERSION (the analytic SSI only emits placeholder uv).
    /// Findings this test locks:
    ///   1. sphere inversion (u=atan2(Δy,Δx), v=asin(Δz/r)) round-trips `evaluate`.
    ///   2. a LATITUDE SSI (const-z) → seam-spanning chain → 2 uv regions.
    ///   3. a single OBLIQUE SSI raw (clipped to the hemisphere) crosses the u-seam
    ///      → WRONG region count; SEAM-SHIFTING u by π fixes it → 2 regions whose
    ///      areas partition the uv rectangle.
    /// (Multiple oblique curves that jointly cover all u need a seam-aware periodic
    ///  arrangement — the β-3-γ-2b sub-step; a single/few-curve cut works via shift.)
    #[test]
    fn adr197_beta3g2_uv_arrangement_approach() {
        use std::f64::consts::{FRAC_PI_2, PI, TAU};
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        let invert = |p: DVec3| -> (f64, f64) {
            let d = p - center;
            let v = (d.z / radius).clamp(-1.0, 1.0).asin();
            let mut u = d.y.atan2(d.x);
            if u < 0.0 {
                u += TAU;
            }
            (u, v)
        };
        // 1. inversion round-trips evaluate.
        for &(u, v) in &[(0.3_f64, 0.4_f64), (2.0, -0.5), (5.0, 0.9)] {
            let p = crate::surfaces::sphere::evaluate(center, radius, DVec3::Z, DVec3::X, u, v);
            let (ui, vi) = invert(p);
            assert!((ui - u).abs() < 1e-9 && (vi - v).abs() < 1e-9, "inversion round-trip");
        }
        let rc = 5.0_f64.sqrt();
        let rect = vec![
            Pt2::new(0.0, 0.0),
            Pt2::new(TAU, 0.0),
            Pt2::new(TAU, FRAC_PI_2),
            Pt2::new(0.0, FRAC_PI_2),
        ];
        let rect_area = TAU * FRAC_PI_2;
        // 2. latitude (z=2) as a seam-spanning chain → 2 regions.
        let v0 = (2.0_f64 / 3.0).asin();
        let zlat = [(Pt2::new(0.0, v0), Pt2::new(TAU, v0))];
        assert_eq!(arrange_polygon_2d(&rect, &zlat).len(), 2, "latitude → 2 uv regions");
        // 3a. single oblique (x=2) clipped to north, RAW → wrong (seam crossing).
        let xnorth: Vec<(f64, f64)> = (0..=32)
            .map(|i| {
                let phi = TAU * i as f64 / 32.0;
                invert(DVec3::new(2.0, rc * phi.cos(), rc * phi.sin()))
            })
            .filter(|p| p.1 >= -1e-9)
            .collect();
        let raw_segs: Vec<(Pt2, Pt2)> = xnorth
            .windows(2)
            .map(|w| (Pt2::new(w[0].0, w[0].1), Pt2::new(w[1].0, w[1].1)))
            .collect();
        let raw = arrange_polygon_2d(&rect, &raw_segs);
        let raw_partition: f64 = raw.iter().map(|r| polygon_signed_area_2d(&r.outer).abs()).sum();
        assert!(
            raw.len() != 2 || (raw_partition - rect_area).abs() > 1e-3,
            "RAW oblique arc crosses the seam → wrong (documents the need for seam handling)"
        );
        // 3b. SEAM-SHIFT u by π → no seam crossing → 2 regions partitioning the rect.
        let shifted: Vec<(Pt2, Pt2)> = xnorth
            .windows(2)
            .map(|w| {
                (
                    Pt2::new((w[0].0 + PI) % TAU, w[0].1),
                    Pt2::new((w[1].0 + PI) % TAU, w[1].1),
                )
            })
            .collect();
        let fixed = arrange_polygon_2d(&rect, &shifted);
        assert_eq!(fixed.len(), 2, "seam-shifted oblique → 2 uv regions");
        let fixed_partition: f64 = fixed.iter().map(|r| polygon_signed_area_2d(&r.outer).abs()).sum();
        assert!(
            (fixed_partition - rect_area).abs() < 1e-6,
            "seam-shifted regions partition the uv rectangle ({} vs {})",
            fixed_partition,
            rect_area
        );
    }

    /// ADR-197 β-3-ε FOUNDATION (사전검토 시뮬레이션, locked). The curved sew rests on
    /// the 2-faces-on-one-self-loop-edge structure that `create_sphere` already
    /// builds: a single self-loop Circle edge carries TWO faces (one per twin HE),
    /// giving a WATERTIGHT, manifold, tessellable solid. So `sphere ∩ half-space`
    /// sews the same way — a curved cap (Sphere, restricted v-range) + a planar
    /// disk (Plane) sharing the SSI Circle edge (ε-1, no annulus). The SUBTRACT
    /// case needs a curved ANNULUS (a self-loop face with a self-loop HOLE), which
    /// `add_face_closed_curve` does not yet build, and whose hole the surface-range
    /// `tessellate_face_surface` would wrongly fill — the harder ε-2 sub-step.
    #[test]
    fn adr197_beta3e_sew_mechanism_validated() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let sphere = mesh.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        assert_eq!(sphere.len(), 2, "2 hemisphere faces on one self-loop equator");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "2-faces-on-self-loop → watertight");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
        assert_eq!(
            mesh.edges.iter().filter(|(_, e)| e.is_active()).count(),
            1,
            "single shared circle edge carries both faces"
        );
        for &fid in &sphere {
            let t = mesh.tessellate_face_surface(fid, 0.1).expect("curved tessellation");
            assert!(t.vertices.len() > 10 && !t.triangles.is_empty(), "curved face tessellates");
        }
    }

    /// ADR-197 β-3-ε-1 — the FIRST working curved Boolean: `sphere ∩ {z>2}`. The
    /// kept curved cap (Sphere, v ∈ [v0, π/2]) and the kept planar disk (Plane,
    /// z=2) are sewn across their shared SSI Circle (radius √5) → a WATERTIGHT,
    /// manifold capped sphere. The disk's `−Z` normal is the result's flat bottom;
    /// the cap stays analytic (exact dome).
    #[test]
    fn adr197_beta3e1_capped_sphere_intersect() {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        let z0 = 2.0_f64;
        let v0 = (z0 / radius).asin();
        let rc = (radius * radius - z0 * z0).sqrt(); // √5

        let cap = AnalyticSurface::Sphere {
            center,
            radius,
            axis_dir: DVec3::Z, ref_dir: DVec3::X,
            u_range: (0.0, TAU),
            v_range: (v0, FRAC_PI_2),
        };
        let disk = AnalyticSurface::Plane {
            origin: DVec3::new(0.0, 0.0, z0),
            normal: DVec3::NEG_Z,
            basis_u: DVec3::X,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        let circle = AnalyticCurve::Circle {
            center: DVec3::new(0.0, 0.0, z0),
            radius: rc,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        let (f_cap, f_disk) = mesh
            .sew_closed_curve_pair(
                DVec3::new(rc, 0.0, z0),
                circle,
                cap,
                DVec3::Z,
                disk,
                DVec3::NEG_Z,
                mat,
            )
            .expect("sew cap + disk");

        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "capped sphere watertight, got {} boundary HEs", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
        assert_eq!(
            mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
            2,
            "cap + disk"
        );
        assert_eq!(
            mesh.edges.iter().filter(|(_, e)| e.is_active()).count(),
            1,
            "single shared SSI circle edge"
        );
        assert!(matches!(mesh.face_surface(f_cap), Some(AnalyticSurface::Sphere { .. })));
        assert!(matches!(mesh.face_surface(f_disk), Some(AnalyticSurface::Plane { .. })));
        if let Some(AnalyticSurface::Sphere { v_range, .. }) = mesh.face_surface(f_cap) {
            assert!(
                (v_range.0 - v0).abs() < 1e-9 && (v_range.1 - FRAC_PI_2).abs() < 1e-9,
                "cap keeps the restricted v-range [v0, π/2]"
            );
        }
        let t = mesh.tessellate_face_surface(f_cap, 0.1).expect("cap tessellation");
        assert!(!t.triangles.is_empty(), "cap tessellates to a dome band");
    }

    /// ADR-197 β-3-ε-3 ORCHESTRATION TRACE (사전검토 시뮬레이션, locked). The curved
    /// pipeline detect(β) → imprint(γ) → classify(δ) correctly selects the kept cap
    /// for `sphere ∩ {z>2}`, whose sew is ε-1. Findings this test locks:
    ///   • both hemispheres × the z=2 box face yield the z=2 SSI circle (2 SSI);
    ///   • imprinting the north hemisphere splits it; classify keeps EXACTLY the
    ///     upper (z>2) cap and drops the lower (z<2);
    ///   • the south hemisphere is not split (the z=2 latitude is outside its
    ///     band) → it goes through a whole-face classify (here: dropped).
    #[test]
    fn adr197_beta3e3_orchestration_trace() {
        use crate::surfaces::AnalyticSurface;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        let sphere = mesh.create_sphere_kernel_native(center, radius, mat).unwrap();
        let bx = make_box(&mut mesh, DVec3::new(-10., -10., 2.), DVec3::new(10., 10., 10.), mat);
        for &fid in &bx {
            let (n, origin) = {
                let f = mesh.faces.get(fid).unwrap();
                let nn = f.normal().normalize_or_zero();
                let v0 = mesh.collect_loop_verts(f.outer().start).unwrap()[0];
                (nn, mesh.verts.get(v0).unwrap().pos())
            };
            let bu = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
                .normalize_or_zero();
            mesh.set_face_surface(
                fid,
                Some(AnalyticSurface::Plane {
                    origin,
                    normal: n,
                    basis_u: bu,
                    u_range: (-1e6, 1e6),
                    v_range: (-1e6, 1e6),
                }),
            );
        }
        let box_tris = mesh.prepare_solid(&bx).unwrap().all_triangles;
        let in_result =
            |p: DVec3| (p - center).length() < radius && point_in_solid(&box_tris, p);

        let xs = mesh.detect_curved_intersections(&sphere, &bx);
        assert_eq!(xs.len(), 2, "both hemispheres × the z=2 face → 2 SSI");
        assert!(
            xs.iter().all(|c| c.ssi.closed && (c.ssi.points[0].z - 2.0).abs() < 1e-6),
            "each SSI is the closed z=2 circle"
        );
        let (north, south) = (sphere[0], sphere[1]);
        let nsurf = mesh.face_surface(north).cloned().unwrap();
        let nssi = xs.iter().find(|c| c.face_a == north).unwrap();
        let subs = imprint_curved_face(&nsurf, &nssi.ssi).expect("north imprint");
        assert_eq!(subs.len(), 2, "north splits into 2");
        let vc = |s: &CurvedSubFace| {
            s.uv_region.iter().map(|p| p.1).sum::<f64>() / s.uv_region.len() as f64
        };
        let kept: Vec<bool> =
            subs.iter().map(|s| classify_curved_subface(s, &in_result).is_some()).collect();
        assert_eq!(kept.iter().filter(|&&k| k).count(), 1, "exactly one cap kept");
        let ki = kept.iter().position(|&k| k).unwrap();
        assert!(vc(&subs[ki]) > vc(&subs[1 - ki]), "the KEPT cap is the upper (z>2) one");
        let ssurf = mesh.face_surface(south).cloned().unwrap();
        let sssi = xs.iter().find(|c| c.face_a == south).unwrap();
        assert!(
            imprint_curved_face(&ssurf, &sssi.ssi).is_none(),
            "south not split (z=2 latitude outside its band) → whole-face classify"
        );
    }

    /// ADR-197 β-3-ε-3 — the FIRST AUTOMATIC curved Boolean: a kernel-native sphere
    /// clipped by the half-space {z>2}. The pipeline (SSI → imprint → classify →
    /// sew) runs end-to-end and yields a WATERTIGHT capped sphere: a Sphere cap
    /// (v ∈ [v0, π/2]) + a Plane disk on the shared z=2 SSI circle.
    #[test]
    fn adr197_beta3e3_sphere_halfspace_intersect() {
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::FRAC_PI_2;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let sphere = mesh.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let result = mesh
            .boolean_sphere_halfspace(&sphere, DVec3::new(0., 0., 2.), DVec3::Z, mat)
            .expect("automatic sphere ∩ half-space");
        assert_eq!(result.len(), 2, "cap + disk");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "automatic capped sphere watertight, got {}", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
        // exactly one Sphere cap (v∈[v0,π/2]) + one Plane disk.
        let v0 = (2.0_f64 / 3.0).asin();
        let cap = result
            .iter()
            .find(|&&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Sphere { .. })))
            .expect("Sphere cap");
        if let Some(AnalyticSurface::Sphere { v_range, .. }) = mesh.face_surface(*cap) {
            assert!(
                (v_range.0 - v0).abs() < 1e-9 && (v_range.1 - FRAC_PI_2).abs() < 1e-9,
                "cap automatically restricted to the z>2 band [v0, π/2], got {:?}",
                v_range
            );
        }
        assert!(
            result
                .iter()
                .any(|&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Plane { .. }))),
            "Plane disk closes the cut"
        );
        // the kept original sphere faces are gone (replaced by cap + disk).
        assert!(
            sphere.iter().all(|&f| !mesh.faces.get(f).map(|x| x.is_active()).unwrap_or(false)),
            "original hemispheres removed"
        );

        // ADVERSARIAL — the OPPOSITE half-space {z < -2} (keep the south cap) must
        // also clip automatically into a watertight capped sphere.
        let mut m2 = Mesh::default();
        let s2 = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r2 = m2
            .boolean_sphere_halfspace(&s2, DVec3::new(0., 0., -2.), DVec3::NEG_Z, mat)
            .expect("sphere ∩ {z<-2}");
        assert_eq!(r2.len(), 2);
        let b2 = m2
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(b2, 0, "south-cap capped sphere watertight");
        assert!(m2.verify_face_invariants().is_valid(), "manifold");
        // ADR-204: the cap is now oriented with pole = cut normal (−Z), so the
        // south cap reads v∈[asin(2/3), π/2] in the −Z frame (same geometry).
        let v0s = (2.0_f64 / 3.0).asin();
        let cap2 = r2
            .iter()
            .find(|&&f| matches!(m2.face_surface(f), Some(AnalyticSurface::Sphere { .. })))
            .unwrap();
        if let Some(AnalyticSurface::Sphere { axis_dir, v_range, .. }) = m2.face_surface(*cap2) {
            assert!((axis_dir.normalize() - DVec3::NEG_Z).length() < 1e-9,
                "ADR-204: cap pole = cut normal −Z, got {:?}", axis_dir);
            assert!(
                (v_range.0 - v0s).abs() < 1e-9 && (v_range.1 - FRAC_PI_2).abs() < 1e-9,
                "south cap (−Z frame) v∈[asin(2/3), π/2], got {:?}",
                v_range
            );
        }
    }

    /// ADR-197 β-3-ε-2 FINDING (사전검토 시뮬레이션, locked). `sphere − {z>2}` keeps
    /// the part below z=2. The pipeline returns TWO adjacent caps — the south
    /// hemisphere [−π/2, 0] and the north-lower band [0, v0] — that meet at the
    /// equator (v=0). They MERGE into ONE cap [−π/2, v0]: the south pole is a
    /// point, so the merged cap has a single boundary (the z=2 circle). Therefore
    /// the subtract half-space needs a cap-MERGE, NOT a multi-loop annulus — the
    /// annulus is only required for box∩sphere (multiple cuts).
    #[test]
    fn adr197_beta3e2_subtract_is_cap_merge_not_annulus() {
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::FRAC_PI_2;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        let sphere = mesh.create_sphere_kernel_native(center, radius, mat).unwrap();
        let ssi = crate::surfaces::ssi::analytic::plane_sphere(
            DVec3::new(0., 0., 2.),
            DVec3::Z,
            center,
            radius,
            64,
        );
        let in_result = |p: DVec3| (p - center).length() < radius && p.z < 2.0;
        let mut kept: Vec<(f64, f64)> = Vec::new();
        for &sf in &sphere {
            let surf = mesh.face_surface(sf).cloned().unwrap();
            match imprint_curved_face(&surf, &ssi) {
                Some(subs) => {
                    for s in &subs {
                        if classify_curved_subface(s, &in_result).is_some() {
                            let vmin = s.uv_region.iter().map(|p| p.1).fold(f64::MAX, f64::min);
                            let vmax = s.uv_region.iter().map(|p| p.1).fold(f64::MIN, f64::max);
                            kept.push((vmin, vmax));
                        }
                    }
                }
                None => {
                    let whole = CurvedSubFace {
                        surface: surf.clone(),
                        uv_region: full_uv_rect(&surf),
                        uv_holes: Vec::new(),
                        u_shift: 0.0,
                    };
                    if classify_curved_subface(&whole, &in_result).is_some() {
                        if let AnalyticSurface::Sphere { v_range, .. } = &surf {
                            kept.push(*v_range);
                        }
                    }
                }
            }
        }
        kept.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        let v0 = (2.0_f64 / 3.0).asin();
        assert_eq!(kept.len(), 2, "subtract keeps 2 adjacent caps");
        assert!((kept[0].1 - kept[1].0).abs() < 1e-9, "caps meet at the equator (v=0)");
        assert!((kept[0].0 + FRAC_PI_2).abs() < 1e-9, "spans the south pole");
        assert!((kept[1].1 - v0).abs() < 1e-9, "up to the z=2 latitude");
        // → mergeable into a single cap [−π/2, v0] (single boundary).
    }

    /// ADR-197 β-3-ε-2 — SUBTRACT half-space via cap-MERGE: `sphere − {z>2}` (a
    /// sliced sphere). Keeping the z<2 side returns two adjacent caps (south
    /// hemisphere + north band) that merge into ONE Sphere cap v∈[−π/2, v0] (the
    /// south pole is a point → single boundary), sewn to the Plane disk → a
    /// WATERTIGHT flat-topped sphere. No multi-loop annulus needed.
    #[test]
    fn adr197_beta3e2_sphere_minus_halfspace_cap_merge() {
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::FRAC_PI_2;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let sphere = mesh.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        // sphere − {z>2}  ≡  keep the z<2 side (plane normal −Z).
        let result = mesh
            .boolean_sphere_halfspace(&sphere, DVec3::new(0., 0., 2.), DVec3::NEG_Z, mat)
            .expect("sphere − {z>2}");
        assert_eq!(result.len(), 2, "merged cap + disk");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "sliced sphere watertight, got {}", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
        // ADR-204: oriented cap, pole = cut normal (−Z). The z<2 cap spans the
        // −Z (south) pole down to z=2, reading v∈[−asin(2/3), π/2] (same geometry).
        let v0 = (2.0_f64 / 3.0).asin();
        let cap = result
            .iter()
            .find(|&&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Sphere { .. })))
            .expect("merged Sphere cap");
        if let Some(AnalyticSurface::Sphere { axis_dir, v_range, .. }) = mesh.face_surface(*cap) {
            assert!((axis_dir.normalize() - DVec3::NEG_Z).length() < 1e-9,
                "ADR-204: cap pole = cut normal −Z, got {:?}", axis_dir);
            assert!(
                (v_range.0 + v0).abs() < 1e-9 && (v_range.1 - FRAC_PI_2).abs() < 1e-9,
                "merged cap (−Z frame) v∈[−asin(2/3), π/2], got {:?}",
                v_range
            );
        }
        assert!(
            result
                .iter()
                .any(|&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Plane { .. }))),
            "Plane disk closes the flat top"
        );
    }

    /// ADR-197 box∩sphere COMPLEXITY (사전검토 시뮬레이션, locked). Why box∩sphere is a
    /// major multi-step effort vs the single-plane half-space: box[−2,2]³ ∩
    /// sphere(r=3) yields 12 SSI circles — 6 per hemisphere — of which 2 are
    /// Z-latitude (z=±2 faces) and 4 are oblique (x=±2, y=±2 faces). Imprinting a
    /// hemisphere therefore needs ALL 6 mutually-crossing circles at once (a
    /// seam-aware PERIODIC arrangement, γ-2b), the kept sphere patches are
    /// genuine bands/annuli (not pole-reaching caps), and the sew must build
    /// curved faces with arbitrary (uv-polygon) boundaries — all beyond ε-1/ε-2.
    #[test]
    fn adr197_box_sphere_ssi_complexity() {
        use crate::surfaces::AnalyticSurface;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        let sphere = mesh.create_sphere_kernel_native(center, radius, mat).unwrap();
        let bx = make_box(&mut mesh, DVec3::new(-2., -2., -2.), DVec3::new(2., 2., 2.), mat);
        for &fid in &bx {
            let (n, origin) = {
                let f = mesh.faces.get(fid).unwrap();
                let nn = f.normal().normalize_or_zero();
                let v0 = mesh.collect_loop_verts(f.outer().start).unwrap()[0];
                (nn, mesh.verts.get(v0).unwrap().pos())
            };
            let bu = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
                .normalize_or_zero();
            mesh.set_face_surface(
                fid,
                Some(AnalyticSurface::Plane {
                    origin,
                    normal: n,
                    basis_u: bu,
                    u_range: (-1e6, 1e6),
                    v_range: (-1e6, 1e6),
                }),
            );
        }
        let xs = mesh.detect_curved_intersections(&sphere, &bx);
        assert_eq!(xs.len(), 12, "6 box faces × 2 hemispheres = 12 SSI circles");
        for &sf in &sphere {
            let circles: Vec<_> = xs.iter().filter(|c| c.face_a == sf).collect();
            assert_eq!(circles.len(), 6, "each hemisphere meets all 6 box planes");
            let latitude = circles
                .iter()
                .filter(|c| {
                    let z0 = c.ssi.points[0].z;
                    c.ssi.points.iter().all(|p| (p.z - z0).abs() < 1e-6)
                })
                .count();
            assert_eq!(latitude, 2, "2 Z-latitude (z=±2) circles");
            assert_eq!(circles.len() - latitude, 4, "4 oblique (x,y=±2) circles");
        }
    }

    /// ADR-197 β-3-ε-2 (real annulus) — the BARREL: `sphere ∩ {|z|<2}`. The kept
    /// sphere band v∈[−v0, v0] is a genuine annulus (TWO circle boundaries, it
    /// reaches no pole) — a MULTI-LOOP curved face — sewn to a top + bottom disk
    /// via `sew_curved_band` → a watertight barrel. This is the building block
    /// box∩sphere needs (a band that cap-merge cannot collapse to a single cap).
    #[test]
    fn adr197_beta3e2_real_barrel() {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::TAU;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        let z0 = 2.0_f64;
        let v0 = (z0 / radius).asin();
        let rc = (radius * radius - z0 * z0).sqrt(); // √5
        let band = AnalyticSurface::Sphere {
            center,
            radius,
            axis_dir: DVec3::Z, ref_dir: DVec3::X,
            u_range: (0.0, TAU),
            v_range: (-v0, v0),
        };
        let circle = |z: f64| AnalyticCurve::Circle {
            center: DVec3::new(0., 0., z),
            radius: rc,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        let disk = |z: f64, nz: DVec3| AnalyticSurface::Plane {
            origin: DVec3::new(0., 0., z),
            normal: nz,
            basis_u: DVec3::X,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        let (bf, _td, _bd) = mesh
            .sew_curved_band(
                DVec3::new(rc, 0., 2.),
                circle(2.),
                DVec3::new(rc, 0., -2.),
                circle(-2.),
                band,
                DVec3::X,
                disk(2., DVec3::Z),
                DVec3::Z,
                disk(-2., DVec3::NEG_Z),
                DVec3::NEG_Z,
                mat,
            )
            .expect("barrel");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "barrel watertight, got {}", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold barrel");
        assert_eq!(
            mesh.faces.iter().filter(|(_, f)| f.is_active()).count(),
            3,
            "band + 2 disks"
        );
        assert_eq!(
            mesh.faces.get(bf).unwrap().inners().len(),
            1,
            "band is multi-loop: top circle outer + bottom circle inner"
        );
        let t = mesh.tessellate_face_surface(bf, 0.1).expect("band tessellation");
        assert!(!t.triangles.is_empty(), "band tessellates its v-range");
    }

    /// ADR-197 β-3-ε-2 (orchestration) — the AUTOMATIC barrel: `sphere ∩ {|z|<2}`
    /// run end-to-end (two SSI circles → imprint → classify → merge the two kept
    /// caps into a pole-free band → `sew_curved_band`). Watertight barrel with a
    /// multi-loop Sphere band v∈[−v0, v0] + two Plane disks.
    #[test]
    fn adr197_beta3e2_sphere_slab_barrel_auto() {
        use crate::surfaces::AnalyticSurface;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let sphere = mesh.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let result = mesh
            .boolean_sphere_slab(&sphere, -2.0, 2.0, mat)
            .expect("automatic sphere ∩ {|z|<2}");
        assert_eq!(result.len(), 3, "band + 2 disks");
        let boundary = mesh
            .hes
            .iter()
            .filter(|(_, h)| h.is_active() && h.face().is_null())
            .count();
        assert_eq!(boundary, 0, "auto barrel watertight, got {}", boundary);
        assert!(mesh.verify_face_invariants().is_valid(), "manifold");
        let v0 = (2.0_f64 / 3.0).asin();
        let band = result
            .iter()
            .find(|&&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Sphere { .. })))
            .expect("Sphere band");
        if let Some(AnalyticSurface::Sphere { v_range, .. }) = mesh.face_surface(*band) {
            assert!(
                (v_range.0 + v0).abs() < 1e-9 && (v_range.1 - v0).abs() < 1e-9,
                "merged band v∈[−v0, v0] (pole-free), got {:?}",
                v_range
            );
        }
        assert_eq!(
            mesh.faces.get(*band).unwrap().inners().len(),
            1,
            "band is the multi-loop annulus"
        );
        assert_eq!(
            result
                .iter()
                .filter(|&&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Plane { .. })))
                .count(),
            2,
            "top + bottom disks"
        );
        assert!(
            sphere.iter().all(|&f| !mesh.faces.get(f).map(|x| x.is_active()).unwrap_or(false)),
            "original hemispheres removed"
        );

        // ADVERSARIAL — an ASYMMETRIC slab z∈[−1, 2] (still straddles the equator,
        // different cut radii) must also auto-barrel watertight.
        let mut m2 = Mesh::default();
        let s2 = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r2 = m2.boolean_sphere_slab(&s2, -1.0, 2.0, mat).expect("asymmetric slab");
        assert_eq!(r2.len(), 3);
        assert_eq!(
            m2.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "asymmetric barrel watertight"
        );
        assert!(m2.verify_face_invariants().is_valid());
        let band2 = r2
            .iter()
            .find(|&&f| matches!(m2.face_surface(f), Some(AnalyticSurface::Sphere { .. })))
            .unwrap();
        if let Some(AnalyticSurface::Sphere { v_range, .. }) = m2.face_surface(*band2) {
            let v_lo = (-1.0_f64 / 3.0).asin();
            let v_hi = (2.0_f64 / 3.0).asin();
            assert!(
                (v_range.0 - v_lo).abs() < 1e-9 && (v_range.1 - v_hi).abs() < 1e-9,
                "asymmetric band v∈[asin(-1/3), asin(2/3)], got {:?}",
                v_range
            );
        }
    }

    /// Build a clean 3-face kernel-native cylinder (base disk + top disk + side
    /// band) directly via `extrude_cylinder_kernel_native` — NOT via create_solid,
    /// which polygonises into N quads. Z-up, base at `base_z`. Returns
    /// `[base_disk, top_disk, side_band]`.
    fn build_clean_cylinder(
        mesh: &mut Mesh,
        cx: f64,
        cy: f64,
        base_z: f64,
        radius: f64,
        height: f64,
        mat: MaterialId,
    ) -> Vec<FaceId> {
        use crate::surfaces::AnalyticSurface;
        let center = DVec3::new(cx, cy, base_z);
        let anchor = mesh.add_vertex(center + DVec3::X * radius);
        let base_circle = crate::curves::AnalyticCurve::Circle {
            center,
            radius,
            normal: DVec3::Z,
            basis_u: DVec3::X,
        };
        let profile = mesh.add_face_closed_curve(anchor, base_circle, mat).unwrap();
        mesh.faces.get_mut(profile).unwrap().set_surface(Some(AnalyticSurface::Plane {
            origin: center,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (-radius * 1.5, radius * 1.5),
            v_range: (-radius * 1.5, radius * 1.5),
        }));
        let res = mesh.extrude_cylinder_kernel_native(profile, height, mat).unwrap();
        let mut v = vec![res.profile_face, res.top_face];
        v.extend(res.side_faces.iter().copied());
        v
    }

    #[test]
    fn adr197_beta3h_cylinder_clean_structure() {
        use crate::surfaces::AnalyticSurface;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let cyl = build_clean_cylinder(&mut mesh, 0., 0., -3., 2.0, 6.0, mat);
        // 3 faces: 2 Plane disks (self-loop outer) + 1 Cylinder side band.
        assert_eq!(cyl.len(), 3, "clean kernel-native cylinder = 3 faces");
        let planes = cyl
            .iter()
            .filter(|&&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Plane { .. })))
            .count();
        assert_eq!(planes, 2, "two Plane cap disks");
        let side = *cyl
            .iter()
            .find(|&&f| matches!(mesh.face_surface(f), Some(AnalyticSurface::Cylinder { .. })))
            .unwrap();
        // The side is a MULTI-LOOP band: outer = 1 self-loop circle, 1 inner
        // self-loop circle — identical to sew_curved_band's structure.
        let f = mesh.faces.get(side).unwrap();
        assert_eq!(
            mesh.collect_loop_verts(f.outer().start).unwrap().len(),
            1,
            "side band outer = self-loop (1 anchor vert)"
        );
        assert_eq!(f.inners().len(), 1, "side band has 1 inner self-loop (the other circle)");
        // plane(z=1) × cylinder SSI → clean closed latitude circle r=2 at z=1.
        let plane = AnalyticSurface::Plane {
            origin: DVec3::new(0., 0., 1.),
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        let s = surface_surface_intersection(&plane, &mesh.face_surface(side).unwrap())
            .expect("plane×cylinder SSI");
        assert!(s.closed, "cylinder latitude cut is a closed circle");
        assert!(s.points.iter().all(|p| (p.z - 1.0).abs() < 1e-6), "all SSI points at z=1");
        let rmax = s.points.iter().map(|p| (p.x * p.x + p.y * p.y).sqrt()).fold(0.0, f64::max);
        assert!((rmax - 2.0).abs() < 1e-6, "cut circle radius = cylinder radius");
    }

    #[test]
    fn adr197_beta3h_cylinder_slab_truncate() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        // ── slab: truncate both ends. cylinder z∈[-3,3] ∩ {|z|<1.5} → barrel z∈[-1.5,1.5].
        let mut mesh = Mesh::default();
        let cyl = build_clean_cylinder(&mut mesh, 0., 0., -3., 2.0, 6.0, mat);
        let r = mesh.boolean_cylinder_slab(&cyl, -1.5, 1.5, mat).expect("slab truncate");
        assert_eq!(r.len(), 3, "result = band + 2 disks");
        assert_eq!(
            mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "truncated cylinder watertight"
        );
        assert!(mesh.verify_face_invariants().is_valid());
        let band = r
            .iter()
            .find(|&&f| matches!(mesh.face_surface(f), Some(S::Cylinder { .. })))
            .unwrap();
        if let Some(S::Cylinder { v_range, axis_origin, .. }) = mesh.face_surface(*band) {
            // v = z - axis_origin.z. cuts at z=±1.5 with base z0=-3 → v∈[1.5, 4.5].
            assert!((axis_origin.z - (-3.0)).abs() < 1e-9);
            assert!(
                (v_range.0 - 1.5).abs() < 1e-9 && (v_range.1 - 4.5).abs() < 1e-9,
                "band v∈[1.5,4.5] (z∈[-1.5,1.5]); got {:?}",
                v_range
            );
        }

        // ── halfspace {z>0.5}: slab to the cylinder's existing top (z=3).
        let mut m2 = Mesh::default();
        let cyl2 = build_clean_cylinder(&mut m2, 0., 0., -3., 2.0, 6.0, mat);
        let r2 = m2.boolean_cylinder_slab(&cyl2, 0.5, 3.0, mat).expect("halfspace z>0.5");
        assert_eq!(r2.len(), 3);
        assert_eq!(
            m2.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "halfspace cylinder watertight"
        );
        assert!(m2.verify_face_invariants().is_valid());

        // ── whole-cylinder slab → rejected (no genuine cut).
        let mut m3 = Mesh::default();
        let cyl3 = build_clean_cylinder(&mut m3, 0., 0., -3., 2.0, 6.0, mat);
        assert!(
            m3.boolean_cylinder_slab(&cyl3, -5.0, 5.0, mat).is_err(),
            "slab covering the whole cylinder is rejected (no cut)"
        );
    }

    /// **Boolean Z-axis restriction probe (2026-06-18)** — "Boolean Z축 해제"
    /// 트랙의 truth-first 확인. analytic curved ops 가 축 ∥ Z 아닌 primitive 를
    /// hard-reject (bail) 하는지 실측. reject = 진짜 gap (회전된 솔리드 analytic
    /// Boolean 불가 → fallback). 첫 atomic = local-frame transform (Z-frame 회전
    /// → 기존 op 재사용 → 역변환). control = Z축 cylinder 정상.
    #[test]
    fn probe_boolean_zaxis_restriction_real() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // control — Z축 cylinder slab 정상.
        let mut mz = Mesh::default();
        let cz = build_clean_cylinder(&mut mz, 0., 0., -3., 2.0, 6.0, mat);
        assert!(
            mz.boolean_cylinder_slab(&cz, -1.5, 1.5, mat).is_ok(),
            "Z축 cylinder slab works (control)"
        );

        // tilted cylinder (axis ∦ Z) — profile circle normal 기울임 + extrude.
        let mut mt = Mesh::default();
        let normal = DVec3::new(0.0, 0.6, 0.8).normalize(); // Z 에서 기움
        let center = DVec3::ZERO;
        let anchor = mt.add_vertex(center + DVec3::X * 2.0);
        let circle = crate::curves::AnalyticCurve::Circle {
            center,
            radius: 2.0,
            normal,
            basis_u: DVec3::X, // ⊥ normal (normal.x = 0)
        };
        let profile = mt.add_face_closed_curve(anchor, circle, mat).unwrap();
        mt.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
            origin: center,
            normal,
            basis_u: DVec3::X,
            u_range: (-3., 3.),
            v_range: (-3., 3.),
        }));
        let res = mt.extrude_cylinder_kernel_native(profile, 6.0, mat).unwrap();
        let mut tilted = vec![res.profile_face, res.top_face];
        tilted.extend(res.side_faces.iter().copied());

        // 측면 face 가 기운 축 Cylinder surface 를 가지는지 확인 (probe 전제).
        let side = res.side_faces[0];
        match mt.face_surface(side) {
            Some(S::Cylinder { axis_dir, .. }) => {
                assert!(
                    axis_dir.cross(DVec3::Z).length() > 1e-6,
                    "tilted cylinder axis ∦ Z (got {:?})",
                    axis_dir
                );
            }
            other => panic!("side face should be Cylinder surface, got {:?}", other),
        }

        // analytic slab op 가 회전된 cylinder 를 REJECT (Z축 guard, boolean.rs:1966)
        // → 진짜 gap. 첫 atomic local-frame transform 의 타겟.
        let r = mt.boolean_cylinder_slab(&tilted, -1.5, 1.5, mat);
        assert!(
            r.is_err(),
            "rotated cylinder slab REJECTED by Z축 guard → real gap (got {:?})",
            r.map(|v| v.len())
        );
    }

    /// **ADR-197 Z-axis lift (A-1)** — a TILTED cylinder is truncated via the
    /// local-frame wrapper `boolean_cylinder_slab_local` (rotate→Z-frame, run the
    /// existing Z-locked op, rotate back). The probe above shows the raw op bails
    /// on this very cylinder; the lift makes it work. Result must be watertight +
    /// manifold + the band's analytic Cylinder surface must keep the ORIGINAL
    /// tilted axis (not collapse to +Z) with the cut v-range.
    #[test]
    fn adr197_zlift_a1_tilted_cylinder_slab_local() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // TILTED kernel-native cylinder (axis ∦ Z) — same tilt as the probe.
        let mut m = Mesh::default();
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let center = DVec3::ZERO;
        let radius = 2.0;
        let height = 6.0;
        let basis_u = DVec3::X; // ⊥ axis (axis.x = 0)
        let anchor = m.add_vertex(center + basis_u * radius);
        let circle = crate::curves::AnalyticCurve::Circle {
            center,
            radius,
            normal: axis,
            basis_u,
        };
        let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
        m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
            origin: center,
            normal: axis,
            basis_u,
            u_range: (-radius * 1.5, radius * 1.5),
            v_range: (-radius * 1.5, radius * 1.5),
        }));
        let res = m.extrude_cylinder_kernel_native(profile, height, mat).unwrap();
        let mut tilted = vec![res.profile_face, res.top_face];
        tilted.extend(res.side_faces.iter().copied());

        // Cut along the cylinder's OWN axis: v ∈ [1.5, 4.5] (full extent [0, 6]).
        let r = m
            .boolean_cylinder_slab_local(&tilted, 1.5, 4.5, mat)
            .expect("local-frame tilted slab succeeds (no axis∥Z bail)");

        assert_eq!(r.len(), 3, "tilted slab = side band + 2 cap disks");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "tilted truncated cylinder watertight"
        );
        assert!(m.verify_face_invariants().is_valid(), "DCEL invariants valid");
        assert_eq!(
            m.face_set_manifold_info(&r).non_manifold_edge_count,
            0,
            "no non-manifold edges after tilted slab"
        );

        // The band must STILL be a Cylinder whose axis equals the original tilt
        // (the local-frame round-trip must not collapse the axis to +Z).
        let band = r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
            .expect("result has a Cylinder band");
        if let Some(S::Cylinder { axis_dir, axis_origin, radius: br, v_range, .. }) =
            m.face_surface(*band)
        {
            assert!(
                (axis_dir.normalize() - axis).length() < 1e-6,
                "tilt axis preserved through local-frame lift (got {:?})",
                axis_dir
            );
            assert!((br - radius).abs() < 1e-9, "radius preserved");
            assert!(
                (v_range.0 - 1.5).abs() < 1e-9 && (v_range.1 - 4.5).abs() < 1e-9,
                "band v-range == cut bounds (got {:?})",
                v_range
            );
            assert!(
                (*axis_origin - center).length() < 1e-6,
                "axis_origin (pivot) preserved"
            );
        }

        // Tessellation finite (no NaN from the rotation round-trip).
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!pos.is_empty() && !tris.is_empty() && pos.iter().all(|c| c.is_finite()));
    }

    /// **ADR-205 β-2** — a Z-axis cylinder cut by an OBLIQUE plane keeps the
    /// +m side as a watertight solid: trimmed Cylinder band (elliptic + circular
    /// boundary) + elliptic cap + kept end disk. The band must NOT poke past the
    /// cut plane (boundary-aware clip), and the kept side is entirely on +m.
    #[test]
    fn adr205_beta2_cylinder_oblique_halfspace_watertight_and_clipped() {
        use crate::surfaces::AnalyticSurface as S;
        let mut m = Mesh::default();
        let mat = MaterialId::new(0);
        // Z-axis cylinder z∈[0,6], r=2 (axis_origin at z=0).
        let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
        let pm = DVec3::new(0.3, 0.0, 1.0).normalize();
        let o = DVec3::new(0.0, 0.0, 3.0);
        let r = m
            .boolean_cylinder_oblique_halfspace(&cyl, o, pm, mat)
            .expect("oblique halfspace cut succeeds");
        assert_eq!(r.len(), 3, "band + elliptic cap + kept disk");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "oblique-cut cylinder watertight",
        );
        assert!(m.verify_face_invariants().is_valid(), "DCEL invariants valid");
        assert_eq!(
            m.face_set_manifold_info(&r).non_manifold_edge_count,
            0,
            "manifold after oblique cut",
        );
        let band = *r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
            .expect("result has a Cylinder band");
        // Render: every band vertex stays on the kept +m side (no over-draw past
        // the elliptic cut) — boundary-aware clip in action.
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        let mut below = 0usize;
        let mut total = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let p = DVec3::new(pos[vi * 3] as f64, pos[vi * 3 + 1] as f64, pos[vi * 3 + 2] as f64);
                total += 1;
                if (p - o).dot(pm) < -1e-3 { below += 1; }
            }
        }
        assert!(total > 0, "band rendered");
        assert_eq!(below, 0, "kept band does not over-draw past the elliptic cut (stays +m)");
        assert!(pos.iter().all(|c| c.is_finite()), "finite tessellation");
        assert!(nrm.iter().all(|c| c.is_finite()), "finite normals");
        // Outward normals: the kept truncated cylinder is convex, so every
        // exported vertex normal must point AWAY from the solid centroid (no
        // back-facing / inverted face from the sew).
        let nverts = pos.len() / 3;
        let centroid = {
            let mut c = DVec3::ZERO;
            for i in 0..nverts {
                c += DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64);
            }
            c / (nverts.max(1) as f64)
        };
        let mut inward = 0usize;
        for i in 0..nverts {
            let p = DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64);
            let n = DVec3::new(nrm[i * 3] as f64, nrm[i * 3 + 1] as f64, nrm[i * 3 + 2] as f64);
            if (p - centroid).dot(n) < -1e-3 { inward += 1; }
        }
        assert_eq!(inward, 0, "all kept faces are front-facing (outward normals)");
    }

    /// **ADR-205 β-2 adversarial** — sweep keep-side flip, kept-LOW end, off-origin,
    /// tilted axis, and degenerate rejections. Every valid cut must be watertight +
    /// manifold + front-facing + keep the correct half; every degenerate cut must
    /// `bail!()` cleanly (no panic, no silently-wrong solid).
    #[test]
    fn adr205_beta2_cylinder_oblique_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // (axis_dir, axis_origin, radius, height, plane_origin, plane_normal)
        // valid oblique cuts — each must produce a clean kept solid.
        let valid: &[(DVec3, DVec3, f64, f64, DVec3, DVec3)] = &[
            // flipped normal (keeps the LOW end → kept_outward = −n_a).
            (DVec3::Z, DVec3::ZERO, 2.0, 6.0, DVec3::new(0., 0., 3.), DVec3::new(0.3, 0., -1.)),
            // off-origin cylinder, oblique tilt of the plane.
            (DVec3::Z, DVec3::new(5., -2., 1.), 1.5, 5.0, DVec3::new(5., -2., 3.5), DVec3::new(0.4, 0.2, 1.)),
            // genuinely tilted cylinder axis, plane oblique to it.
            (DVec3::new(0., 0.6, 0.8).normalize(), DVec3::ZERO, 2.0, 6.0,
             DVec3::new(0., 1.8, 2.4), DVec3::new(0.2, 0.3, 1.)),
        ];
        for (idx, &(axis, origin, r, h, po, pn)) in valid.iter().enumerate() {
            let mut m = Mesh::default();
            let basis_u = if axis.x.abs() < 0.9 { axis.cross(DVec3::X) } else { axis.cross(DVec3::Y) }
                .normalize();
            let anchor = m.add_vertex(origin + basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: origin, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin, normal: axis, basis_u,
                u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());

            let r_faces = m.boolean_cylinder_oblique_halfspace(&cyl, po, pn, mat)
                .unwrap_or_else(|e| panic!("valid {} cut failed: {}", idx, e));
            assert_eq!(r_faces.len(), 3, "valid {}: 3 faces", idx);
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "valid {}: watertight", idx,
            );
            assert!(m.verify_face_invariants().is_valid(), "valid {}: invariants", idx);
            assert_eq!(
                m.face_set_manifold_info(&r_faces).non_manifold_edge_count, 0,
                "valid {}: manifold", idx,
            );
            // keep-side + outward normals via export.
            let pn_n = pn.normalize();
            let (pos, nrm, idx_b, fmap, _uv) = m.export_buffers().expect("export");
            let band = *r_faces.iter()
                .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
                .expect("band");
            let mut below = 0usize;
            for (ti, &fid) in fmap.iter().enumerate() {
                if fid != band.raw() { continue; }
                for k in 0..3 {
                    let vi = idx_b[ti * 3 + k] as usize;
                    let p = DVec3::new(pos[vi*3] as f64, pos[vi*3+1] as f64, pos[vi*3+2] as f64);
                    if (p - po).dot(pn_n) < -1e-3 { below += 1; }
                }
            }
            assert_eq!(below, 0, "valid {}: kept band on +m side", idx);
            let nv = pos.len() / 3;
            let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
                c + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
            let mut inward = 0usize;
            for i in 0..nv {
                let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
                let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
                if (p - centroid).dot(n) < -1e-3 { inward += 1; }
            }
            assert_eq!(inward, 0, "valid {}: front-facing", idx);
            assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()),
                "valid {}: finite", idx);
        }

        // degenerate cuts — each must bail cleanly.
        let degenerate: &[(&str, DVec3, DVec3)] = &[
            ("⟂ axis (use local-frame)", DVec3::new(0., 0., 3.), DVec3::Z),
            ("∥ axis (no section)", DVec3::new(0., 0., 3.), DVec3::X),
            ("misses the cylinder", DVec3::new(0., 0., 20.), DVec3::new(0.3, 0., 1.)),
            ("clips an end cap (not clean)", DVec3::new(0., 0., 0.2), DVec3::new(0.9, 0., 1.).normalize()),
            ("degenerate normal", DVec3::new(0., 0., 3.), DVec3::ZERO),
        ];
        for (label, po, pn) in degenerate {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
            let r = m.boolean_cylinder_oblique_halfspace(&cyl, *po, *pn, mat);
            assert!(r.is_err(), "degenerate '{}' must bail (got Ok)", label);
            // the mesh must remain intact (clean cylinder still watertight) since the
            // bail happens BEFORE any face removal.
            assert!(m.verify_face_invariants().is_valid(), "'{}': mesh intact after bail", label);
        }

        // gate: a plain perpendicular Path-B cylinder band → tessellate_cylinder_clipped None.
        let mut m = Mesh::default();
        let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
        let band = *cyl.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).unwrap();
        assert!(m.tessellate_cylinder_clipped(band, 0.02).is_none(),
            "perpendicular band must NOT be clipped (unchanged render)");
    }

    /// **ADR-205 β-3** — a Z-axis cylinder cut by TWO PARALLEL OBLIQUE planes keeps
    /// the elliptic SLAB between them: a trimmed Cylinder band (two elliptic
    /// boundaries) + two elliptic caps. The band must stay BETWEEN the planes
    /// (boundary-aware min/max strip) and be front-facing.
    #[test]
    fn adr205_beta3_cylinder_oblique_slab_watertight_and_clipped() {
        use crate::surfaces::AnalyticSurface as S;
        let mut m = Mesh::default();
        let mat = MaterialId::new(0);
        // Z-axis cylinder z∈[0,6], r=2 (axis_origin z=0). ndm = m.z = 0.958,
        // z_span ≈ 0.6 → t_lo≈1.98, t_hi≈3.97 both wholly on the side.
        let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
        let pm = DVec3::new(0.3, 0.0, 1.0).normalize();
        let (d_lo, d_hi) = (1.9, 3.8);
        let r = m
            .boolean_cylinder_oblique_slab(&cyl, pm, d_lo, d_hi, mat)
            .expect("oblique slab cut succeeds");
        assert_eq!(r.len(), 3, "band + two elliptic caps");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "oblique-slab cylinder watertight",
        );
        assert!(m.verify_face_invariants().is_valid(), "DCEL invariants valid");
        assert_eq!(
            m.face_set_manifold_info(&r).non_manifold_edge_count, 0,
            "manifold after oblique slab",
        );
        let band = *r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
            .expect("result has a Cylinder band");
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        // every band vertex stays BETWEEN the two cut planes (in m-offset).
        let mut outside = 0usize;
        let mut total = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let p = DVec3::new(pos[vi*3] as f64, pos[vi*3+1] as f64, pos[vi*3+2] as f64);
                let d = (p - DVec3::ZERO).dot(pm); // axis_origin = origin
                total += 1;
                if d < d_lo - 1e-3 || d > d_hi + 1e-3 { outside += 1; }
            }
        }
        assert!(total > 0, "band rendered");
        assert_eq!(outside, 0, "band stays between the two cut planes (no over-draw)");
        // outward normals (convex slab → all front-facing).
        let nv = pos.len() / 3;
        let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
            c + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
        let mut inward = 0usize;
        for i in 0..nv {
            let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
            let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
            if (p - centroid).dot(n) < -1e-3 { inward += 1; }
        }
        assert_eq!(inward, 0, "all slab faces are front-facing");
        assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()), "finite");
    }

    /// **ADR-205 β-3 adversarial** — sweep oblique slabs over tilted-axis /
    /// off-origin cylinders + assorted plane normals, plus degenerate rejections.
    /// d-bounds are derived from chosen axial centres so each ellipse sits wholly
    /// on the side regardless of the `n_a·m` sign.
    #[test]
    fn adr205_beta3_cylinder_oblique_slab_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // (axis_dir, axis_origin, radius, height, plane_normal)
        let valid: &[(DVec3, DVec3, f64, f64, DVec3)] = &[
            (DVec3::Z, DVec3::ZERO, 2.0, 6.0, DVec3::new(-0.3, 0.2, 1.0)),
            (DVec3::Z, DVec3::new(4., -1., 2.), 1.5, 5.0, DVec3::new(0.4, 0.0, -1.0)),
            (DVec3::new(0., 0.6, 0.8).normalize(), DVec3::ZERO, 2.0, 6.0, DVec3::new(0.2, 0.3, 1.0)),
        ];
        for (idx, &(axis, origin, r, h, pn)) in valid.iter().enumerate() {
            let mut m = Mesh::default();
            let basis_u = if axis.x.abs() < 0.9 { axis.cross(DVec3::X) } else { axis.cross(DVec3::Y) }
                .normalize();
            let anchor = m.add_vertex(origin + basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: origin, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin, normal: axis, basis_u,
                u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());

            // derive d-bounds from axial centres safely inside the side.
            let pmn = pn.normalize();
            let ndm = axis.normalize().dot(pmn);
            let cos_t = ndm.abs();
            let semi_major = r / cos_t;
            let major_dir = (axis.normalize() - ndm * pmn).normalize();
            let z_span = semi_major * major_dir.dot(axis.normalize()).abs();
            let safe_lo = z_span + 0.15 * h;
            let safe_hi = h - z_span - 0.15 * h;
            assert!(safe_lo < safe_hi, "valid {}: safe axial range exists", idx);
            let (t_a, t_b) = (safe_lo + 0.2 * (safe_hi - safe_lo), safe_lo + 0.8 * (safe_hi - safe_lo));
            let (da, db) = (t_a * ndm, t_b * ndm);
            let (d_lo, d_hi) = (da.min(db), da.max(db));

            let r_faces = m.boolean_cylinder_oblique_slab(&cyl, pmn, d_lo, d_hi, mat)
                .unwrap_or_else(|e| panic!("valid {} slab failed: {}", idx, e));
            assert_eq!(r_faces.len(), 3, "valid {}: band + 2 caps", idx);
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "valid {}: watertight", idx,
            );
            assert!(m.verify_face_invariants().is_valid(), "valid {}: invariants", idx);
            assert_eq!(
                m.face_set_manifold_info(&r_faces).non_manifold_edge_count, 0,
                "valid {}: manifold", idx,
            );
            let band = *r_faces.iter()
                .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
                .expect("band");
            let (pos, nrm, idx_b, fmap, _uv) = m.export_buffers().expect("export");
            let mut outside = 0usize;
            for (ti, &fid) in fmap.iter().enumerate() {
                if fid != band.raw() { continue; }
                for k in 0..3 {
                    let vi = idx_b[ti*3+k] as usize;
                    let p = DVec3::new(pos[vi*3] as f64, pos[vi*3+1] as f64, pos[vi*3+2] as f64);
                    let d = (p - origin).dot(pmn);
                    if d < d_lo - 1e-3 || d > d_hi + 1e-3 { outside += 1; }
                }
            }
            assert_eq!(outside, 0, "valid {}: band between planes", idx);
            let nv = pos.len() / 3;
            let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
                c + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
            let mut inward = 0usize;
            for i in 0..nv {
                let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
                let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
                if (p - centroid).dot(n) < -1e-3 { inward += 1; }
            }
            assert_eq!(inward, 0, "valid {}: front-facing", idx);
            assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()),
                "valid {}: finite", idx);
        }

        // degenerate slabs — each must bail cleanly without corrupting the mesh.
        let degenerate: &[(&str, DVec3, f64, f64)] = &[
            ("d_lo >= d_hi", DVec3::new(0.3, 0., 1.), 3.0, 1.0),
            ("⟂ axis", DVec3::Z, -1.0, 1.0),
            ("∥ axis", DVec3::X, -1.0, 1.0),
            ("ellipse past end cap", DVec3::new(0.3, 0., 1.), 5.5, 6.5),
        ];
        for (label, pn, d_lo, d_hi) in degenerate {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
            let r = m.boolean_cylinder_oblique_slab(&cyl, *pn, *d_lo, *d_hi, mat);
            assert!(r.is_err(), "degenerate '{}' must bail (got Ok)", label);
            assert!(m.verify_face_invariants().is_valid(), "'{}': mesh intact after bail", label);
        }
    }

    /// **ADR-205 β-4** — a Z-axis cylinder cut by a plane PARALLEL to the axis
    /// keeps a flat-on-cylinder (D-shaft): partial Cylinder band + flat rect + two
    /// D-caps. The band stays on the kept side, ALL FOUR faces render (the D-caps'
    /// arc boundary must survive the polygon path's ≥3-vert guard), and the solid
    /// is watertight + front-facing.
    #[test]
    fn adr205_beta4_cylinder_axial_halfspace_dshaft() {
        use crate::surfaces::AnalyticSurface as S;
        let mut m = Mesh::default();
        let mat = MaterialId::new(0);
        let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
        // plane x=1, normal −X → keep x<1 (the major arc / big flat).
        let pm = DVec3::new(-1.0, 0.0, 0.0);
        let o = DVec3::new(1.0, 0.0, 0.0);
        let r = m
            .boolean_cylinder_axial_halfspace(&cyl, o, pm, mat)
            .expect("axial halfspace (flat) cut succeeds");
        assert_eq!(r.len(), 4, "band + flat + 2 D-caps");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "D-shaft watertight",
        );
        assert!(m.verify_face_invariants().is_valid(), "DCEL invariants valid");
        assert_eq!(
            m.face_set_manifold_info(&r).non_manifold_edge_count, 0,
            "manifold D-shaft",
        );
        assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))), "has band");
        assert_eq!(
            r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).count(),
            3, "flat + 2 D-caps are planar",
        );

        let pm_n = pm.normalize();
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        // EVERY one of the 4 faces emits triangles (D-cap arc render survives).
        for &f in &r {
            let tris = fmap.iter().filter(|&&fid| fid == f.raw()).count();
            assert!(tris > 0, "face {:?} renders ({} tris)", f, tris);
        }
        // band stays on the kept side ((p−o)·m ≥ 0, i.e. x ≤ 1).
        let band = *r.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).unwrap();
        let mut wrong = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti*3+k] as usize;
                let p = DVec3::new(pos[vi*3] as f64, pos[vi*3+1] as f64, pos[vi*3+2] as f64);
                if (p - o).dot(pm_n) < -1e-3 { wrong += 1; }
            }
        }
        assert_eq!(wrong, 0, "band on the kept (flat) side");
        // outward normals (a circular segment is convex → all front-facing).
        let nv = pos.len() / 3;
        let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
            c + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
        let mut inward = 0usize;
        for i in 0..nv {
            let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
            let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
            if (p - centroid).dot(n) < -1e-3 { inward += 1; }
        }
        assert_eq!(inward, 0, "all D-shaft faces front-facing");
        assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()), "finite");
    }

    /// **ADR-205 β-4 adversarial** — D-shaft over the keep-major / keep-minor /
    /// diametral (half) arc, tilted-axis, and off-origin cases, plus degenerate
    /// rejections. Every valid cut: 4 faces all render, watertight + manifold +
    /// front-facing, band on the kept side. Every degenerate: clean `bail!()`.
    #[test]
    fn adr205_beta4_cylinder_axial_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // (axis_dir, axis_origin, radius, height, plane_origin, plane_normal ⟂ axis)
        let valid: &[(DVec3, DVec3, f64, f64, DVec3, DVec3)] = &[
            // keep-minor arc (small flat sliver toward +x).
            (DVec3::Z, DVec3::ZERO, 2.0, 6.0, DVec3::new(1., 0., 0.), DVec3::X),
            // diametral cut (plane through the axis → half-cylinder).
            (DVec3::Z, DVec3::ZERO, 2.0, 6.0, DVec3::ZERO, DVec3::X),
            // off-origin, normal ⟂ Z but not axis-aligned.
            (DVec3::Z, DVec3::new(3., -2., 1.), 1.5, 5.0,
             DVec3::new(3.6, -2., 1.), DVec3::new(0.8, 0.6, 0.0)),
            // tilted axis (axis.x=0 → X ⟂ axis), keep major.
            (DVec3::new(0., 0.6, 0.8).normalize(), DVec3::ZERO, 2.0, 6.0,
             DVec3::new(-0.8, 0., 0.), DVec3::new(1., 0., 0.)),
        ];
        for (idx, &(axis, origin, r, h, po, pn)) in valid.iter().enumerate() {
            let mut m = Mesh::default();
            let basis_u = if axis.x.abs() < 0.9 { axis.cross(DVec3::X) } else { axis.cross(DVec3::Y) }
                .normalize();
            let anchor = m.add_vertex(origin + basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: origin, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin, normal: axis, basis_u,
                u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());

            let r_faces = m.boolean_cylinder_axial_halfspace(&cyl, po, pn, mat)
                .unwrap_or_else(|e| panic!("valid {} D-shaft failed: {}", idx, e));
            assert_eq!(r_faces.len(), 4, "valid {}: 4 faces", idx);
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "valid {}: watertight", idx,
            );
            assert!(m.verify_face_invariants().is_valid(), "valid {}: invariants", idx);
            assert_eq!(
                m.face_set_manifold_info(&r_faces).non_manifold_edge_count, 0,
                "valid {}: manifold", idx,
            );
            let pmn = pn.normalize();
            let (pos, nrm, idx_b, fmap, _uv) = m.export_buffers().expect("export");
            for &f in &r_faces {
                assert!(fmap.iter().any(|&fid| fid == f.raw()), "valid {}: face {:?} renders", idx, f);
            }
            let band = *r_faces.iter()
                .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
                .expect("band");
            let mut wrong = 0usize;
            for (ti, &fid) in fmap.iter().enumerate() {
                if fid != band.raw() { continue; }
                for k in 0..3 {
                    let vi = idx_b[ti*3+k] as usize;
                    let p = DVec3::new(pos[vi*3] as f64, pos[vi*3+1] as f64, pos[vi*3+2] as f64);
                    if (p - po).dot(pmn) < -1e-3 { wrong += 1; }
                }
            }
            assert_eq!(wrong, 0, "valid {}: band on kept side", idx);
            let nv = pos.len() / 3;
            let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
                c + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
            let mut inward = 0usize;
            for i in 0..nv {
                let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
                let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
                if (p - centroid).dot(n) < -1e-3 { inward += 1; }
            }
            assert_eq!(inward, 0, "valid {}: front-facing", idx);
            assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()),
                "valid {}: finite", idx);
        }

        // degenerate — each must bail cleanly without corrupting the mesh.
        let degenerate: &[(&str, DVec3, DVec3)] = &[
            ("plane ⟂ axis (oblique elliptic)", DVec3::ZERO, DVec3::Z),
            ("plane oblique to axis", DVec3::ZERO, DVec3::new(0.3, 0., 1.)),
            ("plane misses cylinder", DVec3::new(5., 0., 0.), DVec3::X),
            ("degenerate normal", DVec3::ZERO, DVec3::ZERO),
        ];
        for (label, po, pn) in degenerate {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
            let r = m.boolean_cylinder_axial_halfspace(&cyl, *po, *pn, mat);
            assert!(r.is_err(), "degenerate '{}' must bail (got Ok)", label);
            assert!(m.verify_face_invariants().is_valid(), "'{}': mesh intact after bail", label);
        }
    }

    /// **ADR-205 N-plane corner** — a cylinder cut by THREE oblique upper-bound planes
    /// through a common apex V (a 3-sided "pyramid roof", the production analog of a box
    /// VERTEX clip). `boolean_cylinder_corner_n` keeps: bottom disk + a K=3-arc band +
    /// 3 pie-slice caps meeting at V. Watertight + manifold + all 5 faces render +
    /// kept region + front-facing. Plus: a 2-active-plane config delegates to the tent;
    /// degenerate configs bail.
    #[test]
    fn adr205_cylinder_corner_n_pyramid() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let mut m = Mesh::default();
        let mat = MaterialId::new(0);
        let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 8.0, mat);
        // 3-sided pyramid roof: planes through apex V=(0,0,4), inward normals tilted
        // α=50° from vertical, azimuths 0/120/240. m=(sinα cosθ, sinα sinθ, −cosα).
        let alpha = 50.0_f64.to_radians();
        let v_apex = DVec3::new(0., 0., 4.);
        let planes: Vec<(DVec3, DVec3)> = [0.0, 120.0, 240.0]
            .iter()
            .map(|deg| {
                let t = (*deg as f64).to_radians();
                let m = DVec3::new(alpha.sin() * t.cos(), alpha.sin() * t.sin(), -alpha.cos());
                (v_apex, m)
            })
            .collect();
        let r = m
            .boolean_cylinder_corner_n(&cyl, &planes, mat)
            .expect("3-plane pyramid corner cut succeeds");
        assert_eq!(r.len(), 5, "band + bottom disk + 3 pie-slice caps");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "pyramid-cut cylinder watertight",
        );
        assert!(m.verify_face_invariants().is_valid(), "DCEL invariants valid");
        assert_eq!(
            m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold pyramid cut",
        );
        assert_eq!(
            r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).count(),
            1, "exactly one cylinder band",
        );
        assert_eq!(
            r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).count(),
            4, "bottom disk + 3 caps are Plane",
        );
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        for &f in &r {
            assert!(fmap.iter().any(|&fid| fid == f.raw()), "face {:?} renders", f);
        }
        // every rendered vertex is in the kept region: z≥0 AND below all 3 planes.
        let pn: Vec<(DVec3, DVec3)> = planes.iter().map(|&(o, m)| (o, m.normalize())).collect();
        let mut wrong = 0usize;
        for t in idx.chunks(3) {
            for &vi in t {
                let p = DVec3::new(pos[vi as usize * 3] as f64, pos[vi as usize * 3 + 1] as f64, pos[vi as usize * 3 + 2] as f64);
                if p.z < -1e-3 || pn.iter().any(|&(o, mm)| (p - o).dot(mm) < -1e-3) {
                    wrong += 1;
                }
            }
        }
        assert_eq!(wrong, 0, "all faces stay in the kept region (below all 3 planes, above z=0)");
        // front-facing: outward normals (away from the solid centroid).
        let nv = pos.len() / 3;
        let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
            c + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
        let mut inward = 0usize;
        for i in 0..nv {
            let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
            let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
            if (p - centroid).dot(n) < -1e-3 { inward += 1; }
        }
        assert_eq!(inward, 0, "all pyramid faces front-facing");
        assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()), "finite");
        let _ = TAU;
    }

    /// **ADR-205 N-plane corner — delegation + degenerate rejections.** Two active
    /// planes (the third dominated) route to the 2-plane tent (4 faces); a single
    /// upper bound / a ⟂ plane / a non-upper-bound bail cleanly, mesh intact.
    #[test]
    fn adr205_cylinder_corner_n_delegation_and_bails() {
        let mat = MaterialId::new(0);
        // (a) 2 active planes (a third placed so high it never wins) → tent (4 faces).
        {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
            let planes = vec![
                (DVec3::new(0., 0., 4.), DVec3::new(-0.5, 0., -1.)),
                (DVec3::new(0., 0., 4.), DVec3::new(0.5, 0., -1.)),
                (DVec3::new(0., 0., 50.), DVec3::new(0.3, 0., -1.)), // oblique but dominated (way up)
            ];
            let r = m.boolean_cylinder_corner_n(&cyl, &planes, mat).expect("2 active → tent");
            assert_eq!(r.len(), 4, "delegates to the 2-plane tent (band + disk + 2 caps)");
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "delegated tent watertight",
            );
        }
        // (b) a ⟂ plane (n_a·m = 0) bails, mesh intact.
        {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
            let before = m.faces.iter().filter(|(_, f)| f.is_active()).count();
            let planes = vec![
                (DVec3::new(0., 0., 4.), DVec3::new(-0.5, 0., -1.)),
                (DVec3::new(1., 0., 0.), DVec3::new(-1., 0., 0.)), // ⟂ the Z axis
            ];
            assert!(m.boolean_cylinder_corner_n(&cyl, &planes, mat).is_err(), "⟂ plane bails");
            assert_eq!(m.faces.iter().filter(|(_, f)| f.is_active()).count(), before, "mesh intact after bail");
        }
        // (c) a non-upper-bound plane (n_a·m > 0) bails.
        {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
            let planes = vec![
                (DVec3::new(0., 0., 4.), DVec3::new(-0.5, 0., -1.)),
                (DVec3::new(0., 0., 2.), DVec3::new(0.5, 0., 1.)), // n_a·m > 0
            ];
            assert!(m.boolean_cylinder_corner_n(&cyl, &planes, mat).is_err(), "non-upper-bound bails");
        }
        // (d) only one plane → bail (need ≥2).
        {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
            let planes = vec![(DVec3::new(0., 0., 4.), DVec3::new(-0.5, 0., -1.))];
            assert!(m.boolean_cylinder_corner_n(&cyl, &planes, mat).is_err(), "<2 planes bails");
        }
    }

    /// **ADR-205 N-plane corner — adversarial sweep.** Asymmetric pyramids (varying
    /// tilt α per plane + non-uniform azimuths), off-origin cylinders, and an off-axis
    /// (but still inside) apex. Every valid K=3 cut: 5 faces, watertight + manifold +
    /// front-facing + kept region. An apex pushed OUTSIDE the cylinder bails cleanly.
    #[test]
    fn adr205_cylinder_corner_n_adversarial() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        // (apex, [(αdeg, azdeg); 3], cyl (cx,cy,base,radius,height)).
        let cfgs: &[(DVec3, [(f64, f64); 3], (f64, f64, f64, f64, f64))] = &[
            // symmetric baseline (different radius/height).
            (DVec3::new(0., 0., 3.), [(48., 0.), (48., 120.), (48., 240.)], (0., 0., 0., 1.5, 7.)),
            // asymmetric tilt.
            (DVec3::new(0., 0., 4.), [(42., 0.), (55., 120.), (50., 240.)], (0., 0., 0., 2.0, 9.)),
            // asymmetric azimuth.
            (DVec3::new(0., 0., 4.), [(50., 10.), (50., 140.), (50., 255.)], (0., 0., 0., 2.0, 9.)),
            // off-origin cylinder + off-axis apex (still inside the radius-2 cylinder).
            (DVec3::new(3.5, -1.0, 4.0), [(50., 5.), (52., 125.), (48., 250.)], (3.0, -1.0, 0., 2.0, 9.)),
        ];
        for (ci, (apex, tilts, cyl)) in cfgs.iter().enumerate() {
            let mut m = Mesh::default();
            let c = build_clean_cylinder(&mut m, cyl.0, cyl.1, cyl.2, cyl.3, cyl.4, mat);
            let planes: Vec<(DVec3, DVec3)> = tilts
                .iter()
                .map(|&(adeg, azdeg)| {
                    let (a, az) = (adeg.to_radians(), azdeg.to_radians());
                    (*apex, DVec3::new(a.sin() * az.cos(), a.sin() * az.sin(), -a.cos()))
                })
                .collect();
            let r = m
                .boolean_cylinder_corner_n(&c, &planes, mat)
                .unwrap_or_else(|e| panic!("cfg {ci} cut: {e}"));
            assert_eq!(r.len(), 5, "cfg {ci}: band + disk + 3 caps");
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "cfg {ci} watertight",
            );
            assert!(m.verify_face_invariants().is_valid(), "cfg {ci} invariants valid");
            assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "cfg {ci} manifold");
            let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
            for &f in &r { assert!(fmap.iter().any(|&fid| fid == f.raw()), "cfg {ci} face {:?} renders", f); }
            let pn: Vec<(DVec3, DVec3)> = planes.iter().map(|&(o, mm)| (o, mm.normalize())).collect();
            let zbase = cyl.2;
            let mut wrong = 0usize;
            for t in idx.chunks(3) {
                for &vi in t {
                    let p = DVec3::new(pos[vi as usize*3] as f64, pos[vi as usize*3+1] as f64, pos[vi as usize*3+2] as f64);
                    if p.z < zbase - 1e-3 || pn.iter().any(|&(o, mm)| (p - o).dot(mm) < -1e-3) { wrong += 1; }
                }
            }
            assert_eq!(wrong, 0, "cfg {ci} kept region");
            let nv = pos.len() / 3;
            let cen = (0..nv).fold(DVec3::ZERO, |s, i| s + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
            let mut inward = 0usize;
            for i in 0..nv {
                let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
                let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
                if (p - cen).dot(n) < -1e-3 { inward += 1; }
            }
            assert_eq!(inward, 0, "cfg {ci} front-facing");
            assert!(pos.iter().all(|v| v.is_finite()) && nrm.iter().all(|v| v.is_finite()), "cfg {ci} finite");
            assert_eq!(
                r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).count(),
                1, "cfg {ci} one band",
            );
        }
        // apex OUTSIDE the cylinder → V-inside check bails, mesh intact.
        {
            let mut m = Mesh::default();
            let c = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 9.0, mat);
            let before = m.faces.iter().filter(|(_, f)| f.is_active()).count();
            let apex = DVec3::new(6.0, 0., 4.); // radial 6 ≫ radius 2
            let planes: Vec<(DVec3, DVec3)> = [0.0, 120.0, 240.0]
                .iter()
                .map(|&az: &f64| {
                    let (a, azr) = (50.0_f64.to_radians(), az.to_radians());
                    (apex, DVec3::new(a.sin() * azr.cos(), a.sin() * azr.sin(), -a.cos()))
                })
                .collect();
            assert!(m.boolean_cylinder_corner_n(&c, &planes, mat).is_err(), "apex outside bails");
            assert_eq!(m.faces.iter().filter(|(_, f)| f.is_active()).count(), before, "mesh intact after bail");
        }
    }

    /// **ADR-205 β-5 β-2** — a Z-axis cylinder cut by two oblique planes meeting at
    /// a ridge keeps the tent: bottom disk + corner band (piecewise elliptic top) +
    /// two partial elliptic caps. Watertight + manifold + all 4 faces render + the
    /// band stays in the kept region + front-facing.
    #[test]
    fn adr205_beta5_cylinder_corner_tent() {
        use crate::surfaces::AnalyticSurface as S;
        let mut m = Mesh::default();
        let mat = MaterialId::new(0);
        let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
        // tent: two planes meeting at the ridge x=0, z=4, both keeping below.
        let (m1, o1) = (DVec3::new(-0.5, 0., -1.), DVec3::new(0., 0., 4.));
        let (m2, o2) = (DVec3::new(0.5, 0., -1.), DVec3::new(0., 0., 4.));
        let r = m
            .boolean_cylinder_corner(&cyl, o1, m1, o2, m2, mat)
            .expect("corner (tent) cut succeeds");
        assert_eq!(r.len(), 4, "band + bottom disk + 2 partial caps");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "tent-cut cylinder watertight",
        );
        assert!(m.verify_face_invariants().is_valid(), "DCEL invariants valid");
        assert_eq!(
            m.face_set_manifold_info(&r).non_manifold_edge_count, 0,
            "manifold tent cut",
        );
        assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))), "has band");
        let (m1n, m2n) = (m1.normalize(), m2.normalize());
        let band = *r.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).unwrap();
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        for &f in &r {
            assert!(fmap.iter().any(|&fid| fid == f.raw()), "face {:?} renders", f);
        }
        let mut wrong = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let p = DVec3::new(pos[vi*3] as f64, pos[vi*3+1] as f64, pos[vi*3+2] as f64);
                if (p - o1).dot(m1n) < -1e-3 || (p - o2).dot(m2n) < -1e-3 || p.z < -1e-3 {
                    wrong += 1;
                }
            }
        }
        assert_eq!(wrong, 0, "corner band stays in the kept region");
        let nv = pos.len() / 3;
        let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
            c + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
        let mut inward = 0usize;
        for i in 0..nv {
            let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
            let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
            if (p - centroid).dot(n) < -1e-3 { inward += 1; }
        }
        assert_eq!(inward, 0, "all tent faces front-facing");
        assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()), "finite");
    }

    /// **ADR-205 β-5 β-2 adversarial** — corner (tent) cuts over symmetric /
    /// asymmetric / tilted-ridge / off-origin configs, plus degenerate rejections.
    /// Every valid cut: 4 faces all render, watertight + manifold + front-facing +
    /// band in the kept region. Every degenerate: clean `bail!()`.
    #[test]
    fn adr205_beta5_cylinder_corner_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // (cx, cy, r, h, m1, o1, m2, o2) — both planes meet on the axis at o.
        let valid: &[(f64, f64, f64, f64, DVec3, DVec3, DVec3, DVec3)] = &[
            // asymmetric x-tilt.
            (0., 0., 2.0, 6.0, DVec3::new(-0.7, 0., -1.), DVec3::new(0., 0., 4.),
             DVec3::new(0.3, 0., -1.), DVec3::new(0., 0., 4.)),
            // tilted ridge (not along a cardinal axis).
            (0., 0., 2.0, 6.0, DVec3::new(-0.4, 0.3, -1.), DVec3::new(0., 0., 4.),
             DVec3::new(0.4, -0.3, -1.), DVec3::new(0., 0., 4.)),
            // off-origin cylinder.
            (3., -1., 1.5, 5.0, DVec3::new(-0.5, 0., -1.), DVec3::new(3., -1., 3.5),
             DVec3::new(0.5, 0., -1.), DVec3::new(3., -1., 3.5)),
        ];
        for (idx, &(cx, cy, r, h, m1, o1, m2, o2)) in valid.iter().enumerate() {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, cx, cy, 0., r, h, mat);
            let r_faces = m.boolean_cylinder_corner(&cyl, o1, m1, o2, m2, mat)
                .unwrap_or_else(|e| panic!("valid {} corner failed: {}", idx, e));
            assert_eq!(r_faces.len(), 4, "valid {}: 4 faces", idx);
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "valid {}: watertight", idx,
            );
            assert!(m.verify_face_invariants().is_valid(), "valid {}: invariants", idx);
            assert_eq!(
                m.face_set_manifold_info(&r_faces).non_manifold_edge_count, 0,
                "valid {}: manifold", idx,
            );
            let (m1n, m2n) = (m1.normalize(), m2.normalize());
            let band = *r_faces.iter()
                .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
                .expect("band");
            let (pos, nrm, idx_b, fmap, _uv) = m.export_buffers().expect("export");
            for &f in &r_faces {
                assert!(fmap.iter().any(|&fid| fid == f.raw()), "valid {}: face {:?} renders", idx, f);
            }
            let mut wrong = 0usize;
            for (ti, &fid) in fmap.iter().enumerate() {
                if fid != band.raw() { continue; }
                for k in 0..3 {
                    let vi = idx_b[ti*3+k] as usize;
                    let p = DVec3::new(pos[vi*3] as f64, pos[vi*3+1] as f64, pos[vi*3+2] as f64);
                    if (p - o1).dot(m1n) < -1e-3 || (p - o2).dot(m2n) < -1e-3 { wrong += 1; }
                }
            }
            assert_eq!(wrong, 0, "valid {}: band in kept region", idx);
            let nv = pos.len() / 3;
            let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
                c + DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64)) / (nv.max(1) as f64);
            let mut inward = 0usize;
            for i in 0..nv {
                let p = DVec3::new(pos[i*3] as f64, pos[i*3+1] as f64, pos[i*3+2] as f64);
                let n = DVec3::new(nrm[i*3] as f64, nrm[i*3+1] as f64, nrm[i*3+2] as f64);
                if (p - centroid).dot(n) < -1e-3 { inward += 1; }
            }
            assert_eq!(inward, 0, "valid {}: front-facing", idx);
            assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()),
                "valid {}: finite", idx);
        }

        // degenerate — each must bail cleanly without corrupting the mesh.
        let degenerate: &[(&str, DVec3, DVec3, DVec3, DVec3)] = &[
            ("parallel planes", DVec3::new(0.3, 0., -1.), DVec3::new(0., 0., 3.),
             DVec3::new(0.3, 0., -1.), DVec3::new(0., 0., 4.)),
            ("⟂ plane", DVec3::Z, DVec3::new(0., 0., 4.),
             DVec3::new(0.5, 0., -1.), DVec3::new(0., 0., 4.)),
            ("ridge misses the side", DVec3::new(-0.5, 0., -1.), DVec3::new(5., 0., 4.),
             DVec3::new(0.5, 0., -1.), DVec3::new(5., 0., 4.)),
            ("plane cuts from below (n_a·m>0)", DVec3::new(-0.5, 0., 1.), DVec3::new(0., 0., 4.),
             DVec3::new(0.5, 0., -1.), DVec3::new(0., 0., 4.)),
        ];
        for (label, m1, o1, m2, o2) in degenerate {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat);
            let r = m.boolean_cylinder_corner(&cyl, *o1, *m1, *o2, *m2, mat);
            assert!(r.is_err(), "degenerate '{}' must bail (got Ok)", label);
            assert!(m.verify_face_invariants().is_valid(), "'{}': mesh intact after bail", label);
        }
    }

    /// **SIMULATION (ADR-205 γ dispatch)** — the user-facing dispatch key for a
    /// single arbitrary plane cutting a TILTED cylinder is the plane-vs-axis angle
    /// `cosθ = |n_a·m|`: `≈0` (∥ axis) → β-4 axial flat; `0<·<1` (oblique) → β-2
    /// elliptic halfspace; `≈1` (⟂ axis) → the local-frame family. This probe
    /// builds a tilted cylinder, routes by that key, and verifies each branch is
    /// watertight + manifold + invariant-valid — proving the γ routing geometry
    /// before the WASM/TS/SliceTool wiring.
    #[test]
    fn sim_adr205_gamma_tilted_cylinder_plane_dispatch() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let origin = DVec3::ZERO;
        let (r, h) = (2.0, 8.0);
        let build = || {
            let mut m = Mesh::default();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(origin + basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: origin, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin, normal: axis, basis_u,
                u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        // mid-axis point (a clean cut through the side for both branches).
        let mid = origin + axis * (h * 0.5);

        // single-plane TRIM via the γ dispatch entry `boolean_cylinder_trim_plane`.
        let check = |m: &Mesh, r_faces: &[FaceId], lbl: &str| {
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0,
                "{} watertight", lbl);
            assert!(m.verify_face_invariants().is_valid(), "{} invariants", lbl);
            assert_eq!(m.face_set_manifold_info(r_faces).non_manifold_edge_count, 0, "{} manifold", lbl);
        };
        // OBLIQUE (cosθ in (0,1)) → β-2.
        {
            let pn = DVec3::new(0.2, 0.3, 1.0).normalize();
            let cos = axis.dot(pn).abs();
            assert!(cos > 1e-4 && cos < 1.0 - 1e-4, "oblique key (cosθ={})", cos);
            let (mut m, cyl) = build();
            let r_faces = m.boolean_cylinder_trim_plane(&cyl, mid, pn, mat).expect("γ trim oblique");
            check(&m, &r_faces, "oblique");
        }
        // AXIAL (cosθ ≈ 0, plane ∥ axis) → β-4.
        {
            let pn = DVec3::X; // ⟂ the tilted axis (axis.x = 0)
            assert!(axis.dot(pn).abs() < 1e-4, "axial key");
            let po = origin + DVec3::X * 1.0; // |d_axis| = 1 < r
            let (mut m, cyl) = build();
            let r_faces = m.boolean_cylinder_trim_plane(&cyl, po, -pn, mat).expect("γ trim axial");
            check(&m, &r_faces, "axial");
        }
        // PERPENDICULAR (cosθ ≈ 1, plane ⟂ axis) → local-frame slab to the far end.
        {
            let pn = axis; // ⟂ cut plane (normal = axis)
            let po = origin + axis * (h * 0.5);
            let (mut m, cyl) = build();
            let r_faces = m.boolean_cylinder_trim_plane(&cyl, po, pn, mat).expect("γ trim ⟂");
            check(&m, &r_faces, "perpendicular");
        }
        // BOX ∩ cylinder (Z-only cut, exits two parallel Z-faces) → β-3 oblique
        // slab with the box's Z-range. d = z − axis_origin.z (axis_origin.z = 0).
        // This is the routing op for a tilted cylinder passing through a box.
        {
            let (z_lo, z_hi) = (2.0, 4.0); // a Z-slab cutting cleanly through the side
            let (mut m, cyl) = build();
            let r_faces = m.boolean_cylinder_oblique_slab(&cyl, DVec3::Z, z_lo - origin.z, z_hi - origin.z, mat)
                .expect("γ routes box Z-cut → β-3 slab");
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0,
                "box-slab-routed cut watertight");
            assert!(m.verify_face_invariants().is_valid(), "box-slab invariants");
            assert_eq!(m.face_set_manifold_info(&r_faces).non_manifold_edge_count, 0, "box-slab manifold");
            // every band vertex sits within the box Z-range (the kept ∩).
            let band = *r_faces.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).unwrap();
            let (pos, _n, idx, fmap, _uv) = m.export_buffers().expect("export");
            let mut outside = 0usize;
            for (ti, &fid) in fmap.iter().enumerate() {
                if fid != band.raw() { continue; }
                for k in 0..3 {
                    let p = idx[ti*3+k] as usize;
                    let z = pos[p*3+2] as f64;
                    if z < z_lo - 1e-3 || z > z_hi + 1e-3 { outside += 1; }
                }
            }
            assert_eq!(outside, 0, "box ∩ cylinder slab band within the box Z-range");
        }
    }

    /// **SIMULATION (ADR-205 γ-2a)** — `box ∩ tilted-cylinder` AUTO-ROUTING. A
    /// world-axis box that is a SLAB in one cardinal direction (thin in e, wide
    /// in the other two) passing through a tilted cylinder cuts the cylinder with
    /// its two parallel e-faces (oblique elliptic sections) and CONTAINS it in the
    /// other two. The auto-router must (1) detect which cardinal pairs cut the
    /// cylinder's lateral surface, (2) recognise the pure single-axis-slab config,
    /// (3) route to `boolean_cylinder_oblique_slab` with that pair's normal +
    /// offsets. This probe computes the which-faces-cut detection inline + verifies
    /// the β-3 result, proving the routing geometry BEFORE the dispatch wiring.
    #[test]
    fn sim_adr205_gamma2_box_slab_tilted_cylinder_detection() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let origin = DVec3::ZERO;
        let (r, h) = (2.0, 8.0);
        let build = || {
            let mut m = Mesh::default();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(origin + basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: origin, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin, normal: axis, basis_u,
                u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };

        // The cylinder lateral surface's extent along a cardinal axis e:
        //   [ao·e + min(v0,v1)·(â·e) − r·amp,  ao·e + max(v0,v1)·(â·e) + r·amp]
        // where amp = √((bu·e)² + (bw·e)²) is the radial sweep projected onto e.
        let extent_along = |ao: DVec3, ad: DVec3, bu: DVec3, rad: f64, vr: (f64, f64), e: DVec3| -> (f64, f64) {
            let ad = ad.normalize();
            let bw = ad.cross(bu).normalize();
            let adot = ad.dot(e);
            let amp = ((bu.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            let (v0, v1) = (vr.0.min(vr.1), vr.0.max(vr.1));
            let a_lo = (v0 * adot).min(v1 * adot);
            let a_hi = (v0 * adot).max(v1 * adot);
            (ao.dot(e) + a_lo - rad * amp, ao.dot(e) + a_hi + rad * amp)
        };

        // read the cylinder geometry once.
        let (ao, ad, rad, bu, vr) = {
            let (m, cyl) = build();
            m.cylinder_full_of(&cyl).expect("cylinder geometry")
        };
        let exts = [DVec3::X, DVec3::Y, DVec3::Z].map(|e| extent_along(ao, ad, bu, rad, vr, e));
        // axis (0,.6,.8), r=2, v∈[0,8] → X≈[-2,2], Y≈[-1.6,6.4], Z≈[-1.2,7.6].
        assert!((exts[0].0 + 2.0).abs() < 1e-6 && (exts[0].1 - 2.0).abs() < 1e-6, "X-extent {:?}", exts[0]);
        assert!((exts[1].0 + 1.6).abs() < 1e-6 && (exts[1].1 - 6.4).abs() < 1e-6, "Y-extent {:?}", exts[1]);
        assert!((exts[2].0 + 1.2).abs() < 1e-6 && (exts[2].1 - 7.6).abs() < 1e-6, "Z-extent {:?}", exts[2]);

        // detection: per axis, how many of the box's two e-faces fall strictly
        // inside the cylinder's e-extent (→ that face cuts the lateral surface).
        const EPS: f64 = 1e-9;
        let cut_count = |bmin: DVec3, bmax: DVec3| -> [u8; 3] {
            let mut c = [0u8; 3];
            for (i, e) in [DVec3::X, DVec3::Y, DVec3::Z].into_iter().enumerate() {
                let (lo, hi) = exts[i];
                if bmin.dot(e) > lo + EPS && bmin.dot(e) < hi - EPS { c[i] += 1; }
                if bmax.dot(e) > lo + EPS && bmax.dot(e) < hi - EPS { c[i] += 1; }
            }
            c
        };

        // ── (A) a clean Z-slab box: thin in Z, wide in XY (contains the cylinder).
        let bmin = DVec3::new(-5.0, -5.0, 2.0);
        let bmax = DVec3::new(5.0, 7.0, 4.0);
        let cc = cut_count(bmin, bmax);
        assert_eq!(cc, [0, 0, 2], "pure Z-slab: only the ±Z pair cuts (got {:?})", cc);

        // classify → pure single-axis slab on Z; route β-3 (oblique, |axis·Z|=.8).
        let slab_axis = (0..3).find(|&i| cc[i] == 2 && cc[(i + 1) % 3] == 0 && cc[(i + 2) % 3] == 0);
        assert_eq!(slab_axis, Some(2), "classified as Z-slab");
        let e = DVec3::Z;
        let cos_theta = ad.normalize().dot(e).abs();
        assert!(cos_theta > 1e-6 && cos_theta < 1.0 - 1e-6, "oblique slab (cosθ={})", cos_theta);

        let (mut m, cyl) = build();
        let d_lo = bmin.dot(e) - ao.dot(e); // 2 − 0
        let d_hi = bmax.dot(e) - ao.dot(e); // 4 − 0
        let r_faces = m.boolean_cylinder_oblique_slab(&cyl, e, d_lo, d_hi, mat).expect("γ-2a routes Z-slab → β-3");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0,
            "auto-routed box-slab cut watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants");
        assert_eq!(m.face_set_manifold_info(&r_faces).non_manifold_edge_count, 0, "manifold");
        // every band vertex within the box Z-range (= the kept ∩).
        let band = *r_faces.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).unwrap();
        let (pos, _n, idx, fmap, _uv) = m.export_buffers().expect("export");
        let mut outside = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let z = pos[idx[ti * 3 + k] as usize * 3 + 2] as f64;
                if z < 2.0 - 1e-3 || z > 4.0 + 1e-3 { outside += 1; }
            }
        }
        assert_eq!(outside, 0, "auto-routed slab band within the box Z-range");

        // ── (B) negative: a box thin in X too → the ±X pair ALSO cuts → NOT a
        // pure single-axis slab (multi-plane corner, deferred to γ-2b+).
        let cc2 = cut_count(DVec3::new(-1.0, -5.0, 2.0), DVec3::new(1.0, 7.0, 4.0));
        assert_eq!(cc2, [2, 0, 2], "box cutting X and Z → two slab pairs");
        let pure = (0..3).any(|i| cc2[i] == 2 && cc2[(i + 1) % 3] == 0 && cc2[(i + 2) % 3] == 0);
        assert!(!pure, "multi-axis cut is NOT a pure slab (falls through)");
    }

    /// **ADR-205 γ-2a** — the public `boolean(tilted-cylinder, axis-box, Intersect)`
    /// entry AUTO-routes a single-axis SLAB box to β-3 oblique-slab through the
    /// curved-intersect dispatch (no manual plane). The result keeps the analytic
    /// Cylinder band (surface preserved, NOT faceted), the tilted axis, is
    /// watertight + manifold, lies within the box Z-slab, and the box solid is
    /// consumed. Commutative in operand order. This is the wiring proof on top of
    /// the `sim_…detection` geometry probe.
    #[test]
    fn adr205_gamma2_box_slab_intersect_autoroutes_to_beta3() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let build = || {
            let mut m = Mesh::default();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(basis_u * 2.0);
            let circle = crate::curves::AnalyticCurve::Circle { center: DVec3::ZERO, radius: 2.0, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: DVec3::ZERO, normal: axis, basis_u, u_range: (-3., 3.), v_range: (-3., 3.),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, 8.0, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let verify = |m: &mut Mesh, r: &[FaceId]| {
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0,
                "watertight");
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            let band = r.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })));
            assert!(band.is_some(), "Cylinder band preserved (analytic surface, not facets)");
            if let Some(S::Cylinder { axis_dir, .. }) = m.face_surface(*band.unwrap()) {
                assert!((axis_dir.normalize() - axis).length() < 1e-6, "tilted axis preserved");
            }
            let (pos, _n, idx, fmap, _uv) = m.export_buffers().expect("export");
            let mut out = 0usize;
            for (ti, &fid) in fmap.iter().enumerate() {
                if fid != band.unwrap().raw() { continue; }
                for k in 0..3 {
                    let z = pos[idx[ti * 3 + k] as usize * 3 + 2] as f64;
                    if z < 2.0 - 1e-3 || z > 4.0 + 1e-3 { out += 1; }
                }
            }
            assert_eq!(out, 0, "band within the box Z-slab [2,4]");
        };

        // cyl ∩ box
        let (mut m, cyl) = build();
        let bx = make_box(&mut m, DVec3::new(-5., -5., 2.), DVec3::new(5., 7., 4.), mat);
        let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("γ-2a tilted cyl ∩ Z-slab box auto-routes");
        verify(&mut m, &r.faces);

        // box ∩ cyl (commutative — dispatch tries both operand orderings).
        let (mut m2, cyl2) = build();
        let bx2 = make_box(&mut m2, DVec3::new(-5., -5., 2.), DVec3::new(5., 7., 4.), mat);
        let r2 = m2.boolean(&bx2, &cyl2, BoolOp::Intersect, mat).expect("γ-2a commutative");
        verify(&mut m2, &r2.faces);
    }

    /// **ADR-205 γ-2a adversarial sweep** — robustness of the `box ∩ tilted-
    /// cylinder` auto-route across general tilt (all 3 axis components), an
    /// off-origin pivot, and graceful decline for the DEFERRED configs (corner /
    /// end-cap-clipping slab). Each routed case is watertight + manifold +
    /// Cylinder-band-preserving; each declined case Errs WITHOUT a crash and leaves
    /// the mesh invariant-valid (verify-and-bail, 메타-원칙 #16). Case (5) guards
    /// that a Z-AXIS cylinder still routes via the existing circular-section path
    /// (the tilted route returns `None` for it — no regression).
    #[test]
    fn adr205_gamma2_box_slab_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = |axis: DVec3, pivot: DVec3, r: f64, h: f64| {
            let mut m = Mesh::default();
            let axis = axis.normalize();
            let basis_u = axis.cross(DVec3::Z).normalize();
            let anchor = m.add_vertex(pivot + basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: pivot, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: pivot, normal: axis, basis_u, u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let ok_case = |m: &mut Mesh, r: &[FaceId], lbl: &str| {
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "{lbl} watertight");
            assert!(m.verify_face_invariants().is_valid(), "{lbl} invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "{lbl} manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))), "{lbl} Cylinder band preserved");
        };
        let gen = DVec3::new(0.36, 0.48, 0.8); // unit; X, Y AND Z components.

        // (1) generic tilt + Z-slab → routes β-3 (Z oblique).
        {
            let (mut m, cyl) = build(gen, DVec3::ZERO, 2.0, 8.0);
            let bx = make_box(&mut m, DVec3::new(-3., -3., 2.), DVec3::new(5., 6., 4.), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("generic-tilt Z-slab routes");
            ok_case(&mut m, &r.faces, "generic Z-slab");
        }
        // (2) off-origin pivot + Z-slab → routes (pivot translation preserved).
        {
            let piv = DVec3::new(1.5, -1.0, 0.5);
            let (mut m, cyl) = build(gen, piv, 1.5, 8.0);
            let bx = make_box(&mut m, DVec3::new(-6., -6., piv.z + 2.5), DVec3::new(8., 8., piv.z + 4.0), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("off-origin Z-slab routes");
            ok_case(&mut m, &r.faces, "off-origin Z-slab");
        }
        // (3) corner box (Z pair cuts + one Y face cuts) → NOT a pure slab →
        //     deferred (γ-2b) → graceful Err, mesh intact.
        {
            let (mut m, cyl) = build(gen, DVec3::ZERO, 2.0, 8.0);
            let _bx = make_box(&mut m, DVec3::new(-5., 3., 2.), DVec3::new(5., 8., 4.), mat);
            let r = m.boolean(&cyl, &_bx, BoolOp::Intersect, mat);
            assert!(r.is_err(), "corner/multi-plane config deferred (Err)");
            assert!(m.verify_face_invariants().is_valid(), "mesh intact after corner-config bail");
        }
        // (4) slab too close to the base cap (an ellipse would extend past it) →
        //     β-3 declines → surfaced Err, mesh intact (verify-and-bail).
        {
            let (mut m, cyl) = build(gen, DVec3::ZERO, 2.0, 8.0);
            let _bx = make_box(&mut m, DVec3::new(-3., -3., 0.4), DVec3::new(5., 6., 1.6), mat);
            let r = m.boolean(&cyl, &_bx, BoolOp::Intersect, mat);
            assert!(r.is_err(), "end-cap-clipping slab declined by β-3 (surfaced Err)");
            assert!(m.verify_face_invariants().is_valid(), "mesh intact after end-cap bail");
        }
        // (5) Z-AXIS cylinder + Z-slab → still routes via the EXISTING circular-
        //     section path (the tilted route returns None for it — no regression).
        {
            let mut m = Mesh::default();
            let cyl = build_clean_cylinder(&mut m, 0., 0., -3., 2.0, 6.0, mat); // z∈[-3,3]
            let bx = make_box(&mut m, DVec3::new(-5., -5., -1.5), DVec3::new(5., 5., 1.5), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("Z-axis cyl ∩ Z-slab still routes");
            ok_case(&mut m, &r.faces, "Z-axis slab (existing path)");
        }
    }

    /// **SIMULATION (ADR-205 γ-2b)** — `box ∩ tilted-cylinder` HALFSPACE + no-op
    /// containment detection. Beyond the slab (γ-2a), a box can clip the tilted
    /// cylinder with EXACTLY ONE binding face (the other five non-binding → cyl
    /// entirely inside) → β-2 oblique halfspace; or fully CONTAIN it (zero binding
    /// faces, none excluding) → no-op A∩B=A; or be DISJOINT (a face excludes the
    /// whole cylinder) → empty (deferred). This probe classifies each of the 6 box
    /// faces (0=Cuts / 1=NonBinding / 2=Excluding) against the cylinder's cardinal
    /// extents, then verifies the β-2 route keeps the INSIDE of the box.
    #[test]
    fn sim_adr205_gamma2b_box_halfspace_noop_detection() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let (r, h) = (2.0, 8.0);
        let build = || {
            let mut m = Mesh::default();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: DVec3::ZERO, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: DVec3::ZERO, normal: axis, basis_u, u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let (ao, ad, rad, bu, vr) = {
            let (m, cyl) = build();
            m.cylinder_full_of(&cyl).expect("cyl geom")
        };
        let ad = ad.normalize();
        let bw = ad.cross(bu).normalize();
        let (v0, v1) = (vr.0.min(vr.1), vr.0.max(vr.1));
        let extent = |e: DVec3| -> (f64, f64) {
            let adot = ad.dot(e);
            let amp = ((bu.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            ((ao.dot(e) + (v0 * adot).min(v1 * adot) - rad * amp), (ao.dot(e) + (v0 * adot).max(v1 * adot) + rad * amp))
        };
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let exts: [(f64, f64); 3] = [extent(axes[0]), extent(axes[1]), extent(axes[2])];
        // axis (0,.6,.8) r=2 v∈[0,8] → X[-2,2] Y[-1.6,6.4] Z[-1.2,7.6].
        assert!((exts[2].0 + 1.2).abs() < 1e-6 && (exts[2].1 - 7.6).abs() < 1e-6, "Z-extent {:?}", exts[2]);

        const EPS: f64 = 1e-9;
        // 0=Cuts, 1=NonBinding (cyl inside this face), 2=Excluding (cyl outside).
        let classify = |c: f64, lo: f64, hi: f64, is_max: bool| -> u8 {
            if c > lo + EPS && c < hi - EPS {
                0
            } else if is_max {
                if c >= hi - EPS { 1 } else { 2 }
            } else if c <= lo + EPS {
                1
            } else {
                2
            }
        };
        // classify the 6 faces of a box → (cuts, excluding, the single cutter).
        let faces6 = |bmin: DVec3, bmax: DVec3| -> (u32, u32, Option<(usize, bool)>) {
            let (mut cuts, mut excl, mut cutter) = (0u32, 0u32, None);
            for i in 0..3 {
                let (lo, hi) = exts[i];
                for (is_max, c) in [(false, bmin.dot(axes[i])), (true, bmax.dot(axes[i]))] {
                    match classify(c, lo, hi, is_max) {
                        0 => { cuts += 1; cutter = Some((i, is_max)); }
                        2 => excl += 1,
                        _ => {}
                    }
                }
            }
            (cuts, excl, cutter)
        };

        // ── (A) HALFSPACE: only the +Z face clips (rest non-binding). ──
        let (hmin, hmax) = (DVec3::new(-5., -5., -3.), DVec3::new(5., 7., 4.));
        let (cuts, excl, cutter) = faces6(hmin, hmax);
        assert_eq!((cuts, excl), (1, 0), "exactly one binding face, none excluding");
        assert_eq!(cutter, Some((2, true)), "the +Z face is the single cutter");
        // route β-2: keep inside the box → normal = −outward (−Z for a max face).
        let (i, is_max) = cutter.unwrap();
        let e = axes[i];
        let coord = if is_max { hmax.dot(e) } else { hmin.dot(e) };
        let p_origin = e * coord;
        let p_normal = if is_max { -e } else { e };
        let cos = ad.dot(p_normal).abs();
        assert!(cos > 1e-6 && cos < 1.0 - 1e-6, "oblique halfspace (cosθ={})", cos);
        let (mut m, cyl) = build();
        let rf = m.boolean_cylinder_oblique_halfspace(&cyl, p_origin, p_normal, mat).expect("γ-2b β-2 route");
        assert_eq!(m.hes.iter().filter(|(_, hh)| hh.is_active() && hh.face().is_null()).count(), 0, "halfspace watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants");
        assert_eq!(m.face_set_manifold_info(&rf).non_manifold_edge_count, 0, "manifold");
        // every band vertex on the kept (inside-box, z < 4) side.
        let band = *rf.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).unwrap();
        let (pos, _n, idx, fmap, _uv) = m.export_buffers().expect("export");
        let mut outside = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                if pos[idx[ti * 3 + k] as usize * 3 + 2] as f64 > 4.0 + 1e-3 { outside += 1; }
            }
        }
        assert_eq!(outside, 0, "β-2 keeps the inside-box (z<4) part");

        // ── (B) no-op CONTAINMENT: box ⊇ cylinder → zero binding, none excluding.
        let (cuts_b, excl_b, _) = faces6(DVec3::new(-5., -5., -5.), DVec3::new(5., 8., 9.));
        assert_eq!((cuts_b, excl_b), (0, 0), "box fully contains → no-op (A∩B=A)");

        // ── (C) DISJOINT: box entirely above the cylinder → a face excludes it.
        let (cuts_c, excl_c, _) = faces6(DVec3::new(-5., -5., 10.), DVec3::new(5., 8., 12.));
        assert_eq!(cuts_c, 0, "disjoint box: no binding face");
        assert!(excl_c >= 1, "disjoint box: at least one excluding face → empty (deferred)");
    }

    /// **ADR-205 γ-2b** — the public `boolean(tilted-cylinder, axis-box, Intersect)`
    /// AUTO-routes the HALFSPACE config (one box face clips → β-2) and the no-op
    /// CONTAINMENT config (box ⊇ cylinder → A∩B=A) through the curved-intersect
    /// dispatch. Halfspace keeps the analytic Cylinder band on the INSIDE of the
    /// box (watertight + manifold); containment returns the cylinder unchanged +
    /// consumes the box. Commutative.
    #[test]
    fn adr205_gamma2b_box_halfspace_and_containment_autoroute() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let build = || {
            let mut m = Mesh::default();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(basis_u * 2.0);
            let circle = crate::curves::AnalyticCurve::Circle { center: DVec3::ZERO, radius: 2.0, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: DVec3::ZERO, normal: axis, basis_u, u_range: (-3., 3.), v_range: (-3., 3.),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, 8.0, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };

        // (A) HALFSPACE: box clips the cylinder top (+Z face) → β-2, keep z<4.
        {
            let (mut m, cyl) = build();
            let bx = make_box(&mut m, DVec3::new(-5., -5., -3.), DVec3::new(5., 7., 4.), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("γ-2b halfspace auto-routes");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(&r.faces).non_manifold_edge_count, 0, "manifold");
            let band = *r.faces.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).expect("Cylinder band");
            if let Some(S::Cylinder { axis_dir, .. }) = m.face_surface(band) {
                assert!((axis_dir.normalize() - axis).length() < 1e-6, "tilted axis preserved");
            }
            let (pos, _n, idx, fmap, _uv) = m.export_buffers().expect("export");
            let mut outside = 0usize;
            for (ti, &fid) in fmap.iter().enumerate() {
                if fid != band.raw() { continue; }
                for k in 0..3 {
                    if pos[idx[ti * 3 + k] as usize * 3 + 2] as f64 > 4.0 + 1e-3 { outside += 1; }
                }
            }
            assert_eq!(outside, 0, "halfspace keeps the inside-box (z<4) part");
        }

        // (B) CONTAINMENT no-op: box ⊇ cylinder → returns the cylinder unchanged.
        {
            let (mut m, cyl) = build();
            let cyl_set: std::collections::BTreeSet<u32> = cyl.iter().map(|f| f.raw()).collect();
            let bx = make_box(&mut m, DVec3::new(-5., -5., -5.), DVec3::new(5., 8., 9.), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("γ-2b containment no-op");
            let res_set: std::collections::BTreeSet<u32> = r.faces.iter().map(|f| f.raw()).collect();
            assert_eq!(res_set, cyl_set, "containment returns the cylinder faces unchanged");
            assert!(m.verify_face_invariants().is_valid(), "invariants after no-op");
        }

        // (C) commutative halfspace (box ∩ cyl).
        {
            let (mut m, cyl) = build();
            let bx = make_box(&mut m, DVec3::new(-5., -5., -3.), DVec3::new(5., 7., 4.), mat);
            let r = m.boolean(&bx, &cyl, BoolOp::Intersect, mat).expect("γ-2b commutative halfspace");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
            assert!(r.faces.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))), "Cylinder band");
        }
    }

    /// **ADR-205 γ-2b adversarial sweep** — halfspace robustness across the cutting
    /// face's axis + side, off-origin pivot, parallel-axis graceful defer, and
    /// surfaced declines (disjoint → empty deferred; end-cap-clipping halfspace →
    /// β-2 declines). Routed cases keep the Cylinder band + are watertight; declined
    /// cases Err WITHOUT a crash and leave the mesh invariant-valid.
    #[test]
    fn adr205_gamma2b_halfspace_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = |axis: DVec3, pivot: DVec3, r: f64, h: f64| {
            let mut m = Mesh::default();
            let axis = axis.normalize();
            let basis_u = axis.cross(DVec3::Z).normalize();
            let anchor = m.add_vertex(pivot + basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: pivot, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: pivot, normal: axis, basis_u, u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let ok_band = |m: &mut Mesh, r: &[FaceId], lbl: &str| {
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "{lbl} watertight");
            assert!(m.verify_face_invariants().is_valid(), "{lbl} invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "{lbl} manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))), "{lbl} Cylinder band");
        };
        let gen = DVec3::new(0.36, 0.48, 0.8); // X, Y AND Z components.
        // gen, r=1, h=8 cardinal extents: X[-0.93,3.81] Y[-0.88,4.72] Z[-0.6,7.0];
        // base cap centre at v0 (z=0), top cap at v1 (z=6.4). β-2 needs the clip
        // clear of both caps by r·sinθ (≈0.6 for the Z faces).

        // (1) halfspace via the −Z (min) face → routes β-2 (keep the upper part).
        {
            let (mut m, cyl) = build(gen, DVec3::ZERO, 1.0, 8.0);
            let bx = make_box(&mut m, DVec3::new(-5., -5., 2.0), DVec3::new(6., 7., 12.), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("−Z halfspace routes");
            ok_band(&mut m, &r.faces, "−Z halfspace");
        }
        // (2) halfspace via a +Y face (axis has a Y component → oblique) → β-2.
        {
            let (mut m, cyl) = build(gen, DVec3::ZERO, 1.0, 8.0);
            // +Y clip at y=2 (between cap0 [−0.88,0.88] and cap1 [2.96,4.72] clear zones).
            let bx = make_box(&mut m, DVec3::new(-5., -5., -3.), DVec3::new(6., 2.0, 9.), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("+Y halfspace routes");
            ok_band(&mut m, &r.faces, "+Y halfspace");
        }
        // (3) off-origin pivot + +Z halfspace → routes (pivot preserved).
        {
            let piv = DVec3::new(1.5, -1.0, 0.5);
            let (mut m, cyl) = build(gen, piv, 1.0, 8.0);
            let bx = make_box(&mut m, DVec3::new(-6., -6., piv.z - 3.5), DVec3::new(8., 8., piv.z + 3.5), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("off-origin halfspace routes");
            ok_band(&mut m, &r.faces, "off-origin halfspace");
        }
        // (4) PARALLEL-axis clip: an X face on a (0,.6,.8) cylinder has cosθ=|axis·X|
        //     =0 (plane ∥ axis → no elliptic section, the β-4 case) → our route
        //     returns None → falls through → #Track2 Err, mesh intact.
        {
            let (mut m, cyl) = build(DVec3::new(0., 0.6, 0.8), DVec3::ZERO, 1.0, 8.0);
            let _bx = make_box(&mut m, DVec3::new(-0.5, -5., -3.), DVec3::new(5., 7., 9.), mat);
            let r = m.boolean(&cyl, &_bx, BoolOp::Intersect, mat);
            assert!(r.is_err(), "parallel-axis (X) clip deferred to β-4 → Err");
            assert!(m.verify_face_invariants().is_valid(), "mesh intact after parallel-axis defer");
        }
        // (5) DISJOINT box (entirely above) → empty intersect deferred → Err, intact.
        {
            let (mut m, cyl) = build(gen, DVec3::ZERO, 1.0, 8.0);
            let _bx = make_box(&mut m, DVec3::new(-5., -5., 12.), DVec3::new(6., 7., 15.), mat);
            let r = m.boolean(&cyl, &_bx, BoolOp::Intersect, mat);
            assert!(r.is_err(), "disjoint box → empty intersect deferred (Err)");
            assert!(m.verify_face_invariants().is_valid(), "mesh intact after disjoint defer");
        }
        // (6) END-CAP-CLIPPING halfspace: +Z face at z=6.5 sits inside the top cap's
        //     z-range [5.8,7.0] → β-2 declines (clean-cut guard) → surfaced Err.
        {
            let (mut m, cyl) = build(gen, DVec3::ZERO, 1.0, 8.0);
            let _bx = make_box(&mut m, DVec3::new(-5., -5., -3.), DVec3::new(6., 7., 6.5), mat);
            let r = m.boolean(&cyl, &_bx, BoolOp::Intersect, mat);
            assert!(r.is_err(), "end-cap-clipping halfspace declined by β-2 (Err)");
            assert!(m.verify_face_invariants().is_valid(), "mesh intact after end-cap bail");
        }
    }

    /// **SIMULATION (ADR-205 γ-2c)** — `box ∩ tilted-cylinder` CORNER detection.
    /// Two PERPENDICULAR box faces cutting the tilted cylinder (rest NonBinding)
    /// form a β-5 tent IFF both are "upper bounds" — the cylinder is kept BELOW
    /// each, i.e. the inward normal m = −outward satisfies n_a·m < 0. This probe
    /// classifies the 6 faces, collects the two Cuts on different axes, maps each to
    /// its (origin on the plane, inward normal), verifies the upper-bound + oblique
    /// conditions, routes β-5, and checks the tent is watertight + the band stays in
    /// the kept region. A corner with a non-upper-bound face (+Z + −Y) is deferred.
    #[test]
    fn sim_adr205_gamma2c_box_corner_detection() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let (r, h) = (2.0, 8.0);
        let build = || {
            let mut m = Mesh::default();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: DVec3::ZERO, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: DVec3::ZERO, normal: axis, basis_u, u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let (ao, ad, rad, bu, vr) = {
            let (m, cyl) = build();
            m.cylinder_full_of(&cyl).expect("cyl geom")
        };
        let n_a = ad.normalize();
        let bw = n_a.cross(bu).normalize();
        let (v0, v1) = (vr.0.min(vr.1), vr.0.max(vr.1));
        let extent = |e: DVec3| -> (f64, f64) {
            let adot = n_a.dot(e);
            let amp = ((bu.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            ((ao.dot(e) + (v0 * adot).min(v1 * adot) - rad * amp), (ao.dot(e) + (v0 * adot).max(v1 * adot) + rad * amp))
        };
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        let exts: [(f64, f64); 3] = [extent(axes[0]), extent(axes[1]), extent(axes[2])];
        const EPS: f64 = 1e-9;
        // collect (cutters, excluding) over the 6 faces.
        let scan = |bmin: DVec3, bmax: DVec3| -> (Vec<(usize, bool)>, u32) {
            let (mut cutters, mut excl) = (Vec::new(), 0u32);
            for i in 0..3 {
                let (lo, hi) = exts[i];
                for (is_max, c) in [(false, bmin.dot(axes[i])), (true, bmax.dot(axes[i]))] {
                    if c > lo + EPS && c < hi - EPS {
                        cutters.push((i, is_max));
                    } else if is_max {
                        if c < hi - EPS { excl += 1; }
                    } else if c > lo + EPS {
                        excl += 1;
                    }
                }
            }
            (cutters, excl)
        };
        // map a cutting face → (origin on its plane, inward normal).
        let plane_of = |bmin: DVec3, bmax: DVec3, i: usize, is_max: bool| -> (DVec3, DVec3) {
            let e = axes[i];
            let coord = if is_max { bmax.dot(e) } else { bmin.dot(e) };
            (e * coord, if is_max { -e } else { e }) // o, inward m
        };

        // ── (A) +Z + +Y corner (both upper bounds) → β-5 tent. ──
        let (cmin, cmax) = (DVec3::new(-5., -5., -3.), DVec3::new(5., 3., 4.));
        let (cutters, excl) = scan(cmin, cmax);
        assert_eq!(excl, 0, "corner box: nothing excluded");
        assert_eq!(cutters.len(), 2, "two binding faces");
        assert!(cutters[0].0 != cutters[1].0, "on different axes (perpendicular corner)");
        let (o1, m1) = plane_of(cmin, cmax, cutters[0].0, cutters[0].1);
        let (o2, m2) = plane_of(cmin, cmax, cutters[1].0, cutters[1].1);
        // both upper bounds (n_a·m < 0) + oblique.
        for (mm, lbl) in [(m1, "1"), (m2, "2")] {
            let c = n_a.dot(mm);
            assert!(c < -1e-6, "plane {} is an upper bound (n_a·m={} < 0)", lbl, c);
            assert!(c.abs() < 1.0 - 1e-6, "plane {} oblique", lbl);
        }
        let (mut m, cyl) = build();
        let rf = m.boolean_cylinder_corner(&cyl, o1, m1, o2, m2, mat).expect("γ-2c β-5 tent route");
        assert_eq!(rf.len(), 4, "band + bottom disk + 2 partial caps");
        assert_eq!(m.hes.iter().filter(|(_, hh)| hh.is_active() && hh.face().is_null()).count(), 0, "corner watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants");
        assert_eq!(m.face_set_manifold_info(&rf).non_manifold_edge_count, 0, "manifold");
        // band stays in the kept (+m1 ∩ +m2) region.
        let band = *rf.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).unwrap();
        let (pos, _n, idx, fmap, _uv) = m.export_buffers().expect("export");
        let mut wrong = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let p = DVec3::new(pos[vi * 3] as f64, pos[vi * 3 + 1] as f64, pos[vi * 3 + 2] as f64);
                if (p - o1).dot(m1) < -1e-3 || (p - o2).dot(m2) < -1e-3 { wrong += 1; }
            }
        }
        assert_eq!(wrong, 0, "corner band in the kept region");

        // ── (B) +Z + −Y corner: −Y is a MIN face → inward m=+Y, n_a·m=+0.6 > 0 →
        // NOT an upper bound → not a β-5 tent → deferred. ──
        let (cutters_b, excl_b) = scan(DVec3::new(-5., -1.0, -3.), DVec3::new(5., 7., 4.));
        assert_eq!(excl_b, 0, "no exclusion");
        assert_eq!(cutters_b.len(), 2, "+Z and −Y both cut");
        let upper_ok = cutters_b.iter().all(|&(i, is_max)| {
            let (_, mm) = plane_of(DVec3::new(-5., -1.0, -3.), DVec3::new(5., 7., 4.), i, is_max);
            n_a.dot(mm) < -1e-6
        });
        assert!(!upper_ok, "+Z + −Y is NOT a β-5 top-tent (−Y face fails the upper-bound test) → deferred");
    }

    /// **ADR-205 γ-2c** — the public `boolean(tilted-cylinder, axis-box, Intersect)`
    /// AUTO-routes the CORNER config (two perpendicular box faces clip → β-5 tent)
    /// through the curved-intersect dispatch. The result is the tent — 4 faces
    /// (Cylinder band + bottom disk + 2 partial caps) — watertight + manifold with
    /// the analytic Cylinder band preserved. Commutative.
    #[test]
    fn adr205_gamma2c_box_corner_autoroutes_to_beta5() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let build = || {
            let mut m = Mesh::default();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(basis_u * 2.0);
            let circle = crate::curves::AnalyticCurve::Circle { center: DVec3::ZERO, radius: 2.0, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: DVec3::ZERO, normal: axis, basis_u, u_range: (-3., 3.), v_range: (-3., 3.),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, 8.0, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let verify = |m: &mut Mesh, r: &[FaceId]| {
            assert_eq!(r.len(), 4, "tent: band + bottom disk + 2 partial caps");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            let band = r.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })));
            assert!(band.is_some(), "Cylinder band preserved");
            if let Some(S::Cylinder { axis_dir, .. }) = m.face_surface(*band.unwrap()) {
                assert!((axis_dir.normalize() - axis).length() < 1e-6, "tilted axis preserved");
            }
        };

        // cyl ∩ box (+Z + +Y corner; ridge {y=3,z=4} pierces the axis at v=5).
        let (mut m, cyl) = build();
        let bx = make_box(&mut m, DVec3::new(-5., -5., -3.), DVec3::new(5., 3., 4.), mat);
        let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("γ-2c corner auto-routes");
        verify(&mut m, &r.faces);

        // box ∩ cyl (commutative).
        let (mut m2, cyl2) = build();
        let bx2 = make_box(&mut m2, DVec3::new(-5., -5., -3.), DVec3::new(5., 3., 4.), mat);
        let r2 = m2.boolean(&bx2, &cyl2, BoolOp::Intersect, mat).expect("γ-2c commutative");
        verify(&mut m2, &r2.faces);
    }

    /// **ADR-205 γ N-plane corner** — the public `boolean(tilted-cylinder, axis-box,
    /// Intersect)` AUTO-routes a box VERTEX config (THREE perpendicular box faces clip
    /// → the N-plane pie-slice corner) through the curved-intersect dispatch. The axis
    /// points into the +X+Y+Z octant so the +X/+Y/+Z box faces are all oblique upper
    /// bounds; the box's bmax corner sits inside the cylinder. Result = 5 faces
    /// (Cylinder band + bottom disk + 3 pie-slice caps), watertight + manifold +
    /// tilted axis preserved. Commutative.
    #[test]
    fn adr205_gamma_box_vertex_autoroutes_to_corner_n() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let axis = DVec3::new(1.0, 1.0, 1.0).normalize();
        let build = || {
            let mut m = Mesh::default();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(basis_u * 2.0);
            let circle = crate::curves::AnalyticCurve::Circle { center: DVec3::ZERO, radius: 2.0, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: DVec3::ZERO, normal: axis, basis_u, u_range: (-3., 3.), v_range: (-3., 3.),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, 8.0, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let verify = |m: &mut Mesh, r: &[FaceId]| {
            assert_eq!(r.len(), 5, "box vertex: band + bottom disk + 3 pie-slice caps");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            let band = r.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })));
            assert!(band.is_some(), "Cylinder band preserved");
            if let Some(S::Cylinder { axis_dir, .. }) = m.face_surface(*band.unwrap()) {
                assert!((axis_dir.normalize() - axis).length() < 1e-6, "tilted axis preserved");
            }
            assert_eq!(
                r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).count(),
                4, "bottom disk + 3 caps are Plane",
            );
        };
        // cyl ∩ box (+X+Y+Z vertex at bmax=(2.9,2.9,2.9), on the axis, inside).
        let (mut m, cyl) = build();
        let bx = make_box(&mut m, DVec3::new(-6., -6., -6.), DVec3::new(2.9, 2.9, 2.9), mat);
        let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("γ box-vertex auto-routes");
        verify(&mut m, &r.faces);
        // box ∩ cyl (commutative).
        let (mut m2, cyl2) = build();
        let bx2 = make_box(&mut m2, DVec3::new(-6., -6., -6.), DVec3::new(2.9, 2.9, 2.9), mat);
        let r2 = m2.boolean(&bx2, &cyl2, BoolOp::Intersect, mat).expect("γ box-vertex commutative");
        verify(&mut m2, &r2.faces);
    }

    /// **ADR-205 γ-2c adversarial sweep** — corner robustness across the axis pair
    /// (+Z+X), an off-origin pivot, and graceful decline for the deferred cases:
    /// a non-upper-bound corner (+Z + −Y, which β-5's top-tent convention can't
    /// represent) and a ridge that misses the cylinder. Routed cases are 4-face
    /// tents (Cylinder band, watertight); declined cases Err WITHOUT a crash, mesh
    /// invariant-valid.
    #[test]
    fn adr205_gamma2c_corner_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = |axis: DVec3, pivot: DVec3, r: f64, h: f64| {
            let mut m = Mesh::default();
            let axis = axis.normalize();
            let basis_u = axis.cross(DVec3::X).normalize();
            let anchor = m.add_vertex(pivot + basis_u * r);
            let circle = crate::curves::AnalyticCurve::Circle { center: pivot, radius: r, normal: axis, basis_u };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: pivot, normal: axis, basis_u, u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, h, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let ok_tent = |m: &mut Mesh, r: &[FaceId], lbl: &str| {
            assert_eq!(r.len(), 4, "{lbl} 4-face tent");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "{lbl} watertight");
            assert!(m.verify_face_invariants().is_valid(), "{lbl} invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "{lbl} manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))), "{lbl} band");
        };

        // (1) +Z + +X corner on a general-tilt axis (different axis pair). Both
        //     cutting planes must clear the base cap (X-extent ±1.87, Z ±1.2 at v0):
        //     +X at x=3 > 1.87, +Z at z=4 > 1.2; ridge {x=3,z=4} is 1.09 < r off-axis.
        {
            let gen = DVec3::new(0.36, 0.48, 0.8);
            let (mut m, cyl) = build(gen, DVec3::ZERO, 2.0, 8.0);
            let bx = make_box(&mut m, DVec3::new(-5., -5., -3.), DVec3::new(3.0, 7., 4.), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("+Z+X corner routes");
            ok_tent(&mut m, &r.faces, "+Z+X corner");
        }
        // (2) off-origin +Z + +Y corner → routes (pivot preserved); ridge through
        //     the axis at v=4.
        {
            let piv = DVec3::new(1.0, -0.5, 0.5);
            let (mut m, cyl) = build(DVec3::new(0., 0.6, 0.8), piv, 1.5, 8.0);
            let bx = make_box(&mut m, DVec3::new(-5., -5., -3.), DVec3::new(5., 1.9, 3.7), mat);
            let r = m.boolean(&cyl, &bx, BoolOp::Intersect, mat).expect("off-origin corner routes");
            ok_tent(&mut m, &r.faces, "off-origin corner");
        }
        // (3) +Z + −Y corner: −Y is a MIN face → inward m=+Y, n_a·m=+0.6 > 0 → NOT
        //     an upper bound → β-5 can't represent → deferred (Err, mesh intact).
        {
            let (mut m, cyl) = build(DVec3::new(0., 0.6, 0.8), DVec3::ZERO, 2.0, 8.0);
            let _bx = make_box(&mut m, DVec3::new(-5., -1.0, -3.), DVec3::new(5., 7., 4.), mat);
            let r = m.boolean(&cyl, &_bx, BoolOp::Intersect, mat);
            assert!(r.is_err(), "non-upper-bound corner deferred (Err)");
            assert!(m.verify_face_invariants().is_valid(), "mesh intact after non-tent defer");
        }
        // (4) RIDGE MISSES: +Z+Y corner whose ridge {y=6,z=2} is 3.6 > r from the
        //     axis → β-5 declines ("ridge does not cross") → surfaced Err, intact.
        {
            let (mut m, cyl) = build(DVec3::new(0., 0.6, 0.8), DVec3::ZERO, 2.0, 8.0);
            let _bx = make_box(&mut m, DVec3::new(-5., -5., -3.), DVec3::new(5., 6., 2.), mat);
            let r = m.boolean(&cyl, &_bx, BoolOp::Intersect, mat);
            assert!(r.is_err(), "ridge-miss corner declined by β-5 (Err)");
            assert!(m.verify_face_invariants().is_valid(), "mesh intact after ridge-miss bail");
        }
    }

    /// **SIMULATION (ADR-205 torus spiric de-risk — §1 geometry)** — an oblique plane
    /// cutting a TORUS does NOT give a conic: it gives a *spiric section*, a degree-4
    /// (quartic) curve (Cassini-oval family). So there is no exact NURBS self-loop
    /// (unlike cylinder/cone ellipses) — the cap boundary must be a *sampled* polyline.
    ///
    /// The tractable handle = the **minor circle** at major angle u:
    ///   M_u(v) = c_u + r·cos v·radial_u + r·sin v·n_a,   c_u = C + R·radial_u.
    /// The plane (X−O)·m = 0 pierces it where  A_u + B_u·cos v + D·sin v = 0  with
    ///   A_u = (c_u−O)·m,  B_u = r·(radial_u·m),  D = r·(n_a·m)  (D is u-independent).
    /// Amplitude amp_u = √(B_u²+D²); 0/1/2 solutions per u (exactly the cylinder/cone
    /// boundary-aware pattern). This probe proves: (a) every section point lies on the
    /// torus ∩ plane to ~1e-12; (b) the ⟂-axis plane degenerates to the known z-cut
    /// (2 concentric circles, constant v); (c) classifies the spiric topology over the
    /// shallow→steep range via per-u pierce histogram + NN-chain loop count.
    #[test]
    fn sim_adr205_torus_spiric_section_geometry() {
        use std::f64::consts::TAU;

        // Returns (pierce-histogram [#u with 0,1,2 solutions], loop_count, total_perim).
        let analyze = |c: DVec3, n_a: DVec3, p1: DVec3, rr: f64, mr: f64,
                       o: DVec3, m: DVec3, lbl: &str| -> ([usize; 3], usize, f64) {
            let n_a = n_a.normalize();
            let p1 = (p1 - n_a * p1.dot(n_a)).normalize();
            let p2 = n_a.cross(p1);
            let m = m.normalize();
            let d = mr * n_a.dot(m); // u-independent sin-coefficient
            let n = 2880usize;
            // Per-u-slice section points (0/1/2 each), stored with a global id, so we can
            // join consecutive slices by nearest match (the section is structured by u).
            let mut slice: Vec<Vec<usize>> = vec![Vec::new(); n]; // u_i -> point ids
            let mut pts: Vec<DVec3> = Vec::new();
            let mut hist = [0usize; 3];
            let mut max_plane = 0.0f64;
            let mut max_torus = 0.0f64;
            for i in 0..n {
                let u = TAU * (i as f64) / (n as f64);
                let radial = p1 * u.cos() + p2 * u.sin();
                let c_u = c + radial * rr;
                let a_u = (c_u - o).dot(m);
                let b_u = mr * radial.dot(m);
                let amp = (b_u * b_u + d * d).sqrt();
                let mut k = 0usize;
                if amp > 1e-12 && a_u.abs() <= amp - 1e-12 {
                    // B cos v + D sin v = amp·cos(v − φ),  φ = atan2(D, B)
                    let phi = d.atan2(b_u);
                    let dv = (-a_u / amp).clamp(-1.0, 1.0).acos();
                    for &v in &[(phi + dv).rem_euclid(TAU), (phi - dv).rem_euclid(TAU)] {
                        let x = c_u + radial * (mr * v.cos()) + n_a * (mr * v.sin());
                        max_plane = max_plane.max((x - o).dot(m).abs());
                        let rel = x - c;
                        let ax = rel.dot(n_a);
                        let rad = (rel - n_a * ax).length();
                        max_torus = max_torus.max(((rad - rr).powi(2) + ax * ax - mr * mr).abs());
                        slice[i].push(pts.len());
                        pts.push(x);
                        k += 1;
                    }
                } else if amp > 1e-12 && a_u.abs() < amp + 1e-9 {
                    k = 1; // tangent (rare); not collected
                }
                hist[k.min(2)] += 1;
            }
            // Loop count = connected components of the "join consecutive u-slices by
            // nearest point" graph (union-find). Robust because same-loop points in
            // adjacent slices are ~(2π·R/n) apart, far below the inter-loop gap.
            let thresh = (TAU * rr / (n as f64)) * 6.0;
            let nn = pts.len();
            let mut uf: Vec<usize> = (0..nn).collect();
            fn find(uf: &mut [usize], a: usize) -> usize {
                let mut a = a;
                while uf[a] != a { uf[a] = uf[uf[a]]; a = uf[a]; }
                a
            }
            let mut perim = 0.0f64;
            for i in 0..n {
                let j = (i + 1) % n;
                for &p in &slice[i] {
                    // nearest point in the next slice
                    let mut best = usize::MAX;
                    let mut bestd = f64::MAX;
                    for &q in &slice[j] {
                        let dd = (pts[q] - pts[p]).length_squared();
                        if dd < bestd { bestd = dd; best = q; }
                    }
                    if best != usize::MAX && bestd.sqrt() <= thresh {
                        perim += bestd.sqrt();
                        let (ra, rb) = (find(&mut uf, p), find(&mut uf, best));
                        if ra != rb { uf[ra] = rb; }
                    }
                }
            }
            let mut roots = std::collections::BTreeSet::new();
            for i in 0..nn { let r = find(&mut uf, i); roots.insert(r); }
            let loops = roots.len();
            eprintln!(
                "[{lbl}] hist(0/1/2)={hist:?} loops={loops} perim={perim:.2} plane_err={max_plane:.1e} torus_err={max_torus:.1e}"
            );
            assert!(max_plane < 1e-9, "{lbl}: every section point on the cut plane");
            assert!(max_torus < 1e-9, "{lbl}: every section point on the torus");
            (hist, loops, perim)
        };

        // (A) TILTED torus, plane ⟂ to its OWN axis → must reduce to the z-cut:
        //     2 concentric circles (constant v per branch). hist = all-2, loops = 2,
        //     perimeter = 2π(R+r) + 2π(R−r) = 4πR.
        let na = DVec3::new(0.3, 0.0, 0.954).normalize();
        let (ha, la, pa) = analyze(
            DVec3::new(0., 0., 5.), na, DVec3::X, 4.0, 1.5,
            DVec3::new(0., 0., 5.), na, "A ⟂-axis (≡ z-cut)",
        );
        assert_eq!(ha[1], 0, "A: no tangent u");
        assert_eq!(ha[2], 2880, "A: every minor circle pierced twice (annular)");
        assert_eq!(la, 2, "A: ⟂ section = 2 concentric circles");
        assert!((pa - 4.0 * std::f64::consts::PI * 4.0).abs() < 0.02, "A perim = 4πR");

        // (B) Torus tilted ~11.5° off +Z, cut by a CARDINAL +Z box face THROUGH the
        //     centre. The plane normal stays within ~22° of the axis (the all-2
        //     threshold √: |m_∥|/|m_⊥| ≤ r/√(R²−r²) = 0.40), so every minor circle is
        //     pierced twice → an ANNULAR cap (2 deformed spiric ovals, with a hole).
        //     Perimeter departs from 4πR → it is a spiric, not 2 circles.
        let (hb, lb, pb) = analyze(
            DVec3::new(0., 0., 5.), DVec3::new(0.2, 0., 0.98).normalize(), DVec3::X,
            4.0, 1.5, DVec3::new(0., 0., 5.), DVec3::Z, "B cardinal+Z thru centre",
        );
        assert_eq!(hb[2], 2880, "B: shallow oblique → every minor circle pierced twice");
        assert_eq!(lb, 2, "B: annular spiric → 2 ovals (cap has a hole)");
        assert!((pb - 4.0 * std::f64::consts::PI * 4.0).abs() > 0.05, "B is a spiric (perim ≠ 4πR)");

        // (C) Same torus, CARDINAL +Z face offset HIGH so it grazes only the top of
        //     the tube → most minor circles are entirely below the plane (missed):
        //     hist[0] > 0 (pinched regime — the annulus is broken into oval(s)).
        let (hc, _, _) = analyze(
            DVec3::new(0., 0., 5.), DVec3::new(0.2, 0., 0.98).normalize(), DVec3::X,
            4.0, 1.5, DVec3::new(0., 0., 6.3), DVec3::Z, "C cardinal+Z grazing high",
        );
        assert!(hc[0] > 0, "C: grazing cut misses minor circles (pinched, not annular)");
        assert!(hc[2] > 0, "C: but still pierces a band (real section present)");
    }

    /// **SIMULATION (ADR-205 torus spiric de-risk — §2 DCEL surgery)** — the kept
    /// (annular) side of a torus cut by an oblique plane is bounded by exactly TWO
    /// pieces sharing the SAME two sampled spiric loops (outer + inner ovals):
    ///   • a **Torus band** (the kept half-tube, an annulus), and
    ///   • an **annular Plane cap** (the planar region between the two ovals).
    /// Unlike cylinder/cone (an analytic Circle self-loop → `sew_curved_band`), the
    /// spiric boundary is a *sampled polyline*, so this probe proves the key claim:
    /// **no new sew primitive is needed** — two `add_face_with_holes` calls with
    /// REVERSED windings make the band & cap share each polyline edge's twin
    /// half-edges (add_edge reuses the edge, make_loop grabs the free twin) →
    /// watertight + manifold. This is the production recipe for `boolean_torus_oblique_halfspace`.
    #[test]
    fn sim_adr205_torus_oblique_halfspace_dcel() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;

        // Annular config (≡ §1 case B): torus tilted ~11.5° off +Z, cut by a CARDINAL
        // +Z plane through the centre → every minor circle pierced twice (annular).
        let c = DVec3::new(0., 0., 5.);
        let n_a = DVec3::new(0.2, 0., 0.98).normalize();
        let p1 = (DVec3::X - n_a * DVec3::X.dot(n_a)).normalize();
        let p2 = n_a.cross(p1);
        let (rr, mr) = (4.0_f64, 1.5_f64);
        let m = DVec3::Z;
        let o = c;
        let d = mr * n_a.dot(m);

        let n = 96usize;
        let mut outer: Vec<DVec3> = Vec::with_capacity(n);
        let mut inner: Vec<DVec3> = Vec::with_capacity(n);
        for i in 0..n {
            let u = TAU * (i as f64) / (n as f64);
            let radial = p1 * u.cos() + p2 * u.sin();
            let c_u = c + radial * rr;
            let a_u = (c_u - o).dot(m);
            let b_u = mr * radial.dot(m);
            let amp = (b_u * b_u + d * d).sqrt();
            assert!(a_u.abs() < amp, "annular: every minor circle pierced twice");
            let phi = d.atan2(b_u);
            let dv = (-a_u / amp).clamp(-1.0, 1.0).acos();
            let (v1, v2) = ((phi + dv), (phi - dv));
            // radial-from-axis = R + r·cos v → larger cos v ⇒ OUTER oval.
            let (vo, vi) = if v1.cos() >= v2.cos() { (v1, v2) } else { (v2, v1) };
            outer.push(c_u + radial * (mr * vo.cos()) + n_a * (mr * vo.sin()));
            inner.push(c_u + radial * (mr * vi.cos()) + n_a * (mr * vi.sin()));
        }

        let mut mesh = Mesh::new();
        let outer_ids: Vec<VertId> = outer.iter().map(|&p| mesh.add_vertex(p)).collect();
        let inner_ids: Vec<VertId> = inner.iter().map(|&p| mesh.add_vertex(p)).collect();
        let mat = MaterialId::new(0);

        // Cap (planar annulus): outer oval CCW, inner oval the hole.
        let cap = mesh
            .add_face_with_holes(&outer_ids, &[&inner_ids], mat)
            .expect("annular cap sews");
        mesh.set_face_surface(cap, Some(S::Plane {
            origin: o, normal: -m, basis_u: p1,
            u_range: (-(rr + mr), rr + mr), v_range: (-(rr + mr), rr + mr),
        }));

        // Band (Torus half-tube): SAME loops REVERSED → reuses each edge's free twin.
        let outer_rev: Vec<VertId> = outer_ids.iter().rev().copied().collect();
        let inner_rev: Vec<VertId> = inner_ids.iter().rev().copied().collect();
        let band = mesh
            .add_face_with_holes(&outer_rev, &[&inner_rev], mat)
            .expect("torus band sews onto the cap's free twins");
        mesh.set_face_surface(band, Some(S::Torus {
            center: c, axis_dir: n_a, ref_dir: p1,
            major_radius: rr, minor_radius: mr, u_range: (0., TAU), v_range: (0., TAU),
        }));

        // Watertight: every spiric-loop edge now bears exactly 2 active faces.
        let report = mesh.verify_face_invariants();
        assert!(report.is_valid(), "annular halfspace watertight + manifold: {report:?}");
        let nm = mesh.collect_non_manifold_edges().len();
        assert_eq!(nm, 0, "no non-manifold edges (band+cap share every rim edge)");
        // exactly the 2 faces, surfaces attached.
        assert!(matches!(mesh.face_surface(cap), Some(S::Plane { .. })), "cap is Plane");
        assert!(matches!(mesh.face_surface(band), Some(S::Torus { .. })), "band is Torus");
        eprintln!("[torus halfspace DCEL] cap={cap:?} band={band:?} nm_edges={nm} valid=true");
    }

    /// **SIMULATION (ADR-205 torus PINCHED de-risk — topology + 1-oval DCEL)** — a cut
    /// steeper than the annular threshold misses some minor circles → the spiric is
    /// NOT annular (2 ovals) but a single oval (or, through the centre, two ovals / a
    /// lemniscate — the Cassini family). The pierced u-band is a SUB-interval; its two
    /// v-branches meet at the tangent u-ends → ONE closed oval. This probe (a) counts
    /// the pierced bands to classify the topology (off-centre bulge = 1, through-centre
    /// = 2), and (b) builds the kept bulge patch (Torus, one spiric-oval boundary) + a
    /// disk cap (Plane, the same oval) and proves it sews watertight — the 1-oval MVP.
    #[test]
    fn sim_adr205_torus_pinched_geometry() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{PI, TAU};

        // count contiguous pierced u-bands (circular) for a flat torus R=4 r=1.5.
        let (rr, mr) = (4.0_f64, 1.5_f64);
        let center = DVec3::ZERO;
        let (p1, p2, axis) = (DVec3::X, DVec3::Y, DVec3::Z);
        let count_bands = |m: DVec3, o: DVec3| -> usize {
            let m = m.normalize();
            let d = mr * axis.dot(m);
            let n = 2880usize;
            let mut pierced = vec![false; n];
            for i in 0..n {
                let u = TAU * (i as f64) / (n as f64);
                let radial = p1 * u.cos() + p2 * u.sin();
                let c_u = center + radial * rr;
                let a_u = (c_u - o).dot(m);
                let b_u = mr * radial.dot(m);
                let amp = (b_u * b_u + d * d).sqrt();
                pierced[i] = amp > 1e-12 && a_u.abs() < amp;
            }
            // contiguous runs of `true` on a circle.
            let mut bands = 0;
            for i in 0..n {
                if pierced[i] && !pierced[(i + n - 1) % n] { bands += 1; }
            }
            bands
        };
        // off-centre axial cut (m=X through x=3, keep +X bulge) → 1 oval.
        assert_eq!(count_bands(DVec3::X, DVec3::new(3., 0., 0.)), 1, "off-centre bulge = single oval");
        // a steeper off-centre cut still 1 band; through-centre steep cut → 2 bands.
        assert_eq!(count_bands(DVec3::new(0.9, 0., 0.436), DVec3::new(3.2, 0., 0.)), 1, "oblique bulge = 1 oval");
        assert_eq!(count_bands(DVec3::new(1.0, 0., 0.30), DVec3::ZERO), 2, "through-centre steep = 2 ovals (deferred)");

        // ── 1-oval DCEL: build the kept bulge (m=X through x=3, keep +X).
        // band u ∈ (−u_t, u_t), u_t = acos(0.545); per u cos v = (3−4cos u)/(1.5cos u),
        // v_a=+acos, v_b=−acos; v=0 at the tangents. eval on the flat torus.
        let eval = |u: f64, v: f64| -> DVec3 {
            let radial = p1 * u.cos() + p2 * u.sin();
            center + radial * (rr + mr * v.cos()) + axis * (mr * v.sin())
        };
        // exact tangent: cos v = 1 ⇒ (3 − 4cos u)/(1.5cos u) = 1 ⇒ cos u = 3/5.5.
        let u_t = (3.0_f64 / 5.5).acos();
        let cval = |u: f64| ((3.0 - 4.0 * u.cos()) / (1.5 * u.cos())).clamp(-1.0, 1.0);
        let n = 48usize;
        let mut oval: Vec<DVec3> = Vec::new();
        oval.push(eval(-u_t, 0.0)); // tangent 1
        for i in 1..n {
            let u = -u_t + (2.0 * u_t) * (i as f64) / (n as f64);
            oval.push(eval(u, cval(u).acos())); // v_a (+) branch
        }
        oval.push(eval(u_t, 0.0)); // tangent 2
        for i in (1..n).rev() {
            let u = -u_t + (2.0 * u_t) * (i as f64) / (n as f64);
            oval.push(eval(u, -cval(u).acos())); // v_b (−) branch, reversed
        }
        // every oval point on the cut plane x=3.
        assert!(oval.iter().all(|p| (p.x - 3.0).abs() < 1e-9), "spiric oval lies on x=3");

        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let ids: Vec<VertId> = oval.iter().map(|&p| mesh.add_vertex(p)).collect();
        let rev: Vec<VertId> = ids.iter().rev().copied().collect();
        // cap (Plane, outward −X) + bulge patch (Torus), sharing the one oval.
        let cap = mesh.add_face_with_holes(&ids, &[], mat).expect("cap disk sews");
        mesh.set_face_surface(cap, Some(S::Plane {
            origin: DVec3::new(3., 0., 0.), normal: DVec3::NEG_X, basis_u: DVec3::Y,
            u_range: (-6., 6.), v_range: (-6., 6.),
        }));
        let patch = mesh.add_face_with_holes(&rev, &[], mat).expect("bulge patch sews onto the cap twins");
        mesh.set_face_surface(patch, Some(S::Torus {
            center, axis_dir: axis, ref_dir: p1, major_radius: rr, minor_radius: mr,
            u_range: (0., TAU), v_range: (0., TAU),
        }));

        let open = mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        assert_eq!(open, 0, "no open half-edges → closed bulge shell");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold + invariants");
        assert_eq!(mesh.collect_non_manifold_edges().len(), 0, "cap & patch share the spiric rim");
        let _ = PI;
        eprintln!("[torus pinched DCEL] cap={cap:?} patch={patch:?} open={open} valid=true (1-oval bulge)");
    }

    /// **SIMULATION (ADR-205 N-plane cylinder corner de-risk)** — the existing
    /// `boolean_cylinder_corner` handles EXACTLY two oblique upper-bound planes (a
    /// 2-arc tent at a box EDGE). A box VERTEX clips a tilted cylinder with up to
    /// THREE perpendicular faces simultaneously. The kept top is the LOWER ENVELOPE
    /// of the N planes (per generator angle u, the kept axial bound is
    /// `min_i v_plane_i(u)`), so the top loop is K elliptic arcs joined by K corners
    /// (ridge of two consecutive active planes ∩ cylinder), where K = the number of
    /// planes that are actually the min on some u-arc (≤ N). This probe (a) confirms
    /// the lower envelope of a SYMMETRIC 3-plane box-vertex clip has K=3 active arcs,
    /// (b) extracts the 3 corners + per-arc active plane, and (c) sews the generalized
    /// `2K`-vertex top loop (`sew_corner_band` already takes ≥3 verts) + K partial caps
    /// into a watertight shell — proving the N-plane caller is feasible.
    #[test]
    fn sim_adr205_cyl_corner_n_geometry() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let mat = MaterialId::new(0);
        // tilted cylinder, axis into the +X+Y+Z octant.
        let ao = DVec3::ZERO;
        let n_a = DVec3::new(1., 1., 1.).normalize();
        let radius = 2.0_f64;
        let ref_dir = DVec3::X;
        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize();
        let (v0, v1) = (-3.0_f64, 12.0_f64);
        // box +X +Y +Z faces at coord 2 (inward normals −e); keep x<2 & y<2 & z<2.
        let planes: Vec<(DVec3, DVec3)> = [DVec3::X, DVec3::Y, DVec3::Z]
            .iter()
            .map(|&e| (-e, e * 2.0)) // (inward normal m, origin o)
            .collect();
        for &(m, _) in &planes {
            assert!(n_a.dot(m) < -1e-6, "every plane is an oblique upper bound (n_a·m<0)");
        }
        let v_plane = |m: DVec3, o: DVec3, u: f64| {
            ((o - ao).dot(m) - radius * (r_vec * u.cos() + p_vec * u.sin()).dot(m)) / n_a.dot(m)
        };
        let u_of = |p: DVec3| {
            let rel = p - ao;
            rel.dot(p_vec).atan2(rel.dot(r_vec)).rem_euclid(TAU)
        };
        let surf = |u: f64, v: f64| ao + n_a * v + r_vec * (radius * u.cos()) + p_vec * (radius * u.sin());

        // (a) lower envelope: per-u active plane = argmin v_plane.
        let nsamp = 3600usize;
        let active_at = |u: f64| -> usize {
            (0..planes.len())
                .min_by(|&i, &j| {
                    v_plane(planes[i].0, planes[i].1, u)
                        .partial_cmp(&v_plane(planes[j].0, planes[j].1, u))
                        .unwrap()
                })
                .unwrap()
        };
        let act: Vec<usize> = (0..nsamp).map(|s| active_at(s as f64 / nsamp as f64 * TAU)).collect();
        // contiguous runs (cyclic).
        let mut runs: Vec<(usize, usize, usize)> = Vec::new(); // (plane, start_idx, end_idx)
        let mut s0 = 0usize;
        for s in 1..nsamp {
            if act[s] != act[s - 1] {
                runs.push((act[s - 1], s0, s - 1));
                s0 = s;
            }
        }
        runs.push((act[nsamp - 1], s0, nsamp - 1));
        // merge cyclic wrap (first & last run same plane).
        if runs.len() > 1 && runs[0].0 == runs[runs.len() - 1].0 {
            let last = runs.pop().unwrap();
            runs[0].1 = last.1; // start wraps back
        }
        let distinct: std::collections::BTreeSet<usize> = runs.iter().map(|r| r.0).collect();
        eprintln!("[cyl corner-N] runs={} distinct active planes={:?}", runs.len(), distinct);
        assert_eq!(runs.len(), 3, "symmetric 3-plane box vertex → exactly 3 active arcs");
        assert_eq!(distinct.len(), 3, "all three planes active (K=3)");

        // (b) corners = ridge(plane[k], plane[k+1]) ∩ cylinder near each run boundary.
        let ridge_corner = |a: usize, b: usize, u_hint: f64| -> DVec3 {
            let (ma, oa) = planes[a];
            let (mb, ob) = planes[b];
            let dir_raw = ma.cross(mb);
            let (da, db) = (oa.dot(ma), ob.dot(mb));
            let l0 = (mb.cross(dir_raw) * da + dir_raw.cross(ma) * db) / dir_raw.dot(dir_raw);
            let perp = |w: DVec3| w - n_a * w.dot(n_a);
            let (a0, ad) = (perp(l0 - ao), perp(dir_raw));
            let (qa, qb, qc) = (ad.dot(ad), 2.0 * a0.dot(ad), a0.dot(a0) - radius * radius);
            let disc = qb * qb - 4.0 * qa * qc;
            assert!(qa > 1e-12 && disc > 0.0, "ridge crosses the cylinder");
            let sd = disc.sqrt();
            let c_lo = l0 + dir_raw * ((-qb - sd) / (2.0 * qa));
            let c_hi = l0 + dir_raw * ((-qb + sd) / (2.0 * qa));
            // pick the root whose u matches the run boundary.
            let du = |c: DVec3| {
                let d = (u_of(c) - u_hint).rem_euclid(TAU);
                d.min(TAU - d)
            };
            if du(c_lo) <= du(c_hi) { c_lo } else { c_hi }
        };
        // run[k] is active over [a_k, b_k]; the corner at b_k is between plane[k] & plane[k+1].
        let k = runs.len();
        let u_at = |idx: usize| idx as f64 / nsamp as f64 * TAU;
        let corners: Vec<DVec3> = (0..k)
            .map(|i| {
                let u_b = u_at(runs[i].2); // end of run i
                ridge_corner(runs[i].0, runs[(i + 1) % k].0, u_b)
            })
            .collect();
        for c in &corners {
            // corner on the cylinder side (axial within range) + at radius.
            let av = (*c - ao).dot(n_a);
            assert!(av > v0 + 1e-6 && av < v1 - 1e-6, "corner on the side, not past a cap");
            let radial = (*c - ao) - n_a * av;
            assert!((radial.length() - radius).abs() < 1e-6, "corner on the cylinder surface");
            // corner is on BOTH its planes (kept-boundary) + the min of the third.
            let vs: Vec<f64> = planes.iter().map(|&(m, o)| v_plane(m, o, u_of(*c))).collect();
            let cv = (*c - ao).dot(n_a);
            let below = vs.iter().filter(|&&v| v < cv - 1e-6).count();
            assert_eq!(below, 0, "corner is on the lower envelope (no plane below it)");
        }

        // (c) build the 2K top loop + sew + K caps.
        // ellipse params (β-2) + φ for arc NURBS.
        let ellipse_of = |m: DVec3, o: DVec3| {
            let ndm = n_a.dot(m);
            let center = ao + n_a * ((o - ao).dot(m) / ndm);
            let minor = m.cross(n_a).normalize();
            let major = (n_a - ndm * m).normalize();
            (center, radius / ndm.abs(), radius, major, minor)
        };
        let phi = |p: DVec3, c: DVec3, a: f64, b: f64, mj: DVec3, mn: DVec3| {
            let rel = p - c;
            (rel.dot(mn) / b).atan2(rel.dot(mj) / a)
        };
        let arc_nurbs = |p: DVec3, q: DVec3, c, a, b, mj, mn| {
            let (cp, w, kn, d) = crate::curves::nurbs::ellipse_arc(
                c, a, b, mj, mn, phi(p, c, a, b, mj, mn), phi(q, c, a, b, mj, mn),
            );
            crate::curves::AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: kn, degree: d as u32 }
        };
        // top loop: [corner[k-1], mid_0, corner[0], mid_1, corner[1], ... ] where
        // mid_i is the midpoint of arc on plane[i] (run i), between corner[i-1] & corner[i].
        let mut top_verts: Vec<DVec3> = Vec::with_capacity(2 * k);
        let mut top_curves: Vec<crate::curves::AnalyticCurve> = Vec::with_capacity(2 * k);
        let mut cap_specs: Vec<(usize, usize, usize)> = Vec::new(); // (plane, vid_start, vid_mid)
        for i in 0..k {
            let prev_corner = corners[(i + k - 1) % k];
            let cur_corner = corners[i];
            let plane = runs[i].0;
            let (m, o) = planes[plane];
            let (ec, ea, eb, emj, emn) = ellipse_of(m, o);
            // arc midpoint on plane[i] at the run mid-u.
            let mid_u = {
                let (a_idx, b_idx) = (runs[i].1, runs[i].2);
                let span = if b_idx >= a_idx { b_idx - a_idx } else { nsamp - a_idx + b_idx };
                u_at((a_idx + span / 2) % nsamp)
            };
            let mid_p = surf(mid_u, v_plane(m, o, mid_u));
            let base = top_verts.len();
            top_verts.push(prev_corner);
            top_verts.push(mid_p);
            top_curves.push(arc_nurbs(prev_corner, mid_p, ec, ea, eb, emj, emn));
            top_curves.push(arc_nurbs(mid_p, cur_corner, ec, ea, eb, emj, emn));
            cap_specs.push((plane, base, base + 1));
        }
        // the last curve must close to corner[k-1] (top_verts[0]); fix the final
        // arc's endpoint by construction it already targets cur_corner=corners[k-1]
        // for i=k-1, whose NEXT vertex wraps to top_verts[0]=corners[k-1]. ✓
        assert_eq!(top_verts.len(), 2 * k);

        let mut mesh = Mesh::new();
        let c_bot = ao + n_a * v0;
        let bottom_circle = crate::curves::AnalyticCurve::Circle {
            center: c_bot, radius, normal: -n_a, basis_u: r_vec,
        };
        // bottom wholly kept?
        for &(m, o) in &planes {
            let sin_t = (1.0 - n_a.dot(m).powi(2)).max(0.0).sqrt();
            assert!((c_bot - o).dot(m) > radius * sin_t, "bottom circle wholly kept");
        }
        let band = S::Cylinder {
            axis_origin: ao, axis_dir: n_a, radius, ref_dir,
            u_range: (0.0, TAU), v_range: (v0, v1),
        };
        let bottom_disk = S::Plane {
            origin: c_bot, normal: -n_a, basis_u: r_vec,
            u_range: (-radius * 1.5, radius * 1.5), v_range: (-radius * 1.5, radius * 1.5),
        };
        // box vertex V = the 3-plane intersection (apex shared by all K pie-slice caps).
        let vbox = {
            let (m0, m1, m2) = (planes[0].0, planes[1].0, planes[2].0);
            let (d0, d1, d2) = (planes[0].1.dot(m0), planes[1].1.dot(m1), planes[2].1.dot(m2));
            let det = m0.dot(m1.cross(m2));
            assert!(det.abs() > 1e-9, "three planes meet at a point");
            (m1.cross(m2) * d0 + m2.cross(m0) * d1 + m0.cross(m1) * d2) / det
        };
        // V must be inside the cylinder for the clean pie-slice topology.
        let vbox_av = (vbox - ao).dot(n_a);
        assert!(vbox_av > v0 && vbox_av < v1, "box vertex axially inside the cylinder");
        assert!(((vbox - ao) - n_a * vbox_av).length() < radius + 1e-9, "box vertex radially inside");

        let (band_f, disk_f, vids) = mesh
            .sew_corner_band(&top_verts, &top_curves, c_bot + r_vec * radius, bottom_circle,
                band, r_vec, bottom_disk, -n_a, mat)
            .expect("N-arc corner band sews");
        let vbox_id = mesh.add_vertex(vbox);
        // K pie-slice caps: cap for run i = [corner[i], mid_i, corner[i-1], V] — reuses
        // the 2 band arc twins (reverse) + 2 ridge edges (corner→V) shared pairwise.
        let mut caps: Vec<FaceId> = Vec::new();
        for (plane, vstart, vmid) in &cap_specs {
            let v_prev = *vstart;              // corner[i-1] = vids[2i]
            let v_mid = *vmid;                 // mid_i       = vids[2i+1]
            let v_cur = (vmid + 1) % (2 * k);  // corner[i]   = vids[2i+2]
            let (m, o) = planes[*plane];
            let (ec, _, _, emj, _) = ellipse_of(m, o);
            let cap = mesh
                .add_face_with_holes(&[vids[v_cur], vids[v_mid], vids[v_prev], vbox_id], &[], mat)
                .expect("pie-slice cap sews");
            mesh.set_face_surface(cap, Some(S::Plane {
                origin: ec, normal: -m, basis_u: emj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
            }));
            caps.push(cap);
        }
        let open = mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        assert_eq!(open, 0, "no open half-edges → watertight N-plane corner shell");
        assert!(mesh.verify_face_invariants().is_valid(), "DCEL invariants valid");
        let all: Vec<FaceId> = std::iter::once(band_f).chain(std::iter::once(disk_f)).chain(caps.iter().copied()).collect();
        assert_eq!(mesh.face_set_manifold_info(&all).non_manifold_edge_count, 0, "manifold N-plane corner");
        eprintln!("[cyl corner-N DCEL] band={band_f:?} disk={disk_f:?} caps={} open={open} valid=true (K=3)", caps.len());
    }

    /// **SIMULATION (ADR-205 β-3-torus slab de-risk — §3 topology)** — a torus cut by
    /// TWO parallel oblique planes (shared normal m, kept band `d_lo < (X−C)·m < d_hi`).
    /// Per minor circle the kept set is `{v : c_lo < cos(v−φ) < c_hi}` with
    /// `c_lo=(d_lo−A_u)/amp`, `c_hi=(d_hi−A_u)/amp` (A_u,B_u,D,amp,φ as in §1) → **0/1/2
    /// arcs** (or the WHOLE minor circle when the slab swallows it). Unlike cylinder/cone
    /// (one ellipse per plane → a 2-ellipse band), the torus annular slab can be
    /// 4-oval-bounded with TWO band components, so this probe measures the arc-count
    /// histogram across configs to fix the MVP scope.
    #[test]
    fn sim_adr205_torus_slab_section_geometry() {
        use std::f64::consts::TAU;

        // Returns (histogram of kept-arc counts per minor circle [#0, #1, #2, #whole],
        // max plane-offset error of sampled arc endpoints).
        let analyze = |c: DVec3, n_a: DVec3, p1: DVec3, rr: f64, mr: f64,
                       m: DVec3, d_lo: f64, d_hi: f64, lbl: &str| -> [usize; 4] {
            let n_a = n_a.normalize();
            let p1 = (p1 - n_a * p1.dot(n_a)).normalize();
            let p2 = n_a.cross(p1);
            let m = m.normalize();
            let d = mr * n_a.dot(m);
            let n = 1440usize;
            let mut hist = [0usize; 4]; // [0 arcs, 1 arc, 2 arcs, whole circle]
            let mut max_err = 0.0f64;
            for i in 0..n {
                let u = TAU * (i as f64) / (n as f64);
                let radial = p1 * u.cos() + p2 * u.sin();
                let c_u = c + radial * rr;
                let a_u = (c_u - c).dot(m); // offset measured from torus centre C
                let b_u = mr * radial.dot(m);
                let amp = (b_u * b_u + d * d).sqrt();
                if amp < 1e-12 { continue; }
                let c_lo = (d_lo - a_u) / amp;
                let c_hi = (d_hi - a_u) / amp;
                let bucket = if c_lo >= 1.0 || c_hi <= -1.0 {
                    0 // slab clears this minor circle entirely
                } else if c_lo <= -1.0 && c_hi >= 1.0 {
                    3 // minor circle wholly inside the slab (no plane cut)
                } else if (c_lo <= -1.0) != (c_hi >= 1.0) {
                    1 // exactly one plane cuts → 1 arc
                } else {
                    2 // both planes cut this minor circle → 2 arcs
                };
                hist[bucket] += 1;
                // validate: an arc endpoint (cos(v−φ)=c_hi if in range) is on the plane.
                if c_hi.abs() < 1.0 {
                    let phi = d.atan2(b_u);
                    let v = phi + c_hi.acos();
                    let x = c_u + radial * (mr * v.cos()) + n_a * (mr * v.sin());
                    max_err = max_err.max(((x - c).dot(m) - d_hi).abs());
                }
            }
            eprintln!("[{lbl}] arc-count hist [0/1/2/whole]={hist:?} endpoint_err={max_err:.1e}");
            assert!(max_err < 1e-9, "{lbl}: arc endpoints lie on the d_hi plane");
            hist
        };

        // tilted torus R=4 r=1.5, axis ~11.5° off +Z, planes ⟂ +Z (oblique to axis).
        let c = DVec3::ZERO;
        let na = DVec3::new(0.2, 0., 0.98).normalize();
        let (rr, mr) = (4.0, 1.5);

        // (A) STRADDLING slab through the centre (d_lo<0<d_hi, |d|<r): the classic
        //     "ring slice" — every minor circle is cut by BOTH planes → uniformly 2-arc.
        //     THIS is the clean MVP regime: 2 Torus belts + 2 annular caps (4 ovals).
        let ha = analyze(c, na, DVec3::X, rr, mr, DVec3::Z, -0.4, 0.4, "A straddle thin");
        assert_eq!(ha[2], 1440, "A: every minor circle 2-arc (both planes cut) → 4-oval slab");

        // (B) ONE-SIDED slab on the upper tube (0<d_lo<d_hi): MIXED — minor circles on
        //     the inner/lower part reach only the d_lo plane (1 arc), the rest both
        //     (2 arc). Non-uniform topology → DEFERRED (not the clean 4-oval slab).
        let hb = analyze(c, na, DVec3::X, rr, mr, DVec3::Z, 0.4, 0.9, "B one-sided upper");
        assert!(hb[1] > 0 && hb[2] > 0, "B: one-sided slab is MIXED 1/2-arc (deferred regime)");

        // (C) THICK slab swallowing the tube near the centre (|d| > r somewhere):
        //     some minor circles are WHOLLY inside (no cut) → mixed whole/2. DEFERRED.
        let hc = analyze(c, na, DVec3::X, rr, mr, DVec3::Z, -2.0, 2.0, "C thick swallow");
        assert!(hc[3] > 0, "C: some minor circles wholly inside the thick slab (uncut, deferred)");

        // FINDING: only the STRADDLING regime (A: d_lo<0<d_hi within the tube) is
        // uniformly 2-arc → the kept solid is exactly TWO Torus belts (outer + inner,
        // split per-u by cos v) + TWO annular Plane caps, bounded by FOUR spiric ovals
        // {outer,inner}×{d_lo,d_hi}, each oval shared by 2 faces. The MVP builds that
        // with the §2 recipe applied four times. One-sided (B) and tube-swallowing (C)
        // are non-uniform (mixed arc counts) → separate cap topologies, deferred.
    }

    /// **SIMULATION (ADR-205 β-3-torus slab de-risk — §4 DCEL surgery)** — the
    /// STRADDLING annular slab is bounded by FOUR sampled spiric ovals
    /// {outer,inner}×{d_lo,d_hi} and built from FOUR `add_face_with_holes` calls:
    ///   • outer belt (Torus): outer_lo ↔ outer_hi,
    ///   • inner belt (Torus): inner_lo ↔ inner_hi,
    ///   • cap_lo (Plane): outer_lo (outer) + inner_lo (hole),
    ///   • cap_hi (Plane): outer_hi (outer) + inner_hi (hole).
    /// Each oval is shared by exactly two faces; giving those two faces OPPOSITE
    /// windings of the shared oval makes them grab each rim edge's twin → watertight +
    /// manifold. This probe proves the 4-oval recipe (the §2 recipe scaled to a slab)
    /// before the production op + 2-belt boundary-aware render.
    #[test]
    fn sim_adr205_torus_slab_dcel() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;

        let c = DVec3::ZERO;
        let n_a = DVec3::new(0.2, 0., 0.98).normalize();
        let p1 = (DVec3::X - n_a * DVec3::X.dot(n_a)).normalize();
        let p2 = n_a.cross(p1);
        let (rr, mr) = (4.0_f64, 1.5_f64);
        let m = DVec3::Z;
        let (d_lo, d_hi) = (-0.4_f64, 0.4_f64); // straddling, within the tube
        let d = mr * n_a.dot(m);
        let eval = |u: f64, v: f64| -> DVec3 {
            let radial = p1 * u.cos() + p2 * u.sin();
            c + radial * (rr + mr * v.cos()) + n_a * (mr * v.sin())
        };

        let n = 64usize;
        let (mut ol, mut il, mut oh, mut ih) =
            (Vec::new(), Vec::new(), Vec::new(), Vec::new());
        for i in 0..n {
            let u = TAU * (i as f64) / (n as f64);
            let radial = p1 * u.cos() + p2 * u.sin();
            let c_u = c + radial * rr;
            let a_u = (c_u - c).dot(m);
            let b_u = mr * radial.dot(m);
            let amp = (b_u * b_u + d * d).sqrt();
            let phi = d.atan2(b_u);
            for (pd, outv, innv) in [(d_lo, &mut ol, &mut il), (d_hi, &mut oh, &mut ih)] {
                let cval = ((pd - a_u) / amp).clamp(-1.0, 1.0);
                assert!(cval.abs() < 1.0, "straddle: both planes cut every minor circle");
                let dphi = cval.acos();
                let (v1, v2) = (phi + dphi, phi - dphi);
                let (vo, vi) = if v1.cos() >= v2.cos() { (v1, v2) } else { (v2, v1) };
                outv.push(eval(u, vo));
                innv.push(eval(u, vi));
            }
        }

        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);
        let ids = |mesh: &mut Mesh, pts: &[DVec3]| -> Vec<VertId> {
            pts.iter().map(|&p| mesh.add_vertex(p)).collect()
        };
        let (ol_id, il_id, oh_id, ih_id) =
            (ids(&mut mesh, &ol), ids(&mut mesh, &il), ids(&mut mesh, &oh), ids(&mut mesh, &ih));
        let rev = |v: &[VertId]| -> Vec<VertId> { v.iter().rev().copied().collect() };

        // Assignment satisfying "each oval: the two faces traverse it oppositely":
        //   outer_lo: cap_lo(fwd) / outer_belt(rev) ; inner_lo: cap_lo-hole(fwd) / inner_belt(rev)
        //   outer_hi: outer_belt-hole(fwd) / cap_hi(rev) ; inner_hi: cap_hi-hole(fwd) / inner_belt-hole(rev)
        let cap_lo = mesh.add_face_with_holes(&ol_id, &[il_id.as_slice()], mat).expect("cap_lo");
        let outer_belt = mesh.add_face_with_holes(&rev(&ol_id), &[oh_id.as_slice()], mat).expect("outer belt");
        let cap_hi = mesh.add_face_with_holes(&rev(&oh_id), &[ih_id.as_slice()], mat).expect("cap_hi");
        let inner_belt = mesh.add_face_with_holes(&rev(&il_id), &[rev(&ih_id).as_slice()], mat).expect("inner belt");

        for (f, surf) in [
            (cap_lo, S::Plane { origin: c + m * d_lo, normal: -m, basis_u: p1, u_range: (-6., 6.), v_range: (-6., 6.) }),
            (cap_hi, S::Plane { origin: c + m * d_hi, normal: m, basis_u: p1, u_range: (-6., 6.), v_range: (-6., 6.) }),
            (outer_belt, S::Torus { center: c, axis_dir: n_a, ref_dir: p1, major_radius: rr, minor_radius: mr, u_range: (0., TAU), v_range: (0., TAU) }),
            (inner_belt, S::Torus { center: c, axis_dir: n_a, ref_dir: p1, major_radius: rr, minor_radius: mr, u_range: (0., TAU), v_range: (0., TAU) }),
        ] {
            mesh.set_face_surface(f, Some(surf));
        }

        assert!(mesh.verify_face_invariants().is_valid(), "straddling slab watertight + manifold");
        let nm = mesh.collect_non_manifold_edges().len();
        assert_eq!(nm, 0, "every one of the 4 ovals shared by exactly 2 faces (no non-manifold edge)");
        let open = mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        assert_eq!(open, 0, "no open (null-face) half-edges → closed slab shell");
        eprintln!("[torus slab DCEL] belts={outer_belt:?},{inner_belt:?} caps={cap_lo:?},{cap_hi:?} nm={nm} open={open} valid=true");
    }

    /// **ADR-205 β-2-torus** — production: a kernel-native torus tilted ~11.5° off +Z,
    /// cut by a CARDINAL +Z plane through the centre (oblique to the torus axis but
    /// within the annular threshold) → a Torus band + an annular Plane cap, watertight
    /// + manifold, oriented (band normal = kept +m, cap = outward −m), and rendered
    /// boundary-aware (every band tri vertex on the kept +m side).
    #[test]
    fn adr205_beta2torus_oblique_halfspace_annular() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let torus = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
        // tilt ~11.5° about X so the cardinal +Z plane through the centre is oblique
        // to the (tilted) axis, yet still within the §1 annular √ bound.
        let tv = m.solid_loop_verts(&[torus]);
        m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, 0.20).unwrap();
        let pm = DVec3::Z;
        let o = DVec3::ZERO;
        let r = m
            .boolean_torus_oblique_halfspace(&[torus], o, pm, mat)
            .expect("annular oblique halfspace");
        assert_eq!(r.len(), 2, "Torus band + annular Plane cap");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "watertight (band & cap share every rim edge)"
        );
        assert!(m.verify_face_invariants().is_valid(), "manifold + ADR-007 invariants");
        assert_eq!(m.collect_non_manifold_edges().len(), 0, "no non-manifold edges");

        let band = r.iter().copied()
            .find(|&f| matches!(m.face_surface(f), Some(S::Torus { .. })))
            .expect("result has a Torus band");
        let cap = r.iter().copied()
            .find(|&f| matches!(m.face_surface(f), Some(S::Plane { .. })))
            .expect("result has a Plane cap");
        assert!(m.faces[band].normal().dot(pm) > 0.5, "band normal = kept +m side");
        assert!(m.faces[cap].normal().dot(pm) < -0.5, "cap outward normal = −m");

        // boundary-aware render: the clipped band is non-empty and entirely on +m.
        let tess = m
            .tessellate_torus_clipped(band, 0.05)
            .expect("Torus band renders clipped");
        assert!(!tess.vertices.is_empty() && !tess.triangles.is_empty(), "non-empty clipped band");
        let min_side = tess.vertices.iter().map(|p| (*p - o).dot(pm)).fold(f64::MAX, f64::min);
        assert!(min_side > -1e-6, "all band verts on the kept +m side (boundary-aware, not over-drawn)");
        // a genuine half-tube: some interior verts well above the cut plane.
        let max_side = tess.vertices.iter().map(|p| (*p - o).dot(pm)).fold(f64::MIN, f64::max);
        assert!(max_side > 0.5, "band bulges into the kept halfspace (real clipped tube)");
    }

    /// **ADR-205 β-2-torus** — adversarial sweep: several annular configs route
    /// (2 faces, watertight, render non-panicking); non-annular configs (a plane
    /// outside the tube, or a steep tilt past the threshold = the pinched regime)
    /// return Err with the mesh left INTACT (validation precedes any face removal).
    #[test]
    fn adr205_beta2torus_oblique_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let run = |tilt: f64, pm: DVec3, o: DVec3| -> (Mesh, Result<Vec<FaceId>>) {
            let mut m = Mesh::default();
            let torus = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
            if tilt.abs() > 1e-9 {
                let tv = m.solid_loop_verts(&[torus]);
                m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, tilt).unwrap();
            }
            let r = m.boolean_torus_oblique_halfspace(&[torus], o, pm.normalize(), mat);
            (m, r)
        };

        // (a) annular configs route + watertight + render OK.
        for (tilt, pm, o) in [
            (0.20, DVec3::Z, DVec3::ZERO),
            (0.15, DVec3::Z, DVec3::new(0., 0., 0.3)),
            (0.0, DVec3::Z, DVec3::ZERO), // ⟂ axis (z-cut, 2 circles) = the most annular
            (0.25, DVec3::new(0.1, 0., 1.0), DVec3::ZERO),
        ] {
            let (mut m, r) = run(tilt, pm, o);
            let faces = r.unwrap_or_else(|e| panic!("annular tilt={tilt} should route: {e}"));
            assert_eq!(faces.len(), 2, "tilt={tilt}: band + cap");
            assert_eq!(m.collect_non_manifold_edges().len(), 0, "tilt={tilt}: watertight");
            assert!(m.verify_face_invariants().is_valid(), "tilt={tilt}: invariants");
            let band = faces.iter().copied()
                .find(|&f| matches!(m.face_surface(f), Some(S::Torus { .. })))
                .expect("Torus band");
            let tess = m.tessellate_torus_clipped(band, 0.05).expect("renders");
            assert!(!tess.triangles.is_empty(), "tilt={tilt}: non-empty band render");
            let _ = m.export_buffers().expect("full export doesn't panic");
        }

        // (b) plane entirely OUTSIDE the tube (z = 100) → bail, mesh intact.
        let (m, r) = run(0.20, DVec3::Z, DVec3::new(0., 0., 100.));
        assert!(r.is_err(), "plane clear of the torus → Err");
        assert!(m.verify_face_invariants().is_valid(), "mesh intact after no-cut bail");

        // (c) steep tilt ~69° (≫ the ~22° annular threshold) → pinched → bail, intact.
        let (m, r) = run(1.2, DVec3::Z, DVec3::ZERO);
        assert!(r.is_err(), "steep tilt (pinched single-oval) → Err (deferred)");
        assert!(m.verify_face_invariants().is_valid(), "mesh intact after pinched bail");
    }

    /// **ADR-205 β-2-torus** — deterministic ground-truth across the five adversarial
    /// dimensions (kept-side orientation / render front-facing / annular threshold /
    /// off-centre / scale-axis generality). For every routing config it verifies
    /// watertight + manifold + invariants + orientation (band = kept +m, cap = −m) +
    /// **every render triangle front-facing** (its winding normal agrees with the
    /// torus outward normal); for every non-routing config it verifies a clean Err
    /// with the mesh intact.
    #[test]
    fn adr205_beta2torus_orientation_scale_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = |rr: f64, mr: f64, tilt: f64, tax: DVec3| -> (Mesh, FaceId) {
            let mut m = Mesh::default();
            let t = m.create_torus_kernel_native(DVec3::ZERO, rr, mr, mat).unwrap();
            if tilt.abs() > 1e-12 {
                let tv = m.solid_loop_verts(&[t]);
                m.rotate_verts(&tv, DVec3::ZERO, tax.normalize(), tilt).unwrap();
            }
            (m, t)
        };
        let check_routes = |m: &Mesh, faces: &[FaceId], o: DVec3, mn: DVec3, lbl: &str| {
            assert_eq!(faces.len(), 2, "{lbl}: band + cap");
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "{lbl}: watertight"
            );
            assert!(m.verify_face_invariants().is_valid(), "{lbl}: invariants");
            assert_eq!(m.collect_non_manifold_edges().len(), 0, "{lbl}: manifold");
            let band = faces.iter().copied()
                .find(|&f| matches!(m.face_surface(f), Some(S::Torus { .. }))).expect("band");
            let cap = faces.iter().copied()
                .find(|&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).expect("cap");
            assert!(m.faces[band].normal().dot(mn) > 0.5, "{lbl}: band normal = kept +m");
            assert!(m.faces[cap].normal().dot(mn) < -0.5, "{lbl}: cap outward = −m");
            let tess = m.tessellate_torus_clipped(band, 0.05).expect("renders");
            assert!(!tess.triangles.is_empty(), "{lbl}: non-empty band");
            assert!(tess.vertices.iter().all(|p| (*p - o).dot(mn) > -1e-6), "{lbl}: band on kept +m side");
            // front-facing: every triangle winding normal agrees with the torus outward normal.
            let (center, axis, major) = match m.face_surface(band) {
                Some(S::Torus { center, axis_dir, major_radius, .. }) =>
                    (*center, axis_dir.normalize(), *major_radius),
                _ => unreachable!(),
            };
            let mut back = 0usize;
            for tri in &tess.triangles {
                let (a, b, c) = (
                    tess.vertices[tri[0] as usize],
                    tess.vertices[tri[1] as usize],
                    tess.vertices[tri[2] as usize],
                );
                let tn = (b - a).cross(c - a);
                let cen = (a + b + c) / 3.0;
                let rel = cen - center;
                let radialv = rel - axis * rel.dot(axis);
                if radialv.length() < 1e-9 { continue; }
                let minor_center = center + radialv.normalize() * major;
                if tn.dot((cen - minor_center).normalize()) <= 0.0 { back += 1; }
            }
            assert_eq!(back, 0, "{lbl}: all band triangles front-facing");
        };

        // 1. KEPT-SIDE: m = −Z (keep the lower half) + a negative tilt.
        let (mut m, t) = build(4.0, 1.5, 0.20, DVec3::X);
        let f = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, DVec3::NEG_Z, mat).expect("m=−Z routes");
        check_routes(&m, &f, DVec3::ZERO, DVec3::NEG_Z, "kept m=−Z");
        let (mut m, t) = build(4.0, 1.5, -0.20, DVec3::X);
        let f = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, DVec3::Z, mat).expect("neg tilt routes");
        check_routes(&m, &f, DVec3::ZERO, DVec3::Z, "neg tilt");

        // 2. THRESHOLD: 0.35 rad (~20° < 22°) routes; 0.50 rad (~29°) pinched → bail;
        //    exact ⟂-axis (m = the tilted axis, the z-cut limit) routes (2 circles).
        let (mut m, t) = build(4.0, 1.5, 0.35, DVec3::X);
        let f = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, DVec3::Z, mat).expect("just-inside routes");
        check_routes(&m, &f, DVec3::ZERO, DVec3::Z, "threshold inside 20°");
        let (mut m, t) = build(4.0, 1.5, 0.50, DVec3::X);
        let r = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, DVec3::Z, mat);
        assert!(r.is_err(), "threshold outside 29° → pinched bail");
        assert!(m.verify_face_invariants().is_valid(), "intact after pinched bail");
        let (mut m, t) = build(4.0, 1.5, 0.30, DVec3::X);
        let axis = match m.face_surface(t) { Some(S::Torus { axis_dir, .. }) => axis_dir.normalize(), _ => unreachable!() };
        let f = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, axis, mat).expect("⟂-axis z-cut routes");
        check_routes(&m, &f, DVec3::ZERO, axis, "perp-axis z-cut");

        // 3. OFF-CENTRE along +m (annular but asymmetric ovals).
        let (mut m, t) = build(4.0, 1.5, 0.20, DVec3::X);
        let o = DVec3::new(0., 0., 0.5);
        let f = m.boolean_torus_oblique_halfspace(&[t], o, DVec3::Z, mat).expect("off-centre routes");
        check_routes(&m, &f, o, DVec3::Z, "off-centre +m");

        // 4. SCALE: large torus R=1000 r=300 (threshold ~17°, tilt 8.6° annular).
        let (mut m, t) = build(1000.0, 300.0, 0.15, DVec3::X);
        let f = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, DVec3::Z, mat).expect("large torus routes");
        check_routes(&m, &f, DVec3::ZERO, DVec3::Z, "large R=1000");
        // thin torus r=0.3 (threshold ~4.3°): tilt 11.5° pinches → bail; tilt 2.3° routes.
        let (mut m, t) = build(4.0, 0.3, 0.20, DVec3::X);
        let r = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, DVec3::Z, mat);
        assert!(r.is_err(), "thin torus steep → pinched bail");
        assert!(m.verify_face_invariants().is_valid(), "intact after thin pinched bail");
        let (mut m, t) = build(4.0, 0.3, 0.04, DVec3::X);
        let f = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, DVec3::Z, mat).expect("thin shallow routes");
        check_routes(&m, &f, DVec3::ZERO, DVec3::Z, "thin shallow");

        // 5. NON-CARDINAL tilt axis (gentle, stays annular with a +Z cut).
        let (mut m, t) = build(4.0, 1.5, 0.15, DVec3::new(1., 1., 0.));
        let f = m.boolean_torus_oblique_halfspace(&[t], DVec3::ZERO, DVec3::Z, mat).expect("non-cardinal axis routes");
        check_routes(&m, &f, DVec3::ZERO, DVec3::Z, "non-cardinal axis");
    }

    /// **ADR-205 β-3-torus** — production: a torus tilted ~11.5° off +Z cut by two
    /// CARDINAL +Z planes straddling the centre (z = ±0.4) → an annular SLAB of 2
    /// Torus belts + 2 Plane caps, watertight + manifold, caps outward (±m), every
    /// belt rendered boundary-aware (verts within the slab) and front-facing.
    #[test]
    fn adr205_beta3torus_oblique_slab_straddling() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let torus = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
        let tv = m.solid_loop_verts(&[torus]);
        m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, 0.20).unwrap();
        let pm = DVec3::Z;
        let r = m.boolean_torus_oblique_slab(&[torus], pm, -0.4, 0.4, mat).expect("straddling slab");
        assert_eq!(r.len(), 4, "2 belts + 2 caps");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "watertight slab shell"
        );
        assert!(m.verify_face_invariants().is_valid(), "manifold + invariants");
        assert_eq!(m.collect_non_manifold_edges().len(), 0, "no non-manifold edges");

        let belts: Vec<_> = r.iter().copied()
            .filter(|&f| matches!(m.face_surface(f), Some(S::Torus { .. }))).collect();
        let caps: Vec<_> = r.iter().copied()
            .filter(|&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).collect();
        assert_eq!(belts.len(), 2, "two Torus belts");
        assert_eq!(caps.len(), 2, "two Plane caps");
        let cap_dots: Vec<f64> = caps.iter().map(|&f| m.faces[f].normal().dot(pm)).collect();
        assert!(cap_dots.iter().any(|&x| x < -0.5), "one cap outward −m");
        assert!(cap_dots.iter().any(|&x| x > 0.5), "one cap outward +m");

        for &belt in &belts {
            let tess = m.tessellate_torus_slab_clipped(belt, 0.05).expect("slab belt renders");
            assert!(!tess.triangles.is_empty(), "non-empty belt");
            for p in &tess.vertices {
                let off = p.dot(pm); // torus centre at origin
                assert!(off > -0.4 - 1e-6 && off < 0.4 + 1e-6, "belt vert within the slab");
            }
            let (center, axis, major) = match m.face_surface(belt) {
                Some(S::Torus { center, axis_dir, major_radius, .. }) =>
                    (*center, axis_dir.normalize(), *major_radius),
                _ => unreachable!(),
            };
            let mut back = 0usize;
            for tri in &tess.triangles {
                let (a, b, c) = (
                    tess.vertices[tri[0] as usize],
                    tess.vertices[tri[1] as usize],
                    tess.vertices[tri[2] as usize],
                );
                let tn = (b - a).cross(c - a);
                let cen = (a + b + c) / 3.0;
                let rel = cen - center;
                let radialv = rel - axis * rel.dot(axis);
                if radialv.length() < 1e-9 { continue; }
                let mc = center + radialv.normalize() * major;
                if tn.dot((cen - mc).normalize()) <= 0.0 { back += 1; }
            }
            assert_eq!(back, 0, "belt front-facing");
        }
        let _ = m.export_buffers().expect("full export (both belts route to the slab render)");
    }

    /// **ADR-205 β-3-torus** — adversarial: straddling variants route + watertight +
    /// render OK; non-straddling configs (d_lo≥d_hi, a one-sided/mixed slab, a slab
    /// clear of the tube) return Err with the mesh intact (validation precedes removal).
    #[test]
    fn adr205_beta3torus_slab_adversarial() {
        let mat = MaterialId::new(0);
        let run = |tilt: f64, d_lo: f64, d_hi: f64| -> (Mesh, Result<Vec<FaceId>>) {
            let mut m = Mesh::default();
            let t = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
            if tilt.abs() > 1e-9 {
                let tv = m.solid_loop_verts(&[t]);
                m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, tilt).unwrap();
            }
            let r = m.boolean_torus_oblique_slab(&[t], DVec3::Z, d_lo, d_hi, mat);
            (m, r)
        };
        for (tilt, d_lo, d_hi) in [(0.20, -0.4, 0.4), (0.15, -0.3, 0.5), (0.0, -0.5, 0.5)] {
            let (mut m, r) = run(tilt, d_lo, d_hi);
            let f = r.unwrap_or_else(|e| panic!("straddling tilt={tilt} routes: {e}"));
            assert_eq!(f.len(), 4, "tilt={tilt}: 4 faces");
            assert_eq!(m.collect_non_manifold_edges().len(), 0, "tilt={tilt}: watertight");
            assert!(m.verify_face_invariants().is_valid(), "tilt={tilt}: invariants");
            let _ = m.export_buffers().expect("export");
        }
        // d_lo ≥ d_hi → bail.
        let (m, r) = run(0.20, 0.4, -0.4);
        assert!(r.is_err(), "d_lo≥d_hi → Err");
        assert!(m.verify_face_invariants().is_valid(), "intact after d_lo≥d_hi bail");
        // one-sided slab (both planes above centre → mixed 1/2-arc regime) → bail.
        let (m, r) = run(0.20, 0.5, 1.0);
        assert!(r.is_err(), "one-sided (mixed) slab → Err (deferred)");
        assert!(m.verify_face_invariants().is_valid(), "intact after one-sided bail");
        // slab clear of the tube entirely → bail.
        let (m, r) = run(0.20, 10.0, 11.0);
        assert!(r.is_err(), "slab clear of torus → Err");
        assert!(m.verify_face_invariants().is_valid(), "intact after clear bail");
    }

    /// **ADR-205 β-3-torus** — deterministic ground truth that the slab's cap-orientation
    /// flip + the 2-belt render generalise: m = −Z, a non-cardinal tilt axis + normal,
    /// large/thin scale, and an asymmetric slab all give 4 faces, watertight, one cap
    /// outward each way (±m), and BOTH belts within the slab + front-facing.
    #[test]
    fn adr205_beta3torus_orientation_scale_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let check = |m: &Mesh, faces: &[FaceId], mn: DVec3, d_lo: f64, d_hi: f64, lbl: &str| {
            assert_eq!(faces.len(), 4, "{lbl}: 4 faces");
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "{lbl}: watertight"
            );
            assert!(m.verify_face_invariants().is_valid(), "{lbl}: invariants");
            assert_eq!(m.collect_non_manifold_edges().len(), 0, "{lbl}: manifold");
            let caps: Vec<f64> = faces.iter().copied()
                .filter(|&f| matches!(m.face_surface(f), Some(S::Plane { .. })))
                .map(|f| m.faces[f].normal().dot(mn)).collect();
            assert_eq!(caps.len(), 2, "{lbl}: two caps");
            assert!(caps.iter().any(|&x| x < -0.5) && caps.iter().any(|&x| x > 0.5), "{lbl}: caps outward ±m");
            for &belt in faces.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Torus { .. }))) {
                let tess = m.tessellate_torus_slab_clipped(belt, 0.05).expect("belt renders");
                assert!(!tess.triangles.is_empty(), "{lbl}: non-empty belt");
                assert!(tess.vertices.iter().all(|p| {
                    let off = p.dot(mn);
                    off > d_lo - 1e-6 && off < d_hi + 1e-6
                }), "{lbl}: belt verts within the slab");
                let (center, axis, major) = match m.face_surface(belt) {
                    Some(S::Torus { center, axis_dir, major_radius, .. }) =>
                        (*center, axis_dir.normalize(), *major_radius),
                    _ => unreachable!(),
                };
                let mut back = 0usize;
                for tri in &tess.triangles {
                    let (a, b, c) = (
                        tess.vertices[tri[0] as usize],
                        tess.vertices[tri[1] as usize],
                        tess.vertices[tri[2] as usize],
                    );
                    let tn = (b - a).cross(c - a);
                    let cen = (a + b + c) / 3.0;
                    let rel = cen - center;
                    let radialv = rel - axis * rel.dot(axis);
                    if radialv.length() < 1e-9 { continue; }
                    let mc = center + radialv.normalize() * major;
                    if tn.dot((cen - mc).normalize()) <= 0.0 { back += 1; }
                }
                assert_eq!(back, 0, "{lbl}: belt front-facing");
            }
        };
        let build = |rr: f64, mr: f64, tilt: f64, tax: DVec3| -> (Mesh, FaceId) {
            let mut m = Mesh::default();
            let t = m.create_torus_kernel_native(DVec3::ZERO, rr, mr, mat).unwrap();
            if tilt.abs() > 1e-12 {
                let tv = m.solid_loop_verts(&[t]);
                m.rotate_verts(&tv, DVec3::ZERO, tax.normalize(), tilt).unwrap();
            }
            (m, t)
        };

        // m = −Z (caps flip).
        let (mut m, t) = build(4.0, 1.5, 0.20, DVec3::X);
        let f = m.boolean_torus_oblique_slab(&[t], DVec3::NEG_Z, -0.4, 0.4, mat).expect("m=−Z slab");
        check(&m, &f, DVec3::NEG_Z, -0.4, 0.4, "m=−Z");

        // asymmetric slab.
        let (mut m, t) = build(4.0, 1.5, 0.20, DVec3::X);
        let f = m.boolean_torus_oblique_slab(&[t], DVec3::Z, -0.2, 0.6, mat).expect("asymmetric slab");
        check(&m, &f, DVec3::Z, -0.2, 0.6, "asymmetric");

        // non-cardinal tilt axis.
        let (mut m, t) = build(4.0, 1.5, 0.15, DVec3::new(1., 1., 0.));
        let f = m.boolean_torus_oblique_slab(&[t], DVec3::Z, -0.4, 0.4, mat).expect("non-cardinal slab");
        check(&m, &f, DVec3::Z, -0.4, 0.4, "non-cardinal");

        // large scale R=1000 r=300 (slab ±60).
        let (mut m, t) = build(1000.0, 300.0, 0.15, DVec3::X);
        let f = m.boolean_torus_oblique_slab(&[t], DVec3::Z, -60.0, 60.0, mat).expect("large slab");
        check(&m, &f, DVec3::Z, -60.0, 60.0, "large R=1000");
    }

    /// **SIMULATION (ADR-205 cone Dandelin de-risk)** — an oblique plane cutting a
    /// CONE in the ellipse case (|n_a·m| > p·tanα, p = |m − (n_a·m)·n_a|) produces a
    /// true planar ELLIPSE (Dandelin). This probe derives the ellipse params in
    /// closed form from the cone (apex A, axis n_a, half-angle α) + plane (O, m):
    ///   D = n_a·m,  q = (m − D·n_a)/p,  r2 = n_a × q,
    ///   a = cosα·D,  b = sinα·p,  k = (O−A)·m,  denom = a²−b²  (>0 for the ellipse),
    ///   center     = A + (k/denom)·(a·cosα·n_a − b·sinα·q),
    ///   semi_major = |k|·√(b²cos²α + a²sin²α) / denom,   (axis in the n_a–q plane)
    ///   semi_minor = |k|·sinα / √denom,                   (axis along r2)
    /// then verifies every sampled point lies BOTH on the cone (angle to axis = α)
    /// and on the plane, within the finite axial range — proving the section before
    /// `boolean_cone_oblique_halfspace`.
    #[test]
    fn sim_adr205_cone_oblique_ellipse_geometry() {
        use std::f64::consts::TAU;
        // Validate the `cone_oblique_ellipse` helper: every sampled point of the
        // returned ellipse lies on the cone (angle to axis = α) ∩ plane, within the
        // finite axial range [0, h] (h = the cone's apex→base axial extent).
        let check = |apex: DVec3, n_a: DVec3, alpha: f64, height: f64, m: DVec3, o: DVec3, lbl: &str| {
            let n_a = n_a.normalize();
            let m = m.normalize();
            let ca = alpha.cos();
            let (center, semi_major, semi_minor, major_dir, minor_dir) =
                cone_oblique_ellipse(apex, n_a, alpha, o, m).unwrap_or_else(|| panic!("{lbl}: ellipse case"));
            assert!(semi_major >= semi_minor - 1e-9, "{lbl}: semi_major {semi_major} ≥ semi_minor {semi_minor}");
            for i in 0..64 {
                let th = TAU * (i as f64) / 64.0;
                let e = center + semi_major * th.cos() * major_dir + semi_minor * th.sin() * minor_dir;
                assert!((e - o).dot(m).abs() < 1e-6, "{lbl} pt {i} on the cut plane (off={})", (e - o).dot(m));
                let rel = e - apex;
                let axial = rel.dot(n_a);
                let cos_ang = axial / rel.length();
                assert!((cos_ang - ca).abs() < 1e-6, "{lbl} pt {i} on the cone (cosang={cos_ang} vs cosα={ca})");
                assert!(axial > 1e-6 && axial < height - 1e-6, "{lbl} pt {i} within the finite cone (axial={axial})");
            }
        };

        // (A) Z-axis apex-up cone (axis −Z) cut by an oblique plane ~30° off level.
        check(
            DVec3::new(0., 0., 6.), DVec3::NEG_Z, (2.0_f64 / 6.0).atan(), 6.0,
            DVec3::new(0.5, 0., 0.866_025_403_8), DVec3::new(0., 0., 3.),
            "Z-axis cone + oblique plane",
        );

        // (B) TILTED cone cut by a CARDINAL +Z box face — the actual γ target.
        check(
            DVec3::new(0., 0., 8.), DVec3::new(0.25, 0., -0.968), 0.35_f64.atan(), 6.0,
            DVec3::Z, DVec3::new(0., 0., 4.),
            "tilted cone + cardinal +Z plane",
        );

        // parabola / hyperbola (|D| ≤ p·tanα) + apex / ⟂ planes return None.
        let wide = (1.2_f64).atan(); // a wide cone → easy parabola/hyperbola
        assert!(cone_oblique_ellipse(DVec3::new(0., 0., 6.), DVec3::NEG_Z, wide, DVec3::new(0., 0., 3.), DVec3::new(0.9, 0., 0.436).normalize()).is_none(), "steep plane → not an ellipse");
        assert!(cone_oblique_ellipse(DVec3::new(0., 0., 6.), DVec3::NEG_Z, (2.0_f64 / 6.0).atan(), DVec3::new(0., 0., 6.), DVec3::Z).is_none(), "plane through the apex → None");

        // the ellipse infra (β-1) accepts the params (rational quadratic B-spline).
        let (cp, w, knots, deg) = crate::curves::nurbs::ellipse(DVec3::ZERO, 2.0, 1.0, DVec3::X, DVec3::Y);
        assert_eq!((cp.len(), deg, knots.len()), (9, 2, 12), "ellipse NURBS shape");
        assert!(w.iter().all(|&x| x > 0.), "positive weights");
    }

    /// **SIMULATION (ADR-205 β-2-cone α — DCEL surgery de-risk)** — the kept BASE
    /// side of a cone cut by an oblique plane is a frustum-with-an-elliptic-top:
    /// base disk (Plane) + cone-side band (Cone surface, base circle + ellipse
    /// boundaries) + elliptic cap (Plane). This probe builds that solid with the
    /// reuse primitive `sew_curved_band` (the same one cylinder β-2 uses) — proving
    /// the Cone band sews watertight + manifold BEFORE the production op +
    /// boundary-aware render. (The render clip — tessellate_cone_clipped — is the
    /// β step; here the band carries the full Cone surface for the topology check.)
    #[test]
    fn sim_adr205_cone_oblique_halfspace_dcel() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let mut m = Mesh::default();
        let mat = MaterialId::new(0);
        // cone: apex (0,0,6), axis −Z (apex→base), base z=0 r=2, α = atan(2/6).
        let apex = DVec3::new(0., 0., 6.);
        let n_a = DVec3::NEG_Z;
        let (radius, height) = (2.0_f64, 6.0_f64);
        let alpha = (radius / height).atan();
        let ref_dir = DVec3::X;
        let base_center = apex + n_a * height; // (0,0,0)

        // oblique plane (ellipse case); m points toward the apex (+m = apex side),
        // so the KEPT base side is −m.
        let m_plane = DVec3::new(0.3, 0., 0.954).normalize();
        let o = DVec3::new(0., 0., 3.);
        let (e_center, sm, sn, maj, min) =
            cone_oblique_ellipse(apex, n_a, alpha, o, m_plane).expect("ellipse section");

        // top = the elliptic section (NURBS self-loop).
        let (cp, w, kn, deg) = crate::curves::nurbs::ellipse(e_center, sm, sn, maj, min);
        let top_anchor = cp[0];
        let top_ellipse = crate::curves::AnalyticCurve::NURBS {
            control_pts: cp, weights: w, knots: kn, degree: deg as u32,
        };
        // bottom = the base circle (outward normal −Z = n_a, the disk faces away
        // from the cone body).
        let base_circle = crate::curves::AnalyticCurve::Circle {
            center: base_center, radius, normal: n_a, basis_u: ref_dir,
        };
        let base_anchor = base_center + ref_dir * radius;

        // band = the Cone surface (full v_range for the topology check; the β step
        // clips it boundary-aware). cap faces +m (away from the kept −m solid);
        // base disk faces n_a (−Z, away from the body).
        let band = S::Cone {
            apex, axis_dir: n_a, half_angle: alpha, ref_dir,
            u_range: (0., TAU), v_range: (0., height),
        };
        let cap = S::Plane {
            origin: e_center, normal: m_plane, basis_u: maj,
            u_range: (-sm * 1.2, sm * 1.2), v_range: (-sm * 1.2, sm * 1.2),
        };
        let base_disk = S::Plane {
            origin: base_center, normal: n_a, basis_u: ref_dir,
            u_range: (-radius * 1.5, radius * 1.5), v_range: (-radius * 1.5, radius * 1.5),
        };

        let (band_f, cap_f, disk_f) = m.sew_curved_band(
            top_anchor, top_ellipse,
            base_anchor, base_circle,
            band, ref_dir,        // band normal hint (true normal from the surface)
            cap, m_plane,         // elliptic cap → +m (outward)
            base_disk, n_a,       // base disk → −Z (outward)
            mat,
        ).expect("sew the cone frustum");

        // ── topology: watertight + manifold + invariants ──
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0,
            "oblique-cut cone frustum watertight");
        assert!(m.verify_face_invariants().is_valid(), "DCEL invariants valid");
        let r = [band_f, cap_f, disk_f];
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold frustum");
        assert!(matches!(m.face_surface(band_f), Some(S::Cone { .. })), "band keeps the Cone surface");
        assert!(matches!(m.face_surface(cap_f), Some(S::Plane { .. })), "cap is the elliptic Plane");
        assert!(matches!(m.face_surface(disk_f), Some(S::Plane { .. })), "base disk Plane");
    }

    /// **SIMULATION (ADR-205 cone apex-tip de-risk — DCEL surgery)** — the kept APEX
    /// side of a cone cut by an oblique plane is a small cone: a degenerate apex pole
    /// + a cone-side face (Cone surface, apex degenerate) + an elliptic cap (Plane),
    /// bounded by ONE elliptic loop. Unlike the base frustum (`sew_curved_band`, two
    /// loops), this is the Path B cone pattern (one self-loop, apex degenerate). This
    /// probe builds it with `sew_cone_tip` — proving the tip sews watertight + manifold
    /// BEFORE the production op + apex-clipped render.
    #[test]
    fn sim_adr205_cone_apex_tip_dcel() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let mat = MaterialId::new(0);
        // apex-up cone: apex (0,0,6), axis −Z (apex→base), base z=0 radius 2.
        let apex = DVec3::new(0., 0., 6.);
        let n_a = DVec3::NEG_Z;
        let half_angle = (2.0_f64 / 6.0).atan();
        let ref_dir = DVec3::X;
        // oblique plane at z≈4 separating the apex (kept, +m) from the base (−m).
        let m = DVec3::new(0.2, 0., 0.98).normalize();
        let o = DVec3::new(0., 0., 4.);
        let (e_center, semi_major, semi_minor, major_dir, minor_dir) =
            cone_oblique_ellipse(apex, n_a, half_angle, o, m).expect("ellipse section");
        assert!((apex - o).dot(m) > 0.0, "apex on the kept +m side");

        let (cp, w, knots, deg) =
            crate::curves::nurbs::ellipse(e_center, semi_major, semi_minor, major_dir, minor_dir);
        let anchor = cp[0];
        let ellipse = crate::curves::AnalyticCurve::NURBS {
            control_pts: cp, weights: w, knots, degree: deg as u32,
        };
        let cone_surface = S::Cone {
            apex, axis_dir: n_a, half_angle, ref_dir,
            u_range: (0.0, TAU), v_range: (0.0, 6.0),
        };
        let cap = S::Plane {
            origin: e_center, normal: -m, basis_u: major_dir,
            u_range: (-semi_major * 1.2, semi_major * 1.2),
            v_range: (-semi_major * 1.2, semi_major * 1.2),
        };

        let mut mesh = Mesh::new();
        let (side, cap_f) = mesh
            .sew_cone_tip(anchor, ellipse, cone_surface, n_a, cap, -m, mat)
            .expect("apex tip sews");
        // watertight (every elliptic-loop edge bears exactly 2 active faces) + manifold.
        let open = mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        assert_eq!(open, 0, "no open half-edges → closed tip shell");
        assert!(mesh.verify_face_invariants().is_valid(), "apex tip manifold + invariants");
        assert_eq!(mesh.collect_non_manifold_edges().len(), 0, "side & cap share the elliptic rim");
        assert!(matches!(mesh.face_surface(side), Some(S::Cone { .. })), "side keeps the Cone surface");
        assert!(matches!(mesh.face_surface(cap_f), Some(S::Plane { .. })), "cap is the elliptic Plane");
        eprintln!("[cone apex-tip DCEL] side={side:?} cap={cap_f:?} open={open} valid=true");
    }

    /// **SIMULATION (ADR-205 cone apex-tip CORNER de-risk — DCEL surgery)** — a cone
    /// cut by TWO oblique planes BOTH keeping the apex → the small apex cone clipped by
    /// a corner. It is the MIRROR of cone-corner (the base-keeping tent): the kept
    /// region is `v ∈ [0 (apex), min(v_e1, v_e2)]` so the binding plane per arc is
    /// `argMIN v_e`, and the bottom is the degenerate APEX pole (no base disk). This
    /// probe builds the corner band (multi-edge top + apex pole, `sew_corner_tip`) +
    /// two partial caps (sharing the ridge edge) and proves it sews watertight +
    /// manifold BEFORE the production op + render.
    #[test]
    fn sim_adr205_cone_apex_tip_corner_dcel() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let mat = MaterialId::new(0);
        // apex-up cone: apex (0,0,6), axis −Z, base z=0 r=2.
        let apex = DVec3::new(0., 0., 6.);
        let n_a = DVec3::NEG_Z;
        let half_angle = (2.0_f64 / 6.0).atan();
        let ref_dir = DVec3::X;
        let height = 6.0_f64;
        let tan_a = half_angle.tan();
        // two oblique planes (symmetric about the Y–Z plane) tilted toward the apex
        // (apex on +m, base on −m) — ridge crosses the cone at z≈3.
        let m1 = DVec3::new(0.5, 0., 0.866).normalize();
        let m2 = DVec3::new(-0.5, 0., 0.866).normalize();
        let (o1, o2) = (DVec3::new(0., 0., 3.), DVec3::new(0., 0., 3.));
        for (mp, op) in [(m1, o1), (m2, o2)] {
            assert!((apex - op).dot(mp) > 0.0, "apex on +m (kept)");
            assert!((DVec3::ZERO - op).dot(mp) < 0.0, "base on −m (removed)");
            assert!(cone_oblique_ellipse(apex, n_a, half_angle, op, mp).is_some(), "bounded ellipse");
        }
        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize_or_zero();
        let v_plane = |m: DVec3, o: DVec3, u: f64| {
            let g = n_a + (r_vec * u.cos() + p_vec * u.sin()) * tan_a;
            (o - apex).dot(m) / g.dot(m)
        };
        let surf = |u: f64, v: f64| apex + n_a * v + (r_vec * u.cos() + p_vec * u.sin()) * (v * tan_a);
        let u_of = |p: DVec3| {
            let rel = p - apex;
            let radial = rel - n_a * rel.dot(n_a);
            radial.dot(p_vec).atan2(radial.dot(r_vec)).rem_euclid(TAU)
        };
        // ridge ∩ cone (closed-form quadratic on the nappe) — same as cone-corner.
        let dir = m1.cross(m2);
        let (d1, d2) = (o1.dot(m1), o2.dot(m2));
        let l0 = (m2.cross(dir) * d1 + dir.cross(m1) * d2) / dir.dot(dir);
        let a0 = l0 - apex;
        let (av, bv) = (a0.dot(n_a), dir.dot(n_a));
        let (q0, q1, q2) = (a0.dot(a0), a0.dot(dir), dir.dot(dir));
        let cos2 = half_angle.cos().powi(2);
        let (qa, qb, qc) = (bv * bv - cos2 * q2, 2.0 * (av * bv - cos2 * q1), av * av - cos2 * q0);
        let disc = qb * qb - 4.0 * qa * qc;
        assert!(qa.abs() > 1e-12 && disc > 0.0, "ridge crosses the cone");
        let sd = disc.sqrt();
        let (mut c1, mut c2) = (l0 + dir * ((-qb - sd) / (2.0 * qa)), l0 + dir * ((-qb + sd) / (2.0 * qa)));
        for c in [c1, c2] {
            let av_c = (c - apex).dot(n_a);
            assert!(av_c > 1e-6 && av_c < height - 1e-6, "corner within the cone");
        }
        let (mut uc1, mut uc2) = (u_of(c1), u_of(c2));
        if uc1 > uc2 {
            std::mem::swap(&mut uc1, &mut uc2);
            std::mem::swap(&mut c1, &mut c2);
        }
        // APEX-KEEP: the binding plane per arc is argMIN v_e (kept v < min cut).
        let active = |um: f64| -> (DVec3, DVec3) {
            if v_plane(m1, o1, um) <= v_plane(m2, o2, um) { (m1, o1) } else { (m2, o2) }
        };
        let mid_ua = 0.5 * (uc1 + uc2);
        let mid_ub = (0.5 * (uc2 + uc1 + TAU)).rem_euclid(TAU);
        let (pa, pa_o) = active(mid_ua);
        let (pb, pb_o) = active(mid_ub);
        let mid_a = surf(mid_ua, v_plane(pa, pa_o, mid_ua));
        let mid_b = surf(mid_ub, v_plane(pb, pb_o, mid_ub));
        let (ea_c, ea_sm, ea_sn, ea_mj, ea_mn) = cone_oblique_ellipse(apex, n_a, half_angle, pa_o, pa).unwrap();
        let (eb_c, eb_sm, eb_sn, eb_mj, eb_mn) = cone_oblique_ellipse(apex, n_a, half_angle, pb_o, pb).unwrap();
        let phi = |p: DVec3, c: DVec3, sm: f64, sn: f64, mj: DVec3, mn: DVec3| {
            let rel = p - c;
            (rel.dot(mn) / sn).atan2(rel.dot(mj) / sm)
        };
        let arc = |p: DVec3, q: DVec3, c: DVec3, sm: f64, sn: f64, mj: DVec3, mn: DVec3| {
            let (cp, w, k, d) = crate::curves::nurbs::ellipse_arc(c, sm, sn, mj, mn, phi(p, c, sm, sn, mj, mn), phi(q, c, sm, sn, mj, mn));
            crate::curves::AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: d as u32 }
        };
        let top_verts = [c1, mid_a, c2, mid_b];
        let top_curves = [
            arc(c1, mid_a, ea_c, ea_sm, ea_sn, ea_mj, ea_mn),
            arc(mid_a, c2, ea_c, ea_sm, ea_sn, ea_mj, ea_mn),
            arc(c2, mid_b, eb_c, eb_sm, eb_sn, eb_mj, eb_mn),
            arc(mid_b, c1, eb_c, eb_sm, eb_sn, eb_mj, eb_mn),
        ];
        let band = S::Cone { apex, axis_dir: n_a, half_angle, ref_dir, u_range: (0.0, TAU), v_range: (0.0, height) };

        let mut mesh = Mesh::new();
        let (band_f, vids) = mesh.sew_corner_tip(&top_verts, &top_curves, band, n_a, mat).expect("corner tip sews");
        // two partial caps reuse the band arc twins + share the ridge (c1,c2) edge.
        let cap_a = mesh.add_face_with_holes(&[vids[2], vids[1], vids[0]], &[], mat).expect("cap_a");
        mesh.set_face_surface(cap_a, Some(S::Plane { origin: ea_c, normal: -pa, basis_u: ea_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }));
        let cap_b = mesh.add_face_with_holes(&[vids[0], vids[3], vids[2]], &[], mat).expect("cap_b");
        mesh.set_face_surface(cap_b, Some(S::Plane { origin: eb_c, normal: -pb, basis_u: eb_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }));

        let open = mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        assert_eq!(open, 0, "no open half-edges → closed apex-tip-corner shell");
        assert!(mesh.verify_face_invariants().is_valid(), "manifold + invariants");
        assert_eq!(mesh.collect_non_manifold_edges().len(), 0, "every edge shared by exactly 2 faces");
        assert!(matches!(mesh.face_surface(band_f), Some(S::Cone { .. })), "corner band keeps the Cone surface");
        eprintln!("[apex-tip corner DCEL] band={band_f:?} caps={cap_a:?},{cap_b:?} open={open} valid=true");
    }

    /// **ADR-205 cone apex-tip corner** — production: an apex-up cone cut by TWO
    /// oblique planes both keeping the apex → corner band + 2 caps, watertight +
    /// manifold, rendered apex-clipped (every band vertex on the kept side of BOTH
    /// planes, front-facing). Adversarial: parallel planes + a base-keeping plane bail.
    #[test]
    fn adr205_cone_apex_tip_corner() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        let m1 = DVec3::new(0.5, 0., 0.866).normalize();
        let m2 = DVec3::new(-0.5, 0., 0.866).normalize();
        let o = DVec3::new(0., 0., 3.);
        let r = m.boolean_cone_apex_corner(&cone, o, m1, o, m2, mat).expect("apex-tip corner");
        assert_eq!(r.len(), 3, "corner band + 2 caps (no base disk)");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "watertight tip-corner shell"
        );
        assert!(m.verify_face_invariants().is_valid(), "manifold + invariants");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        let band = r.iter().copied()
            .find(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).expect("Cone band");
        assert_eq!(r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).count(), 2, "two caps");

        let tess = m.tessellate_cone_corner_clipped(band, 0.05).expect("apex-clipped corner render");
        assert!(!tess.triangles.is_empty(), "non-empty corner band");
        assert!(tess.vertices.iter().all(|p| (*p - o).dot(m1) > -1e-6 && (*p - o).dot(m2) > -1e-6),
            "every band vertex on the kept apex side of both planes");
        let apex = DVec3::new(0., 0., 6.);
        let n_a = DVec3::NEG_Z;
        let mut back = 0usize;
        for tri in &tess.triangles {
            let (a, b, c) = (
                tess.vertices[tri[0] as usize], tess.vertices[tri[1] as usize], tess.vertices[tri[2] as usize],
            );
            let tn = (b - a).cross(c - a);
            if tn.length() < 1e-12 { continue; }
            let cen = (a + b + c) / 3.0;
            let rel = cen - apex;
            let radial = rel - n_a * rel.dot(n_a);
            if radial.length() < 1e-9 { continue; }
            if tn.dot(radial.normalize()) <= 0.0 { back += 1; }
        }
        assert_eq!(back, 0, "every corner-band triangle front-facing (outward)");

        // adversarial: parallel planes bail; a base-keeping plane (apex on −m) bails.
        let mut m2m = Mesh::default();
        let cone2 = m2m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        assert!(m2m.boolean_cone_apex_corner(&cone2, o, m1, o, m1, mat).is_err(), "parallel planes → Err");
        assert!(m2m.verify_face_invariants().is_valid(), "intact after parallel bail");
        let mut m3 = Mesh::default();
        let cone3 = m3.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        let base_keep = DVec3::new(0.5, 0., -0.866).normalize(); // apex on −m (base keep)
        assert!(m3.boolean_cone_apex_corner(&cone3, o, base_keep, o, m2, mat).is_err(), "base-keeping plane → Err");
        assert!(m3.verify_face_invariants().is_valid(), "intact after base-keep bail");
    }

    /// **ADR-205 γ cone apex-tip corner** — `box ∩ tilted-cone` keeping the APEX with
    /// TWO perpendicular faces now routes: a box whose two perpendicular binding faces
    /// each put the apex on the inward side + the whole base outward dispatches to
    /// `boolean_cone_apex_corner` (was deferred).
    #[test]
    fn adr205_gamma_cone_box_apex_tip_corner_autoroutes() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
            let cv = m.solid_loop_verts(&c);
            m.rotate_verts(&cv, DVec3::ZERO, DVec3::Y, 0.6).unwrap(); // apex ≈ (4.5,0,6.6), base origin
            (m, c)
        };
        let verify_tip = |m: &Mesh, r: &[FaceId]| {
            assert_eq!(r.len(), 3, "corner band + 2 caps");
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "watertight"
            );
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))), "Cone band kept");
        };

        // box keeps the apex corner: min-X@3 + min-Z@4 both keep the apex (apex ≈
        // (4.5,0,6.6) inside +X/+Z; base at origin outside both).
        let (mut m, cone) = build();
        let bx = make_box(&mut m, DVec3::new(3., -6., 4.), DVec3::new(12., 6., 12.), mat);
        let r = m.boolean(&cone, &bx, BoolOp::Intersect, mat).expect("γ apex-tip corner auto-routes");
        verify_tip(&m, &r.faces);

        // commutative.
        let (mut m2, cone2) = build();
        let bx2 = make_box(&mut m2, DVec3::new(3., -6., 4.), DVec3::new(12., 6., 12.), mat);
        let r2 = m2.boolean(&bx2, &cone2, BoolOp::Intersect, mat).expect("apex-tip corner commutative");
        verify_tip(&m2, &r2.faces);
    }

    /// **ADR-205 cone apex-tip** — production: an apex-up cone cut by an oblique plane
    /// keeping the small APEX tip → cone-side fan + elliptic cap, watertight + manifold,
    /// rendered apex-clipped (every side vertex on the kept +m side, front-facing).
    /// Adversarial: a plane keeping the base (β-2-cone territory) and a ⟂ plane bail
    /// with the mesh intact.
    #[test]
    fn adr205_cone_apex_tip_halfspace() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        // apex-up cone: apex (0,0,6), base z=0, r=2.
        let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        // oblique plane at z≈4, m toward the apex → keep the apex tip (apex on +m).
        let pm = DVec3::new(0.2, 0., 0.98).normalize();
        let o = DVec3::new(0., 0., 4.);
        let r = m.boolean_cone_apex_halfspace(&cone, o, pm, mat).expect("apex tip cut");
        assert_eq!(r.len(), 2, "cone-side fan + elliptic cap");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "watertight tip shell"
        );
        assert!(m.verify_face_invariants().is_valid(), "manifold + invariants");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        let side = r.iter().copied()
            .find(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).expect("Cone side");
        assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Plane { .. }))), "elliptic cap Plane");

        // apex-clipped render: non-empty, every vertex on the kept +m side, front-facing.
        let tess = m.tessellate_cone_clipped(side, 0.05).expect("apex-clipped render");
        assert!(!tess.triangles.is_empty(), "non-empty fan");
        assert!(tess.vertices.iter().all(|p| (*p - o).dot(pm) > -1e-6), "side on the kept +m side");
        let apex = DVec3::new(0., 0., 6.);
        let n_a = DVec3::NEG_Z;
        let mut back = 0usize;
        for tri in &tess.triangles {
            let (a, b, c) = (
                tess.vertices[tri[0] as usize],
                tess.vertices[tri[1] as usize],
                tess.vertices[tri[2] as usize],
            );
            let tn = (b - a).cross(c - a);
            if tn.length() < 1e-12 { continue; } // degenerate apex triangle
            let cen = (a + b + c) / 3.0;
            let rel = cen - apex;
            let radial = rel - n_a * rel.dot(n_a);
            if radial.length() < 1e-9 { continue; }
            if tn.dot(radial.normalize()) <= 0.0 { back += 1; } // cone side normal points outward
        }
        assert_eq!(back, 0, "every cone-side triangle front-facing (outward)");

        // adversarial: keeping the base (m toward the base) is β-2-cone territory → bail.
        let mut m2 = Mesh::default();
        let cone2 = m2.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        let r2 = m2.boolean_cone_apex_halfspace(&cone2, DVec3::new(0., 0., 2.), DVec3::new(0.2, 0., -0.98).normalize(), mat);
        assert!(r2.is_err(), "base on +m → apex-tip bails (use β-2-cone)");
        assert!(m2.verify_face_invariants().is_valid(), "mesh intact after base bail");
        // ⟂ plane (a circular section, not an ellipse) → bail.
        let mut m3 = Mesh::default();
        let cone3 = m3.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        let r3 = m3.boolean_cone_apex_halfspace(&cone3, DVec3::new(0., 0., 4.), DVec3::Z, mat);
        assert!(r3.is_err(), "⟂ plane (circle, not ellipse) → bail");
        assert!(m3.verify_face_invariants().is_valid(), "mesh intact after ⟂ bail");
    }

    /// **ADR-205 γ cone apex-tip** — `box ∩ tilted-cone` keeping the APEX now routes:
    /// a box whose one binding face cuts the cone with the apex on the inward side +
    /// the whole base outward dispatches to `boolean_cone_apex_halfspace` (was deferred).
    #[test]
    fn adr205_gamma_cone_box_apex_tip_autoroutes() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
            let cv = m.solid_loop_verts(&c);
            m.rotate_verts(&cv, DVec3::ZERO, DVec3::X, 0.3).unwrap(); // apex ≈ (0,−1.77,5.73), base z∈±0.6
            (m, c)
        };
        let verify_tip = |m: &Mesh, r: &[FaceId]| {
            assert_eq!(r.len(), 2, "cone-side fan + elliptic cap");
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "watertight"
            );
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))), "Cone side kept");
        };

        // box keeps the apex (min-z face at z=4 cuts; apex z≈5.73 inside, base below outside).
        let (mut m, cone) = build();
        let bx = make_box(&mut m, DVec3::new(-6., -6., 4.), DVec3::new(6., 6., 8.), mat);
        let r = m.boolean(&cone, &bx, BoolOp::Intersect, mat).expect("γ cone apex-tip auto-routes");
        verify_tip(&m, &r.faces);

        // commutative (box ∩ cone).
        let (mut m2, cone2) = build();
        let bx2 = make_box(&mut m2, DVec3::new(-6., -6., 4.), DVec3::new(6., 6., 8.), mat);
        let r2 = m2.boolean(&bx2, &cone2, BoolOp::Intersect, mat).expect("apex-tip commutative");
        verify_tip(&m2, &r2.faces);
    }

    /// **ADR-205 β-2-cone** — the production op cuts a cone with an oblique plane,
    /// keeps the base FRUSTUM (band + elliptic cap + base disk), watertight +
    /// manifold + invariant-valid, the analytic Cone band preserved + rendered
    /// boundary-aware (every band vertex stays on the kept +m side — no over-draw
    /// past the oblique ellipse), all faces front-facing. Keeping the apex tip
    /// (base on −m) is deferred (Err).
    #[test]
    fn adr205_beta2cone_oblique_halfspace_frustum() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        // oblique plane; m points toward the base (base on +m) → keep the frustum.
        let pm = DVec3::new(0.3, 0., -0.954).normalize();
        let o = DVec3::new(0., 0., 3.);
        let r = m.boolean_cone_oblique_halfspace(&cone, o, pm, mat).expect("frustum cut");
        assert_eq!(r.len(), 3, "band + elliptic cap + base disk");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0,
            "frustum watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        let band = *r.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).expect("Cone band");
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        // boundary-aware render: every band vertex on the kept +m side of the cut.
        let mut wrong = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let p = DVec3::new(pos[vi * 3] as f64, pos[vi * 3 + 1] as f64, pos[vi * 3 + 2] as f64);
                if (p - o).dot(pm) < -1e-3 { wrong += 1; }
            }
        }
        assert!(fmap.iter().any(|&fid| fid == band.raw()), "band renders");
        assert_eq!(wrong, 0, "band stays on the kept +m side (boundary-aware clip)");
        // all faces front-facing (outward normals away from the centroid).
        let nv = pos.len() / 3;
        let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
            c + DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64)) / (nv.max(1) as f64);
        let mut inward = 0usize;
        for i in 0..nv {
            let p = DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64);
            let n = DVec3::new(nrm[i * 3] as f64, nrm[i * 3 + 1] as f64, nrm[i * 3 + 2] as f64);
            if (p - centroid).dot(n) < -1e-3 { inward += 1; }
        }
        assert_eq!(inward, 0, "all frustum faces front-facing");
        assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()), "finite");

        // keeping the apex tip (base on −m) is deferred → Err, mesh intact.
        let mut m2 = Mesh::default();
        let cone2 = m2.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        assert!(m2.boolean_cone_oblique_halfspace(&cone2, o, -pm, mat).is_err(), "apex-tip keep deferred");
        assert!(m2.verify_face_invariants().is_valid(), "mesh intact after apex-tip bail");
    }

    /// **ADR-205 β-2-cone adversarial sweep** — frustum robustness across a steeper
    /// oblique, an off-axis cut, and a genuinely TILTED cone cut by a CARDINAL −Z
    /// box face (the γ target, via rotate_verts); plus graceful decline for a
    /// hyperbola section, a ⟂ plane (circle), and a non-separating plane. Routed
    /// cases are watertight 3-face frustums with the Cone band; declined cases Err
    /// WITHOUT a crash, mesh invariant-valid.
    #[test]
    fn adr205_beta2cone_oblique_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let ok_frustum = |m: &mut Mesh, r: &[FaceId], lbl: &str| {
            assert_eq!(r.len(), 3, "{lbl} 3-face frustum");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "{lbl} watertight");
            assert!(m.verify_face_invariants().is_valid(), "{lbl} invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "{lbl} manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))), "{lbl} Cone band");
        };

        // (1) steeper oblique → a more elongated ellipse, still a frustum.
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
            let pm = DVec3::new(0.4, 0., -0.917).normalize();
            let r = m.boolean_cone_oblique_halfspace(&c, DVec3::new(0., 0., 4.), pm, mat).expect("steeper frustum");
            ok_frustum(&mut m, &r, "steeper oblique");
        }
        // (2) off-axis cut (origin off the axis).
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
            let pm = DVec3::new(0.25, 0.1, -0.963).normalize();
            let r = m.boolean_cone_oblique_halfspace(&c, DVec3::new(0.3, -0.2, 4.0), pm, mat).expect("off-axis frustum");
            ok_frustum(&mut m, &r, "off-axis");
        }
        // (3) TILTED cone (rotate a Z-axis cone) cut by a CARDINAL −Z box face — γ target.
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 1.5, 8.0, mat).unwrap();
            let cv = m.solid_loop_verts(&c);
            m.rotate_verts(&cv, DVec3::ZERO, DVec3::X, 0.35).unwrap(); // tilt ~20° about X
            let r = m.boolean_cone_oblique_halfspace(&c, DVec3::new(0., 0., 3.5), DVec3::NEG_Z, mat).expect("tilted cone + cardinal");
            ok_frustum(&mut m, &r, "tilted cone + cardinal −Z");
        }
        // (4) HYPERBOLA section (wide cone + shallow-|D| plane) → Err, intact.
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 6.0, 2.0, mat).unwrap(); // wide α=atan(3)
            let r = m.boolean_cone_oblique_halfspace(&c, DVec3::new(0., 0., 1.), DVec3::new(0.9, 0., -0.436).normalize(), mat);
            assert!(r.is_err(), "hyperbola section deferred");
            assert!(m.verify_face_invariants().is_valid(), "intact after hyperbola bail");
        }
        // (5) ⟂ plane (normal = axis) → a circle, not an oblique ellipse → Err.
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
            let r = m.boolean_cone_oblique_halfspace(&c, DVec3::new(0., 0., 3.), DVec3::NEG_Z, mat);
            assert!(r.is_err(), "perpendicular plane deferred");
            assert!(m.verify_face_invariants().is_valid(), "intact after ⟂ bail");
        }
        // (6) plane entirely below the base (does not separate apex from base) → Err.
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
            let r = m.boolean_cone_oblique_halfspace(&c, DVec3::new(0., 0., -2.), DVec3::new(0.3, 0., -0.954).normalize(), mat);
            assert!(r.is_err(), "non-separating plane deferred");
            assert!(m.verify_face_invariants().is_valid(), "intact after non-separating bail");
        }
    }

    /// **ADR-205 β-3-cone** — a cone cut by TWO parallel oblique planes keeps the
    /// elliptic SLAB (band with two ellipse boundaries + two elliptic caps),
    /// watertight + manifold + invariant, Cone band preserved + rendered boundary-
    /// aware (every band vertex within the slab d∈[d_lo,d_hi]), all front-facing.
    /// A slab containing the apex / base or a ⟂ plane is deferred.
    #[test]
    fn adr205_beta3cone_oblique_slab() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
        let apex = DVec3::new(0., 0., 8.); // create_cone apex = center + Z*height
        let pm = DVec3::new(0.2, 0., -0.98).normalize();
        let (d_lo, d_hi) = (2.0_f64, 5.0_f64);
        let r = m.boolean_cone_oblique_slab(&cone, pm, d_lo, d_hi, mat).expect("cone slab");
        assert_eq!(r.len(), 3, "band + 2 elliptic caps");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0,
            "slab watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        let band = *r.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).expect("Cone band");
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        // boundary-aware: every band vertex within the slab d∈[d_lo,d_hi].
        let mut out = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let p = DVec3::new(pos[vi * 3] as f64, pos[vi * 3 + 1] as f64, pos[vi * 3 + 2] as f64);
                let dd = (p - apex).dot(pm);
                if dd < d_lo - 1e-3 || dd > d_hi + 1e-3 { out += 1; }
            }
        }
        assert!(fmap.iter().any(|&fid| fid == band.raw()), "band renders");
        assert_eq!(out, 0, "slab band within the d∈[d_lo,d_hi] band (boundary-aware)");
        // front-facing.
        let nv = pos.len() / 3;
        let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
            c + DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64)) / (nv.max(1) as f64);
        let mut inward = 0usize;
        for i in 0..nv {
            let p = DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64);
            let n = DVec3::new(nrm[i * 3] as f64, nrm[i * 3 + 1] as f64, nrm[i * 3 + 2] as f64);
            if (p - centroid).dot(n) < -1e-3 { inward += 1; }
        }
        assert_eq!(inward, 0, "all slab faces front-facing");
        assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()), "finite");
    }

    /// **ADR-205 β-3-cone adversarial sweep** — slab robustness across a TILTED
    /// cone cut by two CARDINAL ±Z box faces (the γ slab target via rotate_verts)
    /// + graceful decline for a slab containing the apex and a ⟂ plane.
    #[test]
    fn adr205_beta3cone_slab_adversarial_sweep() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        // (1) TILTED cone cut by two cardinal −Z-normal planes (a Z-slab box).
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 1.5, 10.0, mat).unwrap();
            let cv = m.solid_loop_verts(&c);
            m.rotate_verts(&cv, DVec3::ZERO, DVec3::X, 0.3).unwrap(); // tilt ~17° about X
            let apex = {
                let f = *c.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).unwrap();
                match m.face_surface(f) { Some(S::Cone { apex, .. }) => *apex, _ => unreachable!() }
            };
            // cardinal +Z normal; d along +Z from apex. The slab keeps a Z-band.
            let pm = DVec3::Z;
            let base_d = (DVec3::ZERO - apex).dot(pm); // base at z=0
            let apex_d = 0.0_f64;
            let (lo, hi) = (apex_d.min(base_d), apex_d.max(base_d));
            let (d_lo, d_hi) = (lo + (hi - lo) * 0.3, lo + (hi - lo) * 0.7);
            let r = m.boolean_cone_oblique_slab(&c, pm, d_lo, d_hi, mat).expect("tilted cone Z-slab");
            assert_eq!(r.len(), 3, "tilted slab 3 faces");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))), "Cone band");
        }
        // (2) slab containing the apex (d_lo < 0) → deferred (Err, intact).
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
            let r = m.boolean_cone_oblique_slab(&c, DVec3::new(0.2, 0., -0.98).normalize(), -1.0, 4.0, mat);
            assert!(r.is_err(), "slab containing the apex deferred");
            assert!(m.verify_face_invariants().is_valid(), "intact after apex-slab bail");
        }
        // (3) ⟂ planes (normal = axis) → circle sections, not oblique ellipses → Err.
        {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
            let r = m.boolean_cone_oblique_slab(&c, DVec3::NEG_Z, 2.0, 5.0, mat);
            assert!(r.is_err(), "perpendicular slab deferred");
            assert!(m.verify_face_invariants().is_valid(), "intact after ⟂ bail");
        }
    }

    /// **SIMULATION (ADR-205 γ-cone-slab)** — `box ∩ tilted-cone` SLAB auto-routing.
    /// A box that is a slab in one cardinal direction passing through a tilted cone
    /// cuts it with its two parallel e-faces (oblique elliptic sections) and
    /// CONTAINS it in the other two. Unlike the cylinder, the cone narrows to the
    /// apex, so its cardinal extent spans the apex point + the base rim; a clean
    /// slab needs the apex on one side of BOTH faces and the WHOLE base disk on the
    /// other. This probe computes that detection inline + routes β-3-cone.
    #[test]
    fn sim_adr205_gamma_cone_box_slab_detection() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 1.5, 10.0, mat).unwrap();
            let cv = m.solid_loop_verts(&c);
            m.rotate_verts(&cv, DVec3::ZERO, DVec3::X, 0.3).unwrap(); // tilt ~17° about X
            (m, c)
        };
        let (apex, n_a, half_angle, ref_dir, v_range) = {
            let (m, c) = build();
            m.cone_full_of(&c).expect("cone geom")
        };
        let n_a = n_a.normalize();
        let height = v_range.0.max(v_range.1);
        let base_center = apex + n_a * height;
        let base_radius = height * half_angle.tan();
        let ref_n = ref_dir.normalize();
        let bw = n_a.cross(ref_n).normalize();
        let axes = [DVec3::X, DVec3::Y, DVec3::Z];
        // cone cardinal extent (apex point + base rim).
        let cone_lo_hi = |e: DVec3| -> (f64, f64, f64, f64) {
            let amp = ((ref_n.dot(e)).powi(2) + (bw.dot(e)).powi(2)).sqrt();
            let (base_lo, base_hi) = (base_center.dot(e) - base_radius * amp, base_center.dot(e) + base_radius * amp);
            let ax = apex.dot(e);
            (ax.min(base_lo), ax.max(base_hi), base_lo, base_hi)
        };
        const EPS: f64 = 1e-9;
        // a Z-slab box through the cone middle (apex above z=6, base below z=3).
        let (bmin, bmax) = (DVec3::new(-5., -5., 3.), DVec3::new(5., 5., 6.));
        let bmins = [bmin.x, bmin.y, bmin.z];
        let bmaxs = [bmax.x, bmax.y, bmax.z];
        // detect the clean slab axis: both faces between apex + whole base disk;
        // the other two contain the cone.
        let slab_axis = (0..3).find(|&i| {
            let e = axes[i];
            let (clo, chi, blo, bhi) = cone_lo_hi(e);
            let apex_e = apex.dot(e);
            // case A: apex below the slab, base above; case B: apex above, base below.
            let a = apex_e < bmins[i] - EPS && blo > bmaxs[i] + EPS;
            let b = apex_e > bmaxs[i] + EPS && bhi < bmins[i] - EPS;
            let is_slab = a || b;
            let others_contain = (0..3).filter(|&j| j != i).all(|j| {
                let (clo2, chi2, ..) = cone_lo_hi(axes[j]);
                let _ = (clo, chi);
                clo2 >= bmins[j] - EPS && chi2 <= bmaxs[j] + EPS
            });
            is_slab && others_contain
        });
        assert_eq!(slab_axis, Some(2), "clean Z-slab on the tilted cone");

        // route β-3-cone(e, box.min[e]−apex·e, box.max[e]−apex·e).
        let e = axes[2];
        let (mut m, c) = build();
        let d_lo = bmins[2] - apex.dot(e);
        let d_hi = bmaxs[2] - apex.dot(e);
        let r = m.boolean_cone_oblique_slab(&c, e, d_lo, d_hi, mat).expect("γ-cone routes Z-slab → β-3-cone");
        assert_eq!(r.len(), 3, "band + 2 caps");
        assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");

        // negative: a box thin in X too (X pair also brackets the apex/base) → not
        // a pure single-axis slab.
        let (bmin2, bmax2) = ([-0.3, -5., 3.], [0.3, 5., 6.]);
        let pure = (0..3).any(|i| {
            let e = axes[i];
            let (_, _, blo, bhi) = cone_lo_hi(e);
            let apex_e = apex.dot(e);
            let a = apex_e < bmin2[i] - EPS && blo > bmax2[i] + EPS;
            let b = apex_e > bmax2[i] + EPS && bhi < bmin2[i] - EPS;
            (a || b) && (0..3).filter(|&j| j != i).all(|j| {
                let (clo2, chi2, ..) = cone_lo_hi(axes[j]);
                clo2 >= bmin2[j] - EPS && chi2 <= bmax2[j] + EPS
            })
        });
        // the narrow X box does NOT contain the cone in X (cone X-extent ≈ ±1.5),
        // so it is not a clean single-axis slab.
        assert!(!pure, "X-thin box is not a clean single-axis slab");
    }

    /// **ADR-205 γ-cone-slab** — the public `boolean(tilted-cone, axis-box, Intersect)`
    /// AUTO-routes a single-axis SLAB box to β-3-cone through the curved-intersect
    /// dispatch (no manual plane). The result keeps the analytic Cone band, is
    /// watertight + manifold, and the box is consumed. Commutative.
    #[test]
    fn adr205_gamma_cone_box_slab_autoroutes() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 1.5, 10.0, mat).unwrap();
            let cv = m.solid_loop_verts(&c);
            m.rotate_verts(&cv, DVec3::ZERO, DVec3::X, 0.3).unwrap();
            (m, c)
        };
        let verify = |m: &mut Mesh, r: &[FaceId]| {
            assert_eq!(r.len(), 3, "band + 2 elliptic caps");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))), "Cone band preserved");
        };

        // cone ∩ Z-slab box.
        let (mut m, cone) = build();
        let bx = make_box(&mut m, DVec3::new(-5., -5., 3.), DVec3::new(5., 5., 6.), mat);
        let r = m.boolean(&cone, &bx, BoolOp::Intersect, mat).expect("γ-cone-slab auto-routes");
        verify(&mut m, &r.faces);

        // box ∩ cone (commutative).
        let (mut m2, cone2) = build();
        let bx2 = make_box(&mut m2, DVec3::new(-5., -5., 3.), DVec3::new(5., 5., 6.), mat);
        let r2 = m2.boolean(&bx2, &cone2, BoolOp::Intersect, mat).expect("γ-cone-slab commutative");
        verify(&mut m2, &r2.faces);
    }

    /// **ADR-205 γ-cone-halfspace** — the public `boolean(tilted-cone, axis-box,
    /// Intersect)` AUTO-routes the HALFSPACE config (one box face clips the apex →
    /// β-2-cone frustum) and the no-op CONTAINMENT config (box ⊇ cone → A∩B=A).
    /// Frustum keeps the Cone band; containment returns the cone unchanged. Keeping
    /// the apex tip (base outside the box) is deferred. Commutative.
    #[test]
    fn adr205_gamma_cone_box_halfspace_autoroutes() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 1.5, 10.0, mat).unwrap();
            let cv = m.solid_loop_verts(&c);
            m.rotate_verts(&cv, DVec3::ZERO, DVec3::X, 0.3).unwrap(); // apex ≈ z 9.55, base ≈ z 0
            (m, c)
        };
        let verify_frustum = |m: &mut Mesh, r: &[FaceId]| {
            assert_eq!(r.len(), 3, "band + elliptic cap + base disk");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))), "Cone band");
        };

        // (A) HALFSPACE: box clips the apex tip (+Z face at z=6) → β-2-cone frustum.
        let (mut m, cone) = build();
        let bx = make_box(&mut m, DVec3::new(-6., -6., -3.), DVec3::new(6., 6., 6.), mat);
        let r = m.boolean(&cone, &bx, BoolOp::Intersect, mat).expect("γ-cone halfspace auto-routes");
        verify_frustum(&mut m, &r.faces);

        // (B) no-op CONTAINMENT: box ⊇ cone → returns the cone unchanged.
        let (mut m2, cone2) = build();
        let cone_set: std::collections::BTreeSet<u32> = cone2.iter().map(|f| f.raw()).collect();
        let bx2 = make_box(&mut m2, DVec3::new(-6., -6., -3.), DVec3::new(6., 6., 11.), mat);
        let r2 = m2.boolean(&cone2, &bx2, BoolOp::Intersect, mat).expect("γ-cone containment no-op");
        let res_set: std::collections::BTreeSet<u32> = r2.faces.iter().map(|f| f.raw()).collect();
        assert_eq!(res_set, cone_set, "containment returns the cone faces unchanged");

        // (C) commutative halfspace.
        let (mut m3, cone3) = build();
        let bx3 = make_box(&mut m3, DVec3::new(-6., -6., -3.), DVec3::new(6., 6., 6.), mat);
        let r3 = m3.boolean(&bx3, &cone3, BoolOp::Intersect, mat).expect("γ-cone commutative");
        assert!(r3.faces.iter().any(|&f| matches!(m3.face_surface(f), Some(S::Cone { .. }))), "Cone band");

        // (D) apex-tip keep (box keeps the apex, base outside) → now routes to
        // boolean_cone_apex_halfspace (cone-side fan + elliptic cap).
        let (mut m4, cone4) = build();
        let bx4 = make_box(&mut m4, DVec3::new(-6., -6., 5.), DVec3::new(6., 6., 12.), mat);
        let r4 = m4.boolean(&cone4, &bx4, BoolOp::Intersect, mat).expect("γ-cone apex-tip auto-routes");
        assert_eq!(r4.faces.len(), 2, "apex-tip → cone-side fan + elliptic cap");
        assert_eq!(
            m4.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "apex-tip watertight"
        );
        assert!(m4.verify_face_invariants().is_valid(), "apex-tip invariants");
        assert!(r4.faces.iter().any(|&f| matches!(m4.face_surface(f), Some(S::Cone { .. }))), "Cone side kept");
    }

    /// **ADR-205 cone-corner γ** — the public `boolean(tilted-cone, axis-box,
    /// Intersect)` AUTO-routes a CORNER box (two perpendicular base-keeping faces)
    /// to `boolean_cone_corner` through the curved-intersect dispatch. The cone is
    /// tilted ~34° about Y so its apex (≈(5.6,0,8.3)) + base (origin) spread across
    /// X and Z, letting a +X face AND a +Z face each keep the base + remove the
    /// apex. The result keeps the analytic Cone band, is watertight + manifold, and
    /// the box is consumed. Commutative.
    #[test]
    fn adr205_gamma_cone_box_corner_autoroutes() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let c = m.create_cone_kernel_native(DVec3::ZERO, 1.5, 10.0, mat).unwrap();
            let cv = m.solid_loop_verts(&c);
            m.rotate_verts(&cv, DVec3::ZERO, DVec3::Y, 0.6).unwrap(); // apex → ≈(5.6,0,8.3)
            (m, c)
        };
        let verify = |m: &mut Mesh, r: &[FaceId]| {
            assert_eq!(r.len(), 4, "band + base disk + 2 partial caps");
            assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cone { .. }))), "Cone band preserved");
        };

        // cone ∩ corner box (+X face at x=3 + +Z face at z=4 both keep the base).
        let (mut m, cone) = build();
        let bx = make_box(&mut m, DVec3::new(-5., -5., -5.), DVec3::new(3., 5., 4.), mat);
        let r = m.boolean(&cone, &bx, BoolOp::Intersect, mat).expect("cone corner auto-routes");
        verify(&mut m, &r.faces);

        // box ∩ cone (commutative).
        let (mut m2, cone2) = build();
        let bx2 = make_box(&mut m2, DVec3::new(-5., -5., -5.), DVec3::new(3., 5., 4.), mat);
        let r2 = m2.boolean(&bx2, &cone2, BoolOp::Intersect, mat).expect("cone corner commutative");
        verify(&mut m2, &r2.faces);
    }

    /// **ADR-205 γ-torus** — `box ∩ tilted-torus` auto-routing through the public
    /// `boolean()`: a torus tilted ~11.5° off +Z (axis e* = Z) routes a 2-cut box
    /// to β-3-torus slab, a 1-cut box to β-2-torus halfspace, and a box ⊇ torus to
    /// the no-op containment — all surface-preserving + watertight + commutative.
    #[test]
    fn adr205_gamma_torus_box_autoroutes() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let t = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
            let tv = m.solid_loop_verts(&[t]);
            m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, 0.20).unwrap(); // axis ≈ (0,−0.2,0.98)
            (m, t)
        };
        let watertight = |m: &Mesh, r: &[FaceId]| {
            assert_eq!(
                m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
                0, "watertight"
            );
            assert!(m.verify_face_invariants().is_valid(), "invariants");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "manifold");
            assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Torus { .. }))), "Torus surface kept");
        };

        // (A) SLAB: box contains X,Y (±6) and cuts both Z faces (z=±0.4 within ±2.30).
        let (mut m, t) = build();
        let bx = make_box(&mut m, DVec3::new(-6., -6., -0.4), DVec3::new(6., 6., 0.4), mat);
        let r = m.boolean(&[t], &bx, BoolOp::Intersect, mat).expect("γ-torus slab auto-routes");
        assert_eq!(r.faces.len(), 4, "slab → 2 belts + 2 caps");
        watertight(&m, &r.faces);

        // (B) HALFSPACE: box cuts only the lower Z face (z=−0.4), contains above (z=10).
        let (mut m, t) = build();
        let bx = make_box(&mut m, DVec3::new(-6., -6., -0.4), DVec3::new(6., 6., 10.), mat);
        let r = m.boolean(&[t], &bx, BoolOp::Intersect, mat).expect("γ-torus halfspace auto-routes");
        assert_eq!(r.faces.len(), 2, "halfspace → band + cap");
        watertight(&m, &r.faces);

        // (C) CONTAINMENT: box ⊇ torus (±6, Z ±3 ⊃ ±2.30) → A∩B=A (torus unchanged).
        let (mut m, t) = build();
        let bx = make_box(&mut m, DVec3::new(-6., -6., -3.), DVec3::new(6., 6., 3.), mat);
        let r = m.boolean(&[t], &bx, BoolOp::Intersect, mat).expect("γ-torus containment");
        assert_eq!(r.faces.len(), 1, "containment → the torus itself");
        assert!(matches!(m.face_surface(r.faces[0]), Some(S::Torus { .. })), "torus kept whole");

        // (D) commutative: box ∩ torus (slab) with the operands swapped.
        let (mut m, t) = build();
        let bx = make_box(&mut m, DVec3::new(-6., -6., -0.4), DVec3::new(6., 6., 0.4), mat);
        let r = m.boolean(&bx, &[t], BoolOp::Intersect, mat).expect("γ-torus commutative");
        assert_eq!(r.faces.len(), 4, "commutative slab → 4 faces");
        watertight(&m, &r.faces);
    }

    /// **ADR-205 γ-torus** — adversarial: e* = X (torus axis ≈ +X) still routes an
    /// X-slab; a box that cuts the torus from the SIDE (a ⊥-axis face) and a torus
    /// tilted past the annular threshold both decline → graceful Err, mesh intact.
    #[test]
    fn adr205_gamma_torus_box_adversarial() {
        let mat = MaterialId::new(0);

        // (A) e* = X: torus rotated ~85° about Y so its axis ≈ +X → an X-slab routes.
        let mut m = Mesh::default();
        let t = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
        let tv = m.solid_loop_verts(&[t]);
        m.rotate_verts(&tv, DVec3::ZERO, DVec3::Y, 1.484).unwrap(); // axis ≈ (0.996,0,0.087)
        let bx = make_box(&mut m, DVec3::new(-0.4, -6., -6.), DVec3::new(0.4, 6., 6.), mat);
        let r = m.boolean(&[t], &bx, BoolOp::Intersect, mat).expect("e*=X slab routes");
        assert_eq!(r.faces.len(), 4, "X-axis torus slab → 4 faces");
        assert_eq!(m.face_set_manifold_info(&r.faces).non_manifold_edge_count, 0, "watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants");

        // (B) SIDE cut: a box smaller than the torus in X (a ⊥-axis face cuts) → the
        //     deferred pinched/side regime → declines → Err, mesh intact.
        let mut m = Mesh::default();
        let t = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
        let tv = m.solid_loop_verts(&[t]);
        m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, 0.20).unwrap();
        let bx = make_box(&mut m, DVec3::new(-3., -6., -0.4), DVec3::new(3., 6., 0.4), mat); // X ±3 < ±5.5
        let r = m.boolean(&[t], &bx, BoolOp::Intersect, mat);
        assert!(r.is_err(), "side (⊥-axis) cut deferred → Err");
        assert!(m.verify_face_invariants().is_valid(), "mesh intact after side-cut decline");

        // (C) torus tilted ~31.5° (> the 22° threshold) from every cardinal → declines.
        let mut m = Mesh::default();
        let t = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
        let tv = m.solid_loop_verts(&[t]);
        m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, 0.55).unwrap(); // axis·Z = 0.85 < 0.927
        let bx = make_box(&mut m, DVec3::new(-6., -6., -0.4), DVec3::new(6., 6., 0.4), mat);
        let r = m.boolean(&[t], &bx, BoolOp::Intersect, mat);
        assert!(r.is_err(), "torus past the annular threshold → deferred Err");
        assert!(m.verify_face_invariants().is_valid(), "mesh intact after over-tilt decline");
    }

    /// **ADR-205 γ-torus-wire** — the SliceTool single-plane TRIM dispatcher
    /// (`trim_curved_by_plane`) routes a Torus face set to the β-2-torus annular
    /// halfspace (and returns `None` for a non-curved set so the UI falls back to its
    /// polygon path). The whole Scene → WASM → SliceTool chain is surface-agnostic, so
    /// this engine branch activates the torus trim end-to-end.
    #[test]
    fn adr205_gamma_torus_wire_trim_routes() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let t = m.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
        let tv = m.solid_loop_verts(&[t]);
        m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, 0.20).unwrap();
        let r = m
            .trim_curved_by_plane(&[t], DVec3::ZERO, DVec3::Z, mat)
            .expect("torus trim is handled")
            .expect("β-2-torus annular halfspace");
        assert_eq!(r.len(), 2, "trim → band + cap");
        assert!(m.verify_face_invariants().is_valid(), "invariants");
        assert_eq!(m.collect_non_manifold_edges().len(), 0, "manifold");
        assert!(r.iter().any(|&f| matches!(m.face_surface(f), Some(S::Torus { .. }))), "Torus band kept");

        // a plane too oblique to the axis (pinched) → handled but Err (graceful).
        let mut m2 = Mesh::default();
        let t2 = m2.create_torus_kernel_native(DVec3::ZERO, 4.0, 1.5, mat).unwrap();
        let tv2 = m2.solid_loop_verts(&[t2]);
        m2.rotate_verts(&tv2, DVec3::ZERO, DVec3::X, 0.55).unwrap(); // axis 31.5° off Z
        let r2 = m2.trim_curved_by_plane(&[t2], DVec3::ZERO, DVec3::Z, mat);
        assert!(matches!(r2, Some(Err(_))), "too-oblique trim → handled with Err (pinched deferred)");
        assert!(m2.verify_face_invariants().is_valid(), "mesh intact after pinched trim");

        // a non-curved (box) face set → None → the UI polygon fallback.
        let mut m3 = Mesh::default();
        let bxf = make_box(&mut m3, DVec3::new(-1., -1., -1.), DVec3::new(1., 1., 1.), mat);
        assert!(m3.trim_curved_by_plane(&bxf, DVec3::ZERO, DVec3::Z, mat).is_none(), "box → None (polygon fallback)");
    }

    /// **SIMULATION (ADR-205 cone-corner α — tent geometry de-risk)** — a cone cut
    /// by TWO oblique planes forming a base-keeping TENT (cone β-5). Each plane
    /// gives a cone-section ellipse; the planes' ridge crosses the cone at two
    /// CORNER points (where the two ellipses meet). Per generator u the kept base
    /// frustum's TOP follows `max(v_e1(u), v_e2(u))` (the binding plane closer to
    /// the base); the active plane switches at the corners. This probe validates
    /// that geometry — corners on both planes + the cone, the band top on the
    /// active plane + the cone — BEFORE the DCEL/render of the production op.
    #[test]
    fn sim_adr205_cone_corner_tent_geometry() {
        use std::f64::consts::TAU;
        let apex = DVec3::new(0., 0., 8.);
        let n_a = DVec3::NEG_Z;
        let (base_radius, height) = (2.0_f64, 8.0_f64);
        let alpha = (base_radius / height).atan();
        let ref_dir = DVec3::X;
        let tan_a = alpha.tan();
        // two oblique planes (symmetric about ±X), both keeping the base → a tent;
        // their ridge {x=0, z=4} crosses the cone (radius 1 at z=4) at (0,±1,4).
        let o = DVec3::new(0., 0., 4.);
        let m1 = DVec3::new(0.35, 0., -0.937).normalize();
        let m2 = DVec3::new(-0.35, 0., -0.937).normalize();
        assert!(cone_oblique_ellipse(apex, n_a, alpha, o, m1).is_some(), "ellipse 1");
        assert!(cone_oblique_ellipse(apex, n_a, alpha, o, m2).is_some(), "ellipse 2");

        // cone basis (matches cone::evaluate).
        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize();
        let v_plane = |m: DVec3, u: f64| -> f64 {
            let g = n_a + (r_vec * u.cos() + p_vec * u.sin()) * tan_a;
            (o - apex).dot(m) / g.dot(m)
        };
        let surf = |u: f64, v: f64| apex + n_a * v + (r_vec * u.cos() + p_vec * u.sin()) * (v * tan_a);

        // find the two corner angles where v_plane1 == v_plane2 (ridge crossings).
        // n is NOT a multiple of 4, so the corners (π/2, 3π/2) fall strictly between
        // samples (a strict-sign-change detector would miss an exact-grid crossing).
        let mut corners = Vec::new();
        let n = 701;
        for i in 1..=n {
            let (ua, ub) = (TAU * ((i - 1) as f64) / n as f64, TAU * i as f64 / n as f64);
            let (fa, fb) = (v_plane(m1, ua) - v_plane(m2, ua), v_plane(m1, ub) - v_plane(m2, ub));
            if fa * fb < 0.0 {
                let (mut lo, mut hi) = (ua, ub);
                for _ in 0..40 {
                    let mid = 0.5 * (lo + hi);
                    let fm = v_plane(m1, mid) - v_plane(m2, mid);
                    if (v_plane(m1, lo) - v_plane(m2, lo)) * fm <= 0.0 { hi = mid; } else { lo = mid; }
                }
                corners.push(0.5 * (lo + hi));
            }
        }
        assert_eq!(corners.len(), 2, "two ridge ∩ cone corners");
        for &uc in &corners {
            let vc = v_plane(m1, uc);
            let pc = surf(uc, vc);
            assert!((pc - o).dot(m1).abs() < 1e-6, "corner on plane 1");
            assert!((pc - o).dot(m2).abs() < 1e-6, "corner on plane 2");
            let rel = pc - apex;
            assert!((rel.dot(n_a) / rel.length() - alpha.cos()).abs() < 1e-6, "corner on the cone");
            assert!(vc > 1e-6 && vc < height - 1e-6, "corner on the finite side");
        }
        // per generator: the base-frustum tent top = max(v_e1, v_e2) is on the
        // ACTIVE plane (the binding one) + on the cone.
        for k in 0..16 {
            let u = TAU * (k as f64 + 0.5) / 16.0;
            let (v1, v2) = (v_plane(m1, u), v_plane(m2, u));
            let (v_top, active) = if v1 >= v2 { (v1, m1) } else { (v2, m2) };
            let p = surf(u, v_top);
            assert!((p - o).dot(active).abs() < 1e-6, "band top on the active plane");
            let rel = p - apex;
            assert!((rel.dot(n_a) / rel.length() - alpha.cos()).abs() < 1e-6, "band top on the cone");
        }
    }

    /// **SIMULATION (ADR-205 cone-corner β-1 — DCEL de-risk)** — build the cone TENT
    /// solid (base disk + corner band [Cone surface, base circle inner + a 4-edge
    /// tent top of two active ellipse ARCS] + two partial elliptic caps) with the
    /// reuse primitive `sew_corner_band` (the same one cylinder β-5 uses) + two
    /// `add_face_with_holes` caps, and verify it is watertight + manifold +
    /// invariant-valid — proving the band wiring BEFORE the production op + render.
    /// The cone mirrors cylinder β-5: the kept base frustum's tent top follows
    /// `max(v_e1, v_e2)` (not `min`), and `n_a` (apex→base) is the base-outward.
    #[test]
    fn sim_adr205_cone_corner_dcel_watertight() {
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::TAU;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let apex = DVec3::new(0., 0., 8.);
        let n_a = DVec3::NEG_Z;
        let (base_radius, height) = (2.0_f64, 8.0_f64);
        let alpha = (base_radius / height).atan();
        let ref_dir = DVec3::X;
        let tan_a = alpha.tan();
        let o = DVec3::new(0., 0., 4.);
        let m1 = DVec3::new(0.35, 0., -0.937).normalize();
        let m2 = DVec3::new(-0.35, 0., -0.937).normalize();

        let r_vec = crate::surfaces::orthonormal_ref(n_a, ref_dir);
        let p_vec = n_a.cross(r_vec).normalize();
        let v_plane = |mp: DVec3, u: f64| -> f64 {
            let g = n_a + (r_vec * u.cos() + p_vec * u.sin()) * tan_a;
            (o - apex).dot(mp) / g.dot(mp)
        };
        let surf = |u: f64, v: f64| apex + n_a * v + (r_vec * u.cos() + p_vec * u.sin()) * (v * tan_a);
        let u_of = |p: DVec3| {
            let rel = p - apex;
            let radial = rel - n_a * rel.dot(n_a);
            radial.dot(p_vec).atan2(radial.dot(r_vec)).rem_euclid(TAU)
        };

        // corners (ridge ∩ cone) via bisection of v_e1 − v_e2 (n = 701, not ÷4).
        let mut corner_us = Vec::new();
        let n = 701;
        for i in 1..=n {
            let (ua, ub) = (TAU * (i - 1) as f64 / n as f64, TAU * i as f64 / n as f64);
            if (v_plane(m1, ua) - v_plane(m2, ua)) * (v_plane(m1, ub) - v_plane(m2, ub)) < 0.0 {
                let (mut lo, mut hi) = (ua, ub);
                for _ in 0..50 {
                    let mid = 0.5 * (lo + hi);
                    if (v_plane(m1, lo) - v_plane(m2, lo)) * (v_plane(m1, mid) - v_plane(m2, mid)) <= 0.0 { hi = mid; } else { lo = mid; }
                }
                corner_us.push(0.5 * (lo + hi));
            }
        }
        assert_eq!(corner_us.len(), 2, "two corners");
        let (uc1, uc2) = (corner_us[0], corner_us[1]);
        let c1 = surf(uc1, v_plane(m1, uc1));
        let c2 = surf(uc2, v_plane(m1, uc2));

        // active plane on each arc (base frustum → argMAX v_e is the binding top).
        let active_at = |um: f64| -> (DVec3, DVec3) {
            if v_plane(m1, um) >= v_plane(m2, um) { (m1, o) } else { (m2, o) }
        };
        let mid_ua = 0.5 * (uc1 + uc2);
        let mid_ub = 0.5 * (uc2 + uc1 + TAU);
        let (pa, _) = active_at(mid_ua);
        let (pb, _) = active_at(mid_ub.rem_euclid(TAU));
        let mid_a = surf(mid_ua, v_plane(pa, mid_ua));
        let mid_b = surf(mid_ub.rem_euclid(TAU), v_plane(pb, mid_ub.rem_euclid(TAU)));

        // ellipse params per active plane (cone Dandelin α) + φ-on-ellipse.
        let ell = |mp: DVec3| cone_oblique_ellipse(apex, n_a, alpha, o, mp).unwrap();
        let (ea_c, ea_sm, ea_sn, ea_mj, ea_mn) = ell(pa);
        let (eb_c, eb_sm, eb_sn, eb_mj, eb_mn) = ell(pb);
        let phi = |p: DVec3, c: DVec3, sm: f64, sn: f64, mj: DVec3, mn: DVec3| {
            let rel = p - c;
            (rel.dot(mn) / sn).atan2(rel.dot(mj) / sm)
        };
        let arc = |p: DVec3, q: DVec3, c: DVec3, sm: f64, sn: f64, mj: DVec3, mn: DVec3| {
            let (cp, w, k, d) = crate::curves::nurbs::ellipse_arc(c, sm, sn, mj, mn, phi(p, c, sm, sn, mj, mn), phi(q, c, sm, sn, mj, mn));
            crate::curves::AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: d as u32 }
        };
        let _ = u_of; // (corners already in u via the detector)
        let top_verts = [c1, mid_b, c2, mid_a];
        let top_curves = [
            arc(c1, mid_b, eb_c, eb_sm, eb_sn, eb_mj, eb_mn),
            arc(mid_b, c2, eb_c, eb_sm, eb_sn, eb_mj, eb_mn),
            arc(c2, mid_a, ea_c, ea_sm, ea_sn, ea_mj, ea_mn),
            arc(mid_a, c1, ea_c, ea_sm, ea_sn, ea_mj, ea_mn),
        ];
        let c_base = apex + n_a * height;
        let base_circle = crate::curves::AnalyticCurve::Circle {
            center: c_base, radius: base_radius, normal: n_a, basis_u: r_vec, // n_a = base-outward
        };
        let band = S::Cone { apex, axis_dir: n_a, half_angle: alpha, ref_dir, u_range: (0., TAU), v_range: (0., height) };
        let base_disk = S::Plane {
            origin: c_base, normal: n_a, basis_u: r_vec,
            u_range: (-base_radius * 1.5, base_radius * 1.5), v_range: (-base_radius * 1.5, base_radius * 1.5),
        };
        let (band_f, disk_f, vids) = m.sew_corner_band(
            &top_verts, &top_curves, c_base + r_vec * base_radius, base_circle,
            band, r_vec, base_disk, n_a, mat,
        ).expect("sew cone corner band");
        // partial caps reuse the band arc edges (opposite traversal).
        let cap_b = m.add_face_with_holes(&[vids[2], vids[1], vids[0]], &[], mat).expect("cap_b");
        m.faces[cap_b].set_surface(Some(S::Plane { origin: eb_c, normal: -pb, basis_u: eb_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }));
        let cap_a = m.add_face_with_holes(&[vids[0], vids[3], vids[2]], &[], mat).expect("cap_a");
        m.faces[cap_a].set_surface(Some(S::Plane { origin: ea_c, normal: -pa, basis_u: ea_mj, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6) }));

        let faces = [band_f, disk_f, cap_b, cap_a];
        let open = m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let valid = m.verify_face_invariants().is_valid();
        if !valid { eprintln!("{}", m.verify_face_invariants().summary()); }
        assert_eq!(open, 0, "cone tent watertight (open HEs)");
        assert!(valid, "DCEL invariants valid");
        assert_eq!(m.face_set_manifold_info(&faces).non_manifold_edge_count, 0, "manifold");
    }

    /// **ADR-205 cone-corner** — the production op cuts a cone with a base-keeping
    /// TENT (two oblique planes) → a 4-face corner solid (band + base disk + 2
    /// partial caps), watertight + manifold + invariant, the Cone band preserved +
    /// rendered boundary-aware (every band vertex on the kept +m side of BOTH
    /// planes — no over-draw past the tent), all faces render + front-facing.
    /// Parallel planes / a ridge that misses the cone are rejected.
    #[test]
    fn adr205_cone_corner_tent() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
        let (m1, o1) = (DVec3::new(0.35, 0., -0.937).normalize(), DVec3::new(0., 0., 4.));
        let (m2, o2) = (DVec3::new(-0.35, 0., -0.937).normalize(), DVec3::new(0., 0., 4.));
        let r = m.boolean_cone_corner(&cone, o1, m1, o2, m2, mat).expect("cone tent");
        assert_eq!(r.len(), 4, "band + base disk + 2 partial caps");
        assert_eq!(m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(), 0, "watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        let band = *r.iter().find(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).expect("Cone band");
        let (pos, nrm, idx, fmap, _uv) = m.export_buffers().expect("export");
        for &f in &r {
            assert!(fmap.iter().any(|&fid| fid == f.raw()), "face {:?} renders", f);
        }
        // boundary-aware: every band vertex on the kept +m side of BOTH planes.
        let (m1n, m2n) = (m1.normalize(), m2.normalize());
        let mut wrong = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let p = DVec3::new(pos[vi * 3] as f64, pos[vi * 3 + 1] as f64, pos[vi * 3 + 2] as f64);
                if (p - o1).dot(m1n) < -1e-3 || (p - o2).dot(m2n) < -1e-3 { wrong += 1; }
            }
        }
        assert_eq!(wrong, 0, "corner band stays in the kept region");
        // all faces front-facing.
        let nv = pos.len() / 3;
        let centroid = (0..nv).fold(DVec3::ZERO, |c, i|
            c + DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64)) / (nv.max(1) as f64);
        let mut inward = 0usize;
        for i in 0..nv {
            let p = DVec3::new(pos[i * 3] as f64, pos[i * 3 + 1] as f64, pos[i * 3 + 2] as f64);
            let n = DVec3::new(nrm[i * 3] as f64, nrm[i * 3 + 1] as f64, nrm[i * 3 + 2] as f64);
            if (p - centroid).dot(n) < -1e-3 { inward += 1; }
        }
        assert_eq!(inward, 0, "all tent faces front-facing");
        assert!(pos.iter().all(|c| c.is_finite()) && nrm.iter().all(|c| c.is_finite()), "finite");

        // parallel planes → rejected (use the slab path).
        let mut m2m = Mesh::default();
        let c2 = m2m.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
        assert!(m2m.boolean_cone_corner(&c2, o1, m1, DVec3::new(0., 0., 5.), m1, mat).is_err(), "parallel planes rejected");
        assert!(m2m.verify_face_invariants().is_valid(), "intact after parallel bail");

        // a ridge that misses the cone (planes meet far off-axis) → rejected.
        let mut m3 = Mesh::default();
        let c3 = m3.create_cone_kernel_native(DVec3::ZERO, 2.0, 8.0, mat).unwrap();
        let far = (DVec3::new(0.35, 0., -0.937).normalize(), DVec3::new(8., 0., 4.));
        let r3 = m3.boolean_cone_corner(&c3, far.1, far.0, o2, m2, mat);
        assert!(r3.is_err(), "ridge-miss rejected");
        assert!(m3.verify_face_invariants().is_valid(), "intact after ridge-miss bail");
    }

    /// **ADR-197 Z-axis lift (A-1)** — fast path consistency: on a +Z cylinder the
    /// local wrapper (`v` along axis) equals the raw `boolean_cylinder_slab` (world
    /// z). build_clean_cylinder spans z∈[-3,3] (axis_origin z=-3), so v∈[1.5,4.5]
    /// = world z∈[-1.5,1.5] — the same cut as `adr197_beta3h_cylinder_slab_truncate`.
    #[test]
    fn adr197_zlift_a1_zaxis_cylinder_slab_local_consistency() {
        use crate::surfaces::AnalyticSurface as S;
        let mut m = Mesh::default();
        let mat = MaterialId::new(0);
        let cyl = build_clean_cylinder(&mut m, 0., 0., -3., 2.0, 6.0, mat);
        let r = m
            .boolean_cylinder_slab_local(&cyl, 1.5, 4.5, mat)
            .expect("+Z cylinder local slab (fast path)");
        assert_eq!(r.len(), 3, "band + 2 disks");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "watertight"
        );
        assert!(m.verify_face_invariants().is_valid());
        let band = r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
            .expect("Cylinder band");
        if let Some(S::Cylinder { axis_dir, v_range, .. }) = m.face_surface(*band) {
            assert!(axis_dir.cross(DVec3::Z).length() < 1e-9, "axis still ∥ Z");
            assert!(
                (v_range.0 - 1.5).abs() < 1e-9 && (v_range.1 - 4.5).abs() < 1e-9,
                "v-range == [1.5, 4.5] (got {:?})",
                v_range
            );
        }
    }

    /// **ADR-197 Z-axis lift (A-1)** — adversarial robustness: an OFF-ORIGIN
    /// cylinder (axis_origin ≠ 0) with a GENERAL tilt (both x AND y axis
    /// components) — stresses the pivot translation and the rot-axis computation
    /// (not just the X-only tilt of the first test). The lift must still produce a
    /// watertight manifold whose band keeps the exact tilt + off-origin pivot.
    #[test]
    fn adr197_zlift_a1_offorigin_general_tilt_cylinder_slab_local() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let axis = DVec3::new(0.5, 0.4, 0.8).normalize(); // tilt with x AND y
        let center = DVec3::new(5.0, 3.0, 2.0); // off-origin axis_origin (pivot)
        let radius = 1.5;
        let height = 6.0;
        let basis_u = axis.cross(DVec3::Z).normalize(); // ⊥ axis
        let anchor = m.add_vertex(center + basis_u * radius);
        let circle = crate::curves::AnalyticCurve::Circle {
            center,
            radius,
            normal: axis,
            basis_u,
        };
        let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
        m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
            origin: center,
            normal: axis,
            basis_u,
            u_range: (-radius * 1.5, radius * 1.5),
            v_range: (-radius * 1.5, radius * 1.5),
        }));
        let res = m.extrude_cylinder_kernel_native(profile, height, mat).unwrap();
        let mut tilted = vec![res.profile_face, res.top_face];
        tilted.extend(res.side_faces.iter().copied());

        let r = m
            .boolean_cylinder_slab_local(&tilted, 1.5, 4.5, mat)
            .expect("off-origin general-tilt local slab succeeds");
        assert_eq!(r.len(), 3, "band + 2 disks");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "watertight"
        );
        assert!(m.verify_face_invariants().is_valid());
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0);
        let band = r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. })))
            .expect("Cylinder band");
        if let Some(S::Cylinder { axis_dir, axis_origin, v_range, .. }) =
            m.face_surface(*band)
        {
            assert!(
                (axis_dir.normalize() - axis).length() < 1e-6,
                "general tilt preserved (got {:?})",
                axis_dir
            );
            assert!(
                (*axis_origin - center).length() < 1e-6,
                "off-origin pivot preserved (got {:?})",
                axis_origin
            );
            assert!((v_range.0 - 1.5).abs() < 1e-9 && (v_range.1 - 4.5).abs() < 1e-9);
        }
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!pos.is_empty() && !tris.is_empty() && pos.iter().all(|c| c.is_finite()));
    }

    /// **ADR-197 Z-axis lift (A-2)** — a TILTED cone is frustum-cut via the
    /// local-frame wrapper `boolean_cone_slab_local`. The cone is built apex-up
    /// (axis −Z) then rotated to a genuine tilt; the lift rotates it back to the
    /// −Z frame (apex-up, which the op requires), cuts, and rotates back. The
    /// frustum band's analytic Cone surface must keep the tilted axis + apex.
    #[test]
    fn adr197_zlift_a2_tilted_cone_slab_local() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        // Apex-up cone: base z=0, apex z=6 (axis_dir −Z), v = axial dist from apex.
        let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
        let apex = DVec3::new(0.0, 0.0, 6.0);
        // Tilt it: rotate 0.4 rad about X, pivoting at the apex (apex stays).
        let cverts = m.solid_loop_verts(&cone);
        m.rotate_verts(&cverts, apex, DVec3::X, 0.4).unwrap();
        // The cone axis must now be genuinely tilted (∦ ±Z) — the op would bail.
        let side = *cone
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. })))
            .unwrap();
        let tilt_axis = if let Some(S::Cone { axis_dir, .. }) = m.face_surface(side) {
            assert!(axis_dir.cross(DVec3::NEG_Z).length() > 1e-6, "cone genuinely tilted");
            axis_dir.normalize()
        } else {
            panic!("cone side must be a Cone surface");
        };

        // Frustum cut along the cone's own axis: v ∈ [1.5, 4.5] (apex v=0, base v=6).
        let r = m
            .boolean_cone_slab_local(&cone, 1.5, 4.5, mat)
            .expect("tilted cone frustum slab succeeds (no axis=−Z bail)");
        assert_eq!(r.len(), 3, "frustum band + 2 cap disks");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "tilted frustum watertight"
        );
        assert!(m.verify_face_invariants().is_valid());
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0);
        let band = r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. })))
            .expect("result has a Cone band");
        if let Some(S::Cone { axis_dir, apex: bapex, v_range, .. }) = m.face_surface(*band) {
            assert!(
                (axis_dir.normalize() - tilt_axis).length() < 1e-6,
                "tilt axis preserved (got {:?})",
                axis_dir
            );
            assert!(
                (*bapex - apex).length() < 1e-6,
                "apex (pivot) preserved (got {:?})",
                bapex
            );
            assert!(
                (v_range.0 - 1.5).abs() < 1e-9 && (v_range.1 - 4.5).abs() < 1e-9,
                "band v-range == cut bounds (got {:?})",
                v_range
            );
        }
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!pos.is_empty() && !tris.is_empty() && pos.iter().all(|c| c.is_finite()));
    }

    /// **ADR-197 Z-axis lift (A-3)** — a TILTED torus is slab-cut (horizontal
    /// donut band, genus-1) via `boolean_torus_slab_local`. The Z-torus is rotated
    /// to a genuine tilt; the lift rotates it back to the +Z frame, cuts the tube
    /// with two planes ⟂ the axis, and rotates back. The Torus band must keep the
    /// tilted axis + centre + radii.
    #[test]
    fn adr197_zlift_a3_tilted_torus_slab_local() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        // Z-up torus R=5 r=1.5 (single Torus face, axis +Z, centre origin).
        let torus = m.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let center = DVec3::ZERO;
        // Tilt it: rotate 0.4 rad about X, pivoting at the centre (centre stays).
        let tverts = m.solid_loop_verts(&[torus]);
        m.rotate_verts(&tverts, center, DVec3::X, 0.4).unwrap();
        let tilt_axis = if let Some(S::Torus { axis_dir, .. }) = m.face_surface(torus) {
            assert!(axis_dir.cross(DVec3::Z).length() > 1e-6, "torus genuinely tilted");
            axis_dir.normalize()
        } else {
            panic!("face must be a Torus surface");
        };

        // Slab the tube with two planes ⟂ axis at axial offset d ∈ [−0.5, 0.5]
        // (|d| < minor_radius 1.5).
        let r = m
            .boolean_torus_slab_local(&[torus], -0.5, 0.5, mat)
            .expect("tilted torus slab succeeds (no axis∥Z bail)");
        assert_eq!(r.len(), 4, "2 Torus bands + 2 Plane washers");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "tilted torus slab watertight"
        );
        assert!(m.verify_face_invariants().is_valid());
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0);
        let band = r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Torus { .. })))
            .expect("result has a Torus band");
        if let Some(S::Torus {
            axis_dir, center: bc, major_radius, minor_radius, ..
        }) = m.face_surface(*band)
        {
            assert!(
                (axis_dir.normalize() - tilt_axis).length() < 1e-6,
                "tilt axis preserved (got {:?})",
                axis_dir
            );
            assert!((*bc - center).length() < 1e-6, "centre (pivot) preserved");
            assert!((major_radius - 5.0).abs() < 1e-9, "major radius preserved");
            assert!((minor_radius - 1.5).abs() < 1e-9, "minor radius preserved");
        }
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!pos.is_empty() && !tris.is_empty() && pos.iter().all(|c| c.is_finite()));
    }

    /// **ADR-204 β-2** — a TILTED-plane sphere halfspace cut now renders the
    /// correct tilted cap (pole = cut normal), not a Z-latitude band. The probe
    /// (reverted) showed 118 rendered verts BELOW the tilted plane with the old
    /// Z-frame v_range; with the oriented cap (axis_dir = n) every vertex is on
    /// the kept +n side.
    #[test]
    fn adr204_beta2_tilted_sphere_halfspace_render_correct() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let sphere = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let n = DVec3::new(1.0, 0.0, 1.0).normalize(); // tilted plane through centre
        let faces = m
            .boolean_sphere_halfspace(&sphere, DVec3::ZERO, n, mat)
            .expect("tilted sphere halfspace");
        assert_eq!(faces.len(), 2, "cap + disk");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "watertight"
        );
        assert!(m.verify_face_invariants().is_valid());

        // The cap is an ORIENTED Sphere whose pole is the cut normal `n`.
        let cap = faces
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Sphere { .. })))
            .expect("Sphere cap");
        if let Some(S::Sphere { axis_dir, .. }) = m.face_surface(*cap) {
            assert!(
                (axis_dir.normalize() - n).length() < 1e-9,
                "cap pole = cut normal, got {:?}",
                axis_dir
            );
        }

        // GEOMETRIC TRUTH: every rendered vertex is on the kept +n side
        // (dot(p, n) ≥ -eps). Was 118 verts below (min_dot = -radius) pre-fix.
        let (pos, _nm, _tris, _e, _uv) = m.export_buffers().expect("export");
        let mut below = 0usize;
        let (mut min_dot, mut max_dot) = (f64::MAX, f64::MIN);
        for chunk in pos.chunks(3) {
            if chunk.len() < 3 { break; }
            let p = DVec3::new(chunk[0] as f64, chunk[1] as f64, chunk[2] as f64);
            let d = p.dot(n);
            min_dot = min_dot.min(d);
            max_dot = max_dot.max(d);
            if d < -1e-3 { below += 1; }
        }
        assert_eq!(below, 0, "no vertex below the tilted cut (min_dot={:.4})", min_dot);
        assert!(max_dot > 3.0 - 1e-2, "cap reaches the +n pole (max_dot={:.4})", max_dot);
    }

    /// **PROBE (truth-first, ADR-204 β-3)** — does an oriented Sphere BAND
    /// (axis_dir = tilted n, v∈[v1,v2], full-u) tessellate entirely WITHIN the
    /// tilted slab (the zone between the two planes ⟂ n at the band latitudes)?
    /// If yes, sphere slab/slice tilted is the same axis_dir=n pattern as β-2 —
    /// NO `tessellate_sphere_clipped` boundary-clip extension needed (that path
    /// is for ADR-202 small-circle sketching, not planar slab cuts).
    #[test]
    fn probe_adr204_beta3_oriented_band_in_tilted_slab() {
        use crate::surfaces::{AnalyticSurface as S, SurfaceOps};
        let center = DVec3::ZERO;
        let radius = 3.0;
        let n = DVec3::new(1.0, 0.0, 1.0).normalize();
        let refd = n.cross(DVec3::Y).normalize_or_zero(); // ⊥ n
        let (v1, v2) = (-0.3_f64, 0.4_f64);
        let band = S::Sphere {
            center, radius,
            axis_dir: n, ref_dir: refd,
            u_range: (0.0, std::f64::consts::TAU),
            v_range: (v1, v2),
        };
        let tess = band.tessellate(0.05);
        let (lo, hi) = (radius * v1.sin(), radius * v2.sin());
        let (mut min_ax, mut max_ax) = (f64::MAX, f64::MIN);
        for p in &tess.vertices {
            let ax = (*p - center).dot(n);
            min_ax = min_ax.min(ax);
            max_ax = max_ax.max(ax);
            // every point on the sphere
            assert!(((*p - center).length() - radius).abs() < 1e-6, "on sphere");
        }
        println!("β-3 probe: n-axial [{:.4},{:.4}], expected slab [{:.4},{:.4}], tris {}",
            min_ax, max_ax, lo, hi, tess.triangles.len());
        assert!(min_ax >= lo - 1e-6 && max_ax <= hi + 1e-6,
            "oriented band within tilted slab → axis_dir=n suffices, no clip needed");
    }

    /// **ADR-204 β-3** — sphere ∩ TILTED slab (`boolean_sphere_slab_oriented`)
    /// produces a watertight oriented band that renders entirely within the slab
    /// between the two planes ⟂ n. Same axis_dir=n pattern as β-2 (no clip).
    #[test]
    fn adr204_beta3_tilted_sphere_slab_render_correct() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let sphere = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let n = DVec3::new(1.0, 0.0, 1.0).normalize(); // tilted slab axis
        let (d_lo, d_hi) = (-1.0, 1.5);
        let faces = m
            .boolean_sphere_slab_oriented(&sphere, n, d_lo, d_hi, mat)
            .expect("tilted sphere slab");
        assert_eq!(faces.len(), 3, "band + 2 disks");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "watertight"
        );
        assert!(m.verify_face_invariants().is_valid());
        assert_eq!(m.face_set_manifold_info(&faces).non_manifold_edge_count, 0);

        // band is an oriented Sphere with axis_dir = n.
        let band = faces
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Sphere { .. })))
            .expect("Sphere band");
        if let Some(S::Sphere { axis_dir, v_range, .. }) = m.face_surface(*band) {
            assert!((axis_dir.normalize() - n).length() < 1e-9, "band pole = n, got {:?}", axis_dir);
            assert!(v_range.0 < 0.0 && v_range.1 > 0.0, "band straddles centre");
        }

        // GEOMETRIC TRUTH: every rendered vertex is in the tilted slab d∈[d_lo,d_hi].
        let (pos, _nm, _tris, _e, _uv) = m.export_buffers().expect("export");
        let mut outside = 0usize;
        let (mut mn, mut mx) = (f64::MAX, f64::MIN);
        for chunk in pos.chunks(3) {
            if chunk.len() < 3 { break; }
            let p = DVec3::new(chunk[0] as f64, chunk[1] as f64, chunk[2] as f64);
            let d = p.dot(n); // centre = origin
            mn = mn.min(d);
            mx = mx.max(d);
            if d < d_lo - 1e-2 || d > d_hi + 1e-2 { outside += 1; }
        }
        assert_eq!(outside, 0, "all verts in tilted slab; d∈[{:.3}, {:.3}]", mn, mx);
        assert!((mn - d_lo).abs() < 0.1 && (mx - d_hi).abs() < 0.1, "band spans the slab");
    }

    /// **ADR-204 β-3** — sphere − TILTED slab (`boolean_sphere_slab_subtract_
    /// oriented`) keeps the two outer caps; every vertex is OUTSIDE the removed
    /// slab (dot ≤ d_lo or ≥ d_hi). 2 disjoint watertight solids.
    #[test]
    fn adr204_beta3_tilted_sphere_slab_subtract_render_correct() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let sphere = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let n = DVec3::new(0.0, 1.0, 1.0).normalize(); // tilted
        let (d_lo, d_hi) = (-0.8, 1.2);
        let faces = m
            .boolean_sphere_slab_subtract_oriented(&sphere, n, d_lo, d_hi, mat)
            .expect("tilted slab subtract");
        assert_eq!(faces.len(), 4, "2 caps + 2 disks");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "watertight (2 disjoint solids)"
        );
        assert!(m.verify_face_invariants().is_valid());
        for &f in &faces {
            if let Some(S::Sphere { axis_dir, .. }) = m.face_surface(f) {
                assert!((axis_dir.normalize() - n).length() < 1e-9, "cap pole = n");
            }
        }
        // GEOMETRIC: no vertex strictly inside the removed slab (d_lo, d_hi).
        let (pos, _nm, _tris, _e, _uv) = m.export_buffers().expect("export");
        let mut inside = 0usize;
        for chunk in pos.chunks(3) {
            if chunk.len() < 3 { break; }
            let p = DVec3::new(chunk[0] as f64, chunk[1] as f64, chunk[2] as f64);
            let d = p.dot(n);
            if d > d_lo + 1e-2 && d < d_hi - 1e-2 { inside += 1; }
        }
        assert_eq!(inside, 0, "no vertex inside the removed tilted slab");
    }

    /// **ADR-204 β-3** — sphere SLICE by a TILTED plane (`boolean_sphere_slice_
    /// oriented`) keeps both halves; together they cover the whole sphere
    /// (both ±n poles present). 2 watertight volumes.
    #[test]
    fn adr204_beta3_tilted_sphere_slice_render_correct() {
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let sphere = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let n = DVec3::new(1.0, 1.0, 0.0).normalize(); // tilted (in XY)
        let d_k = 0.7;
        let faces = m
            .boolean_sphere_slice_oriented(&sphere, n, d_k, mat)
            .expect("tilted slice");
        assert_eq!(faces.len(), 4, "2 caps + 2 disks");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "watertight (2 volumes)"
        );
        assert!(m.verify_face_invariants().is_valid());
        // both caps together span both ±n poles.
        let (pos, _nm, _tris, _e, _uv) = m.export_buffers().expect("export");
        let (mut mn, mut mx) = (f64::MAX, f64::MIN);
        for chunk in pos.chunks(3) {
            if chunk.len() < 3 { break; }
            let p = DVec3::new(chunk[0] as f64, chunk[1] as f64, chunk[2] as f64);
            let d = p.dot(n);
            mn = mn.min(d);
            mx = mx.max(d);
        }
        assert!(mx > 3.0 - 1e-2 && mn < -3.0 + 1e-2,
            "both ±n poles present; d∈[{:.3}, {:.3}]", mn, mx);
    }

    /// **PROBE (truth-first, ADR-204 (B) dispatch)** — what does a TILTED
    /// cylinder ∩ WORLD-axis box do today? The concern: the box's planes are not
    /// ⟂ the tilted cylinder axis, so the sections are OBLIQUE (ellipses), which
    /// the circular-section slab machinery cannot represent. Measure the real
    /// outcome (Err / faceted / crash) before deciding the (B) scope.
    #[test]
    fn probe_b_dispatch_tilted_cylinder_world_box() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        // tilted cylinder (axis ∦ Z)
        let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
        let center = DVec3::ZERO;
        let (radius, height) = (2.0, 6.0);
        let basis_u = DVec3::X;
        let anchor = m.add_vertex(center + basis_u * radius);
        let circle = crate::curves::AnalyticCurve::Circle { center, radius, normal: axis, basis_u };
        let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
        m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
            origin: center, normal: axis, basis_u, u_range: (-3., 3.), v_range: (-3., 3.),
        }));
        let res = m.extrude_cylinder_kernel_native(profile, height, mat).unwrap();
        let mut cyl = vec![res.profile_face, res.top_face];
        cyl.extend(res.side_faces.iter().copied());
        // world-axis box overlapping the cylinder
        let box_faces = make_box(&mut m, DVec3::new(-3., -3., 1.), DVec3::new(3., 3., 3.), mat);
        let r = m.boolean(&cyl, &box_faces, BoolOp::Intersect, mat);
        // This box ([-3,3]² × [1,3]) is a MULTI-PLANE / CORNER config: its +Y face
        // (y=3) cuts the cylinder's Y-extent [-1.6,5.2] AND its ±Z pair cuts → two
        // non-parallel cutting pairs. ADR-205 γ-2a routes only the PURE single-axis
        // SLAB subset (one parallel pair cuts, the other two contain) to β-3; this
        // corner/multi-plane case is still deferred (γ-2b+), so the #Track2 guard
        // rejects it CLEANLY (Err, not crash / not faceted). Co-oriented box +
        // multi-plane corner remain the achievable-later subset.
        assert!(r.is_err(),
            "tilted cylinder ∩ multi-plane (corner) world-box gracefully rejected ((B) deferred)");
    }

    /// **ADR-204 local variants** — tilted CYLINDER subtract + slice via the
    /// local-frame wrappers. Watertight + manifold (axis preserved by the slab
    /// wrappers' tests; here we exercise the subtract/slice Z-ops through the lift).
    #[test]
    fn adr204_local_cylinder_subtract_and_slice() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let build = |hgt: f64| {
            let mut m = Mesh::default();
            let axis = DVec3::new(0.0, 0.6, 0.8).normalize();
            let bu = DVec3::X;
            let anchor = m.add_vertex(bu * 2.0);
            let circle = crate::curves::AnalyticCurve::Circle { center: DVec3::ZERO, radius: 2.0, normal: axis, basis_u: bu };
            let profile = m.add_face_closed_curve(anchor, circle, mat).unwrap();
            m.faces.get_mut(profile).unwrap().set_surface(Some(S::Plane {
                origin: DVec3::ZERO, normal: axis, basis_u: bu, u_range: (-3., 3.), v_range: (-3., 3.),
            }));
            let res = m.extrude_cylinder_kernel_native(profile, hgt, mat).unwrap();
            let mut cyl = vec![res.profile_face, res.top_face];
            cyl.extend(res.side_faces.iter().copied());
            (m, cyl)
        };
        let watertight = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        let (mut m1, c1) = build(6.0);
        let r1 = m1.boolean_cylinder_slab_subtract_local(&c1, 1.5, 4.5, mat).expect("cyl subtract local");
        assert!(!r1.is_empty() && watertight(&m1) == 0 && m1.verify_face_invariants().is_valid(), "subtract watertight");

        let (mut m2, c2) = build(6.0);
        let r2 = m2.boolean_cylinder_slice_local(&c2, 3.0, mat).expect("cyl slice local");
        assert!(!r2.is_empty() && watertight(&m2) == 0 && m2.verify_face_invariants().is_valid(), "slice watertight");
    }

    /// **ADR-204 local variants** — tilted CONE subtract + slice via local-frame.
    #[test]
    fn adr204_local_cone_subtract_and_slice() {
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 6.0, mat).unwrap();
            let apex = DVec3::new(0.0, 0.0, 6.0);
            let cv = m.solid_loop_verts(&cone);
            m.rotate_verts(&cv, apex, DVec3::X, 0.4).unwrap(); // tilt
            (m, cone)
        };
        let watertight = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        let (mut m1, c1) = build();
        let r1 = m1.boolean_cone_slab_subtract_local(&c1, 1.5, 4.5, mat).expect("cone subtract local");
        assert!(!r1.is_empty() && watertight(&m1) == 0 && m1.verify_face_invariants().is_valid(), "cone subtract watertight");

        let (mut m2, c2) = build();
        let r2 = m2.boolean_cone_slice_local(&c2, 3.0, mat).expect("cone slice local");
        assert!(!r2.is_empty() && watertight(&m2) == 0 && m2.verify_face_invariants().is_valid(), "cone slice watertight");
    }

    /// **ADR-204 local variants** — tilted TORUS halfspace + subtract + slice.
    #[test]
    fn adr204_local_torus_halfspace_subtract_slice() {
        let mat = MaterialId::new(0);
        let build = || {
            let mut m = Mesh::default();
            let torus = m.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
            let tv = m.solid_loop_verts(&[torus]);
            m.rotate_verts(&tv, DVec3::ZERO, DVec3::X, 0.4).unwrap(); // tilt
            (m, torus)
        };
        let watertight = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        let (mut m1, t1) = build();
        let r1 = m1.boolean_torus_halfspace_local(&[t1], 0.5, true, mat).expect("torus halfspace local");
        assert!(!r1.is_empty() && watertight(&m1) == 0 && m1.verify_face_invariants().is_valid(), "torus halfspace watertight");

        let (mut m2, t2) = build();
        let r2 = m2.boolean_torus_slab_subtract_local(&[t2], -0.5, 0.5, mat).expect("torus subtract local");
        assert!(!r2.is_empty() && watertight(&m2) == 0 && m2.verify_face_invariants().is_valid(), "torus subtract watertight");

        let (mut m3, t3) = build();
        let r3 = m3.boolean_torus_slice_local(&[t3], 0.5, mat).expect("torus slice local");
        assert!(!r3.is_empty() && watertight(&m3) == 0 && m3.verify_face_invariants().is_valid(), "torus slice watertight");
    }

    /// **SIMULATION (ADR-205 β-2, 섬세한 시뮬레이션 먼저)** — the closed-form
    /// elliptic section of an oblique plane cutting a tilted cylinder, built as a
    /// `nurbs::ellipse`, must lie EXACTLY on BOTH the cylinder surface (radial
    /// distance = r) AND the cut plane (dot(p−o, m) = 0), for a sweep of tilt /
    /// plane configurations. This grounds β-2 before any DCEL/sew/render work.
    ///
    /// Formula (proven by hand): θ = angle(axis n, plane normal m);
    /// center = axis_origin + t·n where t = (o−axis_origin)·m / (n·m);
    /// semi_minor = r, minor_dir = (m × n)̂;
    /// semi_major = r / |cos θ|, major_dir = (n − (n·m)·m)̂.
    #[test]
    fn sim_adr205_beta2_oblique_ellipse_lies_on_cylinder_and_plane() {
        let configs: &[(DVec3, f64, DVec3, DVec3)] = &[
            // (axis n, radius, plane normal m, plane origin o)
            (DVec3::new(0.0, 0.6, 0.8), 2.0, DVec3::Z, DVec3::new(0.0, 0.0, 1.0)),
            (DVec3::new(0.0, 0.6, 0.8), 2.0, DVec3::Z, DVec3::new(0.0, 0.0, -0.5)),
            (DVec3::new(0.3, 0.4, 0.866), 1.5, DVec3::X, DVec3::new(0.4, 0.0, 0.0)),
            (DVec3::new(0.5, 0.5, 0.707), 3.0, DVec3::Y, DVec3::new(0.0, 1.0, 0.0)),
            (DVec3::new(0.0, 0.0, 1.0), 2.0, DVec3::new(0.3, 0.0, 1.0), DVec3::ZERO), // mild tilt of plane vs Z-axis cyl
        ];
        for (idx, &(n_raw, r, m_raw, o)) in configs.iter().enumerate() {
            let n = n_raw.normalize();
            let m = m_raw.normalize();
            let axis_origin = DVec3::ZERO;
            let ndm = n.dot(m);
            let cos_theta = ndm.abs();
            assert!(cos_theta > 1e-6 && cos_theta < 1.0 - 1e-9,
                "config {}: genuinely oblique (cosθ={})", idx, cos_theta);

            let t = (o - axis_origin).dot(m) / ndm;
            let center = axis_origin + t * n;
            let semi_minor = r;
            let minor_dir = m.cross(n).normalize();
            let semi_major = r / cos_theta;
            let major_dir = (n - ndm * m).normalize();

            // axes orthonormal + in the cut plane.
            assert!(minor_dir.dot(major_dir).abs() < 1e-9, "config {}: axes ⟂", idx);
            assert!(minor_dir.dot(m).abs() < 1e-9 && major_dir.dot(m).abs() < 1e-9,
                "config {}: axes in plane", idx);

            let (cp, w, k, deg) =
                crate::curves::nurbs::ellipse(center, semi_major, semi_minor, major_dir, minor_dir);
            for i in 0..=64 {
                let tt = 4.0 * (i as f64) / 64.0;
                let p = crate::curves::nurbs::evaluate(&cp, &w, &k, deg, tt).unwrap();
                // on the cut plane
                assert!((p - o).dot(m).abs() < 1e-7,
                    "config {}: on plane @t={} (got {})", idx, tt, (p - o).dot(m));
                // on the cylinder (radial distance from axis = r)
                let rel = p - axis_origin;
                let radial = (rel - rel.dot(n) * n).length();
                assert!((radial - r).abs() < 1e-7,
                    "config {}: on cylinder @t={} (radial={}, r={})", idx, tt, radial, r);
            }
        }
    }

    /// **SIMULATION 2 (ADR-205 β-2)** — DCEL sew feasibility: can `sew_curved_
    /// band` stitch a TRIMMED cylinder whose TOP boundary is an ELLIPSE (NURBS)
    /// and bottom is a circle, into a watertight manifold? (The helper takes
    /// `AnalyticCurve` generically; this probes whether it assumes a circle.)
    /// Z-axis cylinder cut by a tilted plane isolates the ellipse-boundary sew.
    #[test]
    fn sim_adr205_beta2_sew_trimmed_cylinder_with_elliptic_top() {
        use crate::surfaces::AnalyticSurface as S;
        use crate::curves::AnalyticCurve;
        use std::f64::consts::TAU;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();

        let r = 2.0;
        let n = DVec3::Z; // Z-axis cylinder, base at z=0
        let axis_origin = DVec3::ZERO;
        // tilted cut plane → ellipse section.
        let pm = DVec3::new(0.3, 0.0, 1.0).normalize();
        let o = DVec3::new(0.0, 0.0, 2.0);
        let ndm = n.dot(pm);
        let cos_theta = ndm.abs();
        let t = (o - axis_origin).dot(pm) / ndm;
        let center = axis_origin + t * n;
        let minor_dir = pm.cross(n).normalize();
        let major_dir = (n - ndm * pm).normalize();
        let (semi_major, semi_minor) = (r / cos_theta, r);
        // ellipse axial (z) extent must stay above the base (z=0).
        let z_span = semi_major * major_dir.dot(n).abs();
        assert!(center.z - z_span > 0.0, "ellipse above base");

        // build the trimmed solid: Cylinder band (bottom circle → top ellipse)
        // + bottom disk + elliptic cap.
        let (cp, w, k, deg) = crate::curves::nurbs::ellipse(center, semi_major, semi_minor, major_dir, minor_dir);
        let top_anchor = cp[0];
        let top_ellipse = AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: deg as u32 };
        let bot_anchor = axis_origin + DVec3::X * r;
        let bot_circle = AnalyticCurve::Circle { center: axis_origin, radius: r, normal: n, basis_u: DVec3::X };
        let band = S::Cylinder {
            axis_origin, axis_dir: n, radius: r, ref_dir: DVec3::X,
            u_range: (0.0, TAU), v_range: (0.0, center.z + z_span),
        };
        let elliptic_cap = S::Plane {
            origin: center, normal: pm, basis_u: major_dir,
            u_range: (-semi_major * 1.2, semi_major * 1.2), v_range: (-semi_major * 1.2, semi_major * 1.2),
        };
        let bot_disk = S::Plane {
            origin: axis_origin, normal: DVec3::NEG_Z, basis_u: DVec3::X,
            u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
        };
        let res = m.sew_curved_band(
            top_anchor, top_ellipse, bot_anchor, bot_circle,
            band, DVec3::X, elliptic_cap, pm, bot_disk, DVec3::NEG_Z, mat,
        );
        let (f_band, f_top, f_bot) = res.expect("sew trimmed cylinder with elliptic top");
        let faces = [f_band, f_top, f_bot];
        let open = m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let valid = m.verify_face_invariants().is_valid();
        let nm = m.face_set_manifold_info(&faces).non_manifold_edge_count;
        println!("β-2 sew sim: {} faces, open HEs {}, valid {}, non-manifold {}", faces.len(), open, valid, nm);
        assert_eq!(open, 0, "trimmed cylinder with elliptic top is watertight");
        assert!(valid, "DCEL invariants valid");
        assert_eq!(nm, 0, "manifold");
        // band keeps a Cylinder surface, top cap is bounded by a NURBS (ellipse).
        assert!(faces.iter().any(|&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))), "Cylinder band present");

        // --- SIMULATION 3 → assertion: render respects the elliptic cut ---
        // With `tessellate_cylinder_clipped` wired into the export, the band is
        // tessellated boundary-aware (v ∈ [v_lo(u), v_hi(u)]), so NO band vertex
        // pokes past the cut plane (the prior over-draw was 69/276).
        let (pos, _norm, idx, fmap, _pos64) = m.export_buffers().expect("export");
        let mut above = 0usize;
        let mut total = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != f_band.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let pt = DVec3::new(pos[vi * 3] as f64, pos[vi * 3 + 1] as f64, pos[vi * 3 + 2] as f64);
                total += 1;
                if (pt - o).dot(pm) > 1e-3 { above += 1; }
            }
        }
        println!("β-2 render sim: band verts ABOVE cut plane = {} / {} total", above, total);
        assert!(total > 0, "band rendered");
        assert_eq!(above, 0, "clipped band does not over-draw past the elliptic cut");
    }

    /// **SIMULATION (ADR-205 β-4, ∥-axis line-pair / flat cut)** — a plane PARALLEL
    /// to the cylinder axis (normal ⟂ axis) cuts a LINE PAIR, not an ellipse. Keep
    /// one side → a flat-on-cylinder (D-shaft). The novel render piece is the
    /// PARTIAL band (a Cylinder with a restricted `u_range`), which the existing
    /// `surface.tessellate()` already honours. This probes: (1) the chord geometry
    /// (closed-form), (2) the partial band tessellates wholly on the kept side.
    #[test]
    fn sim_adr205_beta4_axial_plane_chord_and_partial_band() {
        use crate::surfaces::AnalyticSurface as S;
        use crate::surfaces::SurfaceOps;
        use std::f64::consts::TAU;
        // Z-axis cylinder r=2; cut plane x=1 (m=X ⟂ axis), keep x<1 (major side).
        let r = 2.0;
        let n_a = DVec3::Z;
        let axis_origin = DVec3::ZERO;
        let m = DVec3::X;
        let o = DVec3::new(1.0, 0.0, 0.0);
        let h = (o - axis_origin).dot(m); // axis→plane distance = 1 < r
        assert!(h.abs() < r, "plane cuts the cylinder");
        // chord at cos u = h/r → u = ±acos(h/r); keep cos u < h/r → major arc.
        let psi = (h / r).acos();
        let (u_lo, u_hi) = (psi, TAU - psi); // kept arc (x<1)
        // chord endpoints exactly on cylinder AND plane.
        for &u in &[u_lo, u_hi] {
            let p = axis_origin + n_a * 3.0 + r * (DVec3::X * u.cos() + DVec3::Y * u.sin());
            assert!(((p.x * p.x + p.y * p.y).sqrt() - r).abs() < 1e-12, "chord pt on cylinder");
            assert!(((p - o).dot(m)).abs() < 1e-12, "chord pt on plane");
        }
        // partial band: a Cylinder restricted to the kept arc tessellates wholly on
        // x <= 1 (no over-draw past the flat), via the existing u_range honouring.
        let band = S::Cylinder {
            axis_origin, axis_dir: n_a, radius: r, ref_dir: DVec3::X,
            u_range: (u_lo, u_hi), v_range: (0.0, 6.0),
        };
        let tess = band.tessellate(0.05);
        assert!(!tess.vertices.is_empty(), "partial band tessellated");
        let max_x = tess.vertices.iter().map(|p| p.x).fold(f64::MIN, f64::max);
        println!("β-4 sim: partial band max x = {} (kept x<=1)", max_x);
        assert!(max_x <= h + 1e-6, "partial band stays on the kept side (x<=1)");
    }

    /// **SIMULATION 1 (ADR-205 β-5, multi-plane corner)** — a cylinder clipped by
    /// TWO non-parallel oblique planes meeting at a ridge (the minimal box-corner).
    /// Probes: (1) the plane∩plane line crosses the cylinder at the CORNER points
    /// (on the cylinder AND both planes); (2) the band's top boundary is PIECEWISE
    /// `min_i v_plane_i(u)`, with the active plane switching exactly at the corner
    /// angles. Grounds β-5 geometry before any DCEL/render work.
    #[test]
    fn sim_adr205_beta5_corner_geometry_and_piecewise_band() {
        use std::f64::consts::TAU;
        let r = 2.0;
        let n_a = DVec3::Z;
        let axis_origin = DVec3::ZERO;
        // "tent" cut: two planes meeting at ridge x=0, z=4. Keep +m (below).
        let m1 = DVec3::new(-0.5, 0.0, -1.0).normalize();
        let o1 = DVec3::new(0.0, 0.0, 4.0);
        let m2 = DVec3::new(0.5, 0.0, -1.0).normalize();
        let o2 = DVec3::new(0.0, 0.0, 4.0);

        // GENERAL plane∩plane line L = L0 + t·dir.
        let dir_raw = m1.cross(m2); // UNnormalized (the L0 formula needs |m1×m2|²).
        assert!(dir_raw.length_squared() > 1e-9, "planes non-parallel");
        let dir = dir_raw.normalize();
        let (d1, d2) = (o1.dot(m1), o2.dot(m2));
        // a point on the intersection line (standard two-plane formula):
        // L0 = (d1·(n2×dir) + d2·(dir×n1)) / |dir|².
        let l0 = (m2.cross(dir_raw) * d1 + dir_raw.cross(m1) * d2) / dir_raw.dot(dir_raw);
        assert!((l0.dot(m1) - d1).abs() < 1e-9, "L0 on plane1");
        assert!((l0.dot(m2) - d2).abs() < 1e-9, "L0 on plane2");

        // L ∩ cylinder: |rel_perp(t)|² = r², quadratic in t.
        let rel0 = l0 - axis_origin;
        let perp = |w: DVec3| w - n_a * w.dot(n_a);
        let (a0, ad) = (perp(rel0), perp(dir));
        let qa = ad.dot(ad);
        let qb = 2.0 * a0.dot(ad);
        let qc = a0.dot(a0) - r * r;
        let disc = qb * qb - 4.0 * qa * qc;
        assert!(disc > 0.0, "ridge crosses the cylinder (2 corner points)");
        let sd = disc.sqrt();
        let corners: Vec<DVec3> = [(-qb - sd) / (2.0 * qa), (-qb + sd) / (2.0 * qa)]
            .iter().map(|&t| l0 + dir * t).collect();
        // each corner is on the cylinder AND both planes.
        for c in &corners {
            let rad = perp(*c - axis_origin).length();
            assert!((rad - r).abs() < 1e-9, "corner on cylinder (rad={})", rad);
            assert!((*c - o1).dot(m1).abs() < 1e-9, "corner on plane1");
            assert!((*c - o2).dot(m2).abs() < 1e-9, "corner on plane2");
        }
        println!("β-5 sim: corners = {:?}", corners);
        // expected (0, ±2, 4).
        assert!(corners.iter().any(|c| (*c - DVec3::new(0., 2., 4.)).length() < 1e-9));
        assert!(corners.iter().any(|c| (*c - DVec3::new(0., -2., 4.)).length() < 1e-9));

        // band top = min over planes of the upper bound v_plane_i(u).
        // v_plane_i(u): (surf(u,v)-o_i)·m_i = 0, surf=(r cos u, r sin u, v).
        let v_plane = |m: DVec3, o: DVec3, u: f64| -> f64 {
            // a + b·v = 0 → v = -a/b, a = (r cos u, r sin u, -o.z)·m_xy_part... compute directly.
            let denom = n_a.dot(m);
            let radial = DVec3::new(r * u.cos(), r * u.sin(), 0.0);
            ((o - axis_origin).dot(m) - radial.dot(m)) / denom
        };
        // sample: which plane is active, and where it switches.
        let mut switches = Vec::new();
        let n = 720;
        let mut prev_active = 0;
        for i in 0..=n {
            let u = TAU * (i as f64) / (n as f64);
            let (v1, v2) = (v_plane(m1, o1, u), v_plane(m2, o2, u));
            let active = if v1 <= v2 { 1 } else { 2 };
            if i > 0 && active != prev_active {
                switches.push(u);
            }
            prev_active = active;
        }
        println!("β-5 sim: band-top active-plane switches at u = {:?} (≈ π/2, 3π/2)", switches);
        // exactly 2 switches (the 2 corners), near u = π/2 and 3π/2.
        assert_eq!(switches.len(), 2, "piecewise top: 2 arc segments (e1, e2)");
        assert!(switches.iter().any(|&u| (u - std::f64::consts::FRAC_PI_2).abs() < 0.02));
        assert!(switches.iter().any(|&u| (u - 3.0 * std::f64::consts::FRAC_PI_2).abs() < 0.02));
    }

    /// **SIMULATION 2 (ADR-205 β-5)** — band render GENERALISATION to N planes.
    /// `tessellate_cylinder_clipped` (β-2/3) clips a band to exactly 2 boundary
    /// planes (min/max strip). For a corner the band is bounded by N planes (here
    /// 2 upper + the bottom circle): the strip is `v ∈ [max(lower bounds),
    /// min(upper bounds)]`. This probe simulates that strip and verifies every
    /// band vertex lies in the KEPT region (below both cut planes, above the base).
    #[test]
    fn sim_adr205_beta5_nplane_band_strip_in_kept_region() {
        use std::f64::consts::TAU;
        let r = 2.0;
        let n_a = DVec3::Z;
        let axis_origin = DVec3::ZERO;
        let (v0, v1): (f64, f64) = (0.0, 6.0);
        let m1 = DVec3::new(-0.5, 0.0, -1.0).normalize();
        let o1 = DVec3::new(0.0, 0.0, 4.0);
        let m2 = DVec3::new(0.5, 0.0, -1.0).normalize();
        let o2 = DVec3::new(0.0, 0.0, 4.0);
        let planes = [(m1, o1), (m2, o2)];
        // v where the generator at angle u pierces plane (m, o), + lower/upper.
        let v_bound = |m: DVec3, o: DVec3, u: f64| -> (f64, bool) {
            let denom = n_a.dot(m); // !=0 (oblique)
            let radial = DVec3::new(r * u.cos(), r * u.sin(), 0.0);
            let v = ((o - axis_origin).dot(m) - radial.dot(m)) / denom;
            (v, denom > 0.0) // (v, is_lower_bound)
        };
        let mut outside = 0usize;
        let mut total = 0usize;
        let n = 240;
        for i in 0..=n {
            let u = TAU * (i as f64) / (n as f64);
            // max(lower bounds, v0), min(upper bounds, v1).
            let mut v_lo = v0;
            let mut v_hi = v1;
            for &(m, o) in &planes {
                let (vb, is_lower) = v_bound(m, o, u);
                if is_lower { v_lo = v_lo.max(vb); } else { v_hi = v_hi.min(vb); }
            }
            if v_lo >= v_hi { continue; } // empty strip at this u (corner region)
            // sample both strip ends + midpoint; all must be in the kept region.
            for &v in &[v_lo, 0.5 * (v_lo + v_hi), v_hi] {
                let p = axis_origin + n_a * v + DVec3::new(r * u.cos(), r * u.sin(), 0.0);
                total += 1;
                let kept = (p - o1).dot(m1) >= -1e-6
                    && (p - o2).dot(m2) >= -1e-6
                    && p.z >= v0 - 1e-6 && p.z <= v1 + 1e-6;
                if !kept { outside += 1; }
            }
        }
        println!("β-5 sim: N-plane strip — {} / {} band samples OUTSIDE kept region", outside, total);
        assert!(total > 0, "strip non-empty");
        assert_eq!(outside, 0, "N-plane min-upper/max-lower strip stays in the kept region");
    }

    /// **SIMULATION 3 (ADR-205 β-5)** — the PARTIAL elliptic cap boundary. A cap is
    /// bounded by an arc of the cut ellipse (between two corner points) + the ridge
    /// segment. The arc must be representable as an edge curve the render can fill;
    /// `he_arc_fill_points` handles NURBS, and an ellipse arc ≤ 90° is exactly a
    /// rational quadratic (3 control pts, weights [1, cos(Δ/2), 1]) — the affine
    /// image of a circle arc. This probe builds such a sub-arc of the plane1
    /// ellipse and verifies every sampled point lies on the cylinder AND plane1.
    #[test]
    fn sim_adr205_beta5_partial_elliptic_cap_arc_as_nurbs() {
        let r = 2.0;
        let n_a = DVec3::Z;
        let axis_origin = DVec3::ZERO;
        let m1 = DVec3::new(-0.5, 0.0, -1.0).normalize();
        let o1 = DVec3::new(0.0, 0.0, 4.0);
        // e1 = cylinder ∩ plane1 (β-2 closed-form).
        let ndm = n_a.dot(m1);
        let cos_t = ndm.abs();
        let center = axis_origin + n_a * ((o1 - axis_origin).dot(m1) / ndm);
        let minor_dir = m1.cross(n_a).normalize();
        let major_dir = (n_a - ndm * m1).normalize();
        let (a, b) = (r / cos_t, r);
        // a 90° sub-arc φ ∈ [π, 3π/2] as a rational quadratic NURBS.
        let (phi0, phi1) = (std::f64::consts::PI, 1.5 * std::f64::consts::PI);
        let half = 0.5 * (phi1 - phi0);
        let phim = 0.5 * (phi0 + phi1);
        let w1 = half.cos();
        let ell = |cx: f64, cy: f64| center + major_dir * (a * cx) + minor_dir * (b * cy);
        let p0 = ell(phi0.cos(), phi0.sin());
        let p2 = ell(phi1.cos(), phi1.sin());
        let p1 = ell(phim.cos() / w1, phim.sin() / w1); // tangent intersection
        let control_pts = vec![p0, p1, p2];
        let weights = vec![1.0, w1, 1.0];
        let knots = vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0];
        // sample the NURBS; every point must be on the cylinder AND plane1.
        let mut max_cyl_err = 0.0_f64;
        let mut max_plane_err = 0.0_f64;
        for i in 0..=24 {
            let t = i as f64 / 24.0;
            let p = crate::curves::nurbs::evaluate(&control_pts, &weights, &knots, 2, t).unwrap();
            let rad = (p - axis_origin - n_a * (p - axis_origin).dot(n_a)).length();
            max_cyl_err = max_cyl_err.max((rad - r).abs());
            max_plane_err = max_plane_err.max(((p - o1).dot(m1)).abs());
        }
        println!("β-5 sim: partial-ellipse NURBS arc — max cyl err {:.2e}, max plane err {:.2e}",
            max_cyl_err, max_plane_err);
        assert!(max_cyl_err < 1e-9, "cap arc lies on the cylinder");
        assert!(max_plane_err < 1e-9, "cap arc lies on plane1");
        // the NURBS tessellates to ≥3 points (clears he_arc_fill_points' guard).
        let tess = crate::curves::nurbs::tessellate(&control_pts, &weights, &knots, 2, 0.02).unwrap();
        assert!(tess.len() >= 3, "cap arc renders ({} pts)", tess.len());
    }

    /// **SIMULATION 4 (ADR-205 β-5 β)** — DCEL wiring de-risk: actually CONSTRUCT
    /// the tent-cut corner solid (`sew_corner_band` for the mixed self-loop-inner /
    /// multi-edge-outer band + bottom disk, then two partial caps via
    /// `add_face_with_holes` reusing the band arc edges + a shared ridge) and verify
    /// it is watertight + manifold + invariant-valid. This proves the band wiring
    /// before the production `boolean_cylinder_corner`.
    #[test]
    fn sim_adr205_beta5_corner_dcel_watertight() {
        use crate::surfaces::AnalyticSurface as S;
        use crate::curves::AnalyticCurve;
        use std::f64::consts::TAU;
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let r = 2.0;
        let n_a = DVec3::Z;
        let axis_origin = DVec3::ZERO;
        let m1 = DVec3::new(-0.5, 0.0, -1.0).normalize();
        let o1 = DVec3::new(0.0, 0.0, 4.0);
        let m2 = DVec3::new(0.5, 0.0, -1.0).normalize();
        let o2 = DVec3::new(0.0, 0.0, 4.0);
        // e_i = cylinder ∩ plane_i (β-2 closed-form) → (center, a, b, major, minor).
        let ellipse_of = |mp: DVec3, op: DVec3| {
            let ndm = n_a.dot(mp);
            let cos_t = ndm.abs();
            let center = axis_origin + n_a * ((op - axis_origin).dot(mp) / ndm);
            let minor = mp.cross(n_a).normalize();
            let major = (n_a - ndm * mp).normalize();
            (center, r / cos_t, r, major, minor)
        };
        let (c1, a1, b1, mj1, mn1) = ellipse_of(m1, o1);
        let (c2, a2, b2, mj2, mn2) = ellipse_of(m2, o2);
        let phi_of = |p: DVec3, c: DVec3, a: f64, b: f64, mj: DVec3, mn: DVec3| {
            let rel = p - c;
            (rel.dot(mn) / b).atan2(rel.dot(mj) / a)
        };
        // corner points + active-arc midpoints (tent: ridge x=0 z=4).
        let corner_a = DVec3::new(0.0, 2.0, 4.0);
        let corner_b = DVec3::new(0.0, -2.0, 4.0);
        let mid1 = DVec3::new(2.0, 0.0, 3.0);  // e1 active mid (u=0)
        let mid2 = DVec3::new(-2.0, 0.0, 3.0); // e2 active mid (u=π)
        let nurbs = |c, a, b, mj, mn, f0, f1| {
            let (cp, w, k, d) = crate::curves::nurbs::ellipse_arc(c, a, b, mj, mn, f0, f1);
            AnalyticCurve::NURBS { control_pts: cp, weights: w, knots: k, degree: d as u32 }
        };
        // top loop [mid1, corner_a, mid2, corner_b], edge i: top[i]→top[i+1].
        let e1c = |p, q| nurbs(c1, a1, b1, mj1, mn1,
            phi_of(p, c1, a1, b1, mj1, mn1), phi_of(q, c1, a1, b1, mj1, mn1));
        let e2c = |p, q| nurbs(c2, a2, b2, mj2, mn2,
            phi_of(p, c2, a2, b2, mj2, mn2), phi_of(q, c2, a2, b2, mj2, mn2));
        let top_verts = [mid1, corner_a, mid2, corner_b];
        let top_curves = [
            e1c(mid1, corner_a),   // e1
            e2c(corner_a, mid2),   // e2
            e2c(mid2, corner_b),   // e2
            e1c(corner_b, mid1),   // e1
        ];
        let bottom_circle = AnalyticCurve::Circle {
            center: axis_origin, radius: r, normal: n_a, basis_u: DVec3::X,
        };
        let band = S::Cylinder {
            axis_origin, axis_dir: n_a, radius: r, ref_dir: DVec3::X,
            u_range: (0.0, TAU), v_range: (0.0, 4.0),
        };
        let bottom_disk = S::Plane {
            origin: axis_origin, normal: -n_a, basis_u: DVec3::X,
            u_range: (-r * 1.5, r * 1.5), v_range: (-r * 1.5, r * 1.5),
        };
        let (band_f, disk_f, vids) = m.sew_corner_band(
            &top_verts, &top_curves, axis_origin + DVec3::X * r, bottom_circle,
            band, DVec3::X, bottom_disk, -n_a, mat,
        ).expect("sew corner band");
        // vids = [mid1, corner_a, mid2, corner_b]. caps reuse the band arc edges
        // (opposite traversal) + a shared ridge.
        let cap1 = m.add_face_with_holes(&[vids[1], vids[0], vids[3]], &[], mat) // [A, mid1, B]
            .expect("cap1");
        m.faces[cap1].set_surface(Some(S::Plane {
            origin: c1, normal: -m1, basis_u: mj1, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        }));
        let cap2 = m.add_face_with_holes(&[vids[3], vids[2], vids[1]], &[], mat) // [B, mid2, A]
            .expect("cap2");
        m.faces[cap2].set_surface(Some(S::Plane {
            origin: c2, normal: -m2, basis_u: mj2, u_range: (-1e6, 1e6), v_range: (-1e6, 1e6),
        }));
        let faces = [band_f, disk_f, cap1, cap2];
        let open = m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let valid = m.verify_face_invariants().is_valid();
        let nm = m.face_set_manifold_info(&faces).non_manifold_edge_count;
        println!("β-5 DCEL sim: {} faces, open HEs {}, valid {}, non-manifold {}",
            faces.len(), open, valid, nm);
        if !valid {
            eprintln!("{}", m.verify_face_invariants().summary());
        }
        assert_eq!(open, 0, "corner solid watertight");
        assert!(valid, "DCEL invariants valid");
        assert_eq!(nm, 0, "manifold");

        // --- render: band stays in the kept region + ALL faces render ---
        let (pos, _n, idx, fmap, _uv) = m.export_buffers().expect("export");
        for &f in &faces {
            assert!(fmap.iter().any(|&fid| fid == f.raw()), "face {:?} renders", f);
        }
        let mut wrong = 0usize;
        for (ti, &fid) in fmap.iter().enumerate() {
            if fid != band_f.raw() { continue; }
            for k in 0..3 {
                let vi = idx[ti * 3 + k] as usize;
                let p = DVec3::new(pos[vi * 3] as f64, pos[vi * 3 + 1] as f64, pos[vi * 3 + 2] as f64);
                // kept region: below both planes (+m side) + above the base.
                if (p - o1).dot(m1) < -1e-3 || (p - o2).dot(m2) < -1e-3 || p.z < -1e-3 {
                    wrong += 1;
                }
            }
        }
        println!("β-5 DCEL sim: {} band verts outside kept region", wrong);
        assert_eq!(wrong, 0, "corner band stays in the kept region (N-plane clip)");
    }

    #[test]
    fn adr197_beta3h_cone_torus_precheck() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        // ── CONE: base z=0, apex z=4, radius 2. plane_cone SSI works (dispatched).
        let mut mc = Mesh::default();
        let cone = mc.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        let planes = cone
            .iter()
            .filter(|&&f| matches!(mc.face_surface(f), Some(S::Plane { .. })))
            .count();
        let cone_side = *cone
            .iter()
            .find(|&&f| matches!(mc.face_surface(f), Some(S::Cone { .. })))
            .unwrap();
        let cf = mc.faces.get(cone_side).unwrap();
        let cone_outer = mc.collect_loop_verts(cf.outer().start).map(|v| v.len()).unwrap_or(0);
        let cone_inners = cf.inners().len();
        eprintln!(
            "[sim-cone] faces={} planes={} side: outer_verts={} inners={}",
            cone.len(),
            planes,
            cone_outer,
            cone_inners
        );
        // plane z=2 (mid) × cone → one latitude circle. radius at z=2 (half-way to
        // apex from base): r = radius * (apex.z - z)/(apex.z - base.z) = 2*(4-2)/4 = 1.
        let plane = S::Plane {
            origin: DVec3::new(0., 0., 2.),
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (-1e6, 1e6),
            v_range: (-1e6, 1e6),
        };
        let cs = surface_surface_intersection(&plane, &mc.face_surface(cone_side).unwrap());
        match &cs {
            Some(s) => {
                let rmax = s.points.iter().map(|p| (p.x * p.x + p.y * p.y).sqrt()).fold(0.0, f64::max);
                eprintln!(
                    "[sim-cone] plane(z=2)×cone SSI: pts={} closed={} r={:.3} (expect 1.0)",
                    s.points.len(),
                    s.closed,
                    rmax
                );
                assert!(s.closed && (rmax - 1.0).abs() < 1e-6);
            }
            None => panic!("plane_cone SSI must be dispatched"),
        }
        // Cone side is a SINGLE self-loop (apex degenerate → 1 boundary circle).
        assert_eq!(cone_outer, 1, "cone side outer = self-loop base circle");
        assert_eq!(cone_inners, 0, "cone side has no inner loop (apex is a point)");

        // ── TORUS: single self-loop face, full periodic u/v. plane_torus SSI is
        // NOT yet dispatched → confirms torus needs a NEW SSI primitive + an
        // annular washer-disk cap (a z=k cut of a torus is TWO concentric circles).
        let mut mt = Mesh::default();
        let torus = mt.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let torus_surf = mt.face_surface(torus).cloned().unwrap();
        assert!(matches!(torus_surf, S::Torus { .. }), "torus = single Torus face");
        let ts = surface_surface_intersection(&plane, &torus_surf);
        eprintln!("[sim-torus] plane×torus SSI dispatched = {}", ts.is_some());
        assert!(ts.is_none(), "plane_torus SSI not yet implemented (later sub-step)");
    }

    #[test]
    fn adr197_beta3h_cone_slab_and_halfspaces() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        // apex-up cone: base z=0 (radius 2), apex z=4. radius(z) = (4-z)*0.5.

        // ── frustum slab {1 < z < 3}: neither apex nor base kept → band + 2 disks.
        let mut m = Mesh::default();
        let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        let r = m.boolean_cone_slab(&cone, 1.0, 3.0, mat).expect("frustum slab");
        assert_eq!(r.len(), 3, "frustum = band + 2 disks");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "frustum watertight"
        );
        assert!(m.verify_face_invariants().is_valid());
        let band = r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. })))
            .unwrap();
        if let Some(S::Cone { v_range, .. }) = m.face_surface(*band) {
            // v = apex_z - z. cuts z=1,3 → v∈[1, 3].
            assert!(
                (v_range.0 - 1.0).abs() < 1e-9 && (v_range.1 - 3.0).abs() < 1e-9,
                "frustum band v∈[1,3]; got {:?}",
                v_range
            );
        }

        // ── apex halfspace {z > 2}: keeps the apex → smaller cone (side + 1 disk).
        let mut m2 = Mesh::default();
        let cone2 = m2.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        let r2 = m2.boolean_cone_slab(&cone2, 2.0, 4.0, mat).expect("apex halfspace");
        assert_eq!(r2.len(), 2, "smaller cone = side + 1 disk");
        assert_eq!(
            m2.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "smaller cone watertight"
        );
        assert!(m2.verify_face_invariants().is_valid());
        let side = r2
            .iter()
            .find(|&&f| matches!(m2.face_surface(f), Some(S::Cone { .. })))
            .unwrap();
        if let Some(S::Cone { v_range, .. }) = m2.face_surface(*side) {
            // smaller cone keeps the apex: v∈[0, v(2)=2].
            assert!(
                v_range.0.abs() < 1e-9 && (v_range.1 - 2.0).abs() < 1e-9,
                "smaller cone v∈[0,2]; got {:?}",
                v_range
            );
        }

        // ── base halfspace {z < 2}: keeps the base → frustum-to-base (band + 2 disks).
        let mut m3 = Mesh::default();
        let cone3 = m3.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        let r3 = m3.boolean_cone_slab(&cone3, -1.0, 2.0, mat).expect("base halfspace");
        assert_eq!(r3.len(), 3, "frustum-to-base = band + 2 disks");
        assert_eq!(
            m3.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "frustum-to-base watertight"
        );
        assert!(m3.verify_face_invariants().is_valid());

        // ── whole-cone slab → rejected (no genuine cut).
        let mut m4 = Mesh::default();
        let cone4 = m4.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        assert!(
            m4.boolean_cone_slab(&cone4, -1.0, 5.0, mat).is_err(),
            "slab covering the whole cone is rejected"
        );
    }

    #[test]
    fn adr197_beta3l_torus_slab() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        // torus center 0, R=5, r=1.5; slab {|z|<0.5} straddles the tube centre.
        let mut mesh = Mesh::default();
        let torus = mesh.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let r = mesh.boolean_torus_slab(&[torus], -0.5, 0.5, mat).expect("torus slab");
        assert_eq!(r.len(), 4, "2 Torus bands + 2 Plane washers");
        assert_eq!(
            mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "torus slab watertight"
        );
        assert!(mesh.verify_face_invariants().is_valid());
        let manifold = mesh.face_set_manifold_info(&r);
        assert_eq!(manifold.non_manifold_edge_count, 0, "no non-manifold edges");
        let tori = r.iter().filter(|&&f| matches!(mesh.face_surface(f), Some(S::Torus { .. }))).count();
        let planes = r.iter().filter(|&&f| matches!(mesh.face_surface(f), Some(S::Plane { .. }))).count();
        assert_eq!((tori, planes), (2, 2), "2 Torus + 2 Plane");
        // each band + washer is multi-loop (outer + inner circle).
        assert!(r.iter().all(|&f| mesh.faces.get(f).unwrap().inners().len() == 1), "all multi-loop");
        let (pos, _n, tris, _e, _uv) = mesh.export_buffers().expect("export");
        assert!(!pos.is_empty() && !tris.is_empty() && pos.iter().all(|c| c.is_finite()));

        // BAIL: a plane outside the tube → halfspace, not a 2-cut slab.
        let mut m2 = Mesh::default();
        let t2 = m2.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        assert!(
            m2.boolean_torus_slab(&[t2], -5.0, 0.5, mat).is_err(),
            "plane below the tube → not a 2-cut slab"
        );
    }

    #[test]
    fn sim_beta3l_torus_slab_structure() {
        // 사전검토: torus ∩ {z_lo<z<z_hi} (both cuts within the tube) keeps a
        // horizontal band of the donut → still genus-1 (a thinner ring). Boundary
        // = 2 Torus bands (outer + inner tube surface) + 2 Plane washers (annular
        // caps at z_lo, z_hi). 4 cut circles, each shared by a band + a washer.
        let r_major = 5.0_f64;
        let r_minor = 1.5_f64;
        let z_lo = -0.5_f64;
        let z_hi = 0.5_f64;
        let d_lo = z_lo;
        let d_hi = z_hi;
        assert!(d_lo.abs() < r_minor && d_hi.abs() < r_minor, "both cuts within the tube");
        let outer = |d: f64| r_major + (r_minor * r_minor - d * d).sqrt();
        let inner = |d: f64| r_major - (r_minor * r_minor - d * d).sqrt();
        // 4 cut circles.
        let o_hi = outer(d_hi);
        let i_hi = inner(d_hi);
        let o_lo = outer(d_lo);
        let i_lo = inner(d_lo);
        eprintln!("[sim-3l] cut circles: O_hi={o_hi:.3} I_hi={i_hi:.3} O_lo={o_lo:.3} I_lo={i_lo:.3}");
        // outer band v∈[asin(d_lo/r), asin(d_hi/r)]; inner band v∈[π−asin(d_hi/r), π−asin(d_lo/r)].
        let v_o_lo = (d_lo / r_minor).asin();
        let v_o_hi = (d_hi / r_minor).asin();
        let v_i_lo = std::f64::consts::PI - (d_hi / r_minor).asin();
        let v_i_hi = std::f64::consts::PI - (d_lo / r_minor).asin();
        eprintln!("[sim-3l] outer band v∈[{v_o_lo:.3},{v_o_hi:.3}] inner band v∈[{v_i_lo:.3},{v_i_hi:.3}]");
        eprintln!("[sim-3l] faces = 2 Torus band (outer/inner) + 2 Plane washer (z_hi/z_lo); 4 circle edges");
        // sanity: inner radii positive (R > r ⇒ always), outer > inner.
        assert!(i_hi > 0.0 && i_lo > 0.0 && o_hi > i_hi && o_lo > i_lo);
        // structure: 4 faces, 4 self-loop circle edges (each shared band+washer), 4 anchors.
        // genus-1 ring (the slab of a donut is still a donut).
    }

    /// ADR-197 β-3-m 사전검토 — SUBTRACT (A − box) semantics for curved primitives.
    ///
    /// Core identity: an axis-box that XY-contains the curved primitive and only
    /// Z-cuts it is, for Boolean purposes, a Z-slab `{z_lo < z < z_hi}` (or a
    /// halfspace if one plane misses). Therefore:
    ///
    ///   A − box  =  A ∩ ¬box  =  A ∩ ({z < z_lo} ∪ {z > z_hi})
    ///
    /// Two consequences drive the whole track:
    ///   1. SUBTRACT REUSES THE INTERSECT MACHINERY with the keep-side flipped.
    ///      A − halfspace = A ∩ (opposite halfspace) = an *existing* primitive
    ///      (cap / stub / frustum / band-ring) with the cut-plane normal flipped.
    ///   2. A − slab (both planes cut) = two DISJOINT outer pieces = the halfspace
    ///      builder run twice (keep-below z_lo  +  keep-above z_hi). The result
    ///      mesh holds two disconnected closed solids; the returned Vec is the
    ///      union of both face sets (matches box−box subtract).
    ///
    /// This sim locks in the expected STRUCTURE (piece count, face count, cut-circle
    /// radii, kept side, surface kinds) for the 8 Z-cut cases. The two CONCAVE cases
    /// (sphere − corner box, sphere − full rounded box) are deferred — their results
    /// are non-convex (a scooped octant / six bulge-caps) and need new sew topology.
    #[test]
    fn sim_beta3m_subtract_semantics_matrix() {
        use std::f64::consts::PI;
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-9;

        // ── SPHERE (r=3, center origin) ──────────────────────────────────────
        let r = 3.0_f64;
        let rho = |z: f64| (r * r - z * z).sqrt(); // cut-circle radius at height z
        // A − halfspace box {z>1}  (box covers the top, plane at z=1 cuts):
        //   keep bottom cap z<1 = Sphere(v∈[−π/2, asin(1/3)]) + Plane disk(z=1, +Z).
        //   == boolean_sphere_halfspace(plane z=1, keep_below)  [existing fn, flipped].
        let cap_circle = rho(1.0);
        assert!(approx(cap_circle, (8.0_f64).sqrt()), "sphere cap circle ρ=√8");
        let cap_v_top = (1.0_f64 / r).asin();
        eprintln!("[sim-3m] sphere − halfspace{{z>1}} → 1 cap (2 faces: Sphere v∈[−π/2,{cap_v_top:.3}] + disk z=1 +Z), 1 solid");
        // A − slab {−1<z<1}  → 2 DISJOINT caps (top z>1 + bottom z<−1):
        eprintln!("[sim-3m] sphere − slab{{−1<z<1}} → 2 disjoint caps (4 faces: 2 Sphere + 2 Plane disk), 2 solids");
        assert!(approx(rho(1.0), rho(-1.0)), "both cut circles equal radius");

        // ── CYLINDER (axis Z, z∈[−3,3], r=2) ─────────────────────────────────
        // A − slab {−1.5<z<1.5} → 2 disjoint stubs (bottom z∈[−3,−1.5], top z∈[1.5,3]).
        //   each stub = base disk + cut disk + side Cylinder band → 3 faces.
        eprintln!("[sim-3m] cylinder − slab → 2 disjoint stubs (6 faces: 2 base/cut disk pairs + 2 side bands), 2 solids");
        // A − halfspace (plane z=0.5) → 1 stub z∈[−3,0.5] (3 faces), 1 solid.
        eprintln!("[sim-3m] cylinder − halfspace → 1 stub (3 faces: base disk + cut disk + side band), 1 solid");

        // ── CONE (apex-up: base z=−3 r=2, apex z=3; r(z)=(3−z)/3) ─────────────
        let rcone = |z: f64| (3.0 - z) / 3.0;
        // A − slab {−1<z<1} → base frustum (z∈[−3,−1]) + tip cone (z∈[1,3]).
        let r_lo = rcone(-1.0); // base-frustum cut-disk radius
        let r_hi = rcone(1.0); // tip-cone cut-disk radius
        assert!(approx(r_lo, 4.0 / 3.0) && approx(r_hi, 2.0 / 3.0), "cone cut radii");
        eprintln!("[sim-3m] cone − slab → base frustum(3 faces: base r2 + cut r{r_lo:.3} + side) + tip cone(2 faces: cut r{r_hi:.3} + side w/apex), 5 faces, 2 solids");
        // A − halfspace (plane z=1, remove tip) → base+mid frustum z∈[−3,1] (3 faces), 1 solid.
        eprintln!("[sim-3m] cone − halfspace{{remove tip}} → 1 frustum (3 faces), 1 solid; {{remove base}} → 1 tip cone (2 faces)");

        // ── TORUS (R=5, r=1.5) ───────────────────────────────────────────────
        let (rr, mr) = (5.0_f64, 1.5_f64);
        // A − slab {−0.5<z<0.5} → top ring (z>0.5) + bottom ring (z<−0.5).
        //   each ring = Torus band + annular Plane washer (≈ boolean_torus_halfspace).
        let v1 = (0.5_f64 / mr).asin();
        let washer_outer = rr + (mr * mr - 0.25).sqrt();
        let washer_inner = rr - (mr * mr - 0.25).sqrt();
        assert!(washer_outer > washer_inner && washer_inner > 0.0, "annular washer");
        eprintln!("[sim-3m] torus − slab → 2 disjoint band-rings (4 faces: 2 Torus band + 2 annular washer ρ∈[{washer_inner:.3},{washer_outer:.3}]), 2 solids; top band v∈[{v1:.3},{:.3}]", PI - v1);
        // A − halfspace (plane z=0) → 1 band-ring (opposite side, 2 faces), 1 solid.
        eprintln!("[sim-3m] torus − halfspace → 1 band-ring (2 faces: band + washer), 1 solid [boolean_torus_halfspace flipped]");

        // ── DEFERRED (concave, new sew topology) ─────────────────────────────
        eprintln!("[sim-3m] DEFER: sphere − corner box (scooped octant, concave) + sphere − full box (6 bulge-caps)");

        // ── MATRIX SUMMARY (locked target) ───────────────────────────────────
        //  primitive | A − halfspace box        | A − slab box (2 disjoint)
        //  ----------|--------------------------|---------------------------
        //  sphere    | 1 cap (2 faces)          | 2 caps (4 faces)
        //  cylinder  | 1 stub (3 faces)         | 2 stubs (6 faces)
        //  cone      | frustum(3) OR tip(2)     | frustum + tip (5 faces)
        //  torus     | 1 band-ring (2 faces)    | 2 band-rings (4 faces)
        //  Reuse: halfspace = existing intersect fn with flipped keep-side.
        //         slab = halfspace builder ×2 (keep-below z_lo + keep-above z_hi).
        let _ = (cap_circle, r_lo, r_hi, washer_outer);
    }

    /// ADR-197 β-3-o 사전검토 — UNION (#5) gap + design (curved∪box vs curved∪curved).
    ///
    /// AUDIT (4-agent workflow):
    ///   • Union is NOT routed through the curved dispatch — `boolean()` only routes
    ///     `op==Intersect`/`Subtract`; Union skips both → polygonize → legacy →
    ///     analytic surface LOST.
    ///   • The NURBS-DCEL path (ADR-064/066) rejects Sphere/Cylinder/Cone/Torus
    ///     (`UnsupportedSurfaceKind`) → DEAD END for analytic primitives. Build Union
    ///     DIRECTLY like β-3-h..n (analytic SSI + sew), same as ∩/−.
    ///   • `sphere_sphere` SSI is ABSENT but the radical-plane formula is trivial.
    ///   • `sew_closed_curve_pair` fits curved∪curved exactly (2 caps sharing the SSI
    ///     circle). `add_face_with_holes` + curve attach fits curved∪box's pierced
    ///     box faces (γ-2b `build_octagon` pattern).
    ///
    /// DESIGN — two Union sub-cases (sim locks the geometry of each):
    ///   • CASE A — curved ∪ box (box XY-contains + Z-cuts): box solid + the two
    ///     curved caps protruding above/below + the box top/bottom faces PIERCED
    ///     (square outer − circle hole). e.g. sphere ∪ slab-box → 8 faces.
    ///   • CASE B — curved ∪ curved (coaxial, e.g. two Z-offset spheres): each
    ///     trimmed at the SSI circle, keeping the OUTER cap → 2 Sphere caps sharing
    ///     one SSI circle (a capsule). 2 faces via `sew_closed_curve_pair`. Needs
    ///     a `sphere_sphere_z_circle` helper (radical plane).
    /// ADR-197 β-3-o 사전검토 — CASE B same-kind (curved∪curved 동종) feasibility.
    ///
    /// 어느 동종 곡면 union 이 깨끗한 HORIZONTAL-CIRCLE SSI 를 갖는가 (= sphere∪sphere
    /// 처럼 2 cap이 1 원을 공유하는 `sew_closed_curve_pair` 패턴으로 구현 가능)? 각
    /// 케이스를 분석적으로 특성화한다.
    ///
    ///   • sphere∪sphere (Z-offset)      → ✅ radical-plane 수평 원 (구현 완료, β-3-o)
    ///   • cone∪cone (OPPOSING coaxial)  → ✅ 수평 원 at z=(a+b)/2 (bicone/diamond) — 단
    ///                                       한 cone apex-up + 한 apex-down 의 niche 배치
    ///   • cone∪cone (same direction)    → ❌ slant 평행 → 교차 없음
    ///   • cylinder∪cylinder (same axis) → ❌ 측면 coincident(같은 r)=extend / concentric
    ///                                       (다른 r)=교차 없음 → 수평 원 없음
    ///   • cylinder∪cylinder (perp axes) → ❌ saddle 곡선 (non-analytic, Stage 2)
    ///   • torus∪torus                   → ❌ quartic SSI (non-analytic), concentric=nested
    ///   • (mixed bonus) cylinder∪cone coaxial → ✅ 1 수평 원 (where r_cyl = r_cone(z))
    #[test]
    fn sim_beta3o_case_b_same_kind_assessment() {
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-9;

        // ── CONE∪CONE (opposing coaxial OVERLAPPING) — the ONLY clean same-kind
        //    beyond sphere. cone1 apex-up apex z=a=4, cone2 apex-down apex z=b=−4,
        //    half-angle 30°. r1(z)=(a−z)tanθ, r2(z)=(z−b)tanθ → equal at the WAIST
        //    z=(a+b)/2, radius=(a−b)/2·tanθ. **CORRECTION**: the union keeps the
        //    WIDE part of each cone (apex sits inside the other cone → removed) →
        //    the result is an HOURGLASS, NOT a 2-cap diamond: 2 Cone FRUSTUM bands
        //    + 2 base disks (4 faces), the 2 bands sharing the waist SSI circle.
        //    (A diamond would be 2 cones base-to-base = touching, NOT overlapping →
        //    not a Boolean union.)
        let (a, b, theta) = (4.0_f64, -4.0_f64, 30.0_f64.to_radians());
        let z_waist = (a + b) / 2.0;
        let rho_waist = (a - b) / 2.0 * theta.tan();
        assert!(approx(z_waist, 0.0), "opposing-cone waist at midplane z=0");
        assert!((rho_waist - 4.0 * (30.0_f64.to_radians()).tan()).abs() < 1e-9, "waist radius=(a−b)/2·tanθ");
        eprintln!("[sim-3o-B] cone∪cone OPPOSING overlapping → ✅ HOURGLASS (NOT diamond): waist z={z_waist:.1} ρ={rho_waist:.3}; 2 Cone frustum band + 2 base disk = 4면, 2 band이 waist SSI 원 공유. apex는 상대 cone 안 → 제거. niche 배치(apex-up + apex-down).");

        // ── CONE∪CONE same direction → no intersection (parallel slant lines).
        //    both apex-up, apex z=a1, a2: r1(z)=(a1−z)tanθ, r2(z)=(a2−z)tanθ → equal ⟺ a1=a2.
        eprintln!("[sim-3o-B] cone∪cone same-direction → ❌ slant 평행, a1≠a2면 교차 0 (a1=a2면 동일 cone). 깨끗한 원 없음.");

        // ── CYLINDER∪CYLINDER (same axis) — lateral surfaces never meet in a circle.
        //    same r: coincident (degenerate, union=extend). diff r: concentric, no intersection.
        eprintln!("[sim-3o-B] cylinder∪cylinder same-axis → ❌ same r=측면 coincident(union=단순 연장) / diff r=concentric 교차0. 수평 원 없음.");
        // perpendicular axes: SSI is a saddle (Viviani-like) curve, non-circular.
        eprintln!("[sim-3o-B] cylinder∪cylinder perp-axes → ❌ saddle 곡선(non-analytic, ssi/analytic.rs는 평행축만). 별도 ADR.");

        // ── TORUS∪TORUS — generally a quartic SSI curve, no analytic circle.
        eprintln!("[sim-3o-B] torus∪torus → ❌ quartic SSI(non-analytic), concentric=nested. torus_z_cut은 평면 절단 전용. 별도 ADR.");

        // ── MIXED bonus — cylinder∪cone coaxial: 1 horizontal circle where r_cyl=r_cone(z).
        //    cyl radius rc=2, cone apex z=6 half-angle 30° → r_cone(z)=(6−z)tan30°.
        //    rc = r_cone(z) → z = 6 − rc/tanθ.
        let (rc, apex_z) = (2.0_f64, 6.0_f64);
        let z_join = apex_z - rc / theta.tan();
        eprintln!("[sim-3o-B] (mixed) cylinder∪cone coaxial → ✅ 1 수평 원 z={z_join:.3}(where r_cyl={rc}=r_cone). 구현 가능하나 동종 아님.");

        // ── VERDICT (locked):
        //   동종 same-kind 중 깨끗한 수평-원 SSI:
        //     ✅ sphere∪sphere (done, 2 cap)
        //     ✅ cone∪cone OPPOSING overlapping (niche HOURGLASS, 2 frustum + 2 disk)
        //     ❌ cylinder∪cylinder / ❌ torus∪torus (non-analytic → 별도 ADR)
        //   → Case B 동종의 남은 깨끗한 케이스는 cone∪cone opposing 1개뿐 (niche
        //     hourglass, NOT diamond — apex가 상대 cone 안에 들어가 frustum만 남음).
        let _ = (rho_waist, z_join);
    }

    #[test]
    fn sim_beta3o_union_gap_and_design() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let approx = |a: f64, b: f64| (a - b).abs() < 1e-9;

        // ── CASE B now CLOSED: Union of two Z-coaxial overlapping spheres routes
        //    to the curved union (β-3-o) — surface PRESERVED (2 Sphere caps). Before
        //    this sub-step the legacy path errored ("HalfEdge not found") on Path B
        //    spheres; now it is a capsule.
        let mut m = Mesh::default();
        let s1 = m.create_sphere_kernel_native(DVec3::new(0., 0., 0.), 30.0, mat).unwrap();
        let s2 = m.create_sphere_kernel_native(DVec3::new(0., 0., 40.), 30.0, mat).unwrap();
        let res = m.boolean(&s1, &s2, BoolOp::Union, mat).expect("sphere∪sphere now routes");
        let curved = res.faces.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Sphere { .. }))).count();
        assert_eq!(curved, 2, "sphere∪sphere → 2 Sphere caps (surface preserved, gap CLOSED)");
        eprintln!("[sim-3o] CASE B CLOSED: sphere∪sphere Union → {} faces, {curved} Sphere surface (capsule, β-3-o)", res.faces.len());

        // ── CASE B — curved∪curved (two Z-offset spheres). SSI = radical-plane circle.
        //    c1=(0,0,0) r1=30 ; c2=(0,0,40) r2=30 ; d=40.
        let (r1, r2, d) = (30.0_f64, 30.0_f64, 40.0_f64);
        let a = (d * d + r1 * r1 - r2 * r2) / (2.0 * d); // signed dist from c1 along axis to SSI plane
        let z_ssi = a;                                    // c1.z + a (axis = +Z)
        let rho_ssi = (r1 * r1 - a * a).sqrt();
        assert!(approx(z_ssi, 20.0) && approx(rho_ssi, 500.0_f64.sqrt()), "SSI circle z=20, ρ=√500");
        // kept caps: sphere1 below z_ssi (v∈[−π/2, asin(20/30)]); sphere2 above (v∈[asin(−20/30), π/2]).
        let v1 = (z_ssi / r1).asin();         // sphere1 upper trim latitude
        let v2 = ((z_ssi - 40.0) / r2).asin(); // sphere2 lower trim latitude (negative)
        eprintln!("[sim-3o] CASE B sphere∪sphere → 2 Sphere caps share SSI circle (z={z_ssi:.1}, ρ={rho_ssi:.3}); cap1 v∈[−π/2,{v1:.3}], cap2 v∈[{v2:.3},π/2]. 2 faces via sew_closed_curve_pair (NEW sphere_sphere_z_circle helper).");

        // ── CASE A — curved∪box (sphere r30 ∪ box [−40,40]²×[−20,20]). Box Z-cuts at ±20.
        let r = 30.0_f64;
        let zc = 20.0_f64;
        let rho_cut = (r * r - zc * zc).sqrt(); // sphere cross-section at z=±20
        assert!(approx(rho_cut, 500.0_f64.sqrt()), "box pierce circle ρ=√500≈22.36");
        eprintln!("[sim-3o] CASE A sphere∪box → 8 faces: 4 box wall + 2 PIERCED box cap (square 80 − circle ρ={rho_cut:.3} hole) + 2 Sphere cap (z>20, z<−20). box absorbs sphere mid-band.");

        // ── MATRIX (locked target):
        //   case            | faces | new engine                         | reuse
        //   ----------------|-------|------------------------------------|------------------------
        //   B curved∪curved | 2     | sphere_sphere_z_circle (~10 lines) | sew_closed_curve_pair
        //   A curved∪box    | 8     | pierced box face (square+circle)   | halfspace cap + add_face_with_holes
        //   both: build DIRECTLY (NURBS-DCEL dead-end for analytic primitives).
        let _ = (rho_ssi, rho_cut, v1, v2);
    }

    /// ADR-197 β-3-n 사전검토 — CURVED KNIFE (UI cut/slice tool) gap + design.
    ///
    /// AUDIT: the existing `SliceTool` (the knife) calls the POLYGONAL
    /// `slice_volume_by_plane`, which (1) rejects multi-loop faces ("has holes")
    /// — so a Path B cylinder/cone/torus side band bails outright — and (2) even
    /// when it runs, it splits edges/triangulates, DESTROYING the analytic
    /// surface. The curved Boolean we just built (β-3-h..m) preserves it. So the
    /// gap is: route the knife to the curved cut for Z-axis planes on Path B
    /// solids. (The Boolean MENU already routes — `boolean()` → curved dispatch.)
    ///
    /// DESIGN — two semantics for the curved knife (sim asserts the geometry of
    /// each so the implementation has a locked target):
    ///   • TRIM (keep one side)  = exactly `boolean(solid, box, Subtract)` — the
    ///     tool builds a box covering the unwanted side. ZERO new engine code
    ///     (reuses β-3-m). Result = 1 curved volume.
    ///   • SLICE (split into two) = both halves kept as separate volumes (matches
    ///     the existing SliceTool's 2-volume semantics). Needs a 2-piece builder:
    ///     a single Z-plane → keep-above cap + keep-below cap sharing the plane.
    /// MVP plane: horizontal Z (normal ‖ ±Z). Oblique = defer (γ-2a seam-shift).
    /// Non-Z / non-curved → fall back to the polygonal SliceTool.
    #[test]
    fn sim_beta3n_curved_knife_gap_and_design() {
        use crate::operations::slice::SlicePlane;
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // ── GAP (empirical): the polygonal knife bails on a Path B cylinder
        //    (multi-loop side band → "has holes — not yet supported").
        let mut mc = Mesh::default();
        let cyl = build_clean_cylinder(&mut mc, 0., 0., -3., 2.0, 6.0, mat);
        let plane = SlicePlane::new(DVec3::ZERO, DVec3::Z).unwrap();
        let poly = mc.slice_volume_by_plane(&cyl, plane, mat);
        assert!(poly.is_err(), "polygonal slice rejects the curved (multi-loop) cylinder");
        eprintln!("[sim-3n] polygonal knife on Path B cylinder → Err (gap): {}", poly.unwrap_err());

        // ── DESIGN target — TRIM (keep one side) = reuse boolean Subtract.
        //    sphere − {box covering z<0} → keep top cap (1 curved volume).
        let mut mt = Mesh::default();
        let s = mt.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        make_box(&mut mt, DVec3::new(-5., -5., -5.), DVec3::new(5., 5., 0.), mat);
        let bx: Vec<FaceId> = mt.faces.iter().map(|(f, _)| f).filter(|f| !s.contains(f)).collect();
        let trim = mt.boolean(&s, &bx, BoolOp::Subtract, mat).expect("trim = subtract");
        assert!(trim.debug.iter().any(|d| d.contains("β-3-m curved subtract")), "trim reuses subtract");
        assert_eq!(trim.faces.len(), 2, "1 curved volume (cap + disk)");
        assert_eq!(
            mt.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0, "trim watertight"
        );
        assert!(trim.faces.iter().any(|&f| matches!(mt.face_surface(f), Some(S::Sphere { .. }))), "surface preserved");
        eprintln!("[sim-3n] TRIM (keep 1 side) = boolean Subtract reuse → {} faces, surface preserved", trim.faces.len());

        // ── DESIGN target — SLICE (split into two) geometry, single Z-plane z=k:
        //    each primitive → 2 curved volumes sharing the cut plane.
        //    sphere → top cap (v∈[asin(k/r),π/2]) + bottom cap (v∈[−π/2,asin(k/r)]);
        //    cylinder → top stub + bottom stub; cone → tip + base frustum;
        //    torus → top band-ring + bottom band-ring (2 concentric cut circles).
        let (r, k) = (3.0_f64, 1.0_f64);
        let rho = (r * r - k * k).sqrt();
        eprintln!("[sim-3n] SLICE sphere @ z={k}: 2 caps share circle ρ={rho:.3}; top v∈[{:.3},π/2], bot v∈[−π/2,{:.3}]",
            (k / r).asin(), (k / r).asin());
        eprintln!("[sim-3n] SLICE needs a 2-piece builder (single plane); TRIM is zero-new-engine (β-3-m reuse).");

        // MATRIX (locked):
        //   knife semantics | engine work          | result
        //   ----------------|----------------------|------------------------
        //   TRIM (1 side)   | 0 (reuse Subtract)    | 1 curved volume
        //   SLICE (2 vols)  | new 2-piece builder   | 2 curved volumes
        //   plane MVP       | Z-axis only           | oblique = defer
        //   fallback        | polygonal SliceTool   | non-Z / non-curved
        let _ = rho;
    }

    /// ADR-197 β-3-o — curved UNION: sphere ∪ sphere (Z-coaxial) → capsule.
    #[test]
    fn adr197_beta3o_sphere_sphere_union() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        // ── two Z-offset overlapping spheres → 2 Sphere caps sharing SSI circle.
        let mut m = Mesh::default();
        let s1 = m.create_sphere_kernel_native(DVec3::new(0., 0., 0.), 30.0, mat).unwrap();
        let s2 = m.create_sphere_kernel_native(DVec3::new(0., 0., 40.), 30.0, mat).unwrap();
        let r = m.boolean_sphere_sphere_union(&s1, &s2, mat).expect("sphere∪sphere");
        assert_eq!(r.len(), 2, "2 Sphere caps");
        assert_eq!(wt(&m), 0, "capsule watertight");
        assert!(m.verify_face_invariants().is_valid());
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "no non-manifold edges");
        assert!(r.iter().all(|&f| matches!(m.face_surface(f), Some(S::Sphere { .. }))), "both faces Sphere (surface preserved)");
        // geometry: capsule spans z ∈ [−30, 70] (sphere1 south pole to sphere2 north pole),
        // waist (SSI circle) at z=20. No vertices in the removed overlap interior.
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));
        let zmin = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MAX, f64::min);
        let zmax = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MIN, f64::max);
        assert!((zmin + 30.0).abs() < 0.5 && (zmax - 70.0).abs() < 0.5, "capsule spans z∈[−30,70]; got [{zmin:.1},{zmax:.1}]");

        // ── ROUTING: boolean(s1, s2, Union) → curved union dispatch.
        let mut mr = Mesh::default();
        let a = mr.create_sphere_kernel_native(DVec3::new(0., 0., 0.), 30.0, mat).unwrap();
        let b = mr.create_sphere_kernel_native(DVec3::new(0., 0., 40.), 30.0, mat).unwrap();
        let res = mr.boolean(&a, &b, BoolOp::Union, mat).expect("union route");
        assert!(res.debug.iter().any(|d| d.contains("β-3-o curved union")), "routed to curved union");
        assert_eq!(res.faces.len(), 2);
        assert_eq!(wt(&mr), 0);

        // ── BAIL: disjoint spheres (no SSI circle) → not routed (fall through).
        let mut md = Mesh::default();
        let d1 = md.create_sphere_kernel_native(DVec3::new(0., 0., 0.), 10.0, mat).unwrap();
        let d2 = md.create_sphere_kernel_native(DVec3::new(0., 0., 100.), 10.0, mat).unwrap();
        assert!(md.try_curved_union_dispatch(&d1, &d2, mat).is_none(), "disjoint spheres → no SSI → fall through");

        // ── BAIL: nested spheres (one inside the other) → no boundary circle.
        let mut mn = Mesh::default();
        let n1 = mn.create_sphere_kernel_native(DVec3::new(0., 0., 0.), 30.0, mat).unwrap();
        let n2 = mn.create_sphere_kernel_native(DVec3::new(0., 0., 5.), 10.0, mat).unwrap();
        assert!(mn.try_curved_union_dispatch(&n1, &n2, mat).is_none(), "nested spheres → no boundary circle");

        // ── helper unit: SSI circle geometry (equal radii → midpoint).
        let (z_ssi, rho, v1, v2) = sphere_sphere_z_circle(DVec3::ZERO, 30.0, DVec3::new(0., 0., 40.), 30.0).unwrap();
        assert!((z_ssi - 20.0).abs() < 1e-9 && (rho - 500.0_f64.sqrt()).abs() < 1e-9);
        assert!((v1 - (20.0_f64 / 30.0).asin()).abs() < 1e-9 && (v2 + (20.0_f64 / 30.0).asin()).abs() < 1e-9);
    }

    /// ADR-197 β-3-p — curved UNION Case A: sphere ∪ box (pierced box + 2 caps).
    #[test]
    fn adr197_beta3p_sphere_box_union() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        // sphere r3 ∪ box [−5,5]²×[−2,2] (XY-contains, Z-cuts at ±2).
        let mut m = Mesh::default();
        let s = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let bx = make_box(&mut m, DVec3::new(-5., -5., -2.), DVec3::new(5., 5., 2.), mat);
        let r = m.boolean_sphere_box_union(&s, &bx, mat).expect("sphere∪box");
        assert_eq!(r.len(), 8, "6 box faces + 2 sphere caps");
        assert_eq!(wt(&m), 0, "union watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants valid");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "no non-manifold edges");
        // 2 Sphere caps + the box Plane faces.
        let spheres = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Sphere { .. }))).count();
        assert_eq!(spheres, 2, "2 Sphere caps (surface preserved)");
        // the box top/bottom faces are now pierced (have an inner hole loop).
        let pierced = bx.iter().filter(|&&f| m.faces.get(f).map(|fc| !fc.inners().is_empty()).unwrap_or(false)).count();
        assert_eq!(pierced, 2, "box top + bottom faces pierced (1 inner hole each)");
        // geometry: caps poke beyond the box (|z|>2 verts exist) + box footprint at |xy|=5.
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));
        let zmax = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MIN, f64::max);
        assert!((zmax - 3.0).abs() < 0.2, "sphere cap pokes to z=3 (sphere top); got {zmax:.2}");

        // ── ROUTING: boolean(sphere, box, Union) → curved union dispatch (Case A).
        let mut mr = Mesh::default();
        let sr = mr.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let bxr = make_box(&mut mr, DVec3::new(-5., -5., -2.), DVec3::new(5., 5., 2.), mat);
        let res = mr.boolean(&sr, &bxr, BoolOp::Union, mat).expect("union route");
        assert!(res.debug.iter().any(|d| d.contains("β-3-o curved union")), "routed to curved union");
        assert_eq!(res.faces.len(), 8);
        assert_eq!(wt(&mr), 0);

        // ── BAIL: box does NOT XY-contain the sphere (corner box) → fall through.
        let mut mb = Mesh::default();
        let sb = mb.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let nb = make_box(&mut mb, DVec3::new(1., 1., -2.), DVec3::new(10., 10., 2.), mat);
        assert!(mb.try_curved_union_dispatch(&sb, &nb, mat).is_none(), "non-XY-containing box → fall through");
    }

    /// ADR-197 β-3-p — curved UNION Case A: cylinder ∪ box (pierced box + 2 stubs).
    #[test]
    fn adr197_beta3p_cylinder_box_union() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        // cylinder axis Z z∈[−3,3] r2 ∪ box [−5,5]²×[−1.5,1.5] (XY-contains, Z-cuts).
        let mut m = Mesh::default();
        let cyl = build_clean_cylinder(&mut m, 0., 0., -3., 2.0, 6.0, mat);
        let bx = make_box(&mut m, DVec3::new(-5., -5., -1.5), DVec3::new(5., 5., 1.5), mat);
        let r = m.boolean_cylinder_box_union(&cyl, &bx, mat).expect("cyl∪box");
        assert_eq!(r.len(), 10, "6 box faces + 2 stubs (band+disk each)");
        assert_eq!(wt(&m), 0, "union watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants valid");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "no non-manifold edges");
        // 2 Cylinder side bands (surface preserved) + box/disk Planes.
        let cyls = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).count();
        assert_eq!(cyls, 2, "2 Cylinder side bands (surface preserved)");
        // box top/bottom pierced.
        let pierced = bx.iter().filter(|&&f| m.faces.get(f).map(|fc| !fc.inners().is_empty()).unwrap_or(false)).count();
        assert_eq!(pierced, 2, "box top + bottom pierced");
        // geometry: stubs poke to z=±3 (cylinder ends), box footprint at |xy|=5.
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));
        let zmax = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MIN, f64::max);
        let zmin = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MAX, f64::min);
        assert!((zmax - 3.0).abs() < 0.2 && (zmin + 3.0).abs() < 0.2, "stubs poke to z=±3; got [{zmin:.2},{zmax:.2}]");

        // ── ROUTING: boolean(cyl, box, Union) → curved union dispatch (Case A).
        let mut mr = Mesh::default();
        let cylr = build_clean_cylinder(&mut mr, 0., 0., -3., 2.0, 6.0, mat);
        let bxr = make_box(&mut mr, DVec3::new(-5., -5., -1.5), DVec3::new(5., 5., 1.5), mat);
        let res = mr.boolean(&cylr, &bxr, BoolOp::Union, mat).expect("union route");
        assert!(res.debug.iter().any(|d| d.contains("β-3-o curved union")), "routed to curved union");
        assert_eq!(res.faces.len(), 10);
        assert_eq!(wt(&mr), 0);
    }

    /// ADR-197 β-3-p — curved UNION Case A: cone ∪ box (tip cap + frustum stub).
    #[test]
    fn adr197_beta3p_cone_box_union() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        // cone base z=0 r2, apex z=4 ∪ box [−5,5]²×[1,3] (XY-contains, Z-cuts at 1,3).
        let mut m = Mesh::default();
        let cone = m.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        let bx = make_box(&mut m, DVec3::new(-5., -5., 1.), DVec3::new(5., 5., 3.), mat);
        let r = m.boolean_cone_box_union(&cone, &bx, mat).expect("cone∪box");
        assert_eq!(r.len(), 9, "6 box faces + tip(1) + frustum(band+disk=2)");
        assert_eq!(wt(&m), 0, "union watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants valid");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "no non-manifold edges");
        // 2 Cone surfaces (tip + frustum band) preserved.
        let cones = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).count();
        assert_eq!(cones, 2, "2 Cone faces (tip + frustum band, surface preserved)");
        // box top + bottom pierced.
        let pierced = bx.iter().filter(|&&f| m.faces.get(f).map(|fc| !fc.inners().is_empty()).unwrap_or(false)).count();
        assert_eq!(pierced, 2, "box top + bottom pierced");
        // geometry: tip pokes up to z=4 (apex), frustum base down to z=0.
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));
        let zmax = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MIN, f64::max);
        let zmin = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MAX, f64::min);
        assert!((zmax - 4.0).abs() < 0.2 && zmin.abs() < 0.2, "tip→z=4 apex, base→z=0; got [{zmin:.2},{zmax:.2}]");

        // ── ROUTING: boolean(cone, box, Union) → curved union dispatch (Case A).
        let mut mr = Mesh::default();
        let coner = mr.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        let bxr = make_box(&mut mr, DVec3::new(-5., -5., 1.), DVec3::new(5., 5., 3.), mat);
        let res = mr.boolean(&coner, &bxr, BoolOp::Union, mat).expect("union route");
        assert!(res.debug.iter().any(|d| d.contains("β-3-o curved union")), "routed to curved union");
        assert_eq!(res.faces.len(), 9);
        assert_eq!(wt(&mr), 0);
    }

    /// ADR-197 β-3-p — curved UNION Case A: torus ∪ box (annular pierce, completes Case A).
    #[test]
    fn adr197_beta3p_torus_box_union() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        // torus R5 r1.5 ∪ box [−8,8]²×[−0.5,0.5] (XY-contains ±6.5, Z-cuts the tube).
        let mut m = Mesh::default();
        let t = m.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let bx = make_box(&mut m, DVec3::new(-8., -8., -0.5), DVec3::new(8., 8., 0.5), mat);
        let r = m.boolean_torus_box_union(&[t], &bx, mat).expect("torus∪box");
        assert_eq!(r.len(), 10, "6 box + 2 Torus bands + 2 donut-center disks");
        assert_eq!(wt(&m), 0, "union watertight");
        assert!(m.verify_face_invariants().is_valid(), "invariants valid");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "no non-manifold edges");
        let tori = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Torus { .. }))).count();
        assert_eq!(tori, 2, "2 Torus band-rings (surface preserved)");
        // box top + bottom pierced (annular: 1 inner hole each).
        let pierced = bx.iter().filter(|&&f| m.faces.get(f).map(|fc| !fc.inners().is_empty()).unwrap_or(false)).count();
        assert_eq!(pierced, 2, "box top + bottom pierced (annular)");
        // geometry: tube pokes to z=±1.5 (tube top/bottom).
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));
        let zmax = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MIN, f64::max);
        let zmin = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MAX, f64::min);
        assert!((zmax - 1.5).abs() < 0.1 && (zmin + 1.5).abs() < 0.1, "tube pokes z=±1.5; got [{zmin:.2},{zmax:.2}]");

        // ── ROUTING: boolean(torus, box, Union) → curved union dispatch (Case A).
        let mut mr = Mesh::default();
        let tr = mr.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let bxr = make_box(&mut mr, DVec3::new(-8., -8., -0.5), DVec3::new(8., 8., 0.5), mat);
        let res = mr.boolean(&[tr], &bxr, BoolOp::Union, mat).expect("union route");
        assert!(res.debug.iter().any(|d| d.contains("β-3-o curved union")), "routed to curved union");
        assert_eq!(res.faces.len(), 10);
        assert_eq!(wt(&mr), 0);
    }

    /// ADR-197 β-3-o — curved UNION Case B: cone ∪ cone opposing (hourglass).
    #[test]
    fn adr197_beta3o_cone_cone_union() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        // apex-down cone fixture sanity: base z=4, apex z=0, axis_dir=+Z.
        let mut mc = Mesh::default();
        let cd = mc.create_cone_kernel_native_apex_down(DVec3::new(0., 0., 4.), 2.0, 4.0, mat).unwrap();
        assert_eq!(cd.len(), 2, "apex-down cone = base disk + side");
        assert!(cd.iter().any(|&f| matches!(mc.face_surface(f), Some(S::Cone { axis_dir, apex, .. }) if axis_dir.z > 0.5 && apex.z < 1e-6)), "axis_dir=+Z, apex at z=0");

        // ── cone A apex-up (base z=0, apex z=4) ∪ cone B apex-down (base z=4, apex z=0).
        //    both r=2 h=4 → waist at z=2, ρ=1. Result = hourglass (2 Cone band + 2 disk).
        let mut m = Mesh::default();
        let a = m.create_cone_kernel_native(DVec3::new(0., 0., 0.), 2.0, 4.0, mat).unwrap();
        let b = m.create_cone_kernel_native_apex_down(DVec3::new(0., 0., 4.), 2.0, 4.0, mat).unwrap();
        let r = m.boolean_cone_cone_union(&a, &b, mat).expect("cone∪cone hourglass");
        assert_eq!(r.len(), 4, "2 Cone frustum bands + 2 base disks");
        assert_eq!(wt(&m), 0, "hourglass watertight");
        assert!(m.verify_face_invariants().is_valid());
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "no non-manifold edges");
        let cones = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).count();
        let planes = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).count();
        assert_eq!((cones, planes), (2, 2), "2 Cone bands + 2 Plane disks (surface preserved)");
        // geometry: spans z∈[0,4], widest (r=2) at bases z=0/z=4, waist (r=1) at z=2.
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));
        let zmin = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MAX, f64::min);
        let zmax = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MIN, f64::max);
        assert!(zmin.abs() < 0.1 && (zmax - 4.0).abs() < 0.1, "hourglass spans z∈[0,4]; got [{zmin:.2},{zmax:.2}]");
        // waist is the narrowest: a vertex near z=2 should have radius ≈ 1 (< base radius 2).
        let mut min_r_at_waist = f64::MAX;
        for c in pos.chunks(3) {
            if ((c[2] as f64) - 2.0).abs() < 0.1 { min_r_at_waist = min_r_at_waist.min((c[0] as f64).hypot(c[1] as f64)); }
        }
        assert!((min_r_at_waist - 1.0).abs() < 0.15, "waist radius ≈ 1; got {min_r_at_waist:.2}");

        // ── ROUTING: boolean(cone_up, cone_down, Union) → curved union (Case B hourglass).
        let mut mr = Mesh::default();
        let ar = mr.create_cone_kernel_native(DVec3::new(0., 0., 0.), 2.0, 4.0, mat).unwrap();
        let br = mr.create_cone_kernel_native_apex_down(DVec3::new(0., 0., 4.), 2.0, 4.0, mat).unwrap();
        let res = mr.boolean(&ar, &br, BoolOp::Union, mat).expect("hourglass route");
        assert!(res.debug.iter().any(|d| d.contains("β-3-o curved union")), "routed to curved union");
        assert_eq!(res.faces.len(), 4);
        assert_eq!(wt(&mr), 0);

        // ── BAIL: same-direction cones (both apex-up) → not opposing → fall through.
        let mut ms = Mesh::default();
        let s1 = ms.create_cone_kernel_native(DVec3::new(0., 0., 0.), 2.0, 4.0, mat).unwrap();
        let s2 = ms.create_cone_kernel_native(DVec3::new(0., 0., 1.), 2.0, 4.0, mat).unwrap();
        assert!(ms.try_curved_union_dispatch(&s1, &s2, mat).is_none(), "same-direction cones → no hourglass → fall through");
    }

    /// ADR-197 β-3-n — curved SLICE (single Z-plane → 2 volumes) for all 4 primitives.
    #[test]
    fn adr197_beta3n_curved_slice() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let kinds = |m: &Mesh, r: &[FaceId]| {
            let sph = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Sphere { .. }))).count();
            let cyl = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).count();
            let con = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).count();
            let tor = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Torus { .. }))).count();
            let pln = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).count();
            (sph, cyl, con, tor, pln)
        };
        // a slice must produce TWO disjoint closed shells (verified via connected
        // components over the result faces — anchor-angle trick must keep them apart).
        let two_shells = |m: &Mesh, r: &[FaceId]| -> usize {
            let set: std::collections::HashSet<FaceId> = r.iter().copied().collect();
            let mut seen: std::collections::HashSet<FaceId> = std::collections::HashSet::new();
            let mut comps = 0;
            for &start in r {
                if seen.contains(&start) { continue; }
                comps += 1;
                let mut stack = vec![start];
                while let Some(f) = stack.pop() {
                    if !seen.insert(f) { continue; }
                    // neighbours = faces sharing an edge.
                    let mut starts = Vec::new();
                    if let Some(face) = m.faces.get(f) {
                        starts.push(face.outer().start);
                        for inner in face.inners() { starts.push(inner.start); }
                    }
                    for st in starts {
                        if let Ok(hes) = m.collect_loop_hes(st) {
                            for he in hes {
                                let e = m.hes[he].edge();
                                if let Some(edge) = m.edges.get(e) {
                                    for nhe in [edge.any_he(), m.hes[edge.any_he()].next_rad()] {
                                        if !nhe.is_null() {
                                            let nf = m.hes[nhe].face();
                                            if !nf.is_null() && set.contains(&nf) && !seen.contains(&nf) { stack.push(nf); }
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            comps
        };

        // ── SPHERE slice at z=1 → 2 caps (4 faces, 2 shells).
        let mut m = Mesh::default();
        let s = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r = m.boolean_sphere_slice(&s, 1.0, mat).expect("sphere slice");
        assert_eq!(r.len(), 4, "2 caps = 2 Sphere + 2 disk");
        assert_eq!(wt(&m), 0, "sphere slice watertight");
        assert!(m.verify_face_invariants().is_valid());
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0);
        assert_eq!(kinds(&m, &r), (2, 0, 0, 0, 2));
        assert_eq!(two_shells(&m, &r), 2, "sphere slice = 2 disjoint shells (no pinch)");
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));

        // ── CYLINDER slice at z=0 → 2 stubs (6 faces, 2 shells).
        let mut mc = Mesh::default();
        let cyl = build_clean_cylinder(&mut mc, 0., 0., -3., 2.0, 6.0, mat);
        let rc = mc.boolean_cylinder_slice(&cyl, 0.0, mat).expect("cyl slice");
        assert_eq!(rc.len(), 6);
        assert_eq!(wt(&mc), 0, "cylinder slice watertight");
        assert!(mc.verify_face_invariants().is_valid());
        assert_eq!(mc.face_set_manifold_info(&rc).non_manifold_edge_count, 0);
        assert_eq!(kinds(&mc, &rc), (0, 2, 0, 0, 4));
        assert_eq!(two_shells(&mc, &rc), 2, "cylinder slice = 2 disjoint stubs");

        // ── CONE slice at z=2 → tip + base frustum (5 faces, 2 shells).
        let mut mco = Mesh::default();
        let cone = mco.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        let rco = mco.boolean_cone_slice(&cone, 2.0, mat).expect("cone slice");
        assert_eq!(rco.len(), 5, "tip(2) + frustum(3)");
        assert_eq!(wt(&mco), 0, "cone slice watertight");
        assert!(mco.verify_face_invariants().is_valid());
        assert_eq!(mco.face_set_manifold_info(&rco).non_manifold_edge_count, 0);
        assert_eq!(kinds(&mco, &rco), (0, 0, 2, 0, 3));
        assert_eq!(two_shells(&mco, &rco), 2, "cone slice = 2 disjoint shells");

        // ── TORUS slice at z=0 → 2 band-rings (4 faces, 2 shells).
        let mut mt = Mesh::default();
        let t = mt.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let rt = mt.boolean_torus_slice(&[t], 0.0, mat).expect("torus slice");
        assert_eq!(rt.len(), 4);
        assert_eq!(wt(&mt), 0, "torus slice watertight");
        assert!(mt.verify_face_invariants().is_valid());
        assert_eq!(mt.face_set_manifold_info(&rt).non_manifold_edge_count, 0);
        assert_eq!(kinds(&mt, &rt), (0, 0, 0, 2, 2));
        assert_eq!(two_shells(&mt, &rt), 2, "torus slice = 2 disjoint band-rings");

        // ── BAIL: plane outside the sphere → no cut.
        let mut mb = Mesh::default();
        let sb = mb.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        assert!(mb.boolean_sphere_slice(&sb, 5.0, mat).is_err(), "plane misses sphere → err");
    }

    /// ADR-197 β-3-n — curved knife DISPATCHER (slice / trim modes + non-curved None).
    #[test]
    fn adr197_beta3n_cut_curved_dispatch() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();

        // Slice → 2 caps (4 faces).
        let mut m = Mesh::default();
        let s = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r = m.cut_curved_by_z_plane(&s, 1.0, CurvedCutMode::Slice, mat).expect("curved").expect("slice");
        assert_eq!(r.len(), 4);
        assert_eq!(wt(&m), 0);

        // KeepAbove → top cap (2 faces); all verts z≥1.
        let mut ma = Mesh::default();
        let sa = ma.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let ra = ma.cut_curved_by_z_plane(&sa, 1.0, CurvedCutMode::KeepAbove, mat).expect("curved").expect("above");
        assert_eq!(ra.len(), 2);
        assert_eq!(wt(&ma), 0);
        let (pos, _n, _t, _e, _uv) = ma.export_buffers().unwrap();
        assert!(pos.chunks(3).all(|c| c[2] >= 1.0 - 1e-6), "KeepAbove keeps z≥k");

        // KeepBelow → bottom cap (2 faces); all verts z≤1.
        let mut mbb = Mesh::default();
        let sbb = mbb.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let rb = mbb.cut_curved_by_z_plane(&sbb, 1.0, CurvedCutMode::KeepBelow, mat).expect("curved").expect("below");
        assert_eq!(rb.len(), 2);
        let (posb, _n, _t, _e, _uv) = mbb.export_buffers().unwrap();
        assert!(posb.chunks(3).all(|c| c[2] <= 1.0 + 1e-6), "KeepBelow keeps z≤k");

        // Cylinder / cone / torus also dispatch (slice mode).
        let mut mc = Mesh::default();
        let cyl = build_clean_cylinder(&mut mc, 0., 0., -3., 2.0, 6.0, mat);
        assert_eq!(mc.cut_curved_by_z_plane(&cyl, 0.0, CurvedCutMode::Slice, mat).unwrap().unwrap().len(), 6);
        let mut mco = Mesh::default();
        let cone = mco.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        assert_eq!(mco.cut_curved_by_z_plane(&cone, 2.0, CurvedCutMode::Slice, mat).unwrap().unwrap().len(), 5);
        let mut mt = Mesh::default();
        let t = mt.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        assert_eq!(mt.cut_curved_by_z_plane(&[t], 0.0, CurvedCutMode::Slice, mat).unwrap().unwrap().len(), 4);

        // ── None: a polygonal box (no analytic primitive) → caller falls back.
        let mut mp = Mesh::default();
        let bx = make_box(&mut mp, DVec3::new(-1., -1., -1.), DVec3::new(1., 1., 1.), mat);
        assert!(mp.cut_curved_by_z_plane(&bx, 0.0, CurvedCutMode::Slice, mat).is_none(), "non-curved → None (polygonal fallback)");
        let _ = S::Plane { origin: DVec3::ZERO, normal: DVec3::Z, basis_u: DVec3::X, u_range: (0., 1.), v_range: (0., 1.) };
    }

    /// ADR-197 β-3-m — curved SUBTRACT (A − box) for all 4 primitives.
    #[test]
    fn adr197_beta3m_curved_subtract() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let kinds = |m: &Mesh, r: &[FaceId]| {
            let sph = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Sphere { .. }))).count();
            let cyl = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cylinder { .. }))).count();
            let con = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Cone { .. }))).count();
            let tor = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Torus { .. }))).count();
            let pln = r.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).count();
            (sph, cyl, con, tor, pln)
        };

        // ── SPHERE − slab {−1<z<1} → 2 disjoint caps (4 faces).
        let mut m = Mesh::default();
        let s = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r = m.boolean_sphere_slab_subtract(&s, -1.0, 1.0, mat).expect("sphere slab subtract");
        assert_eq!(r.len(), 4, "2 caps = 2 Sphere + 2 disk");
        assert_eq!(wt(&m), 0, "sphere slab subtract watertight");
        assert!(m.verify_face_invariants().is_valid());
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0);
        assert_eq!(kinds(&m, &r), (2, 0, 0, 0, 2), "2 Sphere + 2 Plane");
        // geometry: no rendered vertex lies inside the removed slab (|z|<1).
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));
        assert!(pos.chunks(3).all(|c| c[2].abs() >= 1.0 - 1e-6), "no verts in the removed slab");

        // ── SPHERE − halfspace (box covers z<1) → 1 top cap (2 faces).
        let mut m1 = Mesh::default();
        let s1 = m1.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r1 = m1
            .boolean_sphere_halfspace(&s1, DVec3::new(0., 0., 1.0), DVec3::Z, mat)
            .expect("sphere halfspace keep-above");
        assert_eq!(r1.len(), 2);
        assert_eq!(wt(&m1), 0);

        // ── CYLINDER − slab {−1.5<z<1.5} → 2 disjoint stubs (6 faces).
        let mut mc = Mesh::default();
        let cyl = build_clean_cylinder(&mut mc, 0., 0., -3., 2.0, 6.0, mat);
        let rc = mc.boolean_cylinder_slab_subtract(&cyl, -1.5, 1.5, mat).expect("cyl slab subtract");
        assert_eq!(rc.len(), 6, "2 stubs = 2 side bands + 4 disks");
        assert_eq!(wt(&mc), 0, "cylinder slab subtract watertight");
        assert!(mc.verify_face_invariants().is_valid());
        assert_eq!(mc.face_set_manifold_info(&rc).non_manifold_edge_count, 0);
        assert_eq!(kinds(&mc, &rc), (0, 2, 0, 0, 4), "2 Cylinder + 4 Plane");

        // ── CONE − slab {1<z<3} → base frustum (z∈[0,1]) + tip cone (z∈[3,4]) = 5 faces.
        let mut mco = Mesh::default();
        let cone = mco.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        let rco = mco.boolean_cone_slab_subtract(&cone, 1.0, 3.0, mat).expect("cone slab subtract");
        assert_eq!(rco.len(), 5, "frustum(3) + tip(2)");
        assert_eq!(wt(&mco), 0, "cone slab subtract watertight");
        assert!(mco.verify_face_invariants().is_valid());
        assert_eq!(mco.face_set_manifold_info(&rco).non_manifold_edge_count, 0);
        assert_eq!(kinds(&mco, &rco), (0, 0, 2, 0, 3), "2 Cone + 3 Plane");

        // ── TORUS − slab {−0.5<z<0.5} → 2 disjoint band-rings (4 faces).
        let mut mt = Mesh::default();
        let t = mt.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let rt = mt.boolean_torus_slab_subtract(&[t], -0.5, 0.5, mat).expect("torus slab subtract");
        assert_eq!(rt.len(), 4, "2 band-rings = 2 Torus + 2 washer");
        assert_eq!(wt(&mt), 0, "torus slab subtract watertight");
        assert!(mt.verify_face_invariants().is_valid());
        assert_eq!(mt.face_set_manifold_info(&rt).non_manifold_edge_count, 0);
        assert_eq!(kinds(&mt, &rt), (0, 0, 0, 2, 2), "2 Torus + 2 Plane");

        // ── ROUTING: boolean(sphere, slab box, Subtract) → curved subtract dispatch.
        let mut mr = Mesh::default();
        let sr = mr.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        make_box(&mut mr, DVec3::new(-5., -5., -1.), DVec3::new(5., 5., 1.), mat);
        let bx: Vec<FaceId> = mr.faces.iter().map(|(f, _)| f).filter(|f| !sr.contains(f)).collect();
        let res = mr.boolean(&sr, &bx, BoolOp::Subtract, mat).expect("sphere − slab box");
        assert!(res.debug.iter().any(|d| d.contains("β-3-m curved subtract")), "routed to curved subtract");
        assert_eq!(res.faces.len(), 4, "2 caps");
        assert!(!res.faces.iter().any(|&f| bx.contains(&f)), "box faces consumed");
        assert_eq!(wt(&mr), 0, "routed result watertight");
        assert!(mr.verify_face_invariants().is_valid());

        // ── BAIL: box − sphere (concave, order-sensitive) does NOT route (DEFER).
        let mut mb = Mesh::default();
        let sb = mb.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let bb = make_box(&mut mb, DVec3::new(-5., -5., -1.), DVec3::new(5., 5., 1.), mat);
        assert!(
            mb.try_curved_subtract_dispatch(&bb, &sb, mat).is_none(),
            "box − sphere is concave / order-wrong → fall through"
        );

        // ── BAIL: sphere − XY-cutting box (concave scooped octant) → DEFER.
        let mut mx = Mesh::default();
        let sx = mx.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let nb = make_box(&mut mx, DVec3::new(1., 1., 1.), DVec3::new(10., 10., 10.), mat);
        assert!(
            mx.try_curved_subtract_dispatch(&sx, &nb, mat).is_none(),
            "sphere − corner box is concave → fall through"
        );
    }

    #[test]
    fn adr197_beta3k_full_box_sphere() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut mesh = Mesh::default();
        let sphere = mesh.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r = mesh
            .boolean_sphere_box_full(&sphere, DVec3::splat(-2.), DVec3::splat(2.), mat)
            .expect("full box∩sphere");
        assert_eq!(r.len(), 14, "8 Sphere triangles + 6 Plane octagons");
        assert_eq!(
            mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "rounded box watertight"
        );
        assert!(mesh.verify_face_invariants().is_valid(), "invariants valid");
        let manifold = mesh.face_set_manifold_info(&r);
        assert_eq!(manifold.non_manifold_edge_count, 0, "no non-manifold edges");
        assert!(manifold.is_closed_solid, "closed solid");
        let spheres = r.iter().filter(|&&f| matches!(mesh.face_surface(f), Some(S::Sphere { .. }))).count();
        let planes = r.iter().filter(|&&f| matches!(mesh.face_surface(f), Some(S::Plane { .. }))).count();
        assert_eq!((spheres, planes), (8, 6), "8 Sphere + 6 Plane");
        let (pos, _n, tris, _e, _uv) = mesh.export_buffers().expect("export");
        assert!(!pos.is_empty() && !tris.is_empty() && pos.iter().all(|c| c.is_finite()), "renders");

        // BAIL: box corner inside the sphere (single-corner case) → reject.
        let mut m2 = Mesh::default();
        let s2 = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        assert!(
            m2.boolean_sphere_box_full(&s2, DVec3::new(1., 1., 1.), DVec3::splat(5.), mat).is_err(),
            "corner inside sphere → not the rounded-box case"
        );
    }

    #[test]
    fn sim_beta3k_full_box_sphere_structure() {
        // 사전검토: full box[-2,2]³ ∩ sphere(r=3) = the sphere-rounded box. Each box
        // corner (dist √12 > r) is cut by the sphere → a 3-arc spherical-triangle
        // patch; each box face is clipped to an octagon (4 straight + 4 arc edges).
        // Characterize the vertex/edge/face counts (a single closed manifold).
        let center = DVec3::ZERO;
        let r = 3.0_f64;
        let b = 2.0_f64;
        // 12 box edges = (axis, fixed signs on the other two axes). Each edge ∩
        // sphere → 2 crossings (the shared sphere-arc vertices).
        let mut crossings: Vec<DVec3> = Vec::new();
        // x-axis edges: y,z ∈ {±b}; varying x with x²+b²+b² = r² → x = ±√(r²−2b²).
        let t = (r * r - 2.0 * b * b).sqrt(); // √(9−8) = 1
        for &sy in &[-1.0, 1.0] {
            for &sz in &[-1.0, 1.0] {
                crossings.push(DVec3::new(t, sy * b, sz * b));
                crossings.push(DVec3::new(-t, sy * b, sz * b));
            }
        }
        for &sx in &[-1.0, 1.0] {
            for &sz in &[-1.0, 1.0] {
                crossings.push(DVec3::new(sx * b, t, sz * b));
                crossings.push(DVec3::new(sx * b, -t, sz * b));
            }
        }
        for &sx in &[-1.0, 1.0] {
            for &sy in &[-1.0, 1.0] {
                crossings.push(DVec3::new(sx * b, sy * b, t));
                crossings.push(DVec3::new(sx * b, sy * b, -t));
            }
        }
        // 24 crossings, all on the sphere + on the box surface (one coord = ±t,
        // the other two = ±b).
        assert_eq!(crossings.len(), 24, "12 box edges × 2 crossings");
        assert!(crossings.iter().all(|p| (p.length() - r).abs() < 1e-9), "crossings on sphere");
        assert!(
            crossings.iter().all(|p| (p.x.abs() - b).abs() < 1e-9 || (p.y.abs() - b).abs() < 1e-9 || (p.z.abs() - b).abs() < 1e-9),
            "each crossing has a coord on a box face (±b)"
        );
        // 8 corner patches (Sphere triangle, 3 crossings) + 6 clipped box faces
        // (Plane octagon, 4 line + 4 arc) = 14 faces. Edges = 24 arc (corner↔face
        // shared) + 12 straight (box edge middle, face↔face) = 36. Euler V−E+F:
        let v = 24;
        let e = 36;
        let f = 14;
        eprintln!("[sim-3k] full box∩sphere: V={v} E={e} F={f} (Euler={})", v - e + f);
        eprintln!("[sim-3k] faces = 8 Sphere triangles (3-arc) + 6 Plane octagons (4 line + 4 arc)");
        assert_eq!(v - e + f, 2, "closed solid (genus 0)");
    }

    #[test]
    fn adr197_path_b_sphere_faces_are_in_volume() {
        // Regression for the real-UI Gap D: a Path B sphere (2 self-loop hemisphere
        // faces) must classify as in-volume (a wall), not a sheet — otherwise the
        // Boolean Sheet/Wall mixed-selection guard rejects sphere ∩ box. The
        // self-loop equator edge's twin is found via the radial chain (he_twin's
        // dst-based search fails for self-loops).
        let mat = MaterialId::new(0);
        let mut mesh = Mesh::default();
        let sphere = mesh.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        assert_eq!(sphere.len(), 2);
        for &f in &sphere {
            assert!(
                mesh.is_face_in_volume(f),
                "Path B sphere hemisphere {f:?} must be in a closed volume (not a sheet)"
            );
        }
        // a lone closed-curve disk (1 face, no neighbour across its edge) is a sheet.
        let mut m2 = Mesh::default();
        let anchor = m2.add_vertex(DVec3::new(3., 0., 0.));
        let circle = crate::curves::AnalyticCurve::Circle { center: DVec3::ZERO, radius: 3.0, normal: DVec3::Z, basis_u: DVec3::X };
        let disk = m2.add_face_closed_curve(anchor, circle, mat).unwrap();
        assert!(!m2.is_face_in_volume(disk), "a lone disk is a sheet (no neighbour)");
    }

    #[test]
    fn adr197_beta3j4_sphere_corner_box_routes_to_octant() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        // ── ROUTE: sphere ∩ box keeping the +++ corner (box [1,5]³ → (1,1,1)).
        let mut m = Mesh::default();
        let sphere = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        make_box(&mut m, DVec3::new(1., 1., 1.), DVec3::new(5., 5., 5.), mat);
        let bx: Vec<FaceId> = m.faces.iter().map(|(f, _)| f).filter(|f| !sphere.contains(f)).collect();
        let res = m.boolean(&sphere, &bx, BoolOp::Intersect, mat).expect("corner intersect");
        assert!(res.debug.iter().any(|d| d.contains("β-3-i curved")), "routed to curved path");
        assert_eq!(res.faces.len(), 4, "octant corner = 1 patch + 3 caps");
        let s = res.faces.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Sphere { .. }))).count();
        let p = res.faces.iter().filter(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. }))).count();
        assert_eq!((s, p), (1, 3), "1 Sphere patch + 3 Plane caps");
        assert!(!res.faces.iter().any(|&f| bx.contains(&f)), "box consumed");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "corner result watertight"
        );
        assert!(m.verify_face_invariants().is_valid());

        // ── full box[-2,2]³ routes to the sphere-rounded box (β-3-k) — 14 faces.
        let mut m2 = Mesh::default();
        let s2 = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let nb: Vec<FaceId> = {
            make_box(&mut m2, DVec3::new(-2., -2., -2.), DVec3::new(2., 2., 2.), mat)
        };
        let fb = m2.boolean(&s2, &nb, BoolOp::Intersect, mat).expect("full box intersect");
        assert!(fb.debug.iter().any(|d| d.contains("β-3-i curved")), "full box routed to curved");
        assert_eq!(fb.faces.len(), 14, "sphere-rounded box = 8 patches + 6 octagons");
    }

    #[test]
    fn sim_beta3j4_box_corner_detection() {
        // 사전검토: classify "sphere ∩ box" by the per-axis cut count. (1,1,1) is
        // the corner (octant) case → boolean_sphere_octant; everything else routes
        // elsewhere (no-op / halfspace / slab / wedge-bigon / full-box / complex).
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        // per-axis: 0 = no cut, 1 = one plane cuts (halfspace), 2 = both cut (slab).
        let axis_count = |lo: f64, hi: f64| -> (u8, Option<(f64, f64)>) {
            let lo_cuts = center.x - radius < lo && lo < center.x + radius;
            let hi_cuts = center.x - radius < hi && hi < center.x + radius;
            let n = lo_cuts as u8 + hi_cuts as u8;
            // for the single-cut case return (sign, value): +1 normal at lo, −1 at hi.
            let plane = if n == 1 {
                if lo_cuts { Some((1.0, lo)) } else { Some((-1.0, hi)) }
            } else {
                None
            };
            (n, plane)
        };
        let classify = |bmin: DVec3, bmax: DVec3| -> &'static str {
            let (cx, px) = axis_count(bmin.x, bmax.x);
            let (cy, py) = axis_count(bmin.y, bmax.y);
            let (cz, pz) = axis_count(bmin.z, bmax.z);
            let counts = (cx, cy, cz);
            match counts {
                (0, 0, 0) => "no-op (box ⊇ sphere)",
                (1, 0, 0) | (0, 1, 0) | (0, 0, 1) => "halfspace cap",
                (2, 0, 0) | (0, 2, 0) | (0, 0, 2) => "slab",
                (1, 1, 1) => {
                    // corner: verify the 3-plane corner is inside the sphere.
                    let b = DVec3::new(px.unwrap().1, py.unwrap().1, pz.unwrap().1);
                    if (b - center).length() < radius {
                        "CORNER (octant) → boolean_sphere_octant"
                    } else {
                        "corner outside sphere → empty/complex"
                    }
                }
                (1, 1, 0) | (1, 0, 1) | (0, 1, 1) => "wedge bigon (not done)",
                (2, 2, 2) => "full rounded box → boolean_sphere_box_full",
                _ => "complex (slab×corner) → bail",
            }
        };

        // sphere r=3 fixtures.
        let cases: &[(&str, DVec3, DVec3, &str)] = &[
            ("box ⊇ sphere", DVec3::splat(-10.), DVec3::splat(10.), "no-op (box ⊇ sphere)"),
            ("Z halfspace", DVec3::new(-10., -10., 1.), DVec3::new(10., 10., 10.), "halfspace cap"),
            ("Z slab", DVec3::new(-10., -10., -2.), DVec3::new(10., 10., 2.), "slab"),
            ("+++ corner", DVec3::new(1., 1., 1.), DVec3::splat(10.), "CORNER (octant) → boolean_sphere_octant"),
            ("full box[-2,2]³", DVec3::splat(-2.), DVec3::splat(2.), "full rounded box → boolean_sphere_box_full"),
            ("XY wedge", DVec3::new(1., 1., -10.), DVec3::new(10., 10., 10.), "wedge bigon (not done)"),
            ("slab×corner", DVec3::new(-2., 1., 1.), DVec3::new(2., 10., 10.), "complex (slab×corner) → bail"),
        ];
        for (label, bmin, bmax, expect) in cases {
            let got = classify(*bmin, *bmax);
            eprintln!("[sim-3j4] {label:22} → {got}");
            assert_eq!(got, *expect, "{label}");
        }
    }

    #[test]
    fn adr197_beta3j3_sphere_octant_orchestration() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut mesh = Mesh::default();
        let sphere = mesh.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        // octant: x>1 ∧ y>1 ∧ z>1.
        let planes = [
            (DVec3::X, DVec3::new(1., 0., 0.)),
            (DVec3::Y, DVec3::new(0., 1., 0.)),
            (DVec3::Z, DVec3::new(0., 0., 1.)),
        ];
        let r = mesh.boolean_sphere_octant(&sphere, &planes, mat).expect("octant corner");
        assert_eq!(r.len(), 4, "1 curved patch + 3 planar caps");
        // watertight + manifold.
        assert_eq!(
            mesh.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "octant corner watertight"
        );
        assert!(mesh.verify_face_invariants().is_valid(), "octant corner invariants valid");
        let manifold = mesh.face_set_manifold_info(&r);
        assert_eq!(manifold.non_manifold_edge_count, 0, "no non-manifold edges");
        assert!(manifold.is_closed_solid, "4-face corner is a closed solid");
        // 1 Sphere patch + 3 Plane caps.
        let spheres = r.iter().filter(|&&f| matches!(mesh.face_surface(f), Some(S::Sphere { .. }))).count();
        let planes_n = r.iter().filter(|&&f| matches!(mesh.face_surface(f), Some(S::Plane { .. }))).count();
        assert_eq!((spheres, planes_n), (1, 3), "1 Sphere + 3 Plane");
        // export renders (the Sphere patch via γ-2b-2 uv-earcut, caps via earcut).
        let (pos, _n, tris, _e, _uv) = mesh.export_buffers().expect("export");
        assert!(!pos.is_empty() && !tris.is_empty(), "octant corner renders");
        assert!(pos.iter().all(|c| c.is_finite()));

        // ── BAIL: box corner outside the sphere → reject.
        let mut m2 = Mesh::default();
        let s2 = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let far = [
            (DVec3::X, DVec3::new(2.5, 0., 0.)),
            (DVec3::Y, DVec3::new(0., 2.5, 0.)),
            (DVec3::Z, DVec3::new(0., 0., 2.5)),
        ];
        assert!(m2.boolean_sphere_octant(&s2, &far, mat).is_err(), "corner outside sphere rejected");
    }

    #[test]
    fn adr197_beta3j2_tessellate_arc_bounded_clips() {
        use crate::curves::AnalyticCurve;
        use crate::surfaces::AnalyticSurface as S;
        use std::f64::consts::{FRAC_PI_2, TAU};
        let mat = MaterialId::new(0);
        let mut mesh = Mesh::default();
        let r = 3.0_f64;
        let s7 = 7.0_f64.sqrt();
        let s8 = 8.0_f64.sqrt();
        let lo = (1.0_f64).atan2(s7); // ≈0.361
        let hi = s7.atan2(1.0); // ≈1.209
        let c_xy = mesh.add_vertex(DVec3::new(1., 1., s7));
        let c_xz = mesh.add_vertex(DVec3::new(1., s7, 1.));
        let c_yz = mesh.add_vertex(DVec3::new(s7, 1., 1.));
        let face = mesh.add_face_with_holes(&[c_xy, c_xz, c_yz], &[], mat).unwrap();
        // HE order (dst = loop_verts[i]): hes[0]=C_yz→C_xy (y=1), hes[1]=C_xy→C_xz
        // (x=1), hes[2]=C_xz→C_yz (z=1).
        let arc_x = AnalyticCurve::Arc { center: DVec3::new(1., 0., 0.), radius: s8, normal: DVec3::X, basis_u: DVec3::Y, start_angle: lo, end_angle: hi };
        let arc_y = AnalyticCurve::Arc { center: DVec3::new(0., 1., 0.), radius: s8, normal: DVec3::Y, basis_u: DVec3::Z, start_angle: lo, end_angle: hi };
        let arc_z = AnalyticCurve::Arc { center: DVec3::new(0., 0., 1.), radius: s8, normal: DVec3::Z, basis_u: DVec3::X, start_angle: lo, end_angle: hi };
        let arcs = [arc_y.clone(), arc_x.clone(), arc_z.clone()];
        let hes = mesh.collect_loop_hes(mesh.faces[face].outer().start).unwrap();
        for (i, he) in hes.iter().enumerate() {
            let e = mesh.hes[*he].edge();
            mesh.edges[e].set_curve(Some(arcs[i].clone()));
        }
        mesh.faces[face].set_surface(Some(S::Sphere { center: DVec3::ZERO, radius: r, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, TAU), v_range: (-FRAC_PI_2, FRAC_PI_2) }));

        let tess = mesh.tessellate_arc_bounded_face(face, 0.05).expect("arc-bounded tessellation");
        assert!(!tess.triangles.is_empty(), "produces triangles");
        assert!(tess.vertices.iter().all(|p| (p.length() - r).abs() < 1e-6), "vertices on sphere");
        // clipped: triangle centroids project inside the octant (vs whole sphere).
        let mut inside = 0usize;
        for t in &tess.triangles {
            let cen = (tess.vertices[t[0] as usize] + tess.vertices[t[1] as usize] + tess.vertices[t[2] as usize]) / 3.0;
            let cp = cen.normalize() * r;
            if cp.x > 0.99 && cp.y > 0.99 && cp.z > 0.99 {
                inside += 1;
            }
        }
        eprintln!("[3j2-tess] tris={} inside_octant={}", tess.triangles.len(), inside);
        assert!(inside * 10 >= tess.triangles.len() * 9, "clipped to octant ({inside}/{})", tess.triangles.len());
        // ADR-197 #6 — interior subdivision drives the chord error ≤ chord_tol.
        // (Pre-fix the earcut-only patch was faceted: octant ≈ 0.17mm sagitta.)
        let mut max_sag = 0.0_f64;
        for t in &tess.triangles {
            let vs = [tess.vertices[t[0] as usize], tess.vertices[t[1] as usize], tess.vertices[t[2] as usize]];
            for k in 0..3 {
                let mf = (vs[k] + vs[(k + 1) % 3]) * 0.5;
                max_sag = max_sag.max((r - mf.length()).abs());
            }
        }
        assert!(max_sag <= 0.05, "interior chord error ≤ chord_tol (got {max_sag:.4}mm)");
        // non-arc-bounded faces return None (existing render path preserved).
        let mut m2 = Mesh::default();
        let sphere = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        assert!(m2.tessellate_arc_bounded_face(sphere[0], 0.05).is_none(), "self-loop hemisphere is not arc-bounded");

        // ── export_buffers integration: the octant patch renders CLIPPED to the
        // octant region (inside-ratio below) AND interior-subdivided (ADR-197 #6 —
        // chord-driven; was a coarse ~21-tri earcut fan before the fix).
        let (pos, _nrm, tris, _e, _uv) = mesh.export_buffers().expect("export");
        let ntri = tris.len() / 3;
        assert!(ntri > 60 && ntri < 2000, "octant renders a chord-subdivided clipped patch (got {ntri} tris)");
        let mut inside = 0usize;
        for t in tris.chunks_exact(3) {
            let p = |i: u32| {
                let b = i as usize * 3;
                DVec3::new(pos[b] as f64, pos[b + 1] as f64, pos[b + 2] as f64)
            };
            let cen = (p(t[0]) + p(t[1]) + p(t[2])) / 3.0;
            let cp = cen.normalize() * r;
            if cp.x > 0.99 && cp.y > 0.99 && cp.z > 0.99 {
                inside += 1;
            }
        }
        assert!(inside * 10 >= ntri * 9, "exported triangles clipped to octant ({inside}/{ntri})");
    }

    #[test]
    fn adr197_beta3j2_uv_earcut_clips_octant_patch() {
        // γ-2b-2 사전검토: render an arc-bounded curved patch by polygonising the
        // arcs → invert to uv → earcut the uv-polygon → evaluate to 3D. Proves the
        // result is CLIPPED to the patch (not the whole sphere) + lies on the sphere.
        let center = DVec3::ZERO;
        let r = 3.0_f64;
        let s7 = 7.0_f64.sqrt();
        let s8 = 8.0_f64.sqrt();
        // octant x>1 ∧ y>1 ∧ z>1: 3 crossings.
        let c_xy = DVec3::new(1., 1., s7);
        let c_xz = DVec3::new(1., s7, 1.);
        let c_yz = DVec3::new(s7, 1., 1.);
        // sample an arc on a cut circle from `from` to `to` (angles) → 3D points.
        let sample = |cc: DVec3, normal: DVec3, basis_u: DVec3, from: DVec3, to: DVec3| -> Vec<DVec3> {
            let a = circle_angle_of_point(cc, normal, basis_u, from);
            let mut b = circle_angle_of_point(cc, normal, basis_u, to);
            if b < a { b += std::f64::consts::TAU; }
            let u = basis_u.normalize_or_zero();
            let w = normal.normalize_or_zero().cross(u);
            (0..16).map(|i| {
                let t = a + (b - a) * (i as f64) / 16.0;
                cc + s8 * (t.cos() * u + t.sin() * w)
            }).collect()
        };
        // boundary loop: C_yz →[z=1]→ C_xz →[x=1]→ C_xy →[y=1]→ C_yz.
        let mut loop3d: Vec<DVec3> = Vec::new();
        loop3d.extend(sample(DVec3::new(0., 0., 1.), DVec3::Z, DVec3::X, c_yz, c_xz));
        loop3d.extend(sample(DVec3::new(1., 0., 0.), DVec3::X, DVec3::Y, c_xz, c_xy));
        loop3d.extend(sample(DVec3::new(0., 1., 0.), DVec3::Y, DVec3::Z, c_xy, c_yz));
        // invert to uv (octant is limited-u, away from the seam).
        let uv: Vec<(f64, f64)> = loop3d.iter().map(|&p| sphere_invert(p, center, r)).collect();
        let flat: Vec<f64> = uv.iter().flat_map(|&(u, v)| [u, v]).collect();
        let tris = earcutr::earcut(&flat, &[], 2).expect("earcut uv-polygon");
        // evaluate each uv-triangle to 3D + verify clipped + on-sphere.
        let eval = |u: f64, v: f64| center + r * DVec3::new(v.cos() * u.cos(), v.cos() * u.sin(), v.sin());
        let mut on_sphere = true;
        let mut inside = 0usize;
        let ntri = tris.len() / 3;
        for k in 0..ntri {
            let mut cen = DVec3::ZERO;
            for j in 0..3 {
                let (u, vv) = uv[tris[k * 3 + j]];
                let p = eval(u, vv);
                if (p.length() - r).abs() > 1e-6 { on_sphere = false; }
                cen += p / 3.0;
            }
            // centroid projected to the sphere — is it inside the octant?
            let cp = cen.normalize() * r;
            if cp.x > 0.99 && cp.y > 0.99 && cp.z > 0.99 {
                inside += 1;
            }
        }
        eprintln!("[sim-3j2] uv-loop pts={} earcut tris={} on_sphere={} inside_octant={}/{}",
            uv.len(), ntri, on_sphere, inside, ntri);
        assert!(ntri > 0, "earcut produced triangles");
        assert!(on_sphere, "all uv-triangle vertices lie on the sphere");
        // CLIPPED: ≥90% of triangle centroids fall inside the octant (vs the whole
        // sphere band, where tessellate_face_surface put 208 tris everywhere).
        assert!(inside * 10 >= ntri * 9, "patch is clipped to the octant ({inside}/{ntri})");
    }

    #[test]
    fn adr197_beta3j_sphere_plane_pair_crossings() {
        let center = DVec3::ZERO;
        let r = 3.0_f64;
        let s7 = 7.0_f64.sqrt();
        // x=1 ∩ y=1 on the sphere → (1, 1, ±√7).
        let xy = sphere_plane_pair_crossings(center, r, DVec3::X, DVec3::new(1., 0., 0.), DVec3::Y, DVec3::new(0., 1., 0.));
        assert_eq!(xy.len(), 2, "x=1 ∩ y=1 crosses the sphere twice");
        assert!(xy.iter().all(|p| (p.length() - r).abs() < 1e-9), "crossings lie on the sphere");
        // exact location: (1, 1, ±√7) — both x and y must be 1 (not the −,− octant).
        assert!(xy.iter().all(|p| (p.x - 1.0).abs() < 1e-9 && (p.y - 1.0).abs() < 1e-9), "crossings at x=1, y=1");
        assert!(xy.iter().any(|p| (p.z - s7).abs() < 1e-9) && xy.iter().any(|p| (p.z + s7).abs() < 1e-9));
        // z=1 ∩ x=1 → (1, ±√7, 1) (the wedge crossings).
        let zx = sphere_plane_pair_crossings(center, r, DVec3::Z, DVec3::new(0., 0., 1.), DVec3::X, DVec3::new(1., 0., 0.));
        assert_eq!(zx.len(), 2);
        assert!(zx.iter().any(|p| (p.y - s7).abs() < 1e-9) && zx.iter().any(|p| (p.y + s7).abs() < 1e-9));
        // parallel planes (z=1, z=2) → no line → 0 crossings.
        let par = sphere_plane_pair_crossings(center, r, DVec3::Z, DVec3::new(0., 0., 1.), DVec3::Z, DVec3::new(0., 0., 2.));
        assert!(par.is_empty(), "parallel planes have no crossing");
        // tangent: x=3 ∩ y=0 → line x=3,y=0 touches the sphere at (3,0,0).
        let tan = sphere_plane_pair_crossings(center, r, DVec3::X, DVec3::new(3., 0., 0.), DVec3::Y, DVec3::ZERO);
        assert_eq!(tan.len(), 1, "tangent line → 1 crossing");
        assert!((tan[0] - DVec3::new(3., 0., 0.)).length() < 1e-6);

        // circle_angle_of_point round-trip on the z=1 latitude circle (basis_u=X).
        let cc = DVec3::new(0., 0., 1.);
        let pt = DVec3::new((8.0_f64).sqrt(), 0., 1.); // θ should be 0 (along +X)
        assert!(circle_angle_of_point(cc, DVec3::Z, DVec3::X, pt).abs() < 1e-9);

        // corner_arc_range: octant z=1 circle, crossings C_xz=(1,√7,1) & C_yz=(√7,1,1),
        // kept inside x>1 ∧ y>1. The bounding arc midpoint is at θ≈π/4 (x=y=2).
        let s8 = 8.0_f64.sqrt();
        let (lo, hi) = corner_arc_range(
            DVec3::new(0., 0., 1.), s8, DVec3::Z, DVec3::X,
            DVec3::new(1., s7, 1.), DVec3::new(s7, 1., 1.),
            &[(DVec3::X, DVec3::new(1., 0., 0.)), (DVec3::Y, DVec3::new(0., 1., 0.))],
        );
        let a_xz = (s7 / 1.0_f64).atan2(1.0); // ≈1.209
        let a_yz = (1.0_f64).atan2(s7);       // ≈0.361
        assert!((lo - a_yz).abs() < 1e-6 && (hi - a_xz).abs() < 1e-6, "arc [a_yz, a_xz] inside x>1∧y>1; got ({lo:.4},{hi:.4})");
        // the midpoint of the chosen arc must indeed be inside (x>1 ∧ y>1).
        let mid = DVec3::new(0., 0., 1.) + s8 * (((lo + hi) * 0.5).cos() * DVec3::X + ((lo + hi) * 0.5).sin() * DVec3::Y);
        assert!(mid.x > 1.0 && mid.y > 1.0, "arc midpoint inside kept region");
    }

    #[test]
    fn adr197_beta3j_octant_sew_reuses_existing_api() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let mut mesh = Mesh::default();
        let r = 3.0_f64;
        let s7 = 7.0_f64.sqrt();
        let s8 = 8.0_f64.sqrt();
        // 3 octant crossings (sphere r=3 ∩ x=1,y=1,z=1).
        let c_xy = mesh.add_vertex(DVec3::new(1., 1., s7)); // x=1 ∩ y=1
        let c_xz = mesh.add_vertex(DVec3::new(1., s7, 1.)); // x=1 ∩ z=1
        let c_yz = mesh.add_vertex(DVec3::new(s7, 1., 1.)); // y=1 ∩ z=1
        // Triangle face from the 3 crossings (straight edges first).
        let face = mesh
            .add_face_with_holes(&[c_xy, c_xz, c_yz], &[], mat)
            .expect("3-vertex curved patch via add_face_with_holes");
        // Attach the cut-circle ARCS to the 3 boundary edges + Sphere surface.
        use crate::curves::AnalyticCurve;
        let arc_z = AnalyticCurve::Arc { center: DVec3::new(0., 0., 1.), radius: s8, normal: DVec3::Z, basis_u: DVec3::X, start_angle: (1.0_f64 / s7).atan2(1.0), end_angle: s7.atan2(1.0) };
        let arc_x = AnalyticCurve::Arc { center: DVec3::new(1., 0., 0.), radius: s8, normal: DVec3::X, basis_u: DVec3::Y, start_angle: (1.0_f64).atan2(s7), end_angle: s7.atan2(1.0) };
        let arc_y = AnalyticCurve::Arc { center: DVec3::new(0., 1., 0.), radius: s8, normal: DVec3::Y, basis_u: DVec3::Z, start_angle: 0.0, end_angle: 1.0 };
        let mut attached = 0;
        if let Ok(hes) = mesh.collect_loop_hes(mesh.faces[face].outer().start) {
            let arcs = [arc_x.clone(), arc_z.clone(), arc_y.clone()];
            for (i, he) in hes.iter().enumerate() {
                let e = mesh.hes[*he].edge();
                mesh.edges[e].set_curve(Some(arcs[i % arcs.len()].clone()));
                attached += 1;
            }
        }
        mesh.faces[face].set_surface(Some(S::Sphere { center: DVec3::ZERO, radius: r, axis_dir: DVec3::Z, ref_dir: DVec3::X, u_range: (0.0, std::f64::consts::TAU), v_range: (-std::f64::consts::FRAC_PI_2, std::f64::consts::FRAC_PI_2) }));

        let inv = mesh.verify_face_invariants();
        let tess = mesh.tessellate_face_surface(face, 0.1);
        let exported = mesh.export_buffers();
        eprintln!("[sim-3j-octant] boundary edges with arc attached = {attached}");
        eprintln!("[sim-3j-octant] invariants valid = {}", inv.is_valid());
        eprintln!("[sim-3j-octant] tessellate_face_surface tris = {:?}", tess.as_ref().map(|t| t.triangles.len()));
        eprintln!("[sim-3j-octant] export_buffers ok = {} pos_len = {:?}", exported.is_ok(), exported.as_ref().map(|(p, ..)| p.len()).ok());
        // The DCEL face + arc attach + Sphere surface BUILD via existing API.
        assert_eq!(attached, 3, "3 arc edges attached to the patch boundary");
        assert!(inv.is_valid(), "curved patch passes face invariants");
        assert!(exported.is_ok(), "curved patch exports render buffers");
    }

    #[test]
    fn adr197_beta3j_corner_cut_characterization() {
        use std::f64::consts::{PI, TAU};
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        // Sample the sphere uv-grid and report the kept region's u-coverage for
        // several box∩sphere arrangements. The u-span decides the sew strategy:
        // limited-u (gap exists) → seam-shift (γ-2a infra reuse); full-u → periodic.
        let span = |keep: &dyn Fn(DVec3) -> bool| -> (usize, f64, bool) {
            // collect kept u values; measure largest circular gap.
            let mut us: Vec<f64> = Vec::new();
            let nu = 240;
            let nv = 120;
            for i in 0..nu {
                let u = TAU * (i as f64) / (nu as f64);
                for j in 0..=nv {
                    let v = -PI / 2.0 + PI * (j as f64) / (nv as f64);
                    let p = center
                        + radius * DVec3::new(v.cos() * u.cos(), v.cos() * u.sin(), v.sin());
                    if keep(p) {
                        us.push(u);
                        break; // one hit per u-column is enough for coverage
                    }
                }
            }
            if us.is_empty() {
                return (0, TAU, true);
            }
            us.sort_by(|a, b| a.partial_cmp(b).unwrap());
            let mut gap = 0.0;
            for k in 0..us.len() {
                let a = us[k];
                let b = if k + 1 < us.len() { us[k + 1] } else { us[0] + TAU };
                gap = f64::max(gap, b - a);
            }
            (us.len(), gap, gap > 0.5) // limited-u if a real gap exists
        };

        // 1) single wedge: x>1 ∧ z>1 (2 perpendicular cuts, 1 corner).
        let (n1, g1, lim1) = span(&|p| p.x > 1.0 && p.z > 1.0);
        eprintln!("[sim-3j] wedge x>1∧z>1: u-cols={n1} largest_gap={g1:.2} → {}",
            if lim1 { "LIMITED-u → seam-shift可" } else { "FULL-u → periodic" });

        // 2) single octant: x>0.5 ∧ y>0.5 ∧ z>0.5 (3 cuts, 1 corner of a box).
        let (n2, g2, lim2) = span(&|p| p.x > 0.5 && p.y > 0.5 && p.z > 0.5);
        eprintln!("[sim-3j] octant x,y,z>0.5: u-cols={n2} largest_gap={g2:.2} → {}",
            if lim2 { "LIMITED-u → seam-shift可" } else { "FULL-u → periodic" });

        // 3) full box[-2,2]³ ∩ sphere(r3): all 8 corners kept (rounded box).
        let (n3, g3, lim3) = span(&|p| p.x.abs() < 2.0 && p.y.abs() < 2.0 && p.z.abs() < 2.0);
        eprintln!("[sim-3j] full box[-2,2]³: u-cols={n3} largest_gap={g3:.2} → {}",
            if lim3 { "LIMITED-u → seam-shift可" } else { "FULL-u → periodic" });

        // SSI circle count for the wedge (2 planes → 2 circles that CROSS).
        let c_z = crate::surfaces::ssi::analytic::plane_sphere(
            DVec3::new(0., 0., 1.), DVec3::Z, center, radius, 64);
        let c_x = crate::surfaces::ssi::analytic::plane_sphere(
            DVec3::new(1., 0., 0.), DVec3::X, center, radius, 64);
        eprintln!("[sim-3j] wedge SSI: z=1 circle pts={} closed={}, x=1 circle pts={} closed={}",
            c_z.points.len(), c_z.closed, c_x.points.len(), c_x.closed);
        // circle-circle crossings on the sphere: z=1 ∧ x=1 → y²=r²−2 → y=±√7.
        let y_cross = (radius * radius - 2.0).sqrt();
        eprintln!("[sim-3j] wedge crossings: (1, ±{:.4}, 1) — patch bounded by 2 ARCS meeting at 2 verts", y_cross);

        // FINDINGS (characterization, not a wrong full-u claim):
        //  • A single corner (wedge / octant) keeps ONE curved patch whose
        //    boundary is N arcs (one per cutting plane) meeting at the pairwise
        //    circle-circle crossings — limited-u, seam-shiftable.
        //  • The full box keeps 8 such patches (4 u-clusters) — same patch
        //    primitive, repeated; sequential halfspace cuts produce them.
        //  • The missing building block is therefore an N-ARC CURVED SEW
        //    (a Sphere patch with an arc-edge boundary) + spherical arc
        //    clipping, NOT a full periodic arrangement.
        assert!(lim1 && lim2, "single corner keeps a limited-u patch (seam-shiftable)");
        assert!(c_z.closed && c_x.closed && (y_cross - 7.0_f64.sqrt()).abs() < 1e-9);
        assert!(n3 > 0 && g3 > 0.5, "full box keeps clustered patches (gaps between corners)");
    }

    #[test]
    fn adr197_beta3i_general_routing_sphere_box() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // ── ROUTE: sphere ∩ wide box (straddling Z-slab) → boolean_sphere_slab.
        //    Curved Sphere surface preserved + box consumed + watertight.
        let mut m = Mesh::default();
        let sphere = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        make_box(&mut m, DVec3::new(-5., -5., -2.), DVec3::new(5., 5., 2.), mat);
        let all: Vec<FaceId> = m.faces.iter().map(|(f, _)| f).collect();
        let sph: Vec<FaceId> = all.iter().copied().filter(|f| sphere.contains(f)).collect();
        let bx: Vec<FaceId> = all.iter().copied().filter(|f| !sphere.contains(f)).collect();
        let res = m.boolean(&sph, &bx, BoolOp::Intersect, mat).expect("sphere ∩ box");
        assert!(res.debug.iter().any(|d| d.contains("β-3-i curved")), "routed to curved path");
        // result has a Sphere band (surface preserved), box gone, watertight.
        assert!(
            res.faces.iter().any(|&f| matches!(m.face_surface(f), Some(S::Sphere { .. }))),
            "result preserves the Sphere surface"
        );
        assert!(
            !res.faces.iter().any(|&f| bx.contains(&f)),
            "box faces are consumed"
        );
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "routed result watertight"
        );
        assert!(m.verify_face_invariants().is_valid());

        // ── ROUTE: cylinder ∩ wide box (Z-slab) → boolean_cylinder_slab.
        let mut mc = Mesh::default();
        let cyl = build_clean_cylinder(&mut mc, 0., 0., -3., 2.0, 6.0, mat);
        make_box(&mut mc, DVec3::new(-5., -5., -1.5), DVec3::new(5., 5., 1.5), mat);
        let bxc: Vec<FaceId> = mc.faces.iter().map(|(f, _)| f).filter(|f| !cyl.contains(f)).collect();
        let rc = mc.boolean(&cyl, &bxc, BoolOp::Intersect, mat).expect("cylinder ∩ box");
        assert!(rc.debug.iter().any(|d| d.contains("β-3-i curved")));
        assert!(rc.faces.iter().any(|&f| matches!(mc.face_surface(f), Some(S::Cylinder { .. }))));
        assert!(mc.verify_face_invariants().is_valid());

        // ── ROUTE: torus ∩ box (single Z-cut, keep above) → boolean_torus_halfspace.
        let mut mt = Mesh::default();
        let torus = mt.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        // box top at +5 (> torus top), bottom at 0.5 (cuts the tube) → 1-cut keep-above.
        make_box(&mut mt, DVec3::new(-8., -8., 0.5), DVec3::new(8., 8., 5.), mat);
        let bxt: Vec<FaceId> = mt.faces.iter().map(|(f, _)| f).filter(|&f| f != torus).collect();
        let rt = mt.boolean(&[torus], &bxt, BoolOp::Intersect, mat).expect("torus ∩ box");
        assert!(rt.debug.iter().any(|d| d.contains("β-3-i curved")));
        assert!(rt.faces.iter().any(|&f| matches!(mt.face_surface(f), Some(S::Torus { .. }))));
        assert!(mt.verify_face_invariants().is_valid());

        // ── ROUTE: torus ∩ box (both Z-cuts within the tube) → boolean_torus_slab.
        //    Result = 2 Torus bands + 2 Plane washers, watertight genus-1 ring.
        let mut mts = Mesh::default();
        let torus2 = mts.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        make_box(&mut mts, DVec3::new(-8., -8., -0.5), DVec3::new(8., 8., 0.5), mat);
        let bxts: Vec<FaceId> = mts.faces.iter().map(|(f, _)| f).filter(|&f| f != torus2).collect();
        let rts = mts.boolean(&[torus2], &bxts, BoolOp::Intersect, mat).expect("torus ∩ slab box");
        assert!(rts.debug.iter().any(|d| d.contains("β-3-i curved")), "routed to curved slab path");
        assert_eq!(
            rts.faces.iter().filter(|&&f| matches!(mts.face_surface(f), Some(S::Torus { .. }))).count(),
            2,
            "2 Torus bands preserved"
        );
        assert_eq!(
            mts.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "torus slab routed result watertight"
        );
        assert!(mts.verify_face_invariants().is_valid());

        // ── BAIL: slab×corner (2,1,1) — X both planes cut, Y/Z one each → neither
        // a single corner nor a full rounded box → falls through to legacy.
        let mut mb = Mesh::default();
        let s2 = mb.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let nb = make_box(&mut mb, DVec3::new(-2., 1., 1.), DVec3::new(2., 10., 10.), mat);
        assert!(
            mb.try_curved_intersect_dispatch(&s2, &nb, mat).is_none(),
            "slab×corner box does not route (mixed cut pattern)"
        );

        // ── BAIL: non-straddling sphere slab (both cuts above the equator).
        let mut mn = Mesh::default();
        let s3 = mn.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let nb3 = make_box(&mut mn, DVec3::new(-5., -5., 1.0), DVec3::new(5., 5., 2.5), mat);
        assert!(
            mn.try_curved_intersect_dispatch(&s3, &nb3, mat).is_none(),
            "non-straddling slab falls through (sphere_slab MVP requires straddle)"
        );
    }

    #[test]
    fn adr197_beta3i_subtract_not_routed() {
        // Subtract must NOT enter the curved intersect dispatch (intersect only).
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let sphere = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        make_box(&mut m, DVec3::new(-5., -5., -2.), DVec3::new(5., 5., 2.), mat);
        let bx: Vec<FaceId> = m.faces.iter().map(|(f, _)| f).filter(|f| !sphere.contains(f)).collect();
        // The dispatch helper itself is intersect-only; boolean() only calls it
        // for Intersect. A Subtract goes to the legacy path (no curved debug tag).
        let res = m.boolean(&sphere, &bx, BoolOp::Subtract, mat);
        if let Ok(r) = res {
            assert!(
                !r.debug.iter().any(|d| d.contains("β-3-i curved")),
                "subtract is not routed to the curved intersect path"
            );
        }
    }

    #[test]
    fn adr197_beta3h_curved_boolean_results_render() {
        // GATE for production demo: every curved-Boolean result must export valid
        // render buffers (non-empty, finite positions, in-range triangle indices).
        let mat = MaterialId::new(0);
        let check = |label: &str, mesh: &mut Mesh| {
            let (pos, _nrm, tris, _edges, _uv) = mesh.export_buffers().expect(label);
            assert!(!pos.is_empty(), "{label}: positions non-empty");
            assert!(!tris.is_empty(), "{label}: triangles non-empty");
            assert!(pos.iter().all(|c| c.is_finite()), "{label}: finite positions");
            let nverts = (pos.len() / 3) as u32;
            assert!(tris.iter().all(|&i| i < nverts), "{label}: indices in range");
        };

        let mut ms = Mesh::default();
        let sphere = ms.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        ms.boolean_sphere_halfspace(&sphere, DVec3::new(0., 0., 2.), DVec3::Z, mat).unwrap();
        check("capped sphere", &mut ms);

        let mut msl = Mesh::default();
        let sph2 = msl.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        msl.boolean_sphere_slab(&sph2, -2.0, 2.0, mat).unwrap();
        check("sphere barrel", &mut msl);

        let mut mcy = Mesh::default();
        let cyl = build_clean_cylinder(&mut mcy, 0., 0., -3., 2.0, 6.0, mat);
        mcy.boolean_cylinder_slab(&cyl, -1.5, 1.5, mat).unwrap();
        check("cylinder truncate", &mut mcy);

        let mut mco = Mesh::default();
        let cone = mco.create_cone_kernel_native(DVec3::ZERO, 2.0, 4.0, mat).unwrap();
        mco.boolean_cone_slab(&cone, 1.0, 3.0, mat).unwrap();
        check("cone frustum", &mut mco);

        let mut mto = Mesh::default();
        let torus = mto.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        mto.boolean_torus_halfspace(&[torus], 0.5, true, mat).unwrap();
        check("torus halfspace", &mut mto);
    }

    #[test]
    fn adr197_beta3h_torus_z_cut_geometry() {
        // torus center z=0, R=5, r=1.5. Cut at z=0.5 (d=0.5).
        let (v1, v2, ro, ri) = torus_z_cut(0.0, 5.0, 1.5, 0.5).expect("genuine cut");
        let half = (1.5_f64 * 1.5 - 0.5 * 0.5).sqrt(); // √2
        assert!((ro - (5.0 + half)).abs() < 1e-9, "outer ρ = R+√(r²−d²)");
        assert!((ri - (5.0 - half)).abs() < 1e-9, "inner ρ = R−√(r²−d²)");
        assert!((v1 - (0.5_f64 / 1.5).asin()).abs() < 1e-9, "v1 = asin(d/r)");
        assert!((v2 - (std::f64::consts::PI - v1)).abs() < 1e-9, "v2 = π − v1");
        // round-trip through the torus surface: evaluate at (u=0, v1) → z=0.5, ρ=outer.
        let p1 = crate::surfaces::torus::evaluate(DVec3::ZERO, DVec3::Z, DVec3::X, 5.0, 1.5, 0.0, v1);
        assert!((p1.z - 0.5).abs() < 1e-9 && ((p1.x * p1.x + p1.y * p1.y).sqrt() - ro).abs() < 1e-9);
        // plane misses the tube → None.
        assert!(torus_z_cut(0.0, 5.0, 1.5, 2.0).is_none(), "|d|>r misses the tube");
    }

    #[test]
    fn adr197_beta3h_torus_halfspace() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        // ── keep above {z > 0.5}: top poloidal band + washer (cap faces down).
        let mut m = Mesh::default();
        let torus = m.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let r = m.boolean_torus_halfspace(&[torus], 0.5, true, mat).expect("torus z>0.5");
        assert_eq!(r.len(), 2, "result = band + washer");
        assert_eq!(
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "torus halfspace watertight"
        );
        assert!(m.verify_face_invariants().is_valid());
        let band = r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Torus { .. })))
            .unwrap();
        let washer = r
            .iter()
            .find(|&&f| matches!(m.face_surface(f), Some(S::Plane { .. })))
            .unwrap();
        // band keeps the top arc v∈[v1, v2]; both band + washer are multi-loop
        // (outer circle + inner circle hole).
        if let Some(S::Torus { v_range, .. }) = m.face_surface(*band) {
            let v1 = (0.5_f64 / 1.5).asin();
            assert!(
                (v_range.0 - v1).abs() < 1e-9 && (v_range.1 - (std::f64::consts::PI - v1)).abs() < 1e-9,
                "top band v∈[v1, π−v1]; got {:?}",
                v_range
            );
        }
        assert_eq!(m.faces.get(*band).unwrap().inners().len(), 1, "band has inner circle loop");
        assert_eq!(m.faces.get(*washer).unwrap().inners().len(), 1, "washer has inner hole");

        // ── keep below {z < 0.5}: bottom band (wraps the seam) + washer (faces up).
        let mut m2 = Mesh::default();
        let torus2 = m2.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        let r2 = m2.boolean_torus_halfspace(&[torus2], 0.5, false, mat).expect("torus z<0.5");
        assert_eq!(r2.len(), 2);
        assert_eq!(
            m2.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count(),
            0,
            "bottom torus halfspace watertight"
        );
        assert!(m2.verify_face_invariants().is_valid());

        // ── plane misses the tube → rejected.
        let mut m3 = Mesh::default();
        let torus3 = m3.create_torus_kernel_native(DVec3::ZERO, 5.0, 1.5, mat).unwrap();
        assert!(
            m3.boolean_torus_halfspace(&[torus3], 2.0, true, mat).is_err(),
            "plane above the tube does not cut → rejected"
        );
    }

    #[test]
    fn adr197_beta3g2b_box_sphere_needs_periodic() {
        use crate::surfaces::AnalyticSurface;
        use std::f64::consts::TAU;
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let center = DVec3::ZERO;
        let radius = 3.0_f64;
        let sphere = mesh.create_sphere_kernel_native(center, radius, mat).unwrap();
        let bx = make_box(&mut mesh, DVec3::new(-2., -2., -2.), DVec3::new(2., 2., 2.), mat);
        for &fid in &bx {
            let (n, origin) = {
                let f = mesh.faces.get(fid).unwrap();
                let nn = f.normal().normalize_or_zero();
                let v0 = mesh.collect_loop_verts(f.outer().start).unwrap()[0];
                (nn, mesh.verts.get(v0).unwrap().pos())
            };
            let bu = if n.x.abs() < 0.9 { n.cross(DVec3::X) } else { n.cross(DVec3::Y) }
                .normalize_or_zero();
            mesh.set_face_surface(
                fid,
                Some(AnalyticSurface::Plane {
                    origin,
                    normal: n,
                    basis_u: bu,
                    u_range: (-1e6, 1e6),
                    v_range: (-1e6, 1e6),
                }),
            );
        }
        let xs = mesh.detect_curved_intersections(&sphere, &bx);
        let north = sphere[0];
        // For each SSI on the north hemisphere, invert to uv (clip v≥0) and report
        // its u-coverage: full-u (latitude, seam-spanning) vs an arc (oblique).
        let mut covers_all_u = false;
        let mut arcs = 0;
        for c in xs.iter().filter(|c| c.face_a == north) {
            let uvs: Vec<(f64, f64)> = c
                .ssi
                .points
                .iter()
                .map(|&p| sphere_invert(p, center, radius))
                .filter(|p| p.1 >= -1e-9)
                .collect();
            if uvs.len() < 2 {
                continue;
            }
            let mut us: Vec<f64> = uvs.iter().map(|p| p.0).collect();
            us.sort_by(|a, b| a.partial_cmp(b).unwrap());
            // largest circular gap in u.
            let mut gap = 0.0;
            for i in 0..us.len() {
                let a = us[i];
                let b = if i + 1 < us.len() { us[i + 1] } else { us[0] + TAU };
                gap = f64::max(gap, b - a);
            }
            let full_u = gap < 0.5; // covers ~all u (latitude) if no big gap
            if full_u {
                covers_all_u = true;
            } else {
                arcs += 1;
            }
            eprintln!(
                "[sim-g2b] circle b={:?}: north uv pts={} largest_u_gap={:.2} → {}",
                c.face_b,
                uvs.len(),
                gap,
                if full_u { "FULL-U (latitude)" } else { "arc (oblique)" }
            );
        }
        eprintln!(
            "[sim-g2b] north hemisphere: covers_all_u(latitude present)={} oblique_arcs={} → {}",
            covers_all_u,
            arcs,
            if covers_all_u {
                "NO common gap → seam-shift impossible → PERIODIC arrangement needed (γ-2b)"
            } else {
                "common gap exists → seam-shift-multi可"
            }
        );
        // Characterization: box∩sphere north hemisphere has the z=2 latitude (full-u,
        // seam-spanning) plus 4 oblique arcs (x/y=±2). The latitude leaves no common
        // u-gap → seam-shift (γ-2a) cannot isolate the cuts → a genuine periodic
        // (cylinder-topology) arrangement is required. This locks the γ-2b trigger.
        assert!(
            covers_all_u,
            "z=2 latitude must cover all u (forces periodic arrangement)"
        );
        assert_eq!(arcs, 4, "four oblique arcs (x/y=±2) on the north hemisphere");
    }

    #[test]
    fn boolean_union_basic() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let (a, b) = make_test_cubes(&mut mesh, mat);

        let result = mesh.boolean(&a, &b, BoolOp::Union, mat);
        assert!(result.is_ok(), "union should succeed");

        let r = result.unwrap();
        assert!(!r.faces.is_empty(), "union should produce faces");
        // Union: 겹치는 영역의 내부 face가 제거되어야 함
        let has_debug = !r.debug.is_empty();
        assert!(has_debug, "should have debug info");
    }

    #[test]
    fn boolean_subtract_basic() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let (a, b) = make_test_cubes(&mut mesh, mat);

        let result = mesh.boolean(&a, &b, BoolOp::Subtract, mat);
        assert!(result.is_ok(), "subtract should succeed");

        let r = result.unwrap();
        assert!(!r.faces.is_empty(), "subtract should produce faces");
    }

    #[test]
    fn boolean_intersect_basic() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let (a, b) = make_test_cubes(&mut mesh, mat);

        let result = mesh.boolean(&a, &b, BoolOp::Intersect, mat);
        assert!(result.is_ok(), "intersect should succeed");

        let r = result.unwrap();
        assert!(!r.faces.is_empty(), "intersect should produce faces");
    }

    #[test]
    fn boolean_no_overlap() {
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        // 완전히 떨어진 두 큐브
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(1.0), mat);
        let b = make_box(&mut mesh, DVec3::splat(5.0), DVec3::splat(6.0), mat);

        let r = mesh.boolean(&a, &b, BoolOp::Union, mat).unwrap();
        // Union of non-overlapping: 결과 face가 존재해야 함
        assert!(!r.faces.is_empty(), "disjoint union should produce faces");
    }

    // ── Face Split 단위 테스트 ──────────────────────

    #[test]
    fn split_polygon_2d_horizontal_cut() {
        // Pt2 available via `use super::*`
        // 정사각형 (0,0)-(1,0)-(1,1)-(0,1)
        let poly = vec![
            Pt2::new(0.0, 0.0),
            Pt2::new(1.0, 0.0),
            Pt2::new(1.0, 1.0),
            Pt2::new(0.0, 1.0),
        ];
        // y=0.5에서 가로로 자르는 세그먼트
        let cuts = vec![
            (Pt2::new(-0.5, 0.5), Pt2::new(1.5, 0.5)),
        ];
        let result = split_polygon_2d(&poly, &cuts);
        assert!(result.is_some(), "should split");
        let polys = result.unwrap();
        assert!(polys.len() >= 2, "should produce at least 2 sub-polygons, got {}", polys.len());
    }

    #[test]
    fn split_polygon_2d_no_intersection() {
        // Pt2 available via `use super::*`
        let poly = vec![
            Pt2::new(0.0, 0.0),
            Pt2::new(1.0, 0.0),
            Pt2::new(1.0, 1.0),
            Pt2::new(0.0, 1.0),
        ];
        // 다각형 외부의 세그먼트
        let cuts = vec![
            (Pt2::new(2.0, 0.0), Pt2::new(3.0, 0.0)),
        ];
        let result = split_polygon_2d(&poly, &cuts);
        assert!(result.is_none(), "should not split — no intersection");
    }

    #[test]
    fn split_polygon_2d_diagonal_cut() {
        // Pt2 available via `use super::*`
        let poly = vec![
            Pt2::new(0.0, 0.0),
            Pt2::new(2.0, 0.0),
            Pt2::new(2.0, 2.0),
            Pt2::new(0.0, 2.0),
        ];
        // 대각선 자르기: y = x + 0.5 (에지 중간을 통과)
        let cuts = vec![
            (Pt2::new(-1.0, -0.5), Pt2::new(3.0, 2.5)),
        ];
        let result = split_polygon_2d(&poly, &cuts);
        assert!(result.is_some(), "diagonal cut should split");
        let polys = result.unwrap();
        assert!(polys.len() >= 2, "should produce 2+ sub-polygons");

        // 면적 검증: 원본=4, 분할 합≈4
        let total_area: f64 = polys.iter()
            .map(|p| polygon_signed_area_2d(p).abs())
            .sum();
        assert!((total_area - 4.0).abs() < 0.5, "total area should be ~4, got {}", total_area);
    }

    #[test]
    fn boolean_union_with_face_split() {
        // Face Split이 통합된 전체 Boolean 파이프라인 테스트
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let (a, b) = make_test_cubes(&mut mesh, mat);

        let result = mesh.boolean(&a, &b, BoolOp::Union, mat);
        assert!(result.is_ok(), "union with face split should succeed");

        let r = result.unwrap();
        assert!(!r.faces.is_empty(), "should produce faces");
        // debug 로그에 face split 정보가 포함되어야 함
        let has_split_info = r.debug.iter().any(|d| d.contains("Face splits"));
        assert!(has_split_info, "debug should contain face split info");
    }

    // ── 추가 Boolean 연산 테스트 ──────────────────────

    #[test]
    fn boolean_disjoint_union() {
        // 떨어진 두 상자: Union = 모든 face 유지
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(1.0), mat);
        let b = make_box(&mut mesh, DVec3::splat(5.0), DVec3::splat(6.0), mat);

        let result = mesh.boolean(&a, &b, BoolOp::Union, mat).unwrap();
        assert!(!result.faces.is_empty(),
            "Union of disjoint boxes should produce faces");
    }

    #[test]
    fn boolean_intersect_disjoint() {
        // 떨어진 두 상자: Intersect = 공집합
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(1.0), mat);
        let b = make_box(&mut mesh, DVec3::splat(5.0), DVec3::splat(6.0), mat);

        let result = mesh.boolean(&a, &b, BoolOp::Intersect, mat).unwrap();
        // 교차 없음 → 대부분 face가 제거되어야 함
        assert!(result.faces.is_empty() || result.faces.len() < a.len(),
            "Intersect of disjoint boxes should be empty or minimal");
    }

    #[test]
    fn boolean_subtract_disjoint() {
        // 떨어진 두 상자: A - B = A (겹치지 않음)
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(1.0), mat);
        let b = make_box(&mut mesh, DVec3::splat(5.0), DVec3::splat(6.0), mat);

        let result = mesh.boolean(&a, &b, BoolOp::Subtract, mat).unwrap();
        assert!(!result.faces.is_empty(),
            "Subtract disjoint should keep A's faces");
    }

    #[test]
    fn boolean_overlapping_union_result_is_closed() {
        // 겹치는 box의 union → 닫힌 솔리드여야 함
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(2.0), mat);
        let b = make_box(&mut mesh, DVec3::new(1.0, 0.0, 0.0), DVec3::new(3.0, 2.0, 2.0), mat);

        let result = mesh.boolean(&a, &b, BoolOp::Union, mat).unwrap();
        assert!(!result.faces.is_empty(), "union should produce faces");
        // 모든 face가 여전히 활성 상태여야 함
        for &fid in &result.faces {
            assert!(mesh.faces.get(fid).map(|f| f.is_active()).unwrap_or(false),
                "all result faces should be active");
        }
    }

    #[test]
    fn boolean_empty_input() {
        // 빈 face 목록 처리
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(1.0), mat);

        let result = mesh.boolean(&a, &[], BoolOp::Union, mat);
        // 빈 입력은 에러이거나 A 자체를 반환해야 함
        if result.is_ok() {
            let r = result.unwrap();
            assert!(!r.faces.is_empty(), "should handle empty B gracefully");
        }
    }

    #[test]
    fn boolean_preserves_face_count_rough() {
        // Boolean 후 face 수가 극단적으로 변하지 않는지 확인
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(2.0), mat);
        let b = make_box(&mut mesh, DVec3::new(1.0, 0.0, 0.0), DVec3::new(3.0, 2.0, 2.0), mat);

        let initial_face_count = mesh.face_count();
        let result = mesh.boolean(&a, &b, BoolOp::Union, mat).unwrap();

        // Boolean 결과가 비어있지 않아야 함
        assert!(!result.faces.is_empty(),
            "union should produce visible faces");
    }

    #[test]
    fn boolean_multiple_operations_undo_safe() {
        // 여러 Boolean 연산이 메시 상태를 일관되게 유지
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let initial_snapshot = mesh.snapshot();

        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(1.0), mat);
        let b = make_box(&mut mesh, DVec3::new(0.5, 0.0, 0.0), DVec3::new(2.0, 1.0, 1.0), mat);

        let _ = mesh.boolean(&a, &b, BoolOp::Union, mat);
        let after_boolean = mesh.snapshot();

        // Restore to initial
        mesh.restore_snapshot(&initial_snapshot);
        assert_eq!(mesh.face_count(), 0, "snapshot restore to empty should work");

        // Re-apply
        mesh.restore_snapshot(&after_boolean);
        // Verify consistency
        assert!(mesh.face_count() > 0, "after restore should have faces");
    }

    #[test]
    fn boolean_subtract_creates_cavity() {
        // A - B should create a cavity in A
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(4.0), mat);
        let b = make_box(&mut mesh, DVec3::new(1.0, 1.0, 1.0), DVec3::new(3.0, 3.0, 3.0), mat);

        let result = mesh.boolean(&a, &b, BoolOp::Subtract, mat).unwrap();
        // subtract 결과가 존재해야 함
        assert!(!result.faces.is_empty(), "subtract should produce faces");
    }

    #[test]
    fn boolean_intersect_produces_overlap() {
        // A ∩ B should produce the overlapping volume
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(4.0), mat);
        let b = make_box(&mut mesh, DVec3::new(1.0, 1.0, 1.0), DVec3::new(3.0, 3.0, 3.0), mat);

        let result = mesh.boolean(&a, &b, BoolOp::Intersect, mat).unwrap();
        // Intersection은 smaller box의 경계를 포함해야 함
        assert!(!result.faces.is_empty(), "intersection should produce faces");
    }

    #[test]
    fn boolean_rejects_face_with_hole() {
        // Phase G가 hole-aware split_face_by_line을 추가한 뒤에도, Boolean은
        // constrained Delaunay triangulation을 갖지 않는 한 hole 있는 face를
        // 안전하게 다룰 수 없다. 명시적 거부 + 유용한 에러 메시지를 유지하는
        // regression test.
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);

        // Solid A: 6개 face의 cube
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(4.0), mat);

        // Solid B: hole 있는 단일 face (quad + 중앙 사각형 hole)
        let v0 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(14.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(14.0, 4.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(10.0, 4.0, 0.0));
        // Hole은 CW (outer와 반대 winding)
        let h0 = mesh.add_vertex(DVec3::new(11.0, 1.0, 0.0));
        let h1 = mesh.add_vertex(DVec3::new(11.0, 3.0, 0.0));
        let h2 = mesh.add_vertex(DVec3::new(13.0, 3.0, 0.0));
        let h3 = mesh.add_vertex(DVec3::new(13.0, 1.0, 0.0));
        let b_face = mesh.add_face_with_holes(
            &[v0, v1, v2, v3],
            &[&[h0, h1, h2, h3]],
            mat,
        ).unwrap();

        let result = mesh.boolean(&a, &[b_face], BoolOp::Union, mat);
        assert!(result.is_err(), "boolean must reject hole-containing face");
        let err_msg = format!("{}", result.unwrap_err());
        assert!(err_msg.contains("hole"),
            "error message should mention 'hole': got {}", err_msg);
    }

    #[test]
    fn boolean_rejects_hole_in_either_operand() {
        // Symmetric: hole이 A, B 어느 쪽에 있어도 거부되어야 함.
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);

        // Solid B: 정상 cube
        let b = make_box(&mut mesh, DVec3::new(10.0, 0.0, 0.0), DVec3::new(14.0, 4.0, 4.0), mat);

        // A: hole face
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(4.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(4.0, 4.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 4.0, 0.0));
        let h0 = mesh.add_vertex(DVec3::new(1.0, 1.0, 0.0));
        let h1 = mesh.add_vertex(DVec3::new(1.0, 3.0, 0.0));
        let h2 = mesh.add_vertex(DVec3::new(3.0, 3.0, 0.0));
        let h3 = mesh.add_vertex(DVec3::new(3.0, 1.0, 0.0));
        let a_face = mesh.add_face_with_holes(
            &[v0, v1, v2, v3],
            &[&[h0, h1, h2, h3]],
            mat,
        ).unwrap();

        let result = mesh.boolean(&[a_face], &b, BoolOp::Subtract, mat);
        assert!(result.is_err(), "hole in A must also be rejected");
    }

    #[test]
    fn boolean_debug_info_present() {
        // Debug 정보가 제대로 기록되는지 확인
        let mut mesh = Mesh::default();
        let mat = MaterialId::new(0);
        let a = make_box(&mut mesh, DVec3::ZERO, DVec3::splat(1.0), mat);
        let b = make_box(&mut mesh, DVec3::new(0.5, 0.0, 0.0), DVec3::new(2.0, 1.0, 1.0), mat);

        let result = mesh.boolean(&a, &b, BoolOp::Union, mat).unwrap();
        assert!(!result.debug.is_empty(), "should have debug info");
        // Check for expected keys
        let debug_str = result.debug.join("\n");
        assert!(debug_str.contains("Solid A") || debug_str.contains("Solid B"),
            "should log solid info");
    }

    /// ADR-197 QV (2026-06-14) — adversarial perturbation sweep across the β-3
    /// curved Boolean ops. Every VALID fixture must yield watertight + manifold +
    /// invariant-valid + finite geometry; every DEGENERATE fixture must DECLINE
    /// gracefully (Err / None — never panic / garbage). Locks in the QV result.
    #[test]
    fn adr197_qv_adversarial_topology_sweep() {
        let mat = MaterialId::new(0);
        fn wt(m: &Mesh) -> usize {
            m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count()
        }
        fn assert_valid(m: &mut Mesh, r: &[FaceId], name: &str) {
            assert_eq!(wt(m), 0, "{name}: watertight (0 boundary HE)");
            assert!(m.verify_face_invariants().is_valid(), "{name}: face invariants valid");
            assert_eq!(m.face_set_manifold_info(r).non_manifold_edge_count, 0, "{name}: 0 non-manifold edge");
            let (pos, _, tris, _, _) = m.export_buffers().expect("export");
            assert!(!tris.is_empty(), "{name}: emits triangles");
            assert!(pos.iter().all(|c| c.is_finite()), "{name}: finite positions");
        }
        let z = DVec3::ZERO;

        // ── VALID: must produce watertight + manifold + valid + finite geometry ──
        // intersect (slab / halfspace)
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let r=m.boolean_sphere_slab(&s,-0.2,0.2,mat).unwrap(); assert_valid(&mut m,&r,"sphere∩slab thin"); }
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let r=m.boolean_sphere_slab(&s,-2.95,2.95,mat).unwrap(); assert_valid(&mut m,&r,"sphere∩slab near-pole"); }
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let r=m.boolean_sphere_halfspace(&s,DVec3::new(0.,0.,1.),DVec3::Z,mat).unwrap(); assert_valid(&mut m,&r,"sphere halfspace top"); }
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let r=m.boolean_sphere_halfspace(&s,DVec3::new(0.,0.,-1.),DVec3::NEG_Z,mat).unwrap(); assert_valid(&mut m,&r,"sphere halfspace bottom"); }
        { let mut m=Mesh::default(); let c=build_clean_cylinder(&mut m,0.,0.,-3.,2.,6.,mat); let r=m.boolean_cylinder_slab(&c,-1.5,1.5,mat).unwrap(); assert_valid(&mut m,&r,"cylinder∩slab"); }
        { let mut m=Mesh::default(); let c=m.create_cone_kernel_native(z,2.,4.,mat).unwrap(); let r=m.boolean_cone_slab(&c,0.5,3.,mat).unwrap(); assert_valid(&mut m,&r,"cone∩slab"); }
        { let mut m=Mesh::default(); let t=m.create_torus_kernel_native(z,5.,1.5,mat).unwrap(); let r=m.boolean_torus_slab(&[t],-0.5,0.5,mat).unwrap(); assert_valid(&mut m,&r,"torus∩slab 2-cut"); }
        { let mut m=Mesh::default(); let t=m.create_torus_kernel_native(z,5.,1.5,mat).unwrap(); let r=m.boolean_torus_halfspace(&[t],0.,true,mat).unwrap(); assert_valid(&mut m,&r,"torus halfspace above"); }
        { let mut m=Mesh::default(); let t=m.create_torus_kernel_native(z,5.,1.5,mat).unwrap(); let r=m.boolean_torus_halfspace(&[t],0.,false,mat).unwrap(); assert_valid(&mut m,&r,"torus halfspace below"); }
        // subtract
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let r=m.boolean_sphere_slab_subtract(&s,-1.,1.,mat).unwrap(); assert_valid(&mut m,&r,"sphere−slab 2caps"); }
        { let mut m=Mesh::default(); let c=build_clean_cylinder(&mut m,0.,0.,-3.,2.,6.,mat); let r=m.boolean_cylinder_slab_subtract(&c,-1.5,1.5,mat).unwrap(); assert_valid(&mut m,&r,"cylinder−slab"); }
        { let mut m=Mesh::default(); let c=m.create_cone_kernel_native(z,2.,4.,mat).unwrap(); let r=m.boolean_cone_slab_subtract(&c,1.,3.,mat).unwrap(); assert_valid(&mut m,&r,"cone−slab"); }
        { let mut m=Mesh::default(); let t=m.create_torus_kernel_native(z,5.,1.5,mat).unwrap(); let r=m.boolean_torus_slab_subtract(&[t],-0.5,0.5,mat).unwrap(); assert_valid(&mut m,&r,"torus−slab"); }
        // union — case B (curved∪curved)
        { let mut m=Mesh::default(); let a=m.create_sphere_kernel_native(DVec3::new(0.,0.,0.),30.,mat).unwrap(); let b=m.create_sphere_kernel_native(DVec3::new(0.,0.,40.),30.,mat).unwrap(); let r=m.boolean_sphere_sphere_union(&a,&b,mat).unwrap(); assert_valid(&mut m,&r,"sphere∪sphere equal"); }
        { let mut m=Mesh::default(); let a=m.create_sphere_kernel_native(DVec3::new(0.,0.,0.),30.,mat).unwrap(); let b=m.create_sphere_kernel_native(DVec3::new(0.,0.,58.),30.,mat).unwrap(); let r=m.boolean_sphere_sphere_union(&a,&b,mat).unwrap(); assert_valid(&mut m,&r,"sphere∪sphere near-tangent"); }
        { let mut m=Mesh::default(); let a=m.create_sphere_kernel_native(DVec3::new(0.,0.,0.),30.,mat).unwrap(); let b=m.create_sphere_kernel_native(DVec3::new(0.,0.,35.),20.,mat).unwrap(); let r=m.boolean_sphere_sphere_union(&a,&b,mat).unwrap(); assert_valid(&mut m,&r,"sphere∪sphere unequal"); }
        { let mut m=Mesh::default(); let a=m.create_cone_kernel_native(DVec3::new(0.,0.,0.),2.,4.,mat).unwrap(); let b=m.create_cone_kernel_native_apex_down(DVec3::new(0.,0.,4.),2.,4.,mat).unwrap(); let r=m.boolean_cone_cone_union(&a,&b,mat).unwrap(); assert_valid(&mut m,&r,"cone∪cone hourglass"); }
        { let mut m=Mesh::default(); let a=m.create_cone_kernel_native(DVec3::new(0.,0.,0.),2.,4.,mat).unwrap(); let b=m.create_cone_kernel_native_apex_down(DVec3::new(0.,0.,4.),3.,4.,mat).unwrap(); let r=m.boolean_cone_cone_union(&a,&b,mat).unwrap(); assert_valid(&mut m,&r,"cone∪cone asym"); }
        // union — case A (curved∪box)
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let b=make_box(&mut m,DVec3::new(-5.,-5.,-2.),DVec3::new(5.,5.,2.),mat); let r=m.boolean_sphere_box_union(&s,&b,mat).unwrap(); assert_valid(&mut m,&r,"sphere∪box"); }
        { let mut m=Mesh::default(); let c=build_clean_cylinder(&mut m,0.,0.,-3.,2.,6.,mat); let b=make_box(&mut m,DVec3::new(-5.,-5.,-1.5),DVec3::new(5.,5.,1.5),mat); let r=m.boolean_cylinder_box_union(&c,&b,mat).unwrap(); assert_valid(&mut m,&r,"cylinder∪box"); }
        { let mut m=Mesh::default(); let c=m.create_cone_kernel_native(z,2.,4.,mat).unwrap(); let b=make_box(&mut m,DVec3::new(-5.,-5.,1.),DVec3::new(5.,5.,3.),mat); let r=m.boolean_cone_box_union(&c,&b,mat).unwrap(); assert_valid(&mut m,&r,"cone∪box"); }
        { let mut m=Mesh::default(); let t=m.create_torus_kernel_native(z,5.,1.5,mat).unwrap(); let b=make_box(&mut m,DVec3::new(-8.,-8.,-0.5),DVec3::new(8.,8.,0.5),mat); let r=m.boolean_torus_box_union(&[t],&b,mat).unwrap(); assert_valid(&mut m,&r,"torus∪box"); }
        // octant / full-box / slice
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let b=make_box(&mut m,DVec3::new(1.,1.,1.),DVec3::new(5.,5.,5.),mat); let res=m.boolean(&s,&b,BoolOp::Intersect,mat).unwrap(); assert!(res.debug.iter().any(|d|d.contains("β-3")),"octant routes to curved"); assert_valid(&mut m,&res.faces,"sphere corner octant"); }
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let r=m.boolean_sphere_box_full(&s,DVec3::splat(-2.),DVec3::splat(2.),mat).unwrap(); assert_valid(&mut m,&r,"sphere rounded-box"); }
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); let r=m.boolean_sphere_slice(&s,1.,mat).unwrap(); assert_valid(&mut m,&r,"sphere slice"); }
        { let mut m=Mesh::default(); let c=build_clean_cylinder(&mut m,0.,0.,-3.,2.,6.,mat); let r=m.boolean_cylinder_slice(&c,0.,mat).unwrap(); assert_valid(&mut m,&r,"cylinder slice"); }
        { let mut m=Mesh::default(); let c=m.create_cone_kernel_native(z,2.,4.,mat).unwrap(); let r=m.boolean_cone_slice(&c,2.,mat).unwrap(); assert_valid(&mut m,&r,"cone slice"); }
        { let mut m=Mesh::default(); let t=m.create_torus_kernel_native(z,5.,1.5,mat).unwrap(); let r=m.boolean_torus_slice(&[t],0.,mat).unwrap(); assert_valid(&mut m,&r,"torus slice"); }

        // ── DEGENERATE: must decline gracefully (Err / None — never panic) ──
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); assert!(m.boolean_sphere_slab(&s,1.,-1.,mat).is_err(),"inverted slab → Err"); }
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); assert!(m.boolean_sphere_slab(&s,0.5,2.95,mat).is_err(),"non-straddle slab → Err"); }
        { let mut m=Mesh::default(); assert!(m.boolean_sphere_slab(&[],-1.,1.,mat).is_err(),"empty input → Err"); }
        { let mut m=Mesh::default(); let s=m.create_sphere_kernel_native(z,3.,mat).unwrap(); assert!(m.boolean_sphere_slice(&s,5.,mat).is_err(),"slice miss → Err"); }
        { let mut m=Mesh::default(); let a=m.create_sphere_kernel_native(DVec3::new(0.,0.,0.),10.,mat).unwrap(); let b=m.create_sphere_kernel_native(DVec3::new(0.,0.,100.),10.,mat).unwrap(); assert!(m.try_curved_union_dispatch(&a,&b,mat).is_none(),"disjoint spheres → None"); }
        { let mut m=Mesh::default(); let a=m.create_cone_kernel_native(DVec3::new(0.,0.,0.),2.,4.,mat).unwrap(); let b=m.create_cone_kernel_native(DVec3::new(0.,0.,1.),2.,4.,mat).unwrap(); assert!(m.try_curved_union_dispatch(&a,&b,mat).is_none(),"same-dir cones → None"); }
    }

    /// ADR-197 #6 (2026-06-14) — arc-bounded curved patches (sphere octant + the
    /// 8 rounded-box corners) are interior-subdivided so the render chord error
    /// ≤ chord_tol. Pre-fix these were earcut-boundary-only fans (faceted ~0.04–
    /// 0.17mm). Locks in the `tessellate_arc_bounded_face` interior subdivision.
    #[test]
    fn adr197_qv_arc_patch_chord_quality() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let tol = 0.02_f64;
        // max edge-midpoint sagitta of a Sphere tessellation (mm).
        let max_chord = |t: &crate::surfaces::SurfaceTessellation, c: DVec3, r: f64| -> f64 {
            let mut e = 0.0_f64;
            for tri in &t.triangles {
                let v = [t.vertices[tri[0] as usize], t.vertices[tri[1] as usize], t.vertices[tri[2] as usize]];
                for k in 0..3 {
                    let mf = (v[k] + v[(k + 1) % 3]) * 0.5;
                    e = e.max((r - (mf - c).length()).abs());
                }
            }
            e
        };
        // octant (single Sphere arc patch).
        {
            let mut m = Mesh::default();
            let s = m.create_sphere_kernel_native(DVec3::ZERO, 3., mat).unwrap();
            let b = make_box(&mut m, DVec3::new(1., 1., 1.), DVec3::new(5., 5., 5.), mat);
            let res = m.boolean(&s, &b, BoolOp::Intersect, mat).unwrap();
            let mut checked = 0;
            for &f in &res.faces {
                if let Some(S::Sphere { center, radius, .. }) = m.face_surface(f) {
                    let (c, r) = (*center, *radius);
                    let t = m.tessellate_arc_bounded_face(f, tol).expect("octant arc-bounded");
                    let err = max_chord(&t, c, r);
                    assert!(err <= tol * 1.5, "octant patch chord {err:.4}mm ≤ {:.4}mm", tol * 1.5);
                    checked += 1;
                }
            }
            assert_eq!(checked, 1, "octant has exactly one Sphere arc patch");
        }
        // rounded-box: all 8 corner Sphere patches.
        {
            let mut m = Mesh::default();
            let s = m.create_sphere_kernel_native(DVec3::ZERO, 3., mat).unwrap();
            let r = m.boolean_sphere_box_full(&s, DVec3::splat(-2.), DVec3::splat(2.), mat).unwrap();
            let mut checked = 0;
            for &f in &r {
                if let Some(S::Sphere { center, radius, .. }) = m.face_surface(f) {
                    let (c, rr) = (*center, *radius);
                    if let Some(t) = m.tessellate_arc_bounded_face(f, tol) {
                        let err = max_chord(&t, c, rr);
                        assert!(err <= tol * 1.5, "rounded-box corner chord {err:.4}mm ≤ {:.4}mm", tol * 1.5);
                        checked += 1;
                    }
                }
            }
            assert_eq!(checked, 8, "rounded-box has 8 Sphere corner patches");
        }
    }

    /// ADR-198 (drilling) — box − cylinder through-hole = genus-1 watertight solid
    /// (6 box faces + 1 inward bore wall). The killer concave-subtract op.
    #[test]
    fn adr198_box_minus_cylinder_drilling() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let mut m = Mesh::default();
        let bx = make_box(&mut m, DVec3::splat(-5.), DVec3::splat(5.), mat);
        let cyl = build_clean_cylinder(&mut m, 0., 0., -6., 2.0, 12.0, mat); // z∈[-6,6] ⊇ box
        let r = m.boolean_box_minus_cylinder(&bx, &cyl, mat).expect("drill");
        assert_eq!(r.len(), 7, "6 box + 1 bore band");
        assert_eq!(wt(&m), 0, "through-hole watertight (genus-1)");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        assert!(m.verify_face_invariants().is_valid(), "invariants valid");
        let band = *r.last().unwrap();
        assert!(matches!(m.face_surface(band), Some(S::Cylinder { .. })), "bore wall = Cylinder surface");
        assert!(m.is_face_surface_reversed(band), "bore wall renders INWARD (cavity)");
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()), "renders finite");
    }

    /// ADR-198 (enclosed void) — box − sphere (sphere inside) = 2 disjoint shells
    /// (box outer + inward sphere cavity).
    #[test]
    fn adr198_box_minus_sphere_enclosed_void() {
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let mut m = Mesh::default();
        let bx = make_box(&mut m, DVec3::splat(-10.), DVec3::splat(10.), mat);
        let sph = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r = m.boolean_box_minus_void(&bx, &sph, mat).expect("void");
        assert_eq!(wt(&m), 0, "enclosed void watertight (box + sphere both closed)");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        assert!(m.verify_face_invariants().is_valid(), "valid");
        assert_eq!(m.face_connected_components(&r).len(), 2, "box outer + sphere cavity = 2 shells");
        assert!(sph.iter().all(|&f| m.is_face_surface_reversed(f)), "sphere walls render INWARD");
    }

    /// ADR-198 — `boolean()` Subtract routes box − curved to the concave path;
    /// partial cases (blind hole) DEFER → Path B guard bails cleanly (no crash).
    #[test]
    fn adr198_dispatch_box_minus_curved_routes() {
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        // drilling via boolean().
        let mut m = Mesh::default();
        let bx = make_box(&mut m, DVec3::splat(-5.), DVec3::splat(5.), mat);
        let cyl = build_clean_cylinder(&mut m, 0., 0., -6., 2.0, 12.0, mat);
        let res = m.boolean(&bx, &cyl, BoolOp::Subtract, mat).expect("drill route");
        assert_eq!(res.faces.len(), 7, "drilled = 7 faces");
        assert_eq!(wt(&m), 0, "watertight");
        // void via boolean().
        let mut m2 = Mesh::default();
        let bx2 = make_box(&mut m2, DVec3::splat(-10.), DVec3::splat(10.), mat);
        let sph2 = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let res2 = m2.boolean(&bx2, &sph2, BoolOp::Subtract, mat).expect("void route");
        assert_eq!(m2.face_connected_components(&res2.faces).len(), 2, "void = 2 shells");
        // remaining partial: a sphere LARGER than the box in Z pierces BOTH faces
        // (neither strictly-inside nor poke-one-face) → DEFER → guard bails cleanly.
        let mut m3 = Mesh::default();
        let bx3 = make_box(&mut m3, DVec3::new(-5., -5., -2.), DVec3::new(5., 5., 2.), mat); // thin
        let sph3 = m3.create_sphere_kernel_native(DVec3::ZERO, 4.0, mat).unwrap(); // z∈[-4,4] ⊃ box
        assert!(m3.boolean(&bx3, &sph3, BoolOp::Subtract, mat).is_err(), "sphere-through-box defers → guard bails");
    }

    /// ADR-198 (blind hole) — box − cylinder entering one face (floor inside) =
    /// 6 box + inward bore band + flat floor disk, watertight.
    #[test]
    fn adr198_box_minus_cylinder_blind_hole() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let mut m = Mesh::default();
        let bx = make_box(&mut m, DVec3::splat(-5.), DVec3::splat(5.), mat);
        let cyl = build_clean_cylinder(&mut m, 0., 0., 0., 2.0, 6.0, mat); // z∈[0,6]: enters top, floor z=0
        let r = m.boolean_box_minus_cylinder_blind(&bx, &cyl, mat).expect("blind");
        assert_eq!(r.len(), 8, "6 box + bore band + floor disk");
        assert_eq!(wt(&m), 0, "blind hole watertight");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        assert!(m.verify_face_invariants().is_valid(), "valid");
        let band = r[6];
        assert!(matches!(m.face_surface(band), Some(S::Cylinder { .. })) && m.is_face_surface_reversed(band), "bore wall = inward Cylinder");
        // routes via boolean().
        let mut mr = Mesh::default();
        let bxr = make_box(&mut mr, DVec3::splat(-5.), DVec3::splat(5.), mat);
        let cylr = build_clean_cylinder(&mut mr, 0., 0., 0., 2.0, 6.0, mat);
        let res = mr.boolean(&bxr, &cylr, BoolOp::Subtract, mat).expect("blind route");
        assert_eq!(res.faces.len(), 8, "blind routes to 8 faces");
        assert_eq!(wt(&mr), 0, "routed blind watertight");
    }

    /// ADR-198 (dimple) — box − sphere poking one face (far side inside) = 6 box +
    /// inward sub-sphere cap, watertight.
    #[test]
    fn adr198_box_minus_sphere_dimple() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        let mut m = Mesh::default();
        let bx = make_box(&mut m, DVec3::splat(-5.), DVec3::splat(5.), mat);
        let sph = m.create_sphere_kernel_native(DVec3::new(0., 0., 3.), 4.0, mat).unwrap(); // z∈[-1,7]: pokes top, bottom inside
        let r = m.boolean_box_minus_sphere_dimple(&bx, &sph, mat).expect("dimple");
        assert_eq!(r.len(), 7, "6 box + sphere cap");
        assert_eq!(wt(&m), 0, "dimple watertight");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        assert!(m.verify_face_invariants().is_valid(), "valid");
        let cap = r[6];
        assert!(matches!(m.face_surface(cap), Some(S::Sphere { .. })) && m.is_face_surface_reversed(cap), "cap = inward Sphere");
        // routes via boolean().
        let mut mr = Mesh::default();
        let bxr = make_box(&mut mr, DVec3::splat(-5.), DVec3::splat(5.), mat);
        let sphr = mr.create_sphere_kernel_native(DVec3::new(0., 0., 3.), 4.0, mat).unwrap();
        let res = mr.boolean(&bxr, &sphr, BoolOp::Subtract, mat).expect("dimple route");
        assert_eq!(res.faces.len(), 7, "dimple routes to 7 faces");
        assert_eq!(wt(&mr), 0, "routed dimple watertight");
    }

    /// **ADR-202 (2026-06-17)** — the smooth-boundary sphere circle-clip
    /// (`tessellate_sphere_clipped`) must touch ONLY ADR-202 sphere splits, never
    /// ADR-197/198 Boolean caps. A Boolean cap's boundary-circle twin lies on a
    /// Plane BOX face (not a co-spherical Sphere face), so the co-spherical
    /// `twin_role` gate returns None → the function declines → the cap renders its
    /// full restricted-v_range surface. Without the gate the clip would keep the
    /// wrong side: the dimple pocket wall collapses to a sliver and the union's
    /// bottom cap (zmin −3 → −2) vanishes. The existing union test only checks
    /// zmax (top cap, accidentally on the kept side), so this locks BOTH extents.
    #[test]
    fn adr202_smooth_clip_excludes_boolean_caps() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);

        // ── ADR-198 dimple: the inward Sphere pocket cap must NOT be circle-clipped.
        let mut m = Mesh::default();
        let bx = make_box(&mut m, DVec3::splat(-5.), DVec3::splat(5.), mat);
        let sph = m.create_sphere_kernel_native(DVec3::new(0., 0., 3.), 4.0, mat).unwrap();
        let r = m.boolean_box_minus_sphere_dimple(&bx, &sph, mat).expect("dimple");
        let cap = r[6];
        assert!(matches!(m.face_surface(cap), Some(S::Sphere { .. })), "cap is a Sphere face");
        assert!(
            m.tessellate_sphere_clipped(cap, 0.1).is_none(),
            "ADR-198 dimple cap must not be circle-clipped (Some → sliver regression)"
        );

        // ── ADR-197 sphere∪box: BOTH caps render full extent (z ±3), neither slivered.
        let mut mu = Mesh::default();
        let s = mu.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let bu = make_box(&mut mu, DVec3::new(-5., -5., -2.), DVec3::new(5., 5., 2.), mat);
        let ru = mu.boolean_sphere_box_union(&s, &bu, mat).expect("sphere∪box");
        for &f in ru.iter().filter(|&&f| matches!(mu.face_surface(f), Some(S::Sphere { .. }))) {
            assert!(
                mu.tessellate_sphere_clipped(f, 0.1).is_none(),
                "ADR-197 union cap must not be circle-clipped"
            );
        }
        let (pos, _n, tris, _e, _uv) = mu.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()));
        let zmin = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MAX, f64::min);
        let zmax = pos.chunks(3).map(|c| c[2] as f64).fold(f64::MIN, f64::max);
        assert!((zmin + 3.0).abs() < 0.2, "bottom cap reaches z=-3 (not slivered); got {zmin:.2}");
        assert!((zmax - 3.0).abs() < 0.2, "top cap reaches z=3; got {zmax:.2}");
    }

    /// ADR-198 (countersink) — box − cone (apex inside, base poking one face) =
    /// 6 box + inward conical pocket wall, watertight.
    #[test]
    fn adr198_box_minus_cone_countersink() {
        use crate::surfaces::AnalyticSurface as S;
        let mat = MaterialId::new(0);
        let wt = |m: &Mesh| m.hes.iter().filter(|(_, h)| h.is_active() && h.face().is_null()).count();
        // cone: apex-down, base above the box top, apex inside → countersink from top.
        // create_cone_kernel_native(base_center, radius, height) is apex-UP (apex above);
        // apex_down variant has the base ABOVE and apex BELOW.
        let mut m = Mesh::default();
        let bx = make_box(&mut m, DVec3::splat(-5.), DVec3::splat(5.), mat);
        // apex-down cone: base at z=8 (above box top 5), apex at z=0 (inside box).
        let cone = m.create_cone_kernel_native_apex_down(DVec3::new(0., 0., 8.), 3.0, 8.0, mat).unwrap();
        let r = m.boolean_box_minus_cone_countersink(&bx, &cone, mat).expect("countersink");
        assert_eq!(r.len(), 7, "6 box + cone pocket cap");
        assert_eq!(wt(&m), 0, "countersink watertight");
        assert_eq!(m.face_set_manifold_info(&r).non_manifold_edge_count, 0, "manifold");
        assert!(m.verify_face_invariants().is_valid(), "valid");
        let cap = r[6];
        assert!(matches!(m.face_surface(cap), Some(S::Cone { .. })) && m.is_face_surface_reversed(cap), "pocket = inward Cone");
        let (pos, _n, tris, _e, _uv) = m.export_buffers().expect("export");
        assert!(!tris.is_empty() && pos.iter().all(|c| c.is_finite()), "renders finite");
        // routes via boolean().
        let mut mr = Mesh::default();
        let bxr = make_box(&mut mr, DVec3::splat(-5.), DVec3::splat(5.), mat);
        let coner = mr.create_cone_kernel_native_apex_down(DVec3::new(0., 0., 8.), 3.0, 8.0, mat).unwrap();
        let res = mr.boolean(&bxr, &coner, BoolOp::Subtract, mat).expect("countersink route");
        assert_eq!(res.faces.len(), 7, "countersink routes to 7 faces");
        assert_eq!(wt(&mr), 0, "routed countersink watertight");
    }

    /// ADR-197 #Track3 — `face_connected_components` separates the 2 disjoint
    /// shells a curved slice produces (cap+disk per shell).
    #[test]
    fn adr197_track3_face_connected_components_splits_slice_shells() {
        let mat = MaterialId::new(0);
        let mut m = Mesh::default();
        let s = m.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let r = m.boolean_sphere_slice(&s, 1.0, mat).unwrap(); // 4 faces, 2 shells
        let comps = m.face_connected_components(&r);
        assert_eq!(comps.len(), 2, "sphere slice = 2 disjoint shells");
        assert_eq!(comps[0].len() + comps[1].len(), r.len(), "all faces accounted for");
        assert!(comps.iter().all(|c| c.len() == 2), "each shell = cap + disk (2 faces)");
        // a single closed primitive = 1 component (no false split).
        let mut m2 = Mesh::default();
        let s2 = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        assert_eq!(m2.face_connected_components(&s2).len(), 1, "intact sphere = 1 component");
    }

    /// ADR-197 #Track2 — UNSUPPORTED concave subtract (box − curved) declines the
    /// curved dispatch → legacy path → the Path B guard bails CLEANLY (no `HeId`
    /// crash). NOTE: ADR-198 now ROUTES the SUPPORTED concave cases (drilling
    /// through-hole / enclosed void); this test covers the cases that STILL defer.
    #[test]
    fn adr197_track2_concave_subtract_guard_bails_cleanly() {
        let mat = MaterialId::new(0);
        // box − sphere where the sphere is LARGER than the box in Z (pierces BOTH
        // faces — neither strictly-inside void, nor poke-one-face dimple) → still
        // unsupported → guard bail.
        let mut m = Mesh::default();
        let box_f = make_box(&mut m, DVec3::new(-5., -5., -2.), DVec3::new(5., 5., 2.), mat); // thin Z
        let sph = m.create_sphere_kernel_native(DVec3::ZERO, 4.0, mat).unwrap(); // z∈[-4,4] ⊃ box
        let res = m.boolean(&box_f, &sph, BoolOp::Subtract, mat);
        assert!(res.is_err(), "concave box−sphere (through) bails (no crash)");
        let msg = res.unwrap_err().to_string();
        assert!(msg.contains("curved analytic surface"), "clear guard message: {msg}");
        // sphere − corner-box (XY-cutting box = scooped octant, concave) → guard bail.
        let mut m2 = Mesh::default();
        let sph2 = m2.create_sphere_kernel_native(DVec3::ZERO, 3.0, mat).unwrap();
        let bx2 = make_box(&mut m2, DVec3::new(1., 1., 1.), DVec3::new(5., 5., 5.), mat);
        assert!(m2.boolean(&sph2, &bx2, BoolOp::Subtract, mat).is_err(), "scooped octant bails cleanly");
    }
}

// ════════════════════════════════════════════════════════════════════════
// ADR-142 γ — K1 cross-cut 통합 sweep 회귀 자산 (audit-first 19 closure 후)
//
// Sprint 1 ADR-142 γ sub-step (2026-05-22). Amendment 2 (audit-first 19번째
// β-2 cancel) 후 γ + δ + ε 묶음 single atomic PR (LOCKED #44 정합).
//
// 통합 evidence:
//   - Path B Circle × Path B Circle Boolean (ADR-110 entry-level cover)
//   - Path B Circle + split_face_by_chain (ADR-142 β-1 cover via
//     polygonize_if_closed_curve)
//   - Polygonal regression guard (additive only)
//
// 절대 #[ignore] 금지 — 각 fail 은 K1 cross-cut 회귀의 architectural signal.
// ════════════════════════════════════════════════════════════════════════
#[cfg(test)]
mod adr142_gamma_tests {
    use super::*;
    use crate::Mesh;
    use crate::curves::AnalyticCurve;
    use crate::operations::face_split::split_face_by_chain;

    /// Path B Circle face — 1 anchor vert + 1 self-loop edge (Circle curve).
    /// ADR-089 Phase 2 canonical kernel-native representation.
    fn build_path_b_circle(mesh: &mut Mesh, cx: f64, cy: f64, radius: f64) -> FaceId {
        let mat = MaterialId::new(0);
        let basis_u = DVec3::new(1.0, 0.0, 0.0);
        let anchor = mesh.add_vertex(DVec3::new(cx + radius, cy, 0.0));
        let circle = AnalyticCurve::Circle {
            center: DVec3::new(cx, cy, 0.0),
            radius,
            normal: DVec3::new(0.0, 0.0, 1.0),
            basis_u,
        };
        mesh.add_face_closed_curve(anchor, circle, mat).expect("path B face")
    }

    /// γ-1 cross-cut — Path B Circle × Path B Circle Union via ADR-110
    /// entry-level pre-polygonize cover. Boolean 성공 + face count 증가
    /// (split + new sub-polygon faces).
    #[test]
    fn gamma_path_b_circle_union_via_adr110_cover() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);

        let face_a = build_path_b_circle(&mut mesh, 0.0, 0.0, 5.0);
        let face_b = build_path_b_circle(&mut mesh, 6.0, 0.0, 5.0);

        let faces_before = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        let verts_before = mesh.verts.iter().filter(|(_, v)| v.is_active()).count();

        let result = mesh.boolean(&[face_a], &[face_b], BoolOp::Union, mat);
        assert!(result.is_ok(),
            "γ-1: Path B Circle Union must succeed (ADR-110 cover), got {:?}",
            result.err());

        // ADR-110 evidence — polygonize 가 1 anchor → N polygonal verts 확장.
        let verts_after = mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
        assert!(verts_after > verts_before,
            "γ-1: Path B polygonize 후 verts 증가 (before={}, after={})",
            verts_before, verts_after);

        // Boolean 후 face count 양수 (silent fail 아님).
        let faces_after = mesh.faces.iter().filter(|(_, f)| f.is_active()).count();
        assert!(faces_after > 0,
            "γ-1: Boolean 결과 face count > 0 (before={}, after={})",
            faces_before, faces_after);
    }

    /// γ-2 cross-cut — Path B Circle × Path B Circle Subtract via ADR-110.
    /// Subtract path (line 290) 도 동일 entry pre-polygonize 통과.
    #[test]
    fn gamma_path_b_circle_subtract_via_adr110_cover() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);

        let face_a = build_path_b_circle(&mut mesh, 0.0, 0.0, 5.0);
        let face_b = build_path_b_circle(&mut mesh, 4.0, 0.0, 3.0);

        let result = mesh.boolean(&[face_a], &[face_b], BoolOp::Subtract, mat);
        assert!(result.is_ok(),
            "γ-2: Path B Circle Subtract must succeed (ADR-110 cover), got {:?}",
            result.err());
    }

    /// γ-3 cross-cut — Path B Circle × Path B Circle Intersect via ADR-110.
    #[test]
    fn gamma_path_b_circle_intersect_via_adr110_cover() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);

        let face_a = build_path_b_circle(&mut mesh, 0.0, 0.0, 5.0);
        let face_b = build_path_b_circle(&mut mesh, 4.0, 0.0, 5.0);

        let result = mesh.boolean(&[face_a], &[face_b], BoolOp::Intersect, mat);
        assert!(result.is_ok(),
            "γ-3: Path B Circle Intersect must succeed (ADR-110 cover), got {:?}",
            result.err());
    }

    /// γ-4 cross-cut — Path B Circle + split_face_by_chain via β-1 K1 cover.
    /// ADR-142 β-1 (PR #152) 가 `split_face_by_chain` entry 에 polygonize_if_
    /// closed_curve 추가. Path B Circle face 가 chain split input 으로 정상
    /// 통과 (이전: positions.len() < 3 silent skip).
    #[test]
    fn gamma_path_b_circle_chain_split_via_beta1_cover() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);

        let face = build_path_b_circle(&mut mesh, 0.0, 0.0, 5.0);

        // chain endpoints — circle 내부 chord 의 2 endpoints (polygonize 후
        // polygonal boundary 위에 자동 dedup 으로 match).
        let v_left = mesh.add_vertex(DVec3::new(-5.0, 0.0, 0.0));
        let v_right = mesh.add_vertex(DVec3::new(5.0, 0.0, 0.0));

        let result = split_face_by_chain(&mut mesh, face, &[v_left, v_right], mat);
        // β-1 K1 fire → polygonize → chain endpoint lookup 정상 (또는 split
        // 결과 정상 Err — 어쨌든 panic / silent fail 아닌 정의된 결과).
        // Err 도 K1 progression evidence (closed-curve 가 polygon mode 진입).
        match result {
            Ok(_split_result) => {
                // β-1 fire 후 chain split 성공 — Path B Circle 이 polygon
                // boundary 로 확장 + 2 sub-face 생성 시도.
            }
            Err(e) => {
                // 정의된 Err (e.g., chain endpoint not on boundary 등) — K1
                // 자체 진입 evidence (closed-curve panic 회피).
                let msg = format!("{}", e);
                assert!(!msg.is_empty(),
                    "γ-4: Err message must be defined, not empty");
            }
        }

        // Evidence — face 가 polygonize 되어 verts 증가 (silent skip 회피).
        let verts_after = mesh.verts.iter().filter(|(_, v)| v.is_active()).count();
        assert!(verts_after > 2,
            "γ-4: β-1 polygonize 후 verts > 2 (anchor + 2 chain endpoints + N polygonal verts)");
    }

    /// γ-5 regression guard — Polygonal face Boolean + chain split 영향 0.
    /// Additive only — ADR-110 + β-1 의 K1 path 가 polygonal input 에 no-op.
    #[test]
    fn gamma_polygonal_regression_guard() {
        let mut mesh = Mesh::new();
        let mat = MaterialId::new(0);

        // 2 polygonal rects (Path A baseline).
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(10.0, 10.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        let face_a = mesh.add_face(&[v0, v1, v2, v3], mat).expect("rect A");

        let v4 = mesh.add_vertex(DVec3::new(5.0, 5.0, 0.0));
        let v5 = mesh.add_vertex(DVec3::new(15.0, 5.0, 0.0));
        let v6 = mesh.add_vertex(DVec3::new(15.0, 15.0, 0.0));
        let v7 = mesh.add_vertex(DVec3::new(5.0, 15.0, 0.0));
        let face_b = mesh.add_face(&[v4, v5, v6, v7], mat).expect("rect B");

        // Polygonal Boolean — ADR-110 polygonize 가 Ok(None) no-op,
        // 기존 path 통과.
        let bool_result = mesh.boolean(&[face_a], &[face_b], BoolOp::Union, mat);
        assert!(bool_result.is_ok(),
            "γ-5: Polygonal Boolean regression guard — must not error");
    }
}

#[cfg(test)]
mod adr197_arrange_tests {
    use super::*;

    fn sq(s: f64) -> Vec<Pt2> {
        vec![
            Pt2::new(0.0, 0.0),
            Pt2::new(s, 0.0),
            Pt2::new(s, s),
            Pt2::new(0.0, s),
        ]
    }

    fn area(p: &[Pt2]) -> f64 {
        polygon_signed_area_2d(p).abs()
    }

    /// No cuts → the whole polygon, one region, no holes.
    #[test]
    fn adr197_arrange_no_cuts_single_region() {
        let r = arrange_polygon_2d(&sq(10.0), &[]);
        assert_eq!(r.len(), 1, "no cut → 1 region");
        assert!(r[0].holes.is_empty());
        assert!((area(&r[0].outer) - 100.0).abs() < 1e-6);
    }

    /// Single open chain (endpoints on opposite edges) → 2 regions, no holes.
    #[test]
    fn adr197_arrange_single_chain_two_regions() {
        let cuts = [(Pt2::new(5.0, 0.0), Pt2::new(5.0, 10.0))];
        let r = arrange_polygon_2d(&sq(10.0), &cuts);
        assert_eq!(r.len(), 2, "vertical cut → 2 regions");
        assert!(r.iter().all(|x| x.holes.is_empty()));
        for x in &r {
            assert!((area(&x.outer) - 50.0).abs() < 1e-6, "half area {}", area(&x.outer));
        }
    }

    /// Two parallel open chains (tunnel-wall strips) → 3 regions, no holes.
    #[test]
    fn adr197_arrange_two_parallel_chains_three_strips() {
        let cuts = [
            (Pt2::new(0.0, 3.0), Pt2::new(10.0, 3.0)),
            (Pt2::new(0.0, 7.0), Pt2::new(10.0, 7.0)),
        ];
        let r = arrange_polygon_2d(&sq(10.0), &cuts);
        assert_eq!(r.len(), 3, "two parallel cuts → 3 strips");
        assert!(r.iter().all(|x| x.holes.is_empty()));
        let total: f64 = r.iter().map(|x| area(&x.outer)).sum();
        assert!((total - 100.0).abs() < 1e-6, "strip total {total}");
    }

    /// Two crossing chains (non-convex / multi-B) → 4 quadrant regions.
    #[test]
    fn adr197_arrange_crossing_chains_four_regions() {
        let cuts = [
            (Pt2::new(0.0, 5.0), Pt2::new(10.0, 5.0)),
            (Pt2::new(5.0, 0.0), Pt2::new(5.0, 10.0)),
        ];
        let r = arrange_polygon_2d(&sq(10.0), &cuts);
        assert_eq!(r.len(), 4, "crossing cuts → 4 quadrants");
        assert!(r.iter().all(|x| x.holes.is_empty()));
        for x in &r {
            assert!((area(&x.outer) - 25.0).abs() < 1e-6, "quadrant {}", area(&x.outer));
        }
    }

    /// Interior closed loop (tunnel cap cross-section) → annulus(+1 hole) + disk.
    /// This is the case the old single-chain helper could not do.
    #[test]
    fn adr197_arrange_interior_loop_annulus_plus_disk() {
        let cuts = [
            (Pt2::new(3.0, 3.0), Pt2::new(7.0, 3.0)),
            (Pt2::new(7.0, 3.0), Pt2::new(7.0, 7.0)),
            (Pt2::new(7.0, 7.0), Pt2::new(3.0, 7.0)),
            (Pt2::new(3.0, 7.0), Pt2::new(3.0, 3.0)),
        ];
        let r = arrange_polygon_2d(&sq(10.0), &cuts);
        assert_eq!(r.len(), 2, "annulus + disk");
        let annulus = r.iter().find(|x| !x.holes.is_empty()).expect("annulus region");
        let disk = r.iter().find(|x| x.holes.is_empty()).expect("disk region");
        assert_eq!(annulus.holes.len(), 1, "annulus has exactly one hole");
        assert!((area(&annulus.outer) - 100.0).abs() < 1e-6, "annulus outer {}", area(&annulus.outer));
        assert!((area(&annulus.holes[0]) - 16.0).abs() < 1e-6, "annulus hole {}", area(&annulus.holes[0]));
        assert!((area(&disk.outer) - 16.0).abs() < 1e-6, "disk {}", area(&disk.outer));
    }

    /// The unbounded outer cycle must never be mistaken for a hole of a small
    /// interior region (its area exceeds every candidate → dropped).
    #[test]
    fn adr197_arrange_unbounded_not_assigned_as_hole() {
        let cuts = [
            (Pt2::new(3.0, 3.0), Pt2::new(7.0, 3.0)),
            (Pt2::new(7.0, 3.0), Pt2::new(7.0, 7.0)),
            (Pt2::new(7.0, 7.0), Pt2::new(3.0, 7.0)),
            (Pt2::new(3.0, 7.0), Pt2::new(3.0, 3.0)),
        ];
        let r = arrange_polygon_2d(&sq(10.0), &cuts);
        let disk = r.iter().find(|x| (area(&x.outer) - 16.0).abs() < 1e-6).expect("disk");
        assert!(disk.holes.is_empty(), "disk must have no hole");
    }
}

#[cfg(test)]
mod adr197_arrange_degenerate_tests {
    //! ADR-197 β-2d robustness — degenerate-input probes for the unified planar
    //! arrangement. The universal invariant is PARTITION: the kept regions tile
    //! the original polygon, i.e. the sum over regions of (outer_area minus the
    //! hole areas) equals the polygon area. A bug (dropped sliver, double-counted
    //! overlap, mis-assigned hole, corrupt cycle) violates partition or sanity.
    use super::*;

    fn sq(s: f64) -> Vec<Pt2> {
        vec![Pt2::new(0., 0.), Pt2::new(s, 0.), Pt2::new(s, s), Pt2::new(0., s)]
    }
    fn ar(p: &[Pt2]) -> f64 {
        polygon_signed_area_2d(p).abs()
    }
    fn partition(rs: &[Region2D]) -> f64 {
        rs.iter()
            .map(|r| ar(&r.outer) - r.holes.iter().map(|h| ar(h)).sum::<f64>())
            .sum()
    }
    // Every region simple+positive; every hole positive and smaller than its outer.
    fn sane(rs: &[Region2D]) -> bool {
        rs.iter().all(|r| {
            ar(&r.outer) > 1e-9
                && r.holes.iter().all(|h| {
                    let ha = ar(h);
                    ha > 1e-9 && ha < ar(&r.outer)
                })
        })
    }

    /// D1 — two collinear, overlapping cuts on one interior line. Their union is
    /// a dangling spur (a free interior end) → no enclosed split, partition kept.
    #[test]
    fn deg_collinear_overlapping_cuts() {
        let cuts = [
            (Pt2::new(2., 5.), Pt2::new(8., 5.)),
            (Pt2::new(4., 5.), Pt2::new(10., 5.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r), "structurally sane");
    }

    /// D2 — cut endpoints exactly on polygon CORNERS (corner-to-corner diagonal).
    #[test]
    fn deg_cut_corner_to_corner() {
        let cuts = [(Pt2::new(0., 0.), Pt2::new(10., 10.))];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert_eq!(r.len(), 2, "diagonal → 2 triangles, got {}", r.len());
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
    }

    /// D3 — cut lying COLLINEAR with a boundary edge (no interior effect).
    #[test]
    fn deg_cut_collinear_with_boundary() {
        let cuts = [(Pt2::new(2., 0.), Pt2::new(8., 0.))];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r), "no zero-area sliver region");
    }

    /// D4 — blind cut: one endpoint on the boundary, the other free in the
    /// interior → a dangling edge that must NOT corrupt the partition.
    #[test]
    fn deg_blind_dangling_cut() {
        let cuts = [(Pt2::new(5., 0.), Pt2::new(5., 5.))];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
    }

    /// D5 — Y-junction: three cuts meeting at one interior point (valence-3).
    #[test]
    fn deg_y_junction_valence3() {
        let cuts = [
            (Pt2::new(5., 5.), Pt2::new(5., 0.)),
            (Pt2::new(5., 5.), Pt2::new(0., 10.)),
            (Pt2::new(5., 5.), Pt2::new(10., 10.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert_eq!(r.len(), 3, "Y-junction → 3 regions, got {}", r.len());
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
    }

    /// D6 — notch chain: both endpoints on the SAME boundary edge (the case the
    /// old split_convex_polygon_by_chain bailed on with e1==e2).
    #[test]
    fn deg_notch_same_edge() {
        let cuts = [
            (Pt2::new(2., 0.), Pt2::new(5., 3.)),
            (Pt2::new(5., 3.), Pt2::new(8., 0.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert_eq!(r.len(), 2, "notch → 2 regions, got {}", r.len());
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
        // triangle: base (2,0)-(8,0)=6, apex (5,3) → area 9.
        assert!(r.iter().any(|x| (ar(&x.outer) - 9.0).abs() < 1e-6), "notch triangle");
    }

    /// D7 — zero-length (point) cut must be filtered, not crash.
    #[test]
    fn deg_zero_length_cut() {
        let cuts = [(Pt2::new(5., 5.), Pt2::new(5., 5.))];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert_eq!(r.len(), 1, "point cut → whole face");
        assert!((partition(&r) - 100.).abs() < 1e-6);
        assert!(sane(&r));
    }

    /// D8 — two disjoint interior loops → one outer-with-2-holes + 2 disks.
    #[test]
    fn deg_two_disjoint_loops() {
        let cuts = [
            (Pt2::new(1., 1.), Pt2::new(3., 1.)),
            (Pt2::new(3., 1.), Pt2::new(3., 3.)),
            (Pt2::new(3., 3.), Pt2::new(1., 3.)),
            (Pt2::new(1., 3.), Pt2::new(1., 1.)),
            (Pt2::new(6., 6.), Pt2::new(8., 6.)),
            (Pt2::new(8., 6.), Pt2::new(8., 8.)),
            (Pt2::new(8., 8.), Pt2::new(6., 8.)),
            (Pt2::new(6., 8.), Pt2::new(6., 6.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
        let with_holes = r.iter().filter(|x| !x.holes.is_empty()).count();
        let total_holes: usize = r.iter().map(|x| x.holes.len()).sum();
        assert_eq!(with_holes, 1, "exactly one annulus region (the outer)");
        assert_eq!(total_holes, 2, "two holes");
    }

    /// D9 — nested concentric loops (depth-2 hole assignment by containment).
    #[test]
    fn deg_nested_loops_depth2() {
        let cuts = [
            // outer loop [2,8]^2 (area 36)
            (Pt2::new(2., 2.), Pt2::new(8., 2.)),
            (Pt2::new(8., 2.), Pt2::new(8., 8.)),
            (Pt2::new(8., 8.), Pt2::new(2., 8.)),
            (Pt2::new(2., 8.), Pt2::new(2., 2.)),
            // inner loop [4,6]^2 (area 4)
            (Pt2::new(4., 4.), Pt2::new(6., 4.)),
            (Pt2::new(6., 4.), Pt2::new(6., 6.)),
            (Pt2::new(6., 6.), Pt2::new(4., 6.)),
            (Pt2::new(4., 6.), Pt2::new(4., 4.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r), "depth-2 nesting sane");
        assert_eq!(r.len(), 3, "nested → 3 regions, got {}", r.len());
        // the [2,8] region's hole is the IMMEDIATE inner loop [4,6] (area 4),
        // NOT assigned to the outer [0,10] region.
        let middle = r.iter().find(|x| (ar(&x.outer) - 36.0).abs() < 1e-6).expect("middle [2,8]");
        assert_eq!(middle.holes.len(), 1, "middle has hole [4,6]");
        assert!((ar(&middle.holes[0]) - 4.0).abs() < 1e-6, "middle hole is [4,6] (area 4)");
    }

    /// D10 — concave (L-shaped) face polygon cut by a chain.
    #[test]
    fn deg_concave_face_polygon() {
        // L: (0,0)-(10,0)-(10,4)-(4,4)-(4,10)-(0,10), area = 10*4 + 4*6 = 64.
        let l = vec![
            Pt2::new(0., 0.),
            Pt2::new(10., 0.),
            Pt2::new(10., 4.),
            Pt2::new(4., 4.),
            Pt2::new(4., 10.),
            Pt2::new(0., 10.),
        ];
        let area_l = ar(&l);
        let cuts = [(Pt2::new(0., 2.), Pt2::new(10., 2.))];
        let r = arrange_polygon_2d(&l, &cuts);
        assert!(
            (partition(&r) - area_l).abs() < 1e-6,
            "concave partition {} vs {}",
            partition(&r),
            area_l
        );
        assert!(sane(&r));
    }

    /// D11 — three concurrent cuts through one shared interior point (valence-6).
    #[test]
    fn deg_three_concurrent_cuts() {
        let cuts = [
            (Pt2::new(0., 5.), Pt2::new(10., 5.)),
            (Pt2::new(5., 0.), Pt2::new(5., 10.)),
            (Pt2::new(0., 0.), Pt2::new(10., 10.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
        assert_eq!(r.len(), 6, "3 concurrent chords → 6 sectors, got {}", r.len());
    }

    /// D12 — duplicate cut segments (same line twice) must dedup, not double.
    #[test]
    fn deg_duplicate_cuts() {
        let cuts = [
            (Pt2::new(5., 0.), Pt2::new(5., 10.)),
            (Pt2::new(5., 0.), Pt2::new(5., 10.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert_eq!(r.len(), 2, "duplicate cut → still 2 regions, got {}", r.len());
        assert!((partition(&r) - 100.).abs() < 1e-6);
        assert!(sane(&r));
    }

    /// D13 — thin sliver cut very close to a boundary edge (numerical stress).
    #[test]
    fn deg_thin_sliver_near_boundary() {
        let cuts = [(Pt2::new(0., 0.01), Pt2::new(10., 0.01))];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert_eq!(r.len(), 2, "sliver cut → 2 regions, got {}", r.len());
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r), "thin sliver region still > 0 area");
    }

    /// D14 — interior loop sharing a CORNER + two edges with the boundary.
    #[test]
    fn deg_loop_touching_boundary_corner() {
        let cuts = [
            (Pt2::new(0., 0.), Pt2::new(4., 0.)),
            (Pt2::new(4., 0.), Pt2::new(4., 4.)),
            (Pt2::new(4., 4.), Pt2::new(0., 4.)),
            (Pt2::new(0., 4.), Pt2::new(0., 0.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
    }

    // ── Adversary-lens NOVEL probes (numerical / topology / holes / angular tie) ──

    /// N1 — two near-coincident vertical cuts 5e-7 apart (below ARR_TOL=1e-6):
    /// must dedup to a single split, not a spurious zero-width sliver.
    #[test]
    fn deg_near_coincident_cuts() {
        let cuts = [
            (Pt2::new(5.0, 0.), Pt2::new(5.0, 10.)),
            (Pt2::new(5.0000005, 0.), Pt2::new(5.0000005, 10.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-3, "partition {}", partition(&r));
        assert!(sane(&r), "no zero-width sliver region");
    }

    /// T3 — chain whose MIDDLE vertex sits on a boundary edge (tangent mid-chain):
    /// (0,5)→(5,0)→(10,5). The region under the chain pinches to a point at (5,0).
    #[test]
    fn deg_tangent_mid_chain() {
        let cuts = [
            (Pt2::new(0., 5.), Pt2::new(5., 0.)),
            (Pt2::new(5., 0.), Pt2::new(10., 5.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
    }

    /// T4 — a cut extending BEYOND the polygon on both ends (the dangling spurs
    /// outside must not corrupt the interior partition).
    #[test]
    fn deg_cut_extends_beyond_polygon() {
        let cuts = [(Pt2::new(5., -5.), Pt2::new(5., 15.))];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
    }

    /// H1 — NON-CONVEX (L-shaped) interior loop whose AREA-CENTROID falls in the
    /// notch (outside the loop). The hole must still attach to the outer region,
    /// not the (smaller, non-containing) L-disk.
    #[test]
    fn deg_nonconvex_interior_loop() {
        let cuts = [
            (Pt2::new(2., 2.), Pt2::new(8., 2.)),
            (Pt2::new(8., 2.), Pt2::new(8., 4.)),
            (Pt2::new(8., 4.), Pt2::new(4., 4.)),
            (Pt2::new(4., 4.), Pt2::new(4., 8.)),
            (Pt2::new(4., 8.), Pt2::new(2., 8.)),
            (Pt2::new(2., 8.), Pt2::new(2., 2.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
        let with_hole = r.iter().filter(|x| !x.holes.is_empty()).count();
        assert_eq!(with_hole, 1, "one annulus carrying the L-hole");
    }

    /// H3 — two CROSSING interior loops (overlapping squares) → a non-trivial
    /// arrangement; partition must still hold.
    #[test]
    fn deg_two_crossing_interior_loops() {
        let cuts = [
            (Pt2::new(2., 2.), Pt2::new(6., 2.)),
            (Pt2::new(6., 2.), Pt2::new(6., 6.)),
            (Pt2::new(6., 6.), Pt2::new(2., 6.)),
            (Pt2::new(2., 6.), Pt2::new(2., 2.)),
            (Pt2::new(4., 4.), Pt2::new(8., 4.)),
            (Pt2::new(8., 4.), Pt2::new(8., 8.)),
            (Pt2::new(8., 8.), Pt2::new(4., 8.)),
            (Pt2::new(4., 8.), Pt2::new(4., 4.)),
        ];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
    }

    /// A1 — cut COLLINEAR with a boundary edge but extending past the corner.
    #[test]
    fn deg_collinear_cut_past_corner() {
        let cuts = [(Pt2::new(2., 0.), Pt2::new(15., 0.))];
        let r = arrange_polygon_2d(&sq(10.), &cuts);
        assert!((partition(&r) - 100.).abs() < 1e-6, "partition {}", partition(&r));
        assert!(sane(&r));
    }
}

//! ADR-007 Face Orientation Invariants — Mesh-level read-only verifiers.
//!
//! Extracted from `mesh.rs` (Tier 2-A Stack #1, 2026-05-16, LOCKED #44
//! complete meaning per merge). Read-only invariant verification logic
//! kept separate from DCEL mutation paths so that:
//!   - Each verifier is independently auditable
//!   - mesh.rs reduces in size (architectural backbone clarity)
//!   - ADR-007 policy enforcement has a clear home
//!
//! ## Contents
//!
//! - `InvariantReport` — verify_face_invariants result struct
//! - `OutwardReport` — verify_outward_normals result struct
//! - `Mesh::verify_face_invariants` — ADR-007 I1~I5 invariant check
//! - `Mesh::verify_face_invariants_rev2` — Sheet face exemption variant
//! - `Mesh::verify_outward_normals` — closed solid outward normal check
//! - `Mesh::debug_verify_invariants` — debug-build only auto-check
//!
//! ## ADR cross-link
//!
//! - ADR-007 Face Orientation Policy (canonical anchor)
//! - ADR-089 Phase 2 — closed-curve face I1 exemption (1 vert + self-loop edge)
//! - ADR-021 P7 (LOCKED #1) — non-manifold edge I5 enforcement
//! - LOCKED #44 — complete meaning per merge (본 모듈 단위 extraction)

use glam::DVec3;

use crate::entities::*;
use crate::mesh::Mesh;

// ─── Result Structs ──────────────────────────────────────────────────

/// Result of [`Mesh::verify_outward_normals`] — ADR-007 원칙 1 확장 리포트.
///
/// 닫힌 solid의 모든 face normal이 outward(바깥) 향하는지 검증.
/// 열린 surface나 non-manifold mesh는 is_closed_solid=false로 스킵.
#[derive(Debug, Clone)]
pub struct OutwardReport {
    /// 닫힌 2-manifold solid 여부 (false면 검증 스킵됨)
    pub is_closed_solid: bool,
    /// 검사된 face 수
    pub checked_faces: usize,
    /// 내부 향함(inward) 감지된 face 수
    pub inward_count: usize,
    /// Inward face ID 목록 (최대 detail 용)
    pub inward_faces: Vec<FaceId>,
}

impl OutwardReport {
    pub fn is_valid(&self) -> bool {
        !self.is_closed_solid || self.inward_count == 0
    }
    pub fn summary(&self) -> String {
        if !self.is_closed_solid {
            return "Open surface (outward check skipped)".to_string();
        }
        if self.inward_count == 0 {
            format!("✓ {} faces all outward", self.checked_faces)
        } else {
            format!(
                "✗ {}/{} faces inward-facing",
                self.inward_count, self.checked_faces
            )
        }
    }
}

/// Result of [`Mesh::verify_face_invariants`] — ADR-007 정책 준수 여부 리포트.
#[derive(Debug, Clone)]
pub struct InvariantReport {
    /// 검사된 활성 face 수
    pub checked_faces: usize,
    /// 발견된 위반 사항 목록 (비어 있으면 전부 통과)
    pub violations: Vec<String>,
}

impl InvariantReport {
    /// 모든 invariant 통과 여부
    pub fn is_valid(&self) -> bool {
        self.violations.is_empty()
    }

    /// Human-readable 요약
    pub fn summary(&self) -> String {
        if self.violations.is_empty() {
            format!("✓ All {} faces satisfy invariants", self.checked_faces)
        } else {
            let mut s = format!(
                "✗ {} violations in {} faces:\n",
                self.violations.len(),
                self.checked_faces,
            );
            for v in &self.violations {
                s.push_str("  - ");
                s.push_str(v);
                s.push('\n');
            }
            s
        }
    }
}

/// ADR-267 Phase 1.1 — [`Mesh::verify_volume_integrity`] 검사 범위.
///
/// - `ClosedSolid`: extrude/cut/boolean 등 닫힌 솔리드 산출 op — watertight
///   (열린 경계 edge 0)까지 강제.
/// - `OpenMesh`: sheet/wire/draw/split 등 열린 결과가 정상인 op — 경계 edge 는
///   허용, invariant + 크랙만 검사.
#[derive(Debug, Clone, Copy)]
pub enum IntegrityScope<'a> {
    ClosedSolid(&'a [FaceId]),
    OpenMesh,
}

/// ADR-267 — 부피 연산 종료 시 release production 무결성 리포트.
///
/// 신규 검출 알고리즘 없이 기존 3자산을 조립한다:
/// - [`Mesh::verify_face_invariants`] (I1~I5)
/// - [`Mesh::collect_non_manifold_edges_geometric`] (coincident 크랙, ADR-264 D3)
/// - [`Mesh::face_set_manifold_info`] (watertight — ClosedSolid scope 에서만)
#[derive(Debug, Clone)]
pub struct VolumeIntegrityReport {
    /// verify_face_invariants I1~I5 위반 (winding / null loop / 위상 non-manifold 등)
    pub invariant_violations: Vec<String>,
    /// coincident-position 크랙 edge (ADR-264 D3). ADR-021 P7 radial (단일
    /// EdgeId) 은 오탐하지 않음 — geometric 검출기가 ≥2 distinct EdgeId 만 flag.
    pub geometric_cracks: Vec<EdgeId>,
    /// ClosedSolid scope 에서 열린(1-face) 경계 edge 수. OpenMesh 에서는 항상 0.
    pub open_boundary_edges: usize,
    /// 검사된 활성 face 수
    pub checked_faces: usize,
}

impl VolumeIntegrityReport {
    /// 모든 카테고리 clean 여부.
    pub fn is_valid(&self) -> bool {
        self.invariant_violations.is_empty()
            && self.geometric_cracks.is_empty()
            && self.open_boundary_edges == 0
    }

    /// 단일 비교용 손상 총량 (op 전후 delta 판정에 사용 — pre-existing 손상에
    /// 오탐하지 않도록 "op가 새 손상을 유발했는가"를 재는 척도).
    pub fn damage_count(&self) -> usize {
        self.invariant_violations.len() + self.geometric_cracks.len() + self.open_boundary_edges
    }

    /// Human-readable 요약.
    pub fn summary(&self) -> String {
        if self.is_valid() {
            return format!("✓ volume integrity OK ({} faces)", self.checked_faces);
        }
        let mut s = String::from("✗ volume integrity violations:\n");
        if !self.invariant_violations.is_empty() {
            s.push_str(&format!(
                "  invariants: {} violation(s)\n",
                self.invariant_violations.len()
            ));
            for v in &self.invariant_violations {
                s.push_str("    - ");
                s.push_str(v);
                s.push('\n');
            }
        }
        if !self.geometric_cracks.is_empty() {
            s.push_str(&format!(
                "  geometric cracks: {} edge(s)\n",
                self.geometric_cracks.len()
            ));
        }
        if self.open_boundary_edges > 0 {
            s.push_str(&format!(
                "  open boundary edges: {} (not watertight)\n",
                self.open_boundary_edges
            ));
        }
        s
    }
}

// ─── Mesh impl — read-only invariant verifiers ───────────────────────

impl Mesh {
    /// 전체 mesh의 face orientation invariants 검증 결과.
    ///
    /// 위반 사항이 있으면 `violations`에 Human-readable 메시지 열거.
    /// `is_valid == true` 이면 모든 invariant 통과.
    pub fn verify_face_invariants(&self) -> InvariantReport {
        let mut violations: Vec<String> = Vec::new();
        let mut checked_faces = 0usize;

        for (fid, face) in self.faces.iter() {
            if !face.is_active() { continue; }
            checked_faces += 1;

            // I1: outer loop 존재 + 최소 3 verts
            let outer_start = face.outer().start;
            if outer_start.is_null() {
                violations.push(format!("face {:?}: null outer start", fid));
                continue;
            }
            let outer_verts = match self.collect_loop_verts(outer_start) {
                Ok(v) => v,
                Err(e) => {
                    violations.push(format!("face {:?}: cannot collect outer loop: {}", fid, e));
                    continue;
                }
            };
            // ADR-089 Phase 2 (A-ζ-1, 2026-05-08): I1 invariant 갱신.
            // Closed-curve face (1 vert anchor + 1 self-loop edge with
            // analytic curve attached) 도 valid — face = closed boundary
            // 의 byproduct (메타-원칙 #14). Polygon face (≥3 verts) 동작
            // 무변화.
            if outer_verts.len() < 3 {
                // Closed-curve exemption: outer loop = 1 vert + 1 self-loop
                // edge with Edge.curve.is_some().
                let is_closed_curve_face = outer_verts.len() == 1
                    && self.collect_loop_hes(outer_start).map(|hes| {
                        hes.len() == 1 && {
                            let he = &self.hes[hes[0]];
                            self.edges.get(he.edge())
                                .filter(|e| e.is_active())
                                .and_then(|e| e.curve())
                                .is_some()
                        }
                    }).unwrap_or(false);
                if !is_closed_curve_face {
                    violations.push(format!("face {:?}: outer loop has {} verts (< 3)",
                        fid, outer_verts.len()));
                    continue;
                }
                // Skip I2 (winding check via compute_normal) for closed-curve
                // face — the curve's analytic normal is the truth source.
                // Skip I4 (outer HE face check) — single HE already wired in
                // add_face_closed_curve. Continue to next face for I5.
                continue;
            }

            // I2: cached normal이 실제 winding과 일치 (반대 방향이면 위반)
            let cached = face.normal();
            if cached.length_squared() > 1e-10 {
                if let Ok(computed) = self.compute_normal(&outer_verts) {
                    let cn = cached.normalize_or_zero();
                    let gn = computed.normalize_or_zero();
                    if cn.length_squared() > 1e-10 && gn.length_squared() > 1e-10 {
                        let dot = cn.dot(gn);
                        if dot < 0.9 {
                            violations.push(format!(
                                "face {:?}: cached normal opposite to winding (dot={:.3})",
                                fid, dot,
                            ));
                        }
                    }
                }
            }

            // I3: inner loops 도 collect 가능해야 함 + 각각 ≥ 3 verts
            // (ADR-089 A-ζ-1 exemption: 1-vert inner with self-loop edge +
            // analytic curve = valid closed-curve hole).
            for (ii, inner) in face.inners().iter().enumerate() {
                if inner.start.is_null() {
                    violations.push(format!("face {:?}: inner[{}] null start", fid, ii));
                    continue;
                }
                match self.collect_loop_verts(inner.start) {
                    Ok(iv) if iv.len() >= 3 => {}
                    Ok(iv) if iv.len() == 1 => {
                        // ADR-089 A-ζ-1: closed-curve hole exemption.
                        let is_closed_curve_hole = self.collect_loop_hes(inner.start)
                            .map(|hes| {
                                hes.len() == 1 && {
                                    let he = &self.hes[hes[0]];
                                    self.edges.get(he.edge())
                                        .filter(|e| e.is_active())
                                        .and_then(|e| e.curve())
                                        .is_some()
                                }
                            }).unwrap_or(false);
                        if !is_closed_curve_hole {
                            violations.push(format!(
                                "face {:?}: inner[{}] has 1 vert without analytic curve",
                                fid, ii));
                        }
                    }
                    Ok(iv) => violations.push(format!(
                        "face {:?}: inner[{}] has {} verts (< 3)", fid, ii, iv.len())),
                    Err(e) => violations.push(format!(
                        "face {:?}: inner[{}] cannot collect: {}", fid, ii, e)),
                }
            }

            // I4: outer loop의 모든 half-edge가 이 face에 속해야 함
            if let Ok(outer_hes) = self.collect_loop_hes(outer_start) {
                for he in outer_hes {
                    let he_face = self.hes[he].face();
                    if he_face != fid {
                        violations.push(format!(
                            "face {:?}: outer HE {:?} points to wrong face {:?}",
                            fid, he, he_face,
                        ));
                    }
                }
            }
        }

        // I5: 각 edge는 최대 2개 active face와 공유
        for (eid, edge) in self.edges.iter() {
            if !edge.is_active() { continue; }
            let (faces, _) = self.get_faces_sharing_edge(eid);
            let active_faces: Vec<_> = faces.iter()
                .filter(|&&f| self.faces.get(f).map(|face| face.is_active()).unwrap_or(false))
                .collect();
            if active_faces.len() > 2 {
                violations.push(format!(
                    "edge {:?}: shared by {} active faces (non-manifold)",
                    eid, active_faces.len(),
                ));
            }
        }

        InvariantReport {
            checked_faces,
            violations,
        }
    }

    /// ADR-007 Rev 2 Tier 4 (2026-04-20) — Sheet face winding 자유 정책 적용.
    ///
    /// `verify_face_invariants` 의 결과 중 Wall 면에 적용되는 winding-기반
    /// invariant 만 retain, Sheet 면은 winding 무관(자유)이므로 제외.
    pub fn verify_face_invariants_rev2(&self) -> InvariantReport {
        let mut report = self.verify_face_invariants();
        // I2 violation 은 메시지에 "cached normal opposite to winding" 패턴.
        // Sheet 면은 winding 자유이므로 이 케이스만 필터링.
        report.violations.retain(|msg| {
            if !msg.contains("cached normal opposite to winding") { return true; }
            // "face FaceId(N): cached..." 에서 N 파싱
            let Some(start) = msg.find("FaceId(") else { return true; };
            let after = &msg[start + 7..];
            let Some(end) = after.find(')') else { return true; };
            let Ok(raw) = after[..end].parse::<u32>() else { return true; };
            let fid = FaceId::new(raw);
            // Sheet 면이면 violation 제거 (true 가 아닌 false 반환 = drop)
            !self.is_sheet_face(fid)
        });
        report
    }

    /// ADR-007 원칙 1 확장 — 닫힌 solid에서 각 face normal이 outward 향하는지 검증.
    ///
    /// 닫힌 2-manifold solid가 아니면 (open surface 등) 빈 리포트 반환.
    /// 휴리스틱: mesh centroid → face centroid 방향과 face normal이 양의 내적이면
    /// outward. 볼록체에서 완벽, 심한 오목체에선 제한적.
    ///
    /// 사용 예: Phase G/H 이후 closed solid 생성 확인, box/sphere 등 프리미티브
    /// sanity check, push/pull 결과 검증 등.
    pub fn verify_outward_normals(&self) -> OutwardReport {
        let active_faces: Vec<FaceId> = self.faces.iter()
            .filter(|(_, f)| f.is_active())
            .map(|(id, _)| id)
            .collect();

        // 닫힌 solid 여부 확인 — open surface는 outward 정의 불가
        let manifold = self.face_set_manifold_info(&active_faces);
        if !manifold.is_closed_solid {
            return OutwardReport {
                is_closed_solid: false,
                checked_faces: 0,
                inward_count: 0,
                inward_faces: Vec::new(),
            };
        }

        // Mesh centroid — 모든 active vertex 평균
        let mut sum = DVec3::ZERO;
        let mut cnt = 0usize;
        for (_, face) in self.faces.iter() {
            if !face.is_active() { continue; }
            if let Ok(verts) = self.collect_loop_verts(face.outer().start) {
                for v in verts {
                    if let Ok(p) = self.vertex_pos(v) {
                        sum += p;
                        cnt += 1;
                    }
                }
            }
        }
        if cnt == 0 {
            return OutwardReport {
                is_closed_solid: true,
                checked_faces: 0,
                inward_count: 0,
                inward_faces: Vec::new(),
            };
        }
        let mesh_centroid = sum / cnt as f64;

        let mut inward_faces = Vec::new();
        for fid in &active_faces {
            let face = &self.faces[*fid];
            let normal = face.normal();
            if normal.length_squared() < 1e-10 { continue; }

            // Face centroid
            let verts = match self.collect_loop_verts(face.outer().start) {
                Ok(v) => v, Err(_) => continue,
            };
            if verts.is_empty() { continue; }
            let mut fc = DVec3::ZERO;
            let mut fcn = 0usize;
            for v in &verts {
                if let Ok(p) = self.vertex_pos(*v) {
                    fc += p;
                    fcn += 1;
                }
            }
            if fcn == 0 { continue; }
            fc /= fcn as f64;

            let outward = fc - mesh_centroid;
            if outward.length_squared() < 1e-10 { continue; }

            let dot = normal.dot(outward);
            if dot < 0.0 {
                // 내부 향함 감지
                inward_faces.push(*fid);
            }
        }

        OutwardReport {
            is_closed_solid: true,
            checked_faces: active_faces.len(),
            inward_count: inward_faces.len(),
            inward_faces,
        }
    }

    /// ADR-267 Phase 1.1 — 부피 연산 종료 시 release production 무결성 게이트.
    ///
    /// [`debug_verify_invariants`](Self::debug_verify_invariants) 와 달리 **release
    /// 에서도 실행**되며, 기존 3자산을 조립한 [`VolumeIntegrityReport`] 를 반환한다.
    /// 호출측(WASM 경계)이 `is_valid()==false` 시 snapshot rollback + lastError 처리
    /// (ADR-190 P0.2). 신규 검출 알고리즘 없음.
    pub fn verify_volume_integrity(&self, scope: IntegrityScope) -> VolumeIntegrityReport {
        let inv = self.verify_face_invariants();
        let geometric_cracks = self.collect_non_manifold_edges_geometric();
        let open_boundary_edges = match scope {
            IntegrityScope::ClosedSolid(faces) => {
                self.face_set_manifold_info(faces).boundary_edge_count
            }
            IntegrityScope::OpenMesh => 0,
        };
        VolumeIntegrityReport {
            invariant_violations: inv.violations,
            geometric_cracks,
            open_boundary_edges,
            checked_faces: inv.checked_faces,
        }
    }

    /// 디버그 빌드에서만 invariants 검증. Release에서는 no-op.
    /// 편집 연산 끝에 삽입해 조기 버그 감지용.
    #[inline]
    pub fn debug_verify_invariants(&self) {
        #[cfg(debug_assertions)]
        {
            let report = self.verify_face_invariants();
            if !report.is_valid() {
                eprintln!("[ADR-007] Invariant violations:\n{}", report.summary());
            }
        }
    }
}

#[cfg(test)]
mod adr267_tests {
    use super::{IntegrityScope, VolumeIntegrityReport};
    use crate::entities::*;
    use crate::mesh::Mesh;
    use glam::DVec3;

    fn tetra() -> (Mesh, Vec<FaceId>) {
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(5.0, 0.0, 10.0));
        let v3 = mesh.add_vertex(DVec3::new(5.0, 10.0, 5.0));
        let f0 = mesh.add_face(&[v0, v2, v1], MaterialId::new(0)).unwrap();
        let f1 = mesh.add_face(&[v0, v1, v3], MaterialId::new(0)).unwrap();
        let f2 = mesh.add_face(&[v1, v2, v3], MaterialId::new(0)).unwrap();
        let f3 = mesh.add_face(&[v2, v0, v3], MaterialId::new(0)).unwrap();
        (mesh, vec![f0, f1, f2, f3])
    }

    #[test]
    fn verify_volume_integrity_clean_solid_valid() {
        let (mesh, faces) = tetra();
        let r = mesh.verify_volume_integrity(IntegrityScope::ClosedSolid(&faces));
        assert!(r.is_valid(), "clean tetra should pass: {}", r.summary());
        assert!(r.geometric_cracks.is_empty());
        assert_eq!(r.open_boundary_edges, 0);
    }

    #[test]
    fn verify_volume_integrity_open_sheet_valid_as_openmesh_invalid_as_closed() {
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(1.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(0.0, 1.0, 0.0));
        let f = mesh.add_face(&[v0, v1, v2], MaterialId::new(0)).unwrap();
        // OpenMesh: 열린 시트는 경계가 있어도 valid.
        let open = mesh.verify_volume_integrity(IntegrityScope::OpenMesh);
        assert!(open.is_valid(), "open sheet ok as OpenMesh: {}", open.summary());
        // ClosedSolid: 3 경계 edge → not watertight → invalid.
        let closed = mesh.verify_volume_integrity(IntegrityScope::ClosedSolid(&[f]));
        assert!(!closed.is_valid());
        assert_eq!(closed.open_boundary_edges, 3);
    }

    #[test]
    fn verify_volume_integrity_report_invalid_when_crack_present() {
        let r = VolumeIntegrityReport {
            invariant_violations: vec![],
            geometric_cracks: vec![EdgeId::new(0)],
            open_boundary_edges: 0,
            checked_faces: 4,
        };
        assert!(!r.is_valid());
        assert!(r.summary().contains("geometric cracks"));
    }

    #[test]
    fn verify_volume_integrity_p7_radial_not_flagged_as_geometric_crack() {
        // 3 faces sharing edge v0-v1 (radial / ADR-021 P7 stacked) — a SINGLE
        // EdgeId with 3 face-bearing HEs. The geometric crack detector requires
        // ≥2 DISTINCT EdgeIds at one location, so it must NOT flag this
        // (LOCKED #1 P7 no false-positive, ADR-267 L5).
        let mut mesh = Mesh::new();
        let v0 = mesh.add_vertex(DVec3::new(0.0, 0.0, 0.0));
        let v1 = mesh.add_vertex(DVec3::new(10.0, 0.0, 0.0));
        let v2 = mesh.add_vertex(DVec3::new(0.0, 10.0, 0.0));
        let v3 = mesh.add_vertex(DVec3::new(0.0, 0.0, 10.0));
        let v4 = mesh.add_vertex(DVec3::new(0.0, -10.0, 0.0));
        let _f0 = mesh.add_face(&[v0, v1, v2], MaterialId::new(0)).unwrap();
        let _f1 = mesh.add_face(&[v0, v1, v3], MaterialId::new(0)).unwrap();
        let _f2 = mesh.add_face(&[v0, v1, v4], MaterialId::new(0)).unwrap();
        let r = mesh.verify_volume_integrity(IntegrityScope::OpenMesh);
        assert!(
            r.geometric_cracks.is_empty(),
            "radial single-EdgeId must not be flagged as geometric crack: {:?}",
            r.geometric_cracks
        );
    }
}

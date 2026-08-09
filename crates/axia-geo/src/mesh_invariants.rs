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
    /// Is this loop a single self-loop edge carrying an analytic curve — the
    /// ADR-089 Path B boundary shape?
    ///
    /// Such an edge is `twin == self`, so the faces on both sides of it share
    /// one half-edge and `he.face()` can only name one. Any check that asks
    /// "does this half-edge belong to this face?" is therefore inapplicable
    /// here, and says nothing about whether the mesh is damaged.
    fn loop_is_self_loop_curve(&self, start: HeId) -> bool {
        self.collect_loop_hes(start)
            .map(|hes| {
                hes.len() == 1
                    && self
                        .edges
                        .get(self.hes[hes[0]].edge())
                        .filter(|e| e.is_active())
                        .and_then(|e| e.curve())
                        .is_some()
            })
            .unwrap_or(false)
    }

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
            // Closed-curve exemption: outer loop = 1 vert + 1 self-loop edge
            // with `Edge.curve.is_some()`. It exempts I2 — and only I2.
            //
            // This used to `continue`, which dropped the face out of I6, I3 and
            // I4 as well. The comment justified skipping I2 ("the curve's
            // analytic normal is the truth source") and the *outer* half of I4
            // ("single HE already wired in add_face_closed_curve"), and said
            // nothing about the rest. So every Path B primitive — every sphere,
            // cylinder, cone, torus and drawn circle, i.e. the production
            // default — was outside the verifier entirely: repoint one of a
            // ring's inner half-edges at another face and `verify_face_invariants`
            // still returned `valid=true, violations=[]`, with
            // `collect_non_manifold_edges` and `detect_self_intersections` blind
            // to it too. That is ADR-306's asymmetry one face class over, and
            // "correct when we build it" is not a reason for a verifier to look
            // away.
            // The second shape of analytic boundary: a SEAM. Two half-edges,
            // twins of one edge, walked out and back, so the face is bounded
            // without a closed curve being invented for it. That is how a sphere
            // is written when its definition is the AXIS — the poles are the two
            // vertices and the meridian between them is the seam (IFC/STEP's
            // `IfcSeamCurve` + `IfcVertexLoop`).
            //
            // Gated on the face carrying an analytic surface. Without one, two
            // vertices and one edge enclose nothing — that is a degenerate face
            // and I1 should still say so.
            let is_seam = outer_verts.len() == 2
                && self.faces.get(fid).and_then(|f| f.surface()).is_some()
                && self
                    .collect_loop_hes(outer_start)
                    .map(|hes| {
                        hes.len() == 2 && self.hes[hes[0]].edge() == self.hes[hes[1]].edge()
                    })
                    .unwrap_or(false);

            let mut is_closed_curve = false;
            if outer_verts.len() < 3 {
                is_closed_curve = outer_verts.len() == 1
                    && self.collect_loop_hes(outer_start).map(|hes| {
                        hes.len() == 1 && {
                            let he = &self.hes[hes[0]];
                            self.edges.get(he.edge())
                                .filter(|e| e.is_active())
                                .and_then(|e| e.curve())
                                .is_some()
                        }
                    }).unwrap_or(false);
                if !is_closed_curve && !is_seam {
                    violations.push(format!("face {:?}: outer loop has {} verts (< 3)",
                        fid, outer_verts.len()));
                    continue;
                }
                // Fall through: I6, I3 and I4 all still apply.
            }

            // I6 (ADR-304): a normal that is not finite is not a normal.
            //
            // NaN slipped past every check below, because `NaN > 1e-10` is
            // FALSE — so I2 was skipped entirely and the face was reported
            // valid. It is the same NaN-comparison trap that makes
            // `compute_normal`'s degenerate bail unreachable: with
            // NORMAL_EPSILON = 0.0, `len < 0.0` is never true, so a zero-length
            // Newell normal falls through to `Ok(normal / 0.0)` = NaN and a
            // degenerate face enters the mesh carrying it.
            //
            // Creation stays permissive on purpose (tolerances.rs: "keep at 0
            // to avoid missing thin faces"). What was missing is DETECTION —
            // and every gate in this codebase (ADR-267 integrity, ADR-272
            // closure, ADR-302/303 sims) asks this verifier. Zero-length is NOT
            // flagged: the check below already treats it as "no cached normal,
            // skip", and that is load-bearing.
            let cached = face.normal();
            if !cached.is_finite() {
                violations.push(format!(
                    "face {:?}: normal is not finite ({:?}) — degenerate geometry",
                    fid, cached
                ));
                continue;
            }
            // I2: cached normal이 실제 winding과 일치 (반대 방향이면 위반).
            // `!is_closed_curve` — a 1-vertex loop has no winding to compute,
            // and the curve's analytic normal is the truth source. This is the
            // one check the closed-curve exemption actually buys. A seam loop is
            // the same case: two points on an axis have no winding either, and
            // the surface is what says which way the face faces.
            if !is_closed_curve && !is_seam && cached.length_squared() > 1e-10 {
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

            // I4: 이 face 의 모든 half-edge — outer AND inner — 가 이 face 를
            // 가리켜야 함.
            //
            // ADR-306: inner 쪽이 빠져 있었다. I3 는 inner loop 을 *걷지만*
            // (null start / collect 실패 / < 3 verts) 그 half-edge 들이 어느
            // face 에 속하는지는 보지 않았고, I4 는 outer 만 봤다. 그래서 hole
            // 경계의 HE 가 다른 face 를 가리키도록 손상돼도 검증기는
            // `valid=true, violations=[]` 를 반환했다 — 측정 확인. 같은 손상을
            // `collect_non_manifold_edges` 도 `detect_self_intersections` 도
            // 잡지 못한다(둘 다 0). 즉 어떤 게이트에도 보이지 않았다.
            //
            // ADR-298 L-298-5 는 이를 "verify_face_invariants 가 inner loop 을
            // 걷지 않는다" 로 적었는데 부정확하다 — I3 는 baseline(155e127,
            // ADR-298 이전)부터 걷고 있었다. 실제 구멍은 이 **비대칭** 이다.
            // I4 is skipped per LOOP, not per face, and only for a self-loop
            // boundary. A self-loop edge is `twin == self` in the Path B
            // representation (ADR-192 §3.2 records that as deliberate), so the
            // faces on both sides of such a boundary reference the SAME
            // half-edge and it can only name one of them — I4 would flag the
            // other every time. Measured both ways: a freshly drawn Path B
            // sphere produced three "outer HE points to wrong face"
            // violations, and extruding a ring produced "inner[0] HE points to
            // wrong face", each of which made the ADR-267 gate roll back a
            // perfectly good operation.
            //
            // Everything else still runs on a closed-curve face — I6, I3, and
            // I4 on any POLYGONAL loop it may carry. Those are the checks the
            // ADR-089 exemption never justified.
            let loop_starts = std::iter::once(outer_start)
                .chain(face.inners().iter().map(|inner| inner.start))
                .enumerate();
            for (li, start) in loop_starts {
                if start.is_null() { continue; }
                if self.loop_is_self_loop_curve(start) {
                    continue;
                }
                if start.is_null() { continue; }   // I1/I3 가 이미 보고함
                let Ok(hes) = self.collect_loop_hes(start) else { continue; };
                for he in hes {
                    let he_face = self.hes[he].face();
                    if he_face != fid {
                        let which = if li == 0 {
                            "outer".to_string()
                        } else {
                            format!("inner[{}]", li - 1)
                        };
                        violations.push(format!(
                            "face {:?}: {} HE {:?} points to wrong face {:?}",
                            fid, which, he, he_face,
                        ));
                    }
                }
            }
        }

        // I5: an edge may carry more than two faces — but not two that cover the
        //     same ground.
        //
        // 사용자 결정 2026-08-06. Three faces on one edge is what a sheet
        // hanging off a solid looks like: the wall, the cap, and the plate —
        // the solid is still closed, there is simply something attached to it.
        // SketchUp builds exactly that, and refusing to call it valid is our
        // checker's definition, not a law of geometry. Measured the same day:
        // a rectangle drawn across a box's rim produced 8 faces and ONE
        // violation, the rim edge with its three faces, and that alone dropped
        // `is_closed_solid` to false although no boundary edge had opened.
        //
        // What is genuinely wrong is two faces covering the same ground —
        // which is NOT the same question as whether two of them are coplanar,
        // because an edge dividing one plane into two regions carries two
        // coplanar faces by construction. `edge_stacked_face_pair` holds that
        // judgement (side of the edge, not plane alone) for this check and for
        // `face_set_manifold_info` both.
        for (eid, edge) in self.edges.iter() {
            if !edge.is_active() { continue; }
            if let Some((a, b)) = self.edge_stacked_face_pair(eid) {
                let (faces, _) = self.get_faces_sharing_edge(eid);
                violations.push(format!(
                    "edge {:?}: shared by {} active faces (non-manifold) — {:?} / {:?} cover the same ground (stacked)",
                    eid, faces.len(), a, b,
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
mod seam_boundary_tests {
    use crate::mesh::Mesh;
    use crate::surfaces::AnalyticSurface;
    use crate::{MaterialId, VertId};
    use glam::DVec3;

    /// Two vertices and one edge, walked out and back. With a surface that is a
    /// seam-bounded face; without one it encloses nothing.
    fn seam_face(with_surface: bool) -> Mesh {
        let mut m = Mesh::new();
        m.create_sphere_axis_native(DVec3::ZERO, 50.0, DVec3::Z, MaterialId::new(0)).unwrap();
        if !with_surface {
            let fid = m.faces.iter().find(|(_, f)| f.is_active()).map(|(f, _)| f).unwrap();
            m.set_face_surface(fid, None);
        }
        m
    }

    #[test]
    fn a_seam_with_a_surface_is_a_boundary() {
        let m = seam_face(true);
        let r = m.verify_face_invariants();
        assert!(r.is_valid(), "a seam bounds the face: {:?}", r.violations);
    }

    /// The gate on the exemption. Two points and one edge with no surface behind
    /// them enclose nothing, and I1 must keep saying so — otherwise the
    /// exemption would wave through a genuinely degenerate face.
    #[test]
    fn a_seam_without_a_surface_is_still_degenerate() {
        let m = seam_face(false);
        let r = m.verify_face_invariants();
        assert!(
            r.violations.iter().any(|v| v.contains("< 3")),
            "no surface ⇒ nothing is enclosed, expected the I1 complaint, got {:?}",
            r.violations
        );
    }

    /// Control: an ordinary polygon face with a surface is unaffected. Without
    /// it the two tests above could both pass on a verifier that had stopped
    /// looking at faces altogether.
    #[test]
    fn an_ordinary_triangle_is_unaffected() {
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let a: VertId = m.add_vertex(DVec3::ZERO);
        let b = m.add_vertex(DVec3::new(100.0, 0.0, 0.0));
        let c = m.add_vertex(DVec3::new(0.0, 100.0, 0.0));
        let f = m.add_face(&[a, b, c], mat).unwrap();
        // give it a surface so only the vertex count differs from the seam case
        m.set_face_surface(f, Some(AnalyticSurface::Plane {
            origin: DVec3::ZERO,
            normal: DVec3::Z,
            basis_u: DVec3::X,
            u_range: (0.0, 100.0),
            v_range: (0.0, 100.0),
        }));
        assert!(m.verify_face_invariants().is_valid(), "a triangle is fine");
    }
}

#[cfg(test)]
mod edge_occupancy_tests {
    use crate::mesh::Mesh;
    use crate::{FaceId, MaterialId, VertId};
    use glam::DVec3;

    /// Three faces on one edge. The two flat ones are given by their far
    /// vertex: on opposite sides of the edge they are two halves of one plane,
    /// on the same side they cover the same ground. The third stands upright,
    /// so the count is always three and only the plane occupancy differs.
    fn three_on_one_edge(far_a: (f64, f64), far_b: (f64, f64)) -> (Mesh, FaceId, FaceId) {
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let mut v = |x: f64, y: f64, z: f64| -> VertId { m.add_vertex(DVec3::new(x, y, z)) };
        let (o, e) = (v(0.0, 0.0, 0.0), v(100.0, 0.0, 0.0));
        let a = v(far_a.0, far_a.1, 0.0);
        let b = v(far_b.0, far_b.1, 0.0);
        let up = v(0.0, 0.0, 100.0);
        let fa = m.add_face(&[o, e, a], mat).unwrap();
        let fb = m.add_face(&[o, e, b], mat).unwrap();
        m.add_face(&[o, e, up], mat).unwrap();
        (m, fa, fb)
    }

    fn shared_edge(m: &Mesh) -> crate::EdgeId {
        m.edges
            .iter()
            .find(|(eid, e)| e.is_active() && m.get_faces_sharing_edge(*eid).0.len() >= 3)
            .map(|(eid, _)| eid)
            .expect("the fixture puts three faces on one edge")
    }

    /// Two halves of one plane meeting at their border, plus a wall. This is a
    /// rectangle drawn across a solid's rim, and it is not damage.
    #[test]
    fn faces_on_opposite_sides_of_an_edge_are_adjacent() {
        let (m, _, _) = three_on_one_edge((0.0, 100.0), (0.0, -100.0));
        let e = shared_edge(&m);
        assert_eq!(m.get_faces_sharing_edge(e).0.len(), 3, "fixture: three faces");
        assert_eq!(
            m.edge_stacked_face_pair(e),
            None,
            "opposite sides of the edge cover different ground"
        );
        let r = m.verify_face_invariants();
        assert!(
            !r.violations.iter().any(|v| v.contains("stacked")),
            "no stacking to report: {:?}",
            r.violations
        );
    }

    /// The same three faces, but the two flat ones reach out the same way.
    #[test]
    fn faces_on_the_same_side_of_an_edge_are_stacked() {
        let (m, fa, fb) = three_on_one_edge((0.0, 100.0), (100.0, 100.0));
        let e = shared_edge(&m);
        let pair = m.edge_stacked_face_pair(e).expect("same side is stacked");
        assert!(
            (pair == (fa, fb)) || (pair == (fb, fa)),
            "the two flat faces are the stacked ones, got {pair:?}"
        );
        let r = m.verify_face_invariants();
        assert!(
            r.violations.iter().any(|v| v.contains("stacked")),
            "stacking must be reported: {:?}",
            r.violations
        );
    }

    /// Two faces on an edge is the ordinary case and never stacked, whichever
    /// way they lie — the check must not start reading pairs of two.
    #[test]
    fn two_faces_on_an_edge_are_never_stacked() {
        let mut m = Mesh::new();
        let mat = MaterialId::new(0);
        let o = m.add_vertex(DVec3::ZERO);
        let e = m.add_vertex(DVec3::new(100.0, 0.0, 0.0));
        let a = m.add_vertex(DVec3::new(0.0, 100.0, 0.0));
        let b = m.add_vertex(DVec3::new(100.0, 100.0, 0.0));
        m.add_face(&[o, e, a], mat).unwrap();
        m.add_face(&[o, e, b], mat).unwrap();
        let eid = m
            .edges
            .iter()
            .find(|(eid, ed)| ed.is_active() && m.get_faces_sharing_edge(*eid).0.len() == 2)
            .map(|(eid, _)| eid)
            .unwrap();
        assert_eq!(m.edge_stacked_face_pair(eid), None);
    }
}

#[cfg(test)]
mod adr306_tests {
    use crate::entities::*;
    use crate::mesh::Mesh;
    use glam::DVec3;

    /// A 200×200 quad with a 60×60 hole, plus a second face to point at.
    fn quad_with_hole() -> (Mesh, FaceId, FaceId) {
        let mut m = Mesh::new();
        let o: Vec<VertId> = [(0.0, 0.0), (200.0, 0.0), (200.0, 200.0), (0.0, 200.0)]
            .iter()
            .map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0)))
            .collect();
        let h: Vec<VertId> = [(70.0, 70.0), (70.0, 130.0), (130.0, 130.0), (130.0, 70.0)]
            .iter()
            .map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0)))
            .collect();
        let f = m.add_face_with_holes(&o, &[&h], MaterialId::new(0)).expect("quad with hole");
        let o2: Vec<VertId> = [(300.0, 0.0), (400.0, 0.0), (400.0, 100.0), (300.0, 100.0)]
            .iter()
            .map(|&(x, y)| m.add_vertex(DVec3::new(x, y, 0.0)))
            .collect();
        let other = m.add_face_with_holes(&o2, &[], MaterialId::new(0)).expect("second face");
        (m, f, other)
    }

    /// I4 used to check the outer loop only. An inner half-edge repointed at
    /// another face was invisible to `verify_face_invariants` AND to
    /// `collect_non_manifold_edges` AND to `detect_self_intersections` — every
    /// gate in the codebase asks one of those, so nothing could see it.
    #[test]
    fn adr306_inner_half_edge_pointing_at_another_face_is_flagged() {
        let (m, f, _) = quad_with_hole();
        assert!(m.verify_face_invariants().is_valid(), "baseline must be clean");
        assert_eq!(m.faces[f].inners().len(), 1, "fixture must actually have a hole");

        // Outer — was already caught; kept as the symmetry half of the claim.
        let (mut m, f, other) = quad_with_hole();
        let outer_start = m.faces[f].outer().start;
        let he = m.collect_loop_hes(outer_start).expect("outer hes")[0];
        m.hes[he].set_face(other);
        let r = m.verify_face_invariants();
        assert!(!r.is_valid(), "a repointed OUTER half-edge must be flagged");
        assert!(
            r.violations.iter().any(|v| v.contains("outer HE")),
            "expected an 'outer HE' violation, got {:?}", r.violations
        );

        // Inner — the gap ADR-306 closes.
        let (mut m, f, other) = quad_with_hole();
        let inner_start = m.faces[f].inners()[0].start;
        let he = m.collect_loop_hes(inner_start).expect("inner hes")[0];
        m.hes[he].set_face(other);
        let r = m.verify_face_invariants();
        assert!(
            !r.is_valid(),
            "a repointed INNER half-edge must be flagged. Nothing else sees it: \
             collect_non_manifold_edges and detect_self_intersections both report 0 \
             on this exact mesh, so if this check goes, the corruption is invisible."
        );
        assert!(
            r.violations.iter().any(|v| v.contains("inner[0] HE")),
            "expected an 'inner[0] HE' violation, got {:?}", r.violations
        );
    }

    /// The measurement that makes the test above load-bearing rather than
    /// decorative: on the corrupted mesh, the two other gates see nothing.
    #[test]
    fn adr306_no_other_gate_sees_a_repointed_inner_half_edge() {
        let (mut m, f, other) = quad_with_hole();
        let inner_start = m.faces[f].inners()[0].start;
        let he = m.collect_loop_hes(inner_start).expect("inner hes")[0];
        m.hes[he].set_face(other);
        assert_eq!(
            m.collect_non_manifold_edges().len(), 0,
            "if this ever becomes non-zero the finding's premise changed — re-measure"
        );
        assert_eq!(
            m.detect_self_intersections().intersecting_pairs.len(), 0,
            "if this ever becomes non-zero the finding's premise changed — re-measure"
        );
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

/// A closed-curve face is inside the verifier, not outside it.
///
/// Every Path B primitive — sphere, cylinder, cone, torus, drawn circle, i.e.
/// the production default — used to `continue` past I6, I3 and I4, so the
/// damage below was reported as `valid=true, violations=[]`. The exemption is
/// now I2 only.
#[cfg(test)]
mod audit_closed_curve_invariant_tests {
    use super::*;
    use crate::{MaterialId, Mesh};
    use glam::DVec3;

    /// Control first: a healthy Path B sphere must stay clean, or the checks
    /// below prove nothing except that the verifier complains about everything.
    #[test]
    fn a_healthy_path_b_sphere_is_still_valid() {
        let mut mesh = Mesh::new();
        mesh.create_sphere_kernel_native(DVec3::ZERO, 1000.0, MaterialId::new(0)).unwrap();
        let r = mesh.verify_face_invariants();
        assert!(r.is_valid(), "a native sphere must verify clean: {:?}", r.violations);
    }

    /// A closed-curve ring, so there is an INNER loop to damage. Two concentric
    /// Path B circles promoted to an annulus is what the draw path produces
    /// (`assign_circle_holes_innermost`).
    fn ring(mesh: &mut Mesh) -> crate::FaceId {
        use crate::curves::AnalyticCurve;
        let mut disc = |r: f64| {
            let anchor = mesh.add_vertex(DVec3::new(r, 0.0, 0.0));
            let curve = AnalyticCurve::Circle {
                center: DVec3::ZERO,
                radius: r,
                normal: DVec3::Z,
                basis_u: DVec3::X,
            };
            mesh.add_face_closed_curve(anchor, curve, MaterialId::new(0)).unwrap()
        };
        let outer = disc(10.0);
        let inner = disc(5.0);
        crate::operations::annulus::promote_circles_to_annulus(mesh, outer, inner)
            .expect("two concentric circles promote to a ring");
        outer
    }

    #[test]
    fn a_healthy_closed_curve_ring_is_valid() {
        let mut mesh = Mesh::new();
        let outer = ring(&mut mesh);
        assert_eq!(mesh.faces[outer].inners().len(), 1, "a ring has one hole");
        let r = mesh.verify_face_invariants();
        assert!(r.is_valid(), "a healthy ring must verify clean: {:?}", r.violations);
    }

    /// I4 cannot speak about a self-loop boundary, in either direction — and
    /// this test exists so nobody "fixes" that back.
    ///
    /// A closed-curve edge is twin-of-itself, so the faces on both sides share
    /// one half-edge and it can only name one of them (ADR-192 §3.2 records
    /// that representation as deliberate). Repointing it is therefore not
    /// detectable damage. Measured both ways while narrowing this: a bare
    /// Path B sphere produced three "outer HE points to wrong face", and
    /// extruding a ring produced "inner[0] HE points to wrong face" — each of
    /// which made the ADR-267 gate roll back a healthy operation.
    ///
    /// What the widening does buy on this face class is I6 and I3, covered by
    /// the tests either side of this one.
    #[test]
    fn a_self_loop_boundary_is_exempt_from_i4_in_both_directions() {
        let mut mesh = Mesh::new();
        let outer = ring(&mut mesh);
        let far_anchor = mesh.add_vertex(DVec3::new(100.0, 0.0, 0.0));
        let other = mesh
            .add_face_closed_curve(
                far_anchor,
                crate::curves::AnalyticCurve::Circle {
                    center: DVec3::new(90.0, 0.0, 0.0),
                    radius: 10.0,
                    normal: DVec3::Z,
                    basis_u: DVec3::X,
                },
                MaterialId::new(0),
            )
            .unwrap();

        for start in [mesh.faces[outer].outer().start, mesh.faces[outer].inners()[0].start] {
            let hes = mesh.collect_loop_hes(start).unwrap();
            assert_eq!(hes.len(), 1, "a closed-curve boundary is one half-edge");
            mesh.hes[hes[0]].set_face(other);
        }

        let r = mesh.verify_face_invariants();
        assert!(
            !r.violations.iter().any(|v| v.contains("wrong face")),
            "a shared self-loop HE must not be reported as ownership damage: {:?}",
            r.violations
        );
    }

    #[test]
    fn a_non_finite_normal_on_a_closed_curve_face_is_caught() {
        // I6 (ADR-304). A NaN normal is never right, on any face class.
        let mut mesh = Mesh::new();
        let faces = mesh
            .create_sphere_kernel_native(DVec3::ZERO, 1000.0, MaterialId::new(0))
            .unwrap();
        mesh.faces[faces[0]].set_normal(DVec3::new(f64::NAN, 0.0, 0.0));

        let r = mesh.verify_face_invariants();
        assert!(
            !r.is_valid() && r.violations.iter().any(|v| v.contains("not finite")),
            "a NaN normal must be a violation, got {:?}",
            r.violations
        );
    }
}

# ADR-191 — Push/Pull Phase 1 P1.2: Ring (Multi-Loop) Face Push

> ADR-190 로드맵 Phase 1 의 P1.2. **ADR-016 Q2** (multi-loop face Push/Pull
> 거부) 를 **Push/Pull entry 한정 해제** — ring/annulus → manifold tube.
> Boolean / Offset / fillet 의 Q2 reject 는 **불변 유지**.

- **Status**: Accepted
- **Date**: 2026-06-09
- **Track**: 6 (boundary kernel / 유도면) + W (ADR-079 create_solid)
- **Amends**: ADR-016 Q2 (Push/Pull 한정 relax)

---

## Canonical anchor (사용자 결재, 2026-06-09)

> ADR-190 Phase 1 "ring 면 push" 진입 전 "먼저 시뮬레이션으로 문제점검토" 요청.
> 결재 — **P1.2-a Q2 gate 해제 (Push/Pull 한정)** + **P1.2-b (b) disk 자동 제거
> → 관통 hole**.

---

## 1. 시뮬레이션 — 문제 규명 (실측)

ADR-016 Q2 gate 를 **임시로** 풀어 ring push 를 시뮬레이션 (후 revert):

| 시나리오 | 결과 | 결론 |
|---|---|---|
| **빈 hole ring** (rect + punchHole, disk 없음) | push → 1→54면, **manifold valid 0 violations** | push_pull Phase F 는 ring→tube 를 **완벽 처리** |
| **유도면 annulus+disk** (rect-in-rect) | push → 2→11면, **non-manifold 4 violations** | 동반 disk 가 hole 경계 공유 → 3-way |
| ring 면 `hasSurface: false` | (disk 는 true) | Phase 0 containment 경로 gap (별개) |

**핵심**: push_pull 은 ring→tube 능력이 *있으나* (Phase F), 유도면 containment 가
hole 을 채우는 disk 를 *항상 동반 생성* → annulus push 시 inner wall 이 disk 와
3-way non-manifold. "ring push → tube" 는 **hole 이 비어 있을 때만** manifold.

---

## 2. Solution — P1.2-a + P1.2-b

### P1.2-a — Q2 해제 + push_pull 라우팅
- `crates/axia-wasm/src/lib.rs` — `create_solid_extrude` 의 ADR-016 Q2
  multi-loop reject 제거 (Push/Pull entry 한정).
- `crates/axia-core/src/scene.rs` `exec_create_solid` — multi-loop face
  (`!f.inners().is_empty()`) → legacy `push_pull` 라우팅. `create_solid`
  (W track) 은 single-loop profile 전용; push_pull Phase F 가 hole→tube.

### P1.2-b — hole-filler disk 자동 제거
- `crates/axia-geo/src/mesh.rs` — `Mesh::remove_hole_filler_faces(face_id)
  -> usize`: annulus 의 각 inner-loop edge 의 *반대편* active coplanar face
  (`|normal·normal| > 0.999`) = hole 을 채우는 disk → 제거. perpendicular
  wall (3D solid 일부) 은 미접촉. 빈 hole (twin HE face 없음) 은 no-op.
- 제거 후 hole 이 *진짜 관통* → push_pull → manifold through-hole tube.

### Transaction 정합
- `exec_push_pull` 를 transaction-aware 화 (`own_transaction =
  !is_recording()`). multi-loop 라우팅이 `begin → before_snapshot → disk
  제거 → exec_push_pull (nested, no own tx) → commit` 으로 **single Undo
  step** (정확한 pre-op snapshot). standalone caller (P0.2 fallback 포함, cancel
  후 not recording) 은 무영향.

---

## 3. Lock-ins

- **L-191-1** ADR-016 Q2 relax 는 **Push/Pull entry 한정** — Boolean / Offset /
  hole-boundary fillet 의 multi-loop reject 는 **불변 유지** (LOCKED #1 정합).
- **L-191-2** Multi-loop → push_pull 라우팅 (create_solid W track 미사용).
- **L-191-3** P1.2-b disk 제거 = coplanar filler 만 (perpendicular wall 보호).
  빈 hole 은 no-op (true-hole ring 보존).
- **L-191-4** `exec_push_pull` transaction-aware — nested 시 caller 가 tx 관리,
  standalone 시 자체 관리 (회귀 0).
- **L-191-5** Single-loop face 무영향 (`is_multi_loop` gate) — control rect →
  box 회귀 0.
- **L-191-6** ADR-046 P31 #4 additive (createSolidExtrude signature 무변경).
- **L-191-7** 절대 #[ignore] 금지.

---

## 4. Acceptance Log

### 4.1 시뮬레이션 + 결재 (2026-06-09)
- Q2 gate 임시 relax → ring push 시뮬 → 빈 hole = manifold / annulus+disk =
  non-manifold (§1) → revert.
- 결재: P1.2-a + P1.2-b (b) disk 자동 제거.

### 4.2 구현 — commit `9614860` (LOCAL, adr-186/boundary-kernel-port)
- `mesh.rs` +61 (`remove_hole_filler_faces`).
- `scene.rs` +149 (multi-loop 라우팅 + exec_push_pull tx-aware + 회귀 2).
- `lib.rs` Q2 reject 제거.
- 회귀: `adr191_p12_rect_annulus_push_to_manifold_tube` (annulus+disk → manifold
  tube) + `adr191_p12_remove_hole_filler_noop_on_empty_hole` (빈 hole no-op).
- 워크스페이스: axia-core 340 / axia-geo 1694 / foreign 138 / transaction 4 /
  wasm 8 — **2184 PASS, 0 failed, 0 ignored**.

### 4.3 브라우저 검증 (clean scene, ADR-087 K-ζ)
| 면 | 이전 | 이후 |
|---|---|---|
| 유도면 annulus+disk | Q2 거부 (silent) | disk 자동제거 → 10면 tube, manifold valid ✅ |
| 빈-hole ring (punchHole) | Q2 거부 | 54면 tube, manifold valid ✅ |
| control rect (single-loop) | box | box, manifold (회귀 0) ✅ |

---

## 5. Out of scope (Phase 1 잔존 / future)

- **P1.1** Plane+Mixed native (현재 fallback) — 별도.
- **P1.3** Closed-curve Path B 비-Circle (Arc/Bezier/BSpline/NURBS) — 별도.
- ring surface gap (annulus `hasSurface: false`) — Phase 0 후속 (push_pull 은
  surface-agnostic 이라 ring push 비차단, 정합성 보강은 별도).
- Undo 의 완전 round-trip (disk 복원) — transaction-aware 로 single step
  확보, snapshot 정합 검증은 별도 E2E.

---

## 6. Cross-link

- **ADR-190** Push/Pull roadmap (Phase 1 모체) + **LOCKED #78**
- **ADR-016 Q2** multi-loop face 도구 정책 (Push/Pull 한정 amend — **LOCKED #1**)
- **ADR-079** L3 (surface) / Q3 (push_pull fallback) — push_pull Phase F
- **ADR-186** 유도면 re-derive (annulus + disk containment source)
- **ADR-102** Detach-on-Arrangement (P0.2-c cleave 인접)
- **ADR-105 / Window·Hole 도구** punchHole (true-hole ring source)
- **ADR-087** K-ζ 사용자 시연 게이트 / **메타-원칙 #4/#5/#6**
- commit `9614860`

---

## 7. Lessons

- **L1 — 시뮬레이션이 진짜 문제를 분리** — Q2 임시 relax 로 "빈 hole = OK /
  annulus+disk = non-manifold" 를 실측 분리. 동반 disk 가 유일 blocker 임을 확정 →
  fix 가 정확 (disk 제거).
- **L2 — 유도면 containment 의 disk 동반 생성** — annulus 는 항상 disk 와 쌍.
  ring 의 "관통" 의미를 살리려면 disk 처리 필수 (semantic 결재).
- **L3 — transaction-aware 패턴** — `own_transaction = !is_recording()` 로 caller
  가 pre-step 을 같은 Undo step 에 묶을 수 있게. ADR-019 A6 exec_draw_line 답습.
- **L4 — 기존 자산 재사용** — push_pull Phase F (hole 처리) 가 이미 존재. P1.2 는
  *gate 해제 + disk 제거* 만으로 활성 (신규 extrude 코드 0).

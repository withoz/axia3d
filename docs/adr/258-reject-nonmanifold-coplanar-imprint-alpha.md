# ADR-258 — α spec: Reject Non-Manifold Coplanar Imprint (fail-closed guard)

- **Status**: Accepted (α~γ closure 2026-06-25 — engine guard + Toast + real Chromium E2E PASS; §10 Acceptance Log)
- **Date**: 2026-06-25
- **Track**: 6 (Extrude/Cut/Punch) — imprint/면분할 robustness
- **Author**: WYKO + Claude (de-risk workflow + empirical Rust/browser probe)

## 1. Context — 사용자 시연 trigger (2026-06-25)

사용자가 큰 박스 면에 작은 박스를 돌출시킨 뒤 **주황색 선**(ADR-047 R1
비-manifold edge overlay, `0xe85d3a`)을 발견하고 "면이 이상할 때 생기는
선" 으로 정확히 직관. 진단 워크플로우(`wf_838d708e` orange-edge +
`wf_8a5874f5` root-cause) + Rust/browser 실증으로 근본 원인 확정.

## 2. 확정 root cause (empirical)

| 케이스 | 측정 (dev 서버 + Rust repro) |
|---|---|
| 깨끗한 box | 6면, `nonManifoldEdgeCount=0`, closed ✅ |
| **완전 포함(contained) rect** (30×30 면 안쪽) | 7면, `nm=0`, manifold **valid** ✅ — 안전 |
| **부분 겹침(partial overlap) rect** (면 경계 가로지름, center (20,20,50) 100×100) | 8면, **`nm=2`, INVALID (2 violations), 비-closed** ❌ — 비-manifold edge 가 박스 top 경계(z=50, x=50)에 생성 |

**메커니즘**: solid 면에 그린 coplanar 도형이 **그 면의 경계를 건드리거나
가로지르면**, cut 교차점에서 edge 가 3면 공유(면 remainder + 측벽 + 새
sub-face) → 비-manifold + 열린 solid. **면 안쪽에 완전히 들어간 경우는
안전**(manifold). 사전 문서: `repro_rederive_on_solid_top_face_no_panic`
(scene.rs:20079-20083) NOTE 가 "partially overlapping coplanar rect →
non-manifold edge (pre-existing limitation, out of scope)" 명시 — 패닉만
고쳐졌고 비-manifold 자체는 미수정. legacy 경로(`face_rederive_on_draw=
false`)는 더 심함(8-way edge). ADR-102 cleave 는 source-outer-loop-only
(L-102-1)라 cross-boundary 3-way 를 복구 못 함.

## 3. Decision (사용자 결재 2026-06-25)

**옵션 (A) Reject + Toast (fail-closed)** 채택. solid 면 imprint 가
비-manifold 를 만들면 **수행 거부 + rollback + 안내 Toast**. silent
corruption 차단. 대안(B clip / C cleave-extension)은 future ADR.

## 4. Wall 도구 de-risk (결재 옵션 명시 요구)

`DrawWallTool.ts`: `getDrawPlane` 평면에 footprint `drawRectAsShape` →
`createSolidExtrude`. 분석 결과 **reject 안전, carve-out 불필요**:
- **일반 워크플로우(지면 z=0 벽)**: footprint 가 free-floating sheet →
  비-manifold 안 생김 → reject 미발동 ✅
- **"벽 면에 두 번째 벽" cross-boundary**: 현재도 corrupt(비-manifold)
  결과 → reject(명확한 Toast)가 오히려 개선
- DrawWallTool 이 이미 `faces.length===0` graceful 처리 → reject 시 벽
  미생성 + Toast 로 우아하게 degrade
- **완전 포함/flush 가 아닌 contained 벽-on-벽**은 manifold → 통과

## 5. Detection mechanism (β lock 후보 — 우아함의 핵심)

**reject 조건 = "imprint 후 `collect_non_manifold_edges()` 카운트가
증가"** — 이는 **주황 overlay 가 뜨는 바로 그 측정**(ADR-047 R1 의
`getNonManifoldEdgeSegments`/`collect_non_manifold_edges`, ≥3 active
faces). 즉 **reject 가 정확히 "주황이 뜰 imprint"만 거부**.

- 이미 transaction-wrapped 인 coplanar imprint(`exec_draw_*_as_shape` →
  `rebuild_coplanar_faces_analytic_scoped` scene.rs:2382) 의 finalizer 에서:
  1. imprint 전 `collect_non_manifold_edges().len()` 캡처 (`nm_before`)
  2. imprint/rederive 수행
  3. `nm_after` 측정
  4. `nm_after > nm_before` 이면 → **transaction rollback (restore
     before_snapshot)** + rejection 신호 반환
  5. 아니면 → commit
- **self-targeting**: ground footprint(sheet)/free-floating/contained 는
  비-manifold 0 → 미거부. cross-boundary solid-face imprint 만 거부.
- Toast 는 TS-side (Rust 는 rejection 신호 반환 — Boolean reject + Toast
  패턴 mirror).

## 6. Lock-ins

- **L-258-1** fail-closed — imprint 가 비-manifold 도입 시 rollback(거부).
  silent corruption 영구 차단.
- **L-258-2** detection = `collect_non_manifold_edges` 카운트 delta
  (before vs after) — 주황 overlay 와 동일 측정. 휴리스틱 boundary-cross
  predictor 가 아닌 실제 위상 측정 → 모든 비-manifold 원인 robust 포착.
- **L-258-3** contained/ground/free-floating imprint 불변(비-manifold 0
  → 미거부). 회귀: contained rect → 여전히 manifold split.
- **L-258-4** Wall 도구 carve-out 없음 — self-targeting + graceful degrade
  (`faces.length===0`). 지면 벽 불변.
- **L-258-5** Toast TS-side, Rust 는 rejection 신호 (Boolean reject mirror).
- **L-258-6** transaction rollback 재사용 (이미 wrap 됨) — 별도 snapshot 0.
- **L-258-7** ADR-046 P31 #4 additive — valid imprint 동작 불변, 가드만 추가.
- **L-258-8** 메타-원칙 #6 (Preventive over Curative) + #16 (모호하면 거부/
  명시) + #5 (사용자 편의). LOCKED #41 L1 "check first" 변형 — transaction
  으로 atomic 한 check-after-rollback (cross-boundary 사전 예측보다 robust).
- **L-258-9** 절대 #[ignore] 금지.

## 7. Sub-step plan (Path Z atomic)

| sub-step | 내용 | risk |
|---|---|---|
| **α (본 spec)** | ADR + 결재 + Wall de-risk + detection 설계 | LOW |
| β-1 Rust core | imprint finalizer 에 nm-delta 검사 + transaction rollback + rejection 신호. 회귀(partial-overlap→rejected+mesh unchanged / contained→split / ground→unaffected) | MEDIUM |
| β-2 TS | rejection 신호 → Toast + DrawWallTool graceful 확인. vitest 회귀 | LOW |
| γ E2E + 시연 | real Chromium: partial-overlap→rejected+Toast+mesh clean / contained→works + 사용자 시연 게이트 | MEDIUM |

## 8. Out of scope (future ADR)

- **cross-boundary manifold 처리** (B clip-to-face / C cleave-extension /
  Boolean union for flush walls) — 사용자가 reject 선택으로 defer. flush
  벽-on-벽을 manifold 로 합치려면 별도 ADR.
- 곡면(비평면) face imprint reject (현재 평면 coplanar imprint 한정).

## 9. Cross-link

- ADR-047 R1 (비-manifold overlay — reject 가 동일 측정 사용) / ADR-102
  (cleave source-outer-only 한계 — root cause) / ADR-186
  (`rebuild_coplanar_faces_analytic_scoped` rederive 경로) / ADR-176
  (auto-intersect production ON) / ADR-097 (topology auto-recovery —
  reject 는 recover 대신 fail-closed) / ADR-074 §Boolean reject+Toast
  (TS Toast 패턴 mirror) / `repro_rederive_on_solid_top_face_no_panic`
  (pre-existing limitation 문서)
- 메타-원칙 #5 #6 #16 / LOCKED #41 L1 (check first) / #44 / #42 (ADR-102)
- 진단 워크플로우: `wf_838d708e` (orange-edge) + `wf_8a5874f5` (root-cause)

## 10. Acceptance Log (α~γ closure, 2026-06-25)

Path Z atomic 4 sub-step:

| sub-step | layer | commit | 회귀 |
|---|---|---|---|
| α | spec | `0d2f553` | — |
| β-1 | Rust core (`Scene::guard_imprint` nm-delta + rollback, execute() 8 draw arm) | `25535b0` | axia-core +3 |
| β-2 | WASM `set_error` + TS bridge `surfaceDrawReject` Toast (8 wrapper) | `93b3e94` | vitest +3 |
| γ | E2E (real Chromium + prod build + fresh WASM) | `b796063` | Playwright +2 |

**누적**: axia-core +3 (408), vitest +3 (WasmBridge 304), Playwright +2.
모두 PASS, 절대 #[ignore] 금지 준수.

**검증 매트릭스**:
- Rust 408: partial-overlap → reject + 6면 box 복원 + nm 0 + manifold valid /
  contained → ShapeCreated + manifold / ground → ShapeCreated (self-targeting,
  8종 draw wrap 했으나 기존 draw 테스트 false-reject 0)
- vitest 304: surfaceDrawReject → reject(-1+lastError)→Toast.warning /
  success→no Toast / no-engine→no Toast
- E2E 2/2 (real Chromium): partial→reject(ret -1, 6면 manifold closed) /
  contained→accept(split, manifold)
- dev 서버 probe: reject ret -1 + lastError "도형이 면 경계를 넘어 비-manifold
  (겹친 면)를 만듭니다 — 면 안쪽에 그려주세요" + manifold / contained 수락 —
  사용자 시연 readiness 확인

## 11. Lessons (canonical)

- **L-258-L1 (detection = 주황 overlay 동일 측정)**: reject 조건 =
  `collect_non_manifold_edges` 카운트 증가 = ADR-047 R1 주황 overlay 가 쓰는
  바로 그 측정 → reject 가 정확히 "주황 뜰 draw"만 self-targeting 발동. 휴리
  스틱 boundary-cross predictor 대신 실제 위상 측정 → 모든 비-manifold 원인
  robust 포착, false-reject 0 (8종 draw wrap, 기존 테스트 전부 통과).
- **L-258-L2 (root cause = partial-overlap, NOT contained — 초기 가정 정정)**:
  Rust+browser 실증으로 "큰 박스 면에 작은 rect→extrude→비-manifold" 초기
  가정이 틀림 확인 — **완전 포함(contained)은 manifold-safe** (nm=0), 비-
  manifold 는 **면 경계를 건드림/가로지름(partial-overlap)** 에서만 발생
  (nm=2). diagnostic workflow + empirical probe 가 가정 정정. (메타-원칙 #6
  측정 우선.)
- **L-258-L3 (single wrapper at dispatch SSOT)**: 8종 face-draw 를 개별 함수
  편집 대신 `execute()` dispatch 에서 단일 `guard_imprint` wrapper 로 감싸
  DRY + 모든 경로(circle 의 as-shape/as-curve 분기 포함) 일괄 포착.
- **L-258-L4 (transaction rollback 재사용 — ADR-193 패턴)**: rejected op 의
  rollback 은 `restore_scene_snapshot` + `discard_last_undo` (ADR-193
  speculative-rollback canonical) → undo/redo entry 안 남김. 별도 snapshot
  인프라 0.
- **L-258-L5 (Wall de-risk — carve-out 0)**: reject 가 self-targeting 이라
  지면 벽(free-floating sheet) 미발동, cross-boundary 벽-on-벽(현재도 corrupt)
  은 reject(Toast) 로 개선, DrawWallTool `faces.length===0` graceful. 결재
  옵션의 "Wall 영향 먼저 확인" 충족.

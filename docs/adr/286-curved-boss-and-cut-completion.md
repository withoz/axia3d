# ADR-286 — Curved-Surface Boss (outward) + Cut Completion

- **Status**: Accepted (α spec + β-1~β-5 landed 2026-07-10 — Cylinder curved boss; Sphere/Cone/Torus + live boss = ε future)
- Date: 2026-07-10
- Track: "완벽한 extrude" 로드맵 #5 곡면 Phase 2 (boss). ADR-263 Phase 0
  (곡면 sketch-split) + ADR-271 Phase 1 (Cylinder cut) 위.
- Supersedes/amends: 없음 (additive)
- Cross-link: ADR-263 (곡면 sketch), ADR-271 (Cylinder cut, doc-lag —
  실제 구현+배선됨), ADR-268 (planar face 위 곡선 profile cut), ADR-193
  (Live extrude), ADR-267 (watertight gate), ADR-089 A-χ (surface 상속),
  메타-원칙 #5 #6 #9 #14 #16.

---

## 1. Canonical anchor (사용자 결재, 2026-07-10)

AskUserQuestion "다음 작업" → **곡면 cut/boss** 선택.

## 2. Measure-first 감사 (CODE 진실, ADR-271 doc-lag 확인)

ADR 카탈로그는 ADR-271 을 "α spec only, code 0" 으로 표기하나, 실측 결과
**Cylinder 곡면 cut 은 이미 완전 구현 + 배선**:

| 기능 | 상태 | 위치 |
|---|---|---|
| Cylinder blind pocket (cut) | ✅ 구현+WASM+bridge+PushPullTool | carve.rs:679, scene.rs:7989, lib.rs:8756, PushPullTool.ts:217 |
| Cylinder through-hole | ✅ 구현+Scene auto-route (depth≥radius) | carve.rs:800, scene.rs:7993 |
| 곡면 BOSS (outward) | ❌ **전혀 없음** | — |
| Sphere/Cone/Torus cut | ❌ (ADR-271 ε future) | — |

**PushPullTool 현 동작** (PushPullTool.ts):
- 곡면 cap 안쪽 당기기 (dist<0) → `carveCurvedPocket` → 포켓/관통 ✅
- 곡면 cap 바깥 밀기 (dist>0) → `isCurvedCap` 무시 → **planar box fallback
  (잘못된 형상)** ❌ ← 본 ADR 이 해소할 실제 버그

## 3. 진짜 gap + 본 ADR scope

**본 ADR = Cylinder 곡면 BOSS (outward)** — 기존 pocket 의 깨끗한 mirror.
outward push 를 planar box 가 아닌 radial boss 로 정확 처리.

**Boss 기하 (pocket mirror, carve.rs:679 답습)**:
- 입력: Cylinder cap (곡면 sketch split 결과, kind 2), depth>0
- opening ring 을 **per-vertex radial-OUTWARD** 로 depth 만큼 이동 (r+depth)
- cap 제거 → N side wall quad (outward 향함, welds to remainder) + roof cap
- roof cap 은 Cylinder surface (radius+depth) 상속 → 곡면 렌더 (ADR-263)
- pocket 의 `depth < radius` 제약 **없음** (boss 는 임의 높이)
- 결과 watertight manifold (ADR-267 gate + verify_face_invariants)

## 4. 결재 필요 사항 (Q1~Q5)

- **Q1 (scope)**: (a) Cylinder BOSS 만 (pocket mirror, 실제 버그 해소) —
  **추천**. Sphere/Cone/Torus cut+boss 는 별도 ADR (ε). / (b) 전 곡면.
- **Q2 (boss 기하)**: (a) pocket mirror — per-vertex radial-outward, roof
  r+depth, N side wall, Cylinder surface 상속 — **추천**. / (b) planar
  flat-top cap.
- **Q3 (gesture)**: (a) PushPullTool outward branch (dist>0 on curved
  cap) → `carveCurvedBoss` — 현 planar-box fallback 버그 fix — **추천**.
  / (b) 별도 도구.
- **Q4 (through gesture)**: (a) 현 auto-route (depth≥radius) 유지, 명시
  gesture 미도입 — **추천** (신규 작업 0). / (b) 명시 drill 버튼.
- **Q5 (Live preview)**: (a) commit-only v1 (ADR-193 live 는 boss 후속) —
  **추천**. / (b) live boss preview 즉시.

## 5. Lock-ins (β 구현 강제, 결재 후 확정)

- **L-286-1** Cylinder BOSS = `carve_curved_pocket` mirror (signed radial
  outward), carve.rs 신규 `add_curved_boss(cap, height)`.
- **L-286-2** roof cap Cylinder surface (radius+height) 상속 (ADR-263 A-χ).
- **L-286-3** PushPullTool outward (dist>0 on isCurvedCap) → `carveCurvedBoss`
  (planar box fallback 제거).
- **L-286-4** watertight (ADR-267) + verify_face_invariants + snapshot
  rollback (ADR-190 P0.2).
- **L-286-5** Scene transaction-wrap + owner reconcile (pocket_from_cap 답습).
- **L-286-6** additive (ADR-046 P31 #4) — planar extrude / pocket / through
  무회귀.
- **L-286-7** Cylinder MVP; Sphere/Cone/Torus = ε (별도 ADR).
- **L-286-8** 절대 #[ignore] 금지. 사용자 시연 게이트 (ADR-087 K-ζ) +
  E2E (real Chromium).

## 6. Roadmap (β 결재 후)

- β-1 engine `add_curved_boss` (Cylinder radial outward) + 회귀
- β-2 WASM `carveCurvedBoss` + Scene wrap + owner reconcile
- β-3 bridge + PushPullTool outward dispatch
- β-4 E2E (draw circle on cylinder → push out → boss manifold) + 시연
- β-5 closure docs + LOCKED

## 7. de-risk (β 착수 전 시뮬레이션 예정)

pocket 이 정확히 mirror 가능한지 (radial outward + winding flip + roof
surface) 를 engine-level 시뮬로 검증 후 β 착수. 예상 위험 낮음 (pocket 이
이미 watertight 하게 동작 + 구조 동일).

## D. Acceptance Log (2026-07-10, β-1~β-5 landed)

- **β-1 (de-risk sim + engine)** — `carve.rs` `add_curved_boss(cap, height)`
  = `carve_curved_pocket` mirror (per-vertex radial-OUTWARD, roof at
  radius+height, Cylinder surface 상속, `depth<radius` 제약 제거). de-risk
  finding: winding 은 remainder hole-loop welding 에 의해 강제 →
  `[a,a2,b2,b]` + forward roof 가 pocket 과 동일, 별도 flip 불필요.
  회귀 `adr286_add_curved_boss_cylinder` — watertight manifold +
  is_closed_solid + roof verts at r+height + **roof normal 이 radial-
  OUTWARD** (ADR-268 "topology ≠ orientation" 명시 체크) + 음수 height
  reject. 첫 시도 PASS.
- **β-2 (Scene + WASM)** — `scene.rs` `add_curved_boss_from_cap`
  (transaction-wrap + owner reconcile, pocket_from_cap 미러, through-
  routing 없음); `lib.rs` WASM `carveCurvedBoss` (ADR-267 integrity +
  ADR-273 closure gate, pocket 과 동일 defense-in-depth). export baseline
  additive-safe (baseline ⊆ current).
- **β-3 (bridge + tool)** — `WasmBridge.carveCurvedBoss` (interface +
  method); `PushPullTool` outward branch (`isCurvedCap && dist > 0`) →
  `carveCurvedBoss` — 기존 planar-box fallback 버그 해소. preview 는
  dimension-label only (commit-only v1, Q5). tsc clean, WASM rebuilt,
  binding 확인.
- **β-4 (E2E + 시연)** — `web/e2e/adr-286-curved-boss.spec.ts` 2 tests
  (real Chromium + production build + compiled WASM): sketch circle →
  push out → boss (roof at r+height=15, Cylinder-inherited, manifold
  valid 0 viol, +faces) + 음수 height reject (mesh untouched). 2/2 PASS.
  곡면-track 전체 E2E 13/13 (202/257/263/285/286) 무회귀. **시각
  시연 참고**: dev-preview(port 3000) 에서 eval-driven `syncMesh` 가
  onFrame LOD(`setRenderChordTol`, ADR-135) 와 race → `HeId not found`
  panic. **control 로 shipped pocket 도 동일 panic 확인 → boss 무관,
  선재 dev-preview 재진입 artifact** (별도 task 로 flag). boss geometry
  는 engine test + E2E 로 manifold-valid 증명.
- **β-5 (closure + 재검토 + save)** — 전체 회귀 sweep: cargo workspace
  **3003 passed / 0 failed / 1 ignored** (선재 slow-channel), vitest
  **2520 passed / 1 skipped** (무회귀), 곡면 E2E **13/13**, tsc 0,
  production build ✓, CatalogConsistency **3/3** (menu/action drift 0 —
  additive, PushPullTool 재사용, 신규 menu/toolbar entry 없음). Status
  Accepted + LOCKED + README catalog.

## E. 남은 트랙 (별도 ADR, ε)

- Sphere / Cone / Torus 곡면 boss + cut (현재 Cylinder MVP; carve.rs
  surface match arm 확장 — pocket 과 boss 모두 ε).
- Live boss preview (ADR-193 live extrude 답습 — 현재 commit-only v1).
- Through-hole 명시 gesture (현재 depth≥radius auto-route 유지).
- dev-preview syncMesh × onFrame LOD 재진입 fix (선재, spawned task).

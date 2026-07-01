# ADR-238 — NURBS CP Edit Single-Undo (A2 full-2, re-ordered before live)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS Patch Editor A2 (ADR-235 roadmap full-2 — **re-ordered before full-1 live**)
- **Depends on**: ADR-237 (recreateNurbsPatch SSOT) / ADR-236 (drag) / ADR-234/233 (prompt) /
  ADR-232 (overlay) / ADR-050 P-5e-γ (transaction collapse precedent)

## 1. Context — 상세 시뮬레이션이 진행 방향을 뒤집음

사용자가 full-1 (live surface drag) 를 "상세한 시뮬레이션으로" 요청. 시뮬레이션(real WASM)이
ADR-235 의 핵심 가정을 **반증**:

1. **set_face_surface(thin wrapper) 는 render 를 바꾸지 않음** — NURBS 패치에 sphere surface
   설정 → kind 8→3 바뀜, 하지만 **geometry 불변**(117 verts, bbox 동일). 근거: ADR-038 Step A
   진단 *"tessellate_face_surface 는 존재하나 export_buffers 에 통합 안 됨"* — surface metadata 는
   smooth normal + analytic ops 만 구동, render 위치는 createNurbsSurface 시점에 DCEL 에 baked.
   → **얇은 `setFaceSurfaceNurbs` 래퍼로 live 변형 불가** (ADR-235 D2 무효).
2. **re-create(re-bake) 만 변형** — createNurbsSurface(CP 이동)는 geometry 변경. per-frame
   re-create = **1.34ms/frame**(16ms 예산 내) — live 가능하나 프레임당 createNurbsSurface +
   deleteFace = **N×2 Undo 항목**.

→ 깨끗한 live 는 **Undo 병합(full-2)이 선행 필요**. 사용자 결재 **A** — full-2 먼저, live 후속
(재정렬). 추가 측정: 현재 re-create = **2 Undo step** (undo#1 deleteFace 복원, undo#2
createNurbsSurface 복원).

## 2. Decision — combined `replaceNurbsSurface` WASM (1 transaction)

- **WASM `replaceNurbsSurface(oldFid, ctrl, uc, vc, weights, ku, kv, du, dv) → Vec<u32>`**
  (lib.rs): **한 transaction** — `begin() → set_before_snapshot(pre-edit) →
  mesh.create_nurbs_surface(new) + (mesh-level) remove old face → set_after_snapshot(post-edit) →
  commit()` = **1 frame**. Err → `cancel()` (old face untouched). create_nurbs_surface 의 검증/
  grid build mirror + delete_face 의 mesh-level 제거(`unregister_face_from_xia + remove_face +
  faces.remove`) 를 한 begin/commit 안에 결합. (replace_last_after_snapshot 불필요 — 그건 이미
  commit 된 frame collapse 용; 여긴 두 op 을 처음부터 한 frame 에 기록.)
- **bridge `replaceNurbsSurface`** wrapper + **legacy fallback** (engine 미보유 시
  createNurbsSurface + deleteFace = 2 frame, 동작 보존).
- **recreateNurbsPatch SSOT 전환** — (createNurbsSurface + deleteFace) → 단일 replaceNurbsSurface.
  드래그(236)·클릭 prompt(233/234)·패널(237) **모든 편집이 자동 단일 Undo** (셋 다 SSOT 경유).

## 3. Lock-ins

- **L-238-1** 진행 재정렬 — full-2(단일 Undo) 가 full-1(live) **전에**. 근거: live 의 per-frame
  re-create 가 Undo 병합을 전제. (ADR-235 로드맵 순서 = 가칭, 결재로 조정.)
- **L-238-2** `replaceNurbsSurface` = create new + remove old 를 **한 transaction** (1 Undo
  frame). canonical WASM pattern (begin/before/연산/after/commit), ADR-050 P-5e-γ collapse 와
  동일 정신(여긴 처음부터 1 frame 이라 더 단순).
- **L-238-3** recreateNurbsPatch SSOT 단일 진입점 — 전환 1곳으로 모든 CP 편집 경로 단일 Undo.
- **L-238-4** Legacy fallback (bridge) — engine 미보유 시 2-frame 경로 보존 (graceful).
- **L-238-5** **set_face_surface 는 render 무변경** (canonical 발견) — live 곡면 변형은 re-bake
  뿐. 후속 live(ADR-239)는 per-frame re-create(+드래그 중 Undo 억제→release commit) 또는
  re-bake-in-place 엔진 메서드.
- **L-238-6** export_baseline += replaceNurbsSurface (ADR-232 getNurbsSurfaceParams 선례).
- **L-238-7** ADR-046 P31 #4 additive — 신규 WASM 1개 + bridge + SSOT 전환. tool/UI/catalog 0.
- **L-238-8** 절대 #[ignore] 금지.

## 4. 회귀

- axia-wasm: `replaceNurbsSurface` export (baseline +1, `wasm_export_baseline_unchanged` PASS).
- vitest: NurbsEditTool 11 + NurbsPatchPanel 9 = 20 (mock/assertion 을 createNurbsSurface+deleteFace
  → replaceNurbsSurface 단일 호출로 갱신; net 동일). 2410 유지.
- tsc 0 · WASM 재빌드 정상(SIMD 11532) · 패키지 24/24 · CC 173 불변.

## 5. 후속 (live = ADR-239)

- **live surface drag** — per-frame re-create(1.34ms) + 드래그/슬라이더 중 Undo 억제(no-record
  path) → release 시 단일 commit(replaceNurbsSurface). 또는 re-bake-in-place 엔진 메서드
  (`update_nurbs_patch_geometry`: 같은 uv 샘플 재평가 + 기존 verts 이동, churn 0). 별도 de-risk.
- deleteFace orphan cleanup 검증 / panel↔drag 공존 다듬기.

## 6. Lessons

- **L1** **상세 시뮬레이션의 가치** — "얇은 래퍼" 가정(ADR-235 D2)이 real WASM 측정으로 반증
  (set_face_surface render 무변경). 가정은 코드+런타임 확인으로 재검증 (메타-원칙 #6). 시뮬레이션이
  commit 전에 진행 순서를 뒤집음(full-2 ↔ full-1).
- **L2** SSOT 의 배당 — recreateNurbsPatch(ADR-237) 단일 진입점 덕에 transaction 전환 1곳으로
  모든 편집 경로(drag/prompt/panel) 단일 Undo 달성. SSOT 추출(ADR-237)의 후행 가치.
- **L3** combined transaction > collapse — 두 op 을 처음부터 한 begin/commit 에 넣는 것이
  replace_last_after_snapshot collapse(이미 commit 된 frame 수정)보다 단순/명확. 새 multi-op
  단일 Undo 의 canonical.

## 7. Cross-link

- ADR-235 (A2 roadmap — full-2, 순서 재정렬) / ADR-237 (recreateNurbsPatch SSOT) / ADR-236 (drag) /
  ADR-234/233 (prompt) / ADR-232 (overlay).
- ADR-050 P-5e-γ (transaction collapse precedent) / ADR-038 Step A (tessellate_face_surface export
  미통합 — set_face_surface render 무변경 근거).
- 메타-원칙 #6 (Preventive — 가정 재검증) / ADR-046 P31 #4 (additive) / LOCKED #44 (Complete
  Meaning per Merge).

# ADR-239 — Live NURBS Surface Drag (A2 full-1, ADR-193 session mirror)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS Patch Editor A2 (ADR-235 roadmap full-1 — 마지막 단계)
- **Depends on**: ADR-238 (single-Undo replace) / ADR-236 (drag) / ADR-237 (panel) / ADR-232 (overlay) /
  ADR-193 (Live Push/Pull session pattern) / ADR-050 P-5e-γ (transaction)

## 1. Context — de-risk 가 경로를 확정

ADR-238 de-risk 가 결정적으로 확인: **Face.surface 는 패치 render 를 구동하지 않음**(clearFaceSurface /
setFaceSurfaceSphere 모두 geometry 불변 — patch DCEL = 4-corner quad, render 는 create 시점 baked).
→ live 변형은 **re-create 전용** (per-frame 1.34ms). per-frame re-create 는 N×2 Undo →
ADR-238 단일 Undo (replaceNurbsSurface) 위에, **ADR-193 Live Push/Pull session 패턴** mirror 로
드래그 중 Undo 억제 + 단일 commit. 사용자 결재 **A** (Full live, ADR-193 session mirror).

## 2. Decision — ADR-193 live-session mirror for NURBS

- **Scene session** (`LiveNurbsEditSession { before_snapshot, orig_fid, current_fid }`, transient,
  non-serialized) + 4 메서드 + `is_live_nurbs_edit_active`:
  - `begin_live_nurbs_edit(face_id)`: pre-edit snapshot 캡처 (speculative op 없음).
  - `update_live_nurbs_edit(grid, weights, knots…)`: **transaction 없이** create_nurbs_surface(new) +
    이전 preview 직접 제거 → per-frame live deform. 새 preview FaceId 반환 (caller 가 overlay re-point).
  - `commit_live_nurbs_edit(grid, weights, knots…)`: `restore_scene_snapshot(before)`(orig 복원) →
    ONE begin/commit (create final + remove orig) = **단일 Undo**. (session 중 frame 0 → ADR-193 와
    달리 `discard_last_undo` 불필요.)
  - `cancel_live_nurbs_edit()`: `restore_scene_snapshot(before)`.
- **WASM 5 endpoints** (beginLiveNurbsEdit / updateLiveNurbsEdit / commitLiveNurbsEdit /
  cancelLiveNurbsEdit / isLiveNurbsEditActive) + `nurbs_grid_from_flat` 공유 helper.
- **bridge 5 wrappers** (graceful: 미지원 시 begin → false → 도구가 ADR-236 fallback).
- **NurbsEditTool drag 통합**: onMouseMove 첫 drag frame → `beginLiveNurbsEdit` → 매 frame
  `updateLiveNurbsEdit(editedCtrl)` → faceId re-point + syncMesh + overlay (곡면 live 변형). onMouseUp
  드래그 → `commitLiveNurbsEdit` (단일 Undo) + select; 클릭 → `_promptEdit` (ADR-234). onDeactivate/
  cleanup → live active 면 `cancelLiveNurbsEdit`. **legacy build (begin → false)** → ADR-236
  marker-only + release `_recreate` fallback.

## 3. Lock-ins

- **L-239-1** live = **per-frame re-create** (set_face_surface render 무관 — ADR-238 de-risk).
  ADR-193 session 패턴 mirror (begin snapshot → update no-transaction → commit 1 clean op → cancel restore).
- **L-239-2** commit = restore(before=orig) + ONE replace → **단일 Undo**. session 중 transaction frame
  0 → discard_last_undo 불필요 (ADR-193 는 begin 이 speculative recorded op → discard 필요했음, 차이).
- **L-239-3** faceId churn during drag (update 마다 new) — session current_fid 추적, commit 이 orig 복원
  후 clean replace. 드래그 중 re-select 안 함 (panel churn 회피; overlay 만 직접 갱신).
- **L-239-4** legacy fallback — begin → false (미지원) → 도구가 ADR-236 (marker-only + release recreate).
  bridge graceful.
- **L-239-5** session transient (non-serialized) + cancel on deactivate/cleanup/ESC (no leak).
- **L-239-6** 클릭(prompt, ADR-234) + 패널(ADR-237) 은 live 미사용 (recreateNurbsPatch commit-기반 유지).
  드래그만 live.
- **L-239-7** ADR-046 P31 #4 additive — 5 WASM + bridge + 도구 drag 경로. tool/UI/catalog surface 0.
- **L-239-8** 절대 #[ignore] 금지.

## 4. 회귀

- axia-core: live nurbs session 5 scene 메서드 (cargo check PASS). axia-wasm: 5 WASM endpoints
  (build PASS, SIMD 10727).
- vitest: NurbsEditTool **12** (drag → live begin/update/commit + legacy fallback + axis-lock +
  click-prompt; +1 legacy fallback) + NurbsPatchPanel 9 = 21. 2411 유지(net +1).
- tsc 0 · WASM 재빌드 정상 · 패키지 24/24 · CC 173 · 브라우저 live deform + 단일 Undo 검증.

## 5. 🎉 NURBS A2 트랙 완료

| 단계 | ADR | 내용 |
|---|---|---|
| MVP-1 | 232 | control-net overlay |
| MVP-2 | 233 | CP weight edit (pick+prompt) |
| MVP-3 | 234 | CP position edit (통합 prompt) |
| MVP-4 | 236 | drag-on-release |
| MVP-5 | 237 | inline panel (CP 표) |
| full-2 | 238 | single-Undo replace |
| **full-1** | **239** | **live surface drag** |

panel(정밀) + drag(직관, 라이브) 공존 통합 NURBS 패치 에디터 (ADR-235 vision) 완성.

## 6. 후속 (별도 ADR)

- 패널 슬라이더 live (현재 panel 은 commit-기반; slider input → updateLiveNurbsEdit 연결 가능).
- 드래그 중 panel 동기화 (현재 stale until release — re-select 안 하므로).
- re-bake-in-place (uv 샘플 재평가 + verts 이동) — patch DCEL 가 4-corner quad 라 현재 부적용;
  render 가 surface-param tessellation 으로 전환되면 (ADR-038 Step A 해소) faceId churn 0 가능.
- deleteFace orphan cleanup 검증.

## 7. Lessons

- **L1** ADR-193 session 패턴의 재사용성 — live preview + 단일 Undo 의 canonical (begin snapshot /
  update no-transaction / commit 1 clean op / cancel restore). NURBS 는 "update = vertex translate"
  대신 "update = re-create no-transaction" 로 변형 (set_face_surface 불가, ADR-238). 향후 모든 live
  편집(곡선/패치)의 template.
- **L2** discard_last_undo 필요 여부는 begin 의 speculative op 유무 — ADR-193 begin 은 recorded
  extrude → discard 필요. ADR-239 begin 은 snapshot-only → 불필요. session 설계 시 begin 이 frame 을
  남기는지 확인.
- **L3** 상세 시뮬레이션 누적 (ADR-238 + 239) — 얇은 래퍼 → re-create → session. 각 가정을 real WASM
  으로 검증하며 정확한 경로 확정 (메타-원칙 #6).

## 8. Cross-link

- ADR-235 (A2 roadmap — full-1) / ADR-238 (single-Undo replace, de-risk source) / ADR-236 (drag,
  fallback) / ADR-237 (panel) / ADR-232 (overlay) / ADR-234 (click prompt).
- ADR-193 (Live Push/Pull session pattern — mirror source) / ADR-038 Step A (set_face_surface render
  무관 근거) / ADR-050 P-5e-γ (transaction).
- 메타-원칙 #6 (Preventive — 가정 재검증) / ADR-046 P31 #4 (additive) / LOCKED #44 / LOCKED #81 (ADR-193).

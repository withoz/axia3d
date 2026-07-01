# ADR-241 — Slice 견고화 Phase 1 (C5 폴리곤 Trim — keep one half)

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 1 (Slice 견고화) — sub-step 1 of 3 (C5)
- **Depends on**: ADR-240 (extrude/cut/punch 로드맵) / slice_volume_by_plane (`slice.rs`) /
  ADR-197 β-3-n (cutCurvedByZPlane / trimCurvedByPlane — 곡면 trim 선례)

## 1. Context

ADR-240 Phase 1 의 첫 sub-step. 사용자 결재 **C5 (폴리곤 trim — 한쪽 유지)**. de-risk:
`slice_volume_by_plane` 는 평면 cut 후 **2 독립 closed 볼륨**을 만든다(step 5.5 below 절반을
독립 verts 로 detach + 양면 cap). 따라서 trim = slice + **버리는 절반 제거** → 한 closed 볼륨.
기존 slice 알고리즘 전체 재사용(Pattern-12) — 새 알고리즘 0. 곡면 trim(`trimCurvedByPlane`,
ADR-205)은 이미 있으나 폴리곤 솔리드는 `cutMode != 'slice'` 시 "곡면+수평만 지원" 경고 후
2-볼륨 slice 로 격하됐다 — 본 ADR 이 폴리곤 trim 활성화.

## 2. Decision

- **Engine `Mesh::trim_volume_by_plane(face_ids, plane, keep_above, material) -> Vec<FaceId>`**
  (`slice.rs`): `slice_volume_by_plane` 호출 → `keep_above` 면 above_walls+cap_above 유지 /
  below_walls+cap_below 제거 (else 반대). 버리는 절반 `remove_face` (2 반쪽이 독립이라 manifold
  보존). 유지 절반 face id 반환.
- **Scene `Scene::trim_volume_by_plane(face_ids, plane, keep_above)`**: source XIA 유일성 검증 →
  before snapshot → mesh.trim → **source XIA 를 유지 절반에 재할당**(새 XIA 없음, slice 와 차이) →
  단일 Undo. 에러 시 `restore_scene_snapshot`(drill 패턴, cancel 은 mutation 복원 안 함).
- **WASM `trimVolumeByPlane(faceIds, ox,oy,oz, nx,ny,nz, keep_above) -> {ok, totalFaces}`**.
- **SliceTool**: 폴리곤 경로의 `cutMode != 'slice'` 분기 — "미지원 경고+slice 격하" → **`trimVolumeByPlane`
  호출** (`above` = +normal, `below` = −normal; `trimCurvedByPlane` ADR-205 일관). legacy build
  (endpoint 부재) → 2-볼륨 slice fallback + 경고.

## 3. Lock-ins

- **L-241-1** trim = slice + discard half (slice 알고리즘 전체 재사용, 새 기하 0).
- **L-241-2** MVP scope = slice 와 동일 상속 (convex crossed 면 / hole 없음 / closed volume).
  non-convex(C1) / hole(C2)는 Phase 1 후속 sub-step.
- **L-241-3** `keep_above` = +normal side (SliceTool `above`/`below` ↔ trimCurvedByPlane 일관).
- **L-241-4** Scene: source XIA 재할당(새 XIA 없음). slice 의 below→new XIA 와 차이.
- **L-241-5** 에러 시 restore_scene_snapshot (slice 가 incremental mutation 후 late bail 가능).
- **L-241-6** legacy fallback (SliceTool) — endpoint 부재 시 2-볼륨 slice + 경고.
- **L-241-7** 메타-원칙 #16 명시 trigger (SliceTool 명시 cutMode). ADR-046 P31 #4 additive.
- **L-241-8** 절대 #[ignore] 금지.

## 4. 회귀

- axia-geo `slice_volume` +2 (`trim_cube_keep_above_leaves_upper_closed_half` /
  `trim_cube_keep_below_leaves_lower_closed_half` — kept=6 face closed solid + 버린 절반 제거 +
  ADR-007 invariants) → 10 PASS.
- axia-wasm: `trimVolumeByPlane` export (SIMD 11287).
- vitest SliceTool +3 (above keepAbove=true / below keepAbove=false / legacy fallback) → 6 PASS.
- tsc 0 · WASM 재빌드 정상.

## 5. 후속 (Phase 1 나머지)

- **C1** non-convex 면 slice (>2 On verts boundary-walk 페어링 + 면당 다중 split_face). ADR-242?
- **C2** 구멍 있는 솔리드 slice (hole loop above/below/straddle). ADR-243?
- (이후 Phase 2 Punch 확장 / Phase 3 Extrude 완성 — ADR-240 로드맵).

## 6. Lessons

- **L1** slice 의 "2 독립 볼륨" 산물이 trim 을 거의 공짜로 만듦 — 기존 자산의 후행 가치(Pattern-12
  확장변형). de-risk 가 이를 확인 (메타-원칙 #6).
- **L2** keep-one-side 의 XIA 의미론 = slice 와 다름(새 XIA 없이 source 재할당) — 같은 기하 op
  이라도 scene-layer 정책 분기.
- **L3** 곡면 trim(ADR-205) 선례의 cutMode UX 를 폴리곤에 그대로 확장 — UX 일관 (above=+normal).

## 7. Cross-link

- ADR-240 (로드맵 Phase 1) / slice_volume_by_plane (slice.rs) / ADR-197 β-3-n (cutCurvedByZPlane) /
  ADR-205 (trimCurvedByPlane — cutMode 선례) / ADR-007 (manifold invariants) / ADR-194 (drill,
  restore-on-error 패턴).
- 메타-원칙 #6 (audit/de-risk) / #16 (명시 trigger) / ADR-046 P31 #4 (additive) / LOCKED #44.

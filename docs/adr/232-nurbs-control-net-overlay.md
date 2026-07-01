# ADR-232 — NURBS Control-Net Overlay (A2-MVP-1, visualize-only)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: NURBS surface 고급화 A2 (ADR-231 후속, A2-MVP-1 foundation)
- **Depends on**: ADR-231 (NURBS vault preset) / ADR-033 (NURBS surfaces engine) / ADR-228
  (scene-root overlay 패턴) / ADR-037 P22 (owner-ID selection) / ADR-140 (faceSurfaceKind)

## 1. Context — 시뮬레이션 기반 scope

ADR-231 §후속 A2(draggable control-net + per-CP weight) 상세 de-risk + 시뮬레이션 결과
**A2-full = L-XL** (Boolean β-4 defer 패턴):

| 필요 piece | de-risk 결과 |
|---|---|
| control net 읽기 | `get_face_surface_json` 은 NURBSSurface에 **counts만**(step6_json.rs:149-153) → ctrl_grid/weights 값 미반환 → 신규 read-back 필요 |
| surface 업데이트 | `setFaceSurface*` 는 Plane/Cylinder/Sphere/Cone/Torus만 → NURBS update 노출 0 → 신규 WASM 또는 re-create + corner CP boundary-vert subtlety |
| draggable handle | 코드베이스에 **TransformControls/drag-handle 없음** (모든 도구 click-move-click) → 신규 인터랙션 패러다임 |

→ 사용자 결재 **A2-MVP-1 (visualize-only foundation)**: 읽기 인프라 + 정적 control-net 오버레이.
drag/edit는 후속 (A2-MVP-2/full). 큰 drag 투자 전 foundation de-risk.

## 2. Decision — read-back + static control-net overlay

- **Engine** (`mesh.rs`): `NurbsSurfaceParams` struct + `Mesh::nurbs_surface_params(face_id)` —
  BezierPatch / BSplineSurface / NURBSSurface 의 ctrl_grid(row-major flat) + weights(Bezier/
  BSpline=1.0) + knots(Bezier=empty) + dims/degrees 읽기. 비-NURBS-class / missing → `None`.
- **WASM** (`lib.rs`): `getNurbsSurfaceParams(faceId) -> String` (JSON, "" if None). baseline +1.
- **Bridge** (`WasmBridge.ts`): `getNurbsSurfaceParams(faceId) -> NurbsSurfaceParams | null`
  (JSON parse + graceful null).
- **Viewport** (`Viewport.ts`): `updateNurbsControlNet(net | null)` — **scene-root overlay**
  (meshGroup wipe 무관, ADR-228 패턴): CP 마커(THREE.Points, amber, depthTest off) + net 라인
  (THREE.LineSegments, u-dir + v-dir grid edges). `null` → clear + dispose.
- **main.ts**: `selection.onChange` — 단일 NURBS-class face(faceSurfaceKind 6/7/8) 선택 시
  `getNurbsSurfaceParams` → `updateNurbsControlNet`; 그 외 → clear.

## 3. Lock-ins

- **L-232-1** A2-MVP-1 = visualize-only (read-back + 정적 오버레이). **편집/drag 없음** (A2-MVP-2/
  full 후속). drag = L-XL 신규 패러다임 (boundary-vert subtlety + 신규 WASM update).
- **L-232-2** read-back = `nurbs_surface_params` (struct, Rust-testable). 3 NURBS-class variant 모두
  지원. weights all-1.0 for Bezier/BSpline, knots empty for Bezier. 비-NURBS → None.
- **L-232-3** overlay = scene-root (ADR-228 패턴, selection-driven, render-only). always-visible
  amber (depthTest off), renderOrder 1000/1001.
- **L-232-4** selection-driven (faceSurfaceKind 6/7/8 단일 face). 그 외 selection → clear.
- **L-232-5** WASM read-only (`get_nurbs_surface_params` — &self, no mutation). 엔진 surface 변경 0.
- **L-232-6** ADR-046 P31 #4 additive — 신규 read-back + 오버레이만, 기존 기능 무변경.
- **L-232-7** 절대 #[ignore] 금지.

## 4. 회귀

- axia-geo **+3** (patch_surface adr232: bezier read-back / rational weights / None) 1990→1993.
- axia-wasm baseline +1 (`getNurbsSurfaceParams`) — `wasm_export_baseline_unchanged` PASS.
- vitest **+3** (WasmBridge getNurbsSurfaceParams: parse / empty→null / no-engine→null) 2387→2390.
- tsc 0.

## 5. 브라우저 검증 (real WASM, full chain)

- `createNurbsSurface`(vault 5×2 rational) → `getNurbsSurfaceParams(fid)` → kind 'NURBSSurface',
  dims [5,2,2,1], ctrlPts.len 30, **rational weight (1/√2) 보존**.
- `viewport.updateNurbsControlNet(params)` → overlay 'nurbs-control-net': **10 CP 마커** (5×2) +
  **26 net line verts** (13 segments: u-dir 8 + v-dir 5).
- `updateNurbsControlNet(null)` → overlay **cleared**.

## 6. Lessons

- **L1** 시뮬레이션이 scope 단계화 결정 — A2-full(L-XL, 신규 drag 패러다임) → A2-MVP-1(visualize-
  only foundation) 분리. drag 투자 전 read-back + overlay 인프라 확보.
- **L2** read-back gap 발견 — `get_face_surface_json` 이 counts만 반환 (값 미포함). 기존 export
  의 contract 확인 필수 (값이 필요하면 신규 read-back).
- **L3** scene-root overlay 패턴 (ADR-228 text overlay) 재사용 — selection-driven render-only
  오버레이의 canonical. meshGroup wipe 무관, dispose 관리.
- **L4** struct read-back (nurbs_surface_params) = Rust-testable + WASM JSON 직렬화 분리 — 엔진
  로직 단위 테스트 + WASM thin serialize.

## 7. 후속 (별도 트랙)

- **A2-MVP-2** — weight/CP 편집 (re-create via createNurbsSurface, drag 없이 값 편집).
- **A2-full** — draggable CP handle (신규 인터랙션 패러다임) + 라이브 update(신규 WASM
  setFaceSurfaceNurbs + corner boundary-vert) + weight 슬라이더. L-XL.
- CP 마커 weight 시각화 (marker size ∝ weight) / net 라인 색 차별화.
- Track B 곡면 Boolean edge case (deep SSI, 별도 dedicated ADR).

## 8. Cross-link

- ADR-231 (NURBS vault preset — A2 본 트랙 source) / ADR-033 (NURBS surfaces engine) / ADR-228
  (scene-root overlay 패턴) / ADR-037 P22 (owner-ID selection) / ADR-140 (faceSurfaceKind).
- 메타-원칙 #2 (render-only 오버레이) / ADR-046 P31 #4 (additive) / Boolean β-4 defer 패턴
  (시뮬레이션 → 단계화) / LOCKED #44 (Complete Meaning per Merge).

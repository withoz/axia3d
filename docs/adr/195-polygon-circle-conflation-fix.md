# ADR-195 — Polygon ↔ Circle Conflation Fix (dedicated polygon path)

> 사용자 보고 "다각형을 그리면 왜 원이 되나?" 의 root-cause fix. DrawPolygon 이
> *원* 경로(`drawCircleAsShape`)를 재사용 → 두 메커니즘에 의해 폴리곤이 원으로
> 변환. 전용 폴리곤 경로(`drawPolygonAsShape`)로 분리.

- **Status**: Accepted
- **Date**: 2026-06-10
- **Track**: 6 (boundary kernel) — Draw 도구
- **Builds on / fixes**: ADR-107 (≥12 circle threshold), ADR-028 (Arc curve
  metadata), ADR-189 (arc-aware re-derive), ADR-087 K-ε (DrawPolygon =
  drawCircleAsShape, **본 ADR 이 정정**)

---

## 1. 증상

DrawPolygon 도구(변 3~24 입력)로 다각형을 그리면 **원**이 됨. production 기본
설정(`face_rederive_on_draw` ON, ADR-176)에서 **모든 3~24각형**이 원이 됨.

## 2. Root cause — DrawPolygon 이 "원" 경로를 재사용

`DrawPolygonTool` → `bridge.drawCircleAsShape(…, sides)` ("다각형 = N분할 원",
ADR-087 K-ε). 엔진 `exec_draw_circle_as_shape` 는 *원을 N분할로 그리는* 경로라
**두 가지가 원에는 맞지만 폴리곤엔 틀림**:

| 메커니즘 | 위치 | 효과 |
|---|---|---|
| **① ≥12 threshold** | scene.rs `exec_draw_circle_as_shape` (ADR-107) | `segments >= 12 → Path B Circle` — 12~24각형이 원 |
| **② Arc metadata + re-derive** | scene.rs `exec_draw_circle` 가 각 변에 `AnalyticCurve::Arc` 부착 (ADR-028) + `face_rederive_on_draw` ON 의 arc-aware re-derive (ADR-189) | N<12 폴리곤도 매끈한 원으로 collapse |

**시뮬레이션 evidence (브라우저)**:
- faceRederive OFF: 11각형 → 11 edges 폴리곤 / 12·16각형 → 1 edge 원 (① 격리)
- faceRederive ON: 8각형도 1 edge 원 (② 격리)

→ **폴리곤 변은 직선이어야 하는데 "원의 호 조각"으로 취급**되어 원화.

## 3. Fix — 전용 폴리곤 경로 (사용자 결재 A)

`exec_draw_circle_as_shape` 는 **무손상** 보존. 폴리곤은 별도 경로:
- **Command** `DrawPolygonAsShape { center, normal, radius, sides }`
- **Engine** `exec_draw_polygon_as_shape` — N개 **순수 Line 세그먼트** 빌드:
  - **Arc curve metadata 부착 0** → re-derive 가 circularize 안 함 (② 차단)
  - **curve_owner_id 0** (폴리곤 = 별개 직선들)
  - **≥12 threshold 0** (① 차단) — 임의 N 폴리곤
  - 그 외(face 합성 / auto-intersect / Plane attach / drift snap / Xia→Shape
    collapse)는 circle 경로와 동일 (단일 Undo)
- **WASM** `draw_polygon_as_shape` + **TS bridge** `drawPolygonAsShape`
- **DrawPolygonTool** → `drawPolygonAsShape` (mouse commit + VCB 2곳)

## 4. Lock-ins

- **L-195-1** DrawPolygon = **순수 Line N-gon** (Arc metadata 0 / curve_owner 0 /
  threshold 0). 원으로 재통합 금지 — 폴리곤 ↔ 원 intent 데이터 레벨 분리.
- **L-195-2** `exec_draw_circle_as_shape` (원 경로) 무손상 — ≥12 threshold +
  Arc metadata 는 *원에는 맞음* (회귀 가드).
- **L-195-3** 메타-원칙 #4 (SSOT — 명시 의도) / #16 (intent 를 segment-count
  proxy 로 추론 금지).
- **L-195-4** `face_rederive_on_draw` 의 arc-aware re-derive (ADR-189) 는
  *Arc 곡선 metadata 가진 edge* 만 circularize — 폴리곤(plain Line)은 보존.
- **L-195-5** 절대 #[ignore] 금지.

## 5. 검증

**회귀 +2** (axia-core 352 → 354, 절대 #[ignore] 금지):
- `polygon_as_shape_stays_polygon_under_rederive_and_threshold` — sides 8/12/24
  모두 `face_rederive_on_draw` + `auto_intersect_on_draw` ON 에서 N Line edges
  유지 + curve metadata 0
- `circle_as_shape_path_unchanged_after_polygon_fix` — DrawCircleAsShape
  segments=16 은 여전히 1-edge Circle (원 경로 회귀 가드)

vitest **+0** (DrawPolygonTool.test 2 tests 갱신: drawCircleAsShape →
drawPolygonAsShape). 워크스페이스: axia-geo 1709 / axia-core 354 / transaction 5
— 0 failed, 0 ignored. tsc 0. vitest 2173 passed / 1 skipped.

**브라우저 시연 게이트 (ADR-087 K-ζ, production 기본 faceRederive ON)**:

| 그림 | 결과 |
|---|---|
| polygon N=8 / 12 / 24 | **각 N edges 폴리곤** ✓ (수정 전엔 모두 원) |
| circle N=16 | 1 edge Circle ✓ (회귀 무손상) |

**적대적 inline 검토**: (a) degenerate(sides<3 / radius≤0 / normal=0) → Error
(테스트) (b) 폴리곤 overlap → 공유 `intersect_faces_inner` 가 ADR-101 coplanar
split (plain Line 이라 정상) (c) Phase 2 는 circle 경로에서 복제 — drift 위험은
**follow-up: 공유 helper 추출** (현재 isolation 우선).

## 6. Out of scope (follow-up)
- Phase 2 (Xia→Shape) 공유 helper 추출 (circle/polygon 중복 제거).
- DrawPolygonTool sides 입력 UI 개선 (현재 prompt).
- Cross-link: ADR-087 K-ε §정정 (polygon ≠ circle-with-N-segments).

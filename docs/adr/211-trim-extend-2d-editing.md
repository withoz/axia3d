# ADR-211 — 2D Sketch Editing: Trim + Extend (free wire edges)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: 2D CAD 편집 명령 (ADR-210 메뉴 재구성 후속 C1) / Foundation
- **Depends on**: ADR-172 (wire crossing auto-split) / ADR-210 (Modify 메뉴 home) /
  ADR-087 K-ζ (사용자 시연 게이트) / `deleteEdgeCascade` · `move_vertex` ·
  `split_edge` · `is_edge_completely_free` (모두 기존)

## 1. Context

ADR-210 메뉴 재구성 후 2D CAD 편집 명령 구축의 첫 단계 C1. Trim/Extend
menu/command 항목은 이미 존재(index.html 1914-1915 + AxiaCommands 122-123)했으나
backing 도구/엔진이 없었음. 5-angle de-risk audit 결과 모든 building block
(split_edge / move_vertex / deleteEdgeCascade / line-line 교차)이 이미 존재 →
Pattern-12 (대부분 reuse).

## 2. 핵심 발견 (ADR-087 K-ζ 시연 게이트)

**와이어 라인은 교차 시 항상 자동분할된다** (ADR-172, `auto_intersect_on_draw`
flag와 무관한 별개 메커니즘). 브라우저 시연으로 확인:

| 시나리오 | 결과 |
|---|---|
| 두 교차 라인 draw | 교차점에서 *자동* split (4 sub-edge) |
| trimEdge(split+remove) | s=1(끝점 교차)로 **거부 — 도달 불가** |
| deleteEdgeCascade(세그먼트) | edges 5→4, **세그먼트 삭제 성공** |

→ 자동분할 엔진에서 "trim = 교차점에서 split 후 제거"는 중복(split이 이미
일어남). 올바른 trim = **"이미 분할된 세그먼트를 클릭해 삭제"**. 5개 Rust
de-risk 테스트는 `add_edge`로 *수동* 교차 edge를 만들어 통과했지만 실제 draw
흐름은 자동분할되어 trimEdge가 unreachable — unit test가 못 잡고 시연 게이트가
잡은 architectural 발견.

## 3. Decision

사용자 결재 (2026-06-22): **Option A — 세그먼트 삭제 + trimEdge 제거**.

- **Extend** — 신규 엔진 `Mesh::extend_edge_to_boundary(target, boundary)`:
  target의 가장 가까운 끝점을 boundary의 supporting line 교차점으로 `move_vertex`.
  교차 *전*이라 자동분할과 무관 → 진짜 필요한 연산. WASM `extendEdge` + bridge +
  `ExtendTool` (boundary = 선택, click = target).
- **Trim** — `TrimTool`이 클릭한 세그먼트를 `deleteEdgeCascade`로 삭제 (기존
  primitive 재사용, 신규 엔진/WASM 0). 자동분할이 교차점에서 이미 segment 경계를
  만들었으므로 세그먼트 삭제 = 가장 가까운 교차점까지 trim. boundary 선택 불필요.
- 도달 불가능한 `trim_edge_to_boundary` 엔진/WASM/bridge + trim de-risk 2테스트
  **제거**.

## 4. Lock-ins

- **L-211-1** Extend = `move_vertex` 합성 (신규 `operations/edit_2d.rs`, Pattern-12,
  신규 geometric kernel 0). 닫힌-형 segment closest-approach 교차.
- **L-211-2** Trim = 세그먼트 클릭 삭제 (`deleteEdgeCascade`), split+remove 아님 —
  자동분할 엔진 정합.
- **L-211-3** `trimEdge` primitive 제거 (자동분할로 도달 불가). Extend는 유지.
- **L-211-4** 자유 와이어 edge 한정 (`is_edge_completely_free` 가드). face-boundary
  trim/extend (면 재구성)은 별도 ADR.
- **L-211-5** Extend는 boundary=선택 + click=target. Trim은 boundary 불필요
  (클릭 세그먼트 = 자동분할 경계).
- **L-211-6** ADR-087 K-ζ 시연 게이트가 trim 의미를 발견 (unit test는 수동 edge라
  못 잡음). 향후 draw-흐름 의존 기능은 시연 게이트 필수.
- **L-211-7** ADR-046 P31 #4 additive — Trim/Extend 메뉴/단축키/action ID 불변
  (도구+엔진이 기존 entry를 backing).
- **L-211-8** 절대 #[ignore] 금지.

## 5. 구현

- **Engine** (`crates/axia-geo/src/operations/edit_2d.rs`, 신규):
  `extend_edge_to_boundary` + `line_line_closest` + `edge_endpoints_pos` 헬퍼.
- **WASM** (`lib.rs`): `extendEdge(target, boundary) -> i32` (+ export_baseline).
- **Bridge** (`WasmBridge.ts`): `extendEdge` wrapper (graceful fallback).
- **Tools**: `ExtendTool` (extendEdge) + `TrimTool` (deleteEdgeCascade) +
  공유 `edgePick.ts` (`pickClickedEdge` raycast→edgeMap). ToolManager 등록.

## 6. 회귀 + 검증

- **회귀**: axia-geo +4 (extend de-risk: meet-line / interior-crossing reject /
  parallel reject / skew reject) · axia-wasm +1 (extendEdge export, baseline) ·
  vitest +17 (TrimTool 6 + ExtendTool 8 + bridge extendEdge 3). 절대 #[ignore]
  금지. tsc clean.
- **브라우저** (real WASM): EXTEND target 길이 3→5mm (경계선 x=5 도달, topology
  보존) · TRIM 오버행 세그먼트 클릭 삭제 edges 5→4.

## 7. 후속 (별도 ADR, ADR-210 §6 로드맵)

- **C2** Fillet(2D corner) / Chamfer(2D corner) — 두 와이어 edge 사이 둥근/모따기 코너.
- **C3** Join (폴리라인 결합) + edge transform (Copy/Mirror/Array on edges).
- **C5** Point 도구 + Dimension (Linear/Aligned/Angular/Radial) — Dimension 메뉴.
- Face-boundary trim/extend (면 재구성) — 별도 ADR.
- (선택) "implied 경계선까지 trim" (target이 boundary segment를 안 지나는 overshoot)
  — t-무제한 reference-line trim primitive, 필요 시 별도.

## 8. Cross-link

- ADR-172 / ADR-173 (wire crossing auto-split — 본 ADR의 trim 의미 결정자)
- ADR-210 (메뉴 재구성 — Modify 메뉴 home)
- ADR-087 K-ζ (사용자 시연 게이트 canonical)
- ADR-141 (Master Roadmap — 2D 편집 트랙)
- ADR-046 P31 #4 (additive only)
- 메타-원칙 #4 (SSOT) / #5 (사용자 편의) / #6 (Preventive) / #16 (자동화 antipattern)
- LOCKED #44 (Complete Meaning per Merge — 단일 atomic PR)

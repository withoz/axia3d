# ADR-267 — Universal Watertight Production Gate

**Status**: Proposed (α — spec only)
**Track**: Track 7 (Phase 1 — CAD-core 실제 갭)
**연계 계획**: `docs/plans/IMPLEMENTATION-PLAN-2026-07-01.html` Phase 1.1
**Cross-link**: ADR-007(Face Orientation) · ADR-190 P0.2(WASM snapshot rollback) · ADR-097(Topology damage recovery dialog) · ADR-264 D3(coincident-position non-manifold) · LOCKED #1 P7 · 메타-원칙 #4 #6 #9

---

## 1. Problem (engine-grounded, 2026-07-01)

부피 연산(extrude / cut / boolean / face split)의 결과에 대한 **release 빌드
production 무결성 게이트가 없다.** 손상(비-watertight, 크랙, non-manifold,
winding 오류)이 release에서 **조용히 통과**한다.

근거 (clean baseline `E:\AXiA3D` @ ADR-264 실측):

- `Mesh::debug_verify_invariants` 는 `#[cfg(debug_assertions)]` 게이팅
  (`mesh_invariants.rs:364`) — release에서 no-op. 모든 volume op가 이것만 호출
  (`create_solid`/`boolean`/`deform`/`draw`/`array_op`/`edit_2d`).
- 좌표-coincident 크랙 검출기 `collect_non_manifold_edges_geometric`
  (ADR-264 D3, `mesh.rs:10420`)는 **테스트에서만** 호출됨
  (`scene.rs:14392`, `mesh.rs:20081`) — production 미배선.
- `face_set_manifold_info` (`mesh.rs:9895`)는 `boundary_edge_count` /
  `non_manifold_edge_count` / `is_closed_solid`를 계산하나, **분류(classification)
  용도**로만 쓰이고 op 종료 게이트로 쓰이지 않음.
- 결과: "손상이 조용히 통과" (총체 점검 `engine-systemic-review-2026-07-01.html`
  §2 결함 ③ — release 게이트 부재).

## 2. 재사용 자산 (신규 알고리즘 0)

본 ADR은 **새 검출 알고리즘을 만들지 않는다.** 이미 존재하는 3종을 조립한다:

| 자산 | 위치 | 검출 대상 |
|---|---|---|
| `verify_face_invariants()` | `mesh_invariants.rs:110` | I1~I5: null loop / winding 불일치 / inner 유효성 / HE 소속 / **≥3-face 위상 non-manifold** |
| `collect_non_manifold_edges_geometric()` | `mesh.rs:10420` | **coincident-position 크랙** (같은 좌표 ≥3 HE, ≥2 distinct EdgeId — ADR-102 cleave/보스 잔재) |
| `face_set_manifold_info(faces)` | `mesh.rs:9895` | `boundary_edge_count`(열림) / `is_closed_solid`(watertight) |

배선 인프라도 존재: **WASM snapshot rollback (ADR-190 P0.2)** —
`restore_scene_snapshot(&before)` + `lastError()` (`lib.rs:7894/7965`, 4113~4221 문서).

## 3. Decision — Lock-ins

- **L1 (composite report).** `Mesh::verify_volume_integrity(scope) ->
  VolumeIntegrityReport` 신규. 3자산을 합쳐 카테고리별 위반을 담는다:
  `invariant_violations`(I1~I5) / `geometric_cracks`(EdgeId) / `open_boundary`
  (닫힘 기대 시 boundary_edge_count) / `is_valid()`. **release-safe**
  (cfg 게이팅 없음 — 항상 실행 가능).
- **L2 (scope-aware).** 두 모드:
  - `IntegrityScope::ClosedSolid(&[FaceId])` — extrude/cut/boolean 등 닫힌 솔리드
    산출 op: I1~I5 통과 **AND** geometric crack 0 **AND** `is_closed_solid`
    (boundary 0 + 위상 nm 0).
  - `IntegrityScope::OpenMesh` — sheet/wire/draw/split 등 열린 결과 허용 op:
    I1~I5 통과 **AND** geometric crack 0. (boundary 허용 — 열린 시트는 정상.)
- **L3 (fail = rollback, 절대 silent 아님).** 게이트 실패 시 op는 **byte-identical
  롤백**되고 typed error를 남긴다. 부분 손상 상태를 절대 커밋하지 않는다.
  엔진은 self-rollback 안 함(ADR-190 P0.2) → **WASM 경계가 snapshot SSOT**:
  `before = snapshot()` → op → `verify_volume_integrity` → 실패 시
  `restore_scene_snapshot(&before)` + `lastError` set + false 반환.
- **L4 (기존 debug 훅 보존).** `debug_verify_invariants`(debug-only)는 그대로
  유지 — 개발 중 즉시 assert. 본 게이트는 **release 방어선 추가**이지 대체 아님.
- **L5 (LOCKED #1 P7 불변).** ADR-021 P7 form-layer의 **의도적** radial
  non-manifold(스택 시트)는 `collect_non_manifold_edges`(radial)로 남고, 본
  게이트의 crack 검출은 `collect_non_manifold_edges_geometric`(≥2 distinct
  EdgeId 조건)만 사용 — P7 정상 케이스를 오탐하지 않음(ADR-264 D3 설계 그대로).
- **L6 (scope 대상 op, β에서 점진 배선).** 우선순위: `create_solid`(extrude/cut
  포함) → `boolean` → `cut_wall_door_opening`/`punch_*`/`drill_*` → face split.
  각 op별 β sub-step으로 배선(1 op = 1 commit, 회귀 동반).
- **L7 (성능, 메타-원칙 #11).** 게이트는 op당 O(active faces + active HEs).
  Commit-budget(<100ms) 내. scope를 op가 만진 face로 한정 가능 시 한정.
- **L8 (UI 복구, ADR-097 재사용).** 실패 Toast + (선택) ADR-097
  TopologyRecoveryDialog `[Undo]/[강등]/[수동수정]` 재사용. 기본은 자동 롤백
  + Toast.

## 4. VolumeIntegrityReport (β 예정 형태)

```rust
pub enum IntegrityScope<'a> {
    ClosedSolid(&'a [FaceId]),  // watertight 강제
    OpenMesh,                   // boundary 허용
}

pub struct VolumeIntegrityReport {
    pub invariant_violations: Vec<String>,   // verify_face_invariants
    pub geometric_cracks: Vec<EdgeId>,       // collect_non_manifold_edges_geometric
    pub open_boundary_edges: usize,          // ClosedSolid scope에서만 의미
    pub checked_faces: usize,
}
impl VolumeIntegrityReport {
    pub fn is_valid(&self) -> bool { /* 모든 카테고리 clean */ }
    pub fn summary(&self) -> String { /* human-readable */ }
}
```

## 5. Path Z sub-steps

- **α** (본 commit) — spec only. 코드 0.
- **β-1** — engine: `verify_volume_integrity` + `VolumeIntegrityReport` +
  `IntegrityScope`. 3자산 조립. Rust 회귀(정상 솔리드 valid / 크랙 주입 invalid /
  열린 시트 OpenMesh valid ClosedSolid invalid / P7 스택 오탐 0).
- **β-2** — `create_solid` 경로 배선: WASM extrude 래퍼에 snapshot→op→gate→
  실패 시 rollback+lastError. vitest(정상 통과 / 손상 주입 rollback+에러).
- **β-3** — `boolean` 경로 배선 (동일 패턴).
- **γ** — 나머지 op(punch/drill/door/split) 배선 + WASM `verifyVolumeIntegrity()`
  export.
- **δ** — UI: lastError → Toast + ADR-097 dialog 옵션.
- **ε** — real Chromium E2E: 손상 주입 시나리오가 rollback 되어 scene이
  **byte-identical 유지**됨 + 정상 op는 통과.
- **ζ** — closure + 회귀 sweep + LOCKED/CLAUDE.md 갱신.

## 6. 회귀 자산 (절대 #[ignore] 금지)

- `verify_volume_integrity_clean_solid_valid`
- `verify_volume_integrity_injected_crack_invalid` (coincident 크랙 주입)
- `verify_volume_integrity_open_sheet_valid_as_openmesh_invalid_as_closed`
- `verify_volume_integrity_p7_stacked_sheet_not_flagged` (LOCKED #1 오탐 0)
- vitest: `extrude_damage_rolls_back_byte_identical` / `boolean_damage_rolls_back`
- Playwright: `damage_injection_scene_unchanged_after_gate`

## 7. Non-goals

- 손상 **자동 복구(repair)** 는 본 ADR 범위 외 (ADR-097 recovery는 UI 선택지로만
  재사용; 기하 자동 수선은 별도 ADR).
- 새 크랙/manifold 검출 알고리즘 (기존 3자산 재사용).
- import(STEP/IGES) 경로 게이트 (별도 — import는 warnings 누적 정책 ADR-036 P21.7).

# ADR-229 — Trim/Extend Stub-Status Truth-up (Audit Finding 3 closure)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: catalog 정리 (ADR-228 §후속 — trim/extend stale-stub)
- **Depends on**: ADR-211 (Trim/Extend 2D edit impl) / ADR-228 (text3d — 같은 stub list) /
  ADR-226 (status 오라벨 패턴 — explode) / ADR-133 (AC ⊇ CC)

## 1. Context

ADR-228 작업 중 발견: `tool-trim` / `tool-extend` 가 ActionCatalog 에서 `status:'stub'` +
description "(Stub) ... not yet implemented" 인데, **ADR-211 이 이미 구현** (TrimTool /
ExtendTool 등록 + 작동). ADR-226 explode `ui-only` 오라벨과 같은 class 의 stale 상태.

audit 확증:
- `TrimTool` (ToolManagerRefactored.ts:399) → `bridge.deleteEdgeCascade(edgeId)` (composite —
  세그먼트 클릭 삭제, dedicated trim capability 없음).
- `ExtendTool` (ToolManagerRefactored.ts:400) → `bridge.extendEdge(target, boundary)` (WASM
  export js_name `extendEdge`, lib.rs:5179 / WasmBridge.ts:4485 — dedicated capability).
- catalog.ts 의 `status:'stub'` 잔존 = trim/extend 2건만 (text3d 는 ADR-228 에서 'ui-only'
  전환, placeholder 0). → 정정 후 catalog **stub 0**.

## 2. Decision — status truth-up

- **tool-trim**: `status:'stub'` → `'delegated'`, `aliases:{}` (composite — deleteEdgeCascade
  로 위임, pie/rotrect ADR-225 composite-tool 선례). description 정정.
- **tool-extend**: `status:'stub'` → 'ok' (default, status 필드 제거), `aliases:{ bridge:
  'extendEdge', wasm:'extendEdge' }` (dedicated, lib.rs js_name). description 정정.
- **catalog.test.ts** "Audit Finding 3" describe 갱신 — stub 0 이므로:
  - `catalog carries no stub-status entries` (Audit Finding 3 fully closed — 모든 formerly-
    stubbed tool 구현).
  - `any stub (if present) has an honest description` (future stub 가드 — zero 허용).
  - unused `type ActionDef` import 정리.

## 3. Lock-ins

- **L-229-1** trim = 'delegated' (composite deleteEdgeCascade), extend = 'ok' + extendEdge
  alias (dedicated). ADR-211 구현 반영.
- **L-229-2** Audit Finding 3 **완전 closure** — catalog stub-status 엔트리 0. "no stale stubs"
  회귀 가드 (작동하는 tool 이 'stub' 으로 재-오라벨되는 drift 차단).
- **L-229-3** future 정당한 stub 는 허용 (deliberate — "any stub has honest description" 가드 +
  "준비 중" integrity guard at MenuBar.setActiveTool). "no stub-status" 단언은 현 시점 reality
  문서화 (future stub 추가 시 deliberate 업데이트).
- **L-229-4** AC ⊇ CC / CC count 불변 (172) — 엔트리 add/remove 0, metadata(status/aliases/
  description)만 변경.
- **L-229-5** 엔진/WASM/도구/메뉴 변경 0 (ADR-211 구현 그대로, catalog metadata만).
- **L-229-6** 절대 #[ignore] 금지.

## 4. 회귀

- 패키지 catalog 24/24 (no-stale-stubs 단언 + honest-description 가드 PASS). dist 재빌드.
- AC ⊇ CC / CC count 172 불변. 엔진/WASM/web src 변경 0.

## 5. Lessons

- **L1** stale-stub = ADR-226 explode 오라벨과 동일 class — 구현 후 catalog status 갱신 누락.
  새 tool 구현 ADR 은 catalog status('stub'→'ok'/'ui-only'/'delegated') 동시 갱신 강제.
- **L2** "no stale stubs" 가드 > 특정-tool 단언 — 특정 id 를 stub 로 단언하면 그 tool 구현 시
  test 도 갱신해야 (ADR-220→228→229 누적 갱신). catalog-level "stub 0" + "honest stub" 가드가
  drift 에 더 robust.
- **L3** composite vs dedicated status — trim(deleteEdgeCascade composite)='delegated' /
  extend(extendEdge dedicated)='ok'+alias. ADR-224/225/226 의 status 분류 (ui-only/delegated/
  ok/redirect) 일관 적용.

## 6. Cross-link

- ADR-211 (Trim/Extend impl — 본 ADR 이 catalog status 반영) / ADR-228 (text3d — 같은 stub
  list, §후속 trigger) / ADR-226 (explode status 오라벨 패턴) / ADR-225 (pie/rotrect 'delegated'
  composite 선례) / ADR-224 (status 분류).
- ADR-133 (AC ⊇ CC — count 불변) / ADR-046 P31 #4 (additive — 엔진/도구 무변경) / 메타-원칙 #4
  (SSOT — catalog status = reality) / LOCKED #44 (Complete Meaning per Merge).

# ADR-220 — Sweep/Loft Discoverability + Catalog Drift Restoration (AC ⊇ CC)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: Roadmap ③ 24-도구 Phase 5 (Sweep 진입점) / Foundation
- **Depends on**: ADR-133 (Dual Catalog AC ⊇ CC invariant) / ADR-045 D1 (ActionCatalog
  identity SSOT) / ADR-079·087 (create_solid) / ADR-210 (MoDeling 메뉴)

## 1. Context

24-도구 로드맵 ③의 첫 항목 Sweep. de-risk (4 서브시스템 병렬 조사 + 브라우저 검증)가
**Pattern-12 확정**: Sweep/Loft 도구가 **이미 존재·작동**.

- `DrawSweepTool`(원형 프로파일 + 클릭 경로 → 파이프) / `DrawLoftTool`(단면 블렌드) —
  ToolManager 등록, MoDeling 메뉴, MenuBar case, KeyboardShortcuts 'W'→sweep 모두 배선.
- WASM `sweep_profile_along_path`/`loft_sections`/`revolve_profile` + bridge 래퍼 작동.
- **브라우저 검증**: 원형 r3 → L-경로 sweep → 48면 추가, invariants valid 0 violations.

**진짜 gap**: `tool-sweep`/`tool-loft`이 **CommandCatalog(AxiaCommands.ts)에 미등록** →
Command Palette / 단축키 도움말에 안 보임 (ADR-133 identity-vs-dispatch 패턴).

**사용자 결재 (2026-06-23)**: Scope **A — Discoverability 마무리** (등록 + 검증 + 문서).
Profile-face sweep (엔진 `sweep_surface_1_rail` Bishop frame 배선)은 별도 후속 ADR.

**Audit-first 확장 (구현 중 발견)**: `tool-sweep`/`tool-loft`을 CommandCatalog에 추가하면
`CatalogConsistency.test.ts`(AC ⊇ CC)가 검증 → 단, 이 invariant이 **ADR-206부터 이미
drift**: ellipse/chamfer/copy/mirror/array×2/fillet/corner-fillet/corner-chamfer/join/
dimension×4 = **14개 tool이 CommandCatalog에만 등록, ActionCatalog 누락** (stale dist가
가림). 따라서 본 ADR의 진짜 complete meaning = **AC ⊇ CC invariant 복원** (Sweep/Loft가
진입점, LOCKED #44 정합 — 한 의미 단위 = "catalog가 모든 user-facing 도구를 정확히 반영").

## 2. Decision

**Sweep/Loft discoverability closure + catalog drift 전면 복원** (Pattern-12, 코드 0).

- **CommandCatalog** (`AxiaCommands.ts`): `tool-sweep`(category modify, 'W') + `tool-loft`
  추가. 단축키 dispatch SSOT는 `KeyboardShortcuts.ts` keyMap('W'→sweep, 이미 존재);
  CommandCatalog `shortcut`은 표시용 메타데이터 → 충돌 없음.
- **ActionCatalog** (`packages/axia-action-catalog/src/catalog.ts`): AC ⊇ CC 복원 —
  16개 entry 추가 (sweep/loft + 누락 14개) + 정정:
  - `tool-point` stub status 제거 (ADR-219 구현 완료 — catalog 정확성).
  - `mirror-x`/`fillet-edge`/`chamfer-edge`의 stale `legacy: ['tool-mirror'/'tool-fillet'
    /'tool-chamfer']` 제거 (이들은 ADR-209/207의 *현행 별개 도구*, legacy 아님).
  - copy/mirror/array entry에 bridge alias 부여 (arrayLinearFaces/mirrorFaces/arrayRadialFaces).
- **dist 재빌드** (ADR-133 L-133-8: catalog.ts → dist, web import source).
- **테스트 정정**: CatalogConsistency CC count 148→164; 패키지 stub-list에서 tool-point
  제거; 패키지 `CATALOG_SIZE === 95` stale snapshot 단언 제거 (13 endpoint는 개별 검증
  유지, 95→161→177 drift는 fragile snapshot).
- **엔진/WASM/도구 변경 0** — Sweep/Loft 도구·메뉴·bridge·KeyboardShortcuts 모두 보존.

## 3. Lock-ins

- **L-220-1** Sweep/Loft = 이미 작동 (Pattern-12, 브라우저 검증 48면 manifold). 본 ADR은
  discoverability + catalog 정확성만.
- **L-220-2** AC ⊇ CC invariant 복원 (ADR-133 L-133-3) — CommandCatalog 모든 id가
  ActionCatalog에 존재 (CatalogConsistency.test 강제).
- **L-220-3** Stale legacy alias 제거 (mirror/fillet/chamfer) — 현행 별개 도구를 legacy로
  오기록한 것 정정.
- **L-220-4** tool-point catalog 정확성 (ADR-219 구현 반영).
- **L-220-5** 단축키 dispatch SSOT = KeyboardShortcuts.ts (CommandCatalog shortcut은 표시용).
- **L-220-6** dist 재빌드 필수 (web import source).
- **L-220-7** Fragile snapshot 단언(`CATALOG_SIZE === 95`) 제거 — additive ADR마다 깨지던
  pre-existing debt. 개별 endpoint 검증 + self-consistency 단언 유지.
- **L-220-8** Profile-face sweep (sweep_surface_1_rail Bishop frame 배선) 별도 후속 ADR.
- **L-220-9** ADR-046 P31 #4 additive only — 도구/메뉴/엔진 surface 보존.
- **L-220-10** 절대 #[ignore] 금지.

## 4. 회귀 + 검증

- **회귀**: ActionCatalog +16 entries (sweep/loft + 14 drifted), CommandCatalog +2
  (sweep/loft). CatalogConsistency 3/3 PASS (AC ⊇ CC + count 164). 패키지 catalog
  24/24 PASS (#1 legacy collision / #2 alias-or-status / #3 stub-list / #4 stale count
  모두 해소). web commands 26/26. tsc clean. 엔진(cargo) 무변경.
- **브라우저** (real WASM, de-risk): `sweepProfileAlongPath`(원형 r3, L-경로) → 48면 추가,
  invariants valid 0 violations (manifold 파이프). sweep/loft 도구 ToolManager 등록 확인.

## 5. 후속 (별도 ADR)

- **Profile-face Sweep** — 엔진 `sweep_surface_1_rail`(Bishop frame, rotation-minimizing,
  surfaces/sweep.rs 구현됨) 배선 + 프로파일 면 선택 → 임의 경로 sweep. `CreateSolidMode::
  Sweep` alignment 완화 또는 신규 SweepRail mode. DrawSweepTool 면-선택 개편.
- **Revolve 인터랙티브 도구** (현재 revolve-x/y/z one-shot action) + 부분 각도(360° only 완화).
- **Loft N-profile** (현재 2-profile 또는 하드코딩 VASE) + auto-resampling.
- tool-trim/tool-extend/tool-text3d catalog status 정확성 (ADR-211 등 후속).

## 6. Cross-link

- ADR-133 (Dual Catalog AC ⊇ CC, CatalogConsistency.test) / ADR-045 D1 (ActionCatalog SSOT)
- ADR-079·087 (create_solid / kernel-aware) / ADR-210 (MoDeling 메뉴) / ADR-219 (tool-point)
- ADR-206~218 (drifted 14 tools — 본 ADR이 ActionCatalog 복원)
- ADR-046 P31 #4 (additive) / ADR-087 K-ζ (시연 게이트) / LOCKED #44 (Complete Meaning per Merge)
- 메타-원칙 #4 (SSOT) / #5 / #6

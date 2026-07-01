# ADR-247 — Phase 3 E2: Loft auto-resample + 2-face Loft UI

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 3 (Extrude 완성) — E2 (Loft resample)
- **Depends on**: ADR-079 W-3-β (loft_between_profiles) / Mesh::loft (loft.rs) /
  ADR-133 (AC ⊇ CC) / ADR-046 P31 #4 (additive) / split_edge / compute_parameters

## 1. Context

Phase 3 audit (5-agent) 결과 E1 부분 Revolve = medium(UI 얽힘), E2 Loft resample =
small, E3 multi-loop extrude = ALREADY-ROBUST (ADR-191 scene.rs:7080 push_pull
intercept). 사용자 결재 **E2 + 2면 Loft UI 연결 (완전성)**.

**E2 gap**: `create_solid(Loft)` → `loft_between_profiles` (create_solid.rs:1814)
가 두 프로파일의 vert count 불일치 시 `NotYetSupported "no auto-resampling in
W-3-β MVP"` bail. `Mesh::loft` 은 두 section 의 len 이 같아야 함.

**audit 정정 (C2 교훈 재현)**: audit synthesis 가 "E2 가 fully-wired Loft tool 을
unblock" 이라 했으나, 실측 검증 결과 **2개 loft 경로**가 있음:
- 경로 1 `create_solid(Loft{other})` (face-based 2-profile) — E2 대상. scene/WASM/TS
  **어디서도 호출 안 됨** (UI 미연결).
- 경로 2 `bridge.loftSections` → WASM `loft_sections` → `Mesh::loft` — DrawLoftTool
  (균등 circular sections, mismatch 없음).
DrawLoftTool 은 경로 2 사용 → E2(경로 1)는 DrawLoftTool 을 unblock 하지 않음. 그래서
**2면 Loft UI 를 신설**해 경로 1 을 사용자에게 노출 (Fusion 식 "2 프로파일 블렌드").

## 2. Decision

**E2 engine (create_solid.rs)** — `loft_between_profiles` 의 mismatch bail 제거 →
**짧은 프로파일 cap 을 `resample_loft_profile`(신규)로 N verts 까지 subdivide**.
`Mesh::loft` 은 `add_vertex`(LOCKED #5 spatial-hash dedup)로 section positions 를
vert 화하므로 **section positions 가 cap verts 와 일치**해야 manifold (cap ↔ side
wall 공유). 따라서 단순 position-interp 가 아닌 **cap 자체를 edge-split** (긴 boundary
edge midpoint greedy)로 N verts 화 → FaceId + outline 보존 (verts 가 원래 perimeter
위) → 양 cap dedup-match → manifold. `Mesh::loft` 무변경.

**2면 Loft UI** — 경로 1 을 end-to-end wiring (createSolidExtrude 패턴 답습):
- WASM `create_solid_loft(profile, other)` → `Command::CreateSolid { Loft }`
  (exec_create_solid 가 Loft 를 mesh.create_solid 로 통과 — fallback_dist=None 이라
  Extrude-only 분기 skip). baseline additive-guard PASS.
- Bridge `createSolidLoft(f1, f2)` (graceful, `as any` cast).
- ToolManager `loft-selected-faces` action — 정확히 2 선택 면 → createSolidLoft →
  syncMesh. VALID_ACTIONS + ACTION_DISPLAY 등록.
- 메뉴 `로프트 — 선택 면 2개 (Loft 2 faces)` (Modeling, tool-loft 옆) + MenuBar case.
- CommandCatalog + ActionCatalog (AC ⊇ CC, loft-selected-faces ↔ createSolidLoft/
  create_solid_loft aliases). DrawLoftTool('loft', 경로 2) 와 별개 entry — 공존.

## 3. Lock-ins

- **L-247-1** resample = cap edge-split (FaceId + outline 보존, dedup-match) — NOT
  position-interp (cap↔side-wall 정합 깨짐). `Mesh::loft` 무변경.
- **L-247-2** 폴리곤 프로파일 한정 (≥3 boundary verts). 닫힌-곡선 self-loop 프로파일은
  scope 밖 (<3 verts → guard bail).
- **L-247-3** greedy 긴 edge midpoint split (균등 분포). 정확 arc-length 대응은 future
  (MVP 는 valid manifold; correspondence 품질 secondary).
- **L-247-4** 경로 1 (create_solid Loft) UI 신설 — 경로 2 (DrawLoftTool, circular
  vase) 와 공존, 별개 entry. SSOT (단일 tool-pushpull 처럼 단일 identity loft-selected-
  faces).
- **L-247-5** ADR-191 정합 — E3 (multi-loop extrude) 는 이미 scene push_pull intercept.
  본 ADR 무관 (Loft 는 multi-loop reject 유지, ADR-016 Q2).
- **L-247-6** ADR-046 P31 #4 additive (신규 메뉴 항목 + 단축키 없음).
- **L-247-7** AC ⊇ CC (ADR-133) — CommandCatalog 174 + ActionCatalog 동기, dist 재빌드.
- **L-247-8** 절대 #[ignore] 금지.

## 4. 회귀 / 검증

- axia-geo create_solid +2 (loft_mode_vertex_count_mismatch_resamples: square↔triangle
  3→4 resample + closed solid + invariants / loft_mode_triangle_to_hexagon_resamples:
  3→6 multi-split) — 기존 mismatch-bail 테스트를 success 로 전환. axia-geo 1994 lib.
- axia-wasm: create_solid_loft export (baseline additive-guard PASS).
- vitest: CatalogConsistency count 173→174 갱신, AC⊇CC PASS / action-catalog D1 24 /
  web commands 26 / 전체 web 161 files PASS. tsc 0. catalog dist 재빌드.
- 브라우저 end-to-end (real WASM): 메뉴 "로프트 — 선택 면 2개" 존재 + triangle(z=0,3
  verts) + square(z=200) → createSolidLoft → faces 2→6 (3→4 resample + 4 walls + 2
  caps) + invariants valid 0 violations.

## 5. Lessons

- **L1 audit synthesis ≠ ground truth (C2 교훈 재현)**: audit 가 "E2 unblocks Loft
  tool" 이라 했으나 실측 검증으로 **2 loft 경로** 발견 (경로 1 face-based UI 미연결 /
  경로 2 DrawLoftTool). LLM audit 의 "fully-wired" 주장을 grep 으로 반증 → UI 신설
  scope 확장 결정. **모든 audit 결론은 commit 전 실측 grep**.
- **L2 loft dedup-match 제약**: `Mesh::loft` 의 add_vertex 가 dedup → section
  positions = cap verts 여야 manifold. resample 은 cap 자체를 edge-split 해야 함
  (position-interp 면 cap↔side-wall 깨짐). audit 의 "small position-interp" sketch 가
  이를 놓침 → 실제는 cap edge-split (small-medium).
- **L3 경로 1 ↔ 경로 2 공존**: 같은 "Loft" 개념의 2 구현 (2-profile blend vs
  circular-section vase) 을 별개 UI entry 로 공존. user vocabulary 의 "Loft" 가 둘 다
  포함 — SSOT 위반 없이 (각자 단일 identity).
- **L4 createSolidExtrude 패턴 재사용**: WASM/bridge/action/menu/CC/AC 5-layer wiring
  이 createSolidExtrude (경로 1 Extrude) 템플릿 1:1 답습 — Pattern-12.

## 6. 후속 (Phase 3 나머지)

- **E1 부분 Revolve (<360°)**: revolve_with_angle + 양 end cap + angle UI (medium,
  engine+UI entangled, ADR-007 winding 위험). 별도 ADR.
- **E3 guard (defense-in-depth)**: create_solid Extrude entry 에 multi-loop reject
  guard (scene 이 이미 push_pull intercept하므로 user 가치 0, 선택적 rider).
- Loft correspondence 품질 (arc-length 대응) — 현재 greedy midpoint (valid manifold).

## 7. Cross-link

- ADR-240 (Phase 3 로드맵) / ADR-079 W-3-β (loft_between_profiles) / ADR-191 (E3
  multi-loop push_pull, LOCKED #79) / Mesh::loft (loft.rs) / split_edge /
  compute_parameters (curves/fitting.rs) / ADR-133 (AC ⊇ CC) / ADR-046 P31 #4 /
  createSolidExtrude (경로 1 Extrude wiring 템플릿). 메타-원칙 #4 (SSOT) / #6 (audit).

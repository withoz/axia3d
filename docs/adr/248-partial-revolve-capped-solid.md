# ADR-248 — Phase 3 E1: Partial Revolve (<360°) → capped wedge solid + UI

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 3 (Extrude 완성) — E1 (partial Revolve)
- **Depends on**: ADR-079 W-4 (revolve_profile_face) / Mesh::revolve / Mesh::loft
  (ADR-247 답습) / ADR-007 (winding) / ADR-133 (AC ⊇ CC) / ADR-046 P31 #4

## 1. Context

Phase 3 audit: E1 부분 Revolve = real-bail. `revolve_profile_face` (create_solid.rs)
가 `(angle_rad - TAU).abs() > 1e-3 → NotYetSupported` (full 360° only). 사용자 결재
**B: capped SOLID 부분 revolve + UI (완전성)**.

**de-risk 정정 (C2/E2 교훈 재현)**: 실측 — **2 revolve 경로** 모두 캡 없는 open
SURFACE + full-only:
- Path A (UI) revolve-x/y/z → `bridge.revolveProfile` → WASM revolve_profile →
  `Mesh::revolve` (선택 엣지, open-polyline, full 360°, 캡 없음).
- Path B (tested-only) `create_solid(Revolve{angle_rad})` → revolve_profile_face
  (profile FACE, angle 인지하나 full-guard, 역시 Mesh::revolve → 캡 없음). UI 미연결.

즉 어느 경로도 capped partial solid 을 안 만듦 → E1 = 엔진 partial-capped 경로 신설
+ 새 UI (E2 loft 와 동일 완전성 패턴).

## 2. Decision

**E2 engine (create_solid.rs `revolve_profile_face`)**:
- full-only guard 제거 → angle ∈ (0, 2π]. full(≈2π)은 기존 open-polyline `Mesh::revolve`
  경로 유지. partial(<2π)은 신규 **capped wedge** 경로.
- partial: 프로파일 boundary(닫힌 loop)를 angular arc 따라 **회전 sections 로 loft**
  (ADR-247 E2 답습 — closed_sections=true). section[k] = profile rotated by k·(angle/
  segments), k=0..=segments (segments = round(32·angle/2π), ≥2). loft section 0 =
  profile → add_vertex dedup → profile_face verts (θ=0 cap). θ=angle cap = rotated
  profile 의 새 face (reversed loop winding → +θ outward). `reconcile_face_normals`.
- **pole guard**: profile 가 축에 닿으면(radial < EPSILON_LENGTH·10) bail (partial
  pole = future).
- `rotate_around_axis` (revolve.rs) `pub(crate)` 승격.

**UI (createSolidExtrude/Loft 5-layer 답습)**:
- WASM `create_solid_revolve(face, ox,oy,oz, dx,dy,dz, angle_rad)` → Command::
  CreateSolid{Revolve} (exec_create_solid 가 fallback_dist=None → Extrude-only 분기
  skip 후 통과). baseline additive-guard PASS.
- bridge `createSolidRevolve(...)` + ToolManager `revolve-face-solid` action (1 선택
  면 → 각도(도)·축(X/Y/Z) prompt → createSolidRevolve, axis 원점 통과) + 메뉴 '회전체
  — 선택 면 (Revolve · 각도 입력 · 부분/360°)' + MenuBar case + CC/AC (AC⊇CC,
  count 174→175). 기존 revolve-x/y/z (edge open-surface)와 공존 별개 entry.

## 3. Lock-ins

- **L-248-1** partial revolve = closed-profile loft of rotated sections + θ=0
  (profile_face) + θ=angle end cap. full = 기존 Mesh::revolve open-polyline 보존.
- **L-248-2** pole guard — partial revolve 프로파일은 축에서 떨어져야 (axis-touching
  = future). full 은 pole 허용 (Mesh::revolve pole 처리).
- **L-248-3** θ=angle cap reversed loop winding (+θ outward), reconcile_face_normals.
  manifold 검증으로 확인.
- **L-248-4** axis 원점 통과 cardinal (X/Y/Z) — UI MVP. 프로파일 평면이 축 포함 +
  축에서 offset 해야 (W4-C + pole guard, clear error). 유연 축 pick 은 future.
- **L-248-5** segments = round(32·angle/2π) ≥ 2 (full 360° = 32 비례).
- **L-248-6** UI 5-layer (createSolidExtrude/Loft 답습), 기존 edge revolve-x/y/z 공존.
- **L-248-7** AC ⊇ CC (ADR-133), CommandCatalog 175, dist 재빌드.
- **L-248-8** ADR-046 P31 #4 additive (신규 메뉴 + 단축키 없음). 절대 #[ignore] 금지.

## 4. 회귀 / 검증

- axia-geo create_solid: revolve_partial_angle_returns_not_yet_supported →
  revolve_partial_axis_touching_profile_pole_bails (unit square 축 접촉 = pole bail)
  + 신규 revolve_partial_offset_profile_makes_capped_wedge (XZ rect x∈[2,4] 90°
  around Z → closed solid + invariants). axia-geo **1995 lib**.
- axia-wasm: create_solid_revolve export (baseline additive PASS).
- vitest: CatalogConsistency 174→175 (AC⊇CC), action-catalog D1 24, web commands 26,
  전체 web 161 files PASS. tsc 0. catalog dist 재빌드.
- 브라우저 end-to-end (real WASM): 메뉴 '회전체 — 선택 면' 존재 + XZ rect(x∈[2,4])
  → createSolidRevolve 90° around Z → faces 1→34 (8 segments × 4 edges + 2 caps) +
  invariants valid 0 violations.

## 5. Lessons

- **L1 audit 정정 3번째 (C2/E2 교훈)**: roadmap "partial revolve" 가 단순 guard-flip
  처럼 보였으나, 실측으로 **2 revolve 경로 모두 캡 없는 open surface** 발견 → E1 =
  엔진 capped 경로 신설 + UI. commit 전 실측 grep 필수.
- **L2 loft 재사용 (ADR-247 답습)**: partial revolve 의 side surface = 회전 sections
  의 loft (closed). E2 의 loft + dedup-cap 패턴이 E1 에 직접 재사용 — Pattern-12.
- **L3 cap winding 휴리스틱 + manifold 검증**: θ=angle cap 을 reversed loop 으로 (θ=0
  profile_face 반대) → +θ outward. 정확성은 manifold (boundary 0) + invariants 로 확인
  (ADR-007 winding SSOT).
- **L4 UI 5-layer 3번째 답습 (createSolidExtrude→Loft→Revolve)**: WASM/bridge/action/
  menu/CC/AC 템플릿 reproducible. 경로1↔경로2 (face capped vs edge open-surface) 공존.

## 6. 후속 (Phase 3 나머지 / E1 확장)

- **Axis-touching partial revolve** (pole): profile 가 축에 닿는 경우 (반-구 같은) —
  pole 처리 + 캡. future.
- **유연 axis pick** (2-point / edge 선택) — 현재 원점-cardinal MVP.
- **E3 guard** (create_solid Extrude multi-loop reject defense-in-depth, user 가치 0).
- **Phase 2 Punch 확장** (P1 사각 관통 등).

## 7. Cross-link

- ADR-240 (Phase 3 로드맵) / ADR-079 W-4 (revolve_profile_face) / ADR-247 (E2 loft —
  rotated-section loft + dedup-cap 패턴 source) / Mesh::revolve / Mesh::loft /
  rotate_around_axis / ADR-007 (winding) / ADR-133 (AC ⊇ CC) / ADR-046 P31 #4 /
  createSolidExtrude·createSolidLoft (UI 5-layer 템플릿). 메타-원칙 #4 (SSOT) / #6 (audit).

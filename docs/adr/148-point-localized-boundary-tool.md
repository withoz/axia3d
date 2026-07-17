# ADR-148 — B-γ' Point-Localized BoundaryTool (ADR-139 자연 후속)

**Status**: Accepted (α + β-1~β-4 + γ closure — §8 Acceptance Log 참조)
**Date**: 2026-05-26 (α) / 2026-07-17 (Status 정정 — 아래 §8 참조)
**Author**: WYKO + Claude
**Trigger**: LOCKED #65 (ADR-141 Master Roadmap) Sprint 2 마지막 ADR.
ADR-141 §3 reserve:
> "ADR-148 | B-γ' Point-Localized BoundaryTool (ADR-139 자연 후속) | S2 | 1주"
**Direct predecessor**: LOCKED #64 ADR-139 (Boundary tool 명시 only)
§14 B-γ' sub-step — "`Mesh::boundary_from_point(p, plane)` 신규 point-
based localization (full mesh sweep 보다 정밀)".
**Sprint**: S2 (ADR-141 §3 — 2~3주, 회귀 +30 share ~10).

## Canonical anchor

ADR-139 §14 B-γ MVP audit pivot (2026-05-22) 후 명시 deferred:
> "B-γ MVP 의 본질: ADR-139 가 자동 trigger 폐기 → 명시 trigger 가 필요한데,
> `resynthesize-faces` 가 *전체 mesh sweep* 명시 trigger 로 이미 활성.
> 사용자 명시 호출 = 보고서 4단계 파이프라인의 entry point.
> ...
> B-γ' (가칭) — Point-based localization (`Mesh::boundary_from_point
> (p, plane)` 신규 — click point 근처 region-limited boundary detection,
> ADR-139 §10 L-139-5 specific). Full mesh sweep 보다 정밀."

**CAD 표준 BOUNDARY 명령**: AutoCAD `BOUNDARY` / `BPOLY` / `BHATCH` 등
은 사용자가 *영역 내부의 한 점*을 클릭하면 그 점을 둘러싼 가장 작은
boundary loop 를 검출하여 결과 boundary 를 생성. 본 ADR 은 AxiA 의
equivalent.

## 1. Problem statement

### 1.1 현재 `resynthesize_orphan_faces` 한계

**현재 구현** (`crates/axia-core/src/scene.rs:resynthesize_orphan_faces`):
- 전체 mesh 의 *모든* orphan edges 수집
- `mop_up_orphan_cycles_via_dfs` 가 MAX_ROUNDS=8 동안 DFS 로 모든 cycle 발견
- 발견된 cycle 마다 face 합성

**문제점**:
1. **정밀도 부족** — 사용자가 *특정 영역* 에만 면을 만들고 싶어도 전체
   sweep 발동. 다른 영역의 unwanted face 도 합성될 수 있음.
2. **성능** — 대형 mesh (수천 orphan edges) 에서 O(N²) DFS 비용.
3. **사용자 의도 표현 한계** — "이 영역 boundary 만" 의도 표현 불가.
4. **CAD 표준 deviation** — AutoCAD/SketchUp 등 표준 BOUNDARY 는 *point-
   click* 기반.

### 1.2 메타-원칙 #16 정합 강화

ADR-139 (LOCKED #64) 의 *명시 trigger* 정책의 더 정밀한 표현:
- 현재 `resynthesize-faces` (B-γ MVP) — *전체* 명시 trigger
- **B-γ' (본 ADR)** — *국지적* 명시 trigger (사용자 의도 정밀 표현)

메타-원칙 #16: "자동화는 사용자 의도를 미리 알 수 없다 — 휴리스틱 자동화는
cascading 부작용의 source." Point-localized = *사용자가 명시 지정한 영역*
만 처리 → 다른 영역 부작용 0.

### 1.3 외부 anchor (ADR-139 §10 L-139-5)

> "L-139-5 — Algorithm = planar graph face traversal (기존 DCEL 자산
> + Cardinal projection LOCKED #63 + BVH spatial accel) — 새 알고리즘 0"

본 ADR 은 L-139-5 의 *point-localized* 변형. BVH spatial accel 자연 활용
(point proximity query).

## 2. Solution architecture

### 2.1 Engine API (Rust)

```rust
// crates/axia-geo/src/mesh.rs (또는 operations/boundary.rs 신설)

/// ADR-148 — Point-localized boundary face detection.
///
/// Given a 3D point and a plane, find the *smallest enclosing orphan
/// edge cycle* on that plane containing the point. Returns the synthesized
/// face if found, else None with diagnostic.
///
/// CAD 표준 BOUNDARY 명령 equivalent — user clicks inside a region,
/// tool finds the enclosing boundary.
///
/// # Errors
/// - `BoundaryError::PointNotOnPlane` — 점이 plane 에 평면적 (1.5μm)
/// - `BoundaryError::NoEnclosingCycle` — 점을 둘러싼 cycle 없음
/// - `BoundaryError::CycleAlreadyFaced` — cycle 이 이미 face 가짐
pub fn boundary_from_point(
    mesh: &mut Mesh,
    point: DVec3,
    plane: Plane,
    search_radius: f64, // 0 = unbounded (default 1000mm)
) -> Result<FaceId, BoundaryError>;
```

**Validation 4단계**:
1. point 가 plane 에 평면적 (LOCKED #5 ε=1.5μm)
2. orphan edges 수집 (active edges with no face)
3. 점을 둘러싼 *가장 작은* simple closed cycle 검출
4. cycle 이 이미 face 인지 검사 (중복 차단)

### 2.2 Q1 결재 anchor — 알고리즘 선택

**옵션 (a) — Smallest enclosing cycle (직접)** (정확 path):
- 모든 orphan cycle 추출 (DFS, current implementation 재활용)
- 각 cycle 에 대해 point-in-polygon 2D 검사
- 점 포함 cycle 중 area 최소값 선택
- 구현 비용 ~2-3일
- **장점**: CAD parity (AutoCAD BOUNDARY semantic), 정확
- **단점**: 모든 cycle 추출 비용 (full mesh worst case)

**옵션 (b) — Ray-casting BFS** (효율 path):
- 점에서 임의 방향으로 ray 발사
- ray 와 가장 먼저 교차하는 orphan edge 발견
- 그 edge 에서 시작하여 leftmost-turn walker 로 cycle 완성
- 구현 비용 ~3-4일
- **장점**: O(N) (cycle 추출 없음)
- **단점**: leftmost-turn walker edge case (T-junction, vertex shared)

**옵션 (c) — Hybrid (BVH + DFS)** (균형 path):
- BVH spatial query 로 *search_radius* 이내 orphan edges 만 수집
- 그 subset 에 DFS cycle finder 적용
- point-in-polygon 검사 후 smallest 선택
- 구현 비용 ~2일
- **장점**: search_radius 로 비용 제한, 기존 DFS 재활용
- **단점**: search_radius 휴리스틱 (사용자 지정 또는 default 1000mm)

**최우선 추천: (c) Hybrid** — 기존 DFS 자산 재활용 + BVH spatial accel
정합. search_radius default 1000mm (10×10×10m 작업 공간 표준).
Sprint 2 budget (1주) 정합. ADR-139 L-139-5 "기존 DCEL 자산 + BVH"
canonical 답습.

### 2.3 Q2 결재 anchor — UI integration path

**옵션 (a) — Boundary tool (ToolManager)** (CAD parity):
- 신규 `BoundaryTool` 등록 (`web/src/tools/BoundaryTool.ts`)
- Keyboard shortcut 'B' (현재 'b' = bottom view) — 충돌 → 'Ctrl+B' or 'Alt+B'
- 활성 시 cursor crosshair + 클릭 → `bridge.boundaryFromPoint(...)`
- 구현 비용 ~1일

**옵션 (b) — ContextMenu action** (단순 path):
- 우클릭 → "이 영역 면 만들기" action
- ContextMenu 가시성 — 우클릭 위치 ground plane 추론
- 구현 비용 ~30분
- **장점**: 단순, 즉시 가치
- **단점**: 우클릭 위치 = plane intersection (less explicit)

**옵션 (c) — Both** (full coverage):
- Tool + ContextMenu 둘 다 활성 (사용자 선호 path 선택)
- 구현 비용 ~1.5일

**최우선 추천: (a) BoundaryTool** — ADR-139 §14 B-ε 직계 정합:
> "B-ε — TS BoundaryTool 신규 ('B' 단축키 + cursor crosshair)"
- Shortcut: 'Ctrl+B' (bottom view 'b' 충돌 회피, CAD 관습 정합)
- 향후 ContextMenu 도 추가 가능 (메타-원칙 #5 사용자 편의)

### 2.4 BoundaryError 4 variants

```rust
pub enum BoundaryError {
    /// 점이 plane 에 평면적 (LOCKED #5 ε=1.5μm 초과)
    PointNotOnPlane { distance_mm: f64 },
    /// 점을 둘러싼 orphan cycle 없음 (free space click)
    NoEnclosingCycle,
    /// 발견된 cycle 이 이미 active face
    CycleAlreadyFaced { existing_face_id: FaceId },
    /// search_radius 내 orphan edges 0 (작업 영역 비어 있음)
    NoOrphanEdgesInRadius,
}
```

Toast.error 매핑 (한국어):
- PointNotOnPlane → "클릭 위치가 평면 위가 아닙니다 (거리 {N}mm)"
- NoEnclosingCycle → "이 영역을 둘러싼 boundary 가 없습니다"
- CycleAlreadyFaced → "이 영역에 이미 면이 있습니다"
- NoOrphanEdgesInRadius → "주변에 boundary 후보가 없습니다 (반경 {R}mm 확대 필요)"

## 3. Sub-step plan (Path Z atomic)

### 3.1 Plan 매트릭스

| Sub-step | Scope | 비용 | 회귀 |
|---|---|---|---|
| **α** | 본 ADR spec (본 commit) | ~30분 | 0 |
| **β-1** | Engine API skeleton + BoundaryError + 4 validation | ~1일 | axia-geo +4 |
| **β-2** | Engine algorithm (Hybrid BVH + DFS, Q1=c) | ~2일 | axia-geo +3 (happy path + smallest selection) |
| **β-3** | WASM bridge + TS wrapper | ~30분 | axia-wasm +2 + vitest +2 |
| **β-4** | UI BoundaryTool (Q2=a, Ctrl+B) | ~1일 | vitest +3 |
| **γ** | E2E (Playwright) + closure docs (Status flip + §9 Lessons) | ~1시간 | Playwright +1 |
| **합계** | **~4-5일 (Sprint 2 1주 share)** | | **+15** |

### 3.2 Path Z atomic 답습

ADR-145 (Circle annulus) / ADR-146 (Step 1 Inferencing) / ADR-139 (Boundary
tool) 패턴 답습 — sub-step 별 single atomic PR.

### 3.3 회귀 추정

axia-geo +7 / axia-wasm +2 / vitest +5 / Playwright +1 = **+15 total**.
ADR-141 §3 Sprint 2 share +30 의 ~50% (ADR-147 자연 분담 +15).

## 4. Lock-ins

- **L-148-1** ADR-139 명시 trigger only — 사용자 클릭 = 명시 의도
  (휴리스틱 자동 활성 0, 메타-원칙 #16 정합)
- **L-148-2** ADR-139 L-139-5 정합 — 기존 DCEL + BVH 자산 재활용
  (새 알고리즘 0, Hybrid Q1=(c) 채택 근거)
- **L-148-3** LOCKED #5 정합 — point-plane proximity 1.5μm spatial-hash
  tolerance
- **L-148-4** LOCKED #63 z=0 invariant 정합 — Cardinal plane snap 시
  자동 Z=0 plane 추론 (사용자 facing 단순화)
- **L-148-5** LOCKED #44 (Complete Meaning per Merge) — 각 sub-step
  single atomic PR
- **L-148-6** LOCKED #66 (Sunset Policy) — α "Proposed" / γ "Accepted"
- **L-148-7** 절대 #[ignore] 금지 — 15 회귀 자산 모두 enabled
- **L-148-8** ADR-046 P31 #4 additive only — `Mesh` / `WasmBridge`
  signature UNCHANGED, 새 method 추가만
- **L-148-9** Keyboard shortcut canonical — Ctrl+B (bottom view 'b'
  충돌 회피, CAD 관습 정합)
- **L-148-10** BoundaryError variants 4 명시 — silent skip 차단,
  사용자 facing Toast.error 한국어 매핑

## 5. Out of scope (별도 ADR)

- ~~**Multi-loop boundary** (ring with holes) — 본 ADR 은 single simple
  closed cycle. Multi-loop ADR-016 Q2 정책 정합 별도.~~ → **2026-07-17 landed**,
  아래 §8 참조. ADR-016 Q2 는 multi-loop face 를 *쓰는 도구* (Boolean /
  Offset) 의 제약이지 *만드는 것* 의 금지가 아니다 — `merge-as-hole` 이 이미
  만든다. 새 정책 결정이 없으므로 새 ADR 이 아닌 본 로그로 기록.
- ~~**3D BOUNDARY** (closed shell from orphan faces) — ADR-139 §14 B-μ,
  별도 ADR~~ → **2026-07-17 landed** (선택 semantic), 아래 §8 참조.
- ~~**Auto plane inference** (사용자 점만 클릭, plane 자동 추론) —
  ADR-141 §3 reserve 외부, future ADR~~ → **2026-07-17 landed**, 아래 §8 참조.
- ~~**ContextMenu integration** — Q2=(a) BoundaryTool 우선, ContextMenu
  는 follow-up ADR~~ → **2026-07-17 landed** (Q2=(c) Both 완성), 아래 §8 참조.

## 6. Cross-link

- **ADR-141** (Master Roadmap S2)
- **ADR-139** (LOCKED #64 Boundary tool 명시 only — 직계 predecessor)
- **ADR-146** (Step 1 Inferencing — Sprint 2 첫 ADR closure)
- **ADR-016** (Multi-loop face Q2 — 별도 deferred)
- **외부 anchor**: ADR-139 §14 B-γ' sub-step + reports/입력보정파이프라인_
  적용계획.html §priority P9~P12
- **LOCKED #5** (1.5μm spatial-hash — proximity tolerance)
- **LOCKED #44** (Complete Meaning per Merge)
- **LOCKED #63** (z=0 invariant — Cardinal plane snap)
- **LOCKED #64** (ADR-139 — direct predecessor)
- **LOCKED #65** (ADR-141 Master Roadmap S2 reserve)
- **LOCKED #66** (ADR-164 Sunset Policy)
- **메타-원칙 #5** (사용자 편의 — 명시 trigger)
- **메타-원칙 #14** (면은 닫힌 경계로부터 유도된다 — Jordan-Schoenflies)
- **메타-원칙 #16** (자동화 antipattern — 사용자 클릭 명시 의도)

## 7. Sub-step roadmap

| Sub-step | Scope | 회귀 | 비용 |
|---|---|---|---|
| **α** | 본 ADR spec (본 commit) | 0 | ~30분 |
| **β-1** | Engine API skeleton + BoundaryError + 4 validation | axia-geo +4 | ~1일 |
| **β-2** | Engine algorithm (Hybrid Q1=c) | axia-geo +3 | ~2일 |
| **β-3** | WASM bridge + TS wrapper | axia-wasm +2 + vitest +2 | ~30분 |
| **β-4** | UI BoundaryTool (Q2=a, Ctrl+B) | vitest +3 | ~1일 |
| **γ** | E2E + closure docs | Playwright +1 | ~1시간 |
| **합계** | | **+15** | **~4-5일** |

각 sub-step single atomic PR (LOCKED #44).

## 8. Acceptance Log

- **2026-05-26 α** (본 commit) — α spec + Q1/Q2 결재 anchor + sub-step
  plan + lock-ins.
- **(β-1 ~ γ, ~4-5일)** — 별도 사용자 결재 후 진행 (Q1 결정 + Q2 결정).
  - > ✅ 진행됐다. 아래 2026-07-17 항목 참조 — 이 줄은 α 시점의 기록으로
    > 보존한다.

- **β-1 ~ γ** — landed. 개별 commit hash 는 `155e127` (clean baseline,
  squashed from adr-186 @ 195755d) 에 흡수되어 남아 있지 않다. 코드가
  증거이므로, 2026-07-17 에 실물을 세어 기록한다:

  | sub-step | 산출물 | 실측 |
  |---|---|---|
  | β-1/β-2 | `crates/axia-geo/src/operations/boundary.rs` — `boundary_from_point` + BoundaryError 4 variants | 회귀 6 |
  | β-3 | WASM `boundaryFromPoint` export + `WasmBridge.boundaryFromPoint` | vitest (WasmBridge) |
  | β-4 | `web/src/tools/BoundaryTool.ts` + Ctrl+B (L-148-9 canonical) | `BoundaryTool.test.ts` 16 tests |
  | γ | `web/e2e/adr-148-boundary-demo.spec.ts` | **2/2 PASS** (real Chromium, 2026-07-17 재실행) |

  Q1 = (c) Hybrid, Q2 = (a) BoundaryTool — 두 권장안 모두 코드에 반영된
  상태로 landed.

- **2026-07-17 — Status 정정 + UI 노출** (사용자 결재: "문서 정리 진행").

  Status 가 α 시점 `Proposed` 에 멈춰 있었다. STATUS-POLICY §3.1 의
  `Draft → Accepted` 요건 (β closure + 사용자 시연 게이트 PASS + §D
  Acceptance Log) 은 이미 충족돼 있었고, 문서만 따라오지 못했다
  (LOCKED #88 doc-lag 패턴). 본문은 수정하지 않았다 — Status line 과 본
  로그만 갱신 (§3.2 retroactive 수정 금지 / §3.3 additive 허용).

  같은 날 배선 감사에서 드러난 것: 도구는 Ctrl+B 로 계속 작동하고
  있었으나 **메뉴 · 툴바 · 카탈로그 · 도움말 어디에도 없었다.** 즉
  이미 알고 있는 사람만 쓸 수 있는 상태. β-4 가 "UI BoundaryTool" 로
  계획됐지만 실제로 landed 한 것은 키 바인딩까지였고, 발견 경로는
  빠져 있었다. 2026-07-17 commit `553aedb` 가 메뉴 항목 +
  ActionCatalog/CommandCatalog 등재 + 도움말 행 + 상태바 라벨 갱신을
  추가하고, "등록된 도구는 어디선가 도달 가능해야 한다" 회귀 가드를
  세웠다.

  이름은 「영역 클릭 → 면 (Boundary · BPOLY)」 — `resynthesize-faces`
  가 LOCKED #64 B-γ 로 「경계 도구」를 이미 쓰고 있어 팔레트 검색에서
  충돌한다. 두 연산은 다르다 (전체 mesh sweep ↔ 클릭한 영역 하나).

- **2026-07-17 — ContextMenu 진입점** (사용자 결재: "ContextMenu 진입점
  진행"). §5 가 "follow-up ADR" 로 남긴 항목 — §2.3 에서 이미 옵션 (b) 로
  검토·비용 산정된 것을 (c) Both 로 채우는 확장이라 새 결정이 아니므로 본
  로그로 기록한다 (STATUS-POLICY §3.3 additive).

  `ctx-item[data-action="boundary-here"]` → `ToolManager.synthesizeBoundaryAt
  (x, y)` → **BoundaryTool.onMouseDown 재사용**. plane 해소 (getDrawPlane →
  face-hit / lock / sticky / Z=0), normalizeDrawInput chokepoint,
  BoundaryError → 한국어 매핑이 전부 거기 있으므로 두 번째 사본을 만들지
  않았다 (메타-원칙 #4).

  두 진입점의 의미 차이: **Ctrl+B** 는 도구에 들어가 클릭을 기다리고,
  **우클릭 → 면 만들기** 는 이미 우클릭한 지점에 즉시 합성한다 (도구 전환
  없음). 위치는 `viewport.onContextMenu(x, y)` 가 주는 것을 저장해 쓴다 —
  메뉴 항목의 클릭 좌표는 화면의 다른 곳이라 쓸 수 없다.

  `boundary-here` 는 ActionCatalog `surfaces: ['context-only']` 로만 등재
  하고 CommandCatalog 에는 넣지 않았다: 팔레트 호출에는 우클릭 위치가 없다.

  검증 — vitest ContextMenu +3 (위치 전달 / 최신 우클릭 / 메뉴 미개방 시
  no-op), 뮤테이션 2/2 (위치 저장 제거 · 메뉴 항목 좌표 오용). 실제 앱:
  우클릭 → 항목 클릭 → 엔진 응답 Toast, 도구는 select 유지. 성공 합성
  자체는 γ E2E (2/2) 가 이미 덮는다 — 브라우저에서는 자동 면 합성
  (LOCKED #76) 이 닫힌 loop 를 즉시 면으로 만들어 orphan edge 가 남지 않는다.

- **2026-07-17 — Multi-loop (island → hole)** (사용자 결재: "multi-loop 진행").

  De-risk 먼저 (메타-원칙 #6): 사각형 안 사각형, 링 영역 클릭 → 측정 결과
  **outer=4 / inners=0**. 즉 single-loop 구현은 island 를 **덮어버리는**
  solid face 를 만들고 있었다 (AutoCAD BPOLY 는 ring). 가설이 아니라 실측
  결함. 그 de-risk 테스트가 지금은 (4, 1) 을 단언하는 회귀가 됐다.

  알고리즘은 이미 **모든 cycle 을 추출** 하고 point-in-polygon 으로 걸러
  smallest 를 고르고 있었다 — island 후보가 이미 손에 있었다는 뜻. 추가한
  것은 (1) cycle 투영 1회 재사용 (outer 선택 ↔ island 검출 drift 차단),
  (2) outer 안에 완전 포함 + 더 작은 cycle = hole 후보, (3) **nested 제외**
  (다른 hole 안에 있으면 그건 한 단계 아래 ring 의 몫 — BPOLY "Outer"
  island style), (4) holes 있으면 `add_face_with_holes` (없으면 기존
  `add_face` 그대로). winding/normal 검증은 그 API 가 하므로 두 번째 경로를
  만들지 않았다 (ADR-007 Invariant 2).

  **ADR-016 Q2 정합**: Q2 는 multi-loop face 를 *쓰는* 도구 (Boolean /
  Offset) 의 거부 정책이고 그건 불변. Push/Pull 은 ADR-191 (LOCKED #79) 로
  이미 해제. 본 변경은 face 를 *만드는* 쪽이라 Q2 를 건드리지 않는다.

  회귀 axia-geo +5 (ring → hole / island 클릭 시 island 만 / plain square
  무회귀 / disjoint 2 islands → 2 holes / nested 는 우리 hole 아님). 뮤테이션
  2/2 — island 검출 제거 시 ring 2건 FAIL, nested 필터 제거 시 nested 가
  left:2 right:1 로 FAIL. **첫 뮤테이션은 CRLF 앵커 실패로 적용되지 않은 채
  "통과" 했고, 그때의 초록은 아무것도 증명하지 못했다** — 다시 걸어서 확인.
  axia-geo 2253 / axia-core 440 green.

  **브라우저 시연은 불가** (정직 기록): 자동 면 합성의 earlier stage
  (Step 4.5/4.6/4.9) 가 닫힌 loop 를 즉시 면으로 만들어 orphan edge 를 남기지
  않는다 — flag (LOCKED #76 / ADR-139 B-β-2) 는 Step 4.95/4.99 만 gate 한다.
  두 사각형을 그리면 faces=2 가 되어 Boundary 가 볼 orphan 이 없다. 실사용
  trigger 는 STEP/IGES import 나 erase 로 face 만 사라진 mesh 처럼 orphan 이
  실재하는 경우. 엔진 회귀가 authoritative.

- **2026-07-17 — 3D BOUNDARY** (사용자 결재: "닫힌 껍질의 면들을 선택").

  ADR-139 §14 의 B-μ 는 비용이 "future" 한 단어뿐, **무엇을 만드는지 적혀
  있지 않았다**. audit 결과 만들 것이 없다는 게 답이었다 — `Volume` 은
  계산되는 *상태* 지 엔티티가 아니고 (CLAUDE.md Geometry Layer),
  `entities/shell.rs` (84줄) 는 `add_shell` 호출자 0 · 스냅샷 미포함의 dead
  code 다. 껍질이 닫혔다는 건 **이미 참인 사실**이다. 그래서 4 옵션 (선택 /
  Shell 활성 / XIA 승격 / 보류) 중 **선택** 결재.

  `shell_from_point(&Mesh, point) -> Result<Vec<FaceId>, ShellError>` —
  **read-only**. 새 엔티티 0 이므로 LOCKED #26 시민권도 ADR-016 Q2 도
  건드리지 않는다. 자산 재사용: `face_connected_components` (edge-연결
  그룹) + `is_face_set_closed_solid` (watertight) + `boolean_geo::
  point_in_solid` (3-ray 다수결) — 셋 다 이미 있었고 새 알고리즘은 0.
  중첩 솔리드는 2D 와 같은 smallest-first (bbox 부피 랭크).

  배선: WASM `shellFromPoint` (transaction 없음 — 선택에 undo 항목을 남기면
  안 된다) → bridge (markDirty 없음) → `ToolManager.selectShellAt` (pick 후
  view ray 방향으로 0.01mm 안쪽 샘플 — 경계 위는 ray 판정이 모호하다) →
  ContextMenu `select-shell-here`. ActionCatalog tier 0 / context-only
  (우클릭 위치가 필요하므로 팔레트 불가).

  회귀 axia-geo +5 (박스 안 6면 / 밖 / open shell 은 solid 아님 / nested 는
  inner 선택 + gap 은 outer / empty mesh) · axia-wasm +1 (read-only 계약:
  `&self` + transaction 없음) · vitest ContextMenu +3. 뮤테이션 4/4 —
  closed 필터 제거 / smallest→largest / clearSelection 제거 / 빈 shell 가드
  제거 모두 FAIL 확인.

  **실제 앱 검증** (2D 와 달리 여기서는 가능했다 — 자동 면 합성과 무관):
  box 안 → 6면 선택 + "솔리드 선택: 6개 면", 빈 공간 → 0 선택 +
  "닫힌 솔리드 안이 아닙니다", en 로케일 한글 누출 0.

- **2026-07-17 — Auto plane inference** (사용자 결재: "auto plane inference
  진행").

  **메타-원칙 #16 과 충돌하지 않는다**. Boundary 는 *면이 아직 없는 곳* 에서
  쓰는 도구인데, 바로 그래서 `getDrawPlane` 의 cascade 에 구멍이 있다 —
  평면을 알려줄 face-hit 분기가 발동할 수 없고, z=100 에 그린 loop 는 Z=0
  으로 떨어져 아예 면이 안 된다 (sticky 가 우연히 맞지 않는 한).

  근방 free edges 가 **한 평면에 있으면 그 평면은 추측이 아니라 기하가
  허용하는 유일한 답**이다. 서로 다르면 고르지 않고 거부한다 — 이게
  메타-원칙 #5 (명확하면 자동, 모호하면 명시) 이고, #16 이 경고하는 휴리
  스틱은 바로 *고르는* 쪽이다. `Ambiguous { plane_count }` 는 그래서 기능
  이지 한계가 아니다.

  자산 재사용: ADR-080 V-δ-α `derive_free_wire_plane` (free edge BFS +
  best-fit plane + planarity 게이트 ≥3 verts / non-collinear / scale-aware
  RMS) 를 `pub(crate)` 로 올려 그대로 씀 — 두 번째 구현은 드리프트한다
  (메타-원칙 #4). 신규 알고리즘 0.

  `infer_plane_from_point(&Mesh, point, radius) -> Result<Plane,
  InferPlaneError>` + `boundary_from_point_auto_plane(&mut Mesh, point,
  radius)`. WASM `boundaryFromPointAutoPlane` (plane 인자 **없음** — 그게
  요점이고 회귀가 그걸 단언한다) → bridge → BoundaryTool 이 auto 먼저,
  거부 시 draw plane fallback (lock/sticky 는 사용자가 *표명한* 의도이므로
  정직한 2순위). 두 경로 모두 실패하면 **나중** 에러를 보여준다 — 사용자가
  실제로 있는 평면이 왜 안 됐는지가 필요한 정보다.

  회귀 axia-geo +6 (z=100 wire plane 추론 / 두 평면 → Ambiguous / **같은
  평면 두 wire 는 모호 아님** / 반경 밖 / e2e z=100 면 합성 / 거부 시 면
  0개) · axia-wasm +1 (plane 인자 부재 + transaction 계약) · vitest
  BoundaryTool +3. 뮤테이션 4/4 — 모호해도 첫 평면 선택 / 같은 평면 dedup
  제거 / auto 경로 제거 / fallback 제거 모두 FAIL.

  **브라우저 한계 (정직 기록)**: WASM 도달은 확인 (엔진이
  `NoOrphanEdgesInRadius` 로 응답). 다만 orphan closed loop 를 브라우저에서
  만들 수 없다 — 자동 면 합성의 earlier stage (4.5/4.6/4.9) 는 flag 와
  무관하게 닫힌 cycle 을 즉시 면으로 만든다 (LOCKED #64 B-β-2 가 기록한
  동작). z=100 케이스는 Rust 회귀가 직접 증명한다.

---

~~**다음 trigger**: β-1 진입 결재 (Q1 → 옵션 (c) Hybrid 권장 / Q2 →
옵션 (a) BoundaryTool 권장) 또는 Sprint 2 잔존 ADR-147 (Step 2
Scenario B1) 진입.~~ — α 시점 기록. 두 권장안 모두 채택되어 landed.

**남은 것**: 없음 — §5 의 4 항목 (multi-loop / 3D BOUNDARY / ContextMenu /
auto plane inference) 모두 2026-07-17 closure. 각 항목의 근거는 §8 참조.

> 앞선 판(auto plane 시점)은 "모두 closure" 라 써놓고 같은 문장에서
> "multi-loop 은 남는다" 고 했다. multi-loop 은 그때 이미 들어와 있었고
> (`6dbb76a`, 같은 날), 그 문장이 stale 이었다.

# ADR-287 — Curved cut/boss ε: Sphere / Cone / Torus (unified surface-normal offset)

- **Status**: Accepted (α + β landed 2026-07-10 — Cone + Torus cut/boss; Sphere deferred, self-loop shared boundary)
- Date: 2026-07-10
- Track: ADR-286 §E (ε — Sphere/Cone/Torus boss+cut) + ADR-271 §ε (cut). "완벽한 extrude" 로드맵 #5 곡면 마무리.
- Cross-link: ADR-286 (Cylinder boss, LOCKED #89), ADR-271 (Cylinder cut),
  ADR-263 (곡면 sketch-split — cap 생성 all 4 surfaces, LOCKED #87), ADR-089
  A-χ (surface 상속), ADR-267/273 (watertight/SI gate), ADR-190 P0.2
  (snapshot rollback), 메타-원칙 #4 #5 #6 #14.

---

## 1. Canonical anchor (사용자 결재, 2026-07-10)

AskUserQuestion "다음 작업" → **곡면 cut/boss ε 확장 (Sphere/Cone/Torus)**.

## 2. Measure-first 감사

- **현재 상태**: `carve_curved_pocket`(ADR-271) + `add_curved_boss`(ADR-286)
  둘 다 **Cylinder-surface arm 만 구현**, 나머지 `_ => bail!`.
- **Cylinder 로직**: opening loop 를 **per-vertex 축-radial** 로 ∓depth
  오프셋 → floor/roof 는 `Cylinder{radius∓depth}`. Cylinder 의 축-radial =
  surface normal 이므로 이미 "normal offset" 의 특수 케이스.
- **핵심 wiring 발견**: `PushPullTool.isCurvedCap = surfKind >= 2` 는 이미
  **Sphere(3)/Cone(4)/Torus(5) 모두 포함** → carveCurvedPocket/Boss 호출.
  엔진이 non-Cylinder 를 bail 할 뿐. **⇒ ε 확장은 engine-only** (tool /
  WASM / bridge 모두 이미 general, 변경 0).
- **cap 생성 자산**: ADR-263 이 4 곡면 모두 sketch-split(cap+remainder)
  제공 (drawCircleOn{Sphere,Cylinder,Cone,Torus}).
- **geometry 자산**: 각 곡면 `normal()` / `evaluate()` / `project_to_*()`
  모두 존재 (surfaces/{sphere,cone,torus}.rs).

## 3. 통합 전략 — surface-normal offset

opening vert 를 **per-vertex surface normal** 방향으로 ∓depth 오프셋 →
floor/roof face 는 **동일 surface type 의 offset 파라미터** 상속:

| Surface | offset 방향 | floor/roof surface | depth 상한 |
|---|---|---|---|
| Cylinder | 축-radial (=normal) | `radius ∓ d` | `d < radius` (기존) |
| Sphere | center-radial (=normal) | `radius ∓ d` | `d < radius` |
| Torus | tube-circle radial (=normal) | `minor_radius ∓ d` | `d < minor_radius` |
| Cone | ∥ surface normal | apex 를 axis 방향 `∓d/sinα` 이동, half_angle 불변 | `d < v_min·sinα` (apex 안 넘김) |

**Cone parallel-cone 도출 (de-risk-on-paper)**: cone point
`P = apex + axis·v + radial·(v·tanα)`, normal `n = cosα·radial − sinα·axis`.
inward 오프셋 `P' = P − d·n = apex + axis·(v+d·sinα) + radial·(v·tanα − d·cosα)`.
`apex' = apex + axis·(d/sinα)`, `v'' = v − d·cos²α/sinα` 로 두면 모든 v 에
대해 `P' = apex' + axis·v'' + radial·(v''·tanα)` — **동일 half-angle α 의
parallel cone**. 즉 cone cap 의 모든 opening vert 가 하나의 offset cone 에
정확히 안착 (floor 가 단일 Cone surface).

**Topology 불변**: 4 곡면 모두 Cylinder pocket/boss 와 **동일 DCEL 수술**
(remove cap → N side wall welds to remainder hole-loop → floor/roof cap).
manifold by construction (welding 이 winding 강제, ADR-286 β-1 finding).

## 4. 결재 필요 (Q1~Q5)

- **Q1 (scope/순서)**: (a) 단일 ADR, β sub-step 곡면별 (β-1 Sphere → β-2
  Torus → β-3 Cone, 각 pocket+boss) — **추천** (통합 approach + 곡면별 atomic
  de-risk). / (b) 곡면별 별도 ADR (ADR-113/114/115 패턴). / (c) 3 곡면 한
  번에.
- **Q2 (offset 전략)**: (a) per-vertex surface normal (통합) — **추천**. /
  (b) per-surface 개별 radial 공식.
- **Q3 (구조)**: (a) 공유 core helper `curved_carve_core(cap, offset_pts,
  floor_surface)` 추출 + 곡면별 offset_pts/surface 계산 — **추천** (DRY,
  ADR-091 §E L4 pure helper). / (b) 곡면별 arm 복제.
- **Q4 (범위)**: (a) pocket(cut) + boss 둘 다, 4 곡면 (Cylinder 포함 정합)
  — **추천**. / (b) boss 만 / cut 만.
- **Q5 (UI)**: (a) 변경 0 — PushPullTool isCurvedCap 이미 general, engine-
  only — **추천**.

## 5. Lock-ins (β 확정, 결재 후)

- **L-287-1** 통합 = per-vertex surface normal offset (Q2-a).
- **L-287-2** floor/roof = 동일 surface type offset 파라미터 (Sphere r∓d /
  Torus minor∓d / Cone parallel apex-shift). ADR-089 A-χ 상속.
- **L-287-3** 공유 core helper (Q3-a) — Cylinder 기존 로직도 core 로 수렴
  (회귀 자산 보존 확인).
- **L-287-4** per-surface depth 상한 가드 (§3 표) — 초과 시 bail + snapshot rollback.
- **L-287-5** watertight (ADR-267) + SI (ADR-273) + verify_face_invariants +
  floor/roof normal 방향 명시 검증 (ADR-268 topology≠orientation).
- **L-287-6** engine-only (ADR-046 P31 #4 additive) — tool/WASM/bridge/menu 무변경.
- **L-287-7** 절대 #[ignore] 금지. de-risk sim(Sphere+Cone) + E2E(real Chromium) + 시연.

## 6. Roadmap (β 결재 후)

- β-1 Sphere (core helper 추출 + Sphere pocket+boss + de-risk) + 회귀
- β-2 Torus (minor∓d arm) + 회귀
- β-3 Cone (parallel apex-shift arm) + 회귀
- β-4 E2E (draw circle on {sphere,cone,torus} → push in/out → manifold) + 시연
- β-5 closure docs + LOCKED

## 7. de-risk / Sphere 지연 근거

- **Cone de-risk 확정**: `adr287_curved_pocket_boss_cone` 이 floor verts 를
  parallel cone (`apex + ad·(depth/sin α)`) 에 대해 `project_to_cone` round-
  trip (< 1e-6) 로 검증 → §3 apex-shift 도출 **성립 확정**.
- **Sphere 지연 (measure-driven)**: sphere cap 은 ADR-089 **closed-curve
  self-loop** (planar latitude circle = 1 anchor + self-loop Circle edge).
  그 self-loop edge 는 annulus 의 inner hole 과 **공유**됨. cap 만 polygonize
  하면 공유 edge 가 제거되어 annulus 와 desync → `verify_face_invariants`
  는 valid 지만 `is_closed_solid` **false** (open boundary). 실측으로 확인.
  올바른 해법 = "공유 self-loop boundary 를 N-edge 로 densify" (cap+annulus
  동시) — 별도 sub-step (ADR-287 ε-sphere). Cone/Torus 는 N-vert polyline cap
  (ADR-263 geodesic) 이라 이 문제 없음 → 본 ADR 에서 완결.

## D. Acceptance Log (2026-07-10, β landed — Cone + Torus)

- **β (core + Cylinder refactor + Cone + Torus)** — `carve.rs`:
  - `curved_carve_core(cap, op_name, offset_fn, floor_surface)` 신규 (Q3-a
    shared helper) — surface-agnostic 위상 수술 (remove cap → N wall weld →
    floor/roof cap). Cylinder pocket/boss 를 core 로 refactor (기존 회귀
    `adr271_*` / `adr286_*` 모두 PASS = 무회귀).
  - `carve_curved_pocket` / `add_curved_boss` 를 surface-match dispatch 로
    재작성 — Cylinder / **Cone (parallel apex-shift)** / **Torus (minor∓d)**
    arm. offset = per-vertex surface normal (Q2-a). floor/roof = 동일 surface
    type offset param (ADR-089 A-χ).
  - Sphere arm 은 deferred bail (self-loop, §7).
- **회귀 (절대 #[ignore] 금지)**: axia-geo +2 — `adr287_curved_pocket_boss_cone`
  (floor verts on parallel cone via project_to_cone round-trip = apex-shift
  de-risk + manifold + closed-solid + Cone inherit) + `adr287_curved_pocket_
  boss_torus` (minor∓d + manifold + closed-ness preserved vs baseline +
  Torus inherit + depth>minor reject). Cylinder 기존 회귀 무변경 PASS.
- **E2E (real Chromium production)**: `web/e2e/adr-287-curved-cut-boss-cone-
  torus.spec.ts` 4 tests (cone pocket/boss + torus pocket/boss, walls>0 +
  manifold valid 0 viol + faces↑). 4/4 PASS.
- **wiring**: engine-only (Q5-a). PushPullTool `isCurvedCap = surfKind>=2` 가
  이미 cone(4)/torus(5) 포함 → carveCurvedPocket/Boss 호출. WASM/bridge/tool/
  menu **변경 0**. 사용자 push in→pocket / push out→boss 즉시 활성.
- **dev-preview 시연**: cone pocket 64 walls manifold valid + camera far↔near
  swings → panic 0, engine responsive (LOCKED #89 LOD fix 와 정합).
- **sweep**: cargo workspace **3005 passed / 0 failed / 1 ignored**.

## E. 남은 트랙 (별도 ADR)

- **ε-sphere**: sphere cut/boss — 공유 self-loop boundary densify (cap+annulus
  동시 N-edge). §7 근거.
- Live curved pocket/boss preview (현재 commit-only, ADR-193 답습).
- Cone/Torus through-hole (현재 Cylinder 만, depth≥radius auto-route).

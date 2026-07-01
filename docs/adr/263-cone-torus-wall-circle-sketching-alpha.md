# ADR-263 — α spec: Cone + Torus Wall Circle Sketching (P3-C, foundation completion)

- **Status**: Accepted (α + β-1~β-6 + γ + δ closure 2026-06-26 — Cone+Torus
  벽 원 sketch end-to-end. 4 곡면 프리미티브 전부 sketch-split = #5 곡면
  Phase 0 foundation 완성. §9 Acceptance Log)
- **Date**: 2026-06-26
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL, push 금지)
- **Track**: 6 (Extrude/Cut/Punch) — "완벽한 extrude" 로드맵 **#5 (곡면) Phase 0:
  곡면 sketch-split foundation 완성**
- **Author**: WYKO + Claude (audit-first de-risk + empirical live probe)

## 1. Context — audit-first 발견 (Pattern-12)

사용자 결재: "#5 곡면 extrude/cut → Phase 0 곡면 sketch-split foundation →
(audit) Cone+Torus 원 sketch (foundation 완성)".

**audit-first 발견 (ADR-131 패턴)**: 곡면 원 sketch-split foundation 은
이미 **절반 done** —
- **Sphere** ✅ `drawCircleOnSphere` (ADR-202, LOCKED #83)
- **Cylinder** ✅ `drawCircleOnCylinder` (ADR-257, Accepted 2026-06-25 — 6-layer
  + E2E + 본 세션 라이브 재검증: `drawCircleOnCylinder(2,[500,0,500],[500,0,600])`
  → `{cap:3, annulus:2}` fc 3→4 manifold valid v=0)
- **Cone** ❌ `drawCircleOnCone` 부재 = gap
- **Torus** ❌ `drawCircleOnTorus` 부재 = gap

ADR-257 §11 명시: "Cone / Torus 벽 sketching (β-4 UV-earcut 은 Cone/Torus 도
developable/quad-param 이라 mirror 가능 — 별도 ADR)". **본 ADR = 그 별도 ADR**.
Cone+Torus done → 4 곡면 프리미티브 전부 sketch 가능 = foundation 완성 → cut(#5
Phase 1) / boss(#5 Phase 2) prerequisite 충족.

## 2. De-risk 결과 (코드 + 라이브 probe)

### 2.1 split 은 surface-agnostic (mesh.rs:3660 `split_cylinder_face_by_circle`)
대부분 **generic DCEL surgery** — add_vertex dedup + add_edge N-loop +
`add_face_with_holes`(cap) + **N-edge twin-HE reparent**(annulus.rs 1-edge →
N-edge 일반화). surface-specific 부분은 얇은 shell 3개뿐:
1. surface match arm (`Cylinder{..}` → params),
2. on-surface 검증 + wrap-guard (`project_to_cylinder`),
3. outward orientation (centroid 의 `cylinder::normal` vs Newell).
→ cone/torus split = **projection + normal + wrap-guard 만 교체**.

### 2.2 geometry 가 유일한 신규 작업
기존 자산: `project_to_cylinder`/`circle_on_cylinder`(ADR-257),
`circle_on_sphere`(ADR-202). **cone/torus 용은 신규**:
- **Cone (developable)**: `P(u,v)=apex+v·axis+v·tanα·radial(u)`, v=apex로부터
  축거리, 슬랜트 L=v/cosα. **부채꼴 unroll 등거리** — 점 (L, u) → flat polar
  (L, u·sinα). 둘레 u∈[0,2π] → flat 각 [0, 2π·sinα]. geodesic 원 = flat 원 →
  map-back. **cylinder 와 동급 정확(exact geodesic)**.
- **Torus (비-developable)**: `P(u,v)=center+radial(u)·(R+r·cosv)+axis·(r·sinv)`,
  u=major/v=minor. Gaussian 곡률 ≠ 0 → 등거리 unroll **불가**. **param-space
  (u,v) 원** (metric-scaled: du=ρcosθ/(R+r·cosv₀), dv=ρsinθ/r) → map-back.
  근사 geodesic (true 아님) — MVP sketch region 정의에 충분 (ADR-257 §11
  "quad-param").

### 2.3 render = UV-earcut (ADR-257 §10 L-257-1 확정)
ADR-257 의 cylinder render 는 Sutherland-Hodgman 이 아닌 **UV-earcut**
(developable unroll → `earcutr::earcut(uv-polygon, holes, 2)` → map-back).
- **Cone**: 부채꼴 unroll (developable) → UV-earcut. cylinder 와 동일 패턴.
- **Torus**: (u,v) param rect (quad-param) → UV-earcut. seam-wrap 처리.

## 3. Decision (lock-in)

- **D1 scope = Cone + Torus 원 sketch-split** (extrude 없음, cap+remainder).
  ADR-202/257 1:1 mirror. cut/boss 는 orthogonal (#5 Phase 1/2, defer).
- **D2 Cone curve repr = exact geodesic** (부채꼴 unroll, developable —
  cylinder 와 동급 정확). **Torus = param-space (u,v) 원** (비-developable,
  metric-scaled 근사). sphere(analytic Circle) / cylinder·cone(developable
  geodesic) / torus(param approx) 의 3-tier 자연 분류.
- **D3 render = UV-earcut** 둘 다 (cone 부채꼴 / torus (u,v) rect). ADR-257
  §10 L-257-1 답습 — Sutherland-Hodgman 불필요 (de-risk 완료, render 강등
  gate 없음).
- **D4 split = per-surface mirror** (`split_cone_face_by_circle` /
  `split_torus_face_by_circle`) — ADR-257 1:1, 최저 위험. generic
  `split_curved_face_by_circle` refactor 는 defer (cylinder rework 위험 회피).
- **D5 surfaceKind dispatch** — Cone = kind 4, Torus = kind 5 (DrawCircleTool
  parallel branch, cylinder kind 2 / sphere kind 3 답습).
- **D6 ownership dual-path** (Shape + XIA, ADR-257 L-257-2 답습) — primitive
  (create_cone/torus→XIA) + form-layer draw(→Shape) 둘 다 reconcile.
- **D7 surface 상속** — cap + remainder 둘 다 host 의 Cone/Torus surface 상속
  (ADR-089 A-χ).
- **D8 guards** — Cone: ρ vs 부채꼴 wrap (u 각 span < π·sinα 환산) + apex
  singularity (v>0); Torus: u-wrap AND v-wrap (≥π span graceful None).

## 4. 6-Layer reuse map (ADR-257 cylinder 1:1 mirror)

| Layer | Cone | Torus | 변경 |
|---|---|---|---|
| L1 project | `project_to_cone` 신규 | `project_to_torus` 신규 | foot/슬랜트(cone), (u,v) 역산(torus). cone.rs/torus.rs `evaluate`/`normal` 재사용 |
| L1 curve-gen | `circle_on_cone` 신규 (부채꼴 unroll) | `circle_on_torus` 신규 (param-space) | `circle_on_cylinder` 답습; segment_count_for_arc + clamp 24-64 |
| L2 split | `split_cone_face_by_circle` | `split_torus_face_by_circle` | `split_cylinder_face_by_circle` mirror — projection/normal/wrap만 교체, DCEL surgery REUSE |
| L3 render | `tessellate_cone_circle_clipped` | `tessellate_torus_circle_clipped` | `tessellate_cylinder_circle_clipped` UV-earcut mirror |
| L4 scene | `Scene::draw_circle_on_cone` | `Scene::draw_circle_on_torus` | `draw_circle_on_cylinder` template-copy, dual-path ownership |
| L5 WASM+TS | `drawCircleOnCone` | `drawCircleOnTorus` | `drawCircleOnCylinder` mirror + bridge wrapper |
| L6 dispatch | DrawCircleTool kind===4 | DrawCircleTool kind===5 | kind===2 (cylinder) parallel branch |

## 5. Lock-ins

- **L-263-1** scope = Cone + Torus 원 sketch-split only (no extrude); ADR-202/
  257 1:1 mirror.
- **L-263-2** Cone = exact geodesic (developable 부채꼴 unroll), Torus =
  param-space (u,v) 근사 (비-developable). geometry empirical-validate (β-1
  표면-on + map-back round-trip).
- **L-263-3** render = UV-earcut 둘 다 (ADR-257 §10 L-257-1 답습, 강등 gate 없음).
- **L-263-4** split = per-surface mirror (generic refactor defer); twin-HE
  reparent REUSE (surface-agnostic).
- **L-263-5** cap + remainder 둘 다 Cone/Torus surface 상속 (ADR-089 A-χ).
- **L-263-6** co-conical / co-toroidal twin-gate (render) — Boolean cap / 무관
  face mis-clip 차단 (ADR-257 L-257-6 co-cylindrical / LOCKED #83 L-83-5
  co-spherical mirror).
- **L-263-7** guards: Cone ρ<부채꼴 half-wrap + apex(v>0); Torus u/v wrap
  graceful None.
- **L-263-8** ownership dual-path (Shape + XIA, ADR-257 L-257-2).
- **L-263-9** additive (기존 sphere/cylinder/평면 sketch + 모든 op 불변,
  ADR-046 P31 #4). manifold (ADR-007 verify_face_invariants).
- **L-263-10** 절대 #[ignore] 금지; 사용자 시연 게이트 (ADR-087 K-ζ) γ 필수.

## 6. Sub-step plan (Path Z atomic, ADR-257 답습 ~8-12일)

Cone 먼저 (developable, cylinder 와 동급) → Torus (param-space). 둘 다 본 ADR.

| sub-step | 내용 | risk |
|---|---|---|
| **α (본 spec)** | ADR + lock-in + geometry de-risk + 6-layer mirror map | LOW |
| β-1 Cone geo | `project_to_cone` + `circle_on_cone` (부채꼴 unroll) + 회귀 | MEDIUM |
| β-2 Cone split+render | `split_cone_face_by_circle` + `tessellate_cone_circle_clipped` + 회귀 | MEDIUM |
| β-3 Cone wire | `Scene::draw_circle_on_cone` + WASM + TS + DrawCircleTool kind===4 | LOW |
| β-4 Torus geo | `project_to_torus` + `circle_on_torus` (param-space) + 회귀 | MEDIUM |
| β-5 Torus split+render | `split_torus_face_by_circle` + `tessellate_torus_circle_clipped` + 회귀 | MEDIUM |
| β-6 Torus wire | `Scene::draw_circle_on_torus` + WASM + TS + DrawCircleTool kind===5 | LOW |
| **γ E2E** | real Chromium 시연 (cone 벽 원 → split / torus 벽 원 → split, 둘 다 manifold valid + 사용자 시연 게이트) + 회귀 봉인 | MEDIUM |
| δ | 회고 + LOCKED entry + README/CLAUDE.md | LOW |

## 7. Q1~Q5 (β 진입 전 결재 — α 단계 잠정)

- **Q1 scope**: Cone + Torus 둘 다 (foundation 완성) ✅ 결재됨 (사용자 선택)
- **Q2 Cone repr**: exact geodesic (developable 부채꼴 unroll) — D2 잠정 추천
- **Q3 Torus repr**: param-space (u,v) 근사 (비-developable, 유일 옵션) — D2
- **Q4 render**: UV-earcut 둘 다 (ADR-257 §10 답습) — D3
- **Q5 split**: per-surface mirror (generic refactor defer) — D4

## 8. Cross-link

- **ADR-257** (Cylinder 원 sketch — 6-layer template, §11 cone/torus mirror
  명시) + **LOCKED #83 ADR-202** (Sphere S9 template) + ADR-173 12-gate (S9
  곡면 column)
- ADR-089 A-χ (split surface 상속, LOCKED #35) / annulus.rs (twin-HE reparent)
- ADR-205 (tessellate_cone/torus_clipped — plane-only, circle-clip 신규 대상)
- ADR-031 Phase D (AnalyticSurface Cone/Torus 인프라)
- ADR-087 K-ζ (사용자 시연 게이트) / ADR-046 P31 #4 (additive) / LOCKED #44
- ADR-259/260/261/262 (#1~#4 "완벽한 extrude" 자매 — #5 가 다음)
- 메타-원칙 #5 #6 #14

## 9. Acceptance Log (α~γ closure, 2026-06-26)

6-layer 스택 (ADR-257 cylinder mirror) Path Z atomic. Cone 먼저 (developable,
exact geodesic) → Torus (param-space, 비-developable).

| sub-step | layer | commit | 회귀 |
|---|---|---|---|
| α | spec (audit-first: sphere+cylinder done → cone+torus gap) | `191ca3c` | — |
| β-1 | Cone L1 geometry (`project_to_cone` + `circle_on_cone` 부채꼴 unroll) | `ce15f2f` | axia-geo +6 |
| β-2 | Cone L2 split + render (`split_cone_face_by_circle` + `tessellate_cone_circle_clipped`) | `fa84cf4` | axia-geo +4 |
| β-3 | Cone L4-6 wire (Scene dual-path + mesh_export hook + WASM + bridge + DrawCircleTool kind===4) | `3f42bb3` | axia-core +2, vitest +6 |
| β-4 | Torus L1 geometry (`project_to_torus` + `circle_on_torus` param-space) | `b87a76b` | axia-geo +6 |
| β-5 | Torus L2 split + render (`split_torus_face_by_circle` + `tessellate_torus_circle_clipped` doubly-periodic) | `5744bda` | axia-geo +4 |
| β-6 | Torus L4-6 wire (Scene + mesh_export hook + WASM + bridge + DrawCircleTool kind===5) | `f8213d2` | axia-core +2, vitest +6 |
| γ | E2E (real Chromium + prod build + compiled WASM) | `45cee63` | Playwright +2 |
| δ | closure (Status Accepted + 본 Log + §10 Lessons + README + LOCKED #87) | (본 commit) | docs |

**누적**: axia-geo +20 (2079 → 2099), axia-core +4 (414 → 418), vitest +12
(345 → 357 — bridge 6 + tool 6; 본 spec 측정 시점 351은 2 file subset),
Playwright +2. 모두 PASS, 절대 #[ignore] 금지 준수.

**라이브 검증 (실앱 + 재빌드 WASM, preview_eval)**:
- Cone: `create_cone(0,0,0,500,1000,32)` → fc 2 (base Plane + Cone kind4 side)
  → `drawCircleOnCone(side, [250,0,500], [250,0,600])` → `{cap:2,annulus:1}`
  fc 2→3 manifold valid v=0.
- Torus: `create_torus(0,0,0,500,100)` → fc 1 (single Torus kind5) →
  `drawCircleOnTorus(0, [600,0,0], [580,0,80])` → `{cap:1,annulus:0}` fc 1→2
  manifold valid v=0.

**E2E (γ, real Chromium)**: cone porthole → cap+remainder Cone(kind4) manifold
full 3D (z-span > 900) / torus porthole → Torus(kind5) manifold full 3D
(z-span > 150, rmax > 550). 2/2 PASS.

## 10. Lessons (canonical for curved-sketch family + future surface ops)

- **L1 audit-first canonical (Pattern-12, ADR-131)** — α de-risk 가 사용자가
  고른 Phase 0 foundation 이 *이미 절반 done* (Sphere ADR-202 + Cylinder
  ADR-257) 임을 발견. 초기 probe 가 `drawCircleAsCurve` (generic free-draw)
  를 써서 "cylinder ❌" 오결론 → ADR-257 `drawCircleOnCylinder` (곡면-aware
  split) 누락. **올바른 API 로 재-probe** 가 잔존 gap (Cone+Torus) 을 정확
  히 확정. 향후 곡면 op 는 *기존 surface-aware API inventory 우선*.
- **L2 split 은 surface-agnostic** — `split_*_face_by_circle` 의 DCEL surgery
  (add_face_with_holes + N-edge twin-HE reparent) 는 surface 무관. cone/torus
  는 *projection + outward normal + wrap-guard 만 교체*. 향후 새 곡면 (e.g.
  NURBS surface) sketch-split 도 동일 패턴.
- **L3 3-tier 곡선 표현 (D2 canonical)** — sphere=analytic Circle /
  developable (cylinder, cone)=exact geodesic (unroll) / 비-developable
  (torus)=param-space metric-scaled 근사. 곡면의 Gaussian 곡률이 표현 tier
  를 결정. 향후 곡면은 developable 여부로 분류.
- **L4 doubly-periodic earcut (torus render)** — cylinder 의 단일 u-seam UV-
  earcut 을 2-seam 으로 확장. full param square `[0,2π]²` 4-edge CCW sampling
  minus hole + dual shift (hole 을 (π,π) 중앙으로). 4 edges 가 2 coinciding
  seams 로 map → verts 3D coincide → 무seam. closed periodic 곡면 render 의
  canonical 패턴.
- **L5 apex/특이점 on-surface 체크** (β-2) — cone band 의 v_range 가 apex
  (v=0) 포함 → remainder render 가 apex 까지 tessellate (cone tip, 정당).
  `project_to_*` 가 특이점 역산 불가 → 회귀 on-surface 체크는 surface-equation
  거리 (apex-safe) 사용. 향후 특이점 있는 곡면 (sphere pole 등) 동일 주의.
- **L6 dual-path ownership (ADR-257 L-257-2 답습)** — Scene::draw_circle_on_*
  의 cap 은 Shape-owned (form-layer draw) OR XIA-owned (primitive) 둘 다
  reconcile. 곡면 split 이 ownership 깨지 않음.

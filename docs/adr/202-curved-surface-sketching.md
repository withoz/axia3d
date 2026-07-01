# ADR-202 — Curved-Surface Sketching (곡면 위 직접 그리기)

- **Status**: Accepted — Sphere MVP **구현·closure 완료** (§9). 단, MVP 는 S3
  (DrawLine on sphere) 가 아닌 **S9 (closed Circle on sphere)** 로 pivot 됐다
  (§9.1). S3 line / Cylinder·Cone·Torus (L-202-7) 는 후속 ADR.
- **Date**: 2026-06-17 (α spec) → 2026-06-17 (β + render + E2E closure)
- **Track**: 곡면 스케칭 — ADR-173 12-gate 매트릭스 S3/S6/S9/S12 (곡면 column) 한계 해소
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL)

## 1. Context

ADR-186 트림 작업으로 **평면(coplanar) 곡선×곡선 trim 매트릭스가 완성**됐다
(선/원/호 × 선/원/호/Bezier 모두 작동, ADR-186 step ①③ + 호×호/호×Bezier 봉인).
남은 한계는 ADR-173 12-gate 매트릭스의 **곡면 column** — S3 (DrawLine on 곡면) /
S6 (RECT on 곡면) / S9 (CIRCLE on 곡면) / S12 (Bezier on 곡면) — 모두 ⚠
Documented-Limitation.

근본 원인: 모든 draw 도구는 커서를 **평면**에 투영(`get3DPoint` chord plane /
`getDrawPlane` ADR-140 tangent plane)하고 평면 edge 를 만든다. 곡면(Cylinder/
Sphere) 위에 "선"을 그어도 곡면을 따라가지 않고 평면 근사가 된다. 사용자가 구·
원통에 직접 스케치할 수 없다.

## 2. α audit finding (4-subsystem 병렬 audit, 2026-06-17)

read-only workflow (4 Explore agents, 334K tokens) 로 곡면 sketching 의 자산/gap
을 정합:

| Subsystem | 자산 (재사용 가능) | gap (구현 필요) |
|---|---|---|
| **Surface projection** | forward eval 8종 완비 (`SurfaceOps::evaluate/normal/derivative_u/v`) + `normal_at_world_pos`(analytic closed-form) + SSI Newton (`refine_bezier_pair`) + brute-force uv recovery (`invert_to_uv_brute_force`) | **world→uv 역변환 전무** (모든 surface kind) · **ray-surface intersection 전무** (SSI=surface-surface, ray-curve=edge only) |
| **Face split** | `Face.surface` + 모든 split site (`split_face`/`split_face_by_chain`/`case_b/c/d`)가 parent surface 상속 (ADR-089 A-χ) + `compute_uv_slice_for_quad_face` (tight uv-subrange) | uv-subrange는 **render-time only** (split 시 미적용, 자식이 parent full uv_range 상속) · **edge-on-surface 표현 전무** (AnalyticCurve 전부 world-space) |
| **Draw flow** | `get3DPoint`(chord plane)/`getDrawPlane`(tangent plane ADR-140 δ) + `faceSurfaceKind`/`faceSurfaceNormalAtPos` bridge | 모든 draw command가 3D world point만 (no uv) · **`intersect_faces_inner` coplanar-only → surface curve 에서 깨짐** · 스트로크 중 tangent plane lock (재투영 없음) |
| **Curve 표현** | AnalyticCurve 6종 (전부 world-space) + `TrimCurve2D`(NURBS only, Phase E MVP) + `offset_edge_on_host_face`(ADR-080, "curve on host" precedent) + plane-quadric SSI (`plane_sphere`/`plane_cylinder`/`plane_cone`) | **SurfaceCurve variant 전무** · geodesic solver 전무 |

### 핵심 feasibility (audit 확인)

- `ssi::analytic::plane_sphere` 가 교차원을 **analytic하게 계산** (center `foot`,
  radius `r_circ`, orthonormal basis) 후 샘플 반환 — analytic Arc/Circle 직접
  추출 가능. **곡면 위 Arc 표현에 신규 enum 불필요**.
- `plane_sphere` / `plane_cylinder` / `plane_cone` 모두 존재 → Option C 의 평면
  단면 경로가 즉시 가용.

## 3. Decision (사용자 결재 2026-06-17)

### Q1 = Option C — 평면 단면 → world-space Arc/Circle

곡면에 "선 A→B" 를 그으면: A, B + (구 center / 곡면 법선) 통과 평면을 곡면과
교차 → 곡면 위 **AnalyticCurve::Arc** (또는 닫힌 Circle). 신규 enum 0, 기존 Arc
변형·tessellate·split·serialize 모두 재사용, Phase F (trim) 결합 불필요. CAD
"profile cutting plane" 관습.

**Option A (UV-space line + 신규 SurfaceCurve)** 거부 — high-friction (신규
variant + Phase F + 직렬화 migration + world→uv 역변환). **Option B (geodesic)**
거부 — solver 부재, Phase J defer.

### Q2 = Sphere 먼저

평면 단면 = 깨끗한 원/호 (`plane_sphere` analytic). 가장 작고 명확한 MVP.
Cylinder (oblique 단면 = ellipse → NURBS) 는 후속 거장.

**핵심 통찰 — 구에서 Option C = geodesic**: A, B, sphere center 통과 평면 단면
= **대원(great circle)** = 구 위 최단경로 (geodesic). 즉 sphere MVP 의 Option C
는 수학적으로 정확한 geodesic 과 일치 (Option B 를 단순 경로로 자연 달성).

### Q3 = 면 분할 포함 (surface-aware split)

그린 surface Arc edge 가 곡면 face 를 2 sub-face 로 분할, 자식이 Sphere surface
상속 (ADR-089 A-χ). 실제 사용자 가치 (S3 실현). coplanar arrange 대신 surface-
aware split 신규 경로 (`intersect_faces_inner` 는 surface curve 에서 깨짐).

### Canonical defaults (결재 불필요)

- **클릭점** = `faceHit.point` (viewport raycaster, tessellation ~0.02mm 정확,
  LOCKED #40) → sphere 위로 정밀 투영 (`center + R·(p-center).normalize()`,
  closed-form, world→uv 역변환 신규 불요).
- **Default OFF** + production opt-in flag `surface_sketch_on_draw` (ADR-049
  P-5e-α canonical, localStorage `'true'` opt-in).
- **LOCKED #63 보존**: 빈 공간 클릭 = z=0 ground 강제 (변경 0). 곡면 sketch 는
  곡면 face hit 시에만 발동 (ADR-175 contract 답습).

## 4. Architecture — Sphere MVP 데이터 흐름

```
사용자 클릭 A, B (곡면 Sphere face hit)
  ↓ faceHit.point → project_to_sphere(p) = center + R·(p-center).normalize()
A_s, B_s  (구 표면 위 정밀 점)
  ↓ plane Π = through(A_s, B_s, sphere_center)   (great circle plane)
  ↓ ssi::analytic::plane_sphere(Π, sphere) → (foot=center, r_circ=R, basis u,v)
great circle (center, R, normal=Π.normal, basis_u)
  ↓ angle(A_s), angle(B_s) in (u,v) basis → AnalyticCurve::Arc{center,R,normal,basis_u, a0, a1}
surface Arc edge (world-space Arc, lies exactly on sphere)
  ↓ surface-aware split: Arc edge가 sphere face를 2 sub-face로 분할
  ↓ 자식 face 모두 Sphere surface 상속 (ADR-089 A-χ) + uv-subrange
2 sphere sub-faces (manifold valid)
```

## 5. Phased roadmap (Path Z atomic)

| Sub-step | 내용 | layer |
|---|---|---|
| **α (본 문서)** | spec + 4-subsystem audit + Q1~Q3 결재 + roadmap lock-in | docs |
| **β-1** | Engine: `Mesh::sphere_great_circle_arc(a, b, host_face) → AnalyticCurve::Arc` — 점 투영 + 평면 단면 (plane_sphere analytic 재사용) + Arc 추출. 단위 시뮬레이션 (A,B → 구 위 Arc, 끝점 일치, 모든 샘플 구 위) | axia-geo |
| **β-2** | Engine: surface-aware face split — surface Arc edge 가 sphere face 분할, 자식 Sphere 상속 + uv-subrange. manifold valid. 단위 회귀 | axia-geo / axia-core |
| **β-3** | WASM bridge `drawCurveOnSphere(...)` + TS DrawLineTool 곡면 dispatch (host kind=Sphere 시 surface-draw 경로, faceHit.point 클릭). vitest | axia-wasm / web |
| **β-4** | Production flag `surface_sketch_on_draw` (engine OFF + localStorage opt-in) + SettingsPanel 토글 | axia-core / web |
| **ζ** | 브라우저 시연 (구에 선 → 곡면 위 호, face 분할, manifold valid) + 회귀 sweep + closure docs + LOCKED 등재 | all |

각 sub-step 별도 atomic PR (LOCKED #44), 시뮬레이션-우선 (ADR-087 K-ζ 시연 게이트).

## 6. Lock-ins (canonical for ADR-202)

- **L-202-1** Option C (평면 단면 → world-space Arc) — 신규 AnalyticCurve variant 0.
- **L-202-2** Sphere 먼저 — 평면 단면 = great circle = geodesic (수학적 정확).
- **L-202-3** 클릭점 = faceHit.point → closed-form sphere 투영 (world→uv 역변환 불요).
- **L-202-4** Surface-aware split — 자식 Sphere 상속 (ADR-089 A-χ 답습), coplanar
  arrange 미사용 (surface curve 비-coplanar).
- **L-202-5** Default OFF + flag (ADR-049 P-5e-α), LOCKED #63 빈공간 z=0 보존.
- **L-202-6** ADR-046 P31 #4 additive — 기존 평면 draw 동작 무변경, 곡면 face hit 시에만 발동.
- **L-202-7** Cylinder/Cone/Torus/NURBS-class 는 후속 ADR (oblique=ellipse NURBS / cone·torus 단면 복잡).
- **L-202-8** 절대 #[ignore] 금지.

## 7. Risks (audit riskNotes 종합)

- **ADR-033 Surface ≠ Face**: surface 는 순수 기하; uv 역변환 결과가 face trim
  loop 밖일 수 있음 → caller 가 face uv_bounds 검증. (β-2 에서 Arc 가 face 경계
  안에 있는지 확인.)
- **coplanar-only `intersect_faces_inner`**: surface Arc 는 비-coplanar →
  기존 coplanar arrange 파이프라인 우회, surface-aware split 신규 경로 (β-2).
- **uv-subrange 는 현재 render-time only** (LOCKED #16): split 시 eager uv-slice
  계산은 O(1) 유지 (draw-path perf, ADR-201 β-3 snapshot 우려 답습).
- **snapshot 무결성** (ADR-201 β-3): uv-range 할당은 deterministic + idempotent
  (snapshot/restore round-trip).
- **ADR-178 DrawRect**: RECT 는 `resolveFacePlane` (getDrawPlane parity 미완) →
  RECT 곡면 sketch (S6) 는 별도 ADR.
- **get3DPoint↔getDrawPlane 불일치**: get3DPoint(chord)/getDrawPlane(tangent
  ADR-140). 곡면 sketch 는 getDrawPlane(surface-aware) 경로 우선.

## 8. Cross-link

- ADR-173 12-gate 매트릭스 (S3/S6/S9/S12 곡면 한계 — 본 ADR 이 S3 sphere 부터 해소)
- ADR-186 (평면 곡선 trim 완성 — 곡면은 본 ADR)
- ADR-089 A-χ (split surface 상속), A-ρ/φ (uv-slice tessellation), A-τ (smooth-group)
- ADR-031 Phase D (AnalyticSurface 인프라), ADR-033 (Surface≠Face)
- ADR-034 SSI (`plane_sphere` 재사용), ADR-080 (offset_edge_on_host — curve-on-host precedent)
- ADR-140 (surface-aware getDrawPlane), ADR-175 (face-hit plane), ADR-178 (RECT face-aware)
- ADR-049 P-5e-α (engine OFF + production ON), ADR-046 P31 #4 (additive)
- LOCKED #40 (render chord_tol), #44 (Complete Meaning per Merge), #63 (z=0 invariant),
  #16 (surface tessellation), #35 (surface inheritance)
- 메타-원칙 #14 (면은 닫힌 경계), #4 (SSOT), #5 (사용자 편의), #6 (Preventive)

## 9. Implementation closure (β + render + E2E, 2026-06-17)

α spec (Q1=Option C / Q2=Sphere / Q3=면 분할) 결재 후 Path Z atomic 으로 구현.
**§4/§5 의 원안 (DrawLine A→B → great-circle Arc, S3) 은 §9.1 pivot 으로 갱신**
됐다 — 본 closure 가 canonical (메타-원칙 #10: 원안 보존 + 본 amendment).

### 9.1 S3 → S9 pivot (canonical finding)

β-2 시뮬레이션에서 **구 위 "선 A→B" 분할이 불가** 함을 발견: surface-aware split
은 boundary→boundary 절단이 필요한데, 구 hemisphere face 의 boundary 는 적도뿐
이고 적도 위 두 점은 **적도 평면이 degenerate** (great-circle plane 결정 불가).
→ 결재로 **MVP = 닫힌 원 (closed Circle on sphere, S9)** 으로 pivot. 닫힌 원은
host face 를 cap + annulus 로 깨끗이 분할 (boundary 없이 self-loop). S3 (DrawLine
on sphere) 는 별도 ADR 로 재설계 (`sphere_great_circle_arc` β-1 자산은 보존, S3
재진입 시 재사용).

### 9.2 구현 sub-step (commits)

| Sub-step | commit | 내용 |
|---|---|---|
| α spec | `3de00c3` | 본 문서 + 4-subsystem audit + Q1~Q3 |
| β-1 | `35e46e4` | `sphere_great_circle_arc` (점 투영 + 대원 평면 + Arc, 신규 enum 0) |
| β-2 | `c2b7d91` | **S9 pivot** — `circle_on_sphere` + `split_sphere_face_by_circle` (cap=`add_face_closed_curve`+Sphere override, twin-HE reparent → annulus inner hole, 둘 다 Sphere 상속 A-χ) |
| β-3a | `8012a5b` | `Scene::draw_circle_on_sphere` (단일 transaction) + WASM `drawCircleOnSphere` + TS bridge |
| β-3b | `773ab56` | render — `tessellate_sphere_clipped` (cap/annulus 영역 분리, z-fight 해소) |
| β-3c | `eabdaa3` | DrawCircleTool dispatch (host kind=Sphere 감지 → sphere mode, 둘째 클릭 pick=radius) |
| LOD fix | `09498ff` | ADR-135 Amendment 1 (LOD geometry refresh — 곡면 faceting, ADR-202 무관) |

### 9.3 Render smooth-boundary closure (β-3b 보강)

β-3b 의 초기 clip 은 **per-triangle centroid clip → jagged 경계** (사용자 시연
"빨간 지그재그"). marching-triangles 로 재작성:

| commit | 내용 |
|---|---|
| `dec84a1` | 단일 원 — Sutherland-Hodgman marching (crossing 을 원 위로 snap) + **co-spherical `twin_role` 게이트** (twin HE 면이 같은 center+radius Sphere 일 때만 clip → ADR-197/198 Boolean dimple/union 캡 회귀 차단, 적대적 Workflow 검토가 발견) + sub-grid 캡 `Some(empty)` z-fight 회피 |
| `1c02465` | **multi-circle** — 한 반구에 원 2개+ (host 구멍 N개) 도 단일 multi-clip marching 으로 통합 (각 원 평면 순차 clip + 해당 원 위 snap). 사용자 시연 "1개 깔끔 2개부터 깨짐" 해소 |

co-spherical 게이트의 invariant (ADR-202 split 만 clip, Boolean 캡 제외) 는 회귀
`adr202_smooth_clip_excludes_boolean_caps` (boolean.rs) 로 봉인 — 기존 union
테스트가 zmax 만 검사해 bottom cap 회귀를 놓쳤기에 양극값 (zmin -3 / zmax 3) 모두
검사.

### 9.4 L-202-5 amendment (flag → 무조건 활성)

원안 L-202-5 (Default OFF + `surface_sketch_on_draw` flag) 는 **β-3c (`eabdaa3`)
에서 무조건 활성으로 amend** 됐다 — DrawCircleTool 이 `surfaceKind===3` (Sphere
face hit) 시 자동 sphere mode (flag 없음), ADR-175/178 (face-hit drawing plane)
parity. LOCKED #63 (빈 공간 z=0) 는 보존 (곡면 face hit 시에만 발동). 즉 §3 / §5
β-4 / L-202-5 의 "flag" 는 구현 안 됨 — 무조건 활성이 canonical.

### 9.5 E2E (ζ closure)

`web/e2e/adr-202-sphere-circle-sketching.spec.ts` (3 specs, real Chromium + prod
build + 컴파일 WASM):
1. 단일 원 → cap+annulus 분할 (둘 다 Sphere kind=3, manifold valid, 경계 정점
   원 위 ≥10 = smooth)
2. 분리된 원 2개 (한 반구) → host 구멍 2개, manifold, full 3D (z ±5)
3. 겹치는 원 3개 → full 3D solid (flat-disk 회귀 방지)

Rust unit (engine geometry) + vitest (DrawCircleTool dispatch) + 본 E2E (end-to-end
WASM/브리지) 의 3-layer 회귀.

### 9.6 검증 (closure)

axia-geo **1857** / axia-core **388** / vitest **2197** / E2E **3/3**, 0 failed.
브라우저 시연 (사용자 실앱 localhost:3002): 구에 마우스로 원 N개 → 매끈한 경계,
manifold valid (사용자 확인).

### 9.7 남은 deferred (후속 ADR)

- **S3 — DrawLine on sphere** (great-circle arc): §9.1 degenerate 재설계 (boundary
  →boundary 대신 다른 절단 모델). β-1 `sphere_great_circle_arc` 자산 보존.
- **Cylinder / Cone / Torus** 곡면 sketching (L-202-7): oblique 단면 = ellipse/
  conic → NURBS curve. multi-week 별도 ADR.
- **Sphere DrawCircle 미리보기 곡면화** (`eabdaa3` 후속): 드래그 프리뷰가 현재
  flat tangent-plane (실제 cap/annulus 와 불일치). S 노력 UX polish.
- **uv seam / tangent degenerate** (적대적 검토): sphere normal 2π-주기라 benign /
  measure-zero, 텍스처 추가 시에만 의미 (known-limitation).
- **Visual baseline** (ADR-077): 곡면+원 시각 baseline 은 host-OS 결합 (Windows
  win32 ≠ CI Linux) → CI-Linux 에서 생성하는 별도 follow-up.

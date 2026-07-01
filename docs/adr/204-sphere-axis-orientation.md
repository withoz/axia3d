# ADR-204 — Sphere Surface Axis Orientation (axis-centric sphere)

- **Status**: Accepted
- **Date**: 2026-06-18
- **Author**: WYKO + Claude
- **Track**: ADR-197 Z-axis lift (A) — sphere enablement
- **Supersedes / Amends**: none (additive schema extension to `AnalyticSurface::Sphere`)

## 1. 사용자 결재 (canonical)

> "축을 중심으로 구를 형성하는것이 정답인것 같다."
> ("Forming the sphere around an axis is the correct answer.")

> "우리 구는 원을 기본으로 해서 만든것이라서 중간에 원이 그려져 있음.
> 원을 기본으로 하지않는것이 더 좋지 않은가?"

## 2. Problem (truth-first probe evidence)

ADR-197 Z-axis lift (A-1~3, LOCKED 보류 — 본 ADR 후 등재)이 cylinder/cone/
torus의 비-Z 축 cut을 `with_axis_lifted_to` rotate-roundtrip으로 해제했으나,
**sphere만 같은 패턴이 막혔다.**

근본 원인 — `AnalyticSurface::Sphere`는 **5 quadric 중 유일하게 방향
(axis_dir / ref_dir) 필드가 없다** (`surfaces/mod.rs:92`):

| quadric | 방향 필드 |
|---|---|
| Cylinder | `axis_origin, axis_dir, ref_dir` ✅ |
| Cone | `apex, axis_dir, ref_dir` ✅ |
| Torus | `center, axis_dir, ref_dir` ✅ |
| **Sphere** | `center, radius, u_range, v_range` — **없음** ❌ |

`sphere::evaluate(center, radius, u, v)`가 **implicit +Z 극 / +X ref를
하드코딩**한다 (`v`: latitude, +π/2 = +Z 북극). 그래서 sphere의
v_range는 **항상 Z-latitude band**만 표현할 수 있고, **tilted cap을 표현
불가**하다.

**probe 실측 (2026-06-18, revert)** — `boolean_sphere_halfspace`로 sphere
r=3을 normal=(1,0,1)/√2 평면으로 cut:
- `Ok(2)` watertight (open HE 0) + invariants valid — **DCEL은 정확**
- 그러나 cap Sphere v_range=(−π/4, π/2) Z-latitude band를 **clip 없이**
  render → `dot(p,n)` min=**−3 (=−radius)**, **118 정점이 cut 평면 아래**
  (−n 극 포함) — **render geometry 틀림**

→ 사용자 premise "`boolean_sphere_halfspace`가 임의-평면 template"은
**DCEL만 참, render는 거짓**. **local-frame(A-1~3 helper)도 동일 막힘** —
Sphere surface가 회전 가능한 축을 못 들고가서, 역회전 후 v_range가
geometry와 불일치.

**production 영향 0**: 현 dispatch는 sphere halfspace를 ±Z 평면만 호출
(latitude=정확). tilted 경로는 미사용 latent.

## 3. Decision

**`AnalyticSurface::Sphere`에 `axis_dir: DVec3` + `ref_dir: DVec3` 필드를
추가**하여, sphere를 다른 3 quadric과 일관된 **oriented quadric**으로
만든다. sphere의 극(pole)은 `center + axis_dir·radius`로 유도되는
**방향**이지, DCEL 정점(point)이 아니다.

파라미터화 (cylinder/cone/torus 답습):
- `u`: `ref_dir` 기준 경도(longitude) — `axis_dir` 둘레 회전
- `v`: 적도(`axis_dir` ⊥) 기준 위도(latitude), v=±π/2 = ±극 (`center ± axis_dir·r`)
- `binormal = axis_dir × ref_dir`
- `evaluate = center + r·(cos v·(cos u·ref_dir + sin u·binormal) + sin v·axis_dir)`

이로써:
- tilted cap의 v_range가 **tilted 극 기준**이 되어 render 정확 (118-below 해소)
- **A-1~3 `with_axis_lifted_to` helper가 sphere에도 작동** (surface가 회전축 보유)
- OR 임의-평면 sphere op이 cap `axis_dir = n`(cut normal) 직접 부여 → v_range 정확

## 4. "no first-class points" (ADR-019 A2)와 무관 — 명문화 변경 없음

ADR-019 A2 "Vertex: edge endpoint 로만 존재, 1급 아님"은 **DCEL geometry
계층**의 정책(정점 = 엣지 끝점). 본 ADR의 `axis_dir`는 **surface 파라미터
방향 필드** — 완전히 다른 계층이다. axis_dir 추가는 A2 정책을 건드리지
않는다. **"점을 1급으로 추가"가 아니라 "방향 필드 추가"** (다른 quadric이
이미 가진 것).

## 5. Scope

### In scope (본 ADR + atomic 구현)
- `AnalyticSurface::Sphere` variant에 `axis_dir` + `ref_dir` 추가
  (`#[serde(default = "...")]` → Z/X 기본값, bincode/legacy 호환)
- `surfaces/sphere.rs` — `evaluate / normal / derivative_u / derivative_v /
  tessellate`를 axis_dir/ref_dir 사용하도록 (현 하드코딩 Z/X 제거)
- `AnalyticSurface::transform` Sphere arm — axis_dir/ref_dir 회전 (rigid)
- `migrate_y_up_to_z_up` Sphere arm — axis_dir/ref_dir Y→Z 회전
- `surfaces/merge.rs` / `curvature.rs` Sphere arm — axis_dir 정합
- ssi (`plane_sphere`) + imprint (`sphere_invert`) — axis_dir 기준 inversion
- ~72 `S::Sphere { .. }` 생성 사이트 — 기본값 axis_dir=Z, ref_dir=X
  (backward-compat, mechanical)
- sphere 임의-평면 op (halfspace/slab/slice) cap에 axis_dir=n 부여

### Out of scope (orthogonal, 별도 트랙)
- **DCEL 구성 (적도 원 → 2 반구 + seam)** — 사용자가 지적한 "중간 원"은
  Path B 구성 artifact (적도 seam). 이미 `26042a6`에서 seam 숨김 + hemisphere
  grouping 처리됨. **surface axis_dir fix는 DCEL 구성과 독립** — render/lift는
  axis_dir만으로 해결. axis-기반 hemisphere 재구성(극 기반)은 원하면 별도 ADR.
- 다른 surface(NURBS-class)의 orientation — 본 ADR scope 외

## 6. Backward compatibility (truth-first 정정 — bincode 함정)

**ADR α 초안의 "`#[serde(default)]` bincode 호환" 주장은 틀렸다** (구현 전 검증).
근거:
- bincode 1.3 = **positional** (필드명 추적 없음). `#[serde(default)]`는
  self-describing 포맷에만 유효 — bincode는 enum variant에 필드 추가 시
  **mid-stream 바이트 오정렬** (axis_dir를 u_range 바이트에서 읽음).
- Undo가 **bincode snapshot round-trip** (`before_snapshot: Vec<u8>` =
  `scene_snapshot()`) → axis_dir **반드시 직렬화** (`#[serde(skip)]` 시 매
  Undo마다 tilt 손실 → 불가).
- legacy fixture는 **synthesized** (현 코드 재생성) → OLD→NEW break 미포착.

**결재 (2026-06-18, 사용자 = A)**:
- axis_dir/ref_dir = **실제 직렬화 필드** (serde attr 없음, V4는 항상 6 필드)
- **SNAPSHOT_VERSION 3 → 4 bump** (`2 | 3` arm → `2 | 3 | 4`, V4 = V3 구조
  동일 + Mesh bincode가 새 sphere 레이아웃)
- **핵심**: Mesh struct 불변 — `AnalyticSurface::Sphere` variant만 변경 →
  **sphere 없는 옛 파일(V1/V2/V3)은 그대로 로드** (바이트 레이아웃 동일),
  **sphere 있는 옛 파일만 bincode error로 거부** (silent corruption 아님,
  재생성 필요). 사용자 수용 ("옛 sphere .axia 미지원")
- forward-compat: V4 파일을 옛 빌드가 로드 시 `v > SNAPSHOT_VERSION` reject
- **in-memory 기본값 Z/X** → 기존 모든 sphere 동작 byte-identical
  (Path B sphere / sphere Boolean / render 전부 PASS 유지 강제)

## 7. Lock-ins (canonical for ADR-204)

- **L-204-1** Sphere = oriented quadric (axis_dir + ref_dir), cylinder/cone/
  torus 패밀리 정합
- **L-204-2** 극(pole) = `center + axis_dir·radius` 유도 방향 (DCEL 점 아님)
- **L-204-3** ADR-019 A2 "no first-class points" 정책 불변 (다른 계층)
- **L-204-4** 기본값 Z/X backward-compat (기존 동작 byte-identical)
- **L-204-5** SNAPSHOT_VERSION 3→4 bump (axis_dir/ref_dir 실제 직렬화).
  Mesh struct 불변 → sphere-free 옛 파일 로드 OK, sphere-bearing 옛 파일만
  reject (bincode error, silent corruption 아님). `#[serde(default)]` 무효
  (bincode positional — §6 truth-first 정정)
- **L-204-6** DCEL 구성(적도 seam)은 out of scope (orthogonal)
- **L-204-7** axis_dir 추가 후 A-1~3 `with_axis_lifted_to` helper가 sphere
  에도 작동 (sphere lift unlock)
- **L-204-8** 절대 #[ignore] 금지

## 8. Atomic 구현 plan (sub-steps, 각 별도 commit per LOCKED #44)

- **204-α** (spec): 본 ADR
- **204-β-1** (schema + sphere.rs): Sphere variant 2 필드 + evaluate/normal/
  derivatives/tessellate axis_dir화 + 72 생성 사이트 기본값 + 회귀(oriented
  evaluate/normal)
- **204-β-2** (transform/migrate/merge/curvature): axis_dir 회전 정합
- **204-β-3** (ssi/imprint): plane_sphere + sphere_invert axis_dir 기준
- **204-β-4** (sphere op lift): boolean_sphere_halfspace/slab/slice 임의-평면
  (cap axis_dir=n) OR `boolean_sphere_*_local` (helper 재사용) + 회귀
  (tilted cap render 정확 — dot(p,n) ≥ −eps)
- **204-γ** (closure): 회고 + LOCKED 등재

## 9. Cross-link

- ADR-031 Phase D (AnalyticSurface 5 quadric — sphere만 axis 누락한 ADR)
- ADR-197 Z-axis lift A-1~3 (cylinder/cone/torus, `with_axis_lifted_to` helper)
- ADR-019 A2 (no first-class points — 불변, 다른 계층)
- ADR-038 P23 (surface-aware normals — oriented sphere normal 정합)
- ADR-089 (Path B closed-curve sphere DCEL — 적도 seam, out of scope)
- ADR-103-ε (Y→Z migrate — Sphere arm 확장)
- LOCKED #5 (spatial-hash) / #16 (surface tessellation) / #35 (surface
  inheritance) / #43 (Z-up) / #44 (Complete Meaning per Merge)
- 메타-원칙 #4 (SSOT) / #6 (Preventive) / #10 (ADR 불변 — schema 변경 ADR)

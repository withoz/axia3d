# ADR-271 — Curved-Wall Cut (P1) · α spec

**Status**: Accepted (β~δ Acceptance Log 완료 2026-07-03 — Cylinder radial blind pocket + through shipped; §3 이전이 원 α spec)
**Track**: 6 (Extrude/Cut/Punch) — "완벽한 extrude" 로드맵 **#5 곡면 Phase 1 (cut)**
**Cross-link**: ADR-263(곡면 sketch-split Phase 0) · ADR-257(Cylinder sketch) ·
ADR-202(Sphere sketch) · ADR-252(planar pocket carve) · ADR-249(drill) ·
ADR-267(watertight gate) · ADR-269(cross-drill guard) · 메타-원칙 #5 #6 #9 #10 #14

---

## 1. Context — Phase 0 는 done, Phase 1(cut)이 gap

"완벽한 extrude" 로드맵 #5(곡면):
- **Phase 0 (sketch-split) ✅** — ADR-263: 4 곡면(Sphere/Cylinder/Cone/Torus)
  벽에 원 sketch → cap + remainder 분할. `drawCircleOn{Sphere,Cylinder,Cone,Torus}`.
- **Phase 1 (cut) — 본 ADR** — sketch 한 cap 영역을 pocket/through 로 cut.
- **Phase 2 (boss) — 후속** — cap 을 바깥으로 돌출.

## 2. De-risk 발견 (node-WASM 실측 2026-07-03)

곡면 cap 을 기존 carve 로 자르면 실패:
```
create_cylinder(0,0,0, r=300, h=1000, 32)
drawCircleOnCylinder(side, c=(300,0,500), r-pt=(300,0,600))
  → { cap:34 (kind 2 Cylinder, area 30932), annulus:22 }      ✓ sketch OK
faceHasLargerCoplanarContainer(cap) → false                    (곡면 = coplanar 없음)
carvePocketFromSourceFace(cap, 100) → -1
  err: "no coplanar face contains the polygon hole centroid"    ← planar-only 경로
```

**근본 원인:** `carve_pocket_from_source_face` / `punch_polygon_hole` /
`find_larger_coplanar_container_face` 는 **평면 host 전제** (coplanar container
탐색 + 단일 평면 normal). 곡면 cap 은 (a) 경계가 non-planar geodesic, (b) 안쪽
방향(normal)이 점마다 달라짐(radial) → 기존 경로 부적합. **Pattern-12 아님**
(engine 미비 — 신규 곡면-aware cut 필요).

## 3. Decision — MVP 슬라이스 + 접근법

### L1 — MVP = Cylinder radial blind pocket 우선
가장 흔한 실사용(파이프/봉에 구멍/홈) + 가장 단순한 곡면. Cylinder cap 을 축
방향(radial, 안쪽)으로 depth d 만큼 recess:
- **entry** = cap 경계 (곡면 위 non-planar geodesic circle, 보존).
- **floor** = cap 경계를 축으로 depth d 만큼 이동한 loop (반지름 r−d 의 축소된
  cap; d < r 강제). Cylinder surface 상속 (floor 도 곡면).
- **walls** = entry ↔ floor bridge (radial quads, void-facing winding —
  ADR-268 L2 답습).
- **watertight** — ADR-267 gate 통과 (verify_volume_integrity).

### L2 — inward = per-vertex radial (평면 normal 일반화)
기존 pocket 은 단일 평면 normal 로 floor 를 평행 이동. 곡면은 **점마다 축을 향한
radial** 로 이동. `carve_pocket` 의 "single inward" 를 "per-vertex inward(surface
normal 반대)" 로 일반화하는 것이 핵심 신규 로직.

### L3 — through 는 별도 sub-step
Cylinder 를 radial 로 관통 = 지름 방향 구멍(반대편 벽까지). cross-drill(ADR-269)
가드와 정합 필요 — 관통 축이 반대편 곡면 벽을 정확히 만나는지. MVP(blind) 이후.

### L4 — Sphere/Cone/Torus = 후속 sub-step (1:1 mirror)
ADR-263 이 sketch 를 4 곡면 mirror 로 확장했듯, cut 도 Cylinder MVP 검증 후
Sphere/Cone/Torus 로 mirror. surface normal 평가만 곡면별로 다름.

### L5 — 진입점 = 기존 Push/Pull (dimension dispatch)
곡면 cap 을 Push/Pull inward → 곡면 pocket. PushPullTool 이 `faceSurfaceKind ≥ 2`
+ cap(container 없음) 감지 시 곡면 cut 경로로 dispatch. planar 경로 무영향.

### L6 — Boss(P2)는 본 ADR 밖
cap 바깥 돌출(P2)은 별도 ADR. 본 ADR 은 **안쪽 cut(pocket/through)만**.

## 4. Roadmap (β 이후, 각 sub-step 별도 atomic + 결재)

| sub | 내용 | 규모 |
|---|---|---|
| α | 본 spec | — |
| β | Cylinder radial **blind pocket** engine (`carve_curved_pocket`) | M |
| γ | WASM bridge + PushPullTool dispatch (곡면 cap 감지) | S |
| δ | Cylinder radial **through** (지름 구멍, cross-drill 정합) | M |
| ε | Sphere/Cone/Torus mirror (surface normal per-kind) | M |
| ζ | 실브라우저 E2E + watertight gate + 회귀 자산 | S |

## 5. Lock-ins (β 진입 시 강제)
- watertight (ADR-267 gate) — 모든 곡면 cut 결과 verify_volume_integrity valid.
- winding void-facing (ADR-268 L2) — 곡면 wall/floor 도 축(void) 향함.
- surface metadata 상속 (ADR-263) — floor/wall 이 host 곡면 kind 상속.
- planar cut 무회귀 — 기존 carve 경로 UNCHANGED (곡면은 별도 dispatch).
- cross-drill 정합 (ADR-269) — 곡면 through 도 기존 구멍 교차 시 명확 거부.

## 6. α scope
**α 커밋 = spec only, 코드 0.** β 이후는 사용자 결재 게이트. 메타-원칙 #10 + #9.

## D. Acceptance Log (β ~ δ, 2026-07-03)

| sub | commit | 내용 | 회귀 |
|---|---|---|---|
| α | `304be66` | spec + 로드맵 | — |
| β | `188bab6` | `carve_curved_pocket` — Cylinder radial blind pocket (per-vertex radial inward, floor r−depth 곡면 상속, watertight) | axia-geo +1 |
| γ-core | `6065a2a` | scene `carve_curved_pocket_from_cap` + WASM `carveCurvedPocket` + TS bridge + ADR-267 gate | — |
| γ-tool | `00c8432` | PushPullTool 곡면 cap 감지(kind≥2) → dispatch. 브라우저 E2E (Phase1 faceId / dist=−90 / 55 walls) | vitest 37/37 |
| δ 엔진 | `893da32` | `carve_curved_through` — 지름 관통 tunnel (exit closed-form 투영 + annulus split + tube bridge, genus-1 watertight) | axia-geo +1 |
| δ-wiring | `f15ab3f` | scene pocket↔through depth 자동 라우팅 (depth ≥ radius → through). 기존 export/tool 그대로 through 지원 | — |

**핵심 성과:** de-risk 에서 "carve planar 전용 → 곡면 cut 불가" 였던 것을
**작동하는 곡면 pocket + 지름 관통** 으로 (엔진→bridge→도구 전 계층). 위험했던
**winding(void-facing) + edge welding** 은 첫 구현에 성공. 브라우저 실측:
곡면 pocket recess 흰색(front) + 관통 구멍 watertight.

**교훈:** ① per-vertex radial 일반화가 곡면 cut 의 핵심. ② cap 경계가 annulus 와
공유 edge → welding 방향 강제(free re-punch 불가). ③ exit closed-form
`exit = entry − 2(a·rout)rout` + split_cylinder_face_by_circle 재사용으로 through.
④ **브라우저 wasm HTTP 캐시 함정** — node 는 정상인데 브라우저가 옛 wasm 캐시 →
결과 불일치. 하드리로드/nocache 필수 (node ≡ web 동일 로직).

## 7. 남은 sub-step (별도 결재)
- **ε** — Sphere/Cone/Torus pocket+through mirror (surface normal per-kind, ADR-263
  sketch foundation 완비). Cylinder MVP 검증 완료로 패턴 확립.
- cross-drill(ADR-269) 곡면 through 정합 회귀 자산 보강.
- 곡면 through 실브라우저 E2E (Playwright).

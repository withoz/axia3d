# ADR-288 — Torus tube-through via a cylindrical drill (ε-torus-through)

- **Status**: Proposed (α spec — 코드 0, β 결재 게이트)
- Date: 2026-07-10
- Track: ADR-287 §E 남은 트랙 (ε-torus-through). "완벽한 extrude" 로드맵 #5 곡면
  through 의 마지막 조각.
- Cross-link: ADR-287 (curved cut/boss/through — §F cone through, §E ε-torus-
  through 시도→revert), ADR-194 (drill_circular_through_hole planar drill),
  ADR-034 (SSI Stage 2-4 numerical), ADR-263 (torus circle sketch), ADR-273
  (self-intersection gate), ADR-267 (watertight gate), ADR-190 P0.2 (snapshot
  rollback), 메타-원칙 #4 #5 #6 #14 #16.

---

## 1. Canonical anchor (사용자 결재, 2026-07-10)

ADR-287 §E 의 ε-torus-through 를 별도 ADR 로 진행 (cylindrical-drill 접근).

## 2. 배경 — ADR-287 §E 의 measure-first 발견

torus 의 자연스러운 through = **tube 를 관통** (외벽 → 내벽, minor-circle 방향).
ADR-287 §E 가 **straight-reflection** 접근 (entry vert 를 그 longitude 의 tube-
center C(u) 기준 반사 → `exit = 2·C(u) − P`) 을 시도했으나 실측:
- exit 는 torus 에 정확히 안착 + `verify_face_invariants` valid **하지만**
- **per-vertex 반사라 tube walls 가 twist** → curved tube 를 관통 →
  `detect_self_intersections` **33건** (ADR-273 gate 차단, 작은 cap 에서도).
- Cylinder/Cone diametric bore 가 straight-reflection 으로 되는 이유는 그 표면이
  **ruled/developable** (straight walls 가 표면 내부에 머무름). torus tube 는
  곡률 때문에 안 됨.

## 3. 핵심 통찰 — fixed-axis cylinder drill (parallel walls)

reflection 의 문제는 **per-vertex** reflection 이 walls 를 twist 시킨 것.
**단일 고정 축(fixed bore axis)의 cylinder drill** 을 쓰면 모든 wall 이
bore axis 에 **평행** → twist 없음 → SI 없음 (de-risk 로 검증할 hypothesis).

- **bore axis** = cap 중심의 inward minor-normal (tube center 향함, minor-circle
  plane 내 radial). 외벽 equator cap 이면 bore axis ≈ −major-radial(u0) (donut
  중심 향함).
- **drill** = bore axis 를 축으로 하는 반경 = cap 반경의 **cylinder**.
- **entry hole** (외벽) + **exit hole** (내벽) = torus ∩ cylinder (또는 cap
  boundary ray 들의 ray-torus far intersection).
- **tube walls** = cylinder lateral surface (entry → exit), bore axis 에 평행 →
  no twist → no SI.

## 4. Dependency — ray-torus intersection (신규)

exit hole 을 내벽에 정확히 안착시키려면, cap boundary 의 각 vert P_i 를 bore
axis 방향으로 쏜 ray 가 torus 를 **다시 나가는(far) 교점** 이 필요:
- torus implicit (local, axis=Z): `F = (x²+y²+z²+R²−r²)² − 4R²(x²+y²) = 0`.
- ray `P + t·d` 대입 → **t 의 quartic**. far positive root = exit.
- 신규 `torus::ray_torus_intersections(center, axis, ref, R, r, origin, dir)
  -> smallvec<f64>` (0~4 roots). 기존 SSI (ADR-034) 는 surface-surface; 본 건은
  ray-surface (더 단순, 직접 quartic).
- **대안**: cylinder-torus SSI (numerical, ADR-034 Stage 2-4) — 더 무겁고 곡선
  전체가 필요. ray-torus per-vert 가 충분 (cap boundary 만 필요).

## 5. 결재 필요 (Q1~Q5)

- **Q1 (bore axis)**: (a) cap 중심의 inward minor-normal (fixed) — **추천**
  (parallel walls, SI 회피 핵심). / (b) per-vert (reflection, §E 에서 33 SI —
  거부).
- **Q2 (exit 계산)**: (a) ray-torus quartic per cap-boundary vert (far root) —
  **추천** (정확, 가벼움). / (b) cylinder-torus SSI (무거움).
- **Q3 (drill radius)**: (a) cap 반경 (사용자가 그린 원) — **추천**. bore 는
  cap 을 그대로 tube 로 관통.
- **Q4 (user-route)**: (a) 깊은 inward push (depth ≥ minor_radius) 시 Scene 이
  tube-through 로 route (ADR-287 `curved_cap_axis_radial` torus → minor_radius,
  §E 에서 이미 준비했다가 revert — 재활성) — **추천**. / (b) 명시 도구.
- **Q5 (범위)**: (a) torus 만 (본 ADR) — **추천**. cylinder/cone 은 이미
  diametric through (ADR-287 §F).

## 6. Lock-ins (β 확정, 결재 후)

- **L-288-1** fixed-axis cylinder drill (Q1-a) — parallel walls, no twist.
- **L-288-2** `torus::ray_torus_intersections` quartic (Q2-a) — far root = exit.
- **L-288-3** drill radius = cap radius (Q3-a).
- **L-288-4** entry/exit hole + cylinder tube walls (ADR-194 drill 패턴 답습,
  곡면 hole 은 torus∩cylinder 곡선).
- **L-288-5** watertight (ADR-267) + **SI-free** (ADR-273, §E 33 SI 회귀 방지가
  핵심 acceptance) + snapshot rollback (ADR-190 P0.2).
- **L-288-6** Scene route (Q4-a) — `curved_cap_axis_radial` torus → minor_radius
  재활성 (§E revert 복원) + through branch = tube-drill.
- **L-288-7** additive (ADR-046 P31 #4) — pocket/boss 무회귀. torus 만.
- **L-288-8** 절대 #[ignore] 금지. de-risk (fixed-axis SI-free) + E2E + 시연.

## 7. Roadmap (β 결재 후)

- β-1 `torus::ray_torus_intersections` (quartic) + 회귀 (known roots)
- β-2 engine `drill_torus_tube` (fixed-axis cylinder, entry/exit hole + walls) +
  de-risk (SI-free, vs §E 33 SI)
- β-3 Scene route (curved_cap_axis_radial torus 재활성) + WASM/bridge
- β-4 E2E (torus deep push → tube-through, real Chromium) + 시연 + closure

## 8. de-risk (β-1/β-2 착수 전/직후) + 추가 기하 제약

**핵심 hypothesis**: fixed-axis cylinder drill (parallel walls) 의 tube-through
는 `detect_self_intersections` **0** (§E per-vert reflection 은 33). ray-torus
exit 로 entry/exit hole 을 내벽/외벽에 안착 + cylinder walls 평행 → SI-free
watertight tunnel.

**추가 발견 (α 단계 기하 분석)**: fixed-axis cylinder 도 **작은 cap 한정**.
cylinder 축은 직선(−major-radial(u0), donut 중심 향함)인데 tube center circle
은 반경 R 로 **휘어짐**. cap 의 u-range 가 작으면 tube 가 그 구간에서 ≈ 직선
→ cylinder 가 tube 내부에 머무름 (SI 없음). u-range 가 크면 tube 가 직선
축에서 벗어남 → cylinder walls 가 tube 표면 관통 (SI). ⇒
- **β MVP scope = 작은 cap** (tube diameter 대비 작은 hole — 실무 대부분).
  β-2 de-risk 가 SI-free 를 확정할 cap size 임계 측정.
- **large cap** = curved/toroidal bore (drill 이 tube 를 따라 휨) 또는 정확한
  torus∩cylinder SSI (ADR-034 Stage 2-4) — ADR-288 내 후속 phase 또는 별도
  ADR. β-2 de-risk 결과로 임계 + MVP 경계 확정.

**β-2 de-risk 회귀**: 작은 cap tube-through 가 `detect_self_intersections` 0
+ watertight (§E 33 SI 회귀 방지). 임계 cap size 초과 시 graceful reject
(SI gate 가 이미 차단 — 사용자 facing 은 "hole 이 너무 큼" guidance).

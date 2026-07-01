# ADR-251 — Phase 2 (Punch 확장) Closure + P6 Scope Re-alignment

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 2 (Punch 확장) — closure. P1(ADR-249) +
  P5(ADR-250) + P6 simulation re-alignment.
- **Depends on**: ADR-249 (P1) / ADR-250 (P5) / ADR-194 (`drill_circular_through_
  hole` + carve.rs:223 anti-parallel guard) / ADR-240 (로드맵)

## 1. Context

Phase 2 Punch 확장의 원 scope = P1(사각 관통) + P5(임의-profile 관통) + P6(다면/
비볼록 관통). P1(ADR-249) + P5(ADR-250) closure 후, P6 진입 전 **추가 시뮬레이션**
(실제 엔진 probe, codebase 교훈: empirical > LLM) 으로 P6 의 실제 scope 를 ground-
truth — 결과 **우려된 scope 대부분이 이미 covered**.

## 2. P6 시뮬레이션 결과 (실측, 3-probe scratch test)

| Probe | 결과 | 판정 |
|---|---|---|
| Multi-solid (분리, gap) — 스택 박스 각각 drill | 둘 다 성공 (16 tube, depth 200), 44 faces, manifold valid 0 violations | **이미 작동 (per-solid loop)** |
| Touching boxes (gap 없음) | 입력이 이미 non-manifold (2 coincident shell → edge 4-face 공유) | drill 한계 아님 — 모델링 이슈 (merge/union 필요) |
| Non-convex (L-prism, anti-parallel 벽) — foot 관통 | 성공 (12 tube, depth 100), manifold valid | **이미 작동** |
| Non-convex (non-anti-parallel exit 벽) | carve.rs:223 guard bail | **유일한 hard residue** |

**핵심 발견**:
1. **Multi-solid (분리 솔리드)** — drill 은 nearest opposite wall 까지 (single-solid).
   분리된 솔리드는 각각 drill 하면 됨 (per-solid 이미 작동). "축 따라 모든 솔리드 자동
   drill" 은 thin UI 편의일 뿐 (engine 변경 불필요).
2. **Non-convex + anti-parallel local 벽** — drill 의 anti-parallel guard
   (carve.rs:223 `exit_n.dot(n) > -0.5`) 는 *진입/관통 벽이 anti-parallel 인가* 만 봄,
   **global 볼록성 무관**. 비볼록 L-prism 도 local 터널 벽이 anti-parallel 이면 정상
   관통 (foot 관통 12 tube, manifold 증명).
3. **진짜 hard residue** = **non-anti-parallel exit 벽** (각진/계단형 관통) — non-
   parallel bridge cross-section 필요. 좁고 niche, multi-week.

## 3. Decision

**Phase 2 (Punch 확장) closure 선언**:
- P1 (사각 SOLID 관통) — ADR-249 ✅
- P5 (임의-profile 관통) — ADR-250 ✅
- P6 multi-solid (분리 솔리드) — **이미 작동** (per-solid drill; auto-all-along-axis
  편의는 선택적 future)
- P6 non-convex (anti-parallel local 벽) — **이미 작동** (drill guard = local 벽 기준)
- P6 non-anti-parallel exit (각진 관통) — **niche multi-week future ADR** (유일 residue)

**새 엔진 코드 0** — 이미 작동하는 P6 coverage 를 **회귀 자산으로 봉인** + 문서화.

## 4. Lock-ins

- **L-251-1** P6 multi-solid = per-solid drill loop (이미 작동). 새 엔진 함수 0.
- **L-251-2** P6 non-convex (anti-parallel local 벽) = 이미 작동. drill guard 는
  global 볼록성 아닌 *local 터널 벽 anti-parallelism* 기준 (carve.rs:223).
- **L-251-3** Touching coincident solids = 입력 non-manifold (모델링 이슈, drill 한계
  아님). 라미네이트는 Boolean union 또는 단일 솔리드로 모델링.
- **L-251-4** P6 hard residue = non-anti-parallel exit 벽 (각진/계단형) — future ADR
  (non-parallel bridge cross-section, multi-week). carve.rs:223 guard 가 명시 bail.
- **L-251-5** 회귀 자산 lock-in (절대 #[ignore] 금지): `adr251_p6_nonconvex_anti_
  parallel_drill_works` + `adr251_p6_multisolid_sequential_drill_works`.
- **L-251-6** Phase 2 closure — 다음 priority 는 Phase 4 (고급 carving: P2 countersink/
  P3 slot / P4 곡면 벽 hole) 또는 Phase 5 (곡면 cut + Boolean parity). 별도 결재.

## 5. 회귀 / 검증

- **axia-geo** carve `adr251_p6_*` 2 (non-convex L-prism foot anti-parallel drill →
  12 tube depth 100 manifold / multi-solid 분리 박스 sequential drill → 2×16 tube
  44 faces manifold). carve **26** tests (P1 6 + P5 5 + P6 lock-in 2 + ADR-194 13).
  axia-geo lib **2008** (2006 → +2).
- 새 엔진/WASM/UI 코드 0 (이미 작동하는 함수 exercise + 문서).
- ADR catalog drift ✓.

## 6. Lessons

- **L1 시뮬레이션이 scope 를 줄임 (canonical 재확인)** — P6 "다면/비볼록" 우려가 실측
  으로 대부분 already-covered 로 판명 (multi-solid per-solid 작동, non-convex anti-
  parallel 작동). "non-convex" 공포 과장 — drill guard 는 local 벽 기준. 큰 scope 진입
  전 empirical 시뮬레이션이 multi-week 추정을 niche residue 로 축소.
- **L2 already-working coverage 의 명시 봉인** — 시뮬레이션으로 발견한 "이미 작동" 을
  회귀 자산으로 lock-in → 향후 drill 변경이 이 coverage 를 깨면 CI 검출. 발견을 자산화.
- **L3 honest closure** — Phase 2 를 "P6 전체 구현"이 아닌 "P6 우려 scope 대부분 covered
  + niche residue future" 로 정직하게 closure (truth over completion, ADR-171/172/173
  답습). 사용자 결재로 scope 명확화.
- **L4 Phase 2 atomic 분해 검증** — P1(trivial)/P5(generalization)/P6(mostly-covered)
  의 난이도 스펙트럼이 시뮬레이션으로 사전 노출 → 각 적절 scope 로 atomic closure
  (ADR-249/250/251). 단일 mega-ADR 회피 (LOCKED #44).

## 7. 후속 (별도 ADR, 별도 결재)

- **P6 angled-exit (future ADR, multi-week)** — non-anti-parallel exit 벽 관통:
  carve.rs:223 guard 완화 + 가변 cross-section non-parallel bridge. niche use case.
- **P6 multi-solid auto-drill 편의 (선택적)** — 축 따라 모든 솔리드 ray-cast + loop
  drill (engine 이미 per-solid 작동, thin UI orchestration).
- **Phase 4 고급 carving** (P2 countersink / P3 slot/obround / P4 곡면 벽 hole) —
  ADR-240 로드맵 다음 단계.
- **Phase 5 곡면 cut + Boolean parity** (C3 곡면 임의평면 slice / C6 freeform /
  deep-SSI) — ADR-240 로드맵.

## 8. Cross-link

- ADR-240 (Phase 2 로드맵) / ADR-249 (P1) / ADR-250 (P5) / ADR-194 (drill_circular
  + carve.rs:223 anti-parallel guard) / ADR-007 (manifold) / 메타-원칙 #5 #6 #16 /
  LOCKED #44 (Complete Meaning per Merge — Phase 2 closure).

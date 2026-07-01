# ADR-187 — Boundary Kernel Robust Predicates + "가벼움=속도" 원칙 정정

**Status**: Accepted (α spec — 원칙 정정 + 계획. 사용자 결재 2026-06-01 "정정하고 진행")
**Date**: 2026-06-01
**Author**: WYKO + Claude
**Trigger**: 사용자 algorithm 취약점 조사 (Kayac vs AXiA vs 산업표준 비교표) +
사용자 원칙 정정:
> "가벼운것은 빠르고 신속한것입니다. 성능이나 정밀도가 가벼운것은 아닙니다."

---

## 1. 원칙 정정 (canonical) — "가벼움 ≠ 정밀도 trade-off"

### 1.1 잘못된 framing (Claude 오류, 정정 대상)
> "가벼운 CAD 목표상 CGAL-level 정밀도 추구가 오히려 잘못"

정밀도를 가벼움과 trade-off 로 본 **오독**.

### 1.2 정정된 원칙 (canonical)
> **"가벼움"은 속도/신속(메타-원칙 #11 Latency Budget)의 축이지, 정밀도의
> trade-off 가 아니다. 정밀도와 속도는 동시 추구한다 (메타-원칙 #15
> "빠르고, 신속하고, 정확하게"). robust adaptive predicate 처럼 *둘 다*
> 만족하는 수단을 우선한다.**

### 1.3 근거 — 기존 원칙과 일치
- **프로젝트 목표**: "**스케치업보다 정확한** ... **가벼운 동작**" — 정확 +
  가벼운 동작은 **둘 다** (trade-off 아님). "가벼운 *동작*" = action 이
  빠름.
- **메타-원칙 #15**: "동일 분할 = 동일 contract — **빠르고, 신속하고,
  정확하게**" — 셋 다 동시. 본 정정이 #15 의 직접 적용.
- **메타-원칙 #11**: Latency Budget = 속도 축. 정밀도는 별개 축.

### 1.4 Shewchuk adaptive 가 trade-off 를 없앤다
robust predicate (Shewchuk 1997, `robust` crate 1.1) 는 **adaptive**:
- 공통(non-degenerate) 경우 = **f64 속도** (fast estimate + 오차 한계 검사
  몇 flop)
- degenerate 일 때만 exact 로 escalate
→ **거의 f64 속도로 exact 부호** = 빠름 AND 정확 동시. trade-off 없음.

우리는 **이미 `robust` crate (ADR-058 Shewchuk 1.1) 의존** (NURBS/SSI predicate).
boundary_kernel 만 f64+eps — *AixiAcad port 의 잔재*, 정밀도 원칙상 upgrade 대상.

---

## 2. 문제 — boundary_kernel 의 f64+eps degenerate 취약

비교표 (영역 3) 확인: boundary_kernel = 순수 f64 + eps, robust predicate
미사용. degenerate (collinear / near-coincident) 에서 **부호 판정이 topology
를 결정하는 곳**이 취약:

| 위치 | 현재 (f64+eps) | 위험 |
|---|---|---|
| `geom2::seg_intersect` — `r.cross(s)` / `q_p.cross(r)` 부호 | eps 비교 | near-collinear 교차 오판 |
| `region::angle_of` + next-pointer (leftmost-turn 각도 정렬) | `atan2` 비교 | **near-collinear edge 순서 틀림 → face topology 자체 오류** (면사라짐/오분할 잠재 근원) |
| `geom2::point_in_polygon_even_odd` — ray crossing | f64 | boundary 근처 오판 |

**가장 critical = angle 정렬 (leftmost-turn)** — face cycle traversal 의
정확성이 여기 의존.

---

## 3. 결정 — 기존 `robust` crate 로 degenerate-critical 보강

### 3.1 채택
- boundary_kernel 의 **orientation 부호 판정**을 `robust::orient2d` 로 교체
  (DECISION 만 exact, 교차점 좌표는 f64 — 점 자체는 부호 무관).
- 새 의존 0 (`robust` 이미 axia-geo Cargo.toml 에 있음, ADR-058).
- CGAL arrangement 전체는 **미채택** — 이유는 "무거워서"가 아니라 robust
  predicate 로 *정밀도는 동등* 확보 + CGAL 의 C++ 의존이 WASM 부적합.

### 3.2 Sub-step (β, additive + 회귀 게이트)

| β | 내용 | 위험 |
|---|---|---|
| β-1 | `geom2::orient2d_sign(a, b, c) -> i32` helper (`robust::orient2d` wrap) + 단위 회귀 (collinear/CCW/CW + near-degenerate) | 낮음 |
| β-2 | `seg_intersect` 의 collinear/cross 부호 → orient2d (점 좌표 f64 보존). 기존 38 boundary_kernel 회귀 PASS + near-collinear 회귀 추가 | 중간 |
| β-3 | `region` angle sort (leftmost-turn next-pointer) → orient2d 기반 비교 (atan2 대체). **가장 critical** — face topology 회귀 강화 | 높음 |
| β-4 | `point_in_polygon_even_odd` ray crossing → robust. containment 회귀 | 중간 |
| β-5 | FMA-off 검증 — `robust` crate prerequisite. `verify_predicates_environment` (런타임) 이미 있음 + `.cargo/config.toml` fp-contract 명시 검토 (안전성 확인 후) | 낮음 |

### 3.3 Lock-ins
- **L-187-1** "가벼움=속도, 정밀도 trade-off 아님" canonical (메타-원칙 #15 적용)
- **L-187-2** robust adaptive 우선 (빠름+정확 동시 수단)
- **L-187-3** DECISION 만 robust, 좌표는 f64 (점 자체 부호 무관)
- **L-187-4** 새 의존 0 (`robust` 재사용)
- **L-187-5** CGAL 미채택 — 정밀도는 robust 로 확보, WASM-friendly 유지
- **L-187-6** 기존 boundary_kernel 38 회귀 보존 (additive 보강)
- **L-187-7** angle sort (β-3) = 가장 critical, face topology 회귀 강화
- **L-187-8** 절대 #[ignore] 금지

---

## 4. 비교표 재해석 (정정 후)

| 영역 | 표의 AXiA (old) | port + ADR-187 후 | Kayac | 산업표준 |
|---|---|---|---|---|
| 2 다중교차 | single-cut | B-O sweep | ✅ | sweep-line |
| 2 Containment | bbox | point-in-poly **+robust(β-4)** | convex hull | exact |
| 2 Lineage | ❌ | ✅ | ✅ | — |
| 2 Cycle | pair-map | leftmost **+robust(β-3)** | smallest-CCW | leftmost |
| 2 Hole | 거부 | auto nested | ✅ | ✅ |
| 3 Predicate | f64+eps | **robust orient2d (β)** | f64 | exact |

→ ADR-187 후 boundary_kernel 의 **정밀도가 Kayac 을 능가** (Kayac planar 은
f64, 우리는 robust) + 속도 유지 (adaptive).

---

## 5. Cross-link
- ADR-186 (boundary_kernel port — 본 ADR 이 robustness 보강)
- ADR-058 (Shewchuk robust 1.1 — `robust` crate 의존, FMA prerequisite)
- ADR-124 (`.cargo/config.toml` — β-5 FMA 검토 위치)
- 메타-원칙 #11 (Latency = 속도 축) / #15 (빠르고 신속하고 정확하게 — 정정 anchor)
- 메타-원칙 #6 (Preventive — robust 가 degenerate 선제 차단)
- 프로젝트 목표 ("스케치업보다 정확한 + 가벼운 동작" — 둘 다)
- Kayac/AXiA/산업표준 비교표 (사용자 algorithm 취약점 조사)

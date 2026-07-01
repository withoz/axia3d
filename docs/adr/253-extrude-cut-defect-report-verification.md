# ADR-253 — Extrude/Cut Defect Report Verification + Stale Closure

- **Status**: Accepted
- **Date**: 2026-06-25
- **Track**: 6 (Extrude/Cut/Punch)
- **Type**: Governance / Verification closure (docs-only, 코드 변경 0)
- **Author**: WYKO + Claude (empirical workflow verification)

## 1. Context

외부 에이전트("Cowork")가 작성한 진단 보고서
`reports/ADR_169_ExtrudeCut_DefectDiagnostic.html` (2026-06-15) 가
extrude/cut 관련 **23개 결함**(Extrude E1-E8, Cut C1-C10, Architectural
A1-A5)을 나열하고, **Option A** (C7 HARD flag + C8/ADR-165 Arc 보존, 4-5주,
+173 회귀)를 최우선 권고했다.

사용자 요청: "이 보고서를 검토하고 면분할·선분할·입체에 도형그리기 등
extrude/cut 관련 총체적인 문제점을 파악해줘."

canonical 교훈("audit synthesis ≠ ground truth, 실측 grep 우선" +
"empirical probe > LLM 추론")에 따라, 보고서를 신뢰하지 않고 **현재 작업
브랜치 `adr-186/boundary-kernel-port` 의 실제 코드로 전수 검증**했다
(병렬 verification workflow, 5 agent / 641K tokens / file:line 근거).

## 2. 핵심 결론 — 보고서는 stale + 3개 핵심 주장이 틀림

보고서는 **10일 stale**이며, 최우선 2건(C7/C8)을 포함해 3개 핵심 주장이
현재 코드와 불일치한다. 보고서가 다룬 다수 fix가 이미 이 브랜치에 라이브이고,
진짜 열린 결함은 보고서가 우선시한 것과 다른 작은 집합이다.

### 2.1 보고서가 명백히 틀린 3건 (실측 정정)

| 항목 | 보고서 주장 | 실측 (file:line) |
|---|---|---|
| **C7** (최우선) | split 4함수 `split_face_by_chain`/`case_b/c/d`/`boolean.split_faces_by_intersections` 가 HARD flag 미부여 → 분할선이 LOCKED #16 coplanar-hide 로 안 보임 | ❌ **WRONG/FIXED** — 4함수 전부 부여. `split_face_by_chain` (face_split.rs:828 `mark_chain_edges_hard`), `case_b/c/d` (face_split.rs:1153/1386/1597 `mark_edges_hard`), `boolean.split_faces_by_intersections` (boolean.rs:2120 `mark_edges_hard(&shared_edges)`), `Mesh::split_face` (mesh.rs:4706-4707), `auto_intersect_coplanar` (coplanar.rs:665). ADR-142 β-1 (2026-05-22) + ADR-101 Amendment 10. helper 회귀 `adr101_amendment10_helper_mark_{chain_,}edges_hard` 봉인 |
| **C8** (최우선) | closed-curve(Path B Circle) split → `polygonize_closed_curve_face` substitute → `AnalyticCurve::Circle` 영구 손실, "원 형태 깨짐". ADR-165 신규 3-4주 필요 | ❌ **WRONG/FIXED** — **ADR-189 (2026-06-09, commit `65b6484`, 보고서 6일 전)** 이 해결. `split_circle_face_by_line` (mesh.rs:7233) closed-form analytic trim, faceRederive ON production 경로에서 Arc 보존. 회귀 `adr174_approach_b_line_thru_circle_keeps_arcs` PASS. **ADR-165 신규 작성 불필요** |
| **C2** | coplanar auto-intersect 자동 trigger 가 deprecated (ADR-139, default OFF) | ❌ **WRONG/STALE** — **ADR-176 (2026-06-01, LOCKED #76, 보고서 15일 전)** 이 production default 를 ON 으로 flip. engine default OFF (`scene.rs:507-509`) / production ON (`AutoIntersectSettings.ts: let current=true`). demo-verified (4겹 staggered RECT, invariant 0 violations) |

### 2.2 ADR 번호 충돌 (보고서 결함)

보고서가 인용한 **reserve 번호 ADR-165~169 가 실제 catalog 와 충돌**한다
(외부 에이전트가 stale main 만 보고 번호 재배정한 ADR-141~161 reserve
패턴과 동일 — LOCKED #65 참조):

| 보고서 reserve | 실제 catalog |
|---|---|
| ADR-165 (Arc Preservation) | 실제로는 **ADR-189** 가 담당 |
| ADR-166 | Active Sketch Plane Session Lock (LOCKED #67) |
| ADR-167 | EPS_PLANE SSOT (LOCKED #68) |
| ADR-168 | Face plane drift snap (LOCKED #69) |
| ADR-169 (보고서 자칭) | **Boundary-Routine Unification Audit** (LOCKED #70) |

**보고서의 ADR 번호는 신뢰 불가.** 보고서 자체를 "ADR-169"로 칭한 것도
실제 ADR-169 와 충돌한다.

## 3. 검증된 23개 결함 상태 매트릭스

> 검증 기준 = 현재 작업 브랜치 `adr-186/boundary-kernel-port` (사용자가
> 실제 사용하는 브랜치, LOCAL + push 금지). 이 세션 + 그 사이 작업
> (ADR-189/190/191/192/195/196/252 등)이 모두 이 브랜치에 라이브.

### Extrude (E1-E8)

| # | 보고서 주장 | 실제 상태 | 근거 |
|---|---|---|---|
| E1 | hole-boundary face Push/Pull 거부 | **RESOLVED** | ADR-191 (LOCKED #79) — Q2 relax(Push/Pull entry 한정), `remove_hole_filler_faces` + multi-loop→push_pull. 브라우저 검증(annulus+disk → 10면 tube manifold) |
| E2 | Cylinder Path B side N-quad, single-quad hover | **OPEN** | ADR-094 §6.3. ADR-192 §3.2 latent parity(he_twin self-loop / Boolean 비호환 / `analytic_face_area`=0)와 **shared** → 별도 ADR 로 동시 fix |
| E3 | closed-curve Push/Pull → polygon (Path A) | **RESOLVED** | ADR-192 (LOCKED #80) — `extrude_closed_curve_general_kernel_native`. Bezier→BSplineSurface / BSpline→native knots / NURBS→NURBSSurface rational. 3면 manifold |
| E5 | coplanar sibling manifold 위반 | **RESOLVED** | ADR-102 γ + ADR-190 P0.2 + ADR-196 MoveOnly dispatch |
| E6 | (Plane, Mixed) dispatch NotYetSupported | **RESOLVED** | ADR-190 P0.2-c (`promote_arc_side_faces_to_cylinder` post-process) + ADR-109 π-β |
| E7 | Cone apex degenerate (보고서: RESOLVED 주장) | **UNCLEAR/사실상 OPEN** | ADR-190~196 scope 외 (Cone = offset 경로). 명시적 fix 없음 — 별도 검증 필요 |
| E4, E8 | (보고서 결함) | **미검증** | 5 verification 보고서 범위 밖 — 별도 audit |

### Cut (C1-C10)

| # | 보고서 주장 | 실제 상태 | 근거 |
|---|---|---|---|
| C1 | Boolean hole face 거부 | **OPEN** | `boolean.rs:1587-1590` bail. constrained Delaunay future work |
| C2 | auto-intersect deprecated OFF | **WRONG** | §2.1 — ADR-176 production ON |
| C3 | RECT×CIRCLE cardinal 결함 D | **PARTIAL (의도)** | ADR-128 conservative fallback (`coplanar.rs:59-77`). L-128-8 vertex-on-vertex trade-off. ADR-107 Path B 가 회피 |
| C4 | 3-way overlap (A∩B∩C) deferred | **OPEN** | `coplanar.rs` 전체 2-face. ADR-101 §5 deferred |
| C5 | POLYGON × any matrix 밖 | **RESOLVED** | ADR-195 (2026-06-10) `exec_draw_polygon_as_shape` 전용 경로. non-convex 만 별도 (ADR-242) |
| C6 | NURBS Boolean (보고서: RESOLVED) | **RESOLVED** | ADR-197 β-3 — curved routing. NURBS×NURBS 만 deferred |
| C7 | split 4함수 HARD flag 미부여 | **WRONG/FIXED** | §2.1 — 7 site 전부 부여 |
| C8 | closed-curve split Arc 손실 | **WRONG/FIXED** | §2.1 — ADR-189 해결 |
| C9 | split_edge Bezier curve 상속 | **DEFERRED (의도)** | `mesh.rs:8190-8195` — Circle/Arc/Line 상속, Bezier silent fallback (production 동작 유지). Phase N |
| C10 | SliceTool/SplitTool audit 미완 | **RESOLVED** | ADR-241/242/243 (2026-06-24) trim + non-convex + holed Tier A + `face_set_manifold_info` 버그 fix (mesh.rs:8796) |

## 4. 진짜 열린 결함 (사용자 토픽별 anchor)

### 면분할 (Face Split)
- 평면·입체면 모두 작동 (ADR-101/172/176, demo-verified).
- **OPEN**: C1 hole-face Boolean (boolean.rs:1587, multi-week), C4 3-way
  overlap (pairwise만), 곡면 면분할 (Sphere Circle 만 — ADR-202).

### 선분할 (Edge/Line Split)
- 직선×직선, 직선×Circle 모두 작동 (ADR-172/189 demo-verified).
- **OPEN**: C9 Bezier `split_edge` curve 상속 (의도된 Phase N defer),
  곡면 위 선 (S3 degenerate equator 재설계 필요).

### 입체 도형그리기 (Drawing on Solid)
- 평면·cardinal/slanted 입체면 모두 작동 (ADR-175/178, 박스 윗면 6→7
  분할 demo-verified).
- **OPEN**: 곡면 위 RECT/Bezier (Sphere Circle S9 만 — ADR-202 L-202-7).

### Extrude / Cut
- 대부분 RESOLVED (위 §3). **OPEN**: E2 Cylinder Path B N-quad hover +
  latent parity, E7 Cone apex (불명확), C1/C4 (면분할 참조),
  NURBS×NURBS Boolean.

## 5. Decision — Lock-ins

- **L-253-1** 보고서의 C7/C8/C2 주장은 **WRONG/STALE** — 실측 file:line
  근거로 정정. C7/C8 은 이미 fixed (ADR-142 β-1 + Amendment 10 / ADR-189).
  보고서 Option A (C7 + ADR-165, 4-5주, +173 회귀)는 **불필요** — 두 작업
  모두 이미 완료.
- **L-253-2** 보고서 reserve 번호 ADR-165~169 는 실제 catalog 와 충돌 —
  신뢰 불가. ADR-165 "Arc Preservation" = 실제 ADR-189.
- **L-253-3** 진짜 열린 결함 = §4 의 작은 집합 (E2/C1/C4/곡면 그리기/
  E7/C9). 향후 우선순위 anchor:
  - P1: E2 Cylinder Path B N-quad + latent parity (中, Cylinder Path B 동시 fix)
  - P2: C1 hole-face Boolean (高, constrained Delaunay multi-week)
  - P3: 곡면 그리기/선분할 Cyl/Cone/Torus (中~高, ADR-202 mirror)
  - P4: C4 3-way overlap / E7 Cone apex / C9 Bezier split (低~中, edge case)
- **L-253-4** 검증 기준 = `adr-186` 브랜치 (사용자 active 브랜치, LOCAL +
  push 금지). "main 머지" 는 본 결함과 무관 (별도 main 사용 안 함).
- **L-253-5** 메타-원칙 #6 (Preventive) + audit-first canonical 정합 —
  외부 audit 보고서는 검증 없이 신뢰 금지, 코드 실측이 ground truth.
- **L-253-6** 코드 변경 0 (docs-only governance closure, LOCKED #44
  Complete Meaning per Merge).

## 6. Lessons

- **L1 Audit-first canonical 의 외부 보고서 적용** — ADR-125/127/131 의
  내부 audit-first pivot 패턴을 외부 에이전트 보고서에 적용. 보고서의
  23개 주장 중 3개(C7/C8/C2)가 명백히 틀렸고, 다수가 stale. 외부 audit
  은 검증 없이 신뢰 불가.
- **L2 Stale 시간 격차의 위험** — 보고서(2026-06-15)는 그 1주~2주 전의
  fix(ADR-176 6/1 / ADR-189 6/9)조차 놓쳤다. 빠른 ADR 진행 환경에서 외부
  보고서는 작성 즉시 stale 화. 검증 기준 = 현재 브랜치 HEAD.
- **L3 ADR 번호 충돌 = 외부 에이전트 stale-main 패턴 재발** — LOCKED #65
  (ADR-141 reserve)와 동일. 외부 에이전트는 stale snapshot 기준 번호를
  재배정하므로, 번호는 무시하고 *의도* 만 추출.
- **L4 Empirical workflow 의 verification value** — 5 병렬 agent 가
  file:line grep 으로 23개 주장을 동시 검증. LLM 추론이 아닌 코드 실측이
  C7(7 site HARD)/C8(split_circle_face_by_line:7233) ground truth.
- **L5 Truth over completion** — 보고서가 "최우선 4-5주 작업"이라 한 것이
  이미 완료됨을 인정 → 새 작업 0. 가짜 작업을 만들지 않는다.

## 7. Cross-link

- `reports/ADR_169_ExtrudeCut_DefectDiagnostic.html` (Cowork, 2026-06-15) —
  검증 대상 (stale, 3개 주장 WRONG)
- ADR-142 β-1 + Amendment 10 (C7 HARD flag — 이미 fixed)
- ADR-189 / ADR-174 (C8 Arc 보존 — 이미 fixed)
- ADR-176 (C2 auto-intersect production ON — LOCKED #76)
- ADR-190/191/192/196/252 (E1/E3/E5/E6 + pocket/through — 이미 fixed)
- ADR-195 (C5 polygon 전용 경로) / ADR-197 (C6 NURBS Boolean) /
  ADR-241/242/243 (C10 slice)
- ADR-202 (곡면 sketching — Sphere Circle, 곡면 확장 anchor)
- ADR-125/127/131 (audit-first pivot 패턴 source)
- LOCKED #41 §A9.4 (메타-원칙 #15 HARD flag matrix) / LOCKED #65
  (외부 에이전트 ADR reserve 패턴) / LOCKED #70 (실제 ADR-169) /
  LOCKED #76 (ADR-176 auto ON) / LOCKED #79 (ADR-191) / LOCKED #80 (ADR-192)
- 메타-원칙 #5 (사용자 편의) / #6 (Preventive) / #14 (면은 닫힌 경계) /
  #15 (split contract) / #16 (자동화 antipattern)

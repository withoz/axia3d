# ADR-222 — Hole Kernel Deepening (Plan + Phase 0: Circle Metadata)

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: Hole 커널 심화 (ADR-221 후속) / Foundation
- **Depends on**: ADR-194 (punch) / ADR-191 (ring Push/Pull) / ADR-088 (curve_owner_id) /
  ADR-089 (closed-curve self-loop) / ADR-016 Q2 (multi-loop policy) / LOCKED #1 P7

## 1. Context

ADR-221 closure 시 de-risk가 노출한 두 커널 gap:
- ① **구멍 inner loop이 polygonal** — `AnalyticCurve::Circle` 미부착 (downstream 미인식).
- ② **multi-loop 면이 Boolean/Offset/Revolve/Loft/Sweep에서 거부** (ADR-016 Q2, LOCKED #1).

심층 분석(4 영역 병렬 워크플로우 — effort/risk/dependency/정책)으로 단계별 확장 계획 +
결재 게이트를 확정. 사용자 결재(2026-06-23): **계획 승인 + Phase 0 즉시 구현**.

## 2. Plan (multi-phase roadmap)

| Phase | 내용 | Effort | Risk | 결재 게이트 | Pattern-12 재사용 |
|---|---|---|---|---|---|
| **0 (본 ADR)** | Circle metadata 부착 (Option A) | S | Low | 없음 (순수 additive) | ADR-088 curve_owner_id |
| 1 | Revolve multi-loop (ring→회전 튜브) | M (~200 LoC) | Low-Med | ADR-016 Q2 Revolve | revolve+annulus |
| 2 | Boolean multi-loop | XL (3-4주) | High | ADR-016 Q2 Boolean | ADR-064 NURBS-DCEL depth≤1 |
| 3 (defer) | Offset / true self-loop hole (Opt B) | XL/M-L | High/Med | 별도 | ADR-089 A-ζ |
| (reject) | Loft/Sweep multi-loop | — | — | — | 의미 직교(profile=단면≠면) |

**핵심 정책 원칙** (분석 결론): LOCKED #1 P7 자체는 **불변** — amendment는 *다운스트림
적용 확장*만 + verify_p7_manifold(ADR-051) 통과 강제. ADR-016 Q2 per-op 완화는 각각
**명시 결재 + 새 ADR**(ADR-191 Push/Pull 선례 = 템플릿, 메타-원칙 #10). Push/Pull은 이미
hole 허용(ADR-191), 본 Boolean/Revolve는 비대칭 per-op 완화.

**의존성**: Phase 0(circle metadata)은 모든 후속 enabler — Boolean SSI가 analytic circle을
인식하려면 hole edge에 metadata 필요. Offset/Loft/Sweep은 cost/value 불리 → defer/reject.

## 3. Decision — Phase 0 (구현)

**punched hole의 inner-loop edge에 `AnalyticCurve::Circle` + 공유 `curve_owner_id` 부착**
(ADR-088 패턴). 토폴로지 변경 0.

- **Engine** (`mesh.rs` `punch_circular_hole`): `add_face_with_holes` 직후, 새 circle anchor를
  포함하는 inner loop을 찾아 N개 edge 각각에 `Circle{center_planar, radius, n, e1}` +
  `next_curve_owner_id()` 부여. 기존 hole(존재 시)은 미접촉 (anchor VertId로 명확 구분).
- **Render 영향 0**: 비-self-loop edge의 render 분기(`export_edge_lines_with_map`)는 **Arc만**
  smooth tessellation, **Circle은 chord** → hole이 기존 polygon으로 정상 render (N개 겹친 원
  아님). 회귀 0.
- **값**: ① downstream 곡선 인식 (Phase 1/2 Boolean/Offset이 hole의 circle center/radius 직접
  read) ② 선택 그룹화 (hole edge 하나 클릭 → curve_owner_id로 전체 hole 선택, ADR-088).
- **scope**: `punch_circular_hole`만 (circular). `punch_rect_hole`(Window)은 사각 = 진짜
  polygonal, Circle 부착 없음 (정합).

## 4. Lock-ins

- **L-222-0-1** Phase 0 = full Circle + 공유 owner_id를 N개 inner edge에 부착 (ADR-088 패턴).
  토폴로지/snapshot 변경 0, 8 기존 punch 회귀 보존.
- **L-222-0-2** anchor VertId 매칭으로 새 hole만 tag (기존 hole 미접촉).
- **L-222-0-3** Render 영향 0 — 비-self-loop Circle은 chord render (smooth render는 per-segment
  Arc 후속, ADR-092 패턴).
- **L-222-0-4** punch_rect_hole(Window) 미접촉 (사각 hole = 진짜 polygonal).
- **L-222-1** (Phase 1, 결재 후) Revolve ring multi-loop — ADR-016 Q2 Revolve 완화.
- **L-222-2** (Phase 2, 결재 후) Boolean multi-loop — ADR-016 Q2 Boolean 완화 + CDT/NURBS-DCEL.
- **L-222-3** LOCKED #1 P7 불변 — 모든 amendment는 verify_p7_manifold 0 violations 강제.
- **L-222-4** Offset/Loft/Sweep multi-loop = defer/reject (cost/value).
- **L-222-5** ADR-046 P31 #4 additive only / 절대 #[ignore] 금지.

## 5. 회귀 + 검증 (Phase 0)

- **회귀**: axia-geo +2 (`adr222_punched_hole_inner_edges_carry_circle_curve` — N edge가
  Circle r300 + 공유 owner_id, 토폴로지/manifold 불변 / `adr222_second_punch_tags_only_new_hole`
  — 2 hole distinct owner_id, 기존 hole 미접촉). axia-geo 1988→1990, axia-core 399 무변경.
  절대 #[ignore] 0.
- **브라우저** (real WASM): 100×100 면 r20 punchHole → ring manifold valid, hole 정상 polygon
  render (회귀 0).

## 6. 후속 (결재 게이트별)

- **Phase 1 (결재): Revolve ring multi-loop** — ADR-016 Q2 Revolve per-op 완화 (ADR-191 선례).
  ring→회전 튜브. revolve+annulus+add_face_with_holes 재사용.
- **Phase 2 (결재): Boolean multi-loop** — ADR-016 Q2 Boolean 완화. NURBS-DCEL(depth≤1) 우선
  또는 mesh CDT.
- **Phase 0.5 (선택): smooth hole render** — per-segment Arc 부착(ADR-092 패턴)으로 hole
  wireframe 매끈.
- **defer**: Offset multi-loop / true self-loop hole(ADR-089 A-ζ) / Loft·Sweep(reject).

## 7. Cross-link

- ADR-221 (Hole discoverability — 직전) / ADR-194 (punch) / ADR-191 (ring Push/Pull)
- ADR-088 (curve_owner_id — Phase 0 패턴) / ADR-089 (self-loop — Phase 0.5/Option B anchor)
- ADR-092 (per-segment Arc render — smooth hole 후속 패턴) / ADR-064/066 (NURBS Boolean DCEL)
- ADR-016 Q2 (multi-loop 정책) / ADR-051 (verify_p7_manifold) / ADR-145 (annulus)
- LOCKED #1 P7 (불변) / LOCKED #44 (Complete Meaning per Merge) / 메타-원칙 #10 #14 #16

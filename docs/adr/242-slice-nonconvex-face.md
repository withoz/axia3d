# ADR-242 — Slice 견고화 Phase 1 (C1 — 비볼록 면 slice)

- **Status**: Accepted
- **Date**: 2026-06-24
- **Author**: WYKO + Claude
- **Track**: ADR-240 로드맵 Phase 1 (Slice 견고화) — sub-step 2 of 3 (C1)
- **Depends on**: ADR-240 (extrude/cut/punch 로드맵) / ADR-241 (C5 trim) /
  slice_volume_by_plane (`slice.rs`) / `Mesh::split_face` (mesh.rs:8230) /
  ADR-007 (manifold invariants) / ADR-089 A-χ (split surface inheritance)

## 1. Context

`slice_volume_by_plane` (ADR-240 인벤토리 C1) 는 MVP 에서 **볼록 crossed 면만**
지원했다 — 평면이 면을 정확히 2점에서 가르는 경우(2 On verts)만 `split_face`
한 번으로 above/below 분리. 비볼록 면(L자/U자 cap 을 notch 통과)은 평면이 경계를
**4점 이상**에서 교차 → `bail!("expected exactly 2 — convex faces only in MVP")`.

사용자 결재 **A — clip-rebuild (정석)**. de-risk: `split_face` 는 경계 loop 의
*topological surgery* (v1↔v2 사이를 두 loop 로 가름) — 기하학적 chord 검증은
없으나 edge 제거가 없어 인접 면과의 공유 edge 가 안전. 따라서 비볼록 면도
**여러 cut segment 를 따라 반복 `split_face`** 하면 깨끗이 분리된다 (remove+add
clip 보다 안전 — 공유 edge 보존).

## 2. Decision

`slice.rs` 에 `split_crossing_face_general(mesh, fid, plane)` 추가 — 비볼록
crossing 면(>2 On verts)을 above/below sub-face 로 분해. 알고리즘(iterative
worklist):

1. **Leaf = MONO-SIDE**: 면의 off-plane vert 가 모두 한쪽이면 done(경계에 On vert
   가 남아 있어도 — 이전 ear 절단이 남긴 cut edge). On vert **개수**는 leaf 신호가
   아니다 (초기 버그 #2).
2. **Mixed 면 → cut segment 1개 절단**: On vert 는 모두 cut line 위에 있으므로
   임의의 두 On vert chord 는 cut line 을 따라간다 — cut line 은 면을 interior/
   exterior 구간이 교대로 가른다. 실제 cut segment = **t-인접**(line 방향 정렬 시
   사이에 다른 On vert 없음) interior 구간. exterior 구간(U notch)을 잇는 chord 는
   cut 가 아니다 (초기 버그 #3).
   - On vert 를 cut line 방향 t 로 정렬 → (0,1),(2,3),… 쌍이 interior 구간 후보.
   - interior 판정 = chord midpoint 를 **cut line 수직(perp)으로 1% nudge** 한 점이
     polygon 내부인지 (midpoint 자체는 cut line 위 = 모든 On vert 와 동일 scanline
     → point-in-polygon degenerate, 초기 버그 #1).
   - `split_face(va, vb)` → ear(mono-side) + 나머지. 둘 다 worklist 재투입.
3. cut chord 는 `chords` 에 기록 → 기존 `assemble_loops` 가 cut loop 조립
   (각 On vert degree 2 = 인접 면과 공유).

`slice_volume_by_plane` 통합: 분류 단계에서 crossing 면을 2 On(`crossings`,
기존 볼록 경로) / >2 even On(`complex_crossings`, 신규) 로 분기. 볼록 경로 후
`complex_crossings` 를 `split_crossing_face_general` 로 처리 →
wall_above/wall_below/chords 에 합류. step 5.5(below 독립 detach) + step 6(양면
cap) 는 변경 없이 그대로 동작 (공유 On vert + chord 구조 동일).

`trim_volume_by_plane`(ADR-241)는 `slice_volume_by_plane` 를 호출하므로 **자동으로
비볼록 trim 지원** — 추가 코드 0. scene/WASM/TS surface 변경 없음 (C1 은 순수
engine robustness; slice+trim 경로는 ADR-241 에서 이미 wired).

## 3. Lock-ins

- **L-242-1** 비볼록 처리 = 반복 `split_face`(공유 edge 보존), remove+add clip 아님.
- **L-242-2** Leaf 판정 = **mono-side**(off-plane vert 한쪽), On vert 개수 무관.
- **L-242-3** Cut segment = **t-인접 interior 구간**(perp-nudge point-in-poly).
  exterior gap(notch) chord 거부.
- **L-242-4** 볼록 경로(2 On → `crossings` → 단일 `split_face` + side check)는
  **변경 없음** — 기존 slice 회귀 보존.
- **L-242-5** split sub-face 는 parent material + surface 상속(`split_face`,
  ADR-089 A-χ) + split HE HARD flag(메타-원칙 #15).
- **L-242-6** `trim_volume_by_plane` 자동 상속 — C1 추가 코드 0.
- **L-242-7** MVP scope 유지: 구멍 없는 닫힌 볼륨(C2 별도). odd On count →
  self-touching/degenerate 로 bail.
- **L-242-8** iteration guard(10⁵)로 무한 split 방어.
- **L-242-9** 메타-원칙 #6(de-risk) / #16(명시 trigger) / ADR-046 P31 #4 additive.
- **L-242-10** 절대 #[ignore] 금지.

## 4. 회귀

- axia-geo `slice_volume` +2 → **12 PASS**:
  - `slice_u_prism_nonconvex_cap_through_notch` — U-prism(8-vert U footprint
    extruded) 를 y=15 수직 평면으로 절단(각 U-cap 4 On verts = 비볼록). 2 cut loop
    (2 prong) + 양쪽 2 cap + 양 절반 closed(boundary_edge_count 0) + ADR-007
    invariants valid.
  - `trim_u_prism_nonconvex_keep_below` — 동일 U-prism trim keep-below → kept
    closed + 모든 vert y<16(prong tip 제거) + invariants valid.
- axia-geo 전체 1993 lib + 12 slice + 기타 = 회귀 0.
- WASM 재빌드(SIMD 11077). scene/WASM/TS/vitest 변경 0 (engine-only).

## 5. 검증 (engine + browser)

- **Engine**: 위 2 integration 테스트 (U-prism notch 관통 slice + trim).
- **Browser (real WASM round-trip)**: dev 서버에서 `drawPolylineAsShape`(닫힌 U
  8-vert) → `createSolidExtrude`(+Z 100) → `promoteShapeToXia`(강철) → XIA 소유
  10-face U-prism → `trimVolumeByPlane`(y=15, normal +Y, keep_below) →
  **kept 10-face closed solid, invariants valid(0 violations), centroid Y∈[0,15],
  2개 cut cap(prong별)** = 2 cut loop 정확 조립. 비볼록 slice 가 bail 없이 valid
  manifold 생성 확인.

## 6. Lessons

- **L1 de-risk 3-iteration (정석 알고리즘의 함정)**: 비볼록 polygon-line split 은
  표준이지만 3개 함정을 순차 발견 — (#1) chord midpoint 가 cut line 위라 모든
  On vert 와 같은 scanline → point-in-polygon degenerate → **perp nudge**; (#2)
  ear 절단 후 base 가 mono-side 인데 경계에 4 On vert → leaf 조건은 On 개수 아닌
  **mono-side**; (#3) consecutive-boundary 페어링이 base 가로지르는 spurious
  diagonal 오인 → **t-인접 cut-segment 페어링**. 테스트가 각 함정을 노출(메타-원칙 #6).
- **L2 split_face 의 surgical 안전성**: edge 제거 없는 boundary surgery 라 인접
  면과의 공유 edge 가 안전 → remove+add clip(공유 edge 제거 위험)보다 비볼록에
  적합. 반복 적용으로 다중 cut segment 처리.
- **L3 trim 의 후행 자동 상속**: `trim_volume_by_plane`가 slice 를 호출하므로 C1
  추가 코드 0 으로 비볼록 trim 까지 확보 — ADR-241 의 "slice 2-볼륨 산물 재사용"
  가치가 C1 에도 연장(Pattern-12).
- **L4 engine-only robustness = surface 무변경**: slice/trim 경로(scene/WASM/TS)는
  ADR-241 에서 wired — C1 은 `slice_volume_by_plane` 내부만 개선 → 브라우저가
  새 동작을 쓰려면 WASM 재빌드만 필요. UI 변경 0.

## 7. 후속 (Phase 1 나머지)

- **C2** 구멍 있는 솔리드 slice (hole loop above/below/straddle, `slice.rs:111`
  `face.inners()` 거부 해소). Phase 1 마지막 sub-step. ADR-243?
- (이후 Phase 2 Punch 확장 / Phase 3 Extrude 완성 — ADR-240 로드맵).

## 8. Cross-link

- ADR-240 (로드맵 Phase 1) / ADR-241 (C5 trim — 자매 sub-step, trim 자동 상속) /
  slice_volume_by_plane (slice.rs) / `Mesh::split_face` (mesh.rs:8230 — 반복 적용) /
  ADR-007 (manifold invariants) / ADR-089 A-χ (split surface 상속) / ADR-101 A9
  (split HE HARD flag, 메타-원칙 #15) / ADR-050 (promoteShapeToXia — browser smoke).
- 메타-원칙 #6 (de-risk) / #15 (split contract) / #16 (명시 trigger) /
  ADR-046 P31 #4 (additive) / LOCKED #44 (Complete Meaning per Merge).

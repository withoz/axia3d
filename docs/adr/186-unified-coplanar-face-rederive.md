# ADR-186 — Unified Coplanar Face Division via Planar Re-Derive (α spec / plan)

**Status**: Accepted (전략 B "유도면 모델 port" 결재 2026-06-01 "진행합니다").
Phase 3 (boundary_kernel 신설) 부터 atomic 구현. 임시 패치(전략 A Phase 1) 생략.
**Date**: 2026-06-01
**Author**: WYKO + Claude
**Trigger**: 사용자 (2026-06-01):
> "면사라짐과 면분할 안됨이 반복됨 ... 어떤 원칙으로 만들어진 결과인가?"
> (결재) "A. 통합 원칙 구현 (rect/polygon containment + drop 차단)"
> "E:\axia-sketch의 동작을 확인 ... 통합원칙 구현 계획을 세워주세요"
> "D:\AixiAcad의 엔진의 루틴도 확인 ... 면생성, 분할, 통합, 객체지우기"
**Reference engines**:
- **`D:\AixiAcad` `crates/xia-form/src/boundary_kernel`** — **production-grade
  (primary port 대상, P5.UX.53)**. Bentley-Ottmann robust sweep + 3-branch
  XIA inheritance + containment nesting + drift absorption.
- `E:\axia-sketch` `crates/axia-graph` (SketchGraph) — 단순 버전 (개념 검증).

---

## 1. Problem — 현재는 "정책 패치워크" (단일 원칙 없음)

실측으로 확인된 불일치 매트릭스:

| 케이스 | 원(circle) | 사각형(rect) |
|---|---|---|
| partial overlap (겹침) | ✅ 분할 (ADR-101) | ✅ 분할 (ADR-101) |
| **containment (안에)** | ✅ ring+disk (ADR-185) | ❌ **분할 안 됨 (GAP)** |

추가로 **"면 사라짐"** = auto_intersect/split 의 edge-case 가 degenerate/뒤집힌
winding sub-face 를 만들어 drop (복잡한 multi-overlap).

**근본**: "임의의 coplanar 닫힌 경계가 임의의 면을 일관 분할" 하는 **단일
통합 원칙이 없음.** case-by-case handler (ADR-101 / ADR-185) + GAP + drop 버그.

---

## 2. axia-sketch 의 통합 원칙 (reference)

`crates/axia-graph/src/lib.rs` — face 를 **edge graph 에서 매번 RE-DERIVE**:

### 2.1 `find_all_faces_on_plane` (317) — planar graph face traversal
1. plane 위 edge 수집 → half-edge 2개씩 (fwd/bwd)
2. vertex 별 outgoing HE 를 **angle 순(CCW) 정렬** (plane_basis u/v)
3. `next` = twin 직전(CCW) outgoing → **leftmost-turn** face traversal
4. cycle 추적 → boundary
5. **signed area > 0 (CCW interior)** 만 face, CW(외부)/degenerate skip

→ 모든 **minimal CCW cycle = face**. shape 무관 (선이 만든 모든 닫힌 영역).

### 2.2 `find_faces_with_holes_on_plane` (482) — containment hole 자동 부착
- 각 face 의 **smallest strict container = 직속 부모** (point_in_polygon_uv,
  모든 vertex 안 + vertex 공유 X)
- 자식 polygon 을 부모의 **hole** 로 부착
- **자식도 own face (holes=[]) 로 함께 반환** (line 481) → **ring + inner disk**
- 재귀 nested (O.holes=[M], M.holes=[I], I.holes=[])

### 2.3 핵심 통찰
axia-sketch 는 **incremental 패치 0** — edge 만 등록(add_edge_with_intersections,
항상 성공) 하고, 렌더/op 시점에 face 를 **전부 re-derive**. 그래서:
- 일관성: 어떤 shape 든 닫힌 영역 = face (case 분기 없음)
- containment: strict-inside = hole + own face (ring + inner, shape 무관)
- drop 없음: 매번 깨끗이 re-derive (degenerate 는 signed-area filter 로 자연 제거)

### 2.4-PRE ⭐ AixiAcad ADR-057 "유도면 모델" — 오늘(2026-06-01) 완성·검증된 통합 원칙

**가장 결정적 발견.** AixiAcad 가 **오늘 13 commits 로 통합 원칙 전체를 구현 +
시각 검증 완료** (`유도면 모델` = Derived-Face Model). trigger 가 우리 사용자님과
**정확히 동일**:
> "선 하나 지웠더니 주변면이 모두 사라진다. 단순 버그가 아니라 복잡한 메커니즘
> 문제 아닌가." (ADR-057 trigger = 우리 "면사라짐" 과 동일 증상)

**불변식 (ADR-057 §3, canonical)**:
> *엣지 그래프(sketch_lines + 면 위 엣지)가 평면 면의 **단일 진실원천**. 모든
> 평면 편집(그리기/분할/지우기/면삭제)은 엣지를 변경 → **면 재유도** →
> FaceLineage 로 속성 재매핑.*

**핵심 통찰** (ADR-057 §2): "**우리는 이미 면을 유도한다 — 새 커널이 필요한 게
아니다.**" 면을 *저장 후 패치* 하는 대신 edge graph 를 단일 진실원천으로
**승격** + 덧댄 patch (좌표 coupling / coalesce / suppression) 제거.

| 오늘 commit | 내용 |
|---|---|
| ADR-057 P1 | 선 삭제 = 선 제거 + rebuild (주변면 전멸 해소) |
| ADR-057 P2 | 다중삭제 rebuild + K3 지우개 + 사각형/Freehand 교차분할 정합 |
| ADR-057 P3 | 좌표 coupling 제거 (`faces_bounded_by_line` 死API 정리) |
| ADR-057 P4 | 요철/T-junction/hole corpus + rebuild 벤치 (≤36ms) |
| ADR-057 error01 fix | 과병합 = **평행(중복) edge** → `dedup_parallel_edges` (벡터 이동 0) |
| L-P1/P2/P3 | 닫힌곡선 면분할 — region nesting + mesh 구멍 + raw Circle→loop edge |
| ADR-058 | NURBS point-inversion (Gauss-Newton) — 곡면 위 곡선분할 정밀도 |
| ADR-059 | disk-cutter mutual-split — 원이 사각형 엣지로 분할 (비대칭 해소) |

**FaceLineage** (Preserved/Split/Merged/Removed) — rebuild 후 면 ID/선택/material
재매핑 인프라 **이미 존재** (`run_boundary_split_z0_preserving_selection`).
**검증**: 사용자 시각 "매우 잘된다" + xia-form **1022 tests PASS**.

→ **우리 AxiA 3D 의 통합 원칙 = AixiAcad ADR-057 유도면 모델 port.** 오늘
완성·검증된 proven 모델이라 위험 추가 ↓. 단 우리 엔진은 *저장-후-패치*
(incremental) 이므로, AixiAcad 가 "이미 유도함" 과 달리 **run_boundary_split
동등물(boundary_kernel)을 신설**해야 함 (Phase 3) — 그 후 동일 모델.

### 2.4 AixiAcad boundary_kernel (production-grade — primary port 대상)

`D:\AixiAcad/engine/crates/xia-form/src/boundary_kernel/` (P5.UX.53, 2026-05-19
~ L-P1 2026-06-01). **사상**:
```
Loop Input → Edge Graph → Intersection Resolve → Planar Partition → Face Reconstruction
```
> "단일 알고리즘이 모든 닫힌 boundary 케이스 처리 (Rectangle/Line cycle/Circle/
> BOUNDARY click)." (mod.rs)

axia-sketch 와 동일 사상의 **성숙한 완성본** — 우리 엔진에 거의 그대로 port 가능:

| 모듈 | 역할 | axia-sketch 대비 |
|---|---|---|
| `bentley_ottmann.rs` | **robust sweep-line** intersection resolve | naive O(N²) → **O((N+K)logN) robust** |
| `planar.rs` | PlanarGraph (quantized weld + **Lineage** edge-split 추적) | edge split 추적 강화 |
| `region.rs` `extract_regions_nested` | planar face 추출 + **containment nesting → RegionWithHoles** (annulus+disk) | **self-touching split (Patch A)** + 3단 nesting |
| `robust_split.rs` `robust_split_2d` | **3-branch classification** (shares_edge / all_inside / centroid-fluke) → **material/surface 상속** | XIA 상속 정식화 |
| `face_coalesce.rs` | coplanar 인접 face **통합** (Union-Find + boundary chain) | 통합(coalesce) 정식화 |

**통합점** (`form/form.rs::run_boundary_split_on_plane`):
1. plane 위 mesh vertex → UV(2D) 사영, **scale-aware eps + plane_tol 0.05mm**
   (drift 흡수 — "Rect 그렸는데 면분할 안 됨" 의 정확한 fix, 2026-05-30)
2. **dirty snapshot** — plane 위 기존 face 의 material/surface/polygon (DirtyFaceInfo)
3. PlanarGraph 빌드 → `robust_split_2d` → FaceOut (re-derive + 상속)
4. **rebind** — dirty face 제거 + 새 FaceOut face DCEL 등록

**객체 지우기** (`run_boundary_split` 와 정합):
- `remove_planar_face` / `remove_sketch_line` → `remove_stale_faces_around` (인접
  stale face 정리) → boundary split 재실행 → **face 자동 재-derive** (ADR-019
  "Erase는 깨고 다시 만든다" 와 동일 사상)

**핵심**: AixiAcad 는 우리가 ADR-186 으로 *설계* 하려던 것을 **이미 production
구현 + 통합 + drift 흡수 + 통합(coalesce) + erase 정합**까지 완성. 우리 엔진의
Phase 3~4 (re-derive core) 는 **from-scratch 설계가 아니라 AixiAcad port** →
위험 대폭 감소.

---

## 3. 우리 엔진의 gap — incremental DCEL vs re-derive

| | axia-sketch | 우리 엔진 |
|---|---|---|
| Face 생성 | edge graph 에서 **re-derive** (매번) | **incremental DCEL surgery** (draw → patch) |
| Containment | strict-inside → hole + own face (자동, shape 무관) | circle 만 (ADR-185), rect GAP |
| Partial overlap | re-derive 자연 결과 | auto_intersect_coplanar (ADR-101, case별) |
| Drop | signed-area filter 로 자연 제거 | edge-case 버그로 drop |
| 곡선(Path B) | (평면 sketch, polyline) | Path B self-loop edge + AnalyticCurve |

우리 엔진은 Path 1-4 (ADR-169~173) 로 **edge 등록 측면** (Pattern 3/5, crossing-
split) 은 axia-sketch 화 했으나, **face re-derive 측면** (Phase B/C — 모든 face
검출 + auto-hole) 은 미구현 → 불일치의 근본.

---

## 4. 통합 원칙 (target) + 구현 계획

### 4.1 Target — `rebuild_coplanar_faces(plane)` (axia-sketch Phase B/C 동등)
draw 후 해당 plane 의 모든 coplanar edge 로부터 face 를 re-derive + containment
hole 자동 부착. case-by-case 패치(auto_intersect/annulus) 를 단일 routine 으로
수렴. **shape 무관 일관 + drop 자연 제거.**

### 4.2 Phased implementation (Path Z atomic) — 위험 격리

**Phase 1 — General containment (즉시 gap 해소, 저위험)**
- ADR-185 `split_face_by_inner_circle` → **`split_face_by_inner_loop`** (임의
  닫힌 loop: circle/rect/polygon. inner loop 의 twin HE 들 → outer hole).
- `detect_circle_containment` → **`detect_face_containment`** (strict
  point-in-polygon, shape 무관. Path B circle 은 polygonize 후 판정).
- 기존 scene containment scan 재사용 (auto_intersect_coplanar 전).
- 결과: rect-in-rect / polygon-in-polygon / circle-in-rect 모두 ring+inner.
- 위험: 낮음 (ADR-185 확장, incremental 유지). ~1주.

**Phase 2 — Drop-bug audit + fix (면 사라짐)**
- 복잡 multi-overlap scene 에서 "면 사라짐" 재현 → degenerate/winding drop
  구체 원인 진단 (ADR-183 류).
- auto_intersect/split 의 sub-face drop 차단 (signed-area guard / winding 정정).
- 위험: 중간. ~1-2주.

**Phase 3 — Planar re-derive core (AixiAcad boundary_kernel port, 진짜 통합)**
- **AixiAcad `boundary_kernel` port** — `geom2` + `planar` (PlanarGraph +
  Lineage) + `bentley_ottmann` (robust intersection) + `region`
  (extract_regions_nested) + `robust_split` (3-branch 상속). zero-dep,
  deterministic (BTreeMap), 우리 `crates/axia-geo/src/boundary_kernel/` 신설.
- Path B 곡선 edge → polygonize 후 graph, 결과 face 에 AnalyticCurve metadata
  재부착 (ADR-089 답습).
- 위험: 중간(↓, from-scratch 아닌 **검증된 port**). 다중 LOCKED (#1/#12/#41/#64)
  + 245+ 회귀 + 성능. ~2-4주.

**Phase 4 — DCEL reconcile + XIA inheritance (AixiAcad run_boundary_split 패턴)**
- AixiAcad `run_boundary_split_on_plane` 패턴 답습 — **dirty snapshot**
  (material/surface/polygon) → kernel → **rebind** (dirty 제거 + FaceOut 등록).
  XIA 승계는 `robust_split_2d` 3-branch (shares_edge / all_inside / fluke).
- drift 흡수 — AixiAcad plane_tol 0.05mm 답습 (우리 LOCKED #5 1.5μm + ADR-168
  PLANE_SNAP 와 정합).
- Path B 곡선 metadata 재부착 + undo/snapshot 정합.
- 위험: 중간(↓). ~2-3주.

**Phase 5 — Wire + flag + 12-scenario gate**
- draw 파이프라인 wiring (flag-gated, ADR-176 pattern: engine OFF + prod ON).
- auto_intersect/annulus 를 re-derive 로 수렴 (또는 공존).
- 12 scenario 일관 gate (circle/rect/polygon × partial/containment/nested +
  면 사라짐 0). 사용자 시연 (ADR-087 K-ζ).
- 위험: 중간. ~1-2주.

### 4.3 두 전략 — 결재 포인트
- **전략 A (Phase 1+2 만)**: incremental 확장 — rect containment + drop fix.
  빠름(2-3주), 저위험. 단 "패치워크" 의 근본 architecture 는 유지 (re-derive
  아님). 향후 또 다른 gap 가능.
- **전략 B (Phase 1~5 전체)**: 진짜 re-derive 통합 (**AixiAcad boundary_kernel
  port**). 근본 해결 + 일관성 + drop 자연 제거. AixiAcad 가 검증된 port 대상
  이라 위험 ↓ (from-scratch 아님). multi-week (7-11주) + 중위험 (LOCKED/회귀/
  성능). **AixiAcad 가 이미 모든 케이스 + containment + 통합 + erase 정합 완성**
  → 우리 엔진은 Path B 곡선 + DCEL rebind + LOCKED 정합만 추가.

**권장**: Phase 1+2 먼저 (즉시 가치 + 저위험) → 사용자 시연 후 Phase 3~5
(AixiAcad port) 진입 여부 재결재. additive-first 위험 격리 (ADR-094 §E L1).

---

## 5. Risks / 고려사항

- **LOCKED 정합**: #1 P7 (containment split — 본 ADR 이 일반화), #12 P11, #41
  ADR-101 (partial overlap — re-derive 로 수렴 or 공존), #64 ADR-139/#16 (자동
  trigger — ADR-176 flag 답습), #44 (의미 단위 per phase).
- **Path B 곡선**: re-derive 는 polyline graph — Path B circle/arc 는 polygonize
  후 traversal, 결과에 AnalyticCurve 재부착 (ADR-089). 정밀도 chord_tol 정합.
- **성능**: re-derive per draw — plane 별 edge 수에 O(E log E). 대규모 scene
  caching/dirty-plane 전략 (axia-sketch telemetry 답습).
- **회귀**: 245+ axia-core + LOCKED 회귀 자산. phase 별 atomic + 절대 #[ignore]
  금지.
- **XIA 승계**: re-derive 시 face ↔ XIA 매핑 (signature). undo/snapshot 정합.

---

## 6. Cross-link
- axia-sketch `axia-graph` (find_all_faces_on_plane / find_faces_with_holes —
  reference algorithm)
- ADR-101 (partial overlap auto-intersect — re-derive 로 수렴 대상)
- ADR-185 (circle containment ring+disk — Phase 1 일반화 base)
- ADR-169~173 (Phase 1-4 boundary routine — edge 등록 측면 완료, face re-derive
  측면 본 ADR)
- ADR-021 P7 / ADR-025 P11 (closed boundary → face, LOCKED #1/#12)
- ADR-139 / 메타-원칙 #16 (자동 trigger 정책) / ADR-176 (auto default ON)
- ADR-089 (Path B closed-curve — re-derive 곡선 처리)
- 메타-원칙 #14 (면은 닫힌 경계로부터 — 통합 원칙의 WHAT) / #4 SSOT
- ADR-094 §E L1 (additive-first multi-week atomic) / ADR-087 K-ζ (시연 게이트)
- LOCKED #44 (Complete Meaning per Merge — phase 별)

---

## 7. Phase 4 사전 검토 (DCEL 통합, 2026-06-01)

Phase 3 (boundary_kernel β-1~β-4) **완료** — axia-geo 1552→1587 (+35, 0 reg).
임의 edge → 교차해결(B-O) → 면추출(region) + containment hole + 3-branch 상속.
AixiAcad 35 회귀 1:1. **kernel 알고리즘 layer 완결.** Phase 4 는 이 kernel 을
우리 DCEL mesh 에 연결.

### 7.1 핵심 아키텍처 차이 — DCEL-source vs sketch-lines

AixiAcad `run_boundary_split_on_plane` 감사 결과:

| | AixiAcad | 우리 AxiA 3D |
|---|---|---|
| Edge SSOT | `sketch_lines` (LineId SlotMap, **별도 layer**) | **DCEL half-edge 자체** (별도 layer 없음) |
| 면 source | sketch_lines → PlanarGraph → 유도 | DCEL face (저장) |
| 재유도 입력 | sketch_lines iterate | **coplanar DCEL edge collect** |

**결정 (권장): DCEL-source re-derive** — 우리 DCEL 이 *이미* edge 를 가지므로
별도 sketch_lines layer 도입 불필요. plane 위 coplanar edge (free + face
boundary) 를 모아 PlanarGraph 빌드 → kernel → FaceOut → DCEL reconcile.
sketch-lines layer 도입(대안)은 더 큰 변경 + 두 SSOT 동기화 부담 → 거부.

### 7.2 통합 패턴 (AixiAcad 답습 + DCEL 적응)

```
draw op (rect/circle/line) → 영향 plane P
  ↓
1. dirty snapshot: P 위 coplanar sheet face (Volume 제외) → DirtyFaceInfo
   (polygon UV + XIA/material + signature)
2. PlanarGraph 빌드: P 위 coplanar DCEL edge → project_to_uv → create_edge
   (Path B circle = polygonize)
3. robust_split_2d: B-O 교차해결 → region → FaceOut (3-branch 상속)
4. reconcile: dirty sheet face 제거 (Volume/solid 보호, twin-safe) +
   FaceOut → add_face_with_holes (uplift_to_3d) + XIA 재매핑 (FaceLineage)
   + Path B circle metadata 재부착
```

핵심 API (우리 엔진 보유 확인):
- `Mesh::add_face_with_holes` (3120) — annulus 등록
- `Mesh::face_outer_edges` (5221) / `are_faces_coplanar_with_tolerance` (5907)
- `Mesh::collect_free_edge_segments` (5689) — free edge
- Hook: `Scene::intersect_faces_inner` (1858) — ADR-101/185 현 hook 자리.
  re-derive 가 auto_intersect/annulus 를 **수렴**.
- drift: plane_tol 0.05mm (AixiAcad) ↔ 우리 LOCKED #5 1.5μm / ADR-168 PLANE_SNAP.

### 7.3 Sub-step 분해 (δ, additive-first ADR-094 §E L1)

| δ | 내용 | 위험 | 검증 |
|---|---|---|---|
| **δ-1** | `Mesh::rebuild_coplanar_faces(plane)` — coplanar DCEL edge → kernel → reconcile (polygon only, Path B 제외). **isolated Mesh-level + 회귀** | 중간 | 합성 mesh (rect partial/containment) |
| **δ-2** | Path B 곡선 — circle self-loop edge polygonize + 결과 face 에 Circle metadata 재부착 (ADR-089) | 중간 | Circle 분할 회귀 |
| **δ-3** | XIA/material 상속 — FaceLineage (signature centroid+area) rebuild 후 재매핑 | 중간 | XIA 보존 회귀 |
| **δ-4** | Scene wiring — flag `face_rederive_on_draw` (engine OFF + prod ON, ADR-176) + `intersect_faces_inner` hook + 3D solid 보호 (sheet only) | **높음** | 245+ 회귀 보존 |
| **δ-5** | Demo + 12-scenario gate (circle/rect/polygon × partial/containment/nested + 면사라짐 0) + 사용자 시연 (ADR-087 K-ζ) | 중간 | 실 브라우저 |

**δ-1 먼저** — isolated Mesh-level 함수로 kernel↔DCEL 변환 (project/uplift +
reconcile) 만 검증. draw wiring (δ-4, 고위험) 은 격리. LOCKED #44 의미 단위
per δ.

### 7.4 위험 / 미해결

- **3D solid 보호**: Volume-attached wall face 는 re-derive 제외 (sheet only,
  AixiAcad twin-safe 답습). manifold 보존.
- **LOCKED 정합**: #1 P7 / #12 P11 (ADR-139 supersede — 결과 invariant 보존) /
  #41 ADR-101 (auto_intersect 수렴) / #15 HARD flag (split edge contract).
- **성능**: re-derive per draw — plane 별 edge O(E log E). 대규모 dirty-plane
  scope (AixiAcad O(faces²) hole-containment 병목 인지).
- **undo/snapshot**: reconcile 가 단일 transaction.
- **default OFF**: engine 회귀 자산 245+ 보존 (flag OFF), prod localStorage ON.

### 7.5 결재 포인트
1. **DCEL-source re-derive 채택** (§7.1) — sketch-lines layer 대신. ✅ 결재 2026-06-01.
2. **δ-1 부터 진행** (isolated Mesh-level, 저위험 먼저). ✅ δ-1 완료.
3. δ-4 (Scene wiring) 는 별도 결재 (고위험, 245+ 회귀).

---

## 8. 정책 Collapse 계획 (사용자 결재 2026-06-01 "권고로 진행 (c)")

사용자 통찰: "내 정책도 틀렸는지 확인." → 비판적 검토 결과 **자동-동작 정책들이
증상 치료였음** 을 확인. 유도면 모델 통합의 *진짜 가치*는 버그 수정이 아니라
**정책 표면 collapse**.

### 8.1 증거 — 정책 flip-flop 이 증상 치료의 직접 증거

| 정책 | 내용 | 성격 |
|---|---|---|
| LOCKED #41 ADR-101 | 겹침 → 자동 3분할 | 통합 알고리즘 부재의 patch |
| LOCKED #64 ADR-139 | 자동 OFF ("휴리스틱 antipattern") | patch 의 역방향 patch |
| LOCKED #76 ADR-176 | 자동 ON ("이제 견고") | 또 역방향 (2주 내 flip) |
| ADR-185 | 원 containment → ring+disk | case별 patch |

**ADR-139 (OFF) → ADR-176 (ON) 2주 flip** = 정책이 *근본(통합 알고리즘 부재)*이
아닌 *증상*을 다룬 직접 증거. 4 정책 모두 "임의 닫힌 경계 → 일관 면" 알고리즘이
있었으면 불필요.

### 8.2 Collapse 목표 (유도면 통합 = δ-5 완료 후)

re-derive 가 모든 케이스를 case 분기 없이 처리하면:
- **ADR-101 / ADR-185** (자동 분할/containment) — re-derive 로 **수렴 → 은퇴**
  (case별 handler 삭제, `auto_intersect_coplanar` / `detect_circle_containment` /
  `split_face_by_inner_circle` deprecated)
- **ADR-139 / ADR-176** (자동 trigger ON/OFF flip) — re-derive 가 단일 trigger
  (draw → 영향 plane rebuild) 로 **통합 → flip 정책 무의미화**
- **메타-원칙 #16** (휴리스틱 antipattern) — *보존* (re-derive 는 휴리스틱 아닌
  **결정적 알고리즘** → #16 정합, 위반 아님)
- **메타-원칙 #14** (면은 닫힌 경계로부터) — re-derive 가 **가장 깊은 실현**

**순감**: 4 정책 + 다수 case-handler 코드 삭제. 정책 표면 축소 = "가벼운" 목표 정합.

### 8.3 프로세스 cadence 경량화 (내부 작업)

사용자 결재 — 내부(non-user-facing) 작업은 승인 cadence **batch**:
- Phase 3 (kernel port) 같은 사용자 facing 변화 0 작업 → atomic sub-step 묶어서
  1 결재 (개별 β/δ 마다 승인 X)
- 무거운 governance (개별 결재 + 시연 게이트) 는 **user-facing / 위험 변경** (δ-4
  Scene wiring, δ-5 demo) 에만.
- ADR-087 K-ζ 시연 게이트는 user-facing 단계 유지.

### 8.4 Collapse 시점
- **지금**: 계획 명시만 (정책 변경 0 — 메타-원칙 #10 정합).
- **δ-5 완료 + 사용자 시연 PASS 후**: ADR-101/139/176/185 Superseded by ADR-186
  명시 + LOCKED 갱신 + case-handler 코드 삭제 (별도 결재).

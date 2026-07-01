# ADR-198 — Concave Subtract: box − curved (drilling + enclosed void)

- **Status**: Accepted
- **Date**: 2026-06-15
- **Track**: ADR-197 β-3 curved Boolean (follow-up — concave subtract)
- **Branch**: `adr-186/boundary-kernel-port` (LOCAL)

## 1. Context

ADR-197 β-3 의 curved subtract dispatch (`try_curved_subtract_dispatch`) 는
`curved − box` (analytic primitive 이 minuend, box 가 Z-cut tool) 만 처리했다.
**`box − curved`** (box 가 minuend, curved 가 제거되는 tool) — 즉 "박스에서
실린더/구를 빼서 **구멍/공동**을 만드는" 가장 흔한 CAD 연산 — 은 None 으로
defer 되어 ADR-197 #Track2 guard 가 clean Err 로 막기만 했다 (crash 방지).

사전검토 + 시뮬레이션 (2026-06-15, `sim_concave_subtract_feasibility` 으로
검증) 결과, concave subtract 의 본질은 **"pierce 의 inward 변형"** 이고
기존 머신리 (`sew_curved_band` / pierce / box_horizontal_faces) 재활용도가
높음을 확인했다. 두 최고가치 케이스가 watertight + manifold 로 빌드 가능:

| 케이스 | 결과 topology |
|---|---|
| **box − cylinder 관통** (drilling) | box 6면(top/bot에 hole) + bore band(inward Cylinder) = 7면, **genus-1** |
| **box − sphere/cyl enclosed void** | box 6면 + 곡면 2면(inward) = **2 disjoint shell** |

## 2. Decision (MVP-1 + MVP-2)

### Lock-ins

- **L-198-1** Dispatch — `try_curved_subtract_dispatch` 상단에 concave 분기:
  `faces_a` = axis box + `faces_b` = curved primitive → `try_concave_box_minus_
  curved`. `curved − box` (convex) 는 그대로 fall-through.
- **L-198-2 (drilling)** `boolean_box_minus_cylinder` — Z-axis cylinder 가
  box 를 XY-contain + Z-span ⊇ box (top+bottom 관통) → `Mesh::bore_through_box`
  (`sew_curved_band` 변형: 2 disk → 2 box-face hole, INWARD normal). genus-1.
- **L-198-3 (void)** `boolean_box_minus_void` — primitive AABB 가 box 안에
  STRICTLY inside → box + primitive 를 2-shell 로 유지, primitive face 를
  `face_surface_reversed` 로 mark (inward 렌더).
- **L-198-4** Partial 케이스 (side-pierce / cone countersink / scooped octant /
  sphere-through-box) → `None` (defer → #Track2 guard clean bail). 별도 sub-step.
- **L-198-4b (amendment, 2026-06-15)** Partial-depth concave 추가 — 시뮬레이션
  (`sim_partial_concave_feasibility`) 검증 후, 기존 pierce helper inward 변형으로:
  * **blind hole** `boolean_box_minus_cylinder_blind` — cylinder 가 box 의 한 Z-face
    로 진입(floor inside) → `pierce_face_with_band_stub`(INWARD band + floor disk).
    top/bottom entry 양쪽. 6 box + band + disk = 8면.
  * **dimple** `boolean_box_minus_sphere_dimple` — sphere 가 한 Z-face 관통(far
    side inside) → `pierce_face_with_cap`(INWARD sub-sphere cap, v_range clip).
    top/bottom poke 양쪽. 6 box + cap = 7면.
  신규 sew 0 (drilling/void 와 동일 머신리 재활용).
- **L-198-5** `Mesh.face_surface_reversed: FxHashMap<FaceId,bool>` — cavity
  face 의 surface normal 을 export 에서 negate (inward). Mesh-level map
  (ADR-091 §E L1, `#[serde(default)]`, bincode-safe). DCEL winding / geometry
  불변 — 렌더 normal 만 flip.
- **L-198-6** bore band 는 생성 시 inward-wound (DCEL 정확) — verify_face_
  invariants 통과. void sphere 는 outward-wound + reversed flag (render only;
  point-in-solid orientation 은 §4 follow-up).
- **L-198-7** ADR-197 #Track2 guard 보존 — 지원 케이스만 라우팅, 미지원은
  여전히 clean bail.

## 3. 시뮬레이션 / 검증

- `sim_concave_subtract_feasibility` (사전검토): box−cyl 관통 7면 watertight=0
  manifold=0 valid 208 tris / box−sph void 8면 watertight 2 shells.
- 회귀 `adr198_box_minus_cylinder_drilling` / `adr198_box_minus_sphere_
  enclosed_void` / `adr198_dispatch_box_minus_curved_routes` (partial defer
  포함). axia-geo 1806 → **1809** (+3, 절대 #[ignore] 0).
- #Track2 test 갱신: box−sphere(strictly inside) 는 이제 void 로 라우팅 →
  test 를 dimple (still unsupported) 로 변경.

## 4. Out of scope (follow-up)

- **blind hole / dimple** — ✅ **구현됨** (L-198-4b amendment, 2026-06-15).
- **cone countersink** — ✅ **구현됨** (L-198-4c amendment, 2026-06-15):
  `boolean_box_minus_cone_countersink` — Z-cone 의 apex 가 box 안 + base 가 한
  Z-face 관통 → `pierce_face_with_cap`(INWARD sub-cone cap, apex degenerate,
  v_range clip). apex-down(axis +Z) top pocket / apex-up bottom pocket. 7면.
- **나머지 partial concave**: non-Z-axis cylinder/cone / side-pierce
  (cylinder/sphere/cone 가 box 측면 관통).
- **sphere − corner-box (scooped octant)** — octant intersect 의 inverse.
- **Void orientation correctness** — void primitive 의 DCEL winding reversal
  (point-in-solid / downstream Boolean 정합). 현재 render-only flip.
- **`face_surface_reversed` 의 split 상속** — split_face 가 cavity flag 전파.
- **Demo/UI**: 사용자 메뉴에서 box−cylinder 시연 (production `boolean_op` 경로
  는 자동 라우팅).

## 5. Cross-link

ADR-197 (β-3 curved Boolean) §Track2 (concave guard) · `sew_curved_band` /
pierce 머신리 (β-3-p Union) · ADR-091 §E L1 (Mesh-level map) · ADR-038 P23.5
(surface-aware normal — export negate) · ADR-018 (two-tone render) · 메타-원칙
#5/#6/#14.

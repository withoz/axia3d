# ADR-274 — Sameness Recognition Coherence + Flush-Collapse

**Status**: Accepted (엔진·배선 구현 완료; 브라우저 런타임 시연 pending — §5)
**Track**: Kernel Robustness / UX Precision
**Cross-link**: ADR-147(spatial-hash dedup 0.15μm) · ADR-167(plane.rs EPS SSOT) ·
ADR-101(mergeCoplanarContaining) · ADR-193(Live Push/Pull) · ADR-049/050(Two-Layer
Citizenship — Form/Property) · promote.rs(Volumetric = watertight ∧ vol>0) ·
메타-원칙 #4(SSOT) #5(UX) #6(preventive) #9(무회귀) #13(one source two views) #14(면=닫힌경계)

---

## 1. Context — 사용자 목표

> "명령이 작동할 때 스냅도 정밀하게 — 엔진이 서로 '같은 면 / 같은 높이(평면) /
> 같은 라인(엣지) / 같은 점' 이라고 신뢰성 있게 인식하도록, 필요하면 엔진이
> 조정하도록 정합하게."

두 갈래로 진행:
- **Part A** — 엔진 곳곳에 흩어진 "sameness" 허용오차가 정합(coherent)한지 감사 + 정리.
- **Part B (d)** — "보스/포켓을 flush(height→0)로 되돌리면 깨끗한 면이 돼야 하는데
  찌그러진 흔적이 남는다" 문제의 근본 해결.

핵심 방법론: **실측 먼저(B-first)** — grep audit 의 추정을 headless 시뮬레이션으로
검증. 이 과정에서 audit 의 방향 가정이 **여러 번 교정**됨 (아래).

---

## 2. Part A — Sameness Tolerance Coherence

### 2.1 감사 발견 (raw grep) vs 실측 (runtime-corrected)

세 "같음" subsystem 을 통제 입력에 실제로 돌린 수용 반경 (offset `d`):

| subsystem | 수용 반경 | 용도 |
|---|---|---|
| vertex dedup (`add_vertex`) | **0.15μm** (SPATIAL_HASH_CELL×1.5) | 생성 시 병합. **생성 전용** — 옮긴 vert 소급 병합 0 |
| auto-intersect coplanar (`coplanar.rs`) | 1.5nm | 교차 geometry 계산 (의도적 초정밀) |
| merge-gate coplanar (`are_faces_coplanar_with_tolerance`) | scale-aware, 1000mm 면 0.5° → **~15mm** | 면 병합 |

raw grep 은 "1.5nm ~ 15mm = 10⁷× 스프레드" 로 위험해 보였으나, 실측이 audit 을
**4회 교정**:
- **#5 (merge offset-blind) 과장** — merge-gate 는 scale-aware offset 검사를 함
  (`dist_tol = max(bbox×1e-5, 1e-3, bbox×sin(tol)×1.2)`).
- **#4 방향 뒤집힘** — audit 은 "coplanar 가 dedup 보다 엄격" 예상했으나 merge 경로는
  **정반대**(dedup 보다 10⁵× 관대).
- **(b) 1.5nm 완화 = 부정합 아님** — ADR-167 β-2/β-3 의 **의도적** 설계 + test-lock
  (`adr167_b3_preserve_strict_coplanar_offset_tol`) + 메타 #10. **미진행**.
- **#6 (web COS_THRESHOLD 5.7°) 오분류** — 평면 판정 아니라 엣지 방향 클러스터링.

**결론: 진짜 버그는 #8 하나.** 나머지는 목적별로 정합했거나 의도적 예외.

### 2.2 #8 — merge-gate 과관대 (실측된 유일한 오병합)

`bbox×sin(tol)×1.2` 항이 1000mm 면·0.5° 에서 **~15mm 떨어진 평행 면도 coplanar**
로 판정 → silent 오병합. **Fix**: 최근접-정점 게이트 추가 —
`min_off < base_tol AND max_off < dist_tol`. 진짜 coplanar 면 한 정점은 평면에
닿아야(min≈0); 평행-offset 면(전 정점 offset≈gap)은 거부. shared-edge tilt(min=0)·
same-plane(drift) 는 보존.

### 2.3 SSOT 정리 + 문서 정정

- **carve.rs** `COPLANAR_DOT 0.999`(2.56°) → `1.0 - plane::EPS_PLANE_NORMAL`(0.9999),
  `COPLANAR_OFFSET` → `plane::EPS_PLANE_OFFSET`. behavior-preserving (offset 게이트
  지배, 2155+433 PASS 증명).
- **boundary.rs** `POINT_ON_PLANE_TOL_MM` → `plane::EPS_PLANE_OFFSET` re-export (중복
  2번째 canonical 소스 제거).
- **문서**: dedup 문맥 stale "1.5μm"→0.15μm (mesh.rs 5 + intersect.rs 1; mesh.rs:3717
  은 산술 오류였음). cleave.rs "1.5μm"→1.5nm. web `constants.ts` `DEDUP_TOLERANCE=1.5e-4`
  추가 (지배적 "같은 점" 스케일 명시).

### 2.4 스냅 정밀도 (tool)

`SnapManager.findAlignedDistance` 가 정렬 거리를 raycast hit(면에서 sub-μm 벗어남)
기준으로 재던 것을 **소스 면의 exact 평면(refPt) 기준**으로 정정 → 스냅 시 이동 면이
대상 feature 에 bit-exact 안착.

---

## 3. Part B (d) — Flush-Collapse

### 3.1 Ontology — height 0 은 면인가 입체인가?

**면(Face)이다.** `promote.rs`: Volumetric = watertight AND enclosed volume > 0.
height 0 → 부피 0 → `ZeroVolume` → 입체 아님 → Face (Two-Layer 형태 계층 "0 두께
자연스럽다" 정합).

### 3.2 근본 원인 (실측 ground truth — window-pane)

boss 를 z=0 으로 되돌린 직후: **9면 중 4개 퇴화(area=0) 벽 + coincident-but-distinct
정점 4쌍**. dedup 은 **생성 시에만** 작동 → 옮긴 cap 정점이 rim 정점과 겹쳐도 병합
안 됨 → 퇴화 벽 잔존 → solid 안 닫힘. (부피 0 인데 벽 뼈대가 남은 "입체인 척" 상태.)

### 3.3 판별 규칙 (A/B 분기 불필요)

- **트리거 = 퇴화(0넓이) 면 존재** (정상 메시엔 없음).
- **slice 보호** — slice/knife `add_vertex_force_new` 의 의도적 coincident-distinct
  정점은 **퇴화 다리가 없어** 트리거 안 됨 → 자동 제외.
- **survivor(rim) = 비-퇴화 면에 더 많이 붙은 정점**; cap 정점을 거기로 흡수.
- **Face/Volume 은 collapse 후 `geometry_state()` 가 자동 재분류** (연산은 A/B 동일).

### 3.4 연산 (rebuild 접근, gate-guarded)

`Mesh::collapse_flush_extrusion(area_tol)` — **검증된 remove_face/add_face 만** 사용
(hand-rolled half-edge 수술 회피, "그 부류가 버그 6개 낳음" 교훈):
1. 퇴화 면 = 벽. 없으면 no-op.
2. 퇴화 면 걸린 겹친 정점 그룹 → survivor 규칙으로 cap→rim map.
3. cap face(전 정점 loser) 를 rim loop 로 재구성: remove cap+walls → deactivate cap
   정점 → `add_face(rim)` (기존 rim edge 재사용 → 닫힘).
4. **gate fail-closed**: snapshot → boundary_edge_count 증가 OR non-manifold 증가 OR
   재구성 실패 시 `restore_snapshot` + Err. **절대 corruption 없음**(최악=no-op).

`Scene::collapse_flush_extrusion` 래퍼 — mesh op 후 Xia/Shape face 소유권 reconcile
(제거 면 드롭 + 신규 면을 dominant 전 소유자에게).

### 3.5 배선 (WASM→TS→tool)

- WASM `collapseFlushExtrusion(area_tol) -> JSON{ok,collapsed,error}` (transaction
  wrap = undo).
- `WasmBridge.collapseFlushExtrusion` 래퍼.
- **MoveTool** 커밋 후 자동 호출 (면을 flush 이동 시 정리).

---

## 4. Status & Commits (E:\AXiA3D main)

| commit | 내용 |
|---|---|
| `ea0e345` | 스냅 정밀도 (소스 면 refPt) |
| `ce6c3ba` | merge-gate #8 fix + dedup 문서 0.15μm + web DEDUP_TOLERANCE + 회귀 2 |
| `ab508e9` | carve SSOT + cleave doc |
| `823dbbb` | boundary SSOT |
| `07bd466` | flush-collapse 엔진 (회귀 3: restores_clean_face/noop/preserves_slice) |
| `13d871f` | flush-collapse 배선 (scene+wasm+ts+MoveTool) |

**검증됨**: axia-geo 2155 + axia-core 433 PASS, tsc 0, WASM 바인딩 생성 확인.
**미완**: 브라우저 런타임 시연 (§5) — preview 인프라가 별개 worktree 서빙한 환경
이슈로 이 세션에서 못 함.

---

## 5. 🔴 다음 세션 시연 & 검증 체크리스트

**전제**: Claude Code 를 **`E:\AXiA3D`(공백 없음)** 에서 직접 열 것. (옛 `E:\AXiA 3D`
worktree 에서 열면 preview 가 옛 빌드 서빙 — 본 세션의 혼선 원인.)

### 5.1 빌드 & 서버
```
cd E:/AXiA3D/web
npm run build:wasm          # web/src/wasm 재빌드 (collapseFlushExtrusion 포함 확인)
grep -c collapseFlushExtrusion src/wasm/axia_wasm.js   # → 2 여야 함 (0 이면 빌드 실패)
npm run dev                 # :3000, Ctrl+Shift+R 하드리로드
```

### 5.2 배선 plumbing (preview_eval, headless)
```js
const b = window.__axia.get('bridge');
typeof b.collapseFlushExtrusion === 'function'   // → true
typeof b.engine.collapseFlushExtrusion           // → 'function' (없으면 옛 빌드 서빙 중)
b.collapseFlushExtrusion(0)                       // 빈 씬 → {ok:true, collapsed:0}
```

### 5.3 사용자 시연 (핵심 게이트)
1. 박스 생성 → 윗면에 사각형 그리기 → PushPull 로 **위로 보스** 돌출.
2. 보스 윗면 선택 → **Move 로 z=0(원래 높이) 로 되돌리기**.
3. 기대: 자동 Toast "납작해진 면을 정리했습니다 (벽 N개 제거)" + 보스 흔적 사라지고
   **깨끗한 평평한 윗면** 복원.
4. Undo 1회로 되돌아가는지 확인.
5. **시연 게이트 (테스트가 못 잡는 것)**: 밑면/측면 시각 깨짐, self-intersection,
   XIA 소유권 stale (Inspector 면 개수) 확인.

### 5.4 후속 배선 (남은 것)
- **PushPull 경로** — 원래 제스처("PushPull 로 보스 되돌리기")는 아직 미배선.
  PushPullTool 의 MoveOnly 커밋 후 `collapseFlushIfNeeded()` 추가 (MoveTool 패턴 답습).
  커밋점이 10곳이라 주입 주의 — 또는 엔진 move exec 내 atomic 통합 검토.
- **엔진 atomic 대안** — scene 의 translate/pushpull exec 내에서 collapse 를 같은
  transaction 으로 호출하면 단(單) Undo + 모든 도구 자동 커버 (단, 전 move 에 영향 —
  사용자 결재 필요).

---

## 6. 알려진 한계 / 미결

- 브라우저 런타임 시연 0건 (§5, 환경 이슈). 엔진은 Rust 회귀로 증명됐으나 scene
  ownership reconcile + tool 자동호출은 실런타임 미검증.
- PushPull 경로 미배선 (MoveTool 만).
- `area_tol` 기본값 1e-3 mm² — 모델 스케일별 튜닝 여지 (near-flush 감지 임계).
- CLAUDE.md 교차참조 "1.5μm" 3곳 (메타 #10, 역사 기록으로 보류).

# Follow-up Audit — LOD refresh `HeId not found` panic (primary site + fix)

**Date**: 2026-07-10
**Author**: WYKO + Claude
**Trigger**: ADR-286 curved-boss 시연 중 dev-preview panic 재발견
**Predecessor**: `docs/audits/2026-05-24-demo-2-refcell-aliasing-audit.md`
  (Demo #2 "recursive use" 200 errors — **primary panic site 미식별**,
  §L-Audit2-4 로 deferred "Path C / ADR-148")

## 1. 2026-05-24 audit 가 남긴 gap

2026-05-24 audit 은 "recursive use of an object" 200 errors 를
**secondary cascade** 로 규명 (primary WASM panic → wasm-bindgen RefCell
영구 잠김 → 이후 매 frame `setRenderChordTol` "recursive use"). 하지만
**primary panic 의 정체/site 는 미식별** (자동 reproduce 실패).

## 2. 본 follow-up 가 채운 missing piece

ADR-286 곡면 boss/pocket 시연 중 dev-preview(port 3000) 에서 재현.
`preview_eval` 로 격리 측정:

- **Primary panic = `Entity HeId(NN) not found in storage`**
  (`storage.rs:139` — generic `Index` panic). 2026-05-24 가 못 본
  primary error 메시지 확보.
- **발생 위치 = WASM export pull** (getMeshBuffers/getDeltaBuffers/
  getEdgeLines 계열). `bridge.getMeshBuffers()` 단독 호출은 **panic
  안 함**; `syncMesh` 단독(카메라 정지)도 **panic 안 함**. 오직
  **camera 이동으로 onFrame LOD churn 이 발생할 때** 재현.
- **Deferred BVH/smoothNormals 는 무관** — Viewport.ts:1490/1525 의
  deferred task 는 Three.js `geometry` 객체만 다루는 **순수 JS**
  (WASM engine 미호출) → RefCell/engine panic source 아님. (2026-05-24
  가설 D 배제.)
- **Control: shipped `carveCurvedPocket`(ADR-271)도 동일 panic** →
  ADR-286 boss 무관, curved-carve + LOD 상호작용의 선재 race.

### Root cause (확정 추정)

`onFrame` LOD(main.ts, ADR-135)가 continuous zoom 중 **매 5% step 마다
`setRenderChordTol` 즉시 호출** → render cache 를 dirty 로 invalidate
(LOCKED #62 L-135-5). 실제 re-tessellation(syncMesh)은 160ms **debounce**.
그 사이 frame 경계에서 WASM export pull 이 **invalidate 됐지만 아직
rebuild 안 된 cache** 위에서 실행 → stale HE → `HeId not found` panic →
RefCell 영구 잠김 → "recursive use" cascade.

## 3. Fix (본 follow-up, main.ts)

**`setRenderChordTol` 를 debounced refresh 콜백 안으로 이동** — cache
invalidation + re-tessellation 을 **원자적**(한 task, back-to-back)으로
수행. 부수 효과로 continuous zoom 중 `setRenderChordTol` 이 **settle 후
1회만** 호출됨(step 마다 아님) → invalidation window 대폭 감소.

- **Zero visible behavior change**: 보이는 geometry 는 이미 (debounced)
  syncMesh 시점의 tol 만 반영 → setRenderChordTol 을 같은 task 로 접는
  것은 시각 동작 불변, stale-cache window 만 제거.
- Path A frame-loop try-catch(main.ts, 2026-05-24)는 보존 (secondary
  방어선).

## 4. 검증

- tsc 0, vitest **2520 / 1 skip**(무회귀), 곡면 E2E **13/13**
  (202/257/263/285/286), production build ✓.
- **Browser 스트레스**(dev-preview): 곡면 boss 후 camera far↔near
  **10회 swing**(max LOD churn) → panic 0, mesh valid(0 viol), **engine
  responsive/not-poisoned**(getStats OK, lastError empty). 이전 panic
  run 은 engine poisoned(screenshot hang + console recursive-use 다수)
  였던 것과 대조.

## 5. 한계 / 남은 것 (정직 기록)

- Primary panic 은 **tight timing race** — 본 세션에서도(2026-05-24 처럼)
  **결정적 reproduce 불가**. 따라서 fix 는 *correct-by-construction +
  stress-test 통과* 근거이지, before/after 결정적 repro 대조는 아님.
- **Engine-level 잔여**: export pull 이 transiently-inconsistent cache
  에서 hard-panic(`storage.rs:139` Index) 하는 대신 graceful skip/empty
  반환하는 defense-in-depth(메타-원칙 #6)는 별도 트랙(ADR-148 가칭 Path
  C). `storage.rs:139` 은 generic hot getter 라 **전역 완화 금지** —
  export call site 한정 `.get()` fallback 이 옳은 접근(별도 ADR).

## 6. Cross-link

- `docs/audits/2026-05-24-demo-2-refcell-aliasing-audit.md` (predecessor —
  §L-Audit2-4 primary site 미식별을 본 audit 이 채움)
- ADR-135 / LOCKED #62 (LOD chord_tol frame loop)
- ADR-111 / ADR-112 (deferred render — JS-only, 무관 확인)
- ADR-271 (curved pocket — control) / ADR-286 (curved boss — trigger)
- 메타-원칙 #6 (Preventive) / #11 (Latency Budget)

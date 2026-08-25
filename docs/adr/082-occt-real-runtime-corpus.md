# ADR-082 — OCCT.js 실설치 + NIST 1 Corpus 실검증

**Status**: **Accepted** (C-α spec only — code 변경은 후속 C-β ~ C-ζ
별도 atomic commits)
**Date**: 2026-05-07
**Author**: AXiA team (사용자 결정 + Claude spec)
**Anchor**: 사용자 가치 평가 결정 (2026-05-07):
> "ADR-081 53 mock 회귀의 실파일 round-trip 검증 0건 — demo 시 risk.
> OCCT.js 실설치 + NIST 1 corpus 실검증이 가장 큰 demo unlock 이자
> mock-only confidence 의 첫 truth 검증. ADR-081 알려진 한계 #3 의
> 자연 closure."
**Parent**: ADR-081 (STEP/IGES NURBS-class Import Activation),
ADR-035 (STEP/IGES Hybrid Strategy)
**Cross-cut**: ADR-036 P21 (Curve & Surface Promotion mapping),
ADR-075 (NURBS Boolean Browser E2E — Playwright 인프라 답습),
ADR-046 P31 (P1 + P3 두 페르소나 가치 anchor)

---

## 0. Summary (6 lines)

> ADR-081 트랙 (W-α ~ W-η, 53 vitest +tests) 은 mock fixture 만으로
> closure. 실 OCCT.js 런타임 정합성 0건 검증 — demo 시 risk +
> wrapper drift 가능성. 본 ADR 은 `opencascade.js` 실설치 + NIST 공개
> corpus 1 파일 (`test_part_1.step`) 를 Playwright real Chromium 환경
> 에서 round-trip 검증하여 ADR-081 mock confidence 의 첫 truth 검증
> 완성. Initial bundle 0MB 증가 강제 유지 (P20.C #2 strict).
> 4 sub-atomic 분해 (C-α/β/γ/δ) — minimal scope.

---

## 1. Context

### 1.1 ADR-081 closure 가 unblock 한 것

ADR-081 W-α ~ W-η (commits `c297093` ~ `144835f`, 2026-05-06):
- 11 curve mapping (occtCurvePromote) + 12 surface mapping (occtSurfacePromote)
- BRep traversal + face/edge stable index promotion
- Trim loop handling (PCurve)
- 5 corpus fixture round-trip ≤ 1e-3 mm
- UI integration (Toast + traversal passthrough)

**누적 vitest +53** (1512 → 1569). 모두 mock-based.

### 1.2 Mock-only validation 의 한계

ADR-081 §알려진 한계 #3 명시:
> "실제 vendor STEP/IGES 파일 코퍼스 (NIST/SolidWorks/Fusion/CATIA
> actual files): OCCT.js 설치 + Playwright E2E (ADR-075 인프라 활용)
> 필요. 본 commit 의 unit test 는 mock fixture 만 — *demo 시 실파일
> risk*"

**구체적 risk** (사용자 가치 평가, 2026-05-07):
1. **Wrapper drift**: 우리 `_2 ?? _1 ?? bare` chain 이 실 occt.js v2 API
   와 1:1 정합 안 될 가능성 — 53 mock 회귀가 false positive 일 가능성
2. **DownCast/get() chain**: occt.js Handle 래핑 함정 (ADR-035 P20.7
   주의사항) 이 실 런타임에서 다르게 동작할 수 있음
3. **NCollection_Array2 footgun** (LOCKED #14): 우리 우회 패턴
   (`Pole(i, j)` / `Weight(i, j)` 직접 accessor) 이 실 API 와 정합 검증
   안 됨
4. **BRepMesh tessellation 부재**: import 후 viewport 빈 group → 사용자
   "import 됐는데 안 보임" → demo 효과 반감

### 1.3 사용자 가치 anchor (ADR-046 P31)

- **P1 (건축/디자인)**: 실 SolidWorks/Fusion/CATIA STEP 열림 데모 →
  workflow 통합 가시화
- **P3 (AI 협업자)**: AI agent 가 실 STEP 입력 → axia-engine op 적용
  → MCP 시나리오 first end-to-end 검증

### 1.4 ADR-035 / ADR-075 답습

- ADR-035 P20.C #2 (initial bundle 0MB 증가 강제) — 본 ADR 도 strict
  유지. opencascade.js 는 dynamic import 만 (lazy chunk).
- ADR-075 (NURBS Boolean Browser E2E) 의 Playwright + Vite preview
  인프라를 그대로 활용. `playwright.config.ts` + `web/e2e/` 디렉토리
  + ci.yml `web-e2e` job.

---

## 2. Decision

### 2.1 Lock-ins L1 ~ L7

- **L1 — Dependency 등급**: `opencascade.js` 는 `optionalDependency` 유지
  (ADR-035 P20.7 답습). 추가로 `devDependency` 로 등록 (test 환경에서
  실 OCCT.js 사용). production 사용자는 미설치 graceful fallback 유지.
- **L2 — Corpus 소스**: NIST 공개 corpus 1 파일 (`test_part_1.step`).
  **공개 도메인** + **government-issued** + **라이선스 risk 0**. 저장
  위치: `web/e2e/fixtures/corpus/test_part_1.step` (git-tracked).
- **L3 — Bundle 영향 0MB strict** (ADR-035 P20.C #2 답습): initial bundle
  724.76 kB 절대 증가 안 함. 검증 명령: `vite build` 후 `index-*.js`
  size 비교 + opencascade.js 가 별도 lazy chunk 인지 확인.
- **L4 — Wrapper drift 발견 → typed warnings**: 실 OCCT API 와 우리 wrapper
  chain 이 다르면 `ImportResult.warnings` 에 누적 (ADR-036 P21.7 답습).
  Fatal 아닌 graceful fallback. 발견된 drift 는 별도 commit 으로 본
  ADR 트랙 내에서 1차 수정.
- **L5 — Playwright E2E truth 검증** (ADR-075 인프라 답습): real Chromium
  + Vite preview + opencascade.js 실 로드 → `traverseBrep(occt, shape)
  .faces.length >= 1` 명시 검증. mock-only confidence 의 첫 ground
  truth.
- **L6 — BRepMesh tessellation 별도 트랙 deferred**: `_convertToThreeGroup`
  은 W-3 deferred 영역 (ADR-081 알려진 한계 #2). 본 ADR 은 *import
  correctness* (shape 추출 + traversal) 까지만. *viewport 표시* 는
  별도 ADR.
- **L7 — Mock fixture 보존**: ADR-081 53 mock 회귀는 *design contract
  test* 로 유지. 본 ADR 은 *integration test* 추가 — 대체 아님.
  Mock 회귀가 깨지면 design 변화, real 회귀가 깨지면 OCCT runtime
  drift 라는 명확한 분리.

### 2.2 Out of scope (별도 ADR)

- **NIST 외 vendor 파일** (SolidWorks / Fusion / CATIA) — 본 ADR 은 NIST 1
  파일 minimum scope. vendor 파일 추가는 ADR-082 확장 또는 별도 ADR.
- **Export to STEP/IGES** — ADR-035 P20.B Non-goals 답습.
- **BRepMesh tessellation** — `_convertToThreeGroup` 본체 별도 ADR.
- **WasmBridge owner-ID 매핑** (`bridge.setFaceSurface*` 호출 + axia
  FaceId attach) — 별도 ADR. 본 ADR 은 traversal 결과 추출까지만.
- **PMI / GD&T / Assembly / Drawing views** — ADR-035 P20.B Non-goals
  답습.

### 2.3 Decision matrix (사전 검토 §82-A ~ §82-E)

본 ADR 진입 직전 사용자 가치 평가 (2026-05-07) 의 권장값 모두 채택:
- **§82-A**: NIST 공개 corpus 우선 (vendor 파일 다음 단계)
- **§82-B**: minimal scope 4 sub-atomic (C-α/β/γ/δ) — 일주일 내 closure
  목표
- **§82-C**: bundle 영향 0MB strict (P20.C #2 답습)
- **§82-D**: Playwright real Chromium E2E truth 검증
- **§82-E**: BRepMesh tessellation 별도 트랙

---

## 3. Implementation Plan (post-acceptance)

### 3.1 C-α — ADR-082 spec only — ✅ 본 commit

본 commit 이 C-α. spec docs 작성만, 코드 변경 0.

### 3.2 C-β — opencascade.js devDep + NIST corpus fixture

- `web/package.json` 의 `optionalDependencies.opencascade.js` 유지 +
  `devDependencies.opencascade.js` 추가 (동일 버전)
- NIST corpus 1 파일 (`test_part_1.step`) 다운로드
- 저장: `web/e2e/fixtures/corpus/test_part_1.step` (git-tracked,
  공개 도메인 license 명시)
- Bundle 영향 측정: `vite build` 전후 `index-*.js` size diff 0
  검증. opencascade.js 가 별도 lazy chunk (`opencascade-deps`) 인지
  확인 (vite.config.ts manualChunks 답습).

회귀: vitest +1 (corpus fixture 존재 + size 검증) + bundle size 명시
스크립트.

### 3.3 C-γ — Real OCCT runtime `_readShape` 정합 검증

- `StepIgesImporter._readShape` 가 실 OCCT.js v2 API 와 1:1 정합
  하는지 검증
- 발견된 wrapper drift 1차 fix (e.g., `BRep_Tool.Surface_2` ↔
  `BRep_Tool.Surface` 정합, `STEPControl_Reader_1` ctor signature)
- `occtCurvePromote` / `occtSurfacePromote` / `occtBrepTraversal` 의
  Handle DownCast chain 검증
- Test: vitest 환경에서 opencascade.js 직접 로드 (Node + WASM) →
  실 corpus 파일 byte → `_readShape` → traverseBrep → faces.length

회귀: vitest +3~5 (real runtime smoke + face count + edge count + 1
드 wrapper drift fix 검증).

### 3.4 C-δ — Playwright E2E (real Chromium round-trip)

- ADR-075 Playwright 인프라 답습 (`web/e2e/` + `playwright.config.ts`)
- real Chromium + Vite preview + opencascade.js 실 로드
- `web/e2e/occt-corpus.spec.ts`:
  ```ts
  test('NIST test_part_1.step → traversal.faces ≥ 1', async ({ page }) => {
    // ... bridge call sequence + corpus fetch + import + assert
  })
  ```
- ADR-075 fixture helper 패턴 답습 (`web/e2e/helpers/` 신규 또는 기존
  활용)

회귀: Playwright +1 (1 corpus 1 round-trip).

### 3.5 C-ε — Drift #3 Architectural Fix (amendment 2026-05-08)

**Trigger**: C-δ 발견 — drift #3 (architectural bundler-runtime 한계):
> `StepIgesImporter._loadOcct` 의 `/* @vite-ignore */` 패턴 (ADR-035 P20.7
> graceful build 보호용) 이 Vite 의 import 분석을 차단 → opencascade.js
> 가 production build 에 bundle 안 됨 → browser dynamic import 가
> bare specifier 'opencascade.js' resolve 못함. **현재 production build
> 에서 OCCT 실 사용 불가능**.

**사용자 결재 (2026-05-08)**: 권장안 A 채택 — ADR-082 amendment + C-ε
진입.

**Resolution path**:
1. **`StepIgesImporter._loadOcct` 수정**:
   - `/* @vite-ignore */` 주석 제거
   - `'opencascade' + '.js'` 동적 string indirection → literal `'opencascade.js'`
   - Vite 가 정상 import 분석 → `opencascade-deps` lazy chunk 생성
2. **L1 amendment** (lock-in 정책 수정):
   - **이전**: `optionalDependencies` 유지 + `devDependencies` 추가 (production graceful + test 명시)
   - **변경**: `dependencies` 로 승격 (build 강제 requirement). optionalDep / devDep 양쪽 제거.
   - **근거**: drift #3 발견으로 "graceful build 시 OCCT 실 사용 불가" 가 무의미한 약속 임이 확인됨. 실제 가치를 위해서는 build-time 의존성 명시 필요.
3. **Trade-off accept**:
   - `npm install --no-optional` 시나리오 미지원 (이전 ADR-035 P20.7 의도였으나 실효 없음)
   - `opencascade.js` 가 항상 production build 에 lazy chunk 로 포함
   - **P20.C #2 strict 유지** — initial bundle 0MB 영향 없음, lazy chunk 만 추가
4. **C-δ Playwright 테스트 invert**:
   - `Drift #3 회귀 가드` (negative) → `OCCT browser load 회귀` (positive): `import('opencascade.js')` 가 success → `initOpenCascade` 호출 가능
   - `chunk absence` (negative) → `chunk presence` (positive): `opencascade-deps-{hash}.js` 가 dist 에 존재
5. **Real Chromium round-trip** (원래 C-δ 의도, 본 amendment 로 활성):
   - `traversal.faces.length >= 1` ground truth 검증 (corpus 생성 또는 fetch 후속)
6. **L3 (initial bundle 0MB) strict 검증**: amendment 적용 전후 initial
   bundle hash 동일성 + size 동일성 명시 회귀

**ADR-082 수정 후 lock-in 표** (참고):
- L1 amendment ↑
- L2 ~ L7: 변경 없음
- 신규 amendment 회귀 budget: vitest +0~1, Playwright +0~2 (기존 negative 2개 invert + 추가 round-trip 1개)

**원래 §3.5 (optional C-ε additional corpus)** → §3.5.1 로 강등:

### 3.5.1 (optional) Additional corpus + W-η UI integration 실파일 검증

C-ε closure 후 사용자 결재로 진입 가능. NIST 2번째 파일 또는
SolidWorks 1 파일 추가. 본 ADR 의 minimum scope 외.

### 3.6 (optional) C-ζ — LOCKED #29 갱신 + 회고

C-α ~ C-δ closure 후 거버넌스 정합 (LOCKED #28 ADR-081 패턴 답습).

### 3.7 누적 회귀 예상

- vitest **+4~6** (C-β 1 + C-γ 3~5)
- Playwright **+1** (C-δ)
- Rust 0 (TS-only 변경)
- vite build: bundle size monitoring (P20.C #2 0MB strict)

**예상 cumulative**:
- Best case (drift 없음): vitest +4, Playwright +1
- Realistic (drift 1~2 발견): vitest +6, Playwright +1

---

## 4. Acceptance Criteria

C-α 본 commit 으로 만족:

- [x] ADR-082 spec 작성 (§0 ~ §6)
- [x] ADR-035 / ADR-036 / ADR-075 / ADR-081 cross-link 명시
- [x] L1 ~ L7 lock-ins 명시
- [x] §82-A ~ §82-E 사전 검토 결과 정합
- [x] C-α ~ C-ζ 4+2 sub-atomic 로드맵 명시
- [x] Out of scope (vendor 파일 / Export / BRepMesh / WasmBridge 매핑 /
  PMI) 명시
- [x] 사용자 가치 anchor (P1 / P3 페르소나) 명시

본 ADR 의 commit 만으로 C-α 완료. 후속 C-β ~ C-δ 별도 atomic +
별도 결재.

---

## 5. Cross-references

- **ADR-081** (STEP/IGES NURBS-class Import) — 본 ADR 의 직접 trigger.
  알려진 한계 #3 (실파일 코퍼스 검증 부재) 의 자연 closure.
- **ADR-035** (STEP/IGES Hybrid Strategy) — Stage 4-A activation 답습.
  P20.A format priority + P20.C #2 bundle 0MB + P20.7 wrapper version-
  tolerant 전부 보존.
- **ADR-036** (Curve & Surface Promotion P21) — 11+12 mapping table 의
  *truth 검증*. 매핑 표 자체는 변경 없음, 실 OCCT API 정합만 검증.
- **ADR-075** (NURBS Boolean Browser E2E) — Playwright real Chromium
  인프라 답습. `playwright.config.ts` + `web/e2e/` + ci.yml `web-e2e`
  job 그대로 활용.
- **ADR-046** (P31 UI/UX Strategy) — P1 (건축/디자인) + P3 (AI 협업자)
  두 페르소나 가치 anchor.

---

## 6. Lessons (작성 시점)

- **Mock confidence 의 ceiling**: ADR-081 53 mock 회귀가 closure 했음에도
  실파일 round-trip 0건 → "이론적 완성도" 와 "실 데모 readiness" 의 gap
  명시. *모든 mock-based 트랙은 그에 대응하는 real-runtime 트랙이
  필요* — 본 ADR 이 이 패턴의 첫 사례.
- **Path Z atomic + minimum scope**: 4 sub-atomic (C-α/β/γ/δ) 만으로
  truth 검증 first milestone 도달. ADR-081 의 7 sub-atomic 보다 짧음 —
  *minimum scope 가 가장 큰 가치 unlock 가능* 의 사례.
- **License-aware corpus selection**: NIST 공개 corpus 우선 — government-
  issued + 공개 도메인 + 라이선스 risk 0. vendor 파일은 별도 license
  검토 필요. *교환 가능 fixture* 는 *교환 불가 fixture* 보다 가치 큼.
- **Mock + real test 분리 정책**: design contract test (mock) + integration
  test (real) 의 명확한 분리. 두 회귀의 의미 다름 — mock 깨짐 = design
  변화, real 깨짐 = OCCT runtime drift. 동일 코드 경로의 두 검증 layer.

---

*Author*: AXiA team (사용자 결정 + Claude spec) | *Status*: **Accepted**
(C-α spec only commit 2026-05-07). C-β ~ C-δ 별도 commit 으로 구현.

---

## Amendment 2 — vitest 는 왜 이 패키지를 열려다 마는가 (2026-08-25)

### A2.1 계기

vitest 실행 시간을 재던 세션이 `opencascade.js` 해석을 비용 후보로 지목했다.
제안은 vitest 전용 stub 으로 갈아끼우는 것이었다. **측정 결과 전체 스위트의
실시간은 움직이지 않았고, 변경은 되돌렸다.** 대신 그 과정에서 드러난 사실들이
남았다 — 그게 이 amendment 다.

### A2.2 두 config 는 별개다 (측정된 사실)

프로덕션 `vite.config.ts` 는 이 패키지 하나를 위해 `opencascadeWasmAsUrl()`
플러그인 + `vite-plugin-wasm` + `optimizeDeps.exclude` + manualChunks 규칙을
갖고 있다. `vitest.config.ts` 는 **플러그인이 하나도 없다**. 그래서 vitest
에서는 `.wasm` re-export 를 처리할 것이 없고 import 가 실패한다.

패키지 자체는 234 MB 이고 entry `dist/index.js` 는 8 MB 짜리
`dist/opencascade.js` 를 첫 줄에서 끌어온다. **그러나 그 8 MB 는 실제로
transform 되지 않는다** — `.wasm` 해석이 먼저 실패한다. 처음엔 소스 순서를
근거로 반대를 추론했고, 측정이 그 추론을 뒤집었다.

### A2.3 측정 (콜드 캐시, 3쌍 반복)

`_loadOcct` 를 실제로 타는 두 파일만:

| | stub | 실제 패키지 |
|---|---|---|
| Duration | 718 / 753 / 741 ms | 1.99 / 1.95 / 2.01 s |
| tests | 26 / 27 / 26 ms | 2.49 / 2.44 / 2.51 s |

stub 은 그 두 파일에서 ~1.25s 를 아끼고, 비용은 거의 전부 **테스트 실행
시점** 에 있다 (`_loadOcct` 의 dynamic import 가 테스트 본문에서 돈다).

전체 스위트 (190 파일, ~20 워커), 콜드:

| | stub | 실제 패키지 |
|---|---|---|
| Duration | 14.90s | 13.50s |

**실시간이 움직이지 않는다.** 두 파일의 1.25s 는 임계 경로가 아니고, 실행 간
편차(콜드 13.5–14.9s, 최초 실행 29.3s)가 효과보다 크다.

⚠ 처음에 29.33s → 25.12s (−14.4%) 라는 수치를 얻었는데, **최초 실행 대
데워진 실행** 을 비교한 것이었다. 양쪽 다 콜드로 통제하니 사라졌다.

### A2.4 stub 을 넣었다면 봉인 하나가 무력화됐을 것 (남길 이유)

`occtRuntime.test.ts` 의 *Drift #2 회귀 가드* 는 목적을 스스로 적어 두고
있다 — *"향후 누군가 Node 측 OCCT 통합을 시도할 때 즉시 발견 가능."*
stub 은 **무조건** throw 하므로, 그 봉인이 지켜보려던 사건(Node 가 wasm-ESM
링크를 얻는 날)이 와도 영원히 초록이다. 봉인이 상수 관측으로 바뀐다.

적대적 검토가 이걸 잡았다. 향후 다시 stub 을 제안한다면 **이 봉인이
alias 를 우회해 실제 entry 를 열어야** 한다 (절대 `file://` URL 은
exact-match alias 에 걸리지 않는다). 그 비용은 그 파일 하나가 run 당 실제
entry 를 한 번 여는 것이다.

### A2.5 세 겹으로 낡아 있던 주석 (정정함)

`StepIgesImporter.test.ts` 의 테스트 **이름** 이 `'graceful fallback when
opencascade.js is not installed'` 였고 주석 넷이 *"not in test deps"* /
*"not installed in test env"* 였다. `FileImporter.test.ts` 도 같았다.

패키지는 ADR-082 L1 amendment 로 `dependencies` 이고 디스크에 있다 — npm
workspace 가 저장소 루트로 hoist 하므로 `web/node_modules` 를 보면 비어
있다. 통과 이유는 두 번 바뀌었는데(미설치 → Node 가 wasm 을 링크 못 함)
아무 것도 옆에 적히지 않았다. 전부 정정.

덤으로, `FileImporter.test.ts` 의 *'error includes alternatives'* 는 두
`expect` 가 맨 `catch` 안에 있어 **공허** 했다. `tryImport` 가 거부를 멈추는
날 아무 것도 단언하지 않은 채 초록이 된다. `expect.assertions(2)` 로 닫았다.
변이로 증명: 그 줄이 있으면 변이가 실패하고, 빼면 **같은 변이가 20/20
통과** 한다. 저장소 전체 `expect.assertions` 는 이전까지 **0** 이었다.

### A2.6 Lock-ins

- **L-A2-1** `web/node_modules` 가 비어 있는 것은 미설치가 아니다. npm
  workspace 가 저장소 루트로 hoist 한다. 위치를 잘못 보면 "moot" 라는
  거짓 결론이 난다.
- **L-A2-2** vitest 와 vite 는 **별개 config** 다. vitest 쪽 alias 는
  프로덕션 번들에 닿지 못한다 (ADR-035 P20.C #2 는 무관).
- **L-A2-3** stub 을 다시 제안한다면 Drift #2 봉인을 함께 수리해야 한다
  (A2.4). 수리 없이는 LOCKED #29 가 기록한 것을 더 이상 지켜보지 않는다.
- **L-A2-4** 캐시 상태를 통제하지 않은 전후 비교는 수치가 아니다 (A2.3).
- **L-A2-5** 절대 #[ignore] 금지.

### A2.7 Cross-link

LOCKED #29 (Drift #1~#5) / ADR-035 P20.C #2 / LOCKED #106 (번들 예산 측정) /
ADR-125 · ADR-127 (측정이 뒷받침하지 않은 변경을 접은 선례) / 메타-원칙
#4 (SSOT) · #6 (측정 우선).

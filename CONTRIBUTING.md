# CONTRIBUTING — AXiA 3D

이 문서는 AXiA 3D 개발의 **작업 규율(discipline)**을 정한다.
2026-07-01 clean baseline(`E:\AXiA3D`) 재정비의 산물이며, 구 저장소가
**486개 브랜치 / main 490커밋 stale**로 파편화됐던 실패를 반복하지 않기 위한
규약이다. (배경: `docs/audits/engine-systemic-review-2026-07-01.html`)

---

## 0. 황금 규칙 — 단일 진실 상태 (Single Baseline)

> **`main`이 유일한 진실이다. 모든 작업은 `main`에서 분기해 `main`으로 돌아온다.**

- 장수(long-lived) 로컬 브랜치를 만들지 않는다. 작업 → 완료 → 즉시 `main` 병합/정리.
- "실제 작동하는 코드가 main에 없는" 상태를 절대 만들지 않는다.
- 실험/파생 브랜치가 필요하면 **완료 후 반드시 병합하거나 삭제**한다. 방치 금지.

---

## 1. 브랜치 규약 (486-브랜치 재발 방지)

- **서브브랜치 금지.** ADR을 α/β/γ 단계로 나눠도 그것들을 *브랜치로 만들지 않는다*.
  각 단계는 **로컬 커밋**으로만 쌓고, 트랙 종료(closure) 시 `main`에 ff/squash 한다.
  - ❌ `adr-268/alpha`, `adr-268/beta-1`, `adr-268/gamma-closure` (구 repo의 실패 패턴)
  - ✅ `main` 위 순차 커밋 `feat(ADR-268): α ...`, `feat(ADR-268): β-1 ...`
- **세션/워크트리 위생.** 임시 워크트리는 작업 종료 시 `git worktree remove`.
  활성 워크트리는 "작업 1 + main 1"만 유지.
- **브랜치 총량 목표: 상시 30개 이하.** 넘으면 병합·정리 신호.

---

## 2. 빌드 & 실행 (npm workspace, ADR-265)

이 저장소는 **npm workspace 루트**다. 모든 install/lock은 **루트에서** 이뤄진다.

```bash
# 최초 1회 (또는 의존성 변경 후)
cd <저장소 루트>            # 클론한 위치 (npm workspace 루트)
npm install                # 루트 workspace 설치 (web + packages/axia-action-catalog)

# WASM 엔진 빌드 (Rust → wasm, SIMD)
npm run build:wasm

# 개발 서버
npm run dev                # http://localhost:3000/  (점유 시 --port <N> --strictPort)

# 전체 빌드 (wasm + tsc + vite)
npm run build:all
```

- ⚠️ `cd web && npm install` 하지 말 것. lock은 루트 단일 `package-lock.json`이다.
- `node_modules`는 루트로 hoist된다. 경로를 하드코딩하는 코드/테스트 금지
  (hoist 무관하게 `require.resolve` 또는 후보 경로 탐색 사용 — ADR-266 occtRuntime 선례).
- MCP 패키지(`packages/axia-mcp-server`, `create-axia-mcp`)는 **독립 패키지**로,
  workspace에 포함되지 않는다. 각자 디렉터리에서 `npm install`.

---

## 3. 커밋 전 필수 검증 (회귀 없음 — 메타-원칙 #9)

커밋 전 아래를 **로컬에서 통과**시킨다 (CI가 검증하는 것과 동일):

```bash
npm run typecheck                       # tsc --noEmit
npm run test                            # vitest (전량 green)
cargo test --workspace                  # Rust 전체 (crate 별 --lib 금지 — 아래 참조)
node scripts/check-adr-catalog.mjs      # ADR ↔ README 카탈로그 정합
node scripts/ifc-external-validate.mjs  # 외부 IFC 파서(web-ifc) 검증
```

⚠️ **`--lib` 나 crate 별 나열을 쓰지 말 것.** 2026-07-29 감사 실측: CI 와 이 문서가
`cargo test -p <crate> --lib` 를 나열하고 있어서 **107개 테스트가 실행조차 되지
않았다** — 모든 `tests/` 통합 바이너리(axia-core 36 / axia-geo 60 / axia-foreign 6)
와 **axia-transaction 전체(5)**. 그중에는 CLAUDE.md 가 ADR-291 / 302 / 303 의
**lock 이라고 명시한** `phase3_gate_sim` · `fillet_rollback_sim` ·
`mutate_on_failure_sim` 이 들어 있었다. crate 를 나열하면 언젠가 빠뜨린다.

기준선 건강도(2026-07-29): **cargo --workspace 3281 pass / 0 fail / 1 ignored**
(선재 slow-channel), **vitest 2970 / 1 skipped**, tsc 0.
이보다 줄면 회귀다.

- **`#[ignore]` / `.skip` 절대 금지.** 회귀 자산은 항상 실행되어야 한다.
- 신규 기능마다 Rust/vitest/(해당 시)Playwright 회귀를 **추가**한다.

---

## 4. 방법론 — Path Z Atomic

큰 작업은 sub-step으로 분해하고, 각 sub-step은 **독립 커밋 + 회귀 통과 후** 진행:

`α(spec) → β(engine) → γ(bridge/WASM) → δ(UI) → ε(E2E) → ζ(closure)`

- **시연 게이트.** 회귀 통과 ≠ scene 경로 작동. 트랙 종료 시 브라우저 실동작으로
  최종 확인(fresh reload + eval). 회귀만으로 놓치는 버그가 실제로 존재한다.
- **먼저-시뮬(sim-first).** HE wiring 등 위험한 토폴로지 변경은 구현 전에
  시뮬레이션 테스트로 winding/watertight를 확정한다.

---

## 5. ADR 프로세스

- 설계 결정은 `docs/adr/NNN-*.md`로 남긴다. 번호는 **연속(현재 265+)**.
- ADR 추가/변경 시 **`docs/adr/README.md` 카탈로그를 함께 갱신**한다
  (`check-adr-catalog.mjs`가 drift를 CI에서 차단).
- **ADR 불변(메타-원칙 #10).** 기존 ADR 변경 시 새 ADR + 기존은 `Superseded by ADR-XXX`.
- `CLAUDE.md`의 **LOCKED 정책**과 **메타-원칙(#1~#16)**을 준수한다.
  특히 #14(면은 닫힌 경계로부터 유도) / #15(headless ≡ tool path 의미 등가) /
  #4(SSOT) / #6(Preventive over Curative).
- 문서 다이어트: 장문 spec은 개별 ADR에, `CLAUDE.md`는 요약 인덱스 지향.

---

## 6. 원격 + branch protection

**origin = `https://github.com/withoz/axia3d.git`** (이 문서가 "로컬 전용" 이라고
말하던 것은 2026-07-29 감사에서 stale 로 확인 — 원격은 이 baseline 이전부터
있었다). 아래는 486-브랜치 재발 방지의 마지막 방어선이며, **어느 것이 실제로
켜져 있는지는 GitHub 설정에만 있고 이 저장소 어디에도 기록되지 않는다** — 확인은
repo Settings → Branches 에서:

- `main` 직접 push 제한 → PR 필수
- **CI green 필수** (build / test / rust-test / adr-catalog-check 통과해야 병합)
- **Linear history 강제** (merge commit 금지, squash 또는 rebase)
- 병합된 `claude/*` 등 세션 브랜치 자동 삭제(prune)

---

## 7. 참고 문서

- 총체 점검: `docs/audits/engine-systemic-review-2026-07-01.html`
- 구현 로드맵: `docs/plans/IMPLEMENTATION-PLAN-2026-07-01.html`
- 프로젝트 지침 / LOCKED / 메타-원칙: `CLAUDE.md`
- 아키텍처: `ARCHITECTURE.md`

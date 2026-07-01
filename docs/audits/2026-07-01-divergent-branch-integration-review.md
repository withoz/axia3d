# 발산 브랜치 통합 검토 — `claude/intelligent-shamir` → `adr-186`

- **일자**: 2026-07-01
- **검토자**: WYKO + Claude (Opus 4.8)
- **대상**: `claude/intelligent-shamir-fe9c7f` 고유 24커밋을 최신 통합엔진
  `adr-186/boundary-kernel-port` 에 통합할지 여부
- **결론**: **병합/리베이스 부적절. 23커밋 폐기(대체됨), 1개 기능
  (Entity Provenance) 만 신규 ADR 로 선택적 포팅 권장.**
- **제약**: `adr-186` 은 LOCAL·push 금지. 본 검토/후속 작업 모두 로컬 한정.

## 1. 브랜치 토폴로지 (사실)

| ref | tip | main 대비 |
|---|---|---|
| `main` | 9d556a4 | — |
| `adr-186/boundary-kernel-port` (최신 통합엔진) | a893853 | **+488** (main 완전 포함) |
| `claude/intelligent-shamir-fe9c7f` (세션 브랜치) | 6afdbd2 | 발산: main-only 230 / branch-only 24 |

- merge-base = `aacc9d6`
- `intelligent ↔ adr-186` 발산: intelligent-only **24**, adr-186-only **718**
- 즉 `adr-186` 이 canonical 최신이며, `intelligent-shamir` 는 merge-base
  에서 갈라져 나온 **별개 평행 라인** (ADR-101~110 탐색).

## 2. ADR 번호 전면 충돌

두 라인이 **같은 번호를 서로 다른 기능**에 사용 → 병합 시 ADR 이력 붕괴.

| # | `intelligent-shamir` | `adr-186` |
|---|---|---|
| 101 | Headless hole synthesis explicit promote | coplanar-partial-overlap P7 completion |
| 102 | CI Restoration 4-Track Audit | push-pull-detach-on-arrangement |
| 104 | ADR-092 Path-B test assertion drift | path-b-expansion-spec |
| 108 | Face/Edge BVH spatial index | rect-line-layer-h-policy |
| 109 | Fine-grained dirty regions | arc-extrude-cylinder-promotion |
| 110 | Entity provenance / audit trail | boolean-path-b-compat |

`adr-186` 는 ADR 문서가 **264** 까지 존재.

## 3. 24커밋 항목별 판정 (증거 기반)

| intelligent 작업 | adr-186 현황 | 판정 |
|---|---|---|
| **ADR-108** BVH spatial index (spec) | **ADR-111 "BVH Defer to Idle"** 구현 존재 (latency 중심) | ⛔ 대체됨 |
| **ADR-109** Fine-grained dirty regions | 없음 — 단 intelligent 측도 **spec-only, 구현 0** | ⚠️ 미구현 spec |
| **ADR-110** Entity provenance / audit trail | `faceProvenance/edgeProvenance/vertProvenance` **전무** (scene.rs·WasmBridge·XiaInspector 0건) | ✅ **고유·유효** |
| **ADR-101** headless hole / `merge_coplanar_containing` | 엔진 `mergeCoplanarContaining` **이미 존재** (bridge+action-catalog) | 🔶 MCP 래퍼만 신규 |
| **ADR-105/106** closed-curve split dispatch / split-site owner-id | ADR-089(closed edges) + ADR-093(surface owner-id) 로 **훨씬 앞섬** | ⛔ 대체됨 |
| **ADR-102** CI Restoration | intelligent 브랜치 CI 상태 전용 | ⛔ 무관 |
| ADR-108/109/110 **문서 자체** | adr-186 이 동일 번호를 다른 기능으로 점유 | 💥 번호 충돌 |

- 24커밋 net footprint: 56 files, +5384/-186. 공유 파일(`CLAUDE.md`,
  `scene.rs`, `mesh.rs`, `WasmBridge.ts`)은 양측이 718커밋에 걸쳐 다르게
  수정 → cherry-pick/merge 충돌 대량 확정.

## 4. 왜 병합/리베이스가 잘못된 도구인가

1. **ADR 번호 충돌** — 101/102/104/108/109/110 이 두 라인에서 다른 의미.
2. **공유 파일 대폭 발산** — 충돌 해소 비용이 재구현 비용을 초과.
3. **내용의 ~80% 이미 대체/중복** — BVH→ADR-111, merge_coplanar→기존,
   closed-curve→ADR-089+, CI→브랜치 전용.

## 5. 권장

- **23커밋 폐기** (대체됨/무관/미구현 spec).
- **유일하게 유효한 고유 기능 = Entity Provenance / Audit Trail (ADR-110)**:
  "이 면/엣지/정점을 만든 명령(CommandId)" 추적 + XiaInspector 표시.
  adr-186 에 전무.
- 원할 경우 이 기능만 **신규 ADR-265 로 재구현**(포팅). 아이디어와 코드
  모양(`faceProvenance` API + Inspector 표시)은 참고하되 adr-186 의 현재
  `scene.rs`/bridge 에 맞게 신규 작성. 이는 "정리"가 아닌 실질적 기능
  작업(원 ADR-110 은 P-α~P-δ 4단계)이므로 별도 결재로 진행.

## 6. 결정 상태

- **본 커밋 시점**: 검토 결과 기록만. 코드 변경 0. 리베이스/병합 미실행.
- **후속(선택)**: provenance 포팅을 신규 ADR 로 진행할지 사용자 결재 대기.

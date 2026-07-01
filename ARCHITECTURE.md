# AXiA 3D — Architecture Document v1.0

## Vision
블렌더보다 쉽고, 스케치업보다 정확한 3D 모델링 플랫폼.
"생각한 대로 그리면, 정확한 솔리드가 된다."

## Lessons Learned

### AixxiA (JS Prototype) 에서 배운 것
- ✅ 단일 HTML로 빠른 프로토타이핑 가능
- ✅ Three.js 뷰포트는 빠르게 시각적 결과를 얻을 수 있음
- ❌ `_polyPoints {x, z}` 같은 2D 투영은 3D 표면에서 파탄
- ❌ JS만으로는 Half-Edge 토폴로지, Boolean 연산 불가능
- ❌ 상태 관리가 전역 변수 기반 → 복잡도 증가 시 버그 폭발

### KAYAC (Rust + WGPU) 에서 배운 것
- ✅ buildragon: Half-Edge DCEL, Edge Split/Merge, Face Triangulation 구현 완료
- ✅ Transaction Manager: Undo/Redo 원자적 연산
- ✅ WASM/NAPI 듀얼 빌드 전략
- ❌ WGPU 풀스택 렌더러(30K줄)가 UX 반복을 느리게 함
- ❌ Push/Pull 로직에 하드코딩된 값 존재
- ❌ Preview/Commit 분리가 코드에서 명확하지 않음
- ❌ OCCT 연동 미완성

### 블렌더/스케치업의 약점 (우리가 이길 수 있는 부분)
- 블렌더: 진입장벽이 높음, CAD 정밀도 부족, UX가 아티스트 중심
- 스케치업: 곡면 처리 약함, B-Rep 없음, 확장성 한계, 무거워진 UI

## Architecture: 3-Layer Clean Separation

```
┌─────────────────────────────────────────────────┐
│              VIEWPORT (Three.js)                │
│  - 렌더링, 선택, 스냅, Inference 가이드          │
│  - 마우스/키보드 이벤트 → Command 변환           │
│  - Preview 메시 표시 (Ghost)                     │
│  - 결과 메시 수신 → GPU 업로드                   │
└──────────────────┬──────────────────────────────┘
                   │ WASM Bridge (wasm-bindgen)
                   │ Command ↓ / Mesh Data ↑
┌──────────────────┴──────────────────────────────┐
│           CORE ENGINE (Rust → WASM)             │
│                                                  │
│  ┌─────────────┐  ┌──────────────┐              │
│  │ XIA Model   │  │ Command      │              │
│  │ - Lifecycle  │  │ Dispatcher   │              │
│  │ - Rules     │  │ - Preview    │              │
│  │ - Relations │  │ - Commit     │              │
│  └─────┬───────┘  └──────┬───────┘              │
│        │                  │                      │
│  ┌─────┴──────────────────┴─────┐               │
│  │      GEOMETRY KERNEL         │               │
│  │  (buildragon - Half-Edge)    │               │
│  │  - DCEL Topology             │               │
│  │  - Edge Split/Merge          │               │
│  │  - Face Construction         │               │
│  │  - Push/Pull Extrusion       │               │
│  │  - Triangulation             │               │
│  │  - Boolean (future: OCCT)    │               │
│  └──────────────────────────────┘               │
│                                                  │
│  ┌──────────────┐  ┌──────────────┐             │
│  │ Transaction  │  │ Constraint   │             │
│  │ Manager      │  │ Solver       │             │
│  │ - Undo/Redo  │  │ - Snap       │             │
│  │ - Atomic Ops │  │ - Inference  │             │
│  └──────────────┘  └──────────────┘             │
└─────────────────────────────────────────────────┘
```

## Core Principles

### 1. Command Pattern (Preview → Commit)
대부분의 사용자 행동은 `Command` enum (axia-core `commands.rs`) 으로 변환되어
`Scene::execute_command` 에서 처리됨 (Draw / Push-Pull / CreateSolid / Group 등).
- `DrawLineCmd { start, end, surface? }`
- `PushPullCmd { face_id, normal, distance }`

**예외 — Move/Rotate/Scale 은 Command layer 를 거치지 않음** (2026-06-14 엔진-health
audit 로 정정). 이 transform op 들은 WASM `translate_faces`/`rotate_faces`/
`scale_faces` (axia-geo `operations::transform`) 를 **직접** 호출하고, 트랜잭션
(undo) 도 WASM 계층에서 처리됨. (과거 dead `Command::Move` stub 은 제거됨 —
`CommandResult::None` 만 반환했고 어떤 경로도 호출하지 않았음.)

Preview 단계: 가벼운 연산으로 Ghost 표시 (JS 측에서도 가능)
Commit 단계: Rust Core에서 정확한 토폴로지 연산 수행

### 2. XIA Lifecycle (Edge → Face → Solid)
```
Dissolved ←→ Edge ←→ Face ←→ Solid
   (0D)       (1D)    (2D)    (3D)
```
- 모든 차원에서 객체 존재 가능 (0차원 허용)
- Push/Pull: Face → Solid (thickness 추가)
- Boundary 추출: Solid → Face (경계면)
- 상태 전환은 Rust Core에서만 관리

### 3. Entity Ownership (Rust가 진실의 원천)
- JS는 렌더링용 메시 데이터만 보유
- 토폴로지, 관계, 생명주기는 모두 Rust
- JS → Rust: Command 전송
- Rust → JS: Mesh Buffer + Entity State 전송

## Project Structure

```
AXiA-3D/
├── Cargo.toml                 # Rust workspace
├── crates/
│   ├── axia-core/             # XIA 모델, 생명주기, 규칙
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── xia.rs         # XIA 객체 모델
│   │   │   ├── lifecycle.rs   # Edge→Face→Solid 전환
│   │   │   ├── commands.rs    # Command Pattern
│   │   │   └── relations.rs   # 관계 규칙 테이블
│   │   └── Cargo.toml
│   │
│   ├── axia-geo/              # 기하 커널 (buildragon 정리)
│   │   ├── src/
│   │   │   ├── lib.rs
│   │   │   ├── halfedge.rs    # DCEL 토폴로지
│   │   │   ├── mesh.rs        # 메시 데이터 구조
│   │   │   ├── operations/
│   │   │   │   ├── draw.rs    # Line, Rect, Circle
│   │   │   │   ├── push_pull.rs
│   │   │   │   ├── edge_split.rs
│   │   │   │   ├── boolean.rs # (future)
│   │   │   │   └── offset.rs  # (future)
│   │   │   ├── triangulate.rs
│   │   │   ├── snap.rs        # Snap/Inference
│   │   │   └── export.rs      # 메시 버퍼 출력
│   │   └── Cargo.toml
│   │
│   ├── axia-wasm/             # WASM 바인딩 레이어
│   │   ├── src/
│   │   │   └── lib.rs         # wasm-bindgen exports
│   │   └── Cargo.toml
│   │
│   └── axia-transaction/      # 트랜잭션/Undo
│       ├── src/
│       │   ├── lib.rs
│       │   └── history.rs
│       └── Cargo.toml
│
├── web/                       # Frontend
│   ├── index.html
│   ├── package.json
│   ├── vite.config.ts
│   ├── tsconfig.json
│   └── src/
│       ├── main.ts
│       ├── App.tsx
│       ├── viewport/
│       │   ├── Viewport.tsx    # Three.js 캔버스
│       │   ├── Camera.ts       # 카메라 제어
│       │   ├── Selection.ts    # 선택/하이라이트
│       │   └── MeshSync.ts     # Rust↔Three.js 메시 동기화
│       ├── tools/
│       │   ├── ToolManager.ts
│       │   ├── SelectTool.ts
│       │   ├── LineTool.ts
│       │   ├── RectTool.ts
│       │   ├── CircleTool.ts
│       │   └── PushPullTool.ts
│       ├── ui/
│       │   ├── Toolbar.tsx
│       │   ├── Inspector.tsx
│       │   └── StatusBar.tsx
│       └── bridge/
│           ├── WasmBridge.ts   # WASM 초기화/통신
│           └── Commands.ts     # Command 타입 정의
│
└── docs/
    ├── ARCHITECTURE.md         # 이 문서
    └── XIA-SPEC.md             # XIA 스펙 문서
```

## Build Pipeline

```bash
# 1. Rust → WASM 빌드
cd crates/axia-wasm
wasm-pack build --target web --out-dir ../../web/src/wasm

# 2. Frontend 개발 서버
cd web
npm run dev    # Vite HMR + WASM 자동 로드

# 3. Production 빌드
npm run build  # Vite → dist/
```

## Phase Plan

### Phase 1: Foundation (완료)
- [x] 아키텍처 설계
- [x] Rust workspace 셋업
- [x] buildragon에서 핵심 코드 정리하여 axia-geo 구성
- [x] WASM 바인딩 레이어
- [x] Three.js 뷰포트 기본 구성
- [x] 첫 번째 E2E: Line 그리기 → Rust에서 Edge 생성 → Three.js에서 표시

<!-- 정합성 정정 2026-06-14 (엔진-health audit, 실코드 검증). 체크박스는 실제
     구현 상태를 반영하도록 수정 — 과대표기(Command-layer 암시)와 과소표기
     (Boolean/Group 을 미완으로 표기)를 모두 정정. "완료" = 사용자 경로 작동. -->

### Phase 2: Core Modeling (진행 중)
- [x] Draw 도구 (Line, Rect, Circle) → Face 자동 생성
- [x] Push/Pull → Solid 생성 (아래 상세 참조)
- [x] Move/Rotate/Scale — WASM `translate/rotate/scale_faces` 직접 (Command-layer 미경유)
- [~] Copy — 전용 Command/도구 없음 (Group 복제로 대체 가능)
- [x] Undo/Redo — 트랜잭션 snapshot 기반
- [x] Offset — dimension-aware (ADR-080). host surface 별 지원 불균등(미지원은 typed `OffsetEdgeError`)
- [x] Erase

### Phase 3: Precision (진행 중)
- [x] Snap System (vertex, edge, midpoint, center) — robust predicates(ADR-058 Phase M)는 opt-in
- [x] Inference Engine (평행, 수직, 접선) — SnapManager.findSnap (ADR Phase A/B/C)
- [x] Dimension Input (VCB) — DimensionLabel
- [ ] Boundary 추출 — 명시 Boundary tool (ADR-139)

## Push/Pull 구현 상세 (2026-04-09 확정)

### Rust 엔진 (axia-geo/src/operations/push_pull.rs)
AixxiA 원본 로직 그대로 포팅. 두 가지 모드 자동 판별:

- **MoveOnly**: 직육면체 윗면처럼 모든 연결 edge가 노멀과 평행 → 정점만 이동 (면 수 불변)
- **CreateFace**: 평면이거나 연결 edge가 비평행 → 상부면 + 측면벽 생성 + coplanar 병합 + 원본 유지(솔리드)

핵심 함수:
- `push_pull()` — 진입점, MoveOnly/CreateFace 자동 분기
- `is_move_only()` — 연결 edge 방향 검사 (cos(1°) = 0.999848 허용오차)
- `push_pull_move_only()` — 정점 위치 직접 변경
- `push_pull_create_face()` — 새 면 생성 + `merge_faces_by_edge()` 큐 기반 coplanar 병합

### Three.js 고스트 프리뷰 (web/src/tools/ToolManager.ts)
투명 프리뷰 방식 채택 (SketchUp 스타일):

- **동작**: 면 클릭 → 마우스 이동(프리뷰) → 두 번째 클릭(커밋)
- **프리뷰 구성**: 이동된 면 + 측면 벽 + 엣지 라인
- **면/벽**: `MeshBasicMaterial`, 색상 `0x5b9bd5`, 반투명 (면 opacity 0.3, 벽 0.2)
- **엣지**: `LineBasicMaterial`, 색상 `0x2a6cb8`, depthTest:false로 항상 표시
- **Push/Pull 동일 처리**: 방향에 관계없이 같은 렌더링 코드
- **커밋**: `bridge.pushPull(faceId, dist)` → Rust 엔진 실행 → `syncMesh()`

### 메인 메시 렌더링 (web/src/viewport/Viewport.ts)
Two-tone rendering (SketchUp 스타일):
- **전면**: `0xe8e8e8`, MeshStandardMaterial, FrontSide, roughness 0.6, metalness 0.1
- **후면**: `0x9898b4`, MeshBasicMaterial, BackSide
- **엣지**: `0x333366`, LineBasicMaterial
- polygonOffset 적용 (z-fighting 방지)

### Phase 4: Advanced
- [x] Boolean Operations (Union, Subtract, Intersect) — convex polygonal MVP +
      곡면(sphere/cylinder/cone/torus) surface-preserving (ADR-197 β-3, ~9k LOC).
      미지원: multi-loop(hole) 면, 비-Z축 곡면, NURBS surface SSI
- [x] Group/Component — axia-core `group.rs` (중첩/가시성/잠금/컴포넌트)
- [~] Material/Texture — material 시스템 있음; texture 매핑 일부
- [ ] Follow Me (path sweep) — Sweep/Loft 일부 (ADR-079 W-3/W-4)

### Phase 5: Production
- [ ] File I/O (SKP, OBJ, GLTF, STEP)
- [ ] Electron 데스크톱 앱
- [ ] WGPU 렌더러 전환 (선택)
- [ ] AI 통합

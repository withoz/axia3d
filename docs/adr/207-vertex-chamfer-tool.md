# ADR-207 — Vertex 3-way Chamfer Tool (WASM expose + bridge + UI)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: Foundation Tier 1 도구 (ADR-168 audit 정정 → ADR-206 의 B)
- **Depends on**: ADR-024 P10 (`chamfer_vertex_3way`) / ADR-206 (audit + Pattern-12 패턴) /
  ADR-046 P31 #4

## 1. Context

ADR-206 의 Foundation Tier 1 audit (4-agent) 가 확인한 잔여: `chamfer_vertex_3way`
(fillet.rs:359, ADR-024 P10 — valence-3 꼭짓점을 평면 삼각형 corner cut) 의 **engine 은 존재**
(3 tests) 하나 **WASM / bridge / UI 미노출**. (엣지 모따기는 별개로 `chamfer-edge` action =
`filletEdge(edge, dist, 1)` 으로 이미 작동.)

**Pattern-12 (engine-already-robust)** — ADR-206 답습. de-risk 가 chamfer 결과의 **render +
manifold** 까지 확인 → 작업은 WASM expose + bridge + UI 만, **engine 신규 0**.

## 2. Decision

Vertex 3-way chamfer 를 사용자 facing 도구로 노출: `chamferVertex3way` WASM export
(filletEdge 패턴 = direct mesh op + transaction) + bridge wrapper + `ChamferTool`
(click vertex → `findVertexIdAt` → VCB radius). `chamfer_vertex_3way` engine 재사용.

## 3. Lock-ins

- **L-207-1** engine 신규 0 — `chamfer_vertex_3way` 재사용 (Pattern-12).
- **L-207-2** filletEdge 패턴 답습 — direct mesh op (`self.scene.mesh.chamfer_vertex_3way`) +
  transaction begin/commit, NOT Command/scene (Draw 와 다름).
- **L-207-3** edge chamfer (`chamfer-edge` action) 와 별개 — 본 도구는 **vertex** corner cut.
- **L-207-4** ChamferTool = vertex pick (`findVertexIdAt` at snapped point) + VCB radius +
  localStorage `axia:chamfer:vertex-radius` 재사용.
- **L-207-5** valence != 3 / bad radius → engine bail → bridge -1 → Toast (graceful).
- **L-207-6** ADR-046 P31 #4 additive only / 절대 #[ignore] 금지.

## 4. 구현 (단일 atomic `cec2c20`)

- **de-risk**: `adr207_chamfer_vertex_renders` — cube corner chamfer 가 render + manifold +
  cut (trim face renders, 제거된 corner 에 정점 없음).
- **β-1** WASM: `chamferVertex3way(vert_id, radius) -> i32` (modified-face count / -1) +
  export_baseline lock.
- **β-2** bridge: `chamferVertex3way` wrapper + EngineInstance interface + 3 vitest.
- **β-3** UI: `ChamferTool` (click → findVertexIdAt → VCB) + ToolManager `'chamfer'` +
  Modify 메뉴 "꼭짓점 모따기" + `tool-chamfer` command + 10 vitest.

## 5. 회귀 + 검증

- **회귀**: axia-geo +1 (de-risk) / WasmBridge 262→265 (+3) / ChamferTool +10. tsc clean,
  baseline lock, 0 regression, #[ignore] 0.
- **브라우저** (real WASM rebuild, Chromium): `create_box` → `findVertexIdAt` corner (5,5,5) →
  `chamferVertex3way(vid, 2.0)` → 3 modified faces, **6 → 7 faces** (+1 trim triangle),
  non-manifold 0.

## 6. 후속 (별도 ADR — Foundation Tier 1 잔여)

- **ADR-208** Copy/Duplicate 도구 (arrayLinearFaces count=2 재사용 or clone).
- **ADR-209** interactive UX 폴리시 (5 wired 도구 live preview; 메뉴 이미 작동, marginal).
- chamfer segments ≥ 2 (spherical octant tessellation) — ADR-024 P10 의 future expansion.

전체 corrected spec: `reports/ADR_206_FoundationTier1Tools_CorrectedSpec.md`.

# ADR-227 — 24-Tool Roadmap Completion + Box/Torus Toolbar Exposure

- **Status**: Accepted
- **Date**: 2026-06-23
- **Author**: WYKO + Claude
- **Track**: 24-도구 toolbar 마무리 (ADR-186 toolbar initiative closure)
- **Depends on**: ADR-186 (24-tool toolbar) / ADR-220 (Sweep/Loft) / ADR-221 (Hole/Window) /
  ADR-224 (Plane/Wall/NURBS) / ADR-225 (Pie/RotRect/Spline) / ADR-226 (Explode re-point)

## 1. Context — 24-tool 완료 audit

24-도구 toolbar 로드맵(ADR-186, 사용자 결재 2026-06-05)의 마무리. 정식 audit
(toolbar data-tool ↔ ToolManager `tools.set` ↔ menu data-action ↔ CommandCatalog
cross-check) 결과 **신규 10개 도구 전부 완료** 확정:

| 24-tool | 상태 |
|---|---|
| **14 기존** (Select/Line/Rect/Circle/Polygon/Arc/Freehand/PushPull/Bezier + Knife=Slice/Heal/Revolve/BREP∪/BREP∩) | ✅ Phase 1 |
| **10 신규** | ✅ 전부 built + cataloged + wired + 검증 |
| ├ RotRect / Pie / Spline | ADR-225 (catalog) — DrawRotRectTool/DrawPieTool/DrawSplineTool (ADR-186 phases) |
| ├ Sweep / Loft | ADR-220 — DrawSweepTool/DrawLoftTool |
| ├ Hole / Window | ADR-221 — DrawHoleTool/DrawWindowTool (punch_circular/rect_hole, ADR-194) |
| └ NURBS / Wall / 3P-Plane | ADR-224 — DrawNurbsTool/DrawWallTool/DrawPlaneTool |

- **Window-Door = DrawWindowTool** (직사각 개구부, `punch_rect_hole`) — 별도 Door 도구
  없음/불필요(개구부는 window/door 공용).
- **Phantom toolbar 버튼 0** — 32 data-tool 전부 ToolManager 등록 (explode/text3d 는
  data-action menu-action, integrity 가드 처리 — ADR-226).

## 2. 발견된 toolbar 완성도 gap

24-tool 신규는 완료지만, primitive 드롭다운(index.html data-group="primitive-family")에
**box / torus 누락** — sphere/cylinder/cone/nurbs 만 존재. BoxTool/TorusTool 은 ToolManager
등록 + 메뉴(MenuBar tool-box/tool-torus) + CommandCatalog 완비(작동함), **툴바 버튼만 부재**.

## 3. Decision — box/torus 툴바 드롭다운 추가 (Scope A)

primitive 드롭다운에 box/torus 추가 → primitive 그룹 완성 (solids: sphere/cylinder/cone/
**box/torus**, surface: nurbs).

- **index.html**: primitive-family 드롭다운에 `data-tool="box"` + `data-tool="torus"`(key 'D')
  드롭다운 아이템 2개 추가 (cone 다음, nurbs 앞). 핸들러는 **generic data-tool dispatch**
  (등록된 도구 → 버튼만 추가하면 자동 동작) — **TS/엔진/핸들러 변경 0**.
- box no shortcut / torus 'D' (KeyboardShortcuts keyMap 기존, 표시).

## 4. Lock-ins

- **L-227-1** 24-tool 로드맵(14 기존 + 10 신규) **완료** — 신규 10 전부 built+cataloged+wired.
- **L-227-2** Window-Door = DrawWindowTool (별도 Door 도구 없음). primitive(sphere/cylinder/
  cone/box/torus) + NURBS surface 는 24-tool 외 별개이나 toolbar 완성도 차원에서 box/torus 노출.
- **L-227-3** box/torus 툴바 추가 = index.html 드롭다운 2개 아이템만 (generic data-tool 핸들러
  자동 동작). ADR-046 P31 #4 additive — 기존 항목 변경 0, TS/엔진 변경 0.
- **L-227-4** Phantom toolbar 버튼 0 (audit 확증).
- **L-227-5** 절대 #[ignore] 금지.

## 5. 회귀

- index.html 드롭다운 2 아이템 추가 (HTML only, generic 핸들러).
- TS/엔진/WASM/CommandCatalog/ActionCatalog 변경 0 (box/torus 이미 cataloged).
- vitest/cargo 영향 0 (HTML 변경은 테스트 대상 외; CC/AC 불변).
- 브라우저: primitive 드롭다운에 box/torus 표시 + 클릭 시 BoxTool/TorusTool 활성.

## 6. Lessons

- **L1** Toolbar 완성도 audit = data-tool ↔ tools.set ↔ menu ↔ CC 4-way cross-check. ADR-225
  의 tools.set↔tool() diff 의 toolbar(data-tool) 축 확장.
- **L2** 24-tool "완료"의 정직한 정의 — 신규 10 도구 = 완료. primitive(box/torus) 미노출은
  별개 toolbar 완성도 gap (24-tool 자체 아님). 두 축 구분.
- **L3** Generic data-tool 핸들러의 가치 — 등록된 도구는 index.html 버튼/드롭다운 아이템만
  추가하면 자동 동작 (TS 변경 0). MenuBar data-action 은 명시 case 필요(대조).

## 7. 후속 (별도 트랙)

- text3d 3D 텍스트 기능 빌드 (Text3dTool + engine text capability) — feature ADR.
- Phase 0.5 smooth hole render (per-segment Arc, ADR-092) / ADR-197 곡면 Boolean edge case /
  NURBS surface 고급화 (createNurbsSurface 이미 존재).
- Sweep 후속: Profile-face sweep (sweep_surface_1_rail Bishop frame 배선) / Revolve 인터랙티브 /
  Loft N-profile.

## 8. Cross-link

- ADR-186 (24-tool toolbar initiative — 본 ADR 이 closure) / ADR-220/221/224/225/226
  (24-tool 신규 + catalog 정리 시리즈).
- ADR-046 P31 #4 (additive only) / 메타-원칙 #4 (SSOT) / #5 (discoverability) / LOCKED #44
  (Complete Meaning per Merge).

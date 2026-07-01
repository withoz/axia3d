# ADR-210 — Menu Bar Reorganization (CAD-conventional, ADR-046 점진 migration)

- **Status**: Accepted
- **Date**: 2026-06-22
- **Author**: WYKO + Claude
- **Track**: UI/UX (ADR-046 P31 Pillar 1 Discoverability + Q5 메뉴 재구성 A→B 점진)
- **Depends on**: ADR-046 P31 #4 (메뉴 additive only — 본 ADR 이 명시 예외 결재) /
  ADR-045 (ActionCatalog SSOT) / ADR-103 (Z-up)

## 1. Context + 결재

사용자 결재 (2026-06-22): 2D CAD 편집 명령 (Trim/Extend/Fillet/Chamfer/Join, ADR-211+
예정) **구축 전에 메뉴 구조를 CAD 관습으로 정리**. 4-agent 2D-CAD-coverage audit
(`reports/`) 가 메뉴 ↔ 도구 정합 필요를 노출. Edit/Modify 의미는 **AutoCAD 기준 결재**
— Edit = 클립보드(undo/cut/copy/paste/delete), Modify = 기하 편집(move/rotate/trim/…).

**ADR-046 P31 #4 (메뉴 additive only) 의 명시 예외**: 본 재배열은 Dimension 신설(additive)
+ Primitive 통합 + 순서 변경(비-additive)을 포함하므로, P31 #4 가 요구하는 **새 ADR**
경로로 진행. 단축키 / action ID 는 **전부 불변** (라벨·위치만 변경) → muscle memory 의
핵심(키보드)은 보존, 메뉴 탐색만 개선 = ADR-046 Q5 "A→B 점진" 정합.

## 2. Decision — 14 top-level 메뉴 (순서 확정)

```
File · Edit · View · Draw · Modify · Dimension · Modeling · Material · Camera · Tools · Window · Extension · Help · Ai Design
```

현행 대비 변경: ① **Dimension 신설** (Measure + 향후 치수 도구) ② **Primitive 제거 →
Modeling 통합** ③ **순서**: Modify 를 Draw 바로 뒤(5번)로, Edit 는 2번 유지(File·Edit·View
보편 첫 3개 보존).

## 3. 메뉴별 내용 매핑 (canonical)

| 메뉴 | 내용 |
|---|---|
| **File** | New/Open/Save/SaveAs/Import/Export (현행 유지) |
| **Edit** | Undo·Redo·Cut·Copy·Paste·Duplicate·Select All·Deselect·Delete (클립보드, 현행 유지) |
| **View** | Grid·Axis·ReferenceImage·AO·Fur·Section X/Y/Z/Off (현행 유지) |
| **Draw** | Line·Centerline·Polyline·Rect·RotRect·Polygon·Circle·Ellipse·Arc·Pie·Hole·Freehand·Bezier·Spline·Point·Text3D (현행 유지) |
| **Modify** ⭐ | **기하 편집만** — Move·Copy·Rotate·Scale·Mirror(+x/y/z)·Offset·Array(L/R)·**Trim·Extend·Fillet·Chamfer (2D 로드맵 귀착)**·Erase·Subdivide·Bend/Twist/Taper·Slice·Centerline-convert |
| **Dimension** 🆕 | Measure-selection·Measure-tool·(향후 Linear/Aligned/Angular/Radial) |
| **Modeling** | **3D 빌드** — Primitive(Sphere/Box/Cylinder/Cone/Torus/NURBS)·PushPull·Sweep·Loft·Wall·Window·Revolve·Thicken·Plane·Boolean·Group/Ungroup/Component/Explode·Synthesis/Solidify/Repair·Sketch |
| **Material/Camera/Tools/Window/Extension/Help/Ai** | 현행 유지 |

**이동 요약**: Push/Pull·Sweep·Loft·Wall·Window·Revolve·Thicken·Plane (3D 빌드) 는
Modify → **Modeling**. Primitive 6개 → **Modeling**. Measure → **Dimension**.

## 4. Lock-ins

- **L-210-1** action ID / 단축키 **전부 불변** — 메뉴 라벨·위치만 변경 (muscle memory 의
  키보드 부분 보존). MenuBar dispatch (data-action) 는 그대로.
- **L-210-2** File·Edit·View 첫 3개 보존 (보편 앱 관습).
- **L-210-3** Edit = 클립보드, Modify = 기하 편집 (AutoCAD, 사용자 결재).
- **L-210-4** Modify = 기하 편집만, 3D 빌드(PushPull/Sweep/Loft/Wall/Window/Revolve/Thicken)
  는 Modeling. 2D 편집 로드맵(Trim/Extend/Fillet/Chamfer/Join)의 home = Modify.
- **L-210-5** Primitive → Modeling 통합 (top-level 1개 감소 → Dimension 신설로 14 유지).
- **L-210-6** ADR-046 P31 #4 명시 예외 — 본 ADR 이 재배열 결재 경로.
- **L-210-7** ActionCatalog (ADR-045) 의 action ID 변경 0 — UI 라벨만.
- **L-210-8** 절대 #[ignore] 금지 — MenuBar.test.ts 갱신은 새 구조 정합.

## 5. 구현

- index.html `#menubar` 재배열: top-level 순서 + Dimension 신설 + Primitive→Modeling
  + Modify↔Modeling 내용 재분배.
- AxiaCommands.ts 의 command group / menu 메타데이터 정합 (action ID 불변).
- MenuBar.test.ts 갱신 (새 구조).

## 6. 후속

- **ADR-211+ 2D Sketch Editing** — Trim/Extend (C1), Fillet/Chamfer 2D corner (C2),
  Join + edge transform (C3), Point + Dimension 도구 (C5). 모두 Modify/Dimension 메뉴로
  귀착. 본 ADR 의 메뉴 구조가 그 home.
- Dimension 도구 (Linear/Aligned/Angular/Radial) 구현 시 Dimension 메뉴 확장.

전체 2D CAD coverage audit: `reports/` (4-agent, 2026-06-22).

/**
 * ADR-294 — English strings, keyed by the Korean source text (D2).
 *
 * There is deliberately no `ko.ts`: Korean is the key, so `ko` is the identity
 * function. A key missing from this table renders Korean — which is exactly
 * today's behaviour, so a batch can be wrapped before it is translated.
 *
 * Keep `{name}` placeholders EXACTLY as they appear in the key. They are the
 * one part of a string that must not be translated.
 *
 * Grouped by the migration batch that introduced them (ADR-294 §3), not by
 * feature — so a reviewer can see what a batch actually touched.
 */
export const EN: Record<string, string> = {
  // ── batch 1 — humanizeEngineError (ADR-190 Phase 3) ──
  '곡면은 직접 밀 수 없습니다 — 곡면 위에 원을 그린 뒤 그 면을 미세요':
    'A curved wall cannot be pushed directly — draw a circle on it first, then push that face.',
  '테이퍼(draft)는 직선 경계의 평면 프로파일만 지원합니다 (곡선/곡면 미지원)':
    'Draft extrude supports flat, straight-edged profiles only (no curves or curved surfaces).',
  '위 지름 비율이 100% 이면 원기둥입니다 — 비율 없이 그냥 미세요':
    'A top ratio of 100% is a cylinder — push without a ratio instead.',
  '콘(비율) 돌출은 원형 프로파일만 지원합니다':
    'Cone (ratio) extrude supports circular profiles only.',
  '그 면을 찾을 수 없습니다 — 다시 선택해 주세요':
    'That face no longer exists — please select it again.',
  '곡면 포켓/보스는 곡면 위에 그린 원에서만 만듭니다':
    'Curved pockets and bosses are made from a circle drawn on the curved surface.',
  '이 위치에는 스케치할 수 없습니다 — 기존 구멍/포켓 경계와 겹칩니다 (모델은 그대로입니다)':
    'Cannot sketch here — it overlaps an existing hole or pocket rim. Your model is unchanged.',
  '이 작업은 모델을 깨뜨려서 취소했습니다 — 모델은 그대로입니다':
    'That operation would have broken the model, so it was cancelled. Your model is unchanged.',

  // ══════════════════════════════════════════════════════════════════════
  // batch 2 — the static chrome in index.html (ADR-294 D8)
  //
  // Keys are what the DOM holds, not what the markup spells: index.html
  // writes `&#9633; 직사각형`, so the key is the DECODED '□ 직사각형'.
  //
  // Most already carry their English in parentheses — 「구 (Sphere)」 — so the
  // translation is usually just the term the CAD world already uses. Where the
  // Korean adds a hint the English name does not carry, the hint survives.
  // ══════════════════════════════════════════════════════════════════════

  // ── File ──
  '새로 만들기': 'New',
  '열기': 'Open',
  '저장': 'Save',
  '다른 이름으로 저장': 'Save As',
  '가져오기': 'Import',
  '내보내기': 'Export',
  '삽입': 'Insert',
  '지원되는 모든 유형': 'All supported types',
  'STEP (.step, .stp) — 산업 CAD': 'STEP (.step, .stp) — industry CAD',
  'IGES (.iges, .igs) — 산업 CAD': 'IGES (.iges, .igs) — industry CAD',
  'STEP (.step) — 준비중 (Stage 5)': 'STEP (.step) — coming soon (Stage 5)',
  'IGES (.iges) — 준비중 (Stage 5)': 'IGES (.iges) — coming soon (Stage 5)',
  'IFC (.ifc) — 준비중': 'IFC (.ifc) — coming soon',
  '참조 이미지 불러오기…': 'Load reference image…',
  '🖼️ 텍스처 이미지 업로드…': '🖼️ Upload texture image…',

  // ── Edit ──
  '실행 취소': 'Undo',
  '다시 실행': 'Redo',
  '잘라내기': 'Cut',
  '복사': 'Copy',
  '붙여넣기': 'Paste',
  '삭제': 'Delete',
  '모두 선택': 'Select All',
  '선택 해제': 'Deselect',
  '동일요소 선택': 'Select Same',
  '모두 지우기': 'Clear All',
  '취소': 'Cancel',
  '확인': 'OK',
  '없음': 'None',
  '복제': 'Duplicate',
  '복제 (Copy · 2-click)': 'Duplicate (Copy · 2-click)',

  // ── View ──
  '위 (Top)': 'Top',
  '아래 (Bottom)': 'Bottom',
  '앞 (Front)': 'Front',
  '뒤 (Back)': 'Back',
  '왼쪽 (Left)': 'Left',
  '오른쪽 (Right)': 'Right',
  '3D 투시': '3D Perspective',
  '원점 복귀': 'Reset View',
  '그리드 표시': 'Show Grid',
  '그리드 표시/숨기기': 'Show/Hide Grid',
  '축 표시': 'Show Axes',
  '축 표시/숨기기': 'Show/Hide Axes',
  '엣지 표시': 'Show Edges',
  'AO (주변광 차폐) 토글': 'Toggle AO (ambient occlusion)',
  '털 쉐이더 토글 (Fur shell)': 'Toggle fur shader (fur shell)',
  '🎬 Scenes (저장된 뷰)': '🎬 Scenes (saved views)',

  // ── Draw ──
  '선 (Line)': 'Line',
  '폴리선 (Polyline)': 'Polyline',
  '자유선 (Freehand)': 'Freehand',
  '사각형 (Rectangle)': 'Rectangle',
  '회전 사각형 (Rotated Rect · 3-click)': 'Rotated Rect (3-click)',
  '원 (Circle)': 'Circle',
  '호 (Arc)': 'Arc',
  '호 (3-point Arc)': '3-point Arc',
  '부채꼴 (Pie · Sector)': 'Pie (sector)',
  '타원 (Ellipse · 3-click)': 'Ellipse (3-click)',
  '다각형 (Polygon)': 'Polygon',
  '점 (Point)': 'Point',
  'Bezier 곡선': 'Bezier curve',
  'Bezier 곡선 (Cubic)': 'Bezier curve (cubic)',
  'Spline (B-spline · 가변 점)': 'Spline (B-spline · variable points)',
  'Spline (B-spline · 가변 점, Enter 종료)': 'Spline (B-spline · variable points, Enter to finish)',
  'NURBS 곡면': 'NURBS surface',
  'NURBS 곡면 (NURBS Surface)': 'NURBS surface',
  'NURBS 제어점 편집 (클릭=입력 / 드래그=이동)': 'Edit NURBS control points (click = add / drag = move)',
  '3D 텍스트': '3D text',
  '📐 중심선 (Centerline)': '📐 Centerline',
  '📐 중심선으로 변환': '📐 Convert to centerline',
  '📐 선택 엣지 → 중심선 변환': '📐 Selected edges → centerline',
  '🔹 일반선으로 변환': '🔹 Convert to normal line',
  '🔹 선택 엣지 → 일반선 변환': '🔹 Selected edges → normal line',
  '□ 직사각형': '□ Rectangle',

  // ── Primitives ──
  '박스 (Box)': 'Box',
  '구 (Sphere)': 'Sphere',
  '원통 (Cylinder)': 'Cylinder',
  '원뿔 (Cone)': 'Cone',
  '토러스 (Torus)': 'Torus',
  '벽 (Wall · 기준선 → 두께·높이 압출)': 'Wall (baseline → extrude thickness & height)',
  '창 (Window · 벽 면에 사각 개구부)': 'Window (rectangular opening in a wall face)',
  '구멍 (Hole)': 'Hole',
  '⊘ 구멍 (Hole)': '⊘ Hole',
  '⬡ 다각형 구멍 (Polygon Hole · 임의 윤곽 관통)':
    '⬡ Polygon hole (through, arbitrary outline)',
  '⭕ Annulus 만들기 (2개 원, 내부 ⊂ 외부)': '⭕ Make annulus (2 circles, inner ⊂ outer)',
  '작업 평면 (3-Point Plane · 3점으로 평면 고정)': 'Work plane (3-point plane)',
  '📐 평면': '📐 Plane',

  // ── Modify ──
  '이동 (Move)': 'Move',
  '회전 (Rotate)': 'Rotate',
  '크기 (Scale)': 'Scale',
  '크기 조정 (Scale)': 'Scale',
  '오프셋 (Offset)': 'Offset',
  '삭제 (Erase)': 'Erase',
  '트림 (Trim)': 'Trim',
  '익스텐드 (Extend)': 'Extend',
  '분해 (Explode)': 'Explode',
  '면 반전': 'Flip face',
  '면 머지': 'Merge faces',
  '돌출/잘라내기 (Extrude/Cut · Volume)': 'Extrude / Cut (volume)',
  '포켓 (Recess · Pocket · 면 클릭→여유 깊이)':
    'Recess (pocket · click a face → depth)',
  '서브디비전 (Subdivide)': 'Subdivide (smooth)',
  '조인 (Join · 일직선)': 'Join lines (collinear)',
  '엣지 필렛 (Fillet)…': 'Fillet edge…',
  '엣지 챔퍼 (Chamfer)…': 'Chamfer edge…',
  '꼭짓점 챔퍼 (Vertex Chamfer)': 'Vertex chamfer',
  '코너 필렛 (Corner Fillet · 2D)': 'Corner fillet (2D)',
  '코너 챔퍼 (Corner Chamfer · 2D)': 'Corner chamfer (2D)',
  '필렛 도구 (Fillet · 엣지+반지름 · 반복)': 'Fillet tool (edge + radius · repeatable)',
  '선형 배열 (Array)…': 'Linear array…',
  '선형 배열 (Array Linear)…': 'Linear array…',
  '선형 배열 복제 (Array)…': 'Linear array…',
  '선형 배열 도구 (2-click · 개수 VCB)': 'Linear array tool (2-click · count via VCB)',
  '원형 배열 (Radial)…': 'Radial array…',
  '원형 배열 (Array Radial)…': 'Radial array…',
  '원형 배열 복제 (Radial)…': 'Radial array…',
  '원형 배열 도구 (X/Y/Z 축 · 개수 VCB)': 'Radial array tool (X/Y/Z axis · count via VCB)',
  '미러 도구 (Mirror · X/Y/Z 전환 · 반복)': 'Mirror tool (X/Y/Z · repeatable)',
  '미러 · XY 평면 (Z 반전)': 'Mirror · XY plane (flip Z)',
  '미러 · XZ 평면 (Y 반전)': 'Mirror · XZ plane (flip Y)',
  '미러 · YZ 평면 (X 반전)': 'Mirror · YZ plane (flip X)',
  '대칭 · XY 평면 (Z 반전)': 'Mirror · XY plane (flip Z)',
  '대칭 · XZ 평면 (Y 반전)': 'Mirror · XZ plane (flip Y)',
  '대칭 · YZ 평면 (X 반전)': 'Mirror · YZ plane (flip X)',
  'Mirror · XY 평면 (Z 반전)': 'Mirror · XY plane (flip Z)',
  'Mirror · XZ 평면 (Y 반전)': 'Mirror · XZ plane (flip Y)',
  'Mirror · YZ 평면 (X 반전)': 'Mirror · YZ plane (flip X)',
  '선택 구부리기 (Bend)…': 'Bend selection…',
  '선택 비틀기 (Twist)…': 'Twist selection…',
  '선택 테이퍼 (Taper)…': 'Taper selection…',
  '모델과 교차 (Intersect with Model)': 'Intersect with model',
  '🧱 셸 (Shell/Thicken)…': '🧱 Thicken / shell…',
  '🧱 셸 (Thicken/Shell)…': '🧱 Thicken / shell…',
  '🔪 평면으로 자르기 (Slice)': '🔪 Slice by plane',
  '🎨 색상 지정… (Quick Color)': '🎨 Quick color…',
  '🎨 빠른 색상 지정 (Quick Color)…': '🎨 Quick color…',

  // ── Sweeps / solids ──
  '스윕 (Sweep · 경로 따라 파이프)': 'Sweep (pipe along a path)',
  '로프트 (Loft · 단면 블렌드 화병)': 'Loft (blend between sections)',
  '로프트 — 선택 면 2개 (Loft 2 faces)': 'Loft — 2 selected faces',
  '회전체 Revolve · X축': 'Revolve · X axis',
  '회전체 Revolve · Y축': 'Revolve · Y axis',
  '회전체 Revolve · Z축': 'Revolve · Z axis',
  'Revolve · X축': 'Revolve · X axis',
  'Revolve · Y축': 'Revolve · Y axis',
  'Revolve · Z축': 'Revolve · Z axis',
  'Revolve · X축 회전': 'Revolve · X axis',
  'Revolve · Y축 회전': 'Revolve · Y axis',
  'Revolve · Z축 회전': 'Revolve · Z axis',
  '회전체 — 선택 면 (Revolve · 각도 입력 · 부분/360°)':
    'Revolve — selected face (angle · partial or 360°)',

  // ── Boolean ──
  'BREP ∪ · 합집합 (Union)': 'BREP ∪ · Union',
  'BREP − · 차집합 (Subtract)': 'BREP − · Subtract',
  'BREP ∩ · 교집합 (Intersect)': 'BREP ∩ · Intersect',
  'ⓐ Boolean Group A 지정': 'ⓐ Assign to Boolean group A',
  'ⓑ Boolean Group B 지정': 'ⓑ Assign to Boolean group B',
  '🗑 Boolean Group 해제': '🗑 Clear Boolean groups',

  // ── Groups ──
  '그룹 (Group)': 'Group',
  '그룹 만들기': 'Make group',
  '그룹 해제': 'Ungroup',
  '그룹 해제 (Ungroup)': 'Ungroup',
  '그룹 편집': 'Edit group',
  '그룹 숨기기/표시': 'Hide/show group',
  '그룹 잠금/해제': 'Lock/unlock group',
  '컴포넌트로 변환': 'Convert to component',
  '📁 컴포넌트 / 그룹 패널': '📁 Components / groups panel',

  // ── Sketch ──
  '✏️ XZ 바닥 (평면도)': '✏️ XZ ground (plan)',
  '✏️ XY 정면 (입면도)': '✏️ XY front (elevation)',
  '✏️ YZ 측면': '✏️ YZ side',
  '✏️ 선택 면에서 스케치': '✏️ Sketch on selected face',
  '✏️ 스케치 시작 · XZ 바닥 (평면도)': '✏️ Start sketch · XZ ground (plan)',
  '✏️ 스케치 시작 · XY 정면 (입면도)': '✏️ Start sketch · XY front (elevation)',
  '✏️ 스케치 시작 · YZ 측면': '✏️ Start sketch · YZ side',
  '✏️ 스케치 시작 · 선택 면': '✏️ Start sketch · selected face',
  '✨ 스케치 시작 · 자동 평면 감지': '✨ Start sketch · auto-detect plane',
  '↩ 스케치 재개 · 마지막 평면': '↩ Resume sketch · last plane',
  '↻ 스케치 up 카메라 정렬': '↻ Align camera to sketch up',
  '스케치 종료': 'Exit sketch',
  '스케치 종료 → 3D 변환': 'Exit sketch → 3D',
  '📐 기본 평면으로 (평면 초기화)': '📐 Back to the default plane (reset)',

  // ── Snap ──
  '객체 스냅 모드': 'Object snap modes',
  '객체 스냅 설정': 'Object snap settings',
  '객체 스냅 설정(O)...': 'Object snap settings (O)…',
  '객체 스냅 켜기 (F3)(O)': 'Object snap on (F3) (O)',
  '객체 스냅 추적 켜기 (F11)(K)': 'Object snap tracking on (F11) (K)',
  '끝점': 'Endpoint',
  '끝점(E)': 'Endpoint (E)',
  '중간점': 'Midpoint',
  '중간점(M)': 'Midpoint (M)',
  '교차점': 'Intersection',
  '교차점(I)': 'Intersection (I)',
  '가상 교차점': 'Apparent intersection',
  '가상 교차점(A)': 'Apparent intersection (A)',
  '중심점': 'Center',
  '중앙(C)': 'Center (C)',
  '기하학적 중심': 'Geometric center',
  '기하학적 중심(G)': 'Geometric center (G)',
  '사분점': 'Quadrant',
  '사분점(Q)': 'Quadrant (Q)',
  '접점': 'Tangent',
  '접점(N)': 'Tangent (N)',
  '수직점': 'Perpendicular',
  '수직(U)': 'Perpendicular (U)',
  '평행': 'Parallel',
  '평행(L)': 'Parallel (L)',
  '근처점': 'Nearest',
  '근처점(R)': 'Nearest (R)',
  '노드': 'Node',
  '연장선': 'Extension',
  '연장(X)': 'Extension (X)',
  '면 위(F)': 'On face (F)',
  '2점 사이의 중간': 'Midway between 2 points',
  '임시 추적점': 'Temporary tracking point',
  '스냅 재지정(V)': 'Snap override (V)',
  '스냅 표시 크기(S)': 'Snap marker size (S)',
  '제도 설정값': 'Drafting settings',

  // ── Inspector / properties ──
  '🔍 XIA 인스펙터': '🔍 XIA inspector',
  '▼ 기하학적 속성': '▼ Geometric properties',
  '▼ 물리적 속성': '▼ Physical properties',
  '속성': 'Properties',
  '치수': 'Dimensions',
  '치수 정보': 'Dimensions',
  // '치수:' lives in batch 3b — index.html's #cmd-label initial value and the
  // VCB's fallback label are the same string in the same element.
  '길이 L': 'Length L',
  '너비 W': 'Width W',
  '높이 H': 'Height H',
  '두께': 'Thickness',
  '면적': 'Area',
  '부피': 'Volume',
  '질량': 'Mass',
  '무게': 'Weight',
  '무게(중력)': 'Weight (gravity)',
  '밀도': 'Density',
  '비용': 'Cost',
  '비용 산출': 'Cost estimate',
  '재질': 'Material',
  '열전도율': 'Thermal conductivity',
  '화재 등급': 'Fire rating',
  '불연': 'Non-combustible',
  '준불연': 'Semi-combustible',
  '난연': 'Flame-retardant',
  '시작점': 'Start point',
  '객체를 선택하면': 'Select an object to',
  '속성이 표시됩니다': 'see its properties',
  '재질을 부여하면 이 객체는': 'Assigning a material promotes this object to',
  '로 승격됩니다': '',
  'XIA (특성)': 'XIA (property)',
  '형태 (Shape)': 'Shape (form)',
  '— 없음 (형태 Shape) —': '— none (Shape) —',

  // ── Style ──
  '스타일': 'Style',
  '프리셋 스타일': 'Style presets',
  '면 스타일': 'Face style',
  '엣지 스타일': 'Edge style',
  '앞면': 'Front face',
  '뒷면': 'Back face',
  '배경': 'Background',
  '바닥색': 'Ground color',
  '하늘색': 'Sky color',
  '중간색': 'Horizon color',
  '단색': 'Solid',
  '2색 그라데이션': '2-color gradient',
  '3색 그라데이션': '3-color gradient',
  '그리드 색': 'Grid color',
  '색상': 'Color',
  '투명도': 'Opacity',
  '프로필 엣지 (외곽 강조)': 'Profile edges (emphasize silhouette)',
  '각도 임계': 'Angle threshold',
  '모드': 'Mode',
  '환경': 'Environment',
  '시각': 'Time of day',
  '시각 설정': 'Time of day',
  '단위 설정': 'Unit settings',

  // ── Dimensions / measure ──
  '📏 측정 도구 (2점 거리 / 3점 각도)': '📏 Measure (2-point distance / 3-point angle)',
  '📏 치수 표시 ON/OFF': '📏 Show dimensions on/off',
  '📐 선형 치수 (Linear Dimension · 영구·편집)':
    '📐 Linear dimension (persistent · editable)',
  '📐 각도 치수 (Angular Dimension · 영구·편집)':
    '📐 Angular dimension (persistent · editable)',
  '📐 반지름 치수 (Radial Dimension · 원/호 · 영구·편집)':
    '📐 Radial dimension (circle/arc · persistent · editable)',
  '📐 참조 치수 (Reference Dimension · 읽기전용)':
    '📐 Reference dimension (read-only)',
  '선택 측정 (길이/면적/부피)': 'Measure selection (length / area / volume)',

  // ── Constraints ──
  '🔗 구속 조건 패널 (Constraints)': '🔗 Constraints panel',
  '엣지 평행 정렬': 'Make edges parallel',
  '엣지 수직 정렬': 'Make edges perpendicular',
  '엣지 동일 선상 정렬': 'Make edges collinear',
  '엣지 길이 설정…': 'Set edge length…',
  '끝점 거리 고정…': 'Fix endpoint distance…',
  '엣지 중점 분할': 'Split edge at midpoint',

  // ── Mesh utilities ──
  '🩹 Heal · Mesh Repair (퇴화/와인딩/고립 정리)':
    '🩹 Heal · mesh repair (degenerate / winding / orphans)',
  '🧩 Solidify (열린 쉘 → 닫힌 솔리드)': '🧩 Solidify (open shell → closed solid)',
  '🧩 Solidify (열린 쉘 → 솔리드)': '🧩 Solidify (open shell → solid)',
  '🔧 T-junction 정리 (vertex on edge interior → split + heal)':
    '🔧 Fix T-junctions (vertex on edge interior → split + heal)',
  '🧹 Coplanar 면 일괄 자동 정리 (mergeable pair sweep → batch merge)':
    '🧹 Auto-clean coplanar faces (sweep mergeable pairs → batch merge)',
  '🧲 기하 머지 (크기 달라도 OK, 2면 선택)':
    '🧲 Geometric merge (different sizes OK, select 2 faces)',
  '🫧 강제 머지 (비평면, 내부 엣지 숨김)':
    '🫧 Force merge (non-planar, hides interior edges)',
  '🔄 면 재합성 (닫힌 라인 cycle → face)':
    '🔄 Rebuild faces (closed line cycle → face)',
  '자유 엣지 → 면 합성': 'Free edges → face',
  'XIA 내 Coplanar 면 일괄 머지': 'Merge coplanar faces within a XIA',
  '🕳 수동: 내부 면을 구멍으로 (레거시 · 신규 그리기는 자동)':
    '🕳 Manual: inner face → hole (legacy · new drawings do this automatically)',

  // ── Other panels / tools ──
  '⚠️ 간섭 감지 (Clash Detection)': '⚠️ Clash detection',
  '⚠️ 간섭 표시 해제': '⚠️ Clear clash display',
  '✂️ 섹션 평면 · X축 (좌우 단면)': '✂️ Section plane · X axis',
  '✂️ 섹션 평면 · Y축 (수평 단면)': '✂️ Section plane · Y axis',
  '✂️ 섹션 평면 · Z축 (전후 단면)': '✂️ Section plane · Z axis',
  '✂️ 섹션 평면 해제': '✂️ Clear section plane',
  '☀️ 태양 방향 패널': '☀️ Sun position panel',
  '🎨 재질 속성 패널 (Materials)': '🎨 Materials panel',
  '🕒 작업 기록 패널 (Parametric)': '🕒 History panel (parametric)',
  '키보드 단축키': 'Keyboard shortcuts',
  'AXiA 3D 정보': 'About AXiA 3D',
  'AI 협업 — 준비중': 'AI collaboration — coming soon',
  '확장 (Extension) — 준비중': 'Extensions — coming soon',
  '(준비 중)': '(coming soon)',

  // ── Tooltips (title=) ──
  '메뉴': 'Menu',
  '설정 (Settings)': 'Settings',
  '단위 / 정밀도': 'Units / precision',
  '도움말 (F1)': 'Help (F1)',
  '전체 화면 (Fullscreen)': 'Fullscreen',
  '그리드 (F4)': 'Grid (F4)',
  '엣지 (F6)': 'Edges (F6)',
  '축 (F7)': 'Axes (F7)',
  '그리드 표시/숨기기 (`)': 'Show/hide grid (`)',
  '뷰 원점 복귀 (F5)': 'Reset view (F5)',
  '원점으로 (H)': 'Home (H)',
  '건축 그림자 토글': 'Toggle architectural shadows',
  '변형 선택': 'Pick a variant',
  '선 종류 선택': 'Pick a line type',
  '원/호 종류 선택': 'Pick a circle/arc type',
  '프리미티브 선택': 'Pick a primitive',
  '메시 유틸리티': 'Mesh utilities',
  '유기 모델링 작업': 'Organic modelling',
  'Boolean 연산': 'Boolean operations',
  '그룹 / 컴포넌트': 'Groups / components',
  '그룹 만들기 (Ctrl+G)': 'Make group (Ctrl+G)',
  '스케치 옵션': 'Sketch options',
  '재질 부여/해제': 'Assign / clear material',
  'XIA 이름 변경 (F2)': 'Rename XIA (F2)',
  '이름 입력...': 'Enter a name…',
  '숫자 입력 후 Enter...': 'Type a number, then Enter…',
  '서브디비전 (Catmull-Clark)': 'Subdivide (Catmull-Clark)',
  'RotRect · 회전 사각형 (3-click)': 'RotRect · rotated rectangle (3-click)',
  'Sweep · 경로 따라 파이프 (W)': 'Sweep · pipe along a path (W)',
  'Loft · 단면 블렌드 화병': 'Loft · blend between sections',
  'Wall · 기준선 → 두께·높이 압출': 'Wall · baseline → extrude thickness & height',
  'Window · 벽 면에 사각 개구부': 'Window · rectangular opening in a wall face',
  'Recess · Pocket (포켓)': 'Recess · pocket',
  'Knife · 평면으로 자르기 (Slice)': 'Knife · slice by plane',
  'Solidify (열린 쉘 → 닫힌 솔리드)': 'Solidify (open shell → closed solid)',
  'BREP ∪ · 합집합 (Union) [F8]': 'BREP ∪ · Union [F8]',
  '3-Point Plane · 3점으로 작업 평면 고정': '3-point plane · fix a work plane from 3 points',
  '스케치 시작 · XZ 바닥 (평면도)': 'Start sketch · XZ ground (plan)',
  '스케치 모드 활성 — 모든 드로잉이 평면에 고정됩니다':
    'Sketch mode on — every drawing is pinned to the plane',
  '마지막 그린 평면 기억 활성 — 다음 도형이 같은 평면에 그려집니다 (ADR-164 sticky)':
    'Sticky plane on — the next shape lands on the same plane (ADR-164)',
  '인접 면 각도 임계 (도). 작을수록 panel 경계 더 많이 표시. 건축=10°, 기계=20°, 캐릭터=30°':
    'Angle threshold between adjacent faces (degrees). Lower shows more panel edges. Architecture = 10°, mechanical = 20°, character = 30°',

  // ══════════════════════════════════════════════════════════════════════
  // batch 3 — TS-built panels (ADR-294 §3). These re-render, so they are
  // wrapped with t() rather than swept by translateDom (L-294-11).
  //
  // Settings first: it is where the language switch lives, so a user who
  // switches to English and lands on a Korean panel is the sharpest possible
  // version of the mixed-UI problem.
  //
  // The hints name ADRs and engine internals. That is deliberate — these are
  // experimental toggles for someone who reads ADRs, so the reference is the
  // useful part and is kept verbatim.
  // ══════════════════════════════════════════════════════════════════════
  '언어 / Language': 'Language',
  '단위': 'Units',
  '바꾸면 화면을 다시 불러옵니다 / Reloads the page (ADR-294)':
    'Changing this reloads the page (ADR-294)',
  '소수점 자릿수': 'Decimal places',
  '그리드 스냅': 'Grid snap',
  '스냅 간격': 'Snap spacing',
  '원통 세그먼트 (원주 분할 수)': 'Cylinder segments (divisions around the circumference)',
  '많을수록 매끈하지만 면·정점 증가 (기본 16)':
    'More is smoother but adds faces and vertices (default 16)',
  '면 병합 허용 각도': 'Face-merge angle tolerance',
  '작은 값(0.5°)은 CAD-grade · 큰 값은 관대한 병합':
    'Small (0.5°) is CAD-grade · large merges more freely',
  '재질 경계 존중 (다른 재질은 병합 안 함)':
    'Respect material boundaries (never merge across materials)',
  '그릴 때 자동 교차 (Auto-intersect on draw)': 'Auto-intersect on draw',
  '새 면이 기존 면과 3D 교차하면 edge 로 자동 분할 (SketchUp 스타일)':
    'A new face that intersects an existing one in 3D is split at the edge automatically (SketchUp style)',
  '곡선 모드 (실험) — kernel-native 닫힌 곡선':
    'Curve mode (experimental) — kernel-native closed curves',
  'DrawCircle: 24-segment polygon 대신 1 self-loop edge + AnalyticCurve::Circle 로 그리기 (ADR-089)':
    'DrawCircle draws 1 self-loop edge + AnalyticCurve::Circle instead of a 24-segment polygon (ADR-089)',
  '위상 손상 자동 복구 (실험)': 'Auto-recover topology damage (experimental)',
  '토폴로지 변경 op 후 손상 감지 → 자동 복구. PartialFailure 시 사용자 다이얼로그 ([Undo]/[강등]/[수동수정]) (ADR-097 Phase 4)':
    'Detects damage after a topology-changing op and repairs it. On partial failure you get a dialog ([Undo] / [Demote] / [Fix manually]) (ADR-097 Phase 4)',
  'User 라이브러리 활성화 (실험)': 'Enable the User library (experimental)',
  '자산 라이브러리 의 User tier (사용자 재사용 재질 모음) 활성. localStorage 보존, opt-in default OFF (ADR-098 Phase 5-A)':
    'Enables the asset library\'s User tier (your reusable materials). Kept in localStorage, opt-in, off by default (ADR-098 Phase 5-A)',
  '재질 삭제 자동 복구 (실험)': 'Auto-recover on material removal (experimental)',
  'Material 제거 시 owning Xia 의 자동 복구 (auto-demote → fallback Concrete). PartialFailure 시 사용자 다이얼로그 ([Undo]/[강등]/[수동수정]) (ADR-100 Phase 5-C)':
    'Recovers the owning XIA when its material is removed (auto-demote → fall back to Concrete). On partial failure you get a dialog ([Undo] / [Demote] / [Fix manually]) (ADR-100 Phase 5-C)',
  '3D 텍스트: 스프라이트 모드': '3D text: sprite mode',
  '체크 = 캔버스 빌보드 라벨 (한국어 즉시, 카메라 대면). 해제 = 압출 3D 텍스트 (Latin, 한국어는 자동 스프라이트 fallback) (ADR-228)':
    'On = canvas billboard label (Korean works immediately, always faces the camera). Off = extruded 3D text (Latin; Korean falls back to a sprite automatically) (ADR-228)',
  'NURBS 곡면: 볼트(반원통) 모드': 'NURBS surface: vault (half-cylinder) mode',
  '체크 = 정확한 rational 반원통 vault (createNurbsSurface, 정확한 원호 단면). 해제 = bicubic Bezier bulge (현재) (ADR-231)':
    'On = an exact rational half-cylinder vault (createNurbsSurface, exact arc section). Off = a bicubic Bezier bulge (current) (ADR-231)',
  'Push/Pull 돌출 방향 (ADR-261)': 'Push/Pull extrude direction (ADR-261)',
  '단방향 (OneWay) — 기존': 'One-way — the existing behaviour',
  '대칭 (Symmetric) — 양쪽 각 거리': 'Symmetric — that distance each way',
  '비대칭 (TwoSided) — 위/아래 따로': 'Two-sided — up and down set separately',
  '아래(−) 거리 (mm)': 'Down (−) distance (mm)',
  '대칭 = profile 평면 기준 양쪽 각 d (총 2d). 비대칭 = +방향은 돌출 거리, −방향은 위 값. 단방향이 기본 (동작 불변).':
    'Symmetric = d each way from the profile plane (2d total). Two-sided = the extrude distance goes up, this value goes down. One-way is the default and behaves as before.',

  // ── batch 3 — ShortcutHelpModal (F1) ──
  // Most rows are `English (Korean gloss)` — 'Select (선택)' — so the English
  // is just the term with the gloss dropped. Keys are never translated
  // ('Ctrl+Z' is a key, not a word); the three Korean "keys" are gestures.
  'AXiA 3D 키보드 단축키': 'AXiA 3D keyboard shortcuts',
  'F1로 다시 열기 · Esc로 닫기': 'F1 to reopen · Esc to close',

  '도구': 'Tools',
  '편집': 'Edit',
  '보기 / 화면': 'View / display',
  '스냅 / 축': 'Snap / axis',
  '패널': 'Panels',
  '스케치 / 선택': 'Sketch / selection',

  'Select (선택)': 'Select',
  'Line (선)': 'Line',
  'Rect (사각형)': 'Rect',
  'Circle (원)': 'Circle',
  '📐 Centerline (중심선)': '📐 Centerline',
  'Arc (호)': 'Arc',
  'Polygon (다각형)': 'Polygon',
  'Extrude/Cut (돌출/잘라내기 · Volume)': 'Extrude / Cut (volume)',
  'Sphere (구)': 'Sphere',
  'Cylinder (원통)': 'Cylinder',
  'Cone (원뿔)': 'Cone',
  'Move (이동)': 'Move',
  'Rotate (회전)': 'Rotate',
  'Erase (지우기)': 'Erase',
  'Measure Tool (2점 거리 / 3점 각도)': 'Measure (2-point distance / 3-point angle)',
  'Select 도구로 복귀': 'Back to the Select tool',

  'Undo (되돌리기)': 'Undo',
  'Redo (다시 실행)': 'Redo',
  '복사 (선택된 면)': 'Copy (selected faces)',
  '잘라내기 (복사 + 삭제)': 'Cut (copy + delete)',
  '붙여넣기 (offset 500,0,500mm)': 'Paste (offset 500,0,500mm)',
  '복제 (즉시 duplicate)': 'Duplicate (immediately)',
  'Select All (전체 선택)': 'Select all',
  '프로젝트 저장': 'Save project',
  '프로젝트 열기': 'Open project',
  '재질 패널': 'Materials panel',
  '취소 / 선택 해제': 'Cancel / deselect',
  '선택 XIA 이름 변경': 'Rename the selected XIA',
  'Face Reverse (면 뒤집기)': 'Reverse face',

  '이 도움말': 'This help',
  'OSNAP 토글': 'Toggle OSNAP',
  '그리드 표시/숨김': 'Show/hide grid',
  '뷰 원점 복귀 (카메라 리셋)': 'Reset the view (camera)',
  '엣지 표시/숨김': 'Show/hide edges',
  '축 표시/숨김': 'Show/hide axes',
  '그리드 표시/숨김 (대체)': 'Show/hide grid (alternative)',
  'Top / Bottom 뷰': 'Top / Bottom view',
  'Front / Back 뷰': 'Front / Back view',
  '3D 투시 뷰': '3D perspective view',

  'Tentative snap 순환': 'Cycle tentative snaps',
  'Inference Lock (스냅 고정)': 'Inference lock (pin the snap)',
  'X축 고정': 'Lock to the X axis',
  'Y축 고정': 'Lock to the Y axis',
  'Z축 고정': 'Lock to the Z axis',
  '축 고정 해제': 'Release the axis lock',
  'Endpoint 스냅 토글': 'Toggle endpoint snap',
  'Midpoint 스냅 토글': 'Toggle midpoint snap',
  'Intersection 스냅 토글': 'Toggle intersection snap',
  'Center 스냅 토글': 'Toggle center snap',
  'Perpendicular 스냅 토글': 'Toggle perpendicular snap',
  'Parallel 스냅 토글': 'Toggle parallel snap',
  'OnFace 스냅 토글': 'Toggle on-face snap',
  'Grid 스냅 토글': 'Toggle grid snap',
  'Nearest 스냅 토글': 'Toggle nearest snap',

  'Outliner (컴포넌트 패널)': 'Outliner (components panel)',
  'Constraint 패널': 'Constraints panel',
  '작업 기록 패널 (Parametric History)': 'History panel (parametric)',

  'Alt+엣지 클릭': 'Alt + click an edge',
  '메뉴 → ✏️': 'Menu → ✏️',
  '메뉴 → 스케치 종료': 'Menu → Exit sketch',
  '폴리라인 체인 자동 선택 (Loop Select)': 'Select the whole polyline chain (loop select)',
  'Sketch 모드 시작 (XZ 바닥 / XY 정면 / YZ 측면 / 선택 면)':
    'Start sketch mode (XZ ground / XY front / YZ side / selected face)',
  '닫힌 프로필 자동 감지 → 높이 prompt → Extrude/Cut':
    'Detects a closed profile → asks for a height → extrudes / cuts',
  '우클릭 → 색상 지정 (선택 면에 즉석 커스텀 material)':
    'Right-click → set a colour (an instant custom material on the selected faces)',

  // ══════════════════════════════════════════════════════════════════════
  // batch 4 — the two catalogs (ADR-294 §3)
  //
  // Rendered by the Capability Explorer (ActionCatalog) and the Command
  // Palette (CommandCatalog). Neither catalog imports t(): @axia/action-catalog
  // is a zero-dependency data package, and reaching into web/src/i18n from it
  // would invert the layering. The panels translate at render instead.
  //
  // ActionCatalog's 213 descriptions are already English — nothing to do there.
  //
  // CommandCatalog carries a `short` as well as a `label`, for the palette's
  // narrow column: '회전사각' is an abbreviation, not a word, so its English is
  // an abbreviation too ('RotRect').
  // ══════════════════════════════════════════════════════════════════════

  // ── tools ──
  '선택': 'Select',
  '선택 (Select)': 'Select',
  '선': 'Line',
  '폴리선': 'Polyline',
  '자유선': 'Freehand',
  '사각형': 'Rectangle',
  '회전 사각형': 'Rotated rect',
  '회전 사각형 (Rotated Rectangle · 3-click)': 'Rotated rectangle (3-click)',
  '회전사각': 'RotRect',
  '원': 'Circle',
  '호': 'Arc',
  '부채꼴': 'Pie',
  '부채꼴 (Pie / Sector · 3-click)': 'Pie / sector (3-click)',
  '타원': 'Ellipse',
  '타원 (Ellipse)': 'Ellipse',
  '다각형': 'Polygon',
  '점': 'Point',
  '스플라인': 'Spline',
  '스플라인 (Spline · open B-spline)': 'Spline (open B-spline)',
  '텍스트': 'Text',
  '중심선': 'Centerline',
  '중심선으로 변환': 'Convert to centerline',
  '일반선으로 변환': 'Convert to normal line',
  '→중심선': '→ centerline',
  '→일반': '→ normal',
  '📐 엣지 → 중심선 변환': '📐 Edges → centerline',
  '🔹 엣지 → 일반선 변환': '🔹 Edges → normal line',
  'NURBS 곡면 (NURBS Surface · 2-click bicubic patch)':
    'NURBS surface (2-click bicubic patch)',
  'NURBS 제어점 편집 (위치·weight)': 'Edit NURBS control points (position & weight)',
  'NURBS편집': 'NURBS edit',
  '작업 평면': 'Work plane',
  '평면': 'Plane',

  // ── primitives ──
  '박스': 'Box',
  '구': 'Sphere',
  '원통': 'Cylinder',
  '원뿔': 'Cone',
  '토러스': 'Torus',
  '벽': 'Wall',
  '창': 'Window',
  '구멍': 'Hole',
  '⊘ 구멍 (Hole · 면에 원형 구멍)': '⊘ Hole (a round hole in a face)',
  '다각형 구멍 (Polygon Hole · 임의 윤곽 관통)': 'Polygon hole (through, arbitrary outline)',
  '다각형구멍': 'Poly hole',
  '수동 구멍': 'Manual hole',
  '원 → 환형 승격': 'Circle → annulus',
  '내부 면 → 구멍으로 합치기': 'Inner face → merge as a hole',
  '포켓': 'Recess',
  '포켓 (Recess · Pocket)': 'Recess (pocket)',

  // ── modify ──
  '이동': 'Move',
  '회전': 'Rotate',
  '크기': 'Scale',
  '크기 조정': 'Scale',
  '오프셋': 'Offset',
  '트림': 'Trim',
  '익스텐드': 'Extend',
  '분해': 'Explode',
  '분해 (Explode · = 그룹 해제)': 'Explode (= ungroup)',
  '돌출/잘라내기': 'Extrude / Cut',
  '셸': 'Thicken',
  '서브디비전': 'Subdivide',
  '필렛': 'Fillet',
  '필렛 도구 (Fillet · 엣지+반지름)': 'Fillet tool (edge + radius)',
  '챔퍼': 'Chamfer',
  '엣지 필렛': 'Fillet edge',
  '엣지 챔퍼': 'Chamfer edge',
  '꼭짓점 챔퍼': 'Vertex chamfer',
  '코너 필렛': 'Corner fillet',
  '코너 필렛 (Corner Fillet · 2D 코너+반지름)': 'Corner fillet (2D corner + radius)',
  '코너필렛': 'Corner fillet',
  '코너 챔퍼': 'Corner chamfer',
  '코너 챔퍼 (Corner Chamfer · 2D 코너+거리)': 'Corner chamfer (2D corner + distance)',
  '코너챔퍼': 'Corner chamfer',
  '미러': 'Mirror',
  '미러 (Mirror · X/Y/Z 평면)': 'Mirror (X/Y/Z plane)',
  '미러 · XY 평면': 'Mirror · XY plane',
  '미러 · XZ 평면': 'Mirror · XZ plane',
  '미러 · YZ 평면': 'Mirror · YZ plane',
  '선형 배열': 'Linear array',
  '선형 배열 도구 (Array Linear · 2-click)': 'Linear array tool (2-click)',
  '선형배열': 'Lin. array',
  '원형 배열': 'Radial array',
  '원형 배열 도구 (Array Radial · X/Y/Z 축)': 'Radial array tool (X/Y/Z axis)',
  '원형배열': 'Rad. array',
  '복제 (Copy · 2-click offset)': 'Duplicate (2-click offset)',
  '구부리기': 'Bend',
  '비틀기': 'Twist',
  '테이퍼': 'Taper',
  '모델과 교차': 'Intersect with model',
  '평면으로 자르기': 'Slice by plane',
  '평면으로 자르기/칼 (Slice/Cut)': 'Slice / cut by plane',
  '빠른 색상 지정': 'Quick colour',
  '🎨 빠른 색상 (Quick Color)…': '🎨 Quick colour…',
  '스윕': 'Sweep',
  '로프트': 'Loft',
  '로프트 (선택 면 2개)': 'Loft (2 selected faces)',
  '회전체 — 선택 면': 'Revolve — selected face',
  '회전체 — 선택 면 (Revolve · 각도)': 'Revolve — selected face (angle)',

  // ── boolean ──
  '합집합': 'Union',
  '합집합 (Union)': 'Union',
  '차집합': 'Subtract',
  '차집합 (Subtract)': 'Subtract',
  '교집합': 'Intersect',
  '교집합 (Intersect)': 'Intersect',
  'Boolean 그룹 A': 'Boolean group A',
  'Boolean 그룹 B': 'Boolean group B',
  'Boolean 그룹 해제': 'Clear Boolean groups',
  '불리언 디스패치 (NURBS-aware)': 'Boolean dispatch (NURBS-aware)',
  '필렛 디스패치 (NURBS-aware)': 'Fillet dispatch (NURBS-aware)',

  // ── faces / mesh repair ──
  '면': 'Face',
  '면 뒤집기 (Flip Faces)': 'Flip faces',
  '면 합성': 'Build face',
  '면 머지 (Merge)': 'Merge faces',
  '면 머지 · 강제': 'Merge faces · force',
  '면 머지 · 기하 기반': 'Merge faces · geometric',
  '강제 머지': 'Force merge',
  '기하 머지': 'Geometric merge',
  '동일 XIA · 동일평면 머지': 'Merge coplanar faces in the same XIA',
  'XIA 내 coplanar 면': 'Coplanar faces in a XIA',
  '공면 쌍 치유': 'Heal coplanar pairs',
  'T-정션 치유': 'Heal T-junctions',
  'P7 정규형 강제': 'Force P7 canonical form',
  '경계 도구 (면 재합성)': 'Boundary tool (rebuild faces)',
  // '선 병합' (label) and '선병합' (the palette's abbreviation) both became
  // '조인', because the transliteration is already short enough not to need an
  // abbreviation. One key, one English.
  '조인 (Join · 일직선 2-valence 코너)': 'Join lines (collinear, 2-valence corner)',
  '조인': 'Join',
  '🧩 솔리드화 (Solidify)': '🧩 Solidify',
  '🩹 메시 수리': '🩹 Mesh repair',
  '곡선·표면 마이그레이션': 'Curve & surface migration',

  // ── surface attach (diagnostics) ──
  '평면 표면 부착 (검증)': 'Attach plane surface (validated)',
  '구 표면 부착 (검증)': 'Attach sphere surface (validated)',
  '원통 표면 부착 (검증)': 'Attach cylinder surface (validated)',
  '원뿔 표면 부착 (검증)': 'Attach cone surface (validated)',
  '토러스 표면 부착 (검증)': 'Attach torus surface (validated)',
  '엣지 곡선 정보': 'Edge curve info',
  '면 표면 정보': 'Face surface info',
  '면 법선 캐시 조회': 'Face normal cache lookup',
  '엣지 폴리라인 캐시 조회': 'Edge polyline cache lookup',
  '캐시 통계': 'Cache statistics',

  // ── edit ──
  '되돌리기 (Undo)': 'Undo',
  '다시실행 (Redo)': 'Redo',
  '붙여': 'Paste',
  '전체 선택': 'Select all',
  '해제': 'Deselect',
  '동일 항목 선택': 'Select same',
  '동일': 'Same',
  '모두': 'All',
  '이름': 'Name',
  '이름 변경': 'Rename',
  '종료': 'Exit',
  '자동': 'Auto',
  '재개': 'Resume',
  '가시': 'Visible',
  '잠금': 'Lock',

  // ── groups ──
  '그룹': 'Group',
  '그룹 편집 모드': 'Group edit mode',
  '그룹 가시성 토글': 'Toggle group visibility',
  '그룹 잠금 토글': 'Toggle group lock',
  '컴포넌트 생성': 'Make component',
  '컴포': 'Comp.',

  // ── view ──
  '3D 뷰': '3D view',
  '평면도 (Top)': 'Top',
  '저면도': 'Bottom',
  '정면': 'Front',
  '정면도': 'Front',
  '배면': 'Back',
  '배면도': 'Back',
  '좌': 'Left',
  '좌측면도': 'Left',
  '우': 'Right',
  '우측면도': 'Right',
  '홈 뷰': 'Home view',
  '그리드 토글': 'Toggle grid',
  '축 표시 토글': 'Toggle axes',
  '축': 'Axes',
  'SSAO 토글': 'Toggle SSAO',
  '그림자 PRO': 'Shadows PRO',
  '퍼 렌더': 'Fur render',
  '퍼(fur) 렌더 토글': 'Toggle fur rendering',
  '재질 뷰': 'Material view',
  '단면 · X': 'Section · X',
  '단면 · Y': 'Section · Y',
  '단면 · Z': 'Section · Z',
  '단면 OFF': 'Section off',
  '태양': 'Sun',
  '태양 패널': 'Sun panel',
  '태양 히트맵': 'Sun heatmap',
  '태양 히트맵 OFF': 'Sun heatmap off',
  '간섭 검사': 'Clash detection',
  '간섭 표시 제거': 'Clear clash display',
  '참조 이미지 추가': 'Add a reference image',
  '텍스처 이미지 업로드': 'Upload a texture image',
  '🖼️ 텍스처 업로드…': '🖼️ Upload texture…',

  // ── sketch ──
  '스케치 시작 · XZ': 'Start sketch · XZ',
  '스케치 시작 · XY': 'Start sketch · XY',
  '스케치 시작 · YZ': 'Start sketch · YZ',
  '스케치 시작 · 자동': 'Start sketch · auto',
  '✏️ 스케치 시작 · XZ 바닥': '✏️ Start sketch · XZ ground',
  '✏️ 스케치 시작 · XY 정면': '✏️ Start sketch · XY front',
  '✨ 스케치 시작 · 자동 평면': '✨ Start sketch · auto plane',
  '↩ 스케치 재개': '↩ Resume sketch',
  '↻ up 카메라 정렬': '↻ Align camera to up',
  '평면 초기화': 'Reset the plane',

  // ── snap ──
  '축 스냅': 'Axis snap',
  '엣지 스냅': 'Edge snap',
  '스냅 오버라이드': 'Snap override',
  'OSNAP 패널': 'OSNAP panel',

  // ── measure / dimensions ──
  '측정': 'Measure',
  '측정 (Measure)': 'Measure',
  '측정 도구': 'Measure tool',
  '선택 측정': 'Measure selection',
  '선택 치수 토글': 'Toggle dimensions on the selection',
  '선형 치수': 'Linear dimension',
  '선형 치수 (Linear Dimension · 영구·편집)': 'Linear dimension (persistent · editable)',
  '각도 치수': 'Angular dimension',
  '각도 치수 (Angular Dimension · 영구·편집)': 'Angular dimension (persistent · editable)',
  '각도치수': 'Ang. dim',
  '반지름 치수': 'Radial dimension',
  '반지름 치수 (Radial Dimension · 원/호 · 영구·편집)':
    'Radial dimension (circle/arc · persistent · editable)',
  '반지름치수': 'Rad. dim',
  '참조 치수': 'Reference dimension',
  '참조 치수 (Reference Dimension · 읽기전용)': 'Reference dimension (read-only)',
  '참조치수': 'Ref. dim',

  // ── constraints ──
  '평행 (Parallel)': 'Parallel',
  '평행 정렬': 'Make parallel',
  '수직 (Perpendicular)': 'Perpendicular',
  '수직 정렬': 'Make perpendicular',
  '동일 선상 (Collinear)': 'Collinear',
  '동일 선상 정렬': 'Make collinear',
  '엣지 길이': 'Edge length',
  '엣지 길이 고정': 'Fix edge length',
  '끝점 거리 고정': 'Fix endpoint distance',
  '두 점 거리 고정': 'Fix the distance between 2 points',
  '제약 패널': 'Constraints panel',

  // ── panels ──
  'XIA 인스펙터': 'XIA inspector',
  '컴포넌트 패널': 'Components panel',
  '작업 기록 패널': 'History panel',
  '장면 패널': 'Scenes panel',
  '불변식 검증기': 'Invariant verifier',
  '감사 로그 뷰어': 'Audit log viewer',
  '분석 호버 오버레이': 'Analytic hover overlay',

  // ── file ──
  '새 파일': 'New file',
  '모든 형식': 'All formats',
  '가져오기 (Import)…': 'Import…',
  '내보내기 (Export)…': 'Export…',
  'DXF 가져오기': 'Import DXF',
  'DWG 가져오기': 'Import DWG',
  'OBJ 가져오기': 'Import OBJ',
  'STL 가져오기': 'Import STL',
  'glTF 가져오기': 'Import glTF',
  'DAE 가져오기': 'Import DAE',
  'PLY 가져오기': 'Import PLY',
  '3DS 가져오기': 'Import 3DS',
  '3DM 가져오기': 'Import 3DM',
  'IFC 가져오기': 'Import IFC',
  'STEP 가져오기': 'Import STEP',
  'IGES 가져오기': 'Import IGES',
  'SketchUp 가져오기': 'Import SketchUp',
  'DXF 내보내기': 'Export DXF',
  'OBJ 내보내기': 'Export OBJ',
  'STL 내보내기': 'Export STL',
  'glTF 내보내기': 'Export glTF',
  'STEP 내보내기': 'Export STEP',
  'IGES 내보내기': 'Export IGES',

  // ══════════════════════════════════════════════════════════════════════
  // batch 3b — the surfaces you touch while modelling: the VCB (every draw),
  // the status bar (always), the inspector (every selection) and the right-
  // click menu. The hidden panels can wait; these cannot.
  // ══════════════════════════════════════════════════════════════════════

  // ── VCB (value control box) ──
  // The label is a prompt for what to type, so it keeps its colon.
  '오프셋 거리:': 'Offset distance:',
  '포켓 — 여유(inset), 깊이:': 'Pocket — inset, depth:',
  '돌출 거리 (,각도° = 테이퍼 / ,비율% = 콘):':
    'Extrude distance (,angle° = taper / ,ratio% = cone):',
  '길이:': 'Length:',
  '가로, 세로:': 'Width, height:',
  '반지름:': 'Radius:',
  '이동 거리:': 'Move distance:',
  '각도(°):': 'Angle (°):',
  '배율:': 'Scale:',
  '치수:': 'Dimension:',
  '가로, 세로 ({unit})': 'Width, height ({unit})',
  '여유, 깊이 ({unit})': 'Inset, depth ({unit})',
  '숫자 입력 후 Enter ({unit})': 'Type a number, then Enter ({unit})',

  // ── Status bar ──
  // Split into whole sentences rather than 그리드 {state}: word order differs
  // per language, so a slot-filled fragment does not survive translation.
  'XIA가 선택되지 않았습니다': 'No XIA is selected',
  '그리드 숨김': 'Grid hidden',
  '엣지 숨김': 'Edges hidden',
  '축 숨김': 'Axes hidden',
  '뷰 원점 복귀': 'View reset',
  '전체화면을 지원하지 않습니다': 'Fullscreen is not supported here',
  '정밀도 (소수점)': 'Precision (decimals)',

  // ── XIA Inspector ──
  '곡면 파라미터 (직접 편집)': 'Surface parameters (edit directly)',
  '반지름 (mm)': 'Radius (mm)',
  '높이 (mm)': 'Height (mm)',
  '밑면 반지름 (mm)': 'Base radius (mm)',
  '주 반지름 (mm)': 'Major radius (mm)',
  '부 반지름 (mm)': 'Minor radius (mm)',
  '재질 제거됨 — 형태로 강등': 'Material removed — demoted to a Shape',
  '{n}개 객체 재질 제거됨 — 형태로 강등':
    'Material removed from {n} objects — demoted to Shapes',
  '재질 제거 시 {n}건 강등 실패 (나머지는 적용됨)':
    '{n} could not be demoted when the material was removed (the rest were)',
  '되돌리기': 'Undo',
  '{n}개 선분': '{n} segments',
  '□ 선': '□ Line',
  '{label} {n}개': '{label} × {n}',
  '객체': 'object',

  // ── Capability Explorer chrome ──
  '{tier} 작업: {label}': '{tier} action: {label}',
  '실행하시겠습니까?': 'Run it?',
  '검색 (id / label / description)': 'Search (id / label / description)',
  '검색 결과가 없습니다.': 'No matches.',
  '기존 UI 도구로 실행 (Launch 버튼 사용).': 'Run it with the existing UI tool (use the Launch button).',
  '복합 인자가 필요합니다. 코드 / MCP 호출 권장. (Capability Explorer pilot 외)':
    'This needs composite arguments — call it from code or over MCP. (Outside the Capability Explorer pilot.)',
  ' (변경)': ' (modifies)',
  'onActionInvoke 콜백이 등록되지 않았습니다 (main.ts wire 필요).':
    'No onActionInvoke callback is registered (main.ts needs to wire it).',

  // ══════════════════════════════════════════════════════════════════════
  // batch 5 — what the tools say back. These are the strings a user reads
  // most: one lands after almost every action. They are instructions and
  // outcomes, so they read as sentences, not labels.
  // ══════════════════════════════════════════════════════════════════════

  // ── Draw ──
  '유효한 길이를 입력하세요': 'Enter a valid length',
  '비평면 루프 — 면이 자동 생성되지 않을 수 있습니다':
    'The loop is not flat — a face may not be created',
  '루프 닫기 실행 (면 분할이 아닌 새 경계 생성)':
    'Closing the loop (this makes a new boundary, it does not split a face)',
  '루프 닫힘 — 면 생성됨': 'Loop closed — face created',
  '면 분할됨 — 계속 그리기 (Esc 종료)': 'Face split — keep drawing (Esc to finish)',
  '루프 닫힘 — 면 생성 실패 (비평면 또는 자체교차)':
    'Loop closed, but no face — it is not flat, or it crosses itself',
  '곡면 위 직선은 평면 보조선입니다. 곡면을 나누려면 자유곡선·베지어(구·원뿔) 또는 닫힌 원(원통·토러스)을 쓰세요.':
    'A straight line on a curved surface is only a flat guide. To split one, use a freehand or Bezier curve (sphere, cone) or a closed circle (cylinder, torus).',
  '⚠ 닫힘 세그먼트가 기존 체인과 교차합니다': '⚠ The closing segment crosses the chain',
  '이 곡면에는 그 반지름으로 원을 그릴 수 없습니다 — 마우스로 지정해 주세요':
    'A circle of that radius does not fit on this surface — draw it with the mouse instead',

  // ── Push/Pull ──
  '이 면의 법선을 계산할 수 없습니다 (degenerate)':
    'This face has no usable normal — it is degenerate',
  '곡면 포켓을 파냈습니다': 'Pocket carved into the curved surface',
  '곡면 보스를 세웠습니다': 'Boss raised on the curved surface',
  '포켓(pocket)을 파냈습니다': 'Pocket carved',
  '테이퍼(draft) 돌출은 단일 평면 프로파일만 지원합니다 (곡면/그룹 미지원)':
    'Draft extrude works on a single flat profile only (not curved surfaces or groups)',
  '콘(cone) 돌출은 단일 평면 원 프로파일만 지원합니다 (곡면/그룹 미지원)':
    'Cone extrude works on a single flat circle only (not curved surfaces or groups)',
  '양방향 돌출은 단일 평면 프로파일만 지원합니다 (곡면/그룹 미지원)':
    'Two-way extrude works on a single flat profile only (not curved surfaces or groups)',

  // ── Move / Rotate / Scale / Copy ──
  '이동할 면/에지를 선택하거나 정점을 클릭하세요':
    'Select a face or edge to move, or click a vertex',
  '도착점을 클릭하세요 (Esc: 취소)': 'Click where it should go (Esc to cancel)',
  '복제/붙여넣기 취소': 'Duplicate / paste cancelled',
  '이동할 면 또는 에지를 먼저 선택하세요': 'Select a face or edge to move first',
  '회전할 면 또는 에지를 먼저 선택하세요': 'Select a face or edge to rotate first',
  '참조점이 기준점과 너무 가까움': 'The reference point is too close to the base point',
  '③ 목표 방향 클릭 또는 각도 입력': '③ Click the target direction, or type an angle',
  '회전 취소됨': 'Rotate cancelled',
  '기준점·참조점을 먼저 클릭한 뒤 각도를 입력하세요':
    'Click the base and reference points first, then type an angle',
  '크기 조정할 면 또는 에지를 먼저 선택하세요': 'Select a face or edge to scale first',
  '스케일 값이 0이면 면이 퇴화됩니다 (거부)':
    'A scale of 0 would collapse the face — refused',
  '복제할 면 또는 엣지를 먼저 선택하세요': 'Select a face or edge to duplicate first',

  // ── Erase / Offset ──
  '삭제에 실패했습니다': 'Could not delete that',
  '엣지 offset: 거리(VCB)를 입력하세요. ESC 로 취소.':
    'Edge offset: type a distance. Esc to cancel.',
  'Offset 적용할 엣지가 없습니다.': 'There are no edges to offset.',

  // ── Group ──
  '그룹에 포함할 면들을 선택하세요': 'Select the faces to put in the group',
  '그룹 편집 모드 종료': 'Left group edit mode',
  '그룹을 만들려면 2개 이상의 면을 선택하세요': 'Select at least 2 faces to make a group',
  '그룹 생성 실패': 'Could not create the group',
  '해제할 그룹을 선택하세요': 'Select a group to ungroup',
  '그룹 해제됨': 'Ungrouped',

  // ── Fillet / Chamfer / Join / Trim / Extend ──
  '둥글릴 엣지를 선택하고 반지름을 입력하세요 (또는 클릭 = 마지막 값), Esc 종료':
    'Select an edge and type a radius (or just click to reuse the last one). Esc to finish.',
  '둥글릴 엣지를 먼저 선택하세요': 'Select an edge to fillet first',
  '챔퍼할 꼭짓점 위를 클릭하세요': 'Click a vertex to chamfer',
  '반지름을 입력하세요 (또는 다시 클릭 = 마지막 값)':
    'Type a radius (or click again to reuse the last one)',
  '병합할 일직선 꼭짓점을 클릭하세요 (두 직선 → 하나)':
    'Click a collinear vertex to join (two straight lines → one)',
  '병합할 꼭짓점 위를 클릭하세요': 'Click a vertex to join',
  '조인 완료': 'Joined',
  '잘라낼 선 구간을 클릭하세요 (교차점 사이가 한 구간 · Esc 종료)':
    'Click the segment to trim (a segment runs between intersections). Esc to finish.',
  '잘라낼 선 구간을 클릭하세요': 'Click the segment to trim',
  '선 구간 자르기 완료': 'Trimmed',
  '늘일 기준(경계) 엣지를 먼저 선택한 뒤, 늘일 엣지를 클릭하세요 (Esc 종료)':
    'Select the boundary edge to extend to, then click the edge to extend. Esc to finish.',
  '늘일 기준이 될 경계 엣지를 먼저 선택하세요': 'Select the boundary edge to extend to first',
  '늘일 엣지를 클릭하세요': 'Click the edge to extend',
  '경계 엣지 자신은 늘일 수 없습니다': 'The boundary edge cannot extend to itself',
  '엣지 늘이기 완료': 'Extended',

  // ── Box / Recess ──
  '박스의 가로/세로 코너를 다른 위치에 클릭하세요': 'Click the opposite corner of the base',
  '높이가 0 입니다 — 위/아래로 이동 후 다시 클릭':
    'The height is 0 — move up or down, then click again',
  '박스 도구 취소': 'Box cancelled',
  '박스 생성 실패: ': 'Could not create the box: ',
  '포켓: 면을 클릭하세요.': 'Pocket: click a face.',
  '포켓 취소됨': 'Pocket cancelled',
  '먼저 면을 클릭하세요.': 'Click a face first.',
  '포켓은 두 값이 필요합니다 — "여유 깊이" (예: 20 100).':
    'A pocket needs two values — "inset depth" (e.g. 20 100).',
  '여유(inset)와 깊이(depth)는 0보다 커야 합니다.':
    'The inset and the depth must both be greater than 0.',
  '포켓: VCB에 "여유 깊이" 입력 (예: 20 100). ESC 로 취소.':
    'Pocket: type "inset depth" (e.g. 20 100). Esc to cancel.',

  // ── batch 5, part 2 — what the mechanical wrap could not see ──
  // The Toast.x('…') sweep missed these: engine-error humanisers that return a
  // string, ternaries, fragments concatenated into a label, and a data table.
  // The Korean-literal guard found every one.

  // DrawLineTool.friendlyErrorMessage — same job as humanizeEngineError:
  // turn an engine error into what to do instead.
  '분할선이 너무 짧습니다 (시작점과 끝점을 더 떨어뜨리세요)':
    'The split line is too short — move the start and end further apart',
  '이미 이어진 모서리 위의 두 점은 분할에 사용할 수 없습니다 — 반대쪽 모서리나 면 안쪽을 끝점으로 하세요':
    'Two points on an edge that already joins them cannot split a face — end on the opposite edge, or inside the face',
  '분할 좌표가 유효하지 않습니다 (NaN/Infinity) — 스냅을 확인하세요':
    'The split coordinates are not valid (NaN/Infinity) — check the snap',
  '대상 면을 찾을 수 없습니다 (이미 삭제되었거나 선택 해제됨)':
    'That face is gone — it was deleted or deselected',
  '시작점과 끝점이 같은 정점입니다': 'The start and end are the same vertex',
  '분할선 위치를 경계에서 찾지 못했습니다 — 면 가장자리 근처에서 다시 시도하세요':
    'The split line does not meet the boundary — try again nearer the face edge',
  '면 경계 위에 분할 끝점을 놓아주세요': 'Put the split endpoints on the face boundary',

  // Dimension-label fragments. Short by design — they sit next to a number.
  'X축': 'X axis',
  'Y축(높이)': 'Y axis (height)',
  'Z축': 'Z axis',
  '참조': 'ref',
  ' 관통': ' through',
  '복제 {d}': 'copy {d}',

  '이 곡면에는 원을 그릴 수 없습니다': 'A circle cannot be drawn on this surface',
  '이 곡면에는 포켓을 만들 수 없습니다 — 곡면에 원을 그린 뒤 안쪽으로 밀어 보세요':
    'A pocket cannot be made in this surface — draw a circle on it, then push inwards',
  '이 곡면에는 보스를 세울 수 없습니다 — 곡면에 원을 그린 뒤 바깥쪽으로 밀어 보세요':
    'A boss cannot be raised on this surface — draw a circle on it, then push outwards',
  '이 위치에는 구멍/포켓을 만들 수 없습니다 — 위치를 옮겨 보세요':
    'A hole or pocket will not fit here — try another spot',
  '돌출/잘라내기가 실행되지 않았습니다': 'Nothing was extruded or cut',

  '이동이 자기교차/무효 형상을 만들어 취소되었습니다':
    'That move would have made the shape cross itself, so it was cancelled',
  '회전이 자기교차/무효 형상을 만들어 취소되었습니다':
    'That rotation would have made the shape cross itself, so it was cancelled',
  '스케일이 자기교차/무효 형상을 만들어 취소되었습니다':
    'That scale would have made the shape cross itself, so it was cancelled',
  '📐 복제본의 corner가 커서에 붙어 이동 → 클릭해 고정, Esc 취소':
    '📐 The copy\'s corner follows the cursor — click to place, Esc to cancel',
  '마우스로 위치 조정 → 클릭해 고정, Esc 취소':
    'Move it with the mouse — click to place, Esc to cancel',
  '복제 실패': 'Could not duplicate that',
  '엣지': 'edges',

  '(Shift: 강제 삭제)': '(Shift: force delete)',
  '솔리드 1개가 서피스로 전환됨 (닫힌 볼륨 해체)':
    '1 solid became a surface — the closed volume was opened',
  '선과 면을 동시에 선택했습니다. Offset 명령은 한 차원만 사용합니다 (선 또는 면).':
    'Lines and faces are both selected. Offset works on one at a time — lines or faces.',
  '필렛 실패 (3-way corner 등은 미지원)':
    'Could not fillet that (3-way corners are not supported)',
  '챔퍼 실패 (3-valence 꼭짓점만 가능)':
    'Could not chamfer that (only 3-valence vertices work)',
  '병합 실패 (일직선 2-valence 꼭짓점만 가능)':
    'Could not join that (only collinear 2-valence vertices work)',
  '자르기 실패 (자유 와이어 구간이 아님)':
    'Could not trim that (it is not a free wire segment)',
  '늘이기 실패 (경계에 닿지 않거나 자유 와이어 엣지가 아님)':
    'Could not extend that (it does not reach the boundary, or is not a free wire edge)',

  // ── help ──
  '도움말': 'Help',
  '단축키 보기': 'Keyboard shortcuts',
  '프로그램 정보': 'About',
};

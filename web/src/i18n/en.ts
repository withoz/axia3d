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
  '자르기 (Trim)': 'Trim',
  '연장 (Extend)': 'Extend',
  '분해 (Explode)': 'Explode',
  '면 반전': 'Flip face',
  '면 통합': 'Merge faces',
  '돌출/잘라내기 (Extrude/Cut · Volume)': 'Extrude / Cut (volume)',
  '홈파기 (Recess · Pocket · 면 클릭→여유 깊이)':
    'Recess (pocket · click a face → depth)',
  '매끄럽게 분할 (Subdivide)': 'Subdivide (smooth)',
  '선 병합 (Join · 일직선)': 'Join lines (collinear)',
  '엣지 모깎기 (Fillet)…': 'Fillet edge…',
  '엣지 모따기 (Chamfer)…': 'Chamfer edge…',
  '꼭짓점 모따기 (Vertex Chamfer)': 'Vertex chamfer',
  '코너 둥글리기 (Corner Fillet · 2D)': 'Corner fillet (2D)',
  '코너 모따기 (Corner Chamfer · 2D)': 'Corner chamfer (2D)',
  '모깎기 도구 (Fillet · 엣지+반지름 · 반복)': 'Fillet tool (edge + radius · repeatable)',
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
  '🧱 두께 부여 (Shell/Thicken)…': '🧱 Thicken / shell…',
  '🧱 두께 부여 (Thicken/Shell)…': '🧱 Thicken / shell…',
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
  '치수:': 'Dimensions:',
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
  '🧲 기하 병합 (크기 달라도 OK, 2면 선택)':
    '🧲 Geometric merge (different sizes OK, select 2 faces)',
  '🫧 강제 통합 (비평면, 내부 엣지 숨김)':
    '🫧 Force merge (non-planar, hides interior edges)',
  '🔄 면 재합성 (닫힌 라인 cycle → face)':
    '🔄 Rebuild faces (closed line cycle → face)',
  '자유 엣지 → 면 합성': 'Free edges → face',
  'XIA 내 Coplanar 면 일괄 통합': 'Merge coplanar faces within a XIA',
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
  '매끄럽게 분할 (Catmull-Clark)': 'Subdivide (Catmull-Clark)',
  'RotRect · 회전 사각형 (3-click)': 'RotRect · rotated rectangle (3-click)',
  'Sweep · 경로 따라 파이프 (W)': 'Sweep · pipe along a path (W)',
  'Loft · 단면 블렌드 화병': 'Loft · blend between sections',
  'Wall · 기준선 → 두께·높이 압출': 'Wall · baseline → extrude thickness & height',
  'Window · 벽 면에 사각 개구부': 'Window · rectangular opening in a wall face',
  'Recess · Pocket (홈파기)': 'Recess · pocket',
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
};

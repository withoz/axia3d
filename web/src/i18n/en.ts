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

  // ── Model / Modify menu group headers (ADR-046 P31 #4 reorg, 2026-07-18) ──
  '프리미티브': 'Primitives',
  '솔리드 도구': 'Solid Tools',
  '면 · 힐링': 'Surface · Heal',
  '배열': 'Array',
  '필렛 · 챔퍼': 'Fillet · Chamfer',
  '🛠️ 개발자 (Debug)': '🛠️ Developer (Debug)',

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
  '원점으로 (F5)': 'Home (F5)',
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
  // The hint used to say 'reloads the page' and stop there, which was true and
  // not the part that mattered. The scene is now carried across the reload
  // (i18n/localeSwitchScene), so what it has to say is that the repaint is not
  // a reset — and that the undo stack is the one thing that does not survive.
  '바꾸면 화면을 다시 불러옵니다 — 작업은 유지됩니다 (실행취소 기록은 초기화)':
    'Changing this reloads the page — your work is kept (undo history resets)',
  '언어를 바꿨습니다 — 작업은 그대로입니다 (실행취소 기록은 초기화)':
    'Language changed — your work is intact (undo history reset)',
  // Only when the scene genuinely cannot be carried: past the sessionStorage
  // ceiling, or storage disabled. The common path never asks.
  '언어를 바꾸면 화면을 다시 불러옵니다.\n작업이 너무 커서 유지할 수 없습니다. 계속할까요?':
    'Changing the language reloads the page.\nYour work is too large to carry across. Continue?',
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

  // ══════════════════════════════════════════════════════════════════════
  // batch 6 — what a menu or right-click action says back.
  //
  // The 표시/숨김 toggles are split into whole sentences here too: word order
  // is not universal, and '그리드 {state}' cannot be translated.
  // ══════════════════════════════════════════════════════════════════════

  // ── MenuBar ──
  '현재 작업을 초기화하시겠습니까?': 'Discard the current work and start over?',
  'DXF 내보내기에 실패했습니다': 'Could not export the DXF',
  'OBJ 내보내기에 실패했습니다': 'Could not export the OBJ',
  'glTF 내보내기에 실패했습니다': 'Could not export the glTF',
  'STL 내보내기에 실패했습니다': 'Could not export the STL',
  '유효한 숫자를 입력해주세요.': 'Please enter a valid number.',
  '주변광 차폐 켜짐': 'Ambient occlusion on',
  '주변광 차폐 꺼짐': 'Ambient occlusion off',
  '털 쉐이더 켜짐 (24 shell, 드로우콜 증가 주의)':
    'Fur shader on (24 shells — watch the draw calls)',
  '털 쉐이더 꺼짐': 'Fur shader off',
  '참조 이미지 불러옴 — Shift+휠로 크기, 드래그로 이동':
    'Reference image loaded — Shift+wheel to resize, drag to move',
  'Capability Explorer 를 사용할 수 없습니다.': 'The Capability Explorer is not available.',
  'Invariant Verifier 를 사용할 수 없습니다.': 'The Invariant Verifier is not available.',
  'Audit Log Viewer 를 사용할 수 없습니다.': 'The Audit Log Viewer is not available.',
  'Analytic Hover Overlay 를 사용할 수 없습니다.': 'The Analytic Hover Overlay is not available.',
  '컴포넌트 패널을 사용할 수 없습니다.': 'The components panel is not available.',
  '구속 조건 패널을 사용할 수 없습니다.': 'The constraints panel is not available.',
  'XIA 인스펙터를 사용할 수 없습니다.': 'The XIA inspector is not available.',
  '재질 편집은 XIA 인스펙터에서 수행하세요.': 'Edit materials in the XIA inspector.',
  '간섭 없음 ✓': 'No clashes ✓',
  '간섭 표시 해제': 'Clash display cleared',
  '텍스처 업로드 실패: ': 'Texture upload failed: ',
  '섹션 평면 해제됨': 'Section plane cleared',
  'AXiA 3D v0.1.0\n\n경량 3D 모델링 프로그램\nXIA Geometry Engine (Rust/WASM)':
    'AXiA 3D v0.1.0\n\nA lightweight 3D modeller\nXIA Geometry Engine (Rust/WASM)',

  // ── ContextMenu ──
  'Annulus: 정확히 2개의 면을 선택해야 합니다': 'Annulus: select exactly 2 faces',
  'Annulus 생성 완료': 'Annulus created',
  'T-junction 없음 (mesh 정상)': 'No T-junctions — the mesh is clean',
  'Coplanar 정리 대상 없음 (mesh 정상)': 'Nothing coplanar to clean up — the mesh is clean',
  '작업 평면 초기화 — 빈 공간은 바닥(z=0), 면 위는 그 면':
    'Work plane reset — empty space draws on the ground (z=0), a face draws on that face',
  '기본 평면으로 복귀': 'Back to the default plane',
  'Connected Inner Merge: container + ≥1 inner (총 ≥2 face) 선택 필요':
    'Connected inner merge: select a container plus at least 1 inner face (2 or more in total)',

  // ── BooleanHandler ──
  '모델과 교차: 먼저 면을 선택하세요': 'Intersect with model: select a face first',
  '알 수 없는 오류': 'Unknown error',
  '(상세 없음)': '(no details)',
  '명시 그룹': 'explicit groups',
  '자동 분할': 'auto-split',
  'Boolean 연산 실패: WASM 엔진이 준비되지 않았습니다':
    'Boolean failed: the engine is not ready',
  ' · 곡면 보존됨 (NURBS surface)': ' · curved surfaces preserved (NURBS)',

  // ── MergeActions ──
  // These are diagnostics: they say what failed AND what to try. The English
  // keeps both halves — dropping the "try this" is what makes an error useless.
  '통합하려면 2개 이상의 면 또는 1개의 엣지를 선택하세요':
    'To merge, select 2 or more faces, or 1 edge',
  '재질 경계 존중 모드: 같은 재질 면이 2개 이상 필요합니다':
    'Respecting material boundaries: 2 or more faces of the SAME material are needed',
  '통합할 수 있는 면이 없습니다.': 'There are no faces that can be merged.',
  '• 선택한 면들이 엣지를 공유하지 않습니다': '• The selected faces do not share an edge',
  '  (엣지가 공유되려면 snap으로 정확히 정점 매칭 필요)':
    '  (a shared edge needs vertices matched exactly, via snap)',
  '  → "🧲 기하 머지" 컨텍스트 메뉴로 폴리곤 재구성 시도 가능':
    '  → try "🧲 Geometric merge" from the right-click menu, which rebuilds the polygon',
  ' (mergetol 2 명령으로 허용치 확장 가능)':
    ' (the mergetol 2 command widens the tolerance)',
  '  → "강제 머지"(ADR-008 Axiom 9) 컨텍스트 메뉴로 내부 엣지만 숨기고 비평면 상태로 결합 가능':
    '  → "Force merge" (ADR-008 Axiom 9) joins them non-planar, hiding the interior edge',
  ' · 재질별': ' · by material',
  '통합할 수 있는 면이 없습니다.\n• 엣지를 공유하는 coplanar 면이 있어야 합니다\n• 두 면이 한 엣지만 공유해야 합니다 (C-slit 형태 불가)':
    'There are no faces that can be merged.\n• They must be coplanar and share an edge\n• They must share exactly ONE edge (a C-slit shape will not work)',
  '기하 머지는 2개 이상의 면을 선택해야 합니다. 현재: ':
    'Geometric merge needs 2 or more faces. Selected: ',
  '개': '',
  '기하 머지 실패 — 두 면이 같은 평면 & 경계가 겹치는지 확인 (tol ':
    'Geometric merge failed — check the faces are coplanar and their boundaries overlap (tol ',
  '강제 머지는 2개 이상의 면을 선택해야 합니다': 'Force merge needs 2 or more faces',
  '강제 머지 실패 — 선택된 면들이 엣지를 공유하지 않습니다. 인접한 면을 함께 선택해주세요.':
    'Force merge failed — the selected faces do not share an edge. Select adjacent faces.',
  '선택된 면이 속한 XIA를 찾을 수 없습니다. 먼저 XIA의 면을 하나 선택하세요.':
    'Cannot find the XIA that face belongs to. Select a face of the XIA first.',
  '이 XIA에는 병합할 면이 2개 이상 없습니다': 'This XIA does not have 2 or more faces to merge',
  '통합 실패': 'Merge failed',
  '정확히 2개의 면을 선택하세요 (바깥쪽 + 안쪽) · 참고: 새로 그린 내부 RECT는 자동으로 구멍이 됩니다':
    'Select exactly 2 faces (outer + inner) · Note: a new inner rectangle becomes a hole automatically',
  '면 경계 추출 실패': 'Could not read the face boundary',
  '내부 면을 구멍으로 병합 완료': 'The inner face is now a hole',
  '병합 실패 — 두 면이 같은 평면이고 하나가 다른 하나에 완전히 포함돼야 합니다':
    'Merge failed — the faces must be coplanar, with one fully inside the other',

  // ── KeyboardShortcuts ──
  '뷰 원점 · 기본 평면(z=0) 복귀': 'View reset · back to the default plane (z=0)',
  '🔒 추론 잠금': '🔒 Inference locked',
  '추론 잠금 해제': 'Inference unlocked',
  '이미 기본 평면 (빈 공간 = 바닥)': 'Already on the default plane (empty space = ground)',

  // ══════════════════════════════════════════════════════════════════════
  // batch 7 — the action dispatcher (ToolManagerRefactored). The biggest
  // single file: menu items and command-palette entries all report through
  // here. Log labels, prompts, and outcome toasts.
  // ══════════════════════════════════════════════════════════════════════

  // ── OperationLog / context-action labels ──
  'XIA 내 coplanar 면 일괄 통합': 'Merge coplanar faces within a XIA',
  '비평면 강제 머지 (내부 엣지 숨김)': 'Force merge, non-planar (hides the interior edge)',
  '내부 면을 구멍으로 합치기': 'Merge an inner face as a hole',
  '컴포넌트 변환': 'Convert to component',
  'X축 기준 미러 (YZ 평면)': 'Mirror about X (YZ plane)',
  'Y축 기준 미러 (XZ 평면)': 'Mirror about Y (XZ plane)',
  'Z축 기준 미러 (XY 평면)': 'Mirror about Z (XY plane)',
  '선택 엣지를 X축으로 회전 (Revolve)': 'Revolve the selected edges about X',
  '선택 엣지를 Y축으로 회전 (Revolve)': 'Revolve the selected edges about Y',
  '선택 엣지를 Z축으로 회전 (Revolve)': 'Revolve the selected edges about Z',
  '전체 메시 Catmull-Clark 분할': 'Catmull-Clark subdivide the whole mesh',
  '선택 엣지 필렛 (Fillet)': 'Fillet the selected edges',
  '선택 엣지 챔퍼 (Chamfer)': 'Chamfer the selected edges',
  '선택을 선형 배열로 복제': 'Duplicate the selection as a linear array',
  '선택을 원형 배열로 복제': 'Duplicate the selection as a radial array',
  '선택 면에 셸 (Shell/Thicken)': 'Shell / thicken the selected faces',
  '선택 면 2개를 로프트로 블렌드 (Loft)': 'Loft between the 2 selected faces',
  '선택 면을 축 기준 각도만큼 회전 (Revolve · 부분/360°)':
    'Revolve the selected face about an axis (partial or 360°)',
  '열린 쉘을 닫힌 솔리드로 변환 (Solidify)': 'Solidify — open shell to closed solid',
  '메시 정리 (퇴화면/와인딩/고립 정점)':
    'Clean the mesh (degenerate faces / winding / orphan vertices)',
  '경계 도구 (Boundary) — 닫힌 line cycle 명시 면 합성 (ADR-139)':
    'Boundary tool — build a face from a closed line cycle (ADR-139)',
  '스케치 시작 — XZ 바닥 평면': 'Start sketch — XZ ground plane',
  '스케치 시작 — XY 정면 평면': 'Start sketch — XY front plane',
  '스케치 시작 — YZ 측면 평면': 'Start sketch — YZ side plane',
  '스케치 시작 — 선택 면': 'Start sketch — selected face',
  '스케치 시작 — 자동 평면 감지': 'Start sketch — auto-detect plane',
  '스케치 up 방향을 카메라에 정렬': 'Align the sketch up direction to the camera',
  '마지막 스케치 평면 재진입': 'Re-enter the last sketch plane',
  '선택 엣지 → 중심선 변환': 'Selected edges → centerline',
  '선택 엣지 → 일반선 변환': 'Selected edges → normal line',
  '복사 (Ctrl+C)': 'Copy (Ctrl+C)',
  '잘라내기 (Ctrl+X)': 'Cut (Ctrl+X)',
  '붙여넣기 (Ctrl+V)': 'Paste (Ctrl+V)',
  '복제 (Ctrl+D)': 'Duplicate (Ctrl+D)',
  '선택 구부리기 (Bend)': 'Bend the selection',
  '선택 비틀기 (Twist)': 'Twist the selection',
  '선택 테이퍼 (Taper)': 'Taper the selection',
  '선택 면에 색상 지정': 'Set a colour on the selected faces',

  // ── outcome toasts + prompts ──
  '엣지 복사는 아직 미지원 — 면을 선택하세요': 'Copying edges is not supported yet — select a face',
  '복사할 항목이 선택되지 않음': 'Nothing is selected to copy',
  '복제할 면을 먼저 선택하세요': 'Select a face to duplicate first',
  '붙여넣을 내용이 없습니다 — 먼저 Ctrl+C로 복사하세요':
    'Nothing to paste — copy with Ctrl+C first',
  '반전할 면을 먼저 선택하세요': 'Select a face to flip first',
  'Sheet 면은 앞/뒷면 구분이 없어 반전할 필요가 없습니다 (ADR-007 Rev 2)':
    'A sheet face has no front or back, so there is nothing to flip (ADR-007 Rev 2)',
  '면 반전 실패': 'Could not flip the face',
  '미러링할 면을 먼저 선택하세요': 'Select a face to mirror first',
  '미러링 실패': 'Mirror failed',
  '회전시킬 엣지 체인을 먼저 선택하세요': 'Select an edge chain to revolve first',
  '선택된 엣지가 단순 체인이 아닙니다 (분기/단절). ':
    'The selected edges are not a simple chain (they branch or break). ',
  '연결된 폴리라인만 revolve 가능합니다.': 'Only a connected polyline can be revolved.',
  'Revolve 실패': 'Revolve failed',
  '변형할 면 또는 에지를 먼저 선택하세요': 'Select a face or edge to transform first',
  '선택에서 정점을 추출할 수 없습니다': 'Could not read vertices from the selection',
  '선택 범위가 너무 작습니다': 'The selection is too small',
  '구부리기 각도 (도, +/-):': 'Bend angle (degrees, +/-):',
  '유효한 숫자를 입력하세요': 'Enter a valid number',
  'Bend 실패': 'Bend failed',
  '비틀기 각도 (축 전체에 대해 총 도수):': 'Twist angle (total degrees over the axis):',
  'Twist 실패': 'Twist failed',
  '끝 스케일 (0보다 큰 실수, 1.0 = 원래 크기):': 'End scale (a number above 0, 1.0 = original):',
  '유효한 양수 스케일을 입력하세요': 'Enter a valid positive scale',
  'Taper 실패': 'Taper failed',
  '배열할 면을 먼저 선택하세요': 'Select a face to array first',
  '배열 파라미터 "N, dx, dy, dz" (개수, X 오프셋, Y 오프셋, Z 오프셋):':
    'Array parameters "N, dx, dy, dz" (count, X offset, Y offset, Z offset):',
  '4개 값이 필요합니다: N,dx,dy,dz': '4 values are needed: N, dx, dy, dz',
  '유효한 숫자 값을 입력하세요': 'Enter valid numbers',
  '배열 실패': 'Array failed',
  '두께를 부여할 면을 먼저 선택하세요': 'Select a face to thicken first',
  '0이 아닌 유효한 숫자를 입력하세요': 'Enter a valid non-zero number',
  '모든 면에서 push_pull 실패': 'Push/pull failed on every face',
  '두 면을 로프트로 블렌드했습니다 (Loft)': 'Lofted between the 2 faces',
  '프로파일이 평면 폴리곤(≥3 verts)인지 확인하세요':
    'Check the profile is a flat polygon (3 or more vertices)',
  '회전 각도 (도, 1~360):': 'Revolve angle (degrees, 1–360):',
  '1~360 사이 각도를 입력하세요': 'Enter an angle between 1 and 360',
  '회전축 (X / Y / Z, 면 평면이 축을 포함해야 함):':
    'Revolve axis (X / Y / Z — the face plane must contain the axis):',
  '축은 X, Y, Z 중 하나여야 합니다': 'The axis must be X, Y or Z',
  '면 평면이 축(원점 통과)을 포함하고 축에서 떨어져 있는지 확인하세요':
    'Check the face plane contains the axis (through the origin) and stands off it',
  'XZ 바닥': 'XZ ground',
  'XY 정면': 'XY front',
  'YZ 측면': 'YZ side',
  '스케치 기준 면 1개를 선택하세요': 'Select 1 face to sketch on',
  '선택 면의 경계를 읽을 수 없습니다': 'Could not read the boundary of the selected face',
  '선택 면이 퇴화되어 스케치 평면을 계산할 수 없습니다':
    'The selected face is degenerate — a sketch plane cannot be computed',
  '이전에 사용한 스케치 평면 정보가 없습니다': 'There is no previous sketch plane to reuse',
  '변환할 엣지를 먼저 선택하세요': 'Select an edge to convert first',
  '일반선': 'normal line',
  '원인 불명': 'unknown cause',
  '활성 스케치 세션이 없습니다': 'There is no active sketch session',
  '유효한 양/음수 높이를 입력하세요 — 면은 이미 생성됨':
    'Enter a valid height, positive or negative — the face already exists',
  '면 ID 조회 실패 — 수동으로 Push/Pull하세요': 'Could not look up the face id — push/pull manually',
  '변경 없음': 'No change',
  '재합성할 닫힌 라인 cycle 이 없습니다': 'There is no closed line cycle to rebuild',
  '솔리드화할 메시가 없습니다 (활성 face 0개)': 'There is no mesh to solidify (0 active faces)',
  '원형 배열 파라미터 "N, axis(x|y|z), 총각도°" (예: 6, y, 360):':
    'Radial array parameters "N, axis (x|y|z), total angle°" (e.g. 6, y, 360):',
  '3개 값이 필요합니다: N, axis, 총각도°': '3 values are needed: N, axis, total angle°',
  '축은 x / y / z 중 하나여야 합니다': 'The axis must be x, y or z',
  '원형 배열 실패': 'Radial array failed',
  '색상을 지정할 면을 먼저 선택하세요': 'Select a face to colour first',
  '색상 적용 실패': 'Could not apply the colour',
  '챔퍼할 엣지 1개를 먼저 선택하세요': 'Select 1 edge to chamfer first',
  '챔퍼 거리 (mm):': 'Chamfer distance (mm):',
  '유효한 양수 거리를 입력하세요': 'Enter a valid positive distance',
  '챔퍼 실패': 'Chamfer failed',
  '필렛할 엣지를 1개 이상 선택하세요': 'Select 1 or more edges to fillet',
  '유효한 양수 반경을 입력하세요': 'Enter a valid positive radius',
  'Subdivision 실패': 'Subdivision failed',
  '자유 엣지가 없습니다 (모든 엣지가 이미 면에 속함)':
    'There are no free edges (every edge already belongs to a face)',
  '엣지가 실제로 닫혀 있는지 확인해 주세요.': 'Check the edges actually form a closed loop.',
  '1개의 엣지를 선택해야 합니다': 'Select exactly 1 edge',
  '엣지 엔드포인트 조회 실패': 'Could not look up the edge endpoints',
  '엣지 좌표 조회 실패': 'Could not look up the edge coordinates',
  '엣지 분할 실패': 'Could not split the edge',
  '유효한 양수 값을 입력하세요': 'Enter a valid positive value',
  '엣지 길이 제약 생성 실패': 'Could not create the edge-length constraint',
  '2개의 엣지를 선택해야 합니다': 'Select exactly 2 edges',
  '정점 좌표 조회 실패': 'Could not look up the vertex coordinates',
  '제약 생성 실패': 'Could not create the constraint',
  '2개의 엣지를 선택해야 합니다 (첫 번째 = 기준, 두 번째 = 이동 대상)':
    'Select exactly 2 edges (first = reference, second = the one to move)',
  '수직': 'perpendicular',
  '동일 선상': 'collinear',
  '유효한 반경 필요': 'A valid radius is required',
  '재실행할 엣지를 선택하세요': 'Select an edge to redo',
  '재실행 실패': 'Redo failed',
  '유효한 거리 필요': 'A valid distance is required',
  '1개 엣지 선택 필요': 'Select exactly 1 edge',
  '0이 아닌 두께 필요': 'A non-zero thickness is required',
  '재실행할 면을 선택하세요': 'Select a face to redo',
  '두께 {mm}mm × {n}개 면 (재실행)': 'thickness {mm}mm × {n} faces (redo)',
  '"N, dx, dy, dz" 4개 값 필요': '"N, dx, dy, dz" needs 4 values',
  '유효한 숫자 필요': 'Valid numbers are required',
  '"N, axis, deg" 3개 값 필요': '"N, axis, deg" needs 3 values',
  '축은 x/y/z 중 하나': 'The axis must be x, y or z',
  '스케치 모드가 아닙니다': 'Not in sketch mode',
  '카메라가 스케치 평면에 직각 — 정렬 불가':
    'The camera is perpendicular to the sketch plane — cannot align',
  '스케치 up 방향을 카메라에 정렬했습니다': 'Aligned the sketch up direction to the camera',
  '자유': 'free',
  'Home 또는 우클릭 → 기본 평면으로 (평면 초기화)':
    'Home, or right-click → back to the default plane (reset)',
  '스케치': 'sketch',
  '마지막': 'last',
  '알 수 없는 명령입니다: {action}': 'Unknown command: {action}',

  // ── ToolManagerRefactored — the {param} outcome toasts ──
  "도구 작업 중에는 '{name}' 실행 불가 — Esc 또는 Space로 먼저 완료하세요":
    "'{name}' cannot run while a tool is busy — finish it with Esc or Space first",
  '치수 표시: 켜짐': 'Dimensions: on',
  '치수 표시: 꺼짐': 'Dimensions: off',
  '{n}개 면 {verb} — Ctrl+V로 붙여넣기': '{verb} {n} faces — paste with Ctrl+V',
  '{n}개 sheet 면 건너뜀 (Wall 면만 반전)': 'Skipped {n} sheet faces (only wall faces flip)',
  '{n}개 면 반전됨': 'Flipped {n} faces',
  '{n}개 면을 {label} 평면 기준 미러링 ({m}개 생성)':
    'Mirrored {n} faces about the {label} plane ({m} created)',
  '{n} point profile → {axis} 축 revolve ({m} faces)':
    '{n}-point profile → revolved about {axis} ({m} faces)',
  '{n}개 정점을 {deg}° 구부림': 'Bent {n} vertices by {deg}°',
  '{n}개 정점을 총 {deg}° 비틈': 'Twisted {n} vertices by {deg}° total',
  '{n}개 정점을 ×{s} 테이퍼': 'Tapered {n} vertices ×{s}',
  '{n}개 면을 {count}회 복제 (총 {m}개)': 'Duplicated {n} faces ×{count} ({m} total)',
  '{n}개 면에 두께 {mm}mm 부여': 'Thickened {n} faces by {mm}mm',
  '셸 실패: {reason}': 'Shell failed: {reason}',
  '회전체는 프로파일 면 1개를 선택하세요 (현재 {n}개)':
    'Revolve needs exactly 1 profile face (currently {n})',
  '회전체 생성 — {deg}° around {axis} (Revolve)': 'Revolved — {deg}° around {axis}',
  '✏️ 스케치 시작 — 면 #{id}. 모든 드로잉이 이 평면에 고정됩니다.':
    '✏️ Sketch started — face #{id}. Every drawing pins to this plane.',
  '✏️ 스케치 재개 — {label}': '✏️ Sketch resumed — {label}',
  '스케치 재개 실패: {reason}': 'Could not resume the sketch: {reason}',
  '{n}개 엣지 → {label} 변환 완료': 'Converted {n} edges → {label}',
  '변환 실패: {reason}': 'Conversion failed: {reason}',
  '{n}개 면을 {count}회 원형 복제 ({axis}축, {deg}°)':
    'Radially duplicated {n} faces ×{count} ({axis} axis, {deg}°)',
  '{n}개 면에 {hex} 색상 적용': 'Applied {hex} to {n} faces',
  '챔퍼 완료 — 거리 {mm}mm': 'Chamfered — distance {mm}mm',
  'Catmull-Clark 분할 완료 — {n}개 quad 생성': 'Catmull-Clark subdivided — {n} quads created',
  '{n}개 면 합성 완료 (자유 엣지 {total}개 중)':
    'Built {n} faces (from {total} free edges)',
  '엣지 중점 분할 → 새 vertex {id}': 'Split the edge at its midpoint → new vertex {id}',
  '엣지 길이 제약 추가 (id={id}, {mm} mm)': 'Added an edge-length constraint (id={id}, {mm} mm)',
  '끝점 거리 제약 추가 (id={id}, v{a}↔v{b} = {d})':
    'Added an endpoint-distance constraint (id={id}, v{a}↔v{b} = {d})',
  '{label} 제약 추가 (id={id}) — 이후 이동 시 자동 유지':
    'Added a {label} constraint (id={id}) — kept automatically on later moves',
  '재실행 지원 안 함: {kind}': 'Redo is not supported for: {kind}',

  // The plane badge — 🔒 for a locked plane, 📐 for the sticky one.
  '🔒 평면 잠금 ({axis})': '🔒 Plane locked ({axis})',
  '📐 평면: {src} ({axis})': '📐 Plane: {src} ({axis})',

  // ── help ──
  '도움말': 'Help',
  '단축키 보기': 'Keyboard shortcuts',
  '프로그램 정보': 'About',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '박스 크기가 너무 작습니다 ({w} × {d} × {absH})': 'Box is too small ({w} × {d} × {absH})',
  '박스 {w} × {h} × {d} mm 생성됨': 'Box created — {w} × {h} × {d} mm',
  '복제 완료 ({n}개 {kind})': 'Duplicated {n} {kind}',
  '면 분할 실패: {error} — 일반 선으로 그립니다': 'Face split failed: {error} — drawing a plain line instead',
  '면이 {count}개로 분할됨': 'Face split into {count}',
  '면 분할 중 오류: {err}': 'Error during face split: {err}',
  '{mergedCount}개 면 머지': '{mergedCount} faces merged',
  '{synthesizedCount}개 면 자동 생성': '{synthesizedCount} faces created automatically',
  '{cascadedFaces}개 면 삭제': '{cascadedFaces} faces deleted',
  '{cascadedEdges}개 엣지 삭제': '{cascadedEdges} edges deleted',
  '솔리드 {desolidifiedCount}개가 서피스로 전환됨 (닫힌 볼륨 해체)': '{desolidifiedCount} solids became surfaces (closed volume opened)',
  '{selected}개 면 선택됨 — Enter로 그룹 생성': '{selected} faces selected — press Enter to group',
  'Group-{groupId} 선택됨 — 더블클릭으로 편집': 'Group-{groupId} selected — double-click to edit',
  'Group-{groupId} 생성 ({selected}개 면)': 'Group-{groupId} created ({selected} faces)',
  'Group-{localGid} 생성 ({selected}개 면)': 'Group-{localGid} created ({selected} faces)',
  'Group-{groupId} 편집 모드 — ESC로 종료': 'Editing Group-{groupId} — press Esc to leave',
  '납작해진 면을 정리했습니다 (벽 {collapsed}개 제거)': 'Cleaned up flattened faces ({collapsed} walls removed)',
  ' · {axis}축': ' · {axis} axis',
  '{count}개: 호스트 면 ({kinds}) — V-β-γ 에서 활성됩니다': '{count}: host face ({kinds}) — coming in V-β-γ',
  '{count}개: 곡선 종류 ({kinds}) — V-β-β 에서 활성됩니다': '{count}: curve type ({kinds}) — coming in V-β-β',
  '{count}개: 자유 와이어 — V-δ 에서 활성됩니다': '{count}: free wire — coming in V-δ',
  '{count}개: 호스트 면이 모호합니다': '{count}: host face is ambiguous',
  '{count}개: hole 면 (multi-loop) 거부 (ADR-016)': '{count}: face with holes (multi-loop) rejected (ADR-016)',
  '{count}개: 거리가 너무 작습니다': '{count}: distance is too small',
  '{count}개: arc 평면이 호스트 면과 일치하지 않습니다': '{count}: arc plane does not match the host face',
  '{count}개: 반지름이 0 이하로 축소됩니다 (방향 반전 필요)': '{count}: radius would shrink to zero or less (reverse the direction)',
  '{count}개: 곡선이 호스트 면 ({kinds}) 위에 자연스럽게 놓이지 않습니다': '{count}: the curve does not lie naturally on the host face ({kinds})',
  '{count}개: 축 방향 위치가 호스트 범위를 벗어납니다': '{count}: axial position is outside the host range',
  '{count}개: 자유 와이어가 평면이 아닙니다 (V-δ-β 명시 평면 필요)': '{count}: free wire is not planar (V-δ-β needs an explicit plane)',
  '{count}개: 기준 평면을 찾을 수 없습니다 (단일 엣지 또는 직선) — V-δ-β 활성 시 명시 평면 입력 가능': '{count}: no reference plane found (single edge or straight line) — V-δ-β will let you enter one',
  '{count}개: WASM 미가용': '{count}: WASM unavailable',
  '{count}개: 기타 오류 ({reason})': '{count}: other error ({reason})',
  '엣지 offset 실패 — {parts}': 'Edge offset failed — {parts}',
  ' / {edges}개 실패': ' / {edges} failed',
  '직전 거리 반복: {dist} mm': 'Repeat last distance: {dist} mm',
  '돌출/잘라내기 실패: {err}': 'Extrude/cut failed: {err}',
  '돌출/잘라내기 거리가 너무 짧습니다 (최소 {MIN_COMMIT_DIST}mm)': 'Extrude/cut distance is too short (minimum {MIN_COMMIT_DIST}mm)',
  '두께 {clampLimit}mm 에서 멈췄습니다 — 관통하려면 면에 형상을 그린 뒤 미세요': 'Stopped at the {clampLimit}mm wall — to cut through, draw a shape on the face first, then push',
  '방향 반전 (Tab) — normal=({x}, {y}, {z})': 'Direction flipped (Tab) — normal=({x}, {y}, {z})',
  '곡면 돌출/잘라내기 실패: {err}': 'Curved extrude/cut failed: {err}',
  '① 기준점(회전 중심)을 클릭하세요 · 축: {axisLabel} (X/Y/Z로 변경)': '① Click the pivot (centre of rotation) · axis: {axisLabel} (X/Y/Z to change)',
  '② 참조 방향을 클릭하세요 ({axLabel}축 회전)': '② Click the reference direction (rotating about {axLabel})',
  '{targetDeg}° · {axLabel}축': '{targetDeg}° · {axLabel} axis',
  '축 전환 → {label}축 회전': 'Axis changed → rotating about {label}',
  '복제 실패 — 원본 면이 삭제되었거나 유효하지 않음': 'Duplicate failed — the source face was deleted or is invalid',
  '붙여넣기 실패 — 원본 면이 삭제되었거나 유효하지 않음': 'Paste failed — the source face was deleted or is invalid',
  '📏 엣지 {edges}개': '📏 {edges} edges',
  '합계: {total}': 'Total: {total}',
  '최단 {min} · 최장 {max}': 'Shortest {min} · longest {max}',
  '📐 면 {faces}개\n': '📐 {faces} faces\n',
  '면적 합: {total} {unit}²': 'Total area: {total} {unit}²',
  '\n최대 면: {max} {unit}²': '\nLargest face: {max} {unit}²',
  '🧊 전체 메시 부피\n': '🧊 Total mesh volume\n',
  '(닫힌 솔리드 기준, 열린 쉘은 근사치)': '(assumes a closed solid; open shells are approximate)',
  '선형 배열 {count}회, ({dx},{dy},{dz})': 'Linear array ×{count}, ({dx},{dy},{dz})',
  '두께 (mm, 양수=노멀 방향 / 음수=반대 방향) — 선택 {selFaces}개 면:': 'Thickness (mm, positive = along the normal / negative = against it) — {selFaces} faces selected:',
  '두께 {distance}mm × {success}개 면': 'Thickness {distance}mm × {success} faces',
  '{success}/{selFaces}개 면 성공 — 실패: {firstFailure}': '{success}/{selFaces} faces succeeded — failure: {firstFailure}',
  '로프트는 정확히 2개의 프로파일 면을 선택하세요 (현재 {lofts}개)': 'Loft needs exactly 2 profile faces ({lofts} selected)',
  '로프트 실패: {error}': 'Loft failed: {error}',
  '회전체 실패: {error}': 'Revolve failed: {error}',
  '✏️ 스케치 시작 — XZ 바닥 (Y=0). 모든 드로잉이 이 평면에 고정됩니다.': '✏️ Sketch started — XZ ground (Y=0). All drawing is locked to this plane.',
  '✏️ 스케치 시작 — XY 정면 (Z=0). 모든 드로잉이 이 평면에 고정됩니다.': '✏️ Sketch started — XY front (Z=0). All drawing is locked to this plane.',
  '✏️ 스케치 시작 — YZ 측면 (X=0). 모든 드로잉이 이 평면에 고정됩니다.': '✏️ Sketch started — YZ side (X=0). All drawing is locked to this plane.',
  '면 #{faceId}': 'Face #{faceId}',
  '{label} (재개)': '{label} (resumed)',
  '{ok}/{edges}개 변환 — 나머지 거부: {firstErr}': '{ok}/{edges} converted — rest rejected: {firstErr}',
  '스케치 종료 ({label}) — 자유 엣지 없음 (닫힌 프로필 미작성)': 'Sketch closed ({label}) — no free edges (no closed profile was drawn)',
  '스케치 종료 ({label}) — 자유 엣지 {freeBefore}개 있으나 닫힌 polygon 미감지. 선이 끝점에서 정확히 만났는지 확인하세요.': 'Sketch closed ({label}) — {freeBefore} free edges, but no closed polygon. Check that the lines meet exactly at their endpoints.',
  '✅ 스케치에서 {created}개 닫힌 프로필을 감지했습니다.\n': '✅ Found {created} closed profiles in the sketch.\n',
  '높이(mm)를 입력하면 즉시 Push/Pull로 3D 변환합니다.\n': 'Enter a height (mm) to push/pull them into 3D right away.\n',
  '(취소 = 면만 남기고 종료)': '(Cancel = keep the faces and stop)',
  '스케치 종료 ({label}) — {created}개 면 생성, 3D 변환 건너뜀': 'Sketch closed ({label}) — {created} faces created, 3D step skipped',
  '스케치 Extrude {height}mm × {ok}개 프로필': 'Sketch extrude {height}mm × {ok} profiles',
  '✅ 스케치 완료 — {created}개 프로필 → {ok}개 3D 매스 (높이 {height}mm)': '✅ Sketch done — {created} profiles → {ok} 3D masses (height {height}mm)',
  '{created}개 면은 생성되었으나 Push/Pull 실패. 수동으로 면 선택 후 P 키로 시도.': '{created} faces were created but push/pull failed. Select a face and press P to try manually.',
  '✅ 메시 상태 양호 — 정리할 항목 없음 (면 {faceCount}개)': '✅ Mesh is healthy — nothing to clean up ({faceCount} faces)',
  '퇴화면 {degenerateRemoved}개 제거': '{degenerateRemoved} degenerate faces removed',
  'winding {windingFlipped}개 뒤집음': '{windingFlipped} windings flipped',
  'normal {normalsRecomputed}개 재계산': '{normalsRecomputed} normals recomputed',
  '고립 vertex {isolatedVertsRemoved}개 제거': '{isolatedVertsRemoved} isolated vertices removed',
  '\n⚠️ 잔여 invariant 위반 {remainingViolations}개 — 수동 점검 필요': '\n⚠️ {remainingViolations} invariant violations remain — check them by hand',
  '🔄 면 재합성 — 새 면 {created}개 생성 ({ms}ms 시간 한도 도달, 남은 cycle 처리하려면 다시 실행)': '🔄 Faces rebuilt — {created} new faces ({ms}ms, hit the time limit; run again for the remaining cycles)',
  '🔄 면 재합성 — 새 면 {created}개 생성 ({ms}ms)': '🔄 Faces rebuilt — {created} new faces ({ms}ms)',
  '이미 닫힌 솔리드입니다 (면 {faceCount}개, 내부 엣지 {interiorEdgeCount}개)': 'Already a closed solid ({faceCount} faces, {interiorEdgeCount} interior edges)',
  'Non-manifold 엣지 {count}개 발견 — 3개 이상 면이 공유하는 엣지는 Solidify가 자동 수정할 수 없습니다.\n': 'Found {count} non-manifold edges — Solidify cannot fix an edge shared by 3 or more faces.\n',
  '먼저 Mesh Repair로 non-manifold를 해결한 뒤 다시 시도하세요.': 'Run Mesh Repair first to resolve them, then try again.',
  '경계 엣지가 없지만 닫힌 솔리드 판정 미충족(면 {faceCount}개 — 최소 4면 필요)': 'No boundary edges, but this is not a closed solid ({faceCount} faces — at least 4 are needed)',
  '✅ Solidify 성공 — {created}개 면 cap 생성, 총 {faceCount}면 닫힌 솔리드': '✅ Solidify succeeded — {created} caps created, {faceCount} faces total, closed solid',
  '일부 cap 생성({created}개) but 아직 열린 상태: boundary {boundary}개, non-manifold {nonManifold}개 남음.\n': 'Some caps created ({created}) but still open: {boundary} boundary, {nonManifold} non-manifold edges remain.\n',
  '복잡한 비평면 boundary는 수동 보정이 필요할 수 있음.': 'A complex non-planar boundary may need fixing by hand.',
  'Solidify 실패 — boundary {count}개가 닫힌 polygon을 이루지 않거나 비평면 루프일 수 있습니다.': 'Solidify failed — the {count} boundary edges do not form a closed polygon, or the loop is not planar.',
  '원형 배열 {count}회 · {axisChar}축 · {totalDeg}°': 'Radial array ×{count} · {axisChar} axis · {totalDeg}°',
  '색상 {hex}': 'Colour {hex}',
  '챔퍼 {distance}mm': 'Chamfer {distance}mm',
  '필렛 반경 (mm) — 선택 {edges}개 엣지:': 'Fillet radius (mm) — {edges} edges selected:',
  '필렛 {radius}mm × {successEdges}개 엣지': 'Fillet {radius}mm × {successEdges} edges',
  '필렛 완료 — {successEdges}개 엣지, {totalFaces}개 fillet face 생성': 'Fillet done — {successEdges} edges, {totalFaces} fillet faces created',
  '{successEdges}/{edges}개 성공 — {failed}개 실패 (공유 vertex 충돌 가능성: 첫 실패 "{firstError}")': '{successEdges}/{edges} succeeded — {failed} failed (possibly a shared-vertex clash; first failure: "{firstError}")',
  '{edges}개 엣지 모두 필렛 실패': 'Fillet failed on all {edges} edges',
  'Catmull-Clark 분할 ({count}개 quad)': 'Catmull-Clark subdivision ({count} quads)',
  '자유 엣지 {freeEdgeCount}개 발견했으나 닫힌 polygon 미감지.\n': 'Found {freeEdgeCount} free edges but no closed polygon.\n',
  '엣지 길이 (현재 {current} mm):': 'Edge length (currently {current} mm):',
  '정점 v{vA} ↔ v{vB} 거리 (현재 {current} mm):': 'Distance v{vA} ↔ v{vB} (currently {current} mm):',
  '{label} 제약 생성 실패': 'Could not create the {label} constraint',
  '필렛 {r}mm × {ok}개 엣지 (재실행)': 'Fillet {r}mm × {ok} edges (repeat)',
  '챔퍼 {d}mm (재실행)': 'Chamfer {d}mm (repeat)',
  '선형 배열 {count}회 (재실행)': 'Linear array ×{count} (repeat)',
  '원형 배열 {count}회 · {axisChar}축 · {totalDeg}° (재실행)': 'Radial array ×{count} · {axisChar} axis · {totalDeg}° (repeat)',
  'Catmull-Clark 분할 (재실행, {n}개 quad)': 'Catmull-Clark subdivision (repeat, {n} quads)',
  '엣지 양옆 면 머지 완료{tolNote}': 'Merged the faces either side of the edge{tolNote}',
  '해당 엣지 양옆의 두 면이 같은 평면이 아니거나 (현재 tol {tol}°), 경계가 모호합니다 (공유 엣지 1개 필요)': 'The two faces either side of that edge are not coplanar (tolerance {tol}°), or the boundary is ambiguous (exactly one shared edge is needed)',
  '기하 머지로 통합 완료 (snap 드리프트 보정 · tol {geoTol}°/mm)': 'Geometric merge done (snap drift corrected · tol {geoTol}°/mm)',
  '• {nonCoplanar}쌍이 평면 불일치{tolHint}': '• {nonCoplanar} pairs are not coplanar{tolHint}',
  '• {ambiguous}쌍이 C-slit 형태 (hole 필요 — 미지원)': '• {ambiguous} pairs form a C-slit (would need a hole — unsupported)',
  ' ({skipped}쌍 건너뜀)': ' ({skipped} pairs skipped)',
  '{merged}회 통합 — {workingFaces}개 면이 {workingFaces2}개로 합쳐짐{skipNote}{tolNote}{matNote}': 'Merged {merged} times — {workingFaces} faces became {workingFaces2}{skipNote}{tolNote}{matNote}',
  '기하 머지 {mergedCount}회 완료': 'Geometric merge done ×{mergedCount}',
  '{faces}개 면을 하나의 폴리곤 서피스로 결합 ({softened}개 내부 엣지 숨김)': '{faces} faces combined into one polygon surface ({softened} interior edges hidden)',
  'XIA {xiaId} — 병합 가능한 인접 coplanar 면이 없습니다': 'XIA {xiaId} — no adjacent coplanar faces to merge',
  ' (평면 불일치 {nonCoplanar}쌍)': ' ({nonCoplanar} pairs are not coplanar)',
  'XIA {xiaId} — {merged}회 통합, {xiaFaceIds}개 면 → {xiaFaceIds2}개': 'XIA {xiaId} — merged {merged} times, {xiaFaceIds} faces → {xiaFaceIds2}',
  'Boolean {op} 실패: 알 수 없는 오류': 'Boolean {op} failed: unknown error',
  'Boolean {op} — 선택한 면에 구멍(hole)이 있어 연산할 수 없습니다.\n': 'Boolean {op} — the selected face has a hole, so this cannot run.\n',
  '(현재 Boolean은 단일 outer loop 면만 지원 — constrained Delaunay triangulation 추가 시 확장 예정)\n\n': '(Boolean currently supports single outer-loop faces only — this will widen once constrained Delaunay triangulation lands)\n\n',
  '우회:\n': 'Workarounds:\n',
  '1. 구멍이 없는 다른 면을 선택하거나\n': '1. Pick a face without holes, or\n',
  '2. "내부 면을 구멍으로 합치기"를 역으로 해제한 뒤 시도': '2. Undo "Merge inner face as hole" and try again',
  'Boolean {op} 실패: {rawError}': 'Boolean {op} failed: {rawError}',
  '모델과 교차 실패: {error}': 'Intersect with model failed: {error}',
  '모델과 교차 완료 (총 {totalFaces} 면)': 'Intersected with model ({totalFaces} faces total)',
  '{opName} (multi) — 엔진 오류 ({reason}):\n{detail}': '{opName} (multi) — engine error ({reason}):\n{detail}',
  '{opName}: 변경 없음 — 두 solid 가 떨어져 있거나, 이 형상 구성이 아직 미지원입니다. 겹치는 모서리(corner) 절단과 곡면(구·원기둥·원뿔·원환) ∩ box 는 지원 — 관통 구멍·홈·완전 포함은 준비 중 (ADR-275/276).': '{opName}: nothing changed — either the two solids do not overlap, or this configuration is not supported yet. Overlapping corner cuts and curved (sphere/cylinder/cone/torus) ∩ box do work — through-holes, grooves and full containment are still coming (ADR-275/276).',
  '{opName} (multi) 부분 성공 — {successPairs}/{totalPairs} pair 성공, 새 면 {newCount}개, 제거 {removedCount}개.\n첫 경고: {firstWarning}': '{opName} (multi) partly succeeded — {successPairs}/{totalPairs} pairs, {newCount} faces added, {removedCount} removed.\nFirst warning: {firstWarning}',
  '{opName} (multi, {sourceLabel}) 완료 — 새 면 {newCount}개, 제거 면 {removedCount}개 ({successPairs}/{totalPairs} pair 성공).': '{opName} (multi, {sourceLabel}) done — {newCount} faces added, {removedCount} removed ({successPairs}/{totalPairs} pairs).',
  'Boolean {op}: 두 솔리드의 면을 선택하세요 (현재 {selection}개)\n': 'Boolean {op}: select faces from two solids ({selection} selected)\n',
  '1) 첫 솔리드 면 클릭 → 2) Shift+클릭으로 두 번째 솔리드 면 추가 → 3) 연산 실행': '1) Click a face on the first solid → 2) Shift-click a face on the second → 3) Run the operation',
  '{opName} 완료 (solid CSG)': '{opName} done (solid CSG)',
  'Sheet {sheets}개 + Wall {walls}개 혼합 선택 — Sheet끼리 또는 Wall끼리만 가능합니다.': '{sheets} sheets + {walls} walls selected — mix sheets with sheets, or walls with walls.',
  'Sheet Boolean은 정확히 2개의 동일 평면 Sheet 면이 필요합니다 (현재 {selection}개).': 'Sheet boolean needs exactly 2 coplanar sheet faces ({selection} selected).',
  'Sheet {nameKo} 완료 — 결과 face #{newFace}': 'Sheet {nameKo} done — result face #{newFace}',
  '\nA(유지) {facesA}면 − B(제거) {facesB}면': '\nA (kept) {facesA} faces − B (removed) {facesB} faces',
  'Boolean {nameKo} 완료 — 결과 면 {count}개{curvedNote}{minuendNote}': 'Boolean {nameKo} done — {count} result faces{curvedNote}{minuendNote}',
  'Annulus 만들기 실패: {err}': 'Could not make the annulus: {err}',
  'T-junction 검출 실패: {msg}': 'T-junction detection failed: {msg}',
  'T-junction {healed}개 정리 완료': '{healed} T-junctions cleaned up',
  'T-junction {healed}개 정리, {skipped}개 skip (재시도 가능)': '{healed} T-junctions cleaned up, {skipped} skipped (you can retry)',
  'T-junction 정리 실패 ({skipped}개 skip)': 'T-junction cleanup failed ({skipped} skipped)',
  'Coplanar 검출 실패: {msg}': 'Coplanar detection failed: {msg}',
  'Coplanar {mergedCount}쌍 정리 완료': '{mergedCount} coplanar pairs cleaned up',
  'Coplanar {mergedCount}쌍 정리, {skippedCount}쌍 skip': '{mergedCount} coplanar pairs cleaned up, {skippedCount} skipped',
  'Coplanar 정리 실패 ({skippedCount}쌍 skip)': 'Coplanar cleanup failed ({skippedCount} pairs skipped)',
  'Coplanar 정리 실패: {msg}': 'Coplanar cleanup failed: {msg}',
  'P7 canonical 정합: {componentCount}개 component → ring-with-hole': 'P7 canonical: {componentCount} components → ring-with-hole',
  'P7 canonical ({componentCount}개 component, {violationCount}개 violation — ADR-051 §2.5 deferred boundary 가능)': 'P7 canonical ({componentCount} components, {violationCount} violations — may be the ADR-051 §2.5 deferred boundary)',
  'Connected Inner Merge 실패: {msg}': 'Connected inner merge failed: {msg}',
  '"{tool}" 도구는 아직 준비 중입니다. ': 'The "{tool}" tool is not ready yet.',
  '{fmt} 내보내기는 준비중입니다 (ADR-035 Stage 5).\n': '{fmt} export is not ready yet (ADR-035 Stage 5).\n',
  '현재 가능: OBJ / DXF / glTF / STL.\n': 'Available now: OBJ / DXF / glTF / STL.\n',
  '대안: FreeCAD / Fusion 360 / Rhino 의 STEP→OBJ/STL 변환.': 'Alternative: convert STEP→OBJ/STL in FreeCAD / Fusion 360 / Rhino.',
  '⚠️ {count}개 간섭 발견 (총 {volume}m³). 빨간 박스 확인.': '⚠️ {count} clashes found ({volume}m³ total). See the red boxes.',
  '섹션 {axis}축 위치 (mm, 기본 0)': 'Section {axis} position (mm, default 0)',
  '섹션 {axis}축 @ {pos}mm 활성': 'Section {axis} @ {pos}mm active',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '엣지 필렛 완료 ({ok}개 · 반지름 {radius}mm)': 'Edge fillet done ({ok} edges · radius {radius}mm)',
  '꼭짓점 챔퍼 완료 (반지름 {radius}mm)': 'Vertex chamfer done (radius {radius}mm)',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '자산 라이브러리': 'Asset library',
  '프로젝트 재질 추가': 'Add a project material',
  '프로젝트': 'Project',
  '사용자 재질 추가': 'Add a user material',
  '사용자': 'User',
  '새로고침': 'Refresh',
  '비어 있음': 'Empty',
  'Layered material 채널 추가 (Albedo/Normal/Roughness/Metallic)': 'Add a layered material channel (Albedo/Normal/Roughness/Metallic)',
  '사용자 재질 제거': 'Remove user material',
  '"{name}" 에 추가할 채널 선택\n': 'Pick a channel to add to "{name}"\n',
  '  1 = Albedo (베이스 컬러)\n': '  1 = Albedo (base colour)\n',
  '  2 = Normal (노멀맵)\n': '  2 = Normal (normal map)\n',
  '  3 = Roughness (러프니스)\n': '  3 = Roughness\n',
  '  4 = Metallic (메탈릭)': '  4 = Metallic',
  '{channel} 채널 추가 실패': 'Could not add the {channel} channel',
  '재질 "{name}" 의 {channel} 채널 추가됨': 'Added the {channel} channel to "{name}"',
  '{tier} 재질 이름': '{tier} material name',
  '프로젝트 재질': 'Project material',
  '사용자 재질': 'User material',
  '색상 (hex, 예: #b08040)': 'Colour (hex, e.g. #b08040)',
  '잘못된 색상 형식입니다.': 'That colour format is not valid.',
  '재질 추가 실패 — bridge 미준비': 'Could not add the material — the bridge is not ready',
  '{tier} 재질 "{name}" 추가됨': '{tier} material "{name}" added',
  '사용자 재질 "{name}" 을 제거하시겠습니까?': 'Remove the user material "{name}"?',
  '재질 제거 실패 (사용 중이거나 다른 tier).': 'Could not remove the material (in use, or a different tier).',
  '재질 "{name}" 제거됨': 'Material "{name}" removed',
  'Capability Explorer + UI 도구 invocations 자동 기록 (P26.7 정책).': 'Capability Explorer and UI tool invocations are logged automatically (P26.7).',
  'Audit log 을 모두 삭제하시겠습니까?': 'Clear the whole audit log?',
  '기록된 audit 항목이 없습니다.': 'No audit entries yet.',
  '선택한 면으로 그룹 생성': 'Group the selected faces',
  '그룹이 없습니다': 'No groups',
  'Group-{groupId} 해제됨': 'Group-{groupId} ungrouped',
  '콘솔 열기/닫기': 'Show/hide the console',
  '콘솔': 'Console',
  '전체': 'All',
  '오류': 'Errors',
  '경고': 'Warnings',
  '정보': 'Info',
  '버그 리포트용 — 모든 항목 클립보드로': 'For a bug report — copies every entry to the clipboard',
  '복사됨 ✓': 'Copied ✓',
  '복사 실패': 'Copy failed',
  '지우기': 'Clear',
  '구속 (Constraints)': 'Constraints',
  '모든 제약 재해결': 'Re-solve every constraint',
  '모두 삭제': 'Delete all',
  '제약이 없습니다': 'No constraints',
  '활성/비활성': 'Enable/disable',
  '작업 기록 (Parametric)': 'Operation history (parametric)',
  '기록 전체 삭제': 'Clear the whole history',
  '파라미터 기반 작업만 기록됩니다. 항목을 클릭하면 같은 연산을 새 값으로 재실행합니다. (예: 필렛 반경만 변경해 재적용)': 'Only parameter-driven operations are recorded. Click an entry to re-run the same operation with new values — say, the same fillet at a different radius.',
  '기록된 작업이 없습니다.': 'Nothing recorded yet.',
  '작업 기록을 모두 삭제하시겠습니까?': 'Clear the whole operation history?',
  '같은 연산을 새 값으로 재실행': 'Re-run this operation with new values',
  '재실행…': 'Re-run…',
  ' 외 {count}개': ' and {count} more',
  '⚠️ "{name}" 재실행 시 {count}개 후속 작업이 영향받습니다:\n': '⚠️ Re-running "{name}" affects {count} later operations:\n',
  'Phase 2 MVP는 자동 cascade 재계산을 아직 수행하지 않습니다 — 후속 작업은\n별도로 다시 실행해야 합니다. 계속하시겠습니까?': 'The Phase 2 MVP does not recompute the cascade yet, so you will have to re-run those yourself.\nContinue?',
  '재실행 완료: {kind}': 'Re-ran: {kind}',
  '필렛 반경 (mm) — 현재 선택된 엣지에 적용:': 'Fillet radius (mm) — applies to the selected edges:',
  '챔퍼 거리 (mm) — 현재 선택된 엣지에 적용:': 'Chamfer distance (mm) — applies to the selected edges:',
  '두께 (mm) — 현재 선택된 면에 적용:': 'Thickness (mm) — applies to the selected faces:',
  '선형 배열 "N, dx, dy, dz":': 'Linear array "N, dx, dy, dz":',
  '원형 배열 "N, axis(x|y|z), 총각도°":': 'Radial array "N, axis(x|y|z), total angle°":',
  '(subdivide는 파라미터 없음 — Enter로 재실행)': '(Subdivide takes no parameters — press Enter to re-run)',
  '{kind} — 각도/값:': '{kind} — angle/value:',
  '씬 무결성 검사': 'Scene integrity check',
  '미실행': 'Not run',
  '자기교차(self-intersection) 검증. 큰 mesh 에서 비싸므로 명시 실행.': 'Checks for self-intersections. Expensive on a large mesh, so it only runs when you ask.',
  ' · 자기교차 0': ' · no self-intersections',
  'invariant 위반 {count}': '{count} invariant violations',
  '자기교차 {count} pair': '{count} self-intersecting pairs',
  '✗ {parts} 발견.': '✗ Found {parts}.',
  '⚠ 자기교차 (self-intersection) {count} pair — 위상은 valid 이나 기하가 겹침': '⚠ {count} self-intersecting pairs — the topology is valid but the geometry overlaps',
  'Face {fa} ⨯ Face {fb} 교차': 'Face {fa} ⨯ Face {fb} intersect',
  '{label}\n\nUV 투영 방식\n': '{label}\n\nUV projection\n',
  '  1 = planar (평면 — 바닥/벽)\n': '  1 = planar (floors and walls)\n',
  '  2 = box (박스 — 큐브 자동)\n': '  2 = box (automatic, cube-like)\n',
  '  3 = cylindrical (원통 — 실린더)': '  3 = cylindrical',
  '{label}\n\n타일 크기 (월드 단위 당 반복 횟수)\n': '{label}\n\nTile size (repeats per world unit)\n',
  '  0.001 = 1m 타일\n': '  0.001 = 1m tiles\n',
  '  0.01  = 100mm 타일 (작은 패턴)': '  0.01  = 100mm tiles (small pattern)',
  '유효한 scale 값을 입력해주세요.': 'Enter a valid scale.',
  'NURBS 제어점': 'NURBS control points',
  'NURBS 제어점 — {kind} ({nU}×{nV})': 'NURBS control points — {kind} ({nU}×{nV})',
  'x, y, z, weight 는 숫자여야 합니다': 'x, y, z and weight must be numbers',
  'weight 는 0보다 큰 값이어야 합니다': 'Weight must be greater than 0',
  '현재 뷰 저장': 'Save the current view',
  '추가': 'Add',
  '닫기': 'Close',
  '저장된 Scene 없음. "+ 추가"로 현재 뷰 캡처.': 'No saved scenes. Use "+ Add" to capture the current view.',
  '클릭하여 복원': 'Click to restore',
  'Scene 이름': 'Scene name',
  '새 이름': 'New name',
  '이 Scene을 삭제할까요?': 'Delete this scene?',
  'UV 투영 방식\n': 'UV projection\n',
  '타일 크기 (mm 당 반복 횟수, 기본 0.001 = 1m당 1타일)\n': 'Tile size (repeats per mm; 0.001 = one tile per metre)\n',
  '새 재질 이름': 'New material name',
  '재질 "{name}" 생성 + {count}개 면에 적용': 'Created "{name}" and applied it to {count} faces',
  '재질 "{name}" 생성됨. 면 선택 후 Inspector에서 할당하세요.': 'Created "{name}". Select faces and assign it from the Inspector.',
  '거리': 'Distance',
  'Catmull-Clark 분할': 'Catmull-Clark subdivision',
  '시스템': 'System',
  '베이스 컬러 (Albedo)': 'Base colour (Albedo)',
  '노멀맵 (Normal)': 'Normal map',
  '러프니스맵 (Roughness)': 'Roughness map',
  '메탈릭맵 (Metallic)': 'Metallic map',
  '건축 설계': 'Architectural',
  '건축 분위기': 'Architectural mood',
  '야외 매스': 'Outdoor massing',
  '밝은 하늘': 'Bright sky',
  '클래식 흰색': 'Classic white',
  '다크 모드': 'Dark',
  '블루프린트': 'Blueprint',
  '석양': 'Sunset',
  '모노크롬': 'Monochrome',
  '따뜻한 톤': 'Warm',
  '네온': 'Neon',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '제약 #{id} 삭제됨': 'Constraint #{id} deleted',
  '제약 해결 API를 사용할 수 없습니다': 'The constraint solver API is unavailable',
  '수렴 완료 ({iterations} iter, residual={residual})': 'Converged ({iterations} iterations, residual={residual})',
  '과제약 감지 — 수렴 실패 (residual={residual})': 'Over-constrained — did not converge (residual={residual})',
  '수렴 실패 ({iterations} iter)': 'Did not converge ({iterations} iterations)',
  '{count}개 제약을 모두 삭제할까요?': 'Delete all {count} constraints?',
  '{count}개 제약 삭제됨': '{count} constraints deleted',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '{count}개': '{count}',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '면 방향 표시 (디버그)': 'Show face orientation (debug)',
  'ON: 모든 면 양면 다른 색 (winding 가시화).': 'ON: every face gets two different sides (shows winding).',
  'OFF: open mesh 양면 동일, closed solid 만 두 톤. (ADR-018)': 'OFF: open meshes look the same both sides; only closed solids are two-tone. (ADR-018)',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '라인 그리기. 사용법: L [길이] [높이] 또는 L x1,y1,z1 x2,y2,z2': 'Draw a line. Usage: L [length] [height] or L x1,y1,z1 x2,y2,z2',
  '라인 도구 활성화됨. 클릭으로 시작점을 선택하세요.': 'Line tool active. Click to set the start point.',
  '라인 도구: 길이 {length} mm': 'Line tool: length {length} mm',
  '좌표 형식: x1,y1,z1 x2,y2,z2': 'Coordinate format: x1,y1,z1 x2,y2,z2',
  '모든 좌표는 숫자여야 합니다': 'Every coordinate must be a number',
  '라인 생성됨 (길이: {len} mm)': 'Line created (length: {len} mm)',
  '명령 형식이 잘못되었습니다': 'That command is not formatted correctly',
  '면 머지 각도 tolerance 설정 (°). 예: mergetol 2 — 2°까지 허용': 'Set the merge angle tolerance (°). e.g. mergetol 2 — allows up to 2°',
  '현재 merge tolerance: {getMergeTolerance}°': 'Current merge tolerance: {getMergeTolerance}°',
  '유효한 각도(0~10°)를 입력하세요': 'Enter a valid angle (0–10°)',
  '면 머지 tolerance: {v}° (0.5° = strict, 2~5° = loose)': 'Merge tolerance: {v}° (0.5° = strict, 2–5° = loose)',
  '등록된 Curve 목록 표시 (CurveRegistry)': 'List the registered curves (CurveRegistry)',
  '등록된 곡선 없음': 'No curves registered',
  '곡선 {all}개:\n': '{all} curves:\n',
  'CurveRegistry 전체 초기화 (DCEL 영향 없음)': 'Clear the whole CurveRegistry (does not touch the DCEL)',
  '{n}개 curve 메타데이터 제거 (DCEL edges는 보존)': 'Removed {n} curve records (the DCEL edges are kept)',
  '현재 mesh에 Import Normalizer 재실행 (ADR-007 Barrier)': 'Re-run the import normalizer on the current mesh (ADR-007 barrier)',
  '퇴화 {degenerateRemoved}개 제거': '{degenerateRemoved} degenerate removed',
  'winding {windingFlipped}개 flip': '{windingFlipped} windings flipped',
  '고아 vertex {isolatedVertsRemoved}개 제거': '{isolatedVertsRemoved} orphan vertices removed',
  'Normalize 완료{parts} / {status}': 'Normalize done{parts} / {status}',
  ' (변경 없음)': ' (nothing changed)',
  '남은 위반 {count}건': '{count} violations remain',
  'invariants 통과': 'invariants pass',
  '자유 엣지로 이뤄진 닫힌 polygon을 face로 합성 (수동 트리거)': 'Build faces from closed polygons of free edges (run it yourself)',
  '자유 엣지가 없습니다': 'There are no free edges',
  '{created}개 면 합성 완료 (자유 엣지 {free}개 중)': 'Built {created} faces (from {free} free edges)',
  '{free}개 자유 엣지 발견하나 닫힌 polygon 미감지': 'Found {free} free edges but no closed polygon',
  'ADR-007 invariant 검증 — topology + outward normal 리포트': 'Check the ADR-007 invariants — topology and outward normals',
  '✓ Topology: {checkedFaces}개 face invariants 통과': '✓ Topology: {checkedFaces} faces pass the invariants',
  '✗ Topology: {violationCount}개 위반 ({checkedFaces}개 검사)': '✗ Topology: {violationCount} violations ({checkedFaces} faces checked)',
  '· Outward: open surface — 검증 스킵 (OK)': '· Outward: open surface — skipped (fine)',
  '✓ Outward: {checkedFaces}개 face 모두 바깥 향함': '✓ Outward: all {checkedFaces} faces point outward',
  '✗ Outward: {inwardCount}/{checkedFaces}개 face 내부 향함': '✗ Outward: {inwardCount}/{checkedFaces} faces point inward',
  '씬 부피 무결성 검사 (watertight / 크랙 / winding). 사용: integrity': 'Check scene volume integrity (watertight / cracks / winding). Usage: integrity',
  'integrity: WASM에 verifyVolumeIntegrity 미노출 — rebuild 필요': 'integrity: verifyVolumeIntegrity is not exposed by the WASM build — rebuild needed',
  '✓ 부피 무결성 OK (검사 면 {checkedFaces}개)': '✓ Volume integrity OK ({checkedFaces} faces checked)',
  '✗ 부피 무결성 위반:\n': '✗ Volume integrity violations:\n',
  '  invariant 위반 {invariantViolations}건\n': '  {invariantViolations} invariant violations\n',
  '  기하 크랙 {geometricCracks}개\n': '  {geometricCracks} geometric cracks\n',
  '  열린 경계 edge {openBoundaryEdges}개\n': '  {openBoundaryEdges} open boundary edges\n',
  '  (검사 면 {checkedFaces}개)': '  ({checkedFaces} faces checked)',
  '비-manifold edge (3+ face) 자동 수리. 사용: repair [diag|fix]': 'Repair non-manifold edges (3+ faces) automatically. Usage: repair [diag|fix]',
  'repair: WASM에 findNonManifoldEdges 미노출 — rebuild 필요': 'repair: findNonManifoldEdges is not exposed by the WASM build — rebuild needed',
  '✓ 비-manifold edge 0개 — 메시 깨끗': '✓ No non-manifold edges — the mesh is clean',
  '✗ 비-manifold edge {count}개:\n  {sample}{more}\n': '✗ {count} non-manifold edges:\n  {sample}{more}\n',
  '  → "repair fix" 명령으로 자동 수리': '  → run "repair fix" to repair them',
  'repair: WASM에 repairNonManifoldEdges 미노출 — rebuild 필요': 'repair: repairNonManifoldEdges is not exposed by the WASM build — rebuild needed',
  '✓ 수리할 non-manifold edge 없음 (검사 {edgesExamined}개)': '✓ No non-manifold edges to repair ({edgesExamined} checked)',
  '✓ 수리 완료: edge {edgesRepaired}개 정리, {facesDetached}개 face 분리, {vertsCreated}개 vertex 복제': '✓ Repaired: {edgesRepaired} edges cleaned up, {facesDetached} faces detached, {vertsCreated} vertices duplicated',
  ' ({edgesSkipped}개 skip)': ' ({edgesSkipped} skipped)',
  'CAD 모드 토글 (single-sided 렌더, GPU 성능↑). 사용: cadmode [on|off|toggle]': 'Toggle CAD mode (single-sided rendering, faster GPU). Usage: cadmode [on|off|toggle]',
  'viewport 접근 불가': 'Cannot reach the viewport',
  'CAD 모드: {cur}': 'CAD mode: {cur}',
  '사용법: cadmode [on|off|toggle]': 'Usage: cadmode [on|off|toggle]',
  'CAD 모드: {next}': 'CAD mode: {next}',
  'ON — single-sided 렌더 (외부=Front)': 'ON — single-sided rendering (outside = front)',
  'OFF — two-tone 렌더': 'OFF — two-tone rendering',
  '면 머지 시 재질 경계 존중 토글 (on/off/toggle). 현재값 출력: 인수 없음': 'Toggle whether merging respects material boundaries (on/off/toggle). No argument prints the current value',
  '재질 경계 존중: {cur}': 'Respect material boundaries: {cur}',
  '사용법: mergemat [on|off|toggle]': 'Usage: mergemat [on|off|toggle]',
  '재질 경계 존중: {next}': 'Respect material boundaries: {next}',
  'ON — 같은 재질끼리만 병합': 'ON — only merge faces sharing a material',
  'OFF — 재질 무시': 'OFF — ignore materials',
  '사용할 수 있는 명령 표시': 'Show the available commands',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '명령어 입력 (Command)': 'Command input',
  '예: L 100 (라인), R 50,50,100 (상자), C 50 (원)': 'e.g. L 100 (line), R 50,50,100 (box), C 50 (circle)',
  '알 수 없는 명령: {cmd}': 'Unknown command: {cmd}',
  '실행됨: {name}': 'Ran: {name}',
  '오류: {message}': 'Error: {message}',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '체적': 'Volume',
  '위치만 존재 (L=0, W=0, H=0)': 'Position only (L=0, W=0, H=0)',
  '길이만 존재 (H=0)': 'Length only (H=0)',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '모호한 Orphan {c3}건 발견. \'정리 → Orphan 수동 복구\' 메뉴 참고': '{c3} ambiguous orphans found. See Cleanup → Recover orphans by hand.',
  'Orphan 자동 복구 실패: {result} (원본 유지)': 'Could not recover the orphans automatically: {result} (original kept)',
  '레거시 파일: {autoFaces}개 face를 {newXias}개 XIA로 자동 복구됨 · Ctrl+Z로 취소': 'Legacy file: recovered {autoFaces} faces into {newXias} XIAs · Ctrl+Z to undo',
  '\n(모호한 Orphan {c3}건은 \'정리 → Orphan 수동 복구\' 메뉴로 처리)': '\n({c3} ambiguous orphans are left for Cleanup → Recover orphans by hand)',
  '스냅샷 생성 실패': 'Could not create the snapshot',
  '저장 완료: {currentFileName}': 'Saved: {currentFileName}',
  '저장 실패: {error}': 'Save failed: {error}',
  '로드 완료: {currentFileName}': 'Loaded: {currentFileName}',
  '프로젝트 로드 실패': 'Could not load the project',
  '파일 읽기 실패: {error}': 'Could not read the file: {error}',
  '파일 크기가 너무 작습니다': 'The file is too small',
  '유효하지 않은 AXIA 파일입니다': 'This is not a valid AXIA file',
  '지원하지 않는 버전입니다 (v{version}). 현재 지원: v1~v{AXIA_VERSION}': 'Unsupported version (v{version}). Supported: v1–v{AXIA_VERSION}',
  '파일이 손상되었습니다': 'The file is damaged',
  '파일 가져오기 실패: {error}': 'Import failed: {error}',
  '파일 선택 대화 실패: {error}': 'The file picker failed: {error}',
  'auto-reference 모듈 로드 실패: {error}': 'Could not load the auto-reference module: {error}',
  'axia inject 실패 (graceful): {error}': 'Could not inject into axia (carrying on): {error}',
  '{result} import: {allWarnings}개 경고 (콘솔 참조)': '{result} import: {allWarnings} warnings (see the console)',
  ' · "{autoRegisterRefName}" Reference 등록 ({autoRegisterFaceCount} 면)': ' · registered "{autoRegisterRefName}" as a reference ({autoRegisterFaceCount} faces)',
  '{result} import 완료: {faceCount}면 {edgeCount}엣지{refSuffix}': '{result} imported: {faceCount} faces, {edgeCount} edges{refSuffix}',
  '지원하지 않는 파일 형식입니다: .{ext}': 'Unsupported file type: .{ext}',
  '지원하지 않는 포맷: {format}': 'Unsupported format: {format}',
  'DXF 파일 파싱 실패: {error}': 'Could not parse the DXF file: {error}',
  '.skp 직접 import는 지원하지 않습니다.\n\n': '.skp files cannot be imported directly.\n\n',
  'SketchUp에서 File → Export → 3D Model → COLLADA(.dae) 또는 OBJ로\n': 'In SketchUp, use File → Export → 3D Model → COLLADA (.dae) or OBJ,\n',
  '내보낸 뒤 해당 파일을 import해 주세요. (geometry · material · hierarchy 보존)': 'then import that file. (Geometry, materials and hierarchy are kept.)',
  'DWG 파일 처리 실패: {error}': 'Could not handle the DWG file: {error}',
  'DXF 파싱 실패: {error}': 'Could not parse the DXF: {error}',
  'Rhino 3DM 파일 처리 실패: {error}': 'Could not handle the Rhino 3DM file: {error}',
  'STEP/IGES 엔진(OCCT.js)이 설치되지 않았습니다.\n\n': 'The STEP/IGES engine (OCCT.js) is not installed.\n\n',
  '설치 명령:\n': 'To install:\n',
  '설치 없이 사용 가능한 우회법:\n': 'Or, without installing it:\n',
  '• FreeCAD: STEP → STL/DXF 변환\n': '• FreeCAD: convert STEP → STL/DXF\n',
  '• Fusion 360: 내보내기 → OBJ\n': '• Fusion 360: Export → OBJ\n',
  '• Rhino: Save As → 3DM (AXiA 직접 지원)': '• Rhino: Save As → 3DM (AXiA reads this directly)',
  'STEP/IGES 엔진 로딩 중... (~3.5MB, 첫 사용 시에만)': 'Loading the STEP/IGES engine… (~3.5MB, first use only)',
  '(진단: opencascade.js 패키지에서 initOpenCascade entry 를 찾지 못함 — 버전 호환성 issue 의심. ADR-082 L1 lock-in semver caret 범위 확인.)': '(Diagnostic: no initOpenCascade entry in the opencascade.js package — likely a version mismatch. Check the semver caret range, ADR-082 L1.)',
  'STEP/IGES importer 가 처리할 수 없는 확장자: .{ext}': 'The STEP/IGES importer cannot handle .{ext}',
  '파일 분석 중...': 'Reading the file…',
  'STEP/IGES shape 추출 실패 — traversal 건너뜀': 'Could not extract the STEP/IGES shape — skipping traversal',
  'Mesh 생성 중...': 'Building the mesh…',
  'shape null — tessellation 건너뜀': 'shape is null — skipping tessellation',
  'edge[{index}] line 생성: {error}': 'edge[{index}] line: {error}',
  'face[{index}] mesh 생성: positions length not multiple of 3': 'face[{index}] mesh: positions length is not a multiple of 3',
  'face[{index}] mesh 생성: {error}': 'face[{index}] mesh: {error}',
  'DXF 파싱 실패: {result}': 'Could not parse the DXF: {result}',
  '선 {lines}': '{lines} lines',
  '폴리선 {polylines}': '{polylines} polylines',
  '원 {circles}': '{circles} circles',
  '호 {arcs}': '{arcs} arcs',
  '3D면 {faces3d}': '{faces3d} 3D faces',
  '솔리드 {solids}': '{solids} solids',
  '타원 {ellipses}': '{ellipses} ellipses',
  '스플라인 {splines}': '{splines} splines',
  '단위: 비-mm (계수 {unitScale}×) — 현재 자동 스케일 미지원. 필요 시 모두 선택 후 스케일 도구로 {unitScale}× 적용하세요.': 'Units are not mm (factor {unitScale}×) — auto-scaling is not supported yet. If you need it, select everything and apply {unitScale}× with the scale tool.',
  '자유 엣지 {freeEdges}개 발견. 메뉴 → 수정 → \'자유 엣지 → 면 합성\' 으로 면 생성 가능.': 'Found {freeEdges} free edges. Modify → "Free edges → build faces" can turn them into faces.',
  'DXF 가져오기 중 오류: {error}': 'Error while importing the DXF: {error}',
  '"{fileName}" 의 단위를 선택하세요.\n\n': 'Pick the units for "{fileName}".\n\n',
  'mm (밀리미터, 기본)\n': 'mm (millimetres, default)\n',
  'cm (센티미터)\n': 'cm (centimetres)\n',
  'm (미터)\n': 'm (metres)\n',
  'in (인치)\n': 'in (inches)\n',
  'ft (피트)\n\n': 'ft (feet)\n\n',
  '단위명 입력 (기본 mm):': 'Enter a unit (default mm):',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  'DXF 가져오기 실패: WASM 엔진이 준비되지 않았습니다.\n로컬에서 wasm-pack 빌드 후 다시 시도해 주세요.': 'DXF import failed: the WASM engine is not ready.\nBuild it locally with wasm-pack and try again.',
  '알 수 없음': 'unknown',
  '프로젝트 이름을 입력하세요:': 'Project name:',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '쪼개기 (2 볼륨)': 'Split (2 volumes)',
  '트림 — 위쪽 유지': 'Trim — keep the top',
  '트림 — 아래쪽 유지': 'Trim — keep the bottom',
  'Slice: 자를 볼륨의 면을 먼저 선택하세요': 'Slice: select a face of the volume you want to cut',
  'Slice: 선택된 면에 소속 볼륨(XIA)이 없습니다': 'Slice: the selected face does not belong to a volume (XIA)',
  'Slice: 한 번에 하나의 볼륨만 자를 수 있습니다 — 단일 솔리드의 면을 선택하세요': 'Slice: one volume at a time — select faces from a single solid',
  'Slice: XIA {xiaId}에 면이 없습니다': 'Slice: XIA {xiaId} has no faces',
  'Slice [{mode}]: 평면 3점 클릭 — 또는 한 점 클릭 후 H=수평 절단.\n': 'Slice [{mode}]: click 3 points for the plane — or click one point then H for a horizontal cut.\n',
  'M=모드 전환 (쪼개기/트림), Esc 취소': 'M = switch mode (split/trim), Esc to cancel',
  '두 번째 점은 첫 번째와 다른 위치여야 합니다': 'The second point must be somewhere else than the first',
  '세 점이 일직선 — 다른 위치를 클릭하세요': 'The three points are in a line — click somewhere else',
  'Slice 취소': 'Slice cancelled',
  'Slice 모드: {mode}': 'Slice mode: {mode}',
  '수직 평면 모드: p1-p2가 세로축과 평행 — 세 번째 점을 클릭하세요': 'Vertical plane: p1-p2 runs along the vertical axis — click a third point',
  '⚠️ Slice 불가 — 솔리드(volume) 가 선택되지 않았습니다.\n': '⚠️ Cannot slice — no solid is selected.\n',
  '먼저 돌출/잘라내기(Extrude/Cut) 로 입체를 만들고, 그 면을 선택한 후 Slice 도구 사용': 'Extrude or cut something into a solid first, select one of its faces, then use Slice',
  ' — 아래쪽 새 볼륨 (XIA {newXia})': ' — bottom is a new volume (XIA {newXia})',
  '곡면 쪼개기 완료 — 2개 볼륨, 곡면 보존{xiaNote} (면 {n}개)': 'Curved split done — 2 volumes, curvature kept{xiaNote} ({n} faces)',
  '곡면 트림 완료 — {side} 유지, 곡면 보존 (면 {n}개)': 'Curved trim done — kept the {side}, curvature kept ({n} faces)',
  '위쪽': 'top',
  '아래쪽': 'bottom',
  '곡면 절단 실패: {error}': 'Curved cut failed: {error}',
  '곡면 트림 완료 — 임의 평면, 곡면 보존 (면 {n}개)': 'Curved trim done — arbitrary plane, curvature kept ({n} faces)',
  '곡면 트림 실패: {error}': 'Curved trim failed: {error}',
  'Slice: WASM 엔진에 sliceVolumeByPlane 함수가 없습니다 (rebuild 필요)': 'Slice: sliceVolumeByPlane is missing from the WASM build (rebuild needed)',
  'Trim: 응답 파싱 실패': 'Trim: could not parse the response',
  '트림 완료 — {keepAbove} 유지 (면 {tres}개)': 'Trim done — kept the {keepAbove} ({tres} faces)',
  '위쪽(+법선)': 'top (+normal)',
  '아래쪽(−법선)': 'bottom (−normal)',
  '트림 실패: {error}': 'Trim failed: {error}',
  '트림 미지원 빌드 — 쪼개기(2 볼륨)로 대체합니다.': 'This build cannot trim — splitting into 2 volumes instead.',
  'Slice: 응답 파싱 실패': 'Slice: could not parse the response',
  'Slice 실패: {userMsg}': 'Slice failed: {userMsg}',
  'Slice 완료 — 위쪽은 원본 볼륨에 유지, 아래쪽은 새 볼륨 (XIA {newXia})': 'Slice done — the top stays with the original volume, the bottom is a new one (XIA {newXia})',
  '솔리드(volume) 가 없습니다. 돌출/잘라내기(Extrude/Cut) 로 입체 먼저 만들기': 'There is no solid. Extrude or cut something into one first.',
  '여러 볼륨 동시 자르기 불가 — 단일 솔리드의 면을 선택하세요': 'Cannot cut several volumes at once — select faces from a single solid',
  '선택된 면에 소속 볼륨이 없습니다 (Sheet face — 돌출/잘라내기 필요)': 'The selected face has no volume (it is a sheet — extrude or cut it first)',
  '소속 볼륨을 결정할 수 없습니다': 'Cannot work out which volume this belongs to',
  'Tapered extrude 실패 — 테이퍼가 너무 가파르거나 (자기교차/붕괴) 평면 프로파일이 아닙니다': 'Tapered extrude failed — the taper is too steep (it self-intersects or collapses), or the profile is not planar',
  'Cone extrude 실패 — top 비율은 0~1 (1 이상은 원통→직선 Extrude) 이어야 하고 평면 원 프로파일이어야 합니다': 'Cone extrude failed — the top ratio must be 0–1 (1 or more is a cylinder, use a straight extrude) and the profile must be a planar circle',
  '양방향 extrude 실패 — 위/아래 거리는 0 이상, 합 > 0 이어야 하고 평면 프로파일이어야 합니다': 'Two-way extrude failed — both distances must be 0 or more with a total above 0, and the profile must be planar',
  '프로젝트 내보내기 성공': 'Project exported',
  '프로젝트 내보내기 실패': 'Could not export the project',
  '프로젝트 불러오기 성공': 'Project loaded',
  '프로젝트 불러오기 실패': 'Could not load the project',
  'DXF 불러오기 성공: {faces}개 면': 'DXF loaded: {faces} faces',
  'DXF 불러오기 실패: {error}': 'Could not load the DXF: {error}',
  'DXF 파일 파싱 실패': 'Could not parse the DXF file',
  '이동 실행 실패': 'Move failed',
  '회전 실행 실패': 'Rotate failed',
  '스케일 실행 실패': 'Scale failed',
  'Offset 실패: {result}': 'Offset failed: {result}',
  'Offset 실행 실패': 'Offset failed',
  'Recess 실패: {result}': 'Recess failed: {result}',
  'Recess 실행 실패': 'Recess failed',
  'Edge Offset 실패: {result}': 'Edge offset failed: {result}',
  'Edge Offset 실행 실패': 'Edge offset failed',
  'Boolean {op} 실패: {result}': 'Boolean {op} failed: {result}',
  'Boolean {op} 성공': 'Boolean {op} done',
  'Boolean 연산 실패: {error}': 'Boolean operation failed: {error}',
  'Sheet {op} 실패: {res}': 'Sheet {op} failed: {res}',
  'Sheet {op} 성공': 'Sheet {op} done',
  'Sheet 연산 실패: {error}': 'Sheet operation failed: {error}',
  '참조 치수(읽기전용): 정점→정점 / 원·호 / 엣지→엣지 클릭': 'Reference dimension (read-only): vertex→vertex / circle or arc / edge→edge',
  '둘째 정점을 클릭하세요 (Esc 취소)': 'Click the second vertex (Esc to cancel)',
  '정점·원/호·엣지 위를 클릭하세요': 'Click a vertex, a circle/arc, or an edge',
  '참조 반지름 (R{radius})': 'Reference radius (R{radius})',
  '둘째 엣지를 클릭하세요 (Esc 취소)': 'Click the second edge (Esc to cancel)',
  '치수를 잴 정점 위를 클릭하세요': 'Click the vertex you want to measure from',
  '서로 다른 정점을 선택하세요': 'Pick two different vertices',
  '두 정점이 같은 위치입니다': 'Those two vertices are in the same place',
  '참조 거리 ({distance})': 'Reference distance ({distance})',
  '둘째 엣지를 클릭하세요': 'Click the second edge',
  '서로 다른 엣지를 선택하세요': 'Pick two different edges',
  '평행/일직선 엣지는 각도 치수 불가': 'Parallel or collinear edges have no angle to measure',
  '참조 각도 ({angle}°)': 'Reference angle ({angle}°)',
  '{label} — 읽기전용': '{label} — read-only',
  '참조 치수 생성 실패': 'Could not create the reference dimension',
  'NURBS 곡면 1개를 먼저 선택하세요': 'Select one NURBS surface first',
  '선택한 면이 NURBS 곡면이 아닙니다': 'The selected face is not a NURBS surface',
  '제어망을 읽을 수 없습니다': 'Cannot read the control net',
  '제어점(주황 마커) 클릭=값 입력 / 드래그=이동 (X/Y/Z 축 고정, Esc 종료)': 'Control points (orange): click to type a value, drag to move (X/Y/Z locks an axis, Esc to finish)',
  'NURBS 곡면 선택 후 도구를 다시 활성화하세요': 'Select a NURBS surface, then start the tool again',
  '제어점 마커를 클릭하세요': 'Click a control point marker',
  '제어점 {idx} 라이브 이동 → ({x}, {y}, {z})': 'Control point {idx} moved → ({x}, {y}, {z})',
  'NURBS 라이브 편집 commit 실패': 'Could not commit the NURBS edit',
  '제어점 {idx} — x, y, z, weight:': 'Control point {idx} — x, y, z, weight:',
  'x, y, z, weight 4개 숫자를 쉼표로 구분해 입력하세요': 'Enter four numbers separated by commas: x, y, z, weight',
  '제어점 {idx} → ({x}, {y}, {z}) w={weight} (패치 재생성)': 'Control point {idx} → ({x}, {y}, {z}) w={weight} (patch rebuilt)',
  '손상 없음': 'No damage',
  '{degenerate}개 면 degenerate': '{degenerate} degenerate faces',
  '{nonManifold}개 엣지 non-manifold': '{nonManifold} non-manifold edges',
  '{boundary}개 boundary edge': '{boundary} boundary edges',
  '{orphan}개 orphan face': '{orphan} orphan faces',
  '위상 손상 {initialDamages}건 자동 복구 완료': 'Repaired {initialDamages} topology problems',
  '자동 복구 {fixesApplied}건 적용 — 잔존 {remainingCount}건. ({reason})': 'Applied {fixesApplied} repairs — {remainingCount} left. ({reason})',
  '위상 복구를 되돌렸습니다.': 'Topology repair undone.',
  'XIA를 형태 (Shape)로 강등했습니다.': 'The XIA is now a Shape.',
  '강등 실패: {msg}': 'Could not demote it: {msg}',
  '수동 수정 필요: {reason}': 'Needs fixing by hand: {reason}',
  '3D 텍스트 내용:': '3D text:',
  '3D 텍스트 취소됨 (다시 도구를 선택해 내용 입력)': '3D text cancelled (pick the tool again to type something)',
  '압출': 'extruded',
  '스프라이트': 'sprite',
  '3D 텍스트 "{text}" — 클릭하여 배치 ({mode} 모드, 연속, Esc 종료)': '3D text "{text}" — click to place it ({mode} mode, keeps going, Esc to finish)',
  '텍스트 도구를 다시 선택해 내용을 입력하세요': 'Pick the text tool again to type something',
  '텍스트를 배치할 위치를 클릭하세요': 'Click where the text should go',
  '이 글자는 3D 폰트에 없어 스프라이트 라벨로 표시했습니다': 'The 3D font has no glyph for this, so it is shown as a sprite label',
  '텍스트 생성 실패': 'Could not create the text',
  '3D 텍스트 모듈 로드 실패': 'Could not load the 3D text module',
  '클릭 위치가 평면 위가 아닙니다 (거리 {dist}mm)': 'That click is not on the plane ({dist}mm away)',
  '주변에 boundary 후보가 없습니다 (반경 {r}mm 확대 필요)': 'No boundary nearby (try a radius wider than {r}mm)',
  '이 영역을 둘러싼 boundary 가 없습니다': 'Nothing encloses this area',
  '이 영역에 이미 면이 있습니다': 'This area already has a face',
  'Boundary 도구: 영역 내부 클릭': 'Boundary tool: click inside an area',
  'Boundary: 유효한 평면 위 위치를 클릭하세요': 'Boundary: click somewhere on a valid plane',
  'Boundary: 입력 위치가 너무 작은 영역': 'Boundary: that area is too small',
  'Boundary 면이 생성되었습니다': 'Boundary face created',
  'Boundary 생성 실패: {userMsg}': 'Could not create the boundary: {userMsg}',
  '재질 손상 없음': 'No material problems',
  'Xia {xiaCount}개 / 면 {faceTotal}개 재질 부재': '{xiaCount} XIAs / {faceTotal} faces have no material',
  '재질 손상 {affectedXias}개 자동 복구 완료 (강등 {facesDemoted} / fallback {facesFallback})': 'Repaired {affectedXias} material problems ({facesDemoted} demoted / {facesFallback} fell back)',
  '자동 복구 후 잔존 {remainingOrphans}건. ({reason})': '{remainingOrphans} left after the repair. ({reason})',
  '재질 복구를 되돌렸습니다.': 'Material repair undone.',
  '각도 치수: 첫 엣지 클릭 → 둘째 엣지 클릭 (영구·편집)': 'Angle dimension: click one edge, then another (permanent, editable)',
  '각도를 잴 엣지를 클릭하세요': 'Click the edge you want the angle from',
  '각도 치수 ({angle}°) — 라벨 클릭으로 편집': 'Angle {angle}° — click the label to edit',
  '각도 치수 생성 실패': 'Could not create the angle dimension',
  '치수: 첫 정점 클릭 → 둘째 정점 클릭 (영구·편집가능 치수)': 'Dimension: click one vertex, then another (permanent, editable)',
  '치수 생성 ({distance}) — 라벨 클릭으로 편집': 'Dimension {distance} — click the label to edit',
  '치수 생성 실패': 'Could not create the dimension',
  '개구부(창/문)를 낼 면 위를 클릭 — 바닥까지 끌면 자동으로 문': 'Click a face to cut an opening — drag it to the floor and it becomes a door',
  '창은 기존 면 위에 내야 합니다 — 면을 클릭하세요': 'A window has to go on an existing face — click one',
  '개구부가 너무 작습니다 — 더 크게 끌어 주세요': 'That opening is too small — drag it wider',
  '문(door)을 냈습니다': 'Door cut',
  '관통 창을 냈습니다': 'Window cut all the way through',
  '창 내기 실패 — 면 경계 안에서 다시 시도하세요': 'Could not cut the window — try again inside the face',
  '창을 냈습니다': 'Window cut',
  '선택된 면이 없습니다': 'No faces selected',
  '선택된 엣지가 없습니다': 'No edges selected',
  '선택된 정점이 없습니다': 'No vertices selected',
  '이 면은 이미 다른 객체 (Xia) 에 속해 있어 참조로 표시할 수 없습니다': 'This face already belongs to another object (XIA), so it cannot be a reference',
  '이 면은 이미 형태 (Shape) 에 속해 있어 참조로 표시할 수 없습니다': 'This face already belongs to a Shape, so it cannot be a reference',
  '이 항목은 이미 다른 참조에 등록되어 있습니다': 'This is already registered as a reference',
  '엔진 미준비 — 페이지 새로고침이 필요합니다': 'The engine is not ready — reload the page',
  '구멍을 뚫을 면 위를 클릭해 중심을 지정하세요': 'Click a face to set the centre of the hole',
  '구멍은 기존 면 위에 뚫어야 합니다 — 면을 클릭하세요': 'A hole has to go on an existing face — click one',
  '구멍 반지름이 너무 작습니다': 'That hole radius is too small',
  '관통 구멍을 뚫었습니다': 'Hole cut all the way through',
  '구멍 뚫기 실패 — 면 경계 안에서 다시 시도하세요': 'Could not cut the hole — try again inside the face',
  '면 구멍을 뚫었습니다': 'Hole cut in the face',
  '구멍을 낼 면 위를 클릭해 윤곽 점을 찍으세요 (Enter/더블클릭/첫 점 클릭으로 닫기)': 'Click a face and place the outline points (Enter, double-click, or click the first point to close)',
  '다각형 구멍은 기존 면 위에 내야 합니다 — 면을 클릭하세요': 'A polygon hole has to go on an existing face — click one',
  '점이 3개 이상 필요합니다': 'At least 3 points are needed',
  '다각형 구멍 실패 — 면 경계 안에서 단순 다각형으로 다시 시도하세요': 'Polygon hole failed — try a simple polygon inside the face',
  '자동 Reference 분류 비활성 (Settings)': 'Automatic reference classification is off (Settings)',
  '등록할 face 가 없습니다': 'There are no faces to register',
  '면이 이미 다른 객체에 속해 있어 자동 Reference 분류 실패': 'The face already belongs to another object, so it cannot be classified as a reference',
  '면이 이미 형태 (Shape) 에 속해 있어 자동 Reference 분류 실패': 'The face already belongs to a Shape, so it cannot be classified as a reference',
  '이미 다른 Reference 에 등록되어 있습니다': 'It is already registered as a reference',
  '밀리미터 (mm)': 'Millimetres (mm)',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '배열할 면 또는 엣지를 먼저 선택하세요': 'Select the faces or edges you want to array',
  '방향/간격 점을 클릭하세요 (개수 {count} · VCB로 변경 · Esc 취소)': 'Click a point for direction and spacing ({count} copies · change it in the VCB · Esc to cancel)',
  '간격 {spacing} × {count}': 'Spacing {spacing} × {count}',
  '선형 배열 완료 ({count}개)': 'Linear array done ({count} copies)',
  '선형 배열 실패': 'Linear array failed',
  '챔퍼할 코너 꼭짓점을 클릭하세요': 'Click the corner you want to chamfer',
  '챔퍼할 코너 꼭짓점 위를 클릭하세요': 'Click on the corner you want to chamfer',
  '거리를 입력하세요 (또는 다시 클릭 = 마지막 값)': 'Enter a distance (or click again to reuse the last one)',
  '코너 챔퍼 완료 (거리 {dist}mm)': 'Corner chamfered ({dist}mm)',
  '챔퍼 실패 (2-valence 코너만 가능 · 거리 확인)': 'Chamfer failed (only 2-valence corners; check the distance)',
  '📏 측정: 시작점 클릭 → 끝점 클릭 (→ 3번째 점으로 각도)': '📏 Measure: click a start point, then an end point (a third point gives an angle)',
  '📏 거리: {fmt}\n': '📏 Distance: {fmt}\n',
  '  (세 번째 점을 클릭하면 각도, Esc 취소)': '  (click a third point for an angle, Esc to cancel)',
  '각도 계산 불가: 각 변이 0 길이': 'Cannot work out the angle: one of the sides has no length',
  '📐 각도 (p2 기준): {deg}°\n': '📐 Angle at p2: {deg}°\n',
  '반지름 치수: 원 또는 호 엣지를 클릭하세요 (영구·편집)': 'Radius dimension: click a circle or arc edge (permanent, editable)',
  '반지름을 잴 원/호 엣지를 클릭하세요': 'Click the circle or arc you want the radius of',
  '원 또는 호 엣지가 아닙니다': 'That is not a circle or arc edge',
  '반지름 치수 (R{radius}) — 라벨 클릭으로 편집': 'Radius R{radius} — click the label to edit',
  '반지름 치수 생성 실패': 'Could not create the radius dimension',
  '면 위': 'On face',
  'Y축': 'Y axis',
  '그리드': 'Grid',
  '2점 중간': 'Midpoint',
  '루프 닫기': 'Close loop',
  '센티미터 (cm)': 'Centimetres (cm)',
  '미터 (m)': 'Metres (m)',
  '인치 (in)': 'Inches (in)',
  '피트 (ft)': 'Feet (ft)',
  '둥글릴 코너 꼭짓점을 클릭하세요': 'Click the corner you want to round',
  '둥글릴 코너 꼭짓점 위를 클릭하세요': 'Click on the corner you want to round',
  '코너 필렛 완료 (반지름 {radius}mm)': 'Corner rounded (radius {radius}mm)',
  '필렛 실패 (2-valence 코너만 가능 · 반지름 확인)': 'Fillet failed (only 2-valence corners; check the radius)',
  '회전축 X/Y/Z 선택 + VCB=개수 → 클릭(또는 Enter)으로 원형 배열, Esc 종료': 'Pick an axis with X/Y/Z, set the count in the VCB, then click or press Enter to array. Esc to finish.',
  '원형 배열 완료 ({count}개 · {axis}축)': 'Radial array done ({count} copies · {axis} axis)',
  '📐 중심선 — 두 점 클릭 (교차해도 분절되지 않음, face 합성 제외). Esc 취소.': '📐 Centreline — click two points. It will not split at crossings and never becomes a face. Esc to cancel.',
  '시작점과 끝점이 같음 — 다시 클릭하세요': 'The start and end are the same — click again',
  '중심선 생성 실패 — ': 'Could not create the centreline —',
  '점: 클릭하여 작도 점 배치 (연속, Esc 종료)': 'Point: click to place a construction point (keeps going, Esc to finish)',
  '점을 배치할 위치를 클릭하세요': 'Click where the point should go',
  '점 생성 실패': 'Could not create the point',
  '미러 평면 위 X/Y/Z 키로 축 선택 → 클릭(또는 Enter)으로 반사, Esc 종료': 'Pick the mirror axis with X/Y/Z, then click or press Enter to mirror. Esc to finish.',
  '미러링할 면 또는 엣지를 먼저 선택하세요': 'Select the faces or edges you want to mirror',
  '{n}개 {kind}를 {plane} 평면 기준 미러링 ({out}개 생성)': 'Mirrored {n} {kind} across the {plane} plane ({out} created)',
  '엣지 위에서 클릭하세요': 'Click on an edge',
  '엣지 분할 → 새 vertex {newVid}': 'Edge split → new vertex {newVid}',
  '엣지 분할 실패 (끝점 근처거나 내부 오류)': 'Edge split failed (too close to an endpoint, or an internal error)',
  '재질 손상 자동 복구 실패': 'Could not repair the material problems',
  '수동수정': 'Fix by hand',
  '강등': 'Demote',
  '위상 손상 자동 복구 실패': 'Could not repair the topology',
  'AXiA 엔진이 업데이트됐습니다. 최신 기능이 적용됩니다.': 'The AXiA engine has been updated. The newest features are now available.',
  '알 수 없는 명령입니다: {actionId}': 'Unknown command: {actionId}',
  '명령 검색… (예: line, push, 단면, undo)': 'Search commands… (e.g. line, push, section, undo)',
  '일치하는 명령이 없습니다': 'No matching commands',
  '클릭하여 치수 편집': 'Click to edit the dimension',
  '클릭하여 각도 편집': 'Click to edit the angle',
  '올바른 .xia 파일이 아닙니다.': 'This is not a valid .xia file.',
  '파일을 불러오는데 실패했습니다.': 'Could not load the file.',
  '세 점이 일직선이거나 너무 가깝습니다 — 평면을 만들 수 없어요': 'The three points are in a line or too close together — no plane can be made from them',
  '작업 평면 설정 완료 — 이후 그리기가 이 평면에 투영됩니다 (Home 키로 해제)': 'Work plane set — drawing now projects onto it (Home to clear)',
  '다각형 변의 수 (3~24)\n': 'Number of sides (3–24)\n',
  '6 = 육각형 (벌집/기하)': '6 = hexagon (honeycomb, geometric)',
  '5 = 오각형': '5 = pentagon',
  '8 = 팔각형 (볼트 헤드)': '8 = octagon (bolt head)',
  '3에서 24 사이의 숫자를 입력해주세요.': 'Enter a number between 3 and 24.',
  'Triangulation handle null (BRepMesh 결과 부재)': 'Triangulation handle is null (no BRepMesh result)',
  'Polygon3D handle null (BRepMesh 결과 부재 또는 edge 미tessellate)': 'Polygon3D handle is null (no BRepMesh result, or the edge was not tessellated)',
  '원통·토러스는 열린 선으로 면을 나눌 수 없습니다. 닫힌 원(곡선)을 그려 포트홀을 만들어 보세요.': 'An open line cannot divide a cylinder or torus. Draw a closed circle to make a porthole instead.',
  '이 곡면에는 타원을 그릴 수 없습니다': 'You cannot draw an ellipse on this surface',
  '벽 생성 실패 (형상이 유효하지 않음)': 'Could not create the wall (the shape is not valid)',
  'NURBS 패치 재생성 실패': 'Could not rebuild the NURBS patch',
  'window.__AXIA_DEBUG = true 후 다시 확인하세요.': 'Set window.__AXIA_DEBUG = true and check again.',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  '다각형 변의 수 (3~24)\n\n6 = 육각형 (벌집/기하)\n5 = 오각형\n8 = 팔각형 (볼트 헤드)': 'Number of sides (3–24)\n\n6 = hexagon (honeycomb, geometric)\n5 = pentagon\n8 = octagon (bolt head)',

  // ── batch 8 — backtick templates in already-migrated files.
  // The strong guard only read 'single quotes', so 185 `${n}개 …`
  // templates sat in files the ledger called done. Guard now reads
  // backticks too; these are the keys that turned up.
  'IFC 가져오기는 준비중입니다.\n': 'IFC import is not ready yet.\n',
  '대안: Revit / ArchiCAD 에서 OBJ 또는 DXF 로 내보낸 뒤 가져오세요.': 'Alternative: export to OBJ or DXF from Revit / ArchiCAD, then import that.',

  // ══════════════════════════════════════════════════════════════════════
  // batch 13 — the status-bar tool label (toolDisplayNames.ts)
  //
  // The tool names were English-only, so a Korean user clicked 「사각형」 and
  // the status bar said "Rectangle". The survey never saw it: the scanner
  // hunts raw Korean, and hard-coded English has none. survey 0 means "no
  // raw Korean", not "i18n complete" — this is the gap.
  //
  // 52 of the 64 names already had keys here, because a tool name is the
  // same word the menu uses. Only these 12 are new.
  // ══════════════════════════════════════════════════════════════════════
  '다각형 구멍': 'Polygon Hole',
  '분할': 'Split',
  '슬라이스': 'Slice',
  '경계': 'Boundary',
  'NURBS 편집': 'NURBS Edit',
  // Views keep the axis hint: with Z-up (LOCKED #43), "Top" being XY is not
  // self-evident, and the drawing vocabulary (평면도/정면도) is the Korean
  // AxiaCommands already uses.
  '평면도 (XY)': 'Top (XY)',
  '저면도 (XY)': 'Bottom (XY)',
  '정면도 (XZ)': 'Front (XZ)',
  '배면도 (XZ)': 'Back (XZ)',
  '우측면도 (YZ)': 'Right (YZ)',
  '좌측면도 (YZ)': 'Left (YZ)',

  // ── batch 13b — help rows added when the key collisions were fixed ──
  'Polyline (폴리선)': 'Polyline',
  'Freehand (자유선)': 'Freehand',
  '명령 입력줄 열기/닫기': 'Toggle command input',
  '명령 팔레트 (Ctrl+Shift+P 도 동일)': 'Command palette (Ctrl+Shift+P too)',

  // ── batch 13c — BoundaryTool surfaced (ADR-148 β-4) ──
  // The tool answered Ctrl+B since it landed but appeared in no menu, no
  // catalog and no help sheet. Not named 「경계 도구」: resynthesize-faces
  // already carries that (LOCKED #64 B-γ) and the two would collide.
  '영역 클릭 → 면 (Boundary · BPOLY)': 'Click a region → face (Boundary · BPOLY)',
  '📍 영역 클릭 → 면 (Boundary · BPOLY)': '📍 Click a region → face (Boundary · BPOLY)',
  'Boundary (영역 클릭 → 면)': 'Boundary (click a region → face)',
  '영역면': 'Region',

  // ── batch 13d — group-* stopped failing silently ──
  // The right-click menu hides these unless the selection is in a group, so
  // saying nothing was survivable there. The palette shows every command.
  '그룹 안의 면을 먼저 선택하세요': 'Select a face inside a group first',
  '선택한 면은 그룹에 속해 있지 않습니다': 'The selected face is not in a group',

  // ── batch 13f — the right-click half of Boundary (ADR-148 §2.3 b) ──
  // Two keys, because they are two strings: index.html carries the emoji, the
  // ActionCatalog label does not. The Capability Explorer renders the catalog
  // label, and its guard caught the missing one immediately.
  '📍 이 영역에 면 만들기': '📍 Make a face here',
  '이 영역에 면 만들기': 'Make a face here',

  // ── batch 13g — 3D BOUNDARY (ADR-148 §5): selects, does not create ──
  '🧊 이 솔리드 전체 선택': '🧊 Select this whole solid',
  '이 솔리드 전체 선택': 'Select this whole solid',
  '닫힌 솔리드 안이 아닙니다': 'Not inside a closed solid',
  '솔리드 선택: {n}개 면': 'Solid selected: {n} faces',
};

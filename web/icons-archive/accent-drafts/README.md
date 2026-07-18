# 강조색(Accent) 아이콘 드래프트 — WIP

툴바 아이콘에 **강조색**(예: 빨강 — 도구가 작용하는 부분만)을 넣는 방안의 **작업/보관 영역**입니다.
AutoCAD/BricsCAD 관례처럼 "무엇에 작용하는지"를 색으로 보여줍니다.

> ⚠️ **아직 엔진(`web/index.html` 툴바)에는 적용하지 않았습니다.**
> 여기 모아서 → 선별 → **일괄 적용**할 예정입니다.

## 파일

| 파일 | 내용 |
|---|---|
| `accent-demo.html` | 브라우저로 여는 데모 — 대표 도구 9종의 **현재(단색) vs 빨강 강조** 비교. |

## 적용 방식 (나중에 일괄 적용 시)

CSS에 accent 클래스 한 쌍만 추가하면 됩니다 (지금 `stroke:currentColor` 강제 때문에 클래스 필요):

```css
.tool-btn svg .accent,      .tool-dropdown-item .tdi-icon svg .accent      { stroke: #ef4444; }
.tool-btn svg .accent-fill, .tool-dropdown-item .tdi-icon svg .accent-fill { fill: #ef4444; stroke: none; }
```

그다음, 강조할 요소에 `class="accent"`(선) 또는 `class="accent-fill"`(면)만 붙입니다. 나머지는 흰색 유지.

## 남은 결정 (일괄 적용 전)

1. **도입 여부** — 2톤 아이콘 시스템 채택할지
2. **강조색** — 빨강 `#ef4444` 고정? 라이트/다크 양쪽 가독성 확인
3. **적용 범위** — 전체 vs 편집계열(필렛/챔퍼/오프셋/불리언/포켓 등)만 vs 선별 몇 개
4. **강조 의미 통일** — "작용 대상"만 빨강 (축·결과·제거대상 등 일관 규칙)

선별이 끝나면 위 CSS + 선택된 아이콘들의 `class="accent"`를 **한 번에** `index.html`에 반영합니다.

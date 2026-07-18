# 툴바 아이콘 아카이브 (Toolbar Icon Archive)

현재 `web/index.html` 툴바에 쓰이는 **모든 아이콘의 스냅샷**입니다.
아이콘을 교체해도 이전/현재 디자인을 잃지 않고 나중에 재사용하려고 보관합니다.

## 구성

| 항목 | 설명 |
|---|---|
| `svg/<key>.svg` | 아이콘별 **독립 실행형 SVG** (73종). `currentColor` 사용 → `color` CSS 로 색 지정. 디자인 툴·다른 프로젝트에 바로 재사용 가능. |
| `icons.json` | `{ key, viewBox, style, inner }` 배열. 코드에서 프로그램적으로 재사용. |
| `index.html` | 브라우저로 여는 **카탈로그** — 카테고리별로 전 아이콘 미리보기 + 파일 경로. |
| `generate.mjs` | 재생성 스크립트. |

`key` 는 `data-tool` / `data-action` / `data-toggle` / 버튼 id 입니다
(예: `wall`, `sphere`, `bool-union`, `inspector`). 같은 key 의 변형은 `-2` 접미사.

## 재생성

아이콘을 바꾼 뒤 아카이브를 갱신하려면 (jsdom 필요, 이미 devDependency):

```bash
cd web
node icons-archive/generate.mjs
```

`index.html` 에서 툴바 아이콘을 다시 읽어 `svg/`, `icons.json`, `index.html` 을 다시 씁니다.

## 참고

- 이 폴더는 **소스 아카이브**입니다 — `public/` 이 아니라서 프로덕션 번들에 포함되지 않고, Vite 빌드 엔트리도 아닙니다.
- 교체되어 사라진 **이전(원본) 디자인**은 git 히스토리(PR #12 이전 커밋)와 아이콘 선택기 아티팩트에서 복구할 수 있습니다.

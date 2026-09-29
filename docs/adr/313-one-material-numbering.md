# ADR-313 — One material numbering: the material the user picks is the material the engine records

**Status**: Draft
**Date**: 2026-09-29
**Category**: 시민권 / 배선
**Scope**: renumbers the engine's built-in materials, migrates files saved under
the old numbering, and wires the four places where the app and the engine held
two different answers about a face's material. Touches the meaning (not the
value) of `FORM_MATERIAL` (LOCKED #26), the built-in id range (ADR-098,
LOCKED #37) and the words of ADR-100's fallback (LOCKED #38) — see §6.

사용자 결재 (2026-09-29):

> 재질 번호 — "엔진을 1..12 로" (앱 번호에 맞추고 0 은 '재질 없음' 전용)
> 범위 — "재질 배선 전체" (번호 정렬 + 열기/가져오기/실행취소 후 재질 복원 +
> Quick Color 엔진 등록 + 재질 부여 시 실제 승격, 결함마다 커밋과 가드, PR 하나)

---

## 1. How it was found

The wiring audit (#277–#279) walked the map backwards — bridge methods nothing
calls. `getAllMaterials` was one of them, and using it to read what the engine
records turned up the rest. Every number below is measured in a real Chromium
against the real engine (`npm run build`, Playwright), not read off the code.

## 2. What was measured

### 2.1 Every material the Inspector offers is recorded as the next one

Draw a sheet, select it, pick each material in the Inspector's dropdown, read
`getFaceMaterial` and look the id up in the engine's own library:

| picked in the Inspector | engine records | the engine calls it |
|---|---|---|
| 콘크리트 (Concrete) | 1 | 강철 |
| 철강 (Steel) | 2 | 목재 |
| 목재 (Wood) | 3 | 유리 |
| 유리 (Glass) | 4 | 벽돌 |
| 벽돌 (Brick) | 5 | 알루미늄 |
| 알루미늄 (Aluminum) | 6 | 석재 |
| 석재 (Stone) | 7 | 석고 |
| 석고보드 (Gypsum Board) | 8 | 단열재 |
| 단열재 (Insulation) | 9 | 물 |
| 물 (Water) | 10 | 흙 |
| 토양 (Soil) | 11 | 타일 |
| 타일 (Tile) | **0** | (nothing — id 12 does not exist, the assign failed) |

A concrete box exports as `IFCMATERIAL('강철')`. The app draws it concrete-grey
from its own table, so nothing on screen says anything is wrong; the only
consumers of the engine's answer are the IFC export and anything reading the
engine directly (MCP).

**Why**: two tables. The app numbers its built-ins 1..12 and treats 0 as "no
material" (`syncFromRust`: *"Rust에서 material 0(기본) → TS에서 해제"*). The
engine's `MaterialLibrary::new()` starts `next_id` at 0, so Concrete got 0 —
**the same number as `FORM_MATERIAL`**, the form-layer sentinel for "no
material". The engine's own comment admits it: *"FORM_MATERIAL sentinel (id 0 =
Concrete)"*. So in the engine Concrete was unrepresentable: the export skips
`FORM_MATERIAL`, promotion refuses it, and an imported concrete member loses its
material.

### 2.2 A reopened project has lost its materials — on screen

Assign 벽돌, save (`file-save`), reload the page, open the file through the real
file chooser:

```
before save   viewport #c45a3a   engine 5
after open    viewport #e8e8e8   engine 5   Inspector ""   badge "형태 (Shape)"
```

The engine kept the id. The app never asks for it: the viewport colours faces
from the app's `MaterialLibrary` assignments, and nothing rebuilds those from
the engine after `importSnapshot`. `syncFromRust` (called after undo/redo) only
re-reads faces the app already knew about.

### 2.3 An imported IFC material never reaches the screen

Export a steel and a brick member (promoted with the engine's own ids), rewrite
steel to 콘크리트 in the text, import into a fresh page:

```
brick member      engine 4 벽돌     XIA 1      viewport #e8e8e8   Inspector ""
concrete member   engine 0          XIA none   viewport #e8e8e8   Inspector ""
```

ADR-311's *"이름이 색을 되살린다"* holds inside the engine and nowhere a person
can see it. The concrete member lost its material outright — `find_by_name`
found 0, which is `FORM_MATERIAL`, and promotion refused it.

### 2.4 Quick Colour is not in the engine, and an unrelated undo erases it

```
assign #ff0000              viewport #ff0000   engine 0
draw something else, undo   viewport #e8e8e8
```

`assign-quick-color` invents an id ≥ 10001 on the belief (its comment) that
*"Rust 엔진은 rustId를 opaque u32로 저장"*. It does not: `AssignMaterial`
refuses an id the library does not hold. The undo's `syncFromRust` then reads 0
for the face and drops the colour.

### 2.5 The Inspector says "XIA (특성)" and there is no XIA

Extrude a closed box, select all six faces, pick 콘크리트:

```
badge            "XIA (특성)"   hint hidden
getXiaForFace    -1 on all six
xia ids          []
owner            Shape 1, as before
```

`index.html` promises *"재질을 부여하면 이 객체는 XIA (특성)로 승격됩니다"*, and
ADR-050's own stack diagram routes the Inspector through
`promote_shape_to_xia`. The badge reads the app's local material state, and
`promoteShapeToXia` has **no caller in the app** — the MCP server's `create_xia`
and the IFC importer call it, the browser never does. So ADR-091's demotion
(재질 제거 → 형태로 강등) and ADR-100's recovery have never run on anything a
person drew: there was never an XIA to demote.

## 3. Decision

**D1 — one numbering.** The engine's built-ins are **1..=12** in the same order
the app lists them (Concrete 1 … Tile 12). `FORM_MATERIAL` stays `0` and now
means only "no material" — it is not a material and the library holds no entry
for it.

**D2 — files saved under the old numbering are migrated on load.** Detect the
old layout by the library itself (a built-in stored at id 0), then:

1. the library's built-ins 0..=11 move to 1..=12 (the structs move, so any
   channel a user uploaded onto a built-in goes with it); a legacy custom at id
   12, if any, moves to a fresh id;
2. an XIA's primary material in 1..=11 moves +1 — only the engine's own paths
   (promotion from IFC import, MCP `create_xia`) ever wrote one, in the engine's
   numbering (§2.5: the app never promoted);
3. a face owned by such an XIA whose material equals that XIA's old primary
   moves +1 — it was written by the same path;
4. every other face keeps its id: the app wrote it in the app's numbering, which
   is now the engine's. `0` stays `0`.

**D3 — the app reads materials back from the engine** after open (both file
formats), IFC import and undo/redo — every face, not only the ones it already
knew — and mirrors engine materials it has no entry for (IFC imports, Asset
Library) so they can be drawn.

**D4 — Quick Colour is an engine material.** It is created in the engine (Project
tier, so it is saved with the file) and the app uses the id the engine returns.

**D5 — assigning a material promotes the owning Shape** when the four conditions
hold (ADR-050), and the badge reads the engine's answer, not the app's local
state. A refused promotion says why (PromoteError text) and the face keeps its
face-level material on a Shape.

## 4. What is not changed, and why

- **Names and physical values differ between the two tables** — the app says
  철강 / 석고보드 / 토양 where the engine says 강철 / 석고 / 흙, and some values
  differ (concrete thermal conductivity 1.6 vs 1.4). They are the same materials,
  so this is synonyms, not a wrong material. Renaming the engine's names would
  stop ADR-311's `find_by_name` matching files already exported with them, so it
  is left as a separate decision.
- **The Asset Library's materials do not appear in the Inspector's dropdown**
  unless D3 mirrors them in; whether the panel should also assign is a UI
  decision this ADR does not make.

## 5. Residuals (stated, not fixed)

- An **IFC-imported open shell** saved before this change (a member that carried
  a material but failed promotion, so it stayed a Shape) keeps its face ids and
  now reads as the next material. Nothing in the saved file tells it apart from
  a face the app assigned.
- A face of an IFC-imported XIA that the user later reassigned, via the
  Inspector, to exactly the id of that XIA's primary, is moved with it.
- A **concrete member imported from IFC before this change** comes back without
  a material — the import had already dropped it (§2.3). Importing the IFC file
  again now keeps it.

## 6. LOCKED policies touched

| policy | before | after |
|---|---|---|
| LOCKED #26 `FORM_MATERIAL = MaterialId::new(0)` | value 0; also Concrete | value 0; **only** "no material" |
| LOCKED #37 ADR-098 built-in range | `0..=11` (`BUILTIN_MATERIAL_ID_MAX = 11`) | `1..=12` |
| LOCKED #38 ADR-100 Pass 2 "fallback Concrete (id 0)" | "reassign to Concrete" | "reassign to `FORM_MATERIAL`" — the code already did exactly this; the words said Concrete because 0 was Concrete |

## 7. Lock-ins

- **L-313-1** Built-ins are 1..=12; `FORM_MATERIAL` (0) is never a library entry.
- **L-313-2** The app's `rustId` for a built-in equals the engine's id for the
  material whose `name_en`, lower-cased, is the app's `id` — held by a Rust test
  that builds the real library and parses the app's table.
- **L-313-3** Migration triggers only on the old layout and is idempotent.
- **L-313-4** A face's material is read from the engine after every load and
  every undo/redo — the app never keeps a material the engine does not hold.
- **L-313-5** The Inspector's badge reads the engine's owner of the face.
- **L-313-6** Every guard is mutation-checked; absolutely no `#[ignore]`.

## 8. Related

ADR-049 §4 Q4 (default_material 폐지) · ADR-050 (P-2 promote, P-5e-β
FORM_MATERIAL, P-6 badge) · ADR-091 (demotion) · ADR-098 (tiers, section 9) ·
ADR-099 (layered channels) · ADR-100 (recovery) · ADR-311 (IFC member import) ·
the wiring audit #277 / #278 / #279 · LOCKED #26 #37 #38 #44 · 메타-원칙 #4
#6 #13.

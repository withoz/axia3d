# ADR-313 — One material numbering: the material the user picks is the material the engine records

**Status**: Accepted (2026-09-29 — D1~D5 landed, each with a mutation-checked guard; §9)
**Date**: 2026-09-29
**Category**: 시민권 / 배선
**Scope**: renumbers the engine's built-in materials, migrates files saved under
the old numbering, and wires the places where the app and the engine held two
different answers about a face's material — and about who owns the face. Touches the meaning (not the
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

### 2.6 A material pick is not an undo step

Found while wiring D5, through the Inspector:

```
an extruded box               6 faces
pick 콘크리트                   6 faces, engine material 1
undo once                     1 face,  engine material 0
```

`Command::AssignMaterial` sets face materials and records nothing, so the undo
right after a pick went back to the step before it and took the extrude — with
the material. Removing a material was the same. It matters to D5 directly:
ADR-091's "되돌리기" is one undo, and it can only give back what was recorded.

### 2.7 Removing a XIA's material never demoted it

A XIA made by hand (`promoteShapeToXia`, since the app made none), then "없음"
in the Inspector:

```
face material     0
getXiaForFace     2 — the XIA stayed
toast             "재질 제거 시 1건 강등 실패 (나머지는 적용됨)"
badge             "형태 (Shape)" — reading the app's state, so wrong again
```

ADR-091's trigger is the XIA's own material (L1: `xia.material ==
FORM_MATERIAL`). Removing a material cleared only the faces, and
`attemptMaterialRemovalDemote` then asked the engine for a demotion it refuses
by design. Nothing wrote the XIA's material; engine tests set the field by hand
before demoting, which is why they passed.

### 2.8 Promoting the same Shape twice makes two XIAs

`promoteShapeToXia` on the same Shape, twice: XIA 2, then XIA 3, over the same
faces. The Shape stays on after promotion (ADR-050 P-2-c) and nothing checks
for its link. The IFC export still wrote one element (measured): it gives each
face to the first owner that lists it — XIAs before Shapes, in id order — and
skips an owner left with no face (`export_ifc_model`, `claimed`). So neither
the preserved Shape nor a second XIA shows there.

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

**D5 — a material pick is one undo step, and moves its owner when it leaves no
doubt.** As drafted, D5 read "assigning a material promotes the owning Shape when
the four conditions hold". Implementing it found §2.6–§2.8, and the rule became
precise:

- The app's pick and removal go through `Scene::assign_material_to_faces` /
  `remove_material_from_faces`, which record **one** transaction each, owner
  moves included, and run `Command::AssignMaterial` / `RemoveMaterial` inside
  it. The commands are unchanged.
- Inside that step, an owner of a picked face follows the pick only when the
  pick leaves no doubt about the owner as a whole:

  | after the pick | the owner |
  |---|---|
  | every live face of a **Shape** on the picked material | promoted with it as primary — ADR-050's four conditions decide; a refusal comes back with a reason code and the faces keep the material |
  | every live face of a **XIA** on the picked material | it is the XIA's primary |
  | every live face of a **XIA** on no material (a removal) | the XIA's material goes with them and it is demoted (ADR-091) |
  | anything less | face-level; no owner moves |

  "Anything less" includes a box whose faces carry two materials: which one is
  the box's is not the engine's to guess (메타-원칙 #16). Selecting the box and
  picking one material makes it whole.
- A Shape already promoted is skipped (§2.8), so a second pick never makes a
  second XIA.
- The badge asks the engine who owns the faces. A refused promotion is shown
  with its reason; a demotion with ADR-091's 5-second 되돌리기, which — being one
  step — gives back both the material and the XIA.

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
- **The plain commands** (`assign_material` / `remove_material`, and
  `Command::AssignMaterial` / `RemoveMaterial` beneath them) still record
  nothing. The app reaches the plain exports only through the bridge's
  fallback for an engine built before the new entries; the new entries run
  the commands inside their own transaction, and a few Rust tests call the
  plain exports directly.
- **Quick Colour and the texture dialog** move owners by the same rule (they
  go through the same pick) but do not show a refusal — the promise of
  promotion is the Inspector's, so the Inspector is where it is explained.

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
- **The raw `promoteShapeToXia` still promotes a promoted Shape again** (§2.8).
  The app's pick skips it; MCP `create_xia` can still make a second XIA over the
  same faces. Whether a second promote should refuse, or return the first XIA,
  is a decision for the MCP surface.
- **The Inspector's hint** still reads *"재질을 부여하면 이 객체는 XIA (특성)로
  승격됩니다"* without a condition. A sheet is refused and the refusal says why;
  rewording the hint touches its split-text-node translation test and is left
  to its own change.
- **Creating a material is not an undo step.** Undoing past a Quick Colour
  takes the colour off the face but leaves the Project material in the library,
  unused.
- **`export_baseline.txt` lacks `vertexAt`**, which #278 added. The baseline is
  a subset guard, so `vertexAt` could be deleted with it green. Found here, left
  for its own change.

## 6. LOCKED policies touched

| policy | before | after |
|---|---|---|
| LOCKED #26 `FORM_MATERIAL = MaterialId::new(0)` | value 0; also Concrete | value 0; **only** "no material" |
| LOCKED #37 ADR-098 built-in range | `0..=11` (`BUILTIN_MATERIAL_ID_MAX = 11`) | `1..=12` |
| LOCKED #38 ADR-100 Pass 2 "fallback Concrete (id 0)" | "reassign to Concrete" | "reassign to `FORM_MATERIAL`" — the code already did exactly this; the words said Concrete because 0 was Concrete |
| LOCKED #26 Phase 2 — ADR-091 D-δ | the Inspector attempted the demotion after the removal (`citizenship/MaterialRemovalDemote.ts`); refused every time (§2.7) | the engine demotes inside the removal's own step; the module is deleted. ADR-091's trigger (L1), toast (L5) and two entry points (L6) are unchanged |
| LOCKED #26 Phase 1 — ADR-050 P-6 badge | read the app's material state | reads the engine's owner of the faces |

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
- **L-313-7** A material pick and a material removal are each one undo step,
  and an owner's move is inside that step.
- **L-313-8** An owner moves only when every live face is on the picked
  material (or, for a removal, on none). A mixed owner is face-level.
- **L-313-9** A pick never promotes a Shape that is already promoted.
- **L-313-10** A refused promotion crosses to the app as a stable reason code
  (`promote_reason_code`); the words are the app's.

## 8. Related

ADR-049 §4 Q4 (default_material 폐지) · ADR-050 (P-2 promote, P-5e-β
FORM_MATERIAL, P-6 badge) · ADR-091 (demotion) · ADR-098 (tiers, section 9) ·
ADR-099 (layered channels) · ADR-100 (recovery) · ADR-311 (IFC member import) ·
the wiring audit #277 / #278 / #279 · LOCKED #26 #37 #38 #44 · 메타-원칙 #4
#6 #13.

## 9. Acceptance log

Every number below was measured at that commit's own state (the engine rebuilt
where it changed), not carried over.

| commit | what | guard → the mutation it catches |
|---|---|---|
| `14630d4` | this ADR, measured (§1–§2.5) | — |
| `0318cdb` | D1 + D2 — built-ins 1..=12, 0 = no material; old files migrated on load | `the_app_and_the_engine_number_materials_alike` (the real library vs the app's table) · `a_file_saved_before_the_renumbering` (a snapshot the old engine wrote) · `adr313_*` — each failed on the code before it. cargo 3982 / 0 |
| `517c38d` | three E2E checks that named concrete by 0 | adr-098 S4 and adr-100 S4 failed; **adr-098 S5 passed with both System-tier guards switched off** (it asked to remove 0, refused as not found) — with 1 it fails (`removeOkSystem: true`) |
| `c4488e0` | D4 — Quick Colour and the texture dialog make materials the engine holds | vitest ×4 (fall back to an invented id → "adds nothing" fails; a call site back on `addCustom` → its check fails) · e2e ×2 (the invented id → engine 0; no reuse → 2 Project materials) |
| `ebd6af6` | D3 — the app reads materials back from the engine | cargo (the `format!` list → "the list must be JSON") · vitest ×5 · e2e ×4 read the renderer's colour attribute (the call removed from `syncMesh` → all 4 fail; no late Inspector option → `""` for `engine-100`) |
| `b97473d` | D5 — a pick is one undo step (§2.6) | cargo ×3 (no transaction in assign → the extrude goes; in remove → the undo lands on the pick) · e2e ×2 through the Inspector (same two) |
| `cb90a44` | D5 — the owner follows a pick that leaves no doubt (§2.7, §2.8) | cargo ×11 (no promotion → 9 fail; the XIA's material kept → the removal leaves it; no already-promoted skip → 2 XIAs) · cargo reason codes (renamed → fails) · e2e ×4 (badge from the app's state → sheet and one-face fail; no report → no reason, no 되돌리기) |

Suites at the final code (after the last comment-only commits, measured then):
cargo `--workspace --no-fail-fast` **3998 passed / 0 failed / 30 ignored** (the
same 30 as before this ADR) · tsc 0 · vitest **3185 passed / 1 skipped** ·
Playwright, the whole suite, **312 passed / 1 skipped / 0 failed** (17.8 min).

One observation, not a defect: the first vitest run after a container restart
timed out once in `StepIgesImporter` (5.5 s against 5 s); it passed alone twice
and in the next full run. It does not touch materials.

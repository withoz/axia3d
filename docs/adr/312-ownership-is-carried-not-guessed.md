# ADR-312 — Ownership is carried, not guessed

**Status**: Proposed
**Date**: 2026-08-11
**Category**: 시민권 / 기하 원리
**Scope**: states the model; wires nothing. Amends the wording of meta-principles
#14 and #16 (see §5) — the originals are not rewritten in place, per 메타-원칙 #10.

사용자 결재 요청 (2026-08-11):

> "xia와 일급시민권의 관계를 총체적으로 정립해서 엔진을 전체검토할것"
> "코드,커널 배선과 혹시 잘못된 개념정립도 있는지 전체 기하연산의 원리에서 확인해줘"

The complaint that started it:

> "면과 선이 입체 모서리와 면이 만날때도 면분할이나 면생성이 문제없이 되고
> 면을 잃어버리거나하는 일도 없는데 우리엔진은 아직도 면이 사라지거나
> 분할등이 안되는 전체적인 문제가 많아요"

---

## 1. First measurement: the splitting works

Driven in the running app, one case per fresh page (a 200³ box):

| gesture | faces | closed | verdict |
|---|---|---|---|
| line, both ends INSIDE the top face | 6 → 6 | ✓ | correct — no closed boundary, no face |
| line, edge to edge | 6 → **7** | ✓ | splits |
| line, overshooting past the face | 6 → **7** | ✓ | splits |
| line, over a solid EDGE onto the next face | 6 → 7 → **8** | ✓ | both split |
| rect, inside the face | 6 → **7** | ✓ | splits |
| rect, straddling the boundary | 6 → **8** | scene false | correct — the hanging sheet has free edges |

**The geometry is not the problem.** What is wrong is the same gesture's effect on
*who owns the result*:

```
box + one line across its top
    isSolid   true → false
    volume    8,000,000 → 7,333,333        (the geometry did not change at all)
    ownership the Xia's list and the reverse index give DIFFERENT answers
```

A user reads that as "the solid lost a face". Nothing was lost; the face stopped
being *theirs*.

## 2. Why: ownership is keyed by a thing that keeps being destroyed

`Shape.face_ids` / `Xia.face_ids` and the reverse maps `face_to_shape` /
`face_to_xia` are all keyed by `FaceId`. Almost every operation destroys faces and
makes new ones — ids are never reused (`storage.rs`, monotonic `next_id`). So
ownership has to be **re-attached after every op**, and the engine does it by
guessing:

- `capture_coplanar_owners` snapshots each owner's 2D polygon *area* before the
  re-derive; `adopt_retiled_faces` then point-in-polygons each new face's centroid
  and gives it to the **smallest enclosing** previous owner.
- When that is ambiguous it **gives up**: the XIA path sets `selected_xia = None`
  ("모호하면 face-only"), the Shape path reconciles "ONLY when the re-derived region
  belongs to a SINGLE shape".

The code says so itself, in the header of the function written to patch it:

> *"the pieces it puts back arrive owned by nobody — measured 2026-08-06,
> `face FaceId(11) belongs to nobody` after two circles on a box top."*

**This is exactly the heuristic-automation pattern meta-principle #16 forbids,
applied to LOCKED #26 citizenship.**

## 3. The conceptual error underneath

Meta-principle #14 says a face is *derived from* its closed boundary, and ADR-019
says **"Line is Truth, Face is Byproduct"**.

A `Face` carries eight fields that cannot be recovered from its boundary:

| field | derivable from the boundary? |
|---|---|
| `normal` | yes (Newell) |
| `material` | **no** |
| `surface` (Plane/Cylinder/Sphere/Cone/Torus) | **no** |
| `flags` (selection) | **no** |
| `visible`, `double_sided`, `tolerance` | **no** |
| `parent` | **no** — and dead: no caller outside `face.rs` |
| — and **ownership**, held outside the face | **no** |

The proof is in the re-derive path itself: it snapshots
`DirtyInfo { material, surface, polygon, area }` *before* destroying faces, then
**guesses** the new face's material and surface from the smallest enclosing old
one. A byproduct would have nothing to snapshot.

CLAUDE.md concedes half of this inside #14 already — face surface metadata is "an
input to kernel-aware ops". **Something that is an input is not a byproduct.**

So the principle is not wrong in spirit, it is wrong in *scope*:

> **A face's BOUNDARY is derived. A face's ATTRIBUTES — including who owns it —
> are inherited, and inheritance has to be carried explicitly, because there is
> nothing in the boundary to recover them from.**

## 4. The wiring gap this predicts, and the audit found

Ownership is reconciled only where somebody wired it by hand. `Scene::move_face_owner`
exists precisely to hand an owner from a destroyed face to its replacements —
**and not one of the ~60 face-changing WASM ops calls it.** Unrouted families:

boolean (5) · merge (6) · split (2, incl. `split_face_by_line` — there is no
`Scene::split_face*` wrapper at all) · fillet / chamfer / offset / recess (8+) ·
sweep / loft / revolve / subdivide / bezier / nurbs (7) · array / mirror (6) ·
erase-resynthesize (2) · STEP `inject_external_face_*` (2) · demos (12+).

Three further facts, all measured:

- **No mutual exclusion is enforced anywhere.** A face can be in `face_to_shape`
  and `face_to_xia` at once. The doc comment claiming exclusivity is unenforced,
  and every `assert!` naming it is inside `#[cfg(test)]`.
- **`promote_shape_to_xia` produces exactly that state on purpose** — it preserves
  the Shape and never removes `face_to_shape`. After any promotion every face has
  two owners; `solid_owner_of` absorbing it with "prefer Xia when both" is the
  tell that the code already expects it.
- **`orphan_recovery` is Xia-only** — it classifies a healthy *Shape-owned* face as
  an orphan and would wrap it in a new "Recovered" Xia, manufacturing the dual
  state it was meant to clean up.
- **The Reference citizen is inert.** Nothing in the engine creates one:
  `DrawCenterline` sets an `EdgeClass` and never touches `references`; STEP
  injection writes faces with no citizenship record at all. The `locked` flag is
  read in zero places, so ADR-095's "op operand 거부" is not implemented.

## 4b. Three smaller drifts found in the same sweep

Recorded here rather than fixed, because each needs its own measurement:

- **A Y-up leftover after the Z-up migration.** `scene.rs:7985` —
  `let target = surface_normal.unwrap_or(DVec3::Y)` is the winding-enforcement
  fallback when no hint is supplied. ADR-103 made the engine Z-up. Changing it
  would flip winding for hint-less faces, so it wants a measurement first, not a
  blind edit.
- **`Face::parent` is dead and serialized.** No caller outside `face.rs`
  (`GroupManager::set_parent` is a different thing), yet it round-trips in every
  `.axia`. If faces were byproducts a face-hierarchy pointer would not exist at
  all — it is a vestige of the model this ADR is correcting.
- **`normal` is the only Face cache that persists.** `surface_version`,
  `boundary_version` and `normal_cache` all carry `#[serde(skip)]` per ADR-061;
  `normal` does not. ADR-007 calls the normal a cached result of winding, and
  `cleave.rs` writes a *pre-topology-change* normal back over the winding-derived
  one as a "safety net" — the clearest inversion of ADR-007 in the tree.

## 5. What this ADR proposes

**D1 — Restate meta-principle #14.** Keep "면은 닫힌 경계로부터 유도된다" as the
statement about *boundaries*. Add: *attributes and ownership are inherited, not
derived, and every op that destroys a face owes its replacements an explicit
inheritance.* The current wording ("Face 는 first-class entity 가 아닌 byproduct")
is false as measured and should be marked amended.

**D2 — Restate meta-principle #16's scope.** Its canonical sentence is
epistemic — *"자동화는 사용자 의도를 미리 알 수 없다"*. ADR-176 revived the three
auto-behaviours arguing *"견고해졌으니"*, which answers a robustness claim #16 never
made, and ADR-283 then revived containment split too. Either #16 means "no
heuristic auto-trigger" (and ADR-176/283 violate it), or it means "no *fragile*
auto-trigger" (and #16's sentence is misstated). **The de-facto reading is the
second; #16 should say so.**

**D3 — One owner per face, enforced.** Exactly one of Shape / Xia / Reference owns
a face. Add a debug-time invariant, and make `promote_shape_to_xia` move ownership
rather than duplicate it.

**D4 — Ownership travels with the op, not after it.** Every face-creating op takes
the owner of what it consumed and gives it to what it produced. `move_face_owner`
is the mechanism; the work is wiring the ~60.

**D5 — Guessing is a fallback that must be visible.** Where inheritance genuinely
cannot be determined (a face with no predecessor), say so — do not silently leave
it unowned. `detect_topology_damage` already classifies these as `Orphan`; nothing
surfaces it.

**D6 — Reference is either implemented or removed.** It has indexes, a guard and a
`locked` flag that nothing reads. Wire `DrawCenterline` and STEP import to it, or
say plainly that it is not yet a citizen.

## 6. What was already fixed on the way here

- Drills / punches / crossing bores now reconcile ownership (#115) and stop naming
  dead faces (#117).
- One dead element no longer takes the whole IFC file down (#118).
- A line drawn across a solid's face no longer takes that face away — the split
  pieces stay with whoever owned the face they came from (PR #119).

The rect path still does the opposite (the inner region goes to the drawn Shape,
leaving the solid open), which is the first question D4 has to answer and is
**deliberately left open** for a product decision.

One lesson from that fix belongs here, because it constrains every wiring step D4
implies: **the hand-back has to happen after the arrangement, not during it.** The
post-draw arrangement reads the ownership maps to decide what to absorb, so
changing ownership mid-flight changes its answer — measured at 5,260,000 mm²
against a true 4,940,000. Ownership is carried *through* an op, not rewritten
inside one.

## 7. Lock-ins

- **L-312-1** The measurements in §1 and §2 are the acceptance evidence; a fix that
  does not move them has not fixed this.
- **L-312-2** D1/D2 are wording amendments to meta-principles, so per meta-principle
  #10 they land as this ADR plus an amendment note at each site — the original text
  is not rewritten in place.
- **L-312-3** D3's invariant is debug-time first. Making it a hard error before the
  ~60 ops are wired would fail on ordinary use.
- **L-312-4** No behaviour change ships under this ADR. It states the model; each
  wiring step is its own ADR with its own measurement.
- **L-312-5** 절대 #[ignore] 금지.

## 8. Cross-link

메타-원칙 #4 (SSOT) · #6 (measure first) · #10 (ADR 불변) · #14 · #15 · #16 ·
LOCKED #1 / #12 / #26 / #41 / #64 / #76 · ADR-019 · ADR-049 / 050 (citizenship) ·
ADR-095 (Reference) · ADR-139 · ADR-176 · ADR-203 §36 (ownership reconcile) ·
ADR-283 · PR #115 / #117 / #118.

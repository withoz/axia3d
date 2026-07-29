# ADR-305 — CLAUDE.md doc-lag, and a LOCKED number claimed twice

**Status**: Accepted
**Date**: 2026-07-29
**Category**: Governance / documentation

## 1. Ten accepted ADRs were invisible where it matters

Measured on 2026-07-29: `ADR-295` … `ADR-304` appear in `CLAUDE.md` **zero
times**. CLAUDE.md is loaded into every session as project instructions, so an
ADR absent from it is, to a new session, an ADR that does not exist.

That is not hypothetical. LOCKED #88 exists because it already happened: a
session read a CLAUDE.md that stopped at ADR-263 while the code had reached
ADR-274, and **misdiagnosed working code as broken**. The gap has reopened, and
one of the newly-invisible entries (ADR-297) is a design that was *measured
wrong and corrected* by ADR-298 — precisely the kind of thing a future session
must not rediscover the hard way.

## 2. A LOCKED number that meant two different things

`### 75.` appeared twice, for two **different** ADRs:

| line | entry |
|---|---|
| 5739 | `### 75. ADR-175 — Face-Hit Drawing Plane` |
| 8575 | `### 75. ADR-174 — Curve-Edge Crossing-Split` |

Both had live downstream citations, in opposite senses:

| "LOCKED #75" meant | cited by |
|---|---|
| **ADR-175** | `178-*.md` ×2, `179-*.md` ×1, CLAUDE.md ×2 |
| **ADR-174** | `174-*.md` ×5 (itself), `189-*.md` ×2, `190-*.md` ×1, `scene.rs` ×1, CLAUDE.md ×1 |

A bare "LOCKED #75" could not be resolved without reading its context.

`### 35.` also appears twice, but both are ADR-089 — one is a day-earlier
snapshot (A-α~A-Γ) left beside the complete one (A-α~A-Δ, which carries the
ADR-092/093/094 amendments). The number is not semantically ambiguous there.

## 3. Decision

**ADR-175 keeps #75.** It sits in position, between #74 and #76. The ADR-174
entry had been appended after #98 — out of order — and takes **#99**, with its
nine ADR-174-sense citations updated. Content unchanged; only the label moved.

**LOCKED #100** records ADR-295 … ADR-304 as one entry, because they are one
arc: kernel defect-fixing and audit closure, including two cases where a fix
introduced or was corrected by the next ADR. Each row names its ADR as `ADR-NNN`
rather than a bare number, so a future grep for `ADR-296` finds it — being
findable is the entire point of the entry.

**The `### 35.` pair is marked, not renumbered or deleted.** Both refer to the
same ADR, so citations stay unambiguous; the stale one now says which is
canonical and where new content goes.

## 4. What the ten have in common, recorded as lock-ins

The arc has one theme: **a check that passed was holding nothing.** ADR-299's
specs had no `expect`; ADR-301's guard read two of three keymaps; ADR-302's first
two sweeps returned zero because the inputs were too easy; ADR-304's guards were
skipped because `NaN < eps` is silently false.

LOCKED #100 carries seven lock-ins from that, of which the load-bearing ones are:

- **L-100-1** Never guard with `<` / `>` on a possibly-NaN value. Use
  `is_finite()`.
- **L-100-2** Everything decidable before a teardown runs before the teardown.
- **L-100-5** A report claiming "✅ verified" must name a test that asserts it.
- **L-100-6** A sweep that finds nothing states the inputs it used.
- **L-100-7** A defect that reads real but that no input reaches is reported as
  exactly that.

## 5. Lock-ins for this ADR

- **L-305-1** A LOCKED number identifies exactly one ADR. Two entries may share a
  number only when they document the same ADR, and then the stale one says so.
- **L-305-2** Renumbering updates every citation in the same change, and the
  entry that is **in position** keeps the number.
- **L-305-3** A LOCKED entry names its ADRs as `ADR-NNN` in text, not as bare
  numbers in a table cell. Findability is the point.
- **L-305-4** After a run of merged ADRs, check CLAUDE.md coverage before closing
  the session — `grep -c "ADR-NNN" CLAUDE.md` for each. This is the check that
  LOCKED #88 wished had existed.
- **L-305-5** 절대 #[ignore] 금지.

## 6. Verification

`ADR-295` … `ADR-304`: 0 mentions → **1–9 each**. Duplicate LOCKED numbers:
`#75` resolved; `#35` remains but is documented as one ADR under two headings.
Citation retarget: 9 changed to `#99`, 5 left as `#75` — each verified by reading
which ADR the sentence names. `scripts/check-adr-catalog.mjs` passes.

No code changed.

## 7. Remaining

The open items from LOCKED #100 §"아직 열린 것" — the verifier's inner-loop blind
spot (ADR-298), the ~85 `cancel()` Err arms, `compute_normal`'s dead fallback,
whether creation should reject degenerates, IFC's unread `IfcAdvancedFace.
FaceSurface`, and `SplitTool` having no menu entry.

## Cross-link

LOCKED #88 (the doc-lag that caused a misdiagnosis — this closes its recurrence),
LOCKED #100 (the entry this adds), ADR-295 … ADR-304 (its subjects), ADR-294
(the i18n sweep whose guards catch stale UI strings — same discipline, different
surface), 메타-원칙 #4 (SSOT) / #10 (ADR 불변 — headings relabelled, content
untouched).

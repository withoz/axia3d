# ADR-296 — IFC wiring gap closure (generic importer, export tiers, diagnostics)

**Status**: Accepted
**Date**: 2026-07-28
**Category**: Wiring / UX

## 1. Context

The 2026-07-27 wiring audit (which also surfaced the kernel defect fixed in ADR-295)
listed four IFC loose ends. Each was re-measured before being touched; one turned out
not to be a defect at all.

## 2. What was actually wrong

### 2.1 `.ifc` was invisible to "지원되는 모든 유형" (real, user-facing)

`FileImporter`'s `ALL_ACCEPT` had no `.ifc`, so the OS dialog filtered BIM files out of
the generic importer even though the app reads them. Only the dedicated *IFC 가져오기*
menu item worked.

IFC is not reference geometry — it goes into the **engine** as DCEL faces (I-1..I-5),
where every other `FileImporter` format produces a `THREE.Group`. So the fix is a
delegation, not a new loader:

- `IfcImportHandler` gained `importIfcFileObject(file, deps)` — the analyze → classify
  → import body, extracted from the picker so both entry points share one SSOT;
  `importIfcFile` is now just the dialog that calls it.
- `FileImporter.importFile` grew an `ext === 'ifc'` branch that resolves `bridge` +
  `toolManager` from the ServiceContainer (the pattern the STEP DCEL injection already
  used) and delegates, returning an empty group because nothing joins the reference scene.
- `'ifc'` joined `ImportFormat` / `FORMAT_ACCEPT` / `FORMAT_LABEL` / `detectFormat`, so
  `ALL_ACCEPT` picks it up automatically.

While there, `ALL_ACCEPT` stopped hand-appending `.step,.stp,.iges,.igs`: those are
`FORMAT_ACCEPT` entries now, so the list had been emitting them twice.

### 2.2 `exportIfcAdvanced` was shipped but never called (real, dead code)

Its own doc claimed *"the caller (MenuBar) falls back to exportIfc"*, but MenuBar went
`exportIfcModel → exportIfc`, skipping it. The export chain is now three tiers, best
first: semantic model → single-wall analytic brep → faceted brep. The middle tier
matters when the semantic emitter declines but the mesh is still fully analytic — the
opening refill and placement work it does are failure modes a plain whole-mesh brep
does not have.

### 2.3 Openings do **not** accumulate — the audit was wrong (no defect)

The audit claimed recorded openings "can only accumulate" because nothing calls
`clearOpenings`. Measuring the engine says otherwise: `import_versioned_snapshot`
restores openings from snapshot **section 12**, and for a legacy file that has no such
section it runs `self.openings.clear()`. Openings are therefore scoped to the loaded
project already, and `file-new` is a page reload.

So `clearOpenings` / `openingCount` are **diagnostics**, not a missing lifecycle hook.
They were declared on the engine interface with no wrapper, which is its own
inconsistency, so both got thin, engine-optional bridge wrappers — and the docstrings
say plainly that normal project loading needs neither.

### 2.4 Stale catalog copy (cosmetic)

`export-ifc`'s catalog description still read "IFC4.3 (IfcFacetedBrep)", from before the
semantic model, parametric swept solids, materials and openings existed.

## 3. Lock-ins

- **L-296-1** One import SSOT: `importIfcFileObject` is the only place an `.ifc`
  becomes scene geometry; the dedicated dialog and the generic importer both call it.
- **L-296-2** IFC returns an empty `THREE.Group` from `importFile` — it populates the
  engine, never the reference scene.
- **L-296-3** Export tiers are ordered semantic → analytic → faceted, and each tier is
  only asked when the one above declines (mutation-checked).
- **L-296-4** `ALL_ACCEPT` derives entirely from `FORMAT_ACCEPT`; no hand-appended
  extensions.
- **L-296-5** Opening lifecycle stays with the snapshot. The bridge wrappers are
  diagnostics; do not "fix" loading by calling `clearOpenings` from a load path.
- **L-296-6** 절대 #[ignore] 금지.

## 4. Verification

Regressions (+7): `FileImporter.test.ts` (13 formats; IFC present and its accept string
contains `.ifc`), `MenuBar.test.ts` ×3 (each tier reached only when the ones above
decline — **mutation-checked**: disabling the analytic tier fails the middle test),
`WasmBridge.test.ts` ×3 (both wrappers forward, and both are safe when the engine lacks
them). vitest **2954 pass / 0 fail**, tsc 0.

**Live** (real Chromium, production build):
- The generic dialog's accept string is
  `.obj,…,.iges,.igs,.ifc` — no duplicate STEP/IGES entries.
- Feeding a real `.ifc` into that dialog imports into the engine (faces 6 → 12).
- **Control**: the dedicated IFC menu path produces the identical result on the same
  input, so the generic route is genuinely the same path. (Both report
  `invariants: false` for this input because it re-imports a box on top of an identical
  box — coincident geometry, unrelated to the wiring.)
- `bridge.openingCount()` → 0, `bridge.clearOpenings` → function.

## 5. Remaining

The carve APIs on closed-curve caps (ADR-295 §5) and the export tracks listed in
ADR-203 (sphere revolution, steel-section classification, per-vertex `IfcTextureMap`,
MEP) are untouched here.

## Cross-link

ADR-203 (IFC import/export), ADR-295 (the kernel defect from the same audit),
ADR-035 P20.7 (the dynamic-import precedent this delegation follows),
ADR-046 P31 #4 (additive), ADR-294 (the i18n guard that caught the new string),
메타-원칙 #4 / #6.

# ADR-311 — An imported member comes back a member

**Status**: Accepted
**Date**: 2026-07-29
**Category**: IFC / import
**Scope**: step 4 of ADR-309's four — the last one

## 1. Measured, by running the round-trip

A box with the built-in material 벽돌 (Brick, `#c85a54`), exported and read back:

```
--- export (2360 bytes) ---
  IFCMATERIAL x1  IFCRELASSOCIATESMATERIAL x1  IFCSTYLEDITEM x1
  IFCSURFACESTYLE x1  IFCSURFACESTYLERENDERING x1  IFCCOLOURRGB x1
  #36=IFCMATERIAL('\X2\BCBDB3CC\X0\',$,$);
  #38=IFCCOLOURRGB($,0.7843137254901961,0.35294117647058826,0.32941176470588235);

--- import (geometry layer) ---
  element #35 name=Some("Box") material=Some("벽돌") faces=6   warnings: []

--- import (engine) ---
  stats: {"xias":0, ... "faces":6, "groups":5}

--- re-export (1999 bytes) ---
  #35=IFCWALL('2$9yajP4ccgJ4IbCuhZ4Ta',#5,'Model',$,$,#16,#34,$,$);
```

Three separate facts, and only one of them is where I expected it.

1. **The export is complete** — name, material, colour, rendering, the style
   graph. Even the Korean material name survives its `\X2\` escape.
2. **The geometry layer already recovers the material name.**
   `ElementGeometry.material` is `Some("벽돌")` and it reaches the wasm
   boundary intact. `import_ifc` then ignores it and hands every face
   `FORM_MATERIAL` (`lib.rs:5896`).
3. **Imported faces end up owned by nobody.** Not a Xia, not a Shape. The
   re-export's own comment says what happens next: *"Leftover active faces
   (unowned) → one 'Model' wall"* (`lib.rs:5679`). The member's name, its
   material and its IFC type are all gone, and 2360 bytes become 1999.

## 2. The two gaps are one fix

`export_ifc_model` builds its element list from `scene.xias` (name, material,
`xia_element_kind`) and `scene.shapes` (name, `shape_element_kind`), and only
falls back to the 'Model' catch-all for faces neither owns. So **ownership is
what the round-trip is missing** — give an imported member a Shape with its
name and kind, and the re-export emits it correctly without any change to the
export path.

That also settles where the material goes. The export already honours a
face-level material on a form-layer Shape — the code says so:

> A Shape is form-layer (no primary material) but its faces may carry an
> assigned material — honour it so the viewport + export agree.

So the fix is: **a Shape per element, named, kinded, with its faces carrying the
material the file names.**

## 3. Decisions

- **D1 — Shape per element, always.** Ownership is the point; the citizenship
  layer follows from the material, not from the import.
- **D2 — Promotion is attempted, not forced.** ADR-050's rule is material +
  watertight + manifold + volume > 0. An imported closed box meets all four and
  should come back a Xia, as it left. An open shell does not, and must stay a
  Shape rather than be pushed through. Failure is silent-by-design here: it is
  not a drop, it is the model's own answer.
- **D3 — Material by name, find or create.** An exact match on `name` or
  `name_en` reuses the library material — which is how our own files recover
  their *colour* without parsing any appearance at all, since the library
  material carries it. An unknown name creates a Project-tier material
  (ADR-098's tier rule).
- **D4 — Appearance is parsed only for what the name cannot supply.** The style
  graph (`IfcStyledItem → IfcSurfaceStyle → IfcSurfaceStyleRendering /
  IfcColourRgb`) gives a *newly created* material its colour, transparency,
  roughness and metalness. A material matched by name keeps the library's own
  appearance — the file does not get to repaint 벽돌.
- **D5 — Textures are out of scope.** `IfcImageTexture` / `IfcSurfaceStyle-
  WithTextures` are emitted (§40) and would need the image bytes resolved and a
  UV mapping rebuilt. Named separately so it is not mistaken for done.
- **D6 — The element kind round-trips through the existing maps.**
  `shape_element_kind` / `xia_element_kind` already exist and already feed the
  export; import populates them from `ImportedElement.ifc_type`.

## 4. Lock-ins

- **L-311-1** An imported member owns its faces. "Unowned → one 'Model' wall" is
  a fallback for geometry with no member, not the normal path for an import.
- **L-311-2** The name a member is exported under is the name it comes back
  under. Same for its IFC type.
- **L-311-3** A material name matched in the library reuses that material,
  including its appearance. Only an unmatched name creates one, and only then
  does the file's style graph decide how it looks.
- **L-311-4** Promotion to Xia is attempted under ADR-050's four conditions and
  never forced. A member that cannot be promoted stays a Shape with its faces
  materialled — which the export already honours.
- **L-311-5** A style we cannot read is named in `warnings`, like every other
  drop in this module.
- **L-311-6** Textures are not imported (D5). Saying so is part of the contract.
- **L-311-7** 절대 #[ignore] 금지.

## 5. Sub-steps

| step | meaning | regression |
|---|---|---|
| **α** | this document | — |
| **β-1** | a Shape per element, named and kinded | an imported member is owned; a re-export emits it by name and type, not as 'Model' |
| **β-2** | material by name, find or create; promotion attempted | 벽돌 round-trips to the same library id; an unknown name creates a Project-tier material; a closed box comes back a Xia |
| **β-3** | the style graph gives a *created* material its appearance | a foreign colour survives; a library-matched name is not repainted |
| **γ** | closure — demo gate, ADR, LOCKED, memory | — |

## 6. Acceptance

| step | commit | what landed |
|---|---|---|
| α | `bdee642` | this document, and the round-trip measured before designing |
| β-1 | `5a7d298` | a Shape per element, named and kinded; `ElementGeometry.ifc_type` |
| β-2 | `ba78cda` | material by name, matched or created; promotion attempted |
| β-3 | `0d437ed` | the style graph, read only for a material the name could not supply |
| γ | (this commit) | closure — demo gate, LOCKED, memory |

**Regressions**: `cargo test --workspace` **3268 → 3277** (+9: β-1 2, β-2 4,
β-3 3), 0 failed, 1 ignored (pre-existing slow channel).
`scripts/ifc-external-validate.mjs` — all external-parser checks pass.

**Mutation-checked**, each against the behaviour it is supposed to hold:

| mutation | caught by |
|---|---|
| the member owns nothing | both β-1 tests |
| the material name is never read | *promoted* and *an unmatched name is a new material* |
| a library name is never reused (always mint) | *face FaceId(0) must carry the library 벽돌* |
| promotion never attempted | *promoted* |
| the file's style is never read | *colour from IfcColourRgb* |
| a library-matched material is repainted by the file | *the library material keeps its own colour* |

**Demo gate** (real wasm, fresh reload) — the round-trip is now symmetric:

```
              before ADR-311                     after
export        2360 B, 'Box', 벽돌, colour        (unchanged)
import        xias 0, every face FORM_MATERIAL   xias 1, all six faces material 4, warnings []
re-export     1999 B, 'Model', no material       2363 B, 'Box', IFCMATERIAL('벽돌'), same IFCCOLOURRGB
```

The GUID comes back identical too (`2$9yajP4ccgJ4IbCuhZ4Ta`), since it is
derived from the member's name.

## 7. What is still not imported

- **Textures** (D5). `IfcImageTexture` / `IfcSurfaceStyleWithTextures` are
  emitted by §40 and read by nobody; the image bytes and the UV mapping would
  both have to be rebuilt.
- **`IfcSpecularExponent`.** Files that use the exponent form of
  `SpecularHighlight` fall back to the default roughness rather than have a
  conversion invented for them.
- **Per-face styles.** One style per member is read — the first representation
  item that carries one. A file styling individual faces differently loses that
  distinction.
- **Physical properties of a created material** are placeholders. IFC carries
  them in property sets (`IfcMaterialProperties`), which this step does not read,
  so a mass takeoff on a foreign material is the user's to correct.

## Cross-link

ADR-309 (named this step), ADR-310 (steps 2-3, and the measure-first habit that
found ADR-309's description inaccurate twice), ADR-050 / LOCKED #26 (the
two-layer citizenship rule D2 and D4 obey), ADR-098 (material tiers),
ADR-099 (layered material — the textures D5 defers), §38/§39/§40 of the export
programme (what we emit and therefore what there is to read back),
메타-원칙 #4 / #6, LOCKED #44.

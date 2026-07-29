# ADR-309 — An IFC round-trip says what it loses

**Status**: Accepted
**Date**: 2026-07-29
**Category**: IFC / import
**Scope**: step 1 of 4 — stops the silence, not the loss

> ⚠ **Two factual claims below are corrected by ADR-310 §6** (2026-07-29). The
> body is left as written (메타-원칙 #10); read it together with that section.
>
> 1. **§1's "other BIM tools read `IFCSPHERICALSURFACE` and show the right
>    solid" is false.** Measured against the vendored web-ifc: extent Z is
>    exactly `0.0000` for sphere and torus alike — it flattens them as we did.
>    §1 also contradicts §6 of this same document, which said the export drops
>    u/v. §6 was right.
> 2. **"volume lost" overstates.** A native Path B sphere also reports
>    `mesh_volume() == 0.0`; `mesh.rs:13067` skips loops under three vertices.
>    The loss was extent, shape and analytic surface — there was never a volume.
>
> §6's sizing was also wrong: step 3's stated obstacle was already satisfied by
> vertex dedup, and steps 2 and 3 together came to one field.
> `adr309_path_b_sphere_and_torus_round_trip_flat_and_say_so` has been **removed
> together with the fix**, as its own message instructed — both halves, not only
> the flatness one (see ADR-310 §6). `adr309_a_box_round_trip_warns_about_
> nothing` remains as the control.

## 1. Measured, by running it

`crates/axia-ifc`'s module doc states the contract: *"Every drop is named in
`GeometryImport::warnings`, so a thinner import is visible instead of silent."*
There was one drop that said nothing, and it was the worst one.

A prior audit traced this by reading and said so honestly — no sphere or torus
round-trip test exists, so nobody had run it. Run:

```
Path B sphere r=1000  → emits IFCSPHERICALSURFACE ×2   → re-import extent (2000, 2000,   0.0)
Path B torus R=1000 r=250 → emits IFCTOROIDALSURFACE ×1 → re-import extent (2500, 2500,   0.0)
Path B cone           → (IfcRevolvedAreaSolid)          → re-import extent (1000, 1000, 2000.0)
box (control)                                           → re-import extent (2000, 3000, 1000.0)
warnings: []   — in every case
```

**The sphere and the torus come back flat.** Volume zero, silently.

The export is *correct* — other BIM tools read `IFCSPHERICALSURFACE` and show the
right solid. Only our own re-import flattens it, because `face_bounds`
(`ifc_geometry.rs`) rebuilds a face from its **boundary alone**: it reads
`IfcAdvancedFace` attribute 0 (Bounds) and has never read attribute 1
(FaceSurface). For most faces a boundary is a poorer description. For these two it
is a fatal one, because a Path B sphere hemisphere and a Path B torus have exactly
one boundary — the planar equator circle. Rebuild that faithfully and you have a
disc.

The cone does **not** flatten: §44 routes it to `IfcRevolvedAreaSolid`, which the
importer tessellates, so the shape survives and only the analytic surface is lost.
Two different losses, and the earlier audit had merged them.

## 2. Decision — name it now, fix it next

This ADR does **not** restore the geometry. It makes the loss visible, because a
user who does not know their sphere became a disc cannot even work around it.

`FaceLoops` gains `dropped_surface: Option<String>` — the tag of an
`IfcAdvancedFace.FaceSurface` we did not reconstruct (`None` for `IfcPlane`, and
`None` for plain `IfcFace`, so the tessellated-brep path most files use is
untouched). The caller groups them and warns:

```
#61 'Ball': 2 face(s) carried IFCSPHERICALSURFACE but were rebuilt from their
boundary — the analytic surface is lost, and the boundary is planar, so this
solid is now FLAT (volume lost)
```

The second clause only appears when every such face's boundary really is planar
(`face_loop_is_planar`, 1 μm, ADR-147's floor). A curved face that keeps its depth
gets the first clause alone — the warning distinguishes "coarser" from "gone".

## 3. And the tests stop calling the loss correct

Two curved round-trip tests asserted `faces.len() > 20` — "round-trips as a round
column", "revolved into many faces". That encodes *tessellated output is the right
answer*, so the fix in step 2 would turn them red and read as a regression. Both
now say what they are: a record of today's behaviour, to be updated deliberately
when the importer learns to keep a surface.

Two regressions added. `adr309_path_b_sphere_and_torus_round_trip_flat_and_say_so`
pins both halves — that the export carries its surface, that the re-import is flat
(**delete only together with the fix**; its message says so), and that the warning
names the surface and says FLAT. `adr309_a_box_round_trip_warns_about_nothing` is
the control, without which every assertion above would pass on an importer that
simply warned about everything.

## 4. Lock-ins

- **L-309-1** A drop is named in `warnings`. That was already the contract; this
  closes the case that violated it.
- **L-309-2** A warning distinguishes *coarser* from *gone*. "Analytic surface
  lost" and "solid is now flat" are different news for the user.
- **L-309-3** A test that records a defect says so in its own message, and says
  what to do when it goes red. Otherwise the next person restores the defect to
  make the suite green.
- **L-309-4** 절대 #[ignore] 금지.

## 5. Verification

`cargo test --workspace` → **3262 passed / 0 failed / 1 ignored**.
`scripts/ifc-external-validate.mjs` → all external-parser checks pass.

**Mutation-checked**, both halves separately: making `curved_face_surface` always
return `None` fails with `the dropped surface must be named in warnings, got []`;
removing the FLAT clause fails with `a flattened solid must say so, got [...the
analytic surface is lost]`.

## 6. Remaining — this is step 1 of 4

| step | what | size |
|---|---|---|
| **1 ✅** | **say what is lost** | small |
| 2 | read `IfcAdvancedFace.FaceSurface` — `FaceLoops` needs a surface field; the mapping inverts `surface_to_ifc`. Note the export drops u/v extents too (`IfcPlane` is unbounded in IFC4), so extents must still be synthesized from the loop — an import-only repair cannot restore them | large |
| 3 | rebuild sphere/torus kernel-native — a hemisphere is not derivable from its rim, so the importer must create the self-loop once and hang both faces off the twin half-edges, as `create_sphere_kernel_native` does. The wasm loop builds faces independently today, so it needs to recognise a shared self-loop | large |
| 4 | appearance and material name — every presentation entity we emit is emit-only (the import path recognises 84 tags, none of them presentation), and the material *name* is parsed but dropped, so every face gets `FORM_MATERIAL` | medium |

## Cross-link

ADR-267 (the integrity gate the import path does not run), ADR-089 (Path B
closed-curve faces — the shape that flattens), ADR-147 (the 1 μm floor the
planarity test uses), ADR-299 / ADR-304 (the same family: a check that passed
while holding nothing), 메타-원칙 #6.

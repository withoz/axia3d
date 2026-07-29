# ADR-310 — An imported hemisphere keeps its sphere

**Status**: Accepted
**Date**: 2026-07-29
**Category**: IFC / import
**Scope**: step 2 of ADR-309's four — restores the geometry it named

## 1. Measured — the two hemispheres are byte-identical in the file

ADR-309 named the loss: a Path B sphere and torus come back as flat discs. Its §6
sized the repair as *"read `IfcAdvancedFace.FaceSurface` … extents must be
synthesized from the loop"*. That instruction cannot be followed for a sphere,
and the reason is worth stating exactly.

Dumping our own export and renumbering each `IFCADVANCEDFACE`'s dependency
closure relative to itself:

```
=== FACE A (canonical) ===          === FACE B (canonical) ===
@4=IFCSPHERICALSURFACE(@3,1.);      @4=IFCSPHERICALSURFACE(@3,1.);
@12=IFCEDGECURVE(@6,@6,@11,.T.);    @12=IFCEDGECURVE(@6,@6,@11,.T.);
@13=IFCORIENTEDEDGE(*,*,@12,.T.);   @13=IFCORIENTEDEDGE(*,*,@12,.T.);
@15=IFCFACEOUTERBOUND(@14,.T.);     @15=IFCFACEOUTERBOUND(@14,.T.);
@16=IFCADVANCEDFACE((@15),@4,.T.);  @16=IFCADVANCEDFACE((@15),@4,.T.);

*** BYTE-IDENTICAL AFTER RENUMBERING: true ***
```

All four orientation flags are `.T.` on both faces, and the one that could have
carried a distinction is never computed: `compute_same_sense`
(`ifc_advancedbrep.rs:400`) returns its degenerate default because a one-vertex
loop's Newell normal is exactly zero.

The engine knows the difference and the export drops it:

```
FaceId(0) Sphere { … v_range: (0.0,  1.5707963267948966) }   ← north
FaceId(1) Sphere { … v_range: (-1.5707963267948966, 0.0) }   ← south
```

`emit_surface` destructures `Sphere { center, radius, axis_dir, ref_dir, .. }`
(`ifc_advancedbrep.rs:585`) and `IFCSPHERICALSURFACE` carries only placement and
radius. **Synthesizing a v-range "from the loop" is impossible: both faces have
the same loop.**

## 2. But the *pair* is not ambiguous — and that is the whole fix

Reading one face in isolation, the cap is undecidable. Reading the file is not.

A circle lying on a sphere divides that sphere into exactly two components.
Our export emits two faces that reference the same spherical surface (as two
structurally identical entities) and the same boundary circle. Those two faces
are therefore *necessarily* the two caps. Which one is north is undetermined —
and unobservable, because the faces are identical in every respect.

So a lossless reconstruction is available **from the pair**:

| quantity | pair completion | naive "give both the full domain" |
|---|---|---|
| render | correct sphere | correct-looking (drawn twice) |
| `face_area` per face | 2πr² ✓ | 4πr² ✗ |
| total area | 4πr² ✓ | 8πr² ✗ |

IFC is a quantity-takeoff format; the second column is not acceptable, and it is
what an unconsidered import-only fix produces. Pair completion avoids it.

## 3. What was already correct — ADR-309 §6 mis-sized the work

§6 sizes step 3 (rebuild kernel-native) as **large**, its stated obstacle being
that *"the wasm loop builds faces independently, so it must learn to recognise a
shared self-loop"*. Measured: it already does, because vertex dedup lands both
faces on the same anchor.

```
imported DCEL: verts=1 edges=1 faces=2   invariants valid, 0 violations
native  DCEL:  verts=1 edges=1 faces=2
```

The imported topology is already the shape `create_sphere_kernel_native`
produces. The remaining gap is **one field**: `add_face_closed_curve` attaches
`AnalyticSurface::Plane` (`mesh.rs:7154`), and the polygon branch's `f.plane()`
(`lib.rs:5934`) is never reached because the closed-curve branch `continue`s.

**Step 3 is already done. Step 2 is medium, not large.**

## 4. Decision

**Import-only. The bytes we hand other tools do not change.** (User decision,
2026-07-29, taken over the alternative of flipping the southern
`IfcFaceOuterBound.Orientation` so the file distinguishes the halves itself.)

Recorded so it is not re-litigated — what was declined, and why declining costs
little:

- **Flip the southern bound sense (`.F.`) and read ISO 10303-42's `n × t` rule.**
  Would make a *single* hemisphere decidable. Declined: it changes emitted bytes,
  and no third-party reader was available to confirm any reader honours bound
  orientation on a periodic surface. Pair completion recovers the same geometry
  without touching the export.
- **Emit `IfcRectangularTrimmedSurface` carrying `U1,U2,V1,V2`.** Declined as
  **schema-illegal**: `IfcAdvancedFace`'s `ApplicableSurface` rule admits only
  `IfcElementarySurface` / `IfcSweptSurface` / `IfcBSplineSurface`, and a trimmed
  surface is an `IfcBoundedSurface`. It would also fail
  `scripts/ifc-external-validate.mjs:256`.
- **Emit a seam and pole (`IfcSeamCurve` + `IfcVertexLoop`).** The schema's own
  prescription for periodic surfaces, and the only option that would also fix our
  invalid closed shell (every edge is used once by one face). Deferred, not
  rejected: it is large, buys nothing measurable today, and is what foreign
  hemispheres will need when we import them.

## 5. Mechanism

`FaceLoops` gains `surface: Option<AnalyticSurface>` — `IfcAdvancedFace`
attribute 1, inverted through the same mapping `emit_surface` writes. A
per-element pass (`reconstruct_surfaces`) then decides its parameter range.

The draft of this section proposed a two-rule mechanism (invert the boundary
into a (u, v) box; fall back to pair completion when that collapses).
Implementing it produced **one** rule instead, which is both simpler and says
why the two cases differ:

> A boundary curve cuts its surface into some number of components, and the file
> must supply exactly that many faces.

- A circle on a **sphere** always disconnects it — Jordan, on a sphere — into
  two caps. Exactly two faces sharing one sphere and one circle of latitude
  *are* those two caps, and the split is in v at that latitude.
- A circle of latitude on a **torus** does *not* disconnect it. A single face
  carrying one is therefore the whole surface, and the circle is a seam.

Everything else is refused and keeps today's behaviour — the polygon, the
`dropped_surface` tag, and ADR-309's warning. That includes a lone hemisphere,
which is genuinely undecidable once the export change is declined, three faces
on one sphere, and a circle that is not a parallel. Reconstruction clears
`dropped_surface`, so the warning means exactly "not rebuilt" rather than "was
curved".

The evidence required is exact (L-310-2): the boundary must be a self-loop
`AnalyticCurve::Circle`, not a polygon that looks round, and surfaces are
matched to 1 μm (ADR-147's floor, which also absorbs the decimal round-trip a
STEP file puts every coordinate through).

## 6. Corrections — ADR-309 and this repo's own comments

1. **ADR-309 §1 is wrong.** *"other BIM tools read `IFCSPHERICALSURFACE` and show
   the right solid"*. Measured against the vendored web-ifc: extent Z is exactly
   `0.0000` for both sphere and torus. It flattens them as we do. This ADR fixes
   *our* round-trip; it does not improve any foreign reader, because web-ifc has
   no spherical or toroidal kernel at all. §1 also contradicts §5 of the same
   document, which already said the export drops u/v. §5 was right.
2. **"volume lost" overstates.** A freshly built **native** Path B sphere also
   reports `mesh_volume() == 0.0` and `is_closed_solid == false` —
   `mesh.rs:13067` skips loops with fewer than three vertices. The real loss is
   extent, shape and analytic surface. Restoring the surface restores those and
   gives no volume, because there never was one. "Restore the solid" must not be
   read as "produce a volume-bearing closed solid"; that is a separate, unscoped
   problem.
3. **`ifc_geometry.rs:15-21` is stale.** It claims a Path B rim "collapses to a
   single point, and that face is dropped rather than invented". The faces are
   not dropped; their boundaries are 497-point (sphere) and 512-point (torus)
   loops.
4. **`scripts/ifc-external-validate.mjs:23-28` is wrong.** It says web-ifc
   *"skips"* unsupported surfaces. It does not skip them — it triangulates the
   boundary loop into a flat disc.

## 7. Lock-ins

- **L-310-1** The export is unchanged. Any future need to change emitted bytes is
  a new ADR with its own approval.
- **L-310-2** A cap pair is completed only when the evidence is exact: same
  element, equal surface, equal boundary, and exactly two such faces. Three or
  more, or one, is left alone.
- **L-310-3** Reconstruction clears `dropped_surface`; failure keeps it. The
  warning must continue to mean "not rebuilt", never "was curved".
- **L-310-4** Under a non-identity placement the surface is dropped like
  `closed_curve` already is (`ifc_geometry.rs:410`) — a surface placed wrongly is
  worse than a surface absent.
- **L-310-5** Only spherical and toroidal faces are reconstructed here. Cylindrical
  and conical faces on the advanced-brep path keep their best-fit plane; extending
  to them changes render for existing files and is a separate step.
- **L-310-6** The parity guard is the point: an imported Path B primitive must
  equal the native one. That guard is what would have caught this defect.
- **L-310-8** **A surface that survives is one whose range we decided.** The
  invariant `surface.is_some() ⟹ dropped_surface.is_none()`, held by a final
  pass and pinned by a `debug_assert`. `face_surface` has to stock every
  advanced face at the natural full domain, because a range is not knowable one
  face at a time — so a refusal that only skips *narrowing* leaves a lie behind,
  and the attach site cannot tell. Refusal drops the surface with the tag.
- **L-310-9** Comparison of frames is by direction, not merely by axis.
  Anti-parallel frames describe the same sphere and a different
  parameterization; they are refused rather than accommodated.
- **L-310-10** Every acceptance test is written so that a NaN **fails** it.
  `NaN > eps` is false, so an `if x > eps { continue }` guard falls through on
  NaN — the trap ADR-304 found twice (LOCKED #100 L-100-1).
- **L-310-7** 절대 #[ignore] 금지.

## 8. Sub-steps

| step | meaning | regression |
|---|---|---|
| **α** | this document | — |
| **β-1** | `FaceLoops` carries the surface; invert `emit_surface`. Stored, not used. | an advanced face yields `Some(Sphere{..})`/`Some(Torus{..})`; an `IfcPlane` face is unchanged |
| **β-2** | torus reconstructed from the inverted boundary range | extent regains depth; face carries `Torus`; the warning stops naming `IFCTOROIDALSURFACE` |
| **β-3** | sphere reconstructed by pair completion | extent ≈ 2r; the two faces' `v_range` are complementary; total area 4πr²; a lone hemisphere still warns |
| **β-4** | parity guard + wasm attaches the surface on the closed-curve path | imported primitive equals native: counts, surface kind, range, area |
| **γ** | closure — ADR Accepted, ADR-309 corrections, stale comments, LOCKED, memory | ADR catalog CI |

## 9. Acceptance

| step | commit | what landed |
|---|---|---|
| α | `f58517e` | this document; the byte-identity measurement; the declined options |
| β-1 | `6b29db0` | `FaceLoops.surface`; `face_surface` inverts `emit_surface`. axia-ifc 168 → 169 |
| β-2/β-3 | `a8fce41` | `reconstruct_surfaces`; ADR-309's test replaced. 169 → 170 |
| β-4 | (this branch) | the wasm import attaches the surface; three parity tests |
| γ | (this commit) | Status, ADR-309's correction note, the stale comments, LOCKED |

**Regressions**: `cargo test --workspace` **3262 → 3268** (β-1 +1; β-2/β-3 +2
new − 1 removed; β-4 +3; the review's patch test +1), 0 failed, 1 ignored
(pre-existing slow channel). `vitest` **2970 / 1 skipped**, unchanged. `tsc` 0.
`check-adr-catalog` ✓. `scripts/ifc-external-validate.mjs` — all external-parser
checks pass.

**Mutation-checked**, each guard against the change it is supposed to hold:

| mutation | caught by |
|---|---|
| `face_surface` always returns `None` | `adr310_beta1_…` — *"a hemisphere must carry its sphere, got None"* |
| `reconstruct_surfaces` is a no-op | `adr310_sphere_and_torus_round_trip_keep_their_surface` |
| the `g.len() != 2` guard accepts a lone cap | `adr310_a_lone_hemisphere_is_undecidable_and_still_says_so` |
| the wasm does not attach the surface | both parity tests (the box control stays green, correctly) |
| refusal keeps its un-narrowed surface (with the `debug_assert` disabled too, so the assertion is what fails) | the lone-cap guard, and `adr310_a_spherical_patch_keeps_no_surface_and_still_warns` |

**What the tests deliberately do not assert.** ADR-309 measured `f.outer`'s
extent to expose the loss. That measure cannot show the repair: a hemisphere's
boundary *is* a flat circle, and the depth lives in the surface. A future reader
who reaches for extent to check this will find it unchanged and must not read
that as a regression — the round-trip test says so in its own comment.

### What an adversarial review caught, after all of that

Five independent lenses over the diff converged on one defect, and it was real.

`face_surface` stocks **every** advanced face with its surface at the natural
full domain — it has to, since a range is not knowable one face at a time.
`reconstruct_surfaces` narrowed that on success and cleared `dropped_surface`;
every refusal was a bare `continue`. The full-domain surface stayed on, and the
wasm attached it regardless. Measured on the fixture already in the tree:

```
PROBE lone-cap surface = Some(Sphere { .. v_range: (-1.5707963, 1.5707963) })
```

So the one case L-310-2 exists to refuse — a lone hemisphere — imported as a
**whole sphere**: area 1.26e7 where the truth is 6.28e6, and a full ball
rendered where half of one exists. Worse than the flat disc this ADR set out to
fix, and it reached every spherical or toroidal face we decline, including the
partial patches foreign exporters produce.

**The guard passed the entire time.**
`adr310_a_lone_hemisphere_is_undecidable_and_still_says_so` asserted the tag and
the warning text and never looked at `surface` — a check that held nothing,
which is the failure mode this repo has been bitten by repeatedly (ADR-299,
ADR-302, ADR-304). It now asserts `surface.is_none()`, and reverting the fix
with the `debug_assert` also disabled makes that assertion — not the assert —
fail.

Two smaller ones from the same review, both fixed: `same_surface` accepted
anti-parallel axes (L-310-9), and the acceptance guards were written so a NaN
fell through them (L-310-10). Also corrected: the FLAT clause had lost its only
behavioural assertion when ADR-309's test was replaced, and §6 #4's claim about
the validator was only two-thirds done — one of three "skips" was left.

The lesson is not new, which is the point: a guard proves nothing until the
guarded thing is broken and the guard is watched to fail. Four mutations were
run before the review, and none of them touched the refusal path — because I
wrote the test that would have.

**Demo gate** (CONTRIBUTING §4 — a green suite is not the scene path). Built the
wasm, drew a Path B sphere in the browser, exported it, reloaded to a fresh
engine, imported it back:

```
native   verts=1 edges=1 faces=2, both faceSurfaceKind=3 (Sphere)
export   2 × IFCADVANCEDFACE, IFCSPHERICALSURFACE present, 2791 bytes
import   verts=1 edges=1 faces=2, both faceSurfaceKind=3, warnings []
render   125496 vertices, z ∈ [-1000, +1000]
```

The last line is the user-visible fix: the imported solid spans the full
diameter instead of lying flat at z = 0.

Re-run after the review's fix, with the refusal path added — the same file with
one hemisphere struck out of its `IFCCLOSEDSHELL`:

```
pair      faces=2, both faceSurfaceKind=3 (Sphere), warnings [], z ∈ [-1000, +1000]
lone cap  faces=1,      faceSurfaceKind=1 (Plane),  z ∈ [0, 0]
          warning: "…carried IFCSPHERICALSURFACE but were rebuilt from their
          boundary — … this solid is now FLAT (volume lost)"
```

Before the fix that second row read `faceSurfaceKind=3` and
`z ∈ [-1000, +1000]`: a whole ball built from half of one.

## Cross-link

ADR-309 (named this loss; §1/§6 corrected here), ADR-089 / LOCKED #35 (Path B
closed-curve faces — the DCEL is a rim and the surface carries the shape),
ADR-031 (AnalyticSurface), ADR-104 family (Path B primitives), ADR-147 (the 1 μm
floor), ADR-267 (the integrity gate the import path does not run), LOCKED #44
(one complete meaning per merge), 메타-원칙 #4 / #6.

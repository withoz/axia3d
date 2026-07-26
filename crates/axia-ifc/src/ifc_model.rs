//! Multi-element IFC model (ADR-203 γ) — one `IfcWall` per semantic member
//! (Xia / Shape) with its own analytic geometry + `IfcMaterial`, all under one
//! `Project → Site → Building → Storey`.
//!
//! Where [`crate::emit_advanced_brep`] emits the whole mesh as a single wall,
//! [`emit_ifc_model`] takes a list of [`IfcElement`]s (name + optional material
//! + owned faces) and emits a named, materialed BIM model. The Scene → element
//! mapping (enumerating Xias/Shapes, resolving material names) lives in the
//! WASM caller; this crate stays Scene-agnostic.

use crate::guid::ifc_guid_for;
use crate::ifc_advancedbrep::{advanced_faces_filtered, emit_advanced_geometry, fill_through_hole_openings};
use crate::ifc_common::{emit_owner_units_context, pt};
use crate::step_value::StepValue;
use crate::step_writer::StepWriter;
use axia_geo::{FaceId, MaterialId, Mesh};
use glam::DVec3;
use std::collections::HashSet;

/// Refill an element's baked openings back to a SOLID wall body by boolean union
/// on a scratch clone (`holed wall ∪ each opening box = the original solid`) —
/// the general fill that also handles a notch/edge opening (a door's U-notch),
/// where the void is part of the outer boundary, not an inner loop (§36-amendment).
/// The scene mesh is never mutated. Returns the solid body's advanced faces (the
/// unioned box faces carry no stored surface → §35 synthesizes exact IfcPlanes).
fn union_refill_element(
    mesh: &Mesh,
    host_faces: &[FaceId],
    boxes: &[[DVec3; 8]],
) -> Result<Vec<crate::ifc_advancedbrep::AdvancedFace>, String> {
    let mut work = mesh.clone();
    // The element's face_ids can include stale INACTIVE faces (a carve deactivates
    // the pre-notch faces but leaves them in the owner's list). boolean_solid must
    // only see active operands, so filter first.
    let mut host: Vec<FaceId> =
        host_faces.iter().copied().filter(|&f| work.faces.get(f).is_some_and(|fc| fc.is_active())).collect();
    for b in boxes {
        host = work
            .union_opening_box(&host, b, MaterialId::new(0))
            .map_err(|e| format!("union refill: {}", e))?;
    }
    let allowed: HashSet<FaceId> = host.into_iter().collect();
    crate::ifc_advancedbrep::advanced_faces_filtered(&work, Some(&allowed))
}

/// A rectangular opening (window / door) to emit as an `IfcOpeningElement` that
/// voids one of the model's elements. `host_index` indexes into the `elements`
/// slice; `corners` are the eight box corners in world millimetres, ordered like
/// [`crate::emit_box`] (bottom face 0-3, top face 4-7).
pub struct ModelOpening {
    pub host_index: usize,
    pub corners: [DVec3; 8],
}

/// Box face loops (indices into the eight corners), CCW outward — matches `emit_box`.
const OPENING_FACE_LOOPS: [[usize; 4]; 6] = [
    [0, 3, 2, 1], // bottom
    [4, 5, 6, 7], // top
    [0, 1, 5, 4], // front
    [2, 3, 7, 6], // back
    [0, 4, 7, 3], // left
    [1, 2, 6, 5], // right
];

/// A material's rendering appearance for IFC export (ADR-203 §38/§39).
///
/// Carries colour + transparency (→ `IfcSurfaceStyleRendering.SurfaceColour` /
/// `.Transparency`), the PBR scalars roughness + metalness (→
/// `IfcSpecularRoughness` / `ReflectanceMethod`), and an optional albedo texture
/// (→ `IfcSurfaceStyleWithTextures`/`IfcImageTexture`, a base64 data-URI). All
/// channels flow from the engine material's `VisualProperties`. Deriving
/// `PartialEq` lets equal styles share one `IfcSurfaceStyle` across elements.
#[derive(Clone, PartialEq)]
pub struct MaterialStyle {
    /// Base colour, each component 0..1.
    pub rgb: (f64, f64, f64),
    /// 0 = opaque, 1 = fully transparent (`1 - opacity`).
    pub transparency: f64,
    /// Micro-surface roughness 0..1 (0 = mirror).
    pub roughness: f64,
    /// Metalness 0..1 (>0.5 exports as `ReflectanceMethod` `.METAL.`).
    pub metalness: f64,
    /// Optional albedo texture as a base64 data-URI (`data:image/png;base64,…`),
    /// used as `IfcImageTexture.URLReference`. `None` for untextured materials
    /// (all library materials — only user-uploaded PBR carries a texture).
    pub albedo_data_url: Option<String>,
}

/// One semantic member to export: a display name, an optional material name,
/// what kind of building element it is, and the faces it owns (engine
/// `FaceId`s).
#[derive(Default)]
pub struct IfcElement {
    pub name: String,
    pub material_name: Option<String>,
    /// The material's rendering appearance — the source for the element's
    /// `IfcStyledItem`/`IfcSurfaceStyle` so BIM viewers render it faithfully
    /// (ADR-203 §38/§39). `None` = no style (viewer default grey).
    pub material_style: Option<MaterialStyle>,
    /// What this member *is* (ADR-203 δ). Defaults to `Wall` — which is what
    /// every member used to be, so an unassigned model exports unchanged.
    pub kind: crate::IfcElementKind,
    pub face_ids: Vec<FaceId>,
}

/// `OverallHeight` / `OverallWidth` for an opening element, from the faces it
/// owns.
///
/// Height is the Z extent — unambiguous under Z-up (LOCKED #43). Width is the
/// larger of the two horizontal extents, the smaller being the panel's
/// thickness. Both come back `Unset` if the member has no measurable box, so a
/// degenerate element writes `$` instead of a zero that reads as a real size.
fn overall_size(mesh: &Mesh, allowed: &HashSet<FaceId>, scale: f64) -> (StepValue, StepValue) {
    // From the element's actual mesh vertices, so it is independent of whether the
    // element exported as an advanced or a faceted brep (§37).
    let mut lo = [f64::INFINITY; 3];
    let mut hi = [f64::NEG_INFINITY; 3];
    for &fid in allowed {
        let Some(face) = mesh.faces.get(fid) else { continue };
        if !face.is_active() {
            continue;
        }
        if let Ok(vs) = mesh.collect_loop_verts(face.outer().start) {
            for v in vs {
                if let Ok(p) = mesh.vertex_pos(v) {
                    for (i, c) in [p.x, p.y, p.z].into_iter().enumerate() {
                        lo[i] = lo[i].min(c);
                        hi[i] = hi[i].max(c);
                    }
                }
            }
        }
    }
    if !lo.iter().chain(hi.iter()).all(|v| v.is_finite()) {
        return (StepValue::Unset, StepValue::Unset);
    }
    let (dx, dy, dz) = (hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2]);
    let height = dz * scale;
    let width = dx.max(dy) * scale;
    // IfcPositiveLengthMeasure — zero is not a legal value, so omit instead.
    let pos = |v: f64| if v > 0.0 { StepValue::Real(v) } else { StepValue::Unset };
    (pos(height), pos(width))
}

fn next_guid(gi: &mut u64) -> StepValue {
    let v = *gi;
    *gi += 1;
    StepValue::Str(ifc_guid_for(v))
}

/// Emit a complete IFC4.3 file with one `IfcWall` per [`IfcElement`], each
/// carrying an analytic `IfcAdvancedBrep` of its owned faces + (if given) an
/// `IfcMaterial` via `IfcRelAssociatesMaterial`. `scale` converts engine units
/// to metre; `project_name` labels the `IfcProject`.
///
/// **All-or-nothing** (like β-2.5): errors if any element's faces can't form an
/// advanced brep (unsupported surface / curve) → the WASM caller falls back to
/// the faceted single-wall export.
pub fn emit_ifc_model(
    mesh: &Mesh,
    elements: &[IfcElement],
    scale: f64,
    project_name: &str,
) -> Result<String, String> {
    emit_ifc_model_with_openings(mesh, elements, &[], scale, project_name)
}

/// Like [`emit_ifc_model`], but also emits `IfcOpeningElement` + `IfcRelVoidsElement`
/// for each recorded opening (window / door the user drew).
pub fn emit_ifc_model_with_openings(
    mesh: &Mesh,
    elements: &[IfcElement],
    openings: &[ModelOpening],
    scale: f64,
    project_name: &str,
) -> Result<String, String> {
    if elements.is_empty() {
        return Err("emit_ifc_model: no elements".into());
    }
    let mut w = StepWriter::new();
    w.file_description = format!("AXiA IFC4.3 '{}' (IfcAdvancedBrep model, ADR-203 γ)", project_name);
    w.file_name = format!("{}.ifc", project_name);

    // ── Owner / units / context ──
    let sc = emit_owner_units_context(&mut w);

    let mut gi: u64 = 0;

    // ── Spatial root: Project → Site → Building → Storey ──
    let project = w.add(
        "IFCPROJECT",
        vec![
            next_guid(&mut gi),
            StepValue::Ref(sc.owner),
            StepValue::Str(project_name.into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset,
            StepValue::List(vec![StepValue::Ref(sc.context)]),
            StepValue::Ref(sc.units),
        ],
    );
    let site_pl = w.add("IFCLOCALPLACEMENT", vec![StepValue::Unset, StepValue::Ref(sc.world)]);
    let site = w.add(
        "IFCSITE",
        vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str("Site".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset, StepValue::Unset,
        ],
    );
    let building = w.add(
        "IFCBUILDING",
        vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str("Building".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Unset,
        ],
    );
    let storey = w.add(
        "IFCBUILDINGSTOREY",
        vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str("Storey".into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Unset, StepValue::Unset, StepValue::Enum("ELEMENT".into()), StepValue::Unset,
        ],
    );
    w.add("IFCRELAGGREGATES", vec![
        next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
        StepValue::Ref(project), StepValue::List(vec![StepValue::Ref(site)])]);
    w.add("IFCRELAGGREGATES", vec![
        next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
        StepValue::Ref(site), StepValue::List(vec![StepValue::Ref(building)])]);
    w.add("IFCRELAGGREGATES", vec![
        next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
        StepValue::Ref(building), StepValue::List(vec![StepValue::Ref(storey)])]);

    // ── One IfcWall per element (geometry + material) ──
    let mut walls = Vec::with_capacity(elements.len());
    // Deduplicate IfcMaterial by name (one entity per distinct material).
    let mut materials: Vec<(String, crate::step_value::EntityRef)> = Vec::new();
    // Deduplicate IfcSurfaceStyle by colour (§38 appearance) — one style entity
    // per distinct (r,g,b,transparency); each element gets its own IfcStyledItem
    // pointing at its brep + the shared style.
    let mut styles: Vec<(MaterialStyle, crate::step_value::EntityRef)> = Vec::new();

    for (ei, el) in elements.iter().enumerate() {
        let allowed: HashSet<FaceId> = el.face_ids.iter().copied().collect();
        let boxes: Vec<[DVec3; 8]> =
            openings.iter().filter(|op| op.host_index == ei).map(|op| op.corners).collect();
        // Per-element geometry (§37): an analytic element → IfcAdvancedBrep; an
        // element that can't form one (a non-planar / surfaceless face) → an
        // IfcFacetedBrep FALLBACK, so it keeps its own IfcWall + material + spatial
        // placement instead of collapsing the WHOLE model to one faceted shell.
        // Gate on `advanced_faces_filtered` (a pure query — nothing is emitted to
        // `w` when it fails, so the faceted path leaves no orphan advanced entities).
        let (brep, rep_type) = match advanced_faces_filtered(mesh, Some(&allowed)) {
            Ok(mut faces) => {
                // Emit a SOLID wall body: the openings this element hosts are
                // re-stated as separate IfcOpeningElements below, so the body must
                // NOT also carry the baked hole — else a consumer voids it twice
                // (§35/§36). A through-hole is refilled locally; a notch/edge
                // opening (a door's U-notch) is refilled by boolean union.
                if !boxes.is_empty() {
                    let any_notch = boxes.iter().any(|b| crate::ifc_advancedbrep::is_notch(&faces, b));
                    faces = if any_notch {
                        match union_refill_element(mesh, &el.face_ids, &boxes) {
                            Ok(solid) => solid,
                            Err(_) => fill_through_hole_openings(faces, &boxes),
                        }
                    } else {
                        fill_through_hole_openings(faces, &boxes)
                    };
                }
                match emit_advanced_geometry(&mut w, &faces, scale) {
                    Ok(b) => (b, "AdvancedBrep"),
                    Err(_) => (
                        crate::ifc_facetedbrep::emit_faceted_geometry(&mut w, mesh, Some(&allowed), scale)
                            .map_err(|e| format!("element[{}] '{}' faceted: {}", ei, el.name, e))?,
                        "Brep",
                    ),
                }
            }
            Err(_) => (
                crate::ifc_facetedbrep::emit_faceted_geometry(&mut w, mesh, Some(&allowed), scale)
                    .map_err(|e| format!("element[{}] '{}' faceted: {}", ei, el.name, e))?,
                "Brep",
            ),
        };

        let shape_rep = w.add(
            "IFCSHAPEREPRESENTATION",
            vec![
                StepValue::Ref(sc.context),
                StepValue::Str("Body".into()),
                StepValue::Str(rep_type.into()),
                StepValue::List(vec![StepValue::Ref(brep)]),
            ],
        );
        let prod_def = w.add(
            "IFCPRODUCTDEFINITIONSHAPE",
            vec![StepValue::Unset, StepValue::Unset, StepValue::List(vec![StepValue::Ref(shape_rep)])],
        );
        // The eight attributes every IfcElement has, then whatever this kind
        // adds. A door or a window takes thirteen, not nine — emitting them in
        // the wall-shaped slot produces an entity no reader accepts.
        let mut args = vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str(el.name.clone().into()),
            StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl),
            StepValue::Ref(prod_def), StepValue::Unset,
        ];
        if el.kind.has_overall_size() {
            // OverallHeight / OverallWidth — BIM tools show these as the
            // opening's size, so fill them from the member's own bounds rather
            // than leaving `$`. Height is the Z extent (Z-up, LOCKED #43);
            // width is the larger horizontal extent, the other one being the
            // panel's thickness. A degenerate box leaves both unset.
            let (h, wd) = overall_size(mesh, &allowed, scale);
            args.push(h);
            args.push(wd);
        }
        args.push(StepValue::Unset); // PredefinedType
        if el.kind.has_overall_size() {
            // Door: OperationType + UserDefinedOperationType.
            // Window: PartitioningType + UserDefinedPartitioningType.
            args.push(StepValue::Unset);
            args.push(StepValue::Unset);
        }
        debug_assert_eq!(args.len(), el.kind.attribute_count(), "{} arity", el.kind.tag());
        let wall = w.add(el.kind.tag(), args);
        walls.push(wall);

        if let Some(mat_name) = &el.material_name {
            let mat = match materials.iter().find(|(n, _)| n == mat_name) {
                Some((_, r)) => *r,
                None => {
                    let r = w.add(
                        "IFCMATERIAL",
                        vec![StepValue::Str(mat_name.clone().into()), StepValue::Unset, StepValue::Unset],
                    );
                    materials.push((mat_name.clone(), r));
                    r
                }
            };
            w.add(
                "IFCRELASSOCIATESMATERIAL",
                vec![
                    next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
                    StepValue::List(vec![StepValue::Ref(wall)]), StepValue::Ref(mat),
                ],
            );
        }

        // ── Appearance (§38/§39): style the element's geometry so BIM viewers
        // render it faithfully. IfcStyledItem(brep) → IfcSurfaceStyle →
        // { IfcSurfaceStyleRendering (colour + transparency + roughness +
        //   metalness), IfcSurfaceStyleWithTextures (albedo, when present) }.
        // The IfcSurfaceStyle is shared per distinct appearance. ──
        if let Some(sty) = &el.material_style {
            let style = match styles.iter().find(|(c, _)| c == sty) {
                Some((_, r)) => *r,
                None => {
                    let (r, g, b) = sty.rgb;
                    let colour = w.add(
                        "IFCCOLOURRGB",
                        vec![StepValue::Unset, StepValue::Real(r), StepValue::Real(g), StepValue::Real(b)],
                    );
                    let transparency = if sty.transparency > 1e-6 {
                        StepValue::Real(sty.transparency)
                    } else {
                        StepValue::Unset
                    };
                    // IfcSurfaceStyleRendering (IFC4): SurfaceColour, Transparency,
                    // DiffuseColour, TransmissionColour, DiffuseTransmissionColour,
                    // ReflectionColour, SpecularColour, SpecularHighlight,
                    // ReflectanceMethod. IFC's model is Phong, not PBR: map
                    // roughness → IfcSpecularRoughness and metalness → the
                    // reflectance method (.METAL. when clearly metallic).
                    let reflectance =
                        if sty.metalness > 0.5 { "METAL" } else { "NOTDEFINED" };
                    let rendering = w.add(
                        "IFCSURFACESTYLERENDERING",
                        vec![
                            StepValue::Ref(colour),
                            transparency,
                            StepValue::Unset, // DiffuseColour
                            StepValue::Unset, // TransmissionColour
                            StepValue::Unset, // DiffuseTransmissionColour
                            StepValue::Unset, // ReflectionColour
                            StepValue::Unset, // SpecularColour
                            StepValue::Typed(
                                "IFCSPECULARROUGHNESS",
                                vec![StepValue::Real(sty.roughness.clamp(0.0, 1.0))],
                            ),
                            StepValue::Enum(reflectance.into()),
                        ],
                    );
                    let mut style_elems = vec![StepValue::Ref(rendering)];
                    // §39 texture — embed the albedo image (base64 data-URI) as an
                    // IfcImageTexture. No UV mapping ⇒ viewers apply their default.
                    if let Some(url) = &sty.albedo_data_url {
                        let tex = w.add(
                            "IFCIMAGETEXTURE",
                            vec![
                                StepValue::Enum("T".into()), // RepeatS
                                StepValue::Enum("T".into()), // RepeatT
                                StepValue::Str("DIFFUSE".into()), // Mode
                                StepValue::Unset,            // TextureTransform
                                StepValue::Unset,            // Parameter
                                StepValue::Str(url.clone().into()), // URLReference
                            ],
                        );
                        let with_tex = w.add(
                            "IFCSURFACESTYLEWITHTEXTURES",
                            vec![StepValue::List(vec![StepValue::Ref(tex)])],
                        );
                        style_elems.push(StepValue::Ref(with_tex));
                    }
                    let surf = w.add(
                        "IFCSURFACESTYLE",
                        vec![
                            el.material_name.clone().map_or(StepValue::Unset, |n| StepValue::Str(n.into())),
                            StepValue::Enum("BOTH".into()),
                            StepValue::List(style_elems),
                        ],
                    );
                    styles.push((sty.clone(), surf));
                    surf
                }
            };
            w.add(
                "IFCSTYLEDITEM",
                vec![StepValue::Ref(brep), StepValue::List(vec![StepValue::Ref(style)]), StepValue::Unset],
            );
        }
    }

    // ── Contain every wall in the storey ──
    w.add(
        "IFCRELCONTAINEDINSPATIALSTRUCTURE",
        vec![
            next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
            StepValue::List(walls.iter().map(|&x| StepValue::Ref(x)).collect()),
            StepValue::Ref(storey),
        ],
    );

    // ── Openings the user drew → IfcOpeningElement + IfcRelVoidsElement ──
    // The hole is baked into the host wall's mesh; this re-states it as a real IFC
    // opening so a reader (and our own re-import) sees the wall + opening, not a wall
    // with a mysterious dent. Each opening is a faceted-brep box that voids its host.
    for op in openings {
        let Some(&host) = walls.get(op.host_index) else { continue };
        let verts: Vec<_> = op.corners.iter().map(|&c| pt(&mut w, c * scale)).collect();
        let mut faces = Vec::with_capacity(6);
        for loop_idx in &OPENING_FACE_LOOPS {
            let loop_ = w.add(
                "IFCPOLYLOOP",
                vec![StepValue::List(loop_idx.iter().map(|&i| StepValue::Ref(verts[i])).collect())],
            );
            let bound = w.add(
                "IFCFACEOUTERBOUND",
                vec![StepValue::Ref(loop_), StepValue::Enum("T".into())],
            );
            faces.push(w.add("IFCFACE", vec![StepValue::List(vec![StepValue::Ref(bound)])]));
        }
        let shell = w.add(
            "IFCCLOSEDSHELL",
            vec![StepValue::List(faces.iter().map(|&f| StepValue::Ref(f)).collect())],
        );
        let brep = w.add("IFCFACETEDBREP", vec![StepValue::Ref(shell)]);
        let shape_rep = w.add(
            "IFCSHAPEREPRESENTATION",
            vec![
                StepValue::Ref(sc.context), StepValue::Str("Body".into()), StepValue::Str("Brep".into()),
                StepValue::List(vec![StepValue::Ref(brep)]),
            ],
        );
        let prod_def = w.add(
            "IFCPRODUCTDEFINITIONSHAPE",
            vec![StepValue::Unset, StepValue::Unset, StepValue::List(vec![StepValue::Ref(shape_rep)])],
        );
        let opening = w.add(
            "IFCOPENINGELEMENT",
            vec![
                next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Str("Opening".into()),
                StepValue::Unset, StepValue::Unset, StepValue::Ref(site_pl), StepValue::Ref(prod_def),
                StepValue::Unset, StepValue::Unset, // Tag, PredefinedType
            ],
        );
        w.add(
            "IFCRELVOIDSELEMENT",
            vec![
                next_guid(&mut gi), StepValue::Ref(sc.owner), StepValue::Unset, StepValue::Unset,
                StepValue::Ref(host), StepValue::Ref(opening),
            ],
        );
    }

    Ok(w.build())
}

#[cfg(test)]
mod tests {
    use super::*;
    use glam::DVec3;

    fn assert_refs_resolve(s: &str) {
        let mut max_def = 0u32;
        for line in s.lines() {
            if let Some(eq) = line.find('=') {
                if line.starts_with('#') {
                    if let Ok(id) = line[1..eq].parse::<u32>() {
                        max_def = max_def.max(id);
                    }
                }
            }
        }
        for line in s.lines() {
            if !line.starts_with('#') {
                continue;
            }
            let args = &line[line.find('(').map(|i| i + 1).unwrap_or(line.len())..];
            let chars = args.char_indices();
            for (i, c) in chars {
                if c == '#' {
                    let rest = &args[i + 1..];
                    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
                    if let Ok(id) = num.parse::<u32>() {
                        assert!(id >= 1 && id <= max_def, "ref #{} out of range (max #{})", id, max_def);
                    }
                }
            }
        }
    }

    /// Two separate boxes → two IfcWalls, each with its own material.
    fn two_box_mesh() -> (Mesh, Vec<FaceId>, Vec<FaceId>) {
        let mut mesh = Mesh::new();
        let a = mesh
            .create_box(DVec3::new(-2000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0))
            .unwrap();
        let b = mesh
            .create_box(DVec3::new(2000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0))
            .unwrap();
        (mesh, a, b)
    }

    /// Count the comma-separated attributes of the first `TAG(` entity.
    fn arity_of(src: &str, tag: &str) -> usize {
        let start = src.find(&format!("={tag}(")).unwrap_or_else(|| panic!("no {tag}"));
        let open = src[start..].find('(').unwrap() + start + 1;
        let mut depth = 0usize;
        let mut n = 1usize;
        let mut in_str = false;
        for c in src[open..].chars() {
            match c {
                '\'' => in_str = !in_str,
                '(' if !in_str => depth += 1,
                ')' if !in_str => {
                    if depth == 0 {
                        break;
                    }
                    depth -= 1;
                }
                ',' if !in_str && depth == 0 => n += 1,
                _ => {}
            }
        }
        n
    }

    #[test]
    fn a_door_takes_thirteen_attributes_and_a_wall_still_takes_nine() {
        // δ refused doors because emitting one in the nine-attribute wall slot
        // makes an entity no IFC reader accepts. This is the shape that lets
        // them through.
        let (mesh, a, b) = two_box_mesh();
        let elements = vec![
            IfcElement { name: "Front Door".into(), material_name: None, material_style: None, kind: crate::IfcElementKind::Door, face_ids: a },
            IfcElement { name: "Plain Wall".into(), material_name: None, material_style: None, kind: crate::IfcElementKind::Wall, face_ids: b },
        ];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "House").unwrap();

        assert_eq!(s.matches("=IFCDOOR(").count(), 1);
        assert_eq!(arity_of(&s, "IFCDOOR"), 13, "door: 8 common + height + width + 3");
        assert_eq!(arity_of(&s, "IFCWALL"), 9, "the wall shape must not move");
    }

    #[test]
    fn an_opening_becomes_an_opening_element_that_voids_its_wall() {
        // A window the user drew is a box that voids its host wall — an
        // IfcOpeningElement + an IfcRelVoidsElement, not a mysterious dent.
        let mut mesh = Mesh::new();
        // create_box(center, width→X, height→Z, depth→Y): a 4 m × 3 m × 0.2 m wall.
        let wall = mesh
            .create_box(DVec3::ZERO, 4000.0, 3000.0, 200.0, axia_geo::MaterialId::new(0))
            .unwrap();
        let elements = vec![IfcElement {
            name: "Wall".into(), material_name: None, material_style: None, kind: crate::IfcElementKind::Wall, face_ids: wall,
        }];
        // A 1 m (X) × 1.2 m (Z) box, punched through the 0.2 m depth (Y), ordered
        // like emit_box: bottom four then top four.
        let c = |x: f64, y: f64, z: f64| DVec3::new(x, y, z);
        let corners = [
            c(-500.0, -150.0, -600.0), c(500.0, -150.0, -600.0), c(500.0, 150.0, -600.0), c(-500.0, 150.0, -600.0),
            c(-500.0, -150.0, 600.0), c(500.0, -150.0, 600.0), c(500.0, 150.0, 600.0), c(-500.0, 150.0, 600.0),
        ];
        let openings = vec![ModelOpening { host_index: 0, corners }];
        let s = emit_ifc_model_with_openings(&mesh, &elements, &openings, 0.001, "House").unwrap();

        assert_eq!(s.matches("=IFCOPENINGELEMENT(").count(), 1, "one opening element");
        assert_eq!(s.matches("=IFCRELVOIDSELEMENT(").count(), 1, "one voids relationship");
        assert_eq!(arity_of(&s, "IFCOPENINGELEMENT"), 9);
        assert_eq!(arity_of(&s, "IFCRELVOIDSELEMENT"), 6);
        assert_refs_resolve(&s);

        // A model that records no openings emits none (the wall is still there).
        let plain = emit_ifc_model(&mesh, &elements, 0.001, "House").unwrap();
        assert_eq!(plain.matches("=IFCOPENINGELEMENT(").count(), 0);
        assert_eq!(plain.matches("=IFCWALL(").count(), 1);
    }

    fn rendering_line(s: &str) -> &str {
        s.lines().find(|l| l.contains("IFCSURFACESTYLERENDERING(")).expect("a rendering line")
    }

    #[test]
    fn a_material_style_emits_a_rendering_styled_item() {
        // §38/§39 — an element with a material style gets an IfcStyledItem →
        // IfcSurfaceStyle → IfcSurfaceStyleRendering → IfcColourRgb on its brep,
        // so a BIM viewer renders it faithfully. Two identically-styled elements
        // share one IfcSurfaceStyle but get their own IfcStyledItem.
        let mut mesh = Mesh::new();
        let a = mesh.create_box(DVec3::new(0.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0)).unwrap();
        let b = mesh.create_box(DVec3::new(3000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0)).unwrap();
        let red = Some(MaterialStyle {
            rgb: (1.0, 0.0, 0.0), transparency: 0.0, roughness: 0.5, metalness: 0.0, albedo_data_url: None,
        });
        let elements = vec![
            IfcElement { name: "A".into(), material_name: Some("Brick".into()), material_style: red.clone(), kind: crate::IfcElementKind::Wall, face_ids: a },
            IfcElement { name: "B".into(), material_name: Some("Brick".into()), material_style: red, kind: crate::IfcElementKind::Wall, face_ids: b },
        ];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "Styled").unwrap();
        assert_eq!(s.matches("=IFCSTYLEDITEM(").count(), 2, "one styled item per element/brep");
        assert_eq!(s.matches("=IFCSURFACESTYLE(").count(), 1, "same style → shared surface style");
        assert_eq!(s.matches("=IFCSURFACESTYLERENDERING(").count(), 1);
        assert_eq!(s.matches("=IFCCOLOURRGB(").count(), 1);
        assert!(s.contains("IFCCOLOURRGB($,1.,0.,0.)"), "opaque red: {}", s);
        let r = rendering_line(&s);
        // Rendering args: colour, transparency, 5×$, IfcSpecularRoughness, method.
        assert!(r.contains(",$,$,$,$,$,$,IFCSPECULARROUGHNESS(0.5),"), "opaque + roughness: {}", r);
        assert!(r.ends_with(",.NOTDEFINED.);"), "non-metal → .NOTDEFINED.: {}", r);
        assert_refs_resolve(&s);

        // Translucent + metallic: transparency ratio + .METAL. reflectance.
        let glass = vec![IfcElement {
            name: "G".into(), material_name: Some("Chrome".into()),
            material_style: Some(MaterialStyle {
                rgb: (0.2, 0.4, 0.8), transparency: 0.5, roughness: 0.1, metalness: 0.9, albedo_data_url: None,
            }),
            kind: crate::IfcElementKind::Wall,
            face_ids: mesh.create_box(DVec3::new(-3000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0)).unwrap(),
        }];
        let g = emit_ifc_model(&mesh, &glass, 0.001, "Glass").unwrap();
        let gr = rendering_line(&g);
        assert!(gr.contains("(#") && gr.contains(",0.5,$,$,$,$,$,IFCSPECULARROUGHNESS(0.1),"), "transparency + roughness: {}", gr);
        assert!(gr.ends_with(",.METAL.);"), "metalness>0.5 → .METAL.: {}", gr);
        assert!(g.contains("IFCCOLOURRGB($,0.2,0.4,0.8)"), "glass colour: {}", g);
        assert_refs_resolve(&g);

        // §39 texture — an albedo data-URI emits IfcSurfaceStyleWithTextures +
        // IfcImageTexture carrying the URL, alongside the rendering.
        let tex = vec![IfcElement {
            name: "T".into(), material_name: Some("Wood".into()),
            material_style: Some(MaterialStyle {
                rgb: (0.6, 0.4, 0.2), transparency: 0.0, roughness: 0.8, metalness: 0.0,
                albedo_data_url: Some("data:image/png;base64,iVBORw0KGgo=".into()),
            }),
            kind: crate::IfcElementKind::Wall,
            face_ids: mesh.create_box(DVec3::new(6000.0, 0.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0)).unwrap(),
        }];
        let t = emit_ifc_model(&mesh, &tex, 0.001, "Textured").unwrap();
        assert_eq!(t.matches("=IFCSURFACESTYLEWITHTEXTURES(").count(), 1, "albedo → with-textures");
        assert_eq!(t.matches("=IFCIMAGETEXTURE(").count(), 1);
        assert!(t.contains("'data:image/png;base64,iVBORw0KGgo='"), "embeds the data-URI: {}", t);
        assert!(t.contains("IFCIMAGETEXTURE(.T.,.T.,'DIFFUSE',"), "repeat + diffuse mode: {}", t);
        // The style still carries the rendering (both style elements present).
        assert_eq!(t.matches("=IFCSURFACESTYLERENDERING(").count(), 1);
        assert_refs_resolve(&t);

        // No style → no style entities at all (unchanged behaviour).
        let plain = vec![IfcElement {
            name: "P".into(), material_name: None, material_style: None, kind: crate::IfcElementKind::Wall,
            face_ids: mesh.create_box(DVec3::new(0.0, 3000.0, 0.0), 1000.0, 1000.0, 1000.0, axia_geo::MaterialId::new(0)).unwrap(),
        }];
        let p = emit_ifc_model(&mesh, &plain, 0.001, "Plain").unwrap();
        assert_eq!(p.matches("=IFCSTYLEDITEM(").count(), 0, "no style → no styled item");
    }

    #[test]
    fn a_window_carries_its_measured_size() {
        // OverallHeight / OverallWidth are what a BIM tool shows as the
        // opening's size. Leaving them `$` is legal but useless, so they come
        // from the member's own bounds: height is the Z extent, width the
        // larger horizontal one.
        let mut mesh = Mesh::new();
        // create_box(center, width→X, height→Z, depth→Y): 1200 wide, 900 tall,
        // 100 thick — a window panel.
        let f = mesh
            .create_box(DVec3::ZERO, 1200.0, 900.0, 100.0, axia_geo::MaterialId::new(0))
            .unwrap();
        let elements = vec![IfcElement {
            name: "W1".into(),
            material_name: None, material_style: None,
            kind: crate::IfcElementKind::Window,
            face_ids: f,
        }];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "House").unwrap();

        assert_eq!(arity_of(&s, "IFCWINDOW"), 13);
        let line = s.lines().find(|l| l.contains("=IFCWINDOW(")).expect("window line");
        // metres after the 0.001 scale — height then width, in that order.
        assert!(line.contains("0.9"), "height 900mm -> 0.9m: {line}");
        assert!(line.contains("1.2"), "width 1200mm -> 1.2m (not the 0.1m thickness): {line}");
    }

    #[test]
    fn two_elements_two_walls_two_materials() {
        let (mesh, a, b) = two_box_mesh();
        let elements = vec![
            IfcElement { name: "Wall A".into(), material_name: Some("Concrete".into()), material_style: None, kind: crate::IfcElementKind::Wall, face_ids: a },
            IfcElement { name: "Wall B".into(), material_name: Some("Steel".into()), material_style: None, kind: crate::IfcElementKind::Wall, face_ids: b },
        ];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "House").unwrap();
        assert!(s.contains("FILE_SCHEMA(('IFC4X3'));"));
        // two named walls
        assert_eq!(s.matches("=IFCWALL(").count(), 2);
        assert!(s.contains("'Wall A'") && s.contains("'Wall B'"));
        // one shared spatial hierarchy
        assert_eq!(s.matches("=IFCPROJECT(").count(), 1);
        assert_eq!(s.matches("=IFCBUILDINGSTOREY(").count(), 1);
        // two materials + two associations
        assert_eq!(s.matches("=IFCMATERIAL(").count(), 2);
        assert!(s.contains("IFCMATERIAL('Concrete'") && s.contains("IFCMATERIAL('Steel'"));
        assert_eq!(s.matches("=IFCRELASSOCIATESMATERIAL(").count(), 2);
        // each box is 6 planar advanced faces
        assert_eq!(s.matches("=IFCADVANCEDBREP(").count(), 2);
        assert_eq!(s.matches("=IFCADVANCEDFACE(").count(), 12);
        // both walls contained in the one storey
        assert_eq!(s.matches("=IFCRELCONTAINEDINSPATIALSTRUCTURE(").count(), 1);
        assert_refs_resolve(&s);
    }

    #[test]
    fn shared_material_deduplicated() {
        let (mesh, a, b) = two_box_mesh();
        let elements = vec![
            IfcElement { name: "A".into(), material_name: Some("Concrete".into()), material_style: None, kind: crate::IfcElementKind::Wall, face_ids: a },
            IfcElement { name: "B".into(), material_name: Some("Concrete".into()), material_style: None, kind: crate::IfcElementKind::Wall, face_ids: b },
        ];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "M").unwrap();
        // one IfcMaterial (deduped), two associations (one per wall)
        assert_eq!(s.matches("=IFCMATERIAL(").count(), 1);
        assert_eq!(s.matches("=IFCRELASSOCIATESMATERIAL(").count(), 2);
    }

    #[test]
    fn element_without_material_has_no_association() {
        let (mesh, a, _b) = two_box_mesh();
        let elements = vec![IfcElement { name: "Form".into(), material_name: None, material_style: None, kind: crate::IfcElementKind::Wall, face_ids: a }];
        let s = emit_ifc_model(&mesh, &elements, 0.001, "F").unwrap();
        assert_eq!(s.matches("=IFCWALL(").count(), 1);
        assert_eq!(s.matches("=IFCMATERIAL(").count(), 0);
        assert_eq!(s.matches("=IFCRELASSOCIATESMATERIAL(").count(), 0);
        assert_refs_resolve(&s);
    }

    #[test]
    fn deterministic_byte_identical() {
        let build = || {
            let (mesh, a, b) = two_box_mesh();
            let elements = vec![
                IfcElement { name: "A".into(), material_name: Some("C".into()), material_style: None, kind: crate::IfcElementKind::Wall, face_ids: a },
                IfcElement { name: "B".into(), material_name: None, material_style: None, kind: crate::IfcElementKind::Wall, face_ids: b },
            ];
            emit_ifc_model(&mesh, &elements, 0.001, "M").unwrap()
        };
        assert_eq!(build(), build(), "deterministic (L-203-2)");
    }

    #[test]
    fn empty_elements_rejected() {
        let (mesh, _a, _b) = two_box_mesh();
        assert!(emit_ifc_model(&mesh, &[], 0.001, "M").is_err());
    }
}

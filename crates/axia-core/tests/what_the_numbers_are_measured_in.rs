//! Does a number that says "m²" mean m²?
//!
//! Everything inside the kernel is millimetres (LOCKED #5: unit = mm, f64), so
//! every number is self-consistent and every test passes whatever the labels
//! say. A unit error does not show up in the geometry — it shows up in a
//! displayed number, an exported file, or a weight.
//!
//! So this measures a cube one metre on a side and walks it through every
//! conversion the app performs, asserting the arithmetic each one claims:
//!
//! ```text
//!   engine        1000 mm cube      area 6,000,000 mm²   volume 1,000,000,000 mm³
//!   Inspector     / 1e6, / 1e9      area 6 m²            volume 1 m³
//!   weight        m³ × kg/m³        2400 kg at concrete density
//!   IFC export    × 0.001, .METRE.  side 1.0 in the file
//! ```
//!
//! ⚠ Constants copied out of the TypeScript assert nothing about the
//! TypeScript. `the_conversions_here_are_the_ones_the_app_performs` reads the
//! real files; without it every test here is arithmetic wearing a
//! measurement's clothes.
//!
//! ⚠ The Inspector prints LENGTHS in raw millimetres with a hardcoded "mm"
//! label, and AREA and VOLUME in m² and m³. Three scales on one panel. Each is
//! labelled, so none of them lies — but nothing there follows the app's unit
//! setting, which `MeasureTool` and the VCB do follow. Measured below and left
//! alone: which way the Inspector should go is a product decision.

use axia_core::scene::Scene;
use axia_core::FORM_MATERIAL;
use glam::DVec3;

/// One metre, in the kernel's own unit.
const METRE_MM: f64 = 1000.0;

/// What the Inspector divides by (`XiaInspector.ts`: `surfaceArea / 1e6`).
const MM2_PER_M2: f64 = 1e6;
/// What the Inspector divides by (`XiaInspector.ts`: `volume / 1e9`).
const MM3_PER_M3: f64 = 1e9;
/// What the IFC exporter multiplies by (`axia-wasm`: `const MM_TO_M = 0.001`).
const MM_TO_M: f64 = 0.001;

/// Read a file from the repository root.
fn repo_file(rel: &str) -> String {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/axia-core -> repo root")
        .join(rel);
    std::fs::read_to_string(&root).unwrap_or_else(|e| panic!("{}: {e}", root.display()))
}

fn one_metre_cube() -> Scene {
    let mut s = Scene::new();
    s.mesh
        .create_box(DVec3::ZERO, METRE_MM, METRE_MM, METRE_MM, FORM_MATERIAL)
        .expect("a cube one metre on a side");
    s
}

fn total_area_mm2(s: &Scene) -> f64 {
    s.mesh
        .faces
        .iter()
        .filter(|(_, f)| f.is_active())
        .map(|(fid, _)| s.mesh.face_area(fid))
        .sum()
}

/// A cube one metre on a side measures 6 m² and 1 m³ — after the conversions,
/// and not before.
#[test]
fn a_metre_cube_is_six_square_metres_and_one_cubic_metre() {
    let s = one_metre_cube();
    let area_mm2 = total_area_mm2(&s);
    let vol_mm3 = s.mesh.mesh_volume();

    let area_m2 = area_mm2 / MM2_PER_M2;
    let vol_m3 = vol_mm3 / MM3_PER_M3;

    println!("\n  1 m cube — what the engine holds, and what the panel shows\n");
    println!("    engine   area {area_mm2:>15.1} mm²    volume {vol_mm3:>18.1} mm³");
    println!("    panel    area {area_m2:>15.4} m²     volume {vol_m3:>18.4} m³\n");

    assert!(
        (area_mm2 - 6.0 * METRE_MM * METRE_MM).abs() < 1.0,
        "the kernel measures in mm², so six faces of a 1000 mm cube are 6e6: {area_mm2}"
    );
    assert!((vol_mm3 - METRE_MM.powi(3)).abs() < 1.0, "and 1e9 mm³: {vol_mm3}");
    assert!((area_m2 - 6.0).abs() < 1e-6, "which the Inspector shows as 6 m²: {area_m2}");
    assert!((vol_m3 - 1.0).abs() < 1e-9, "and 1 m³: {vol_m3}");
}

/// Weight is volume in CUBIC METRES times a density in kg/m³.
///
/// `MaterialLibrary.ts` does `volumeMM3 / 1e9` first, and this pins why: the
/// densities are in kg/m³, so feeding them cubic millimetres would report a
/// concrete cube at 2.4 billion kilograms — and that number looks like a
/// number.
#[test]
fn a_concrete_metre_cube_weighs_two_point_four_tonnes() {
    let s = one_metre_cube();
    let vol_m3 = s.mesh.mesh_volume() / MM3_PER_M3;
    // `MaterialLibrary.ts` concrete: 2400 kg/m³.
    const CONCRETE_KG_PER_M3: f64 = 2400.0;
    let kg = vol_m3 * CONCRETE_KG_PER_M3;
    let unconverted = s.mesh.mesh_volume() * CONCRETE_KG_PER_M3;

    println!("\n  concrete, 1 m³");
    println!("    converted    {kg:>20.1} kg");
    println!("    not          {unconverted:>20.1} kg   <- still looks like a number\n");

    assert!((kg - 2400.0).abs() < 1e-6, "2400 kg, not {kg}");
    assert!(
        unconverted > 1e12,
        "and the unconverted figure is absurd by a factor of a billion, which is \
         why the conversion has to happen before the density is applied"
    );
}

/// The IFC file says METRE, so the coordinates written into it must be metres.
///
/// Getting this pair out of step is the one unit error here that would leave
/// the machine: another BIM tool would read a house as a thousand-metre
/// monolith, and nothing on this side would notice.
#[test]
fn the_ifc_scale_matches_the_unit_the_file_declares() {
    let wasm = repo_file("crates/axia-wasm/src/lib.rs");
    let declares_metre = wasm.contains(".LENGTHUNIT.,$,.METRE.");
    let declares_milli = wasm.contains(".LENGTHUNIT.,.MILLI.,.METRE.");
    let scales = wasm.contains("const MM_TO_M: f64 = 0.001;");

    println!("\n  IFC export");
    println!("    declares METRE {declares_metre}    declares MILLI {declares_milli}");
    println!("    MM_TO_M = 0.001 present  {scales}");
    println!("    -> 1000 mm is written as {}\n", METRE_MM * MM_TO_M);

    assert!(declares_metre, "the exporter declares plain METRE");
    assert!(
        !declares_milli,
        "and NOT milli — if it ever declares both, one of them is writing the \
         wrong numbers and a reader cannot tell which"
    );
    assert!(
        scales,
        "so the coordinates must be scaled on the way out. A file saying METRE \
         with millimetre numbers in it reads as a house a kilometre across"
    );
    let geom = repo_file("crates/axia-ifc/src/ifc_geometry.rs");
    assert!(
        geom.contains("Some(\"MILLI\") => 1e-3"),
        "and the import side scales a MILLI file by 1e-3, or a round trip drifts \
         by a thousand"
    );
}

/// The divisors this file uses are the ones the app actually performs.
///
/// Mutation-checked: change `/ 1e6` to `/ 1e5` in `XiaInspector.ts` and this
/// fails while every other test in the file still passes — which is the point.
#[test]
fn the_conversions_here_are_the_ones_the_app_performs() {
    let inspector = repo_file("web/src/ui/XiaInspector.ts");
    let materials = repo_file("web/src/materials/MaterialLibrary.ts");
    let menubar = repo_file("web/src/ui/MenuBar.ts");

    let checks: [(&str, bool); 4] = [
        ("Inspector area    mm2 / 1e6", inspector.contains("surfaceArea || 0) / 1e6")),
        ("Inspector volume  mm3 / 1e9", inspector.contains("volume || 0) / 1e9")),
        ("weight            mm3 / 1e9, then x kg/m3", materials.contains("volumeMM3 / 1e9")),
        ("clash volume      mm3 / 1e9", menubar.contains("totalVol / 1e9")),
    ];
    println!("\n  what the app actually divides by\n");
    for (what, ok) in &checks {
        println!("    {}  {what}", if *ok { "ok  " } else { "GONE" });
    }
    println!();
    for (what, ok) in checks {
        assert!(ok, "{what} — the app changed and this file did not");
    }
}

/// glTF says metres. We write millimetres into it, unscaled.
///
/// ⚠ This is a finding, not a passing check dressed as one. IFC declares
/// `.METRE.` and scales by `MM_TO_M`; glTF's specification also states metres
/// ("The units for all linear distances are meters") and the exporter hands
/// `viewport.scene` — kernel units, millimetres — straight to `GLTFExporter`
/// with no scale anywhere on the way:
///
/// ```text
///   IFC     declares METRE   x 0.001   ->  1 m cube exports as 1.0      ok
///   DXF     $INSUNITS = 4    none      ->  ...as 1000, and 4 IS mm      ok
///   OBJ/STL unitless         none      ->  nothing to disagree with     ok
///   glTF    spec says METRE  none      ->  ...as 1000  = ONE KILOMETRE
/// ```
///
/// Whether to fix it is a product decision, because changing it changes every
/// glTF this app will ever write. Recorded here so it is a measurement rather
/// than a rumour, and so the day someone scales it, this test says so.
#[test]
fn gltf_is_exported_unscaled_although_its_spec_says_metres() {
    let menubar = repo_file("web/src/ui/MenuBar.ts");
    let hands_raw_scene = menubar.contains("lazyExportGltf(viewport.scene");
    let exporter_gets_scene = menubar.contains("exporter.parseAsync(scene3d");
    // No conversion constant anywhere near the mesh exporters.
    let scales_anywhere = menubar.contains("MM_TO_M") || menubar.contains("0.001");

    println!("\n  glTF export");
    println!("    hands viewport.scene straight over   {hands_raw_scene}");
    println!("    exporter receives it unchanged       {exporter_gets_scene}");
    println!("    any mm->m conversion in MenuBar      {scales_anywhere}");
    println!("    -> a 1 m cube leaves as 1000, and glTF reads that as 1 km\n");

    assert!(hands_raw_scene && exporter_gets_scene, "the scene goes over as it is");
    assert!(
        !scales_anywhere,
        "if a conversion has appeared, glTF was fixed — rewrite this test to \
         assert the new scale instead of deleting it"
    );
}

/// Most of the app follows the unit setting. The Inspector does not.
///
/// `UnitSystem` supports mm / cm / m / in / ft, and `MeasureTool` and the VCB
/// route their numbers through `units.format(...)`. The Inspector prints raw
/// millimetres against a hardcoded "mm" label, and its area and volume against
/// hardcoded "m²" and "m³".
///
/// So nothing there LIES — every number sits next to the unit it is in — but
/// set the document to feet and one panel keeps answering in millimetres while
/// the measure tool answers in feet. Recorded, not fixed: which way the
/// Inspector should go is a product decision, not one the kernel can settle.
#[test]
fn the_inspector_is_the_one_surface_that_ignores_the_unit_setting() {
    let inspector = repo_file("web/src/ui/XiaInspector.ts");
    let measure = repo_file("web/src/tools/MeasureTool.ts");
    let vcb = repo_file("web/src/ui/VCB.ts");
    let index = repo_file("web/index.html");

    let inspector_uses_units =
        inspector.contains("units.format(") || inspector.contains("unitSystem");
    let measure_uses_units = measure.contains("units.format(");
    let vcb_uses_units = vcb.contains("units.format(");
    let hardcoded_mm = index.contains("<div class=\"xi-dim-unit\">mm</div>");

    println!("\n  follows the unit setting?\n");
    println!("    MeasureTool     {measure_uses_units}");
    println!("    VCB             {vcb_uses_units}");
    println!("    XiaInspector    {inspector_uses_units}");
    println!("    Inspector length label hardcoded mm: {hardcoded_mm}\n");

    assert!(
        measure_uses_units && vcb_uses_units,
        "the measuring surfaces follow the setting"
    );
    assert!(
        !inspector_uses_units && hardcoded_mm,
        "and the Inspector does not — if it starts to, this file's account of the \
         split is out of date and should be rewritten, not deleted"
    );
}

//! Dump the β-1 IfcFacetedBrep unit cube as IFC4.3 STEP-21 text.
//! `cargo run -p axia-ifc --example dump_cube`
fn main() {
    print!("{}", axia_ifc::emit_unit_cube());
}

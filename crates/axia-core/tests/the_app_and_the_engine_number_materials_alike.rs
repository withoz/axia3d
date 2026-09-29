//! THE APP AND THE ENGINE NUMBER MATERIALS ALIKE (ADR-313 D1, L-313-1/2).
//!
//! The app sends a material to the engine as a bare number — `rustId` in
//! `web/src/materials/MaterialLibrary.ts` — and the engine looks that number up
//! in its own library. Nothing checked that the two tables agree, and until
//! 2026-09-29 they did not: the app numbered its built-ins 1..12 with 0 meaning
//! "no material", the engine numbered them from 0. Measured in a real browser,
//! every one of the twelve materials the Inspector offers was recorded as the
//! next one — 콘크리트 as 강철, 철강 as 목재 … 토양 as 타일 — and 타일 as nothing,
//! because the engine had no 12. A concrete box exported as IFCMATERIAL('강철').
//!
//! Worse, the engine's Concrete sat on `FORM_MATERIAL`, the form-layer sentinel
//! for "no material": the IFC export skips that id, promotion refuses it, and a
//! concrete member imported from IFC lost its material.
//!
//! ## Why this reads the app's source
//!
//! The link is a NUMBER crossing a language boundary, so neither compiler holds
//! it. This test builds the engine's REAL library (`MaterialLibrary::new()`) and
//! parses the app's table, and requires every app built-in's `rustId` to be the
//! engine id of the material with the same identity. The identity key is the
//! app's `id` against the engine's `name_en` lower-cased — the one field the two
//! tables share exactly (the Korean names and the categories do not: 철강/강철,
//! 석고보드/석고, 토양/흙 — synonyms, left alone, see ADR-313 §4).
//!
//! ## What makes it non-vacuous
//!
//! The parse must find all twelve, and the engine must hold twelve built-ins.
//! A table the regex stopped reading would otherwise pass on nothing.
//! ⚠ Mutation-checked: start the engine's numbering at 0 again, or shift one
//! `rustId` in the app's table, and this fails.

use axia_core::material::MaterialLibrary;
use axia_core::FORM_MATERIAL;
use axia_geo::MaterialId;

const APP_TABLE: &str = include_str!("../../../web/src/materials/MaterialLibrary.ts");

/// `(id, rustId)` of every entry in the app's `BUILTIN_MATERIALS` array.
fn app_builtins() -> Vec<(String, u32)> {
    let start = APP_TABLE
        .find("const BUILTIN_MATERIALS")
        .expect("the app's built-in table must still be called BUILTIN_MATERIALS");
    let body = &APP_TABLE[start..];
    let end = body.find("\n];").expect("the built-in table must close with `];`");
    let body = &body[..end];
    let mut out = Vec::new();
    for line in body.lines() {
        let line = line.trim();
        // `id: 'concrete', rustId: 1,`
        let Some(rest) = line.strip_prefix("id: '") else { continue };
        let Some((id, rest)) = rest.split_once('\'') else { continue };
        let Some(num) = rest.split("rustId:").nth(1) else { continue };
        let digits: String = num.trim().chars().take_while(|c| c.is_ascii_digit()).collect();
        out.push((id.to_string(), digits.parse().expect("rustId must be a number")));
    }
    out
}

#[test]
fn every_material_the_app_offers_is_the_one_the_engine_records() {
    let app = app_builtins();
    assert_eq!(
        app.len(),
        12,
        "expected the app's twelve built-ins, parsed {} — the table changed shape \
         and this guard would be checking nothing",
        app.len()
    );

    let engine = MaterialLibrary::new();
    let mut wrong = Vec::new();
    for (id, rust_id) in &app {
        let recorded = engine.get(MaterialId::new(*rust_id));
        let same = recorded.is_some_and(|m| m.name_en.to_lowercase() == *id);
        if !same {
            let meant = engine
                .all()
                .into_iter()
                .find(|m| m.name_en.to_lowercase() == *id)
                .map(|m| m.id.raw());
            wrong.push(format!(
                "app '{id}' sends {rust_id}; the engine's {rust_id} is {:?}, its '{id}' is {:?}",
                recorded.map(|m| m.name_en.as_str()),
                meant
            ));
        }
    }
    assert!(
        wrong.is_empty(),
        "the app and the engine number materials differently — the material a user \
         picks is not the one the engine records:\n  {}",
        wrong.join("\n  ")
    );
}

#[test]
fn no_material_is_not_a_material() {
    // FORM_MATERIAL is the form layer's "no material" (LOCKED #26). If the
    // library held an entry at that id, that material could never be exported,
    // promoted, or told apart from nothing — which is what happened to Concrete.
    assert_eq!(FORM_MATERIAL.raw(), 0);
    let engine = MaterialLibrary::new();
    assert!(
        engine.get(FORM_MATERIAL).is_none(),
        "the library holds a material at FORM_MATERIAL's id ({:?}) — that material \
         is indistinguishable from 'no material'",
        engine.get(FORM_MATERIAL).map(|m| m.name_en.as_str())
    );
}

#[test]
fn the_built_ins_are_one_through_twelve_in_the_apps_order() {
    let engine = MaterialLibrary::new();
    let order: Vec<(u32, String)> = engine
        .all()
        .into_iter()
        .map(|m| (m.id.raw(), m.name_en.to_lowercase()))
        .collect();
    let app: Vec<(u32, String)> = app_builtins().into_iter().map(|(id, n)| (n, id)).collect();
    assert_eq!(
        order, app,
        "the engine's built-ins must be exactly the app's twelve, at the app's numbers"
    );
}

//! AXiA `axia-foreign` — 자체 STEP / IGES 파서 (Phase G Stage 4-B spike).
//!
//! ## Mission
//!
//! ADR-035 P20.2 의 자체 파서 spike. 12개월 후 P20.E 트리거 만족 시
//! OCCT.js 옵션을 대체하여 default 로 promote.
//!
//! ## Scope (MVP, ADR-035 P20.2)
//!
//! - **STEP AP203** import: `B_SPLINE_SURFACE_WITH_KNOTS`, `BOUNDED_CURVE`,
//!   `CARTESIAN_POINT`, `ADVANCED_FACE`, `EDGE_CURVE`
//! - **IGES 5.3** import: Type 128 (NURBS surface), Type 126 (NURBS curve),
//!   Type 110 (line), Type 100 (circle), Type 116 (point)
//! - **Round-trip Export**: 같은 entity types 의 reverse path
//! - **Promotion** to `axia_geo::AnalyticCurve` / `AnalyticSurface` —
//!   ADR-036 P21 매핑 표 그대로 (Stage 4-A OCCT.js 경로와 동일 매핑)
//!
//! ## Where this actually stands, measured 2026-08-23
//!
//! An audit listed "3 TODOs in axia-foreign" as the remaining work. Two of
//! those three describe work that is already done somewhere else, so here is
//! what a `grep` for callers says instead:
//!
//! ```text
//!   step_lexer + step_parser        1218   IN PRODUCTION — axia-ifc imports
//!                                          `step_parser`; IFC is ISO 10303-21
//!                                          physical file syntax, same grammar
//!   step_resolver                    626   written, no production caller
//!   promote_step_curve/_surface     2370   written — Line / Circle / BSpline /
//!                                          Ellipse / Trimmed / Parabola /
//!                                          Hyperbola — no production caller
//!   conic_converter + sweep          894   written, reached from the above
//!   iges                             167   STUB. the 5-step TODO is real
//! ```
//!
//! ⚠ `promote_curve::promote()` and `promote_surface::promote()` — the two
//! that carry "TODO (Stage 4-B 본체)" — have ZERO callers. They take a bare
//! kind enum with no entity data, and the live path is `step.rs` calling
//! `promote_step_curve` / `promote_step_surface`, which are implemented. The
//! TODO lists work that exists thirty lines further down the same file.
//!
//! `tests/cube_roundtrip.rs` parses a real `cube.stp`: 12 LINEs, 6 PLANEs,
//! geometry checked. The STEP side is not a spike any more.
//!
//! What is genuinely missing is narrower than the ADR reads: IGES (a stub),
//! and a caller for the promote layer. Note that STEP-as-a-CAD-format import
//! in the app does NOT come through here — it goes through OCCT.js, Stage 4-A
//! (ADR-082 / ADR-083), which shipped. So ADR-035 P20.E's twelve-month
//! comparison is between a shipped 4-A and a 4-B whose parser is already
//! carrying IFC in production.
//!
//! ## Out of Scope
//!
//! - AP242 / AP238 / IFC — 별도 ADR
//! - Drawing views / annotations / PMI — ADR-035 P20.B
//! - Assembly hierarchy — ADR-035 P20.B
//!
//! ## 의존성 정책 (zero-deps)
//!
//! 외부 STEP/IGES 라이브러리 의존 0. `axia-geo` (AnalyticCurve / Surface
//! enum) + `glam` + `anyhow` + `serde` 만 사용. ADR-035 P20.2 명시 사항.

pub mod step;
pub mod step_lexer;
pub mod step_parser;
pub mod step_resolver;
pub mod conic_converter;
pub mod sweep_converter;
pub mod iges;
pub mod promote_curve;
pub mod promote_surface;

pub use step::StepImporter;
pub use iges::IgesImporter;

/// Format identifier — caller dispatch.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForeignFormat {
    StepAp203,
    StepAp214,
    StepAp242,
    Iges53,
}

/// Import 결과 — promoted geometry + warnings.
///
/// Promotion 매핑은 ADR-036 P21 의 Stage 4-A / 4-B 공통 결정에 의거.
/// `warnings` 는 ADR-036 P21.7 의 6 case 누적 (DownCast 실패 / 변환
/// 정확도 미달 / fitting tolerance 초과 / rational SSI / PCurve missing /
/// self-intersection).
#[derive(Clone, Debug, Default)]
pub struct ImportResult {
    pub format: Option<ForeignFormat>,
    pub curves: Vec<promote_curve::CurvePromotion>,
    pub surfaces: Vec<promote_surface::SurfacePromotion>,
    pub warnings: Vec<String>,
}

impl ImportResult {
    pub fn new(format: ForeignFormat) -> Self {
        Self {
            format: Some(format),
            curves: Vec::new(),
            surfaces: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.curves.is_empty() && self.surfaces.is_empty()
    }
}

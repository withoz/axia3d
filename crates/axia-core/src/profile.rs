//! What a line is thick with.
//!
//! A line has length and nothing else. A structural member — a column, a beam,
//! a brace — is a line that has been given a cross-section, and that section is
//! what turns a length into a quantity: volume, weight, cost. Until now a
//! promoted line reported `cross_section_area = 1.0`, a placeholder that made
//! every take-off silently wrong rather than honestly unknown.
//!
//! This is the section. It is attached to the EDGE rather than to the Shape or
//! the Xia, so it survives promotion and demotion without anyone having to
//! carry it across — both citizenships point at the same edge.
//!
//! The shapes here are the ones the engine already reads and writes on the IFC
//! side, so a member drawn in AxiA and a member arriving from another tool can
//! be described the same way. `Polygon` is the escape for everything else,
//! including the parametric sections (I, T, U, L, C, Z) that arrive already
//! flattened to an outline.
use serde::{Deserialize, Serialize};

/// The cross-section a line carries.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Profile {
    /// Width × height, centred on the line.
    Rectangular { width: f64, height: f64 },
    /// A round bar or pipe of this radius.
    Circular { radius: f64 },
    /// Any other outline, as a closed polygon in the section's own plane.
    /// Points are not repeated: the last joins back to the first.
    Polygon { points: Vec<(f64, f64)> },
}

impl Profile {
    /// The section's area, in the square of whatever unit the points are in.
    ///
    /// Returns `None` when the section is degenerate — a zero or negative
    /// dimension, or an outline with no enclosed area. A member cannot be made
    /// of nothing, and saying so beats reporting a zero as if it were measured.
    pub fn area(&self) -> Option<f64> {
        match self {
            Profile::Rectangular { width, height } => {
                (*width > 0.0 && *height > 0.0).then(|| width * height)
            }
            Profile::Circular { radius } => {
                (*radius > 0.0).then(|| std::f64::consts::PI * radius * radius)
            }
            Profile::Polygon { points } => {
                if points.len() < 3 {
                    return None;
                }
                // Shoelace. Taken as an absolute value so the answer does not
                // depend on which way round the outline was written.
                let mut twice = 0.0;
                for i in 0..points.len() {
                    let (x0, y0) = points[i];
                    let (x1, y1) = points[(i + 1) % points.len()];
                    twice += x0 * y1 - x1 * y0;
                }
                let a = twice.abs() * 0.5;
                (a > 0.0).then_some(a)
            }
        }
    }

    /// A short name for the section, for the inspector and for messages.
    pub fn describe(&self) -> String {
        match self {
            Profile::Rectangular { width, height } => {
                format!("{}×{}", trim(*width), trim(*height))
            }
            Profile::Circular { radius } => format!("⌀{}", trim(radius * 2.0)),
            Profile::Polygon { points } => format!("{}각형", points.len()),
        }
    }
}

/// Whole numbers read as whole numbers; anything else keeps three decimals.
fn trim(x: f64) -> String {
    if (x - x.round()).abs() < 1e-9 {
        format!("{}", x.round() as i64)
    } else {
        format!("{x:.3}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rectangle_is_width_times_height() {
        assert_eq!(
            Profile::Rectangular { width: 200.0, height: 400.0 }.area(),
            Some(80_000.0)
        );
    }

    #[test]
    fn a_circle_is_pi_r_squared() {
        let a = Profile::Circular { radius: 10.0 }.area().unwrap();
        assert!((a - std::f64::consts::PI * 100.0).abs() < 1e-9, "{a}");
    }

    /// The same square, written both ways round, has the same area — an outline
    /// arriving from elsewhere may be wound either way.
    #[test]
    fn winding_does_not_change_the_area() {
        let ccw = Profile::Polygon {
            points: vec![(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)],
        };
        let cw = Profile::Polygon {
            points: vec![(0.0, 0.0), (0.0, 10.0), (10.0, 10.0), (10.0, 0.0)],
        };
        assert_eq!(ccw.area(), Some(100.0));
        assert_eq!(cw.area(), Some(100.0));
    }

    /// An I-section, as the IFC reader hands it over: bounding box centred,
    /// sharp corners. Area = two flanges + the web between them.
    #[test]
    fn a_flattened_i_section_measures_correctly() {
        let (b, h, tw, tf) = (200.0, 400.0, 10.0, 15.0);
        let (hb, hh, hw) = (b / 2.0, h / 2.0, tw / 2.0);
        let i = Profile::Polygon {
            points: vec![
                (-hb, -hh), (hb, -hh), (hb, -hh + tf), (hw, -hh + tf),
                (hw, hh - tf), (hb, hh - tf), (hb, hh), (-hb, hh),
                (-hb, hh - tf), (-hw, hh - tf), (-hw, -hh + tf), (-hb, -hh + tf),
            ],
        };
        let expected = 2.0 * b * tf + (h - 2.0 * tf) * tw;
        let got = i.area().unwrap();
        assert!((got - expected).abs() < 1e-9, "got {got}, expected {expected}");
    }

    #[test]
    fn a_section_with_no_thickness_has_no_area() {
        assert_eq!(Profile::Rectangular { width: 200.0, height: 0.0 }.area(), None);
        assert_eq!(Profile::Circular { radius: 0.0 }.area(), None);
        assert_eq!(Profile::Polygon { points: vec![(0.0, 0.0), (1.0, 1.0)] }.area(), None);
        // Three points on one line enclose nothing.
        assert_eq!(
            Profile::Polygon { points: vec![(0.0, 0.0), (1.0, 0.0), (2.0, 0.0)] }.area(),
            None
        );
    }

    #[test]
    fn it_says_what_it_is() {
        assert_eq!(Profile::Rectangular { width: 200.0, height: 400.0 }.describe(), "200×400");
        assert_eq!(Profile::Circular { radius: 50.0 }.describe(), "⌀100");
    }
}

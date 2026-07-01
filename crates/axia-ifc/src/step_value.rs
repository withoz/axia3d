//! ISO-10303-21 (STEP-21) value model + formatter SSOT (ADR-203 L-203-3).
//!
//! Every STEP attribute value is a [`StepValue`]; [`StepValue::fmt`] is the
//! single source of truth for STEP-21 text serialization. The formatters are
//! hardcoded (no locale, no randomness) → deterministic byte-identical output
//! (L-203-2).

/// Reference to an entity instance (`#N`). Newtype over the 1-based id.
#[derive(Copy, Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct EntityRef(pub(crate) u32);

impl EntityRef {
    #[inline]
    pub fn id(self) -> u32 {
        self.0
    }
}

/// A STEP-21 attribute value (ISO-10303-21 §10).
#[derive(Clone, Debug, PartialEq)]
pub enum StepValue {
    /// `#N` reference.
    Ref(EntityRef),
    /// Integer literal.
    Int(i64),
    /// REAL literal (always contains a `.`).
    Real(f64),
    /// `'...'` string (escaped).
    Str(String),
    /// `.IDENT.` enumeration.
    Enum(String),
    /// `(a,b,c)` list.
    List(Vec<StepValue>),
    /// `$` unset/null.
    Unset,
    /// `*` derived.
    Derived,
    /// `TAG(args)` typed/defined value (e.g. `IFCBOOLEAN(.T.)`).
    Typed(&'static str, Vec<StepValue>),
}

impl StepValue {
    /// Convenience: a `#N` ref value.
    #[inline]
    pub fn r(e: EntityRef) -> StepValue {
        StepValue::Ref(e)
    }

    /// Serialize this value to ISO-10303-21 text.
    pub fn fmt(&self) -> String {
        match self {
            StepValue::Ref(r) => format!("#{}", r.0),
            StepValue::Int(i) => i.to_string(),
            StepValue::Real(x) => fmt_real(*x),
            StepValue::Str(s) => fmt_string(s),
            StepValue::Enum(e) => format!(".{}.", e),
            StepValue::List(vs) => fmt_list(vs),
            StepValue::Unset => "$".to_string(),
            StepValue::Derived => "*".to_string(),
            StepValue::Typed(tag, args) => format!("{}({})", tag, join_fmt(args)),
        }
    }
}

fn join_fmt(vs: &[StepValue]) -> String {
    let parts: Vec<String> = vs.iter().map(StepValue::fmt).collect();
    parts.join(",")
}

fn fmt_list(vs: &[StepValue]) -> String {
    format!("({})", join_fmt(vs))
}

/// REAL formatter. ISO-10303-21 REAL must contain a `.`. Integer-valued reals
/// get a trailing dot (`1.`); very small (`|x| < 1e-6`) or very large
/// (`|x| >= 1e6`) magnitudes use `E` notation. Shortest round-trip otherwise.
pub fn fmt_real(x: f64) -> String {
    if !x.is_finite() || x == 0.0 {
        return "0.".to_string();
    }
    let ax = x.abs();
    let mut s = if ax >= 1e6 || ax < 1e-6 {
        format!("{:E}", x) // e.g. "1.5E6", "2E6", "1E-7"
    } else {
        format!("{}", x) // shortest round-trip: "1", "1.5", "0.001"
    };
    // Guarantee a '.' in the mantissa (before any exponent).
    if let Some(epos) = s.find(['E', 'e']) {
        if !s[..epos].contains('.') {
            s.insert(epos, '.'); // "2E6" → "2.E6", "1E-7" → "1.E-7"
        }
    } else if !s.contains('.') {
        s.push('.'); // "1" → "1.", "-2" → "-2."
    }
    s
}

/// STRING formatter. Wraps in `'...'`, escapes `'` as `''`, and encodes
/// non-ASCII (or control) characters as `\X2\HHHH...\X0\` (UTF-16 units).
pub fn fmt_string(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    out.push('\'');
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\'' {
            out.push_str("''");
        } else if ch.is_ascii() && !ch.is_control() {
            out.push(ch);
        } else {
            // \X2\ <4-hex per UTF-16 code unit, possibly multiple> \X0\.
            out.push_str("\\X2\\");
            let mut buf = [0u16; 2];
            for u in ch.encode_utf16(&mut buf) {
                out.push_str(&format!("{:04X}", u));
            }
            // coalesce consecutive non-ASCII into one \X2\..\X0\ run.
            while let Some(&nx) = chars.peek() {
                if nx.is_ascii() && !nx.is_control() || nx == '\'' {
                    break;
                }
                let nx = chars.next().unwrap();
                let mut b2 = [0u16; 2];
                for u in nx.encode_utf16(&mut b2) {
                    out.push_str(&format!("{:04X}", u));
                }
            }
            out.push_str("\\X0\\");
        }
    }
    out.push('\'');
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fmt_real_integer_valued_gets_trailing_dot() {
        assert_eq!(fmt_real(0.0), "0.");
        assert_eq!(fmt_real(1.0), "1.");
        assert_eq!(fmt_real(-2.0), "-2.");
        assert_eq!(fmt_real(100.0), "100.");
    }

    #[test]
    fn fmt_real_fractional_shortest() {
        assert_eq!(fmt_real(1.5), "1.5");
        assert_eq!(fmt_real(0.5), "0.5");
        assert_eq!(fmt_real(0.001), "0.001");
        assert_eq!(fmt_real(1234.5), "1234.5");
    }

    #[test]
    fn fmt_real_scientific_has_dot_in_mantissa() {
        // very small (< 1e-6) and very large (>= 1e6) use E-notation, with a
        // '.' guaranteed in the mantissa (ISO-10303-21 REAL grammar).
        assert_eq!(fmt_real(1e-7), "1.E-7");
        assert_eq!(fmt_real(2e6), "2.E6");
        assert_eq!(fmt_real(1.5e6), "1.5E6");
        for s in [fmt_real(1e-7), fmt_real(2e6), fmt_real(1.5e9)] {
            let mant = s.split(['E', 'e']).next().unwrap();
            assert!(mant.contains('.'), "mantissa needs a dot: {}", s);
        }
    }

    #[test]
    fn fmt_real_non_finite_safe() {
        assert_eq!(fmt_real(f64::NAN), "0.");
        assert_eq!(fmt_real(f64::INFINITY), "0.");
    }

    #[test]
    fn fmt_real_deterministic() {
        // Same input → same output (no locale/rng).
        for x in [0.0, 1.0, 1.5, -2.5, 1e-7, 2e6, 3.14159] {
            assert_eq!(fmt_real(x), fmt_real(x));
        }
    }

    #[test]
    fn fmt_string_escapes_quote() {
        assert_eq!(fmt_string("AXiA"), "'AXiA'");
        assert_eq!(fmt_string("it's"), "'it''s'");
        assert_eq!(fmt_string(""), "''");
    }

    #[test]
    fn fmt_string_non_ascii_x2() {
        // '가' = U+AC00 → \X2\AC00\X0\.
        assert_eq!(fmt_string("가"), "'\\X2\\AC00\\X0\\'");
        // mixed: ASCII + non-ASCII coalesced run.
        assert_eq!(fmt_string("a가나b"), "'a\\X2\\AC00B098\\X0\\b'");
    }

    #[test]
    fn value_fmt_variants() {
        assert_eq!(StepValue::Ref(EntityRef(42)).fmt(), "#42");
        assert_eq!(StepValue::Int(-7).fmt(), "-7");
        assert_eq!(StepValue::Enum("ADDED".into()).fmt(), ".ADDED.");
        assert_eq!(StepValue::Unset.fmt(), "$");
        assert_eq!(StepValue::Derived.fmt(), "*");
        assert_eq!(
            StepValue::List(vec![StepValue::Real(0.0), StepValue::Real(1.0)]).fmt(),
            "(0.,1.)"
        );
        assert_eq!(
            StepValue::Typed("IFCBOOLEAN", vec![StepValue::Enum("T".into())]).fmt(),
            "IFCBOOLEAN(.T.)"
        );
        // nested list of refs
        assert_eq!(
            StepValue::List(vec![
                StepValue::Ref(EntityRef(1)),
                StepValue::Ref(EntityRef(2))
            ])
            .fmt(),
            "(#1,#2)"
        );
    }
}

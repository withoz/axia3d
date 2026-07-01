//! Deterministic IFC GlobalId (22-char compressed GUID) — ADR-203 L-203-2.
//!
//! IfcRoot subtypes (IfcProject, IfcSite, IfcWall, ...) carry a 22-char
//! `IfcGloballyUniqueId`. We derive it deterministically from a u128 seed
//! (NO wall-clock, NO getrandom) so the same model exports byte-identically.
//! The 128 bits map to 22 chars: 2 bits in the first char + 21×6 bits.

const ALPHABET: &[u8; 64] =
    b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz_$";

/// Encode a 128-bit value to a 22-char IFC GlobalId string (valid alphabet).
pub fn ifc_guid(v: u128) -> String {
    let mut chars = [0u8; 22];
    // first char: top 2 bits (126..128)
    chars[0] = ALPHABET[((v >> 126) & 0x3) as usize];
    // remaining 21 chars: 6 bits each, bits 125..0
    let mut shift: i32 = 120;
    for c in chars.iter_mut().skip(1) {
        *c = ALPHABET[((v >> shift) & 0x3F) as usize];
        shift -= 6;
    }
    // safe: ALPHABET is ASCII
    String::from_utf8(chars.to_vec()).unwrap()
}

/// Derive a deterministic GUID for the `index`-th IfcRoot entity of an export,
/// mixed with a fixed namespace (FNV-1a-ish splmix so distinct indices differ).
pub fn ifc_guid_for(index: u64) -> String {
    // SplitMix64 finalizer on (namespace ^ index), widened to 128 bits with a
    // second mix — purely deterministic.
    const NS: u64 = 0x4158_6941_5F49_4643; // "AXiA_IFC" bytes
    let lo = splitmix64(NS ^ index);
    let hi = splitmix64(lo ^ 0x9E37_79B9_7F4A_7C15);
    ifc_guid(((hi as u128) << 64) | lo as u128)
}

fn splitmix64(mut z: u64) -> u64 {
    z = z.wrapping_add(0x9E37_79B9_7F4A_7C15);
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guid_is_22_chars_valid_alphabet() {
        let g = ifc_guid_for(0);
        assert_eq!(g.len(), 22, "IFC GlobalId is 22 chars");
        assert!(
            g.bytes().all(|b| ALPHABET.contains(&b)),
            "all chars in IFC base64 alphabet: {}",
            g
        );
    }

    #[test]
    fn guid_deterministic() {
        assert_eq!(ifc_guid_for(7), ifc_guid_for(7), "same index → same GUID");
        assert_eq!(ifc_guid(0x1234_5678), ifc_guid(0x1234_5678));
    }

    #[test]
    fn guid_distinct_indices_differ() {
        let a = ifc_guid_for(0);
        let b = ifc_guid_for(1);
        let c = ifc_guid_for(2);
        assert_ne!(a, b);
        assert_ne!(b, c);
        assert_ne!(a, c);
    }

    #[test]
    fn guid_zero_and_max_well_formed() {
        let z = ifc_guid(0);
        let m = ifc_guid(u128::MAX);
        assert_eq!(z.len(), 22);
        assert_eq!(m.len(), 22);
        assert!(z.bytes().all(|b| ALPHABET.contains(&b)));
        assert!(m.bytes().all(|b| ALPHABET.contains(&b)));
        // 0 → all '0', MAX top-2-bits=3 → first char '3', rest '$' (index 63).
        assert_eq!(z, "0000000000000000000000");
        assert_eq!(&m[..1], "3");
    }
}

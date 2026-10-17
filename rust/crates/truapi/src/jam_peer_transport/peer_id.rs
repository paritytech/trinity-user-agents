//! JAMNP-S alternative-name text form of Ed25519 peer keys.
//!
//! `N(k) = "e" ++ B(E32^-1(k), 52)`: the 256-bit key read as a little-endian
//! integer, emitted five bits at a time (least significant first) through the
//! alphabet `abcdefghijklmnopqrstuvwxyz234567`.

const ALPHABET: &[u8; 32] = b"abcdefghijklmnopqrstuvwxyz234567";
/// One prefix letter plus 52 base-32 digits.
const TEXT_LEN: usize = 1 + 256_usize.div_ceil(5);

/// Alternative name of an Ed25519 peer key (`e…`).
pub(super) fn ed25519_text(key: &[u8; 32]) -> String {
    let mut text = String::with_capacity(TEXT_LEN);
    text.push('e');
    for bit in (0..256).step_by(5) {
        let low = u16::from(key[bit / 8]);
        let high = u16::from(key.get(bit / 8 + 1).copied().unwrap_or(0));
        let window = low | (high << 8);
        text.push(char::from(
            ALPHABET[usize::from((window >> (bit % 8)) & 0x1f)],
        ));
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A PolkaJAM node logs its key as `This node is <name>@<addr>`; the name
    /// must match byte for byte or the peer's certificate check fails.
    #[test]
    fn matches_the_name_a_polkajam_node_logs_for_its_key() {
        let key: [u8; 32] =
            hex::decode("1d60a595caeb8e8a8e7e74f4364ee070264d033fed29cb5c5b0c70d4fb65de46")
                .unwrap()
                .try_into()
                .unwrap();
        assert_eq!(
            ed25519_text(&key),
            "e5ayk2kkzlxdvih2pud5ndhb4qtj2ub4hnpkwmonlma4i55xm6wra"
        );
    }
}

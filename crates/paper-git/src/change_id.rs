use std::fmt;
use std::str::FromStr;

/// Stable identity of a save that survives rewrites (amend, rebase)
///
/// 16 random bytes, displayed as 32 "reverse hex" letter in `l..=z`
/// (0 -> z, 1 -> y, ..., f -> k) so it can never be mistaken for a commit hash.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ChangeId([u8; 16]);

#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum ParseChangeIdError {
    #[error("a change ID has 32 letters, got {0}")]
    WrongLength(usize),

    #[error("'{0}' is not a change ID letter (k to z)")]
    InvalidChar(char),
}

impl ChangeId {
    pub const fn from_bytes(bytes: [u8; 16]) -> Self {
        Self(bytes)
    }

    pub const fn as_bytes(&self) -> &[u8; 16] {
        &self.0
    }

    /// Number of letters in a displayed change ID
    pub const LEN: usize = 32;
    /// Short IDs are never displayed with fewer letters than this
    pub const MIN_PREFIX_LEN: usize = 4;

    /// A fresh, random change ID
    pub fn random() -> Self {
        Self(rand::random())
    }

    /// Length of the shortest prefix of `self` that no ID in `others` shares.
    ///
    /// Never shorter than [`Self::MIN_PREFIX_LEN`]. IDs equal to `self` are ignored,
    /// so passing the full list of known IDs (including this one) is fine
    pub fn unique_prefix_len<'a>(&self, others: impl IntoIterator<Item = &'a ChangeId>) -> usize {
        others
            .into_iter()
            .filter(|other| *other != self)
            .map(|other| self.common_prefix_len(other) + 1)
            .fold(Self::MIN_PREFIX_LEN, usize::max)
    }

    /// Number of leading letters (nibbles) shared with `other`
    fn common_prefix_len(&self, other: &ChangeId) -> usize {
        self.0
            .iter()
            .zip(other.0.iter())
            .position(|(a, b)| a != b)
            .map_or(Self::LEN, |i| {
                let same_high_nibble = self.0[i] >> 4 == other.0[i] >> 4;
                2 * i + usize::from(same_high_nibble)
            })
    }
}

impl fmt::Display for ChangeId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(
                f,
                "{}{}",
                nibble_to_letter(byte >> 4),
                nibble_to_letter(byte & 0x0f)
            )?;
        }
        Ok(())
    }
}

impl FromStr for ChangeId {
    type Err = ParseChangeIdError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let len = s.chars().count();
        if len != 32 {
            return Err(ParseChangeIdError::WrongLength(len));
        }
        let nibbles = s
            .chars()
            .map(letter_to_nibble)
            .collect::<Result<Vec<u8>, _>>()?;
        let mut bytes = [0u8; 16];
        for (byte, pair) in bytes.iter_mut().zip(nibbles.chunks_exact(2)) {
            *byte = (pair[0] << 4) | pair[1];
        }
        Ok(Self(bytes))
    }
}

fn nibble_to_letter(nibble: u8) -> char {
    char::from(b'z' - nibble)
}

fn letter_to_nibble(letter: char) -> Result<u8, ParseChangeIdError> {
    match letter {
        'k'..='z' => Ok(b'z' - letter as u8),
        other => Err(ParseChangeIdError::InvalidChar(other)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: [u8; 16] = [
        0x01, 0x23, 0x45, 0x67, 0x89, 0xab, 0xcd, 0xef, 0, 0, 0, 0, 0, 0, 0, 0xff,
    ];
    const SAMPLE_STR: &str = "zyxwvutsrqponmlkzzzzzzzzzzzzzzkk";

    #[test]
    fn displays_each_nibble_as_reverse_hex_letters() {
        assert_eq!(ChangeId::from_bytes(SAMPLE).to_string(), SAMPLE_STR);
    }

    #[test]
    fn parses_what_it_displays() {
        assert_eq!(
            SAMPLE_STR.parse::<ChangeId>(),
            Ok(ChangeId::from_bytes(SAMPLE))
        );
    }

    #[test]
    fn rejects_wrong_length() {
        assert_eq!(
            "zyx".parse::<ChangeId>(),
            Err(ParseChangeIdError::WrongLength(3))
        );
    }

    #[test]
    fn rejects_letters_outside_k_to_z() {
        let with_a = SAMPLE_STR.replacen('z', "a", 1);
        assert_eq!(
            with_a.parse::<ChangeId>(),
            Err(ParseChangeIdError::InvalidChar('a'))
        );
    }

    #[test]
    fn rejects_uppercase() {
        let upper = SAMPLE_STR.to_uppercase();
        assert_eq!(
            upper.parse::<ChangeId>(),
            Err(ParseChangeIdError::InvalidChar('Z'))
        );
    }

    fn id(prefix: &str) -> ChangeId {
        format!("{prefix:z<32}").parse().unwrap()
    }

    #[test]
    fn random_ids_differ_and_round_trip() {
        let a = ChangeId::random();
        let b = ChangeId::random();
        assert_ne!(a, b);
        assert_eq!(a.to_string().parse::<ChangeId>(), Ok(a));
    }

    #[test]
    fn prefix_is_at_least_four_letters() {
        assert_eq!(id("k").unique_prefix_len([]), 4);
        assert_eq!(id("k").unique_prefix_len([&id("m")]), 4);
    }

    #[test]
    fn prefix_ignores_the_id_itself() {
        let me = id("kmrw");
        assert_eq!(me.unique_prefix_len([&me, &id("kmrx")]), 4);
    }

    #[test]
    fn ids_differing_only_in_last_letter_need_all_letters() {
        let almost = id(&format!("{}y", "z".repeat(31)));
        assert_eq!(id("").unique_prefix_len([&almost]), 32);
    }

    #[test]
    fn prefix_extends_past_the_longest_shared_prefix() {
        let others = [id("kkkkkl"), id("kkm"), id("kkkkkkkn")];
        assert_eq!(id("kkkkkkkm").unique_prefix_len(&others), 8);
    }
}

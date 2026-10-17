//! Operation ids.

use std::fmt;
use std::str::FromStr;

use paper_git::ChangeId;
use serde::{Deserialize, Serialize};

const LEN: usize = 32;

/// The id of an operation: 32 lowercase hexadecimal characters of randomness.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct OpId(String);

/// The text was not 32 lowercase hexadecimal characters.
#[derive(Debug, thiserror::Error)]
#[error("'{text}' is not an operation id (32 lowercase hexadecimal characters)")]
pub struct InvalidOpId {
    pub text: String,
}

impl OpId {
    /// A fresh random id (the same randomness source as change IDs).
    #[must_use]
    pub fn random() -> Self {
        use fmt::Write;
        let mut text = String::with_capacity(LEN);
        for byte in ChangeId::random().as_bytes() {
            write!(text, "{byte:02x}").expect("writing to a String cannot fail");
        }
        Self(text)
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for OpId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl FromStr for OpId {
    type Err = InvalidOpId;

    fn from_str(text: &str) -> Result<Self, Self::Err> {
        let valid =
            text.len() == LEN && text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'));
        if valid {
            Ok(Self(text.to_owned()))
        } else {
            Err(InvalidOpId {
                text: text.to_owned(),
            })
        }
    }
}

impl TryFrom<String> for OpId {
    type Error = InvalidOpId;

    fn try_from(text: String) -> Result<Self, Self::Error> {
        text.parse()
    }
}

impl From<OpId> for String {
    fn from(id: OpId) -> Self {
        id.0
    }
}

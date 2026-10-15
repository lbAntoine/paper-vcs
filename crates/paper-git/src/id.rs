//! Object identifiers, independent of the git library underneath

use std::fmt;
use std::str::FromStr;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("'{0}' is not an object hash (expected 40 or 64 hex characters)")]
pub struct ParseIdError(String);

/// Defines a hex object-id newtype: lowercase, 40 (SHA-1) or 64 (SHA-256) characters,
/// serialised as a plain string.
macro_rules! hex_id {
    ($(#[$doc:meta])* $name:ident) => {
        $(#[$doc])*
        #[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn as_str(&self) -> &str {
                &self.0
            }

            pub(crate) fn to_gix(&self) -> gix::ObjectId {
                gix::ObjectId::from_hex(self.0.as_bytes()).expect("validated on construction")
            }

            pub(crate) fn from_gix(id: impl Into<gix::ObjectId>) -> Self {
                Self(id.into().to_hex().to_string())
            }
        }

        impl FromStr for $name {
            type Err = ParseIdError;

            fn from_str(s: &str) -> Result<Self, Self::Err> {
                parse_hex(s).map(Self)
            }
        }

        impl TryFrom<String> for $name {
            type Error = ParseIdError;

            fn try_from(s: String) -> Result<Self, Self::Error> {
                s.parse()
            }
        }

        impl From<$name> for String {
            fn from(id: $name) -> Self {
                id.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

hex_id!(
    /// A git commit hash
    CommitId
);
hex_id!(
    /// A git tree hash (a directory snapshot)
    TreeId
);
hex_id!(
    /// A git blob hash (a file's content)
    BlobId
);

fn parse_hex(s: &str) -> Result<String, ParseIdError> {
    let is_hash = matches!(s.len(), 40 | 64) && s.bytes().all(|b| b.is_ascii_hexdigit());
    if is_hash {
        Ok(s.to_ascii_lowercase())
    } else {
        Err(ParseIdError(s.to_owned()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_sha1_and_sha256_lengths() {
        assert!("a".repeat(40).parse::<CommitId>().is_ok());
        assert!("a".repeat(64).parse::<CommitId>().is_ok());
    }

    #[test]
    fn normalises_to_lowercase() {
        let id: CommitId = "ABCDEF0123".repeat(4).parse().unwrap();
        assert_eq!(id.as_str(), "abcdef0123".repeat(4));
    }

    #[test]
    fn rejects_wrong_length_or_non_hex() {
        for bad in [
            "a".repeat(39),
            "a".repeat(41),
            "g".repeat(40),
            String::new(),
        ] {
            assert!(
                bad.parse::<CommitId>().is_err(),
                "{bad:?} should be rejected"
            );
        }
    }

    #[test]
    fn serialises_as_a_plain_string() {
        let id: CommitId = "a".repeat(40).parse().unwrap();
        let json = serde_json::to_string(&id).unwrap();
        assert_eq!(json, format!("\"{}\"", "a".repeat(40)));
        assert_eq!(serde_json::from_str::<CommitId>(&json).unwrap(), id);
    }

    #[test]
    fn deserialising_validates() {
        assert!(serde_json::from_str::<CommitId>("\"not-a-hash\"").is_err());
    }

    #[test]
    fn converts_to_and_from_gix() {
        let id: TreeId = "4b825dc642cb6eb9a060e54bf8d69288fbee4904".parse().unwrap();
        assert_eq!(TreeId::from_gix(id.to_gix()), id);
    }
}

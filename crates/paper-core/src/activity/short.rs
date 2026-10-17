//! Short operation ids for listings.

use super::OpId;

/// Fewest characters a short id has.
pub const MIN_SHORT_ID: usize = 4;

/// The shortest prefix of each id that no other id in `ids` starts with, at least
/// [`MIN_SHORT_ID`] characters long. The result is in the order of `ids`.
///
/// Ids are random, so two can share a long prefix; the prefix then grows until the ids
/// differ. Distinct ids always differ within their 32 characters.
///
/// # Panics
///
/// Never.
#[must_use]
pub fn short_ids(_ids: &[OpId]) -> Vec<String> {
    todo!("PV-021: shortest unique prefixes")
}

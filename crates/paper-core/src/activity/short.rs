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
pub fn short_ids(ids: &[OpId]) -> Vec<String> {
    ids.iter()
        .enumerate()
        .map(|(index, id)| {
            let text = id.as_str();
            // The longest prefix shared with any other id; one more character tells them apart.
            let shared = ids
                .iter()
                .enumerate()
                .filter(|(other, _)| *other != index)
                .map(|(_, other)| common_prefix(text, other.as_str()))
                .max()
                .unwrap_or(0);
            let len = (shared + 1).max(MIN_SHORT_ID).min(text.len());
            text[..len].to_owned()
        })
        .collect()
}

/// Ids are ASCII hex, so counting bytes counts characters.
fn common_prefix(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

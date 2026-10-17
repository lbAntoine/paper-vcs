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
    let texts: Vec<&str> = ids.iter().map(OpId::as_str).collect();
    short_prefixes(&texts, &[])
}

/// Like [`short_ids`], but no other id in `others` (as file-name text, never parsed) starts
/// with the prefix either. `others` may repeat or include `ids`.
///
/// # Panics
///
/// Never.
#[must_use]
pub fn short_ids_among(ids: &[OpId], others: &[String]) -> Vec<String> {
    let texts: Vec<&str> = ids.iter().map(OpId::as_str).collect();
    let others: Vec<&str> = others.iter().map(String::as_str).collect();
    short_prefixes(&texts, &others)
}

/// Sort everything once; an id's longest shared prefix is with one of its two neighbours.
fn short_prefixes(ids: &[&str], others: &[&str]) -> Vec<String> {
    let mut all: Vec<&str> = ids.iter().chain(others).copied().collect();
    all.sort_unstable();
    all.dedup();
    ids.iter()
        .map(|id| {
            let at = all.binary_search(id).unwrap_or_default();
            let before = at.checked_sub(1).map_or(0, |i| common_prefix(id, all[i]));
            let after = all.get(at + 1).map_or(0, |next| common_prefix(id, next));
            let len = (before.max(after) + 1).max(MIN_SHORT_ID).min(id.len());
            id[..len].to_owned()
        })
        .collect()
}

/// Ids are ASCII hex, so counting bytes counts characters.
fn common_prefix(a: &str, b: &str) -> usize {
    a.bytes().zip(b.bytes()).take_while(|(x, y)| x == y).count()
}

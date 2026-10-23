//! Alignment of two token sequences: which tokens changed.

use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::time::{Duration, Instant};

use similar::{Algorithm, DiffTag, capture_diff_slices_deadline};

use super::{Token, is_break_token, keys};

/// Longest time one call may spend aligning. Past it `similar` returns a
/// coarser but valid diff; no test that asserts a result gets near it.
const DEADLINE: Duration = Duration::from_secs(2);

/// Which tokens are not part of the common text.
pub(super) struct Marks {
    /// Per old token: removed.
    pub(super) del: Vec<bool>,
    /// Per new token: added.
    pub(super) ins: Vec<bool>,
}

/// A maximal stretch of changed tokens, as ranges of old and new indices.
#[derive(Clone)]
pub(super) struct Run {
    pub(super) old: Range<usize>,
    pub(super) new: Range<usize>,
}

/// The changed stretches of a pair of mark vectors. The unmarked elements of
/// the two sides pair up in order.
pub(super) fn runs(del: &[bool], ins: &[bool]) -> Vec<Run> {
    let (mut i, mut j) = (0, 0);
    let mut out = Vec::new();
    while i < del.len() || j < ins.len() {
        if i < del.len() && del[i] || j < ins.len() && ins[j] {
            let (os, ns) = (i, j);
            while i < del.len() && del[i] {
                i += 1;
            }
            while j < ins.len() && ins[j] {
                j += 1;
            }
            out.push(Run {
                old: os..i,
                new: ns..j,
            });
        } else {
            i += 1;
            j += 1;
        }
    }
    out
}

/// Patience diff of two key sequences. Returns which elements changed.
///
/// A common prefix and suffix are matched first. Elements whose key occurs on
/// one side only are changed whatever the alignment, so they are taken out
/// before the diff: an unrelated rewrite costs almost nothing.
fn diff_keys(old: &[u32], new: &[u32], deadline: Instant) -> (Vec<bool>, Vec<bool>) {
    let mut del = vec![true; old.len()];
    let mut ins = vec![true; new.len()];
    let prefix = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let max_suffix = old.len().min(new.len()) - prefix;
    let suffix = old
        .iter()
        .rev()
        .zip(new.iter().rev())
        .take(max_suffix)
        .take_while(|(a, b)| a == b)
        .count();
    del[..prefix].fill(false);
    ins[..prefix].fill(false);
    del[old.len() - suffix..].fill(false);
    ins[new.len() - suffix..].fill(false);

    let old_mid = prefix..old.len() - suffix;
    let new_mid = prefix..new.len() - suffix;
    let old_set: HashSet<u32> = old[old_mid.clone()].iter().copied().collect();
    let new_set: HashSet<u32> = new[new_mid.clone()].iter().copied().collect();
    let old_at: Vec<usize> = old_mid.filter(|&i| new_set.contains(&old[i])).collect();
    let new_at: Vec<usize> = new_mid.filter(|&j| old_set.contains(&new[j])).collect();
    let old_common: Vec<u32> = old_at.iter().map(|&i| old[i]).collect();
    let new_common: Vec<u32> = new_at.iter().map(|&j| new[j]).collect();

    let ops = capture_diff_slices_deadline(
        Algorithm::Patience,
        &old_common,
        &new_common,
        Some(deadline),
    );
    for op in ops {
        let (tag, o, n) = op.as_tag_tuple();
        if tag == DiffTag::Equal {
            for (a, b) in o.zip(n) {
                del[old_at[a]] = false;
                ins[new_at[b]] = false;
            }
        }
    }
    (del, ins)
}

/// Diff two token sequences: patience over the keys, then the slide.
pub(super) fn align(old: &[Token<'_>], new: &[Token<'_>]) -> Marks {
    let deadline = Instant::now() + DEADLINE;
    let mut ids = HashMap::new();
    let old_keys = keys(old, &mut ids);
    let new_keys = keys(new, &mut ids);
    let (del, ins) = diff_keys(&old_keys, &new_keys, deadline);
    let mut marks = Marks { del, ins };
    slide(&old_keys, &new_keys, old, new, &mut marks);
    marks
}

/// Move every pure insertion or deletion to the best of the places where it
/// reads the same. A hunk can move while the token it would pass has the same
/// key as the token it would leave. The best place ends with a paragraph
/// break, else starts right after one, else it stays where the diff put it.
/// So a deleted paragraph that starts like the next one is still shown whole.
fn slide(
    old_keys: &[u32],
    new_keys: &[u32],
    old: &[Token<'_>],
    new: &[Token<'_>],
    marks: &mut Marks,
) {
    let old_brk: Vec<bool> = old.iter().map(is_break_token).collect();
    let new_brk: Vec<bool> = new.iter().map(is_break_token).collect();
    for run in runs(&marks.del, &marks.ins) {
        if run.old.is_empty() {
            slide_hunk(new_keys, &new_brk, &mut marks.ins, &run.new);
        } else if run.new.is_empty() {
            slide_hunk(old_keys, &old_brk, &mut marks.del, &run.old);
        }
    }
}

fn slide_hunk(keys: &[u32], brk: &[bool], marks: &mut [bool], hunk: &Range<usize>) {
    let (s, e) = (hunk.start, hunk.end);
    let mut left = 0;
    while s > left && !marks[s - left - 1] && keys[s - left - 1] == keys[e - left - 1] {
        left += 1;
    }
    let mut right = 0;
    while e + right < keys.len() && !marks[e + right] && keys[s + right] == keys[e + right] {
        right += 1;
    }
    if left + right == 0 {
        return;
    }
    // Candidate starts, nearest to where the hunk is first.
    let starts: Vec<usize> = (0..=left.max(right))
        .flat_map(|d| [(d <= left).then(|| s - d), (d <= right).then_some(s + d)])
        .flatten()
        .collect();
    let len = e - s;
    let pick = starts
        .iter()
        .find(|&&at| brk[at + len - 1])
        .or_else(|| starts.iter().find(|&&at| at > 0 && brk[at - 1]))
        .copied()
        .unwrap_or(s);
    if pick != s {
        marks[s..e].fill(false);
        marks[pick..pick + len].fill(true);
    }
}

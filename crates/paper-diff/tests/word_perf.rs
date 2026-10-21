//! Timing checks of the word diff (PV-041). Run in release mode:
//! `cargo test --release -p paper-diff -- --ignored`

mod common;

use std::time::{Duration, Instant};

use common::*;
use paper_diff::diff_words;

/// About 1 MB of paragraphs of 120 distinct words each.
fn big_document(replace_every: Option<usize>) -> String {
    let mut out = String::new();
    let mut p = 0;
    while out.len() < 1_000_000 {
        let words: Vec<String> = (1..=120)
            .map(|i| match replace_every {
                Some(n) if i % n == 0 => format!("new{p}x{i}"),
                _ => format!("p{p}w{i}"),
            })
            .collect();
        out.push_str(&words.join(" "));
        out.push_str("\n\n");
        p += 1;
    }
    out
}

#[test]
#[ignore = "timing check, run with: cargo test --release -p paper-diff -- --ignored"]
fn a_one_megabyte_file_with_scattered_edits_diffs_in_well_under_a_second() {
    let paragraph = format!("{}\n\n", run("word", 1, 120));
    let old = paragraph.repeat(1_000_000 / paragraph.len());
    let new = old.replace("word60 ", "changed ");
    let start = Instant::now();
    let d = diff_words(&old, &new, GEN);
    assert!(start.elapsed() < Duration::from_secs(1));
    assert!(!d.paragraphs.is_empty());
}

#[test]
#[ignore = "timing check, run with: cargo test --release -p paper-diff -- --ignored"]
fn a_one_megabyte_file_with_a_fifth_of_its_words_replaced_diffs_in_under_a_second() {
    let old = big_document(None);
    let new = big_document(Some(5));
    let start = Instant::now();
    let d = diff_words(&old, &new, GEN);
    let took = start.elapsed();
    eprintln!("20% replaced: {took:?}");
    assert!(took < Duration::from_secs(1));
    assert!(d.added > 0 && d.removed > 0);
}

#[test]
#[ignore = "timing check, run with: cargo test --release -p paper-diff -- --ignored"]
fn an_unrelated_rewrite_of_one_paragraph_diffs_in_under_a_second() {
    let mut old = big_document(None);
    let mut new = old.clone();
    let rewrite_old: String = run("old", 0, 2999);
    let rewrite_new: String = run("fresh", 0, 2999);
    old.push_str(&rewrite_old);
    new.push_str(&rewrite_new);
    let start = Instant::now();
    let d = diff_words(&old, &new, GEN);
    let took = start.elapsed();
    eprintln!("one paragraph rewritten: {took:?}");
    assert!(took < Duration::from_secs(1));
    assert_eq!((d.added, d.removed), (3000, 3000));
}

/// Words drawn from a Zipf-like distribution over `vocabulary` words.
struct Words {
    state: u64,
    vocabulary: f64,
}

impl Words {
    fn next(&mut self) -> String {
        self.state = self
            .state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss
        )]
        let u = (self.state >> 11) as f64 / (1u64 << 53) as f64;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rank = self.vocabulary.powf(u) as usize;
        format!("v{rank}")
    }
}

/// Text of about `bytes` bytes in paragraphs of `per_paragraph` words (0 for
/// one paragraph), and the same text with every `every`-th word replaced.
fn zipf_pair(
    bytes: usize,
    vocabulary: f64,
    per_paragraph: usize,
    every: usize,
) -> (String, String) {
    let mut words = Words {
        state: 7,
        vocabulary,
    };
    let (mut old, mut new) = (String::new(), String::new());
    let mut n = 0;
    while old.len() < bytes {
        let w = words.next();
        let replaced = if n % every == every - 1 {
            format!("{w}x")
        } else {
            w.clone()
        };
        let sep = if per_paragraph > 0 && n % per_paragraph == per_paragraph - 1 {
            "\n\n"
        } else {
            " "
        };
        old.push_str(&w);
        old.push_str(sep);
        new.push_str(&replaced);
        new.push_str(sep);
        n += 1;
    }
    (old, new)
}

#[test]
#[ignore = "timing check, run with: cargo test --release -p paper-diff -- --ignored"]
fn a_one_megabyte_zipf_text_with_scattered_edits_diffs_in_under_a_second() {
    for vocabulary in [2000.0, 300.0] {
        let (old, new) = zipf_pair(1_000_000, vocabulary, 120, 100);
        let start = Instant::now();
        let d = diff_words(&old, &new, GEN);
        let took = start.elapsed();
        eprintln!("zipf {vocabulary} words, paragraphs: {took:?}");
        assert!(took < Duration::from_secs(1), "{vocabulary}: {took:?}");
        assert!(d.added > 0);
    }
}

/// Recorded, not asserted: one huge paragraph with a tiny vocabulary may run
/// into the two second deadline and come out coarse.
#[test]
#[ignore = "timing record, run with: cargo test --release -p paper-diff -- --ignored --nocapture"]
fn a_one_megabyte_single_paragraph_with_a_tiny_vocabulary_is_timed() {
    let (old, new) = zipf_pair(1_000_000, 50.0, 0, 5);
    let start = Instant::now();
    let d = diff_words(&old, &new, GEN);
    eprintln!(
        "single paragraph, 50 words: {:?} (+{} -{})",
        start.elapsed(),
        d.added,
        d.removed
    );
}

/// Recorded, not asserted: a fifth of the words of a repeated-vocabulary text
/// replaced is a very large edit, and may reach the two second deadline.
#[test]
#[ignore = "timing record, run with: cargo test --release -p paper-diff -- --ignored --nocapture"]
fn a_one_megabyte_zipf_text_with_a_fifth_replaced_is_timed() {
    let (old, new) = zipf_pair(1_000_000, 2000.0, 120, 5);
    let start = Instant::now();
    let d = diff_words(&old, &new, GEN);
    eprintln!(
        "zipf 2000 words, a fifth replaced: {:?} (+{} -{})",
        start.elapsed(),
        d.added,
        d.removed
    );
}

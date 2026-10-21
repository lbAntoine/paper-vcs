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

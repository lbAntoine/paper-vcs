//! Cross-check `to_unified` against `git diff --no-index -U3`.

use std::fs;
use std::process::Command;

use paper_diff::{FileDiff, LineDiffOptions, diff_files, to_unified};

/// Run git on two temp files and return the output from the first `@@` line on.
fn git_hunks(old: Option<&str>, new: Option<&str>) -> String {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.txt");
    let new_path = dir.path().join("new.txt");
    let (old_arg, new_arg) = (
        old.map_or("/dev/null".to_string(), |c| {
            fs::write(&old_path, c).unwrap();
            old_path.to_string_lossy().into_owned()
        }),
        new.map_or("/dev/null".to_string(), |c| {
            fs::write(&new_path, c).unwrap();
            new_path.to_string_lossy().into_owned()
        }),
    );
    let out = Command::new("git")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("HOME", dir.path())
        .args(["diff", "--no-index", "-U3", "--no-color", "--"])
        .arg(&old_arg)
        .arg(&new_arg)
        .output()
        .expect("git runs");
    // Exit code 1 means "files differ", which is the normal case here.
    assert!(
        matches!(out.status.code(), Some(0 | 1)),
        "git failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let text = String::from_utf8(out.stdout).unwrap();
    let start = text.find("@@").expect("git printed a hunk");
    normalise_headings(&text[start..])
}

/// Drop git's function-context heading from `@@ ... @@ <heading>` lines.
fn normalise_headings(text: &str) -> String {
    text.split_inclusive('\n')
        .map(|line| {
            if let Some(rest) = line.strip_prefix("@@ ") {
                let end = rest.find(" @@").expect("hunk header") + 3 + 3;
                format!("{}\n", &line[..end])
            } else {
                line.to_string()
            }
        })
        .collect()
}

fn ours(old: Option<&str>, new: Option<&str>) -> String {
    let FileDiff::Text(d) = diff_files(
        old.map(str::as_bytes),
        new.map(str::as_bytes),
        &LineDiffOptions::default(),
    ) else {
        panic!("expected a text diff");
    };
    let out = to_unified(&d, "a/old.txt", "b/new.txt");
    let start = out.find("@@").expect("we printed a hunk");
    out[start..].to_string()
}

fn numbered(n: usize) -> String {
    let mut s = String::new();
    for i in 1..=n {
        s.push_str("line ");
        s.push_str(&i.to_string());
        s.push('\n');
    }
    s
}

fn assert_matches_git(old: Option<&str>, new: Option<&str>) {
    assert_eq!(ours(old, new), git_hunks(old, new));
}

#[test]
fn a_single_line_edit_matches_git() {
    let old = numbered(20);
    let new = old.replace("line 10\n", "changed\n");
    assert_matches_git(Some(&old), Some(&new));
}

#[test]
fn an_insertion_matches_git() {
    let old = numbered(12);
    let new = old.replace("line 6\n", "line 6\nextra\n");
    assert_matches_git(Some(&old), Some(&new));
}

#[test]
fn a_deletion_matches_git() {
    let old = numbered(12);
    let new = old.replace("line 6\n", "");
    assert_matches_git(Some(&old), Some(&new));
}

#[test]
fn an_edit_at_the_start_matches_git() {
    let old = numbered(10);
    let new = old.replace("line 1\n", "first\n");
    assert_matches_git(Some(&old), Some(&new));
}

#[test]
fn an_added_file_matches_git() {
    assert_matches_git(None, Some("a\nb\nc\n"));
}

#[test]
fn a_deleted_file_matches_git() {
    assert_matches_git(Some("a\nb\nc\n"), None);
}

#[test]
fn a_missing_final_newline_matches_git() {
    assert_matches_git(Some("a\nb\n"), Some("a\nb"));
    assert_matches_git(Some("a\nb"), Some("a\nb\n"));
}

#[test]
fn a_cr_in_the_middle_of_a_line_matches_git() {
    let old = "a\rb\nc\nd\n";
    let new = "a\rx\nc\nd\n";
    assert_matches_git(Some(old), Some(new));
}

#[test]
fn crlf_files_match_git() {
    let old = "a\r\nb\r\nc\r\n";
    let new = "a\r\nB\r\nc\r\n";
    assert_matches_git(Some(old), Some(new));
}

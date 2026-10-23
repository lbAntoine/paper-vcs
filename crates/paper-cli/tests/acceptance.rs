//! The v0.1 acceptance scenario (roadmap, v0.1): a researcher's whole first session with the
//! real `paper` binary, then plain git proves nothing proprietary was written (I5, ADR-0002).

mod common;

use common::Env;

const MAIN_TEX: &str =
    "\\documentclass{article}\n\\begin{document}\n\\input{sections/intro}\n\\end{document}\n";
const REFS_BIB: &str = "@book{knuth, title={TeX}, author={Knuth}}\n";

fn intro_after(round: usize) -> String {
    format!("Introduction, draft {round}.\nThis sentence is on its own line.\n")
}

fn git_init_bare(env: &Env, name: &str) -> std::path::PathBuf {
    let bare = env.root.join(name);
    std::fs::create_dir_all(&bare).unwrap();
    env.git(&bare, &["init", "--quiet", "--bare", "-b", "main"]);
    bare
}

/// A history line ends with its subject, optionally followed by the `(main)` marker.
fn ends_with_subject(line: &str, subject: &str) -> bool {
    let line = line.trim_end();
    line.strip_suffix("(main)")
        .map_or(line, str::trim_end)
        .ends_with(&format!("  {subject}"))
}

// One narrative scenario, on purpose: splitting it would hide the order of events.
#[allow(clippy::too_many_lines)]
#[test]
fn a_researcher_can_init_save_restore_and_push_with_plain_git() {
    let env = Env::new();

    // init: latex detected, junk not saved.
    env.write("main.tex", MAIN_TEX);
    env.write("sections/intro.tex", &intro_after(0));
    env.write("refs.bib", REFS_BIB);
    env.write("main.aux", "\\relax\n");
    env.write("main.log", "This is pdfTeX\n");
    let init = env.paper(&["--json", "init"]);
    init.assert_code(0);
    let json = init.json();
    assert_eq!(json["profile"], "latex", "{init:#?}");
    assert_eq!(json["detected"], true, "{init:#?}");
    let tracked = env.git_work(&["ls-tree", "-r", "--name-only", "HEAD"]);
    let tracked: Vec<&str> = tracked.lines().collect();
    for wanted in ["main.tex", "sections/intro.tex", "refs.bib"] {
        assert!(tracked.contains(&wanted), "{wanted} missing: {tracked:?}");
    }
    for junk in ["main.aux", "main.log"] {
        assert!(!tracked.contains(&junk), "{junk} was saved: {tracked:?}");
    }
    assert_eq!(env.subjects(), ["Start project"]);

    // Ten rounds of editing and saving.
    let mut messages = vec!["Start project".to_owned()];
    for round in 1..=10 {
        env.write("sections/intro.tex", &intro_after(round));
        if round % 3 == 0 {
            env.write("main.tex", &format!("{MAIN_TEX}% round {round}\n"));
        }
        let message = format!("Round {round}");
        env.paper(&["save", &message]).assert_code(0);
        messages.push(message);
    }

    // history: 11 saves, newest first; -n 3 shows three.
    let history = env.paper(&["history"]);
    history.assert_code(0);
    let lines: Vec<&str> = history.stdout.lines().collect();
    assert_eq!(lines.len(), 11, "{history:#?}");
    for (line, message) in lines.iter().zip(messages.iter().rev()) {
        assert!(ends_with_subject(line, message), "{line} vs {message}");
    }
    let short = env.paper(&["history", "-n", "3"]);
    short.assert_code(0);
    let lines: Vec<&str> = short.stdout.lines().collect();
    assert_eq!(lines.len(), 3, "{short:#?}");
    assert!(ends_with_subject(lines[0], "Round 10"), "{short:#?}");
    assert!(ends_with_subject(lines[2], "Round 8"), "{short:#?}");

    // restore from last~3 (the content saved in round 7), byte for byte.
    env.paper(&["restore", "sections/intro.tex", "--from", "last~3"])
        .assert_code(0);
    let path = env.work.join("sections").join("intro.tex");
    assert_eq!(
        std::fs::read(&path).unwrap(),
        intro_after(7).into_bytes(),
        "restored bytes differ"
    );
    let status = env.paper(&["status"]);
    status.assert_code(0);
    let intro_line = status
        .stdout
        .lines()
        .find(|line| line.contains("sections/intro.tex"))
        .unwrap_or_else(|| panic!("intro not in status: {status:#?}"));
    assert!(intro_line.contains("modified"), "{status:#?}");
    let diff = env.paper(&["diff", "--lines"]);
    diff.assert_code(0);
    assert!(diff.stdout.contains("sections/intro.tex"), "{diff:#?}");
    assert!(
        diff.stdout.contains("-Introduction, draft 10."),
        "{diff:#?}"
    );
    assert!(diff.stdout.contains("+Introduction, draft 7."), "{diff:#?}");
    env.paper(&["save", "Restore intro"]).assert_code(0);
    assert_eq!(env.subjects().len(), 12);
    assert_eq!(env.subjects()[0], "Restore intro");
    let porcelain = env.git_work(&["status", "--porcelain"]);
    assert!(porcelain.trim().is_empty(), "git sees changes: {porcelain}");

    // Plain git: push to a bare repo, clone it, check refs and integrity.
    env.fsck();
    let bare = git_init_bare(&env, "origin.git");
    let bare_str = bare.to_string_lossy().into_owned();
    env.git_work(&["remote", "add", "origin", &bare_str]);
    let local_refs = env.git_work(&["for-each-ref", "refs/paper"]);
    assert!(
        !local_refs.trim().is_empty(),
        "paper keeps its refs locally, so the remote check below can fail"
    );
    env.git_work(&["push", "-u", "origin", "main"]);

    let clone_dir = env.root.join("clone");
    let clone_str = clone_dir.to_string_lossy().into_owned();
    env.git(&env.root, &["clone", "--quiet", &bare_str, &clone_str]);
    for file in ["main.tex", "refs.bib"] {
        assert!(clone_dir.join(file).is_file(), "{file} missing in clone");
    }
    assert_eq!(
        std::fs::read(clone_dir.join("sections").join("intro.tex")).unwrap(),
        intro_after(7).into_bytes()
    );
    let log = env.git(&clone_dir, &["log", "--format=%s"]);
    assert_eq!(log.lines().count(), 12, "{log}");
    assert_eq!(log.lines().next(), Some("Restore intro"));
    assert_eq!(log.lines().last(), Some("Start project"));

    let remote_refs = env.git(&env.root, &["ls-remote", &bare_str]);
    assert!(remote_refs.contains("refs/heads/main"), "{remote_refs}");
    assert!(!remote_refs.contains("refs/paper"), "{remote_refs}");
    env.git(&clone_dir, &["fsck", "--strict", "--no-progress"]);
    env.git(&bare, &["fsck", "--strict", "--no-progress"]);
    env.fsck();
}

const PI_BEFORE: &str = "\
The first paragraph states the research question and the motivation behind the whole study.

Our method relies on careful measurements of the sample under three different conditions.

Results are summarised in the next section.

This paragraph will be deleted by the reviewer entirely.

Closing remarks end the section.
";

/// The co-author re-wraps paragraph 1, swaps two words in paragraph 2, adds a sentence to
/// paragraph 3 and deletes paragraph 4.
const PI_AFTER: &str = "\
The first paragraph states the research
question and the motivation  behind the
whole study.

Our method relies on precise measurements of the sample under four different conditions.

Results are summarised in the next section. They are stable across runs.

Closing remarks end the section.
";

const MINUS: &str = "\u{2212}";

/// Readability of the paragraph 3 / paragraph 4 change, read from the JSON segments: the
/// deleted paragraph and the added sentence are each one contiguous run, nothing is
/// interleaved word by word, and the removed paragraph break is shown. Where the shared
/// final period sits, and which paragraph number carries the change, are not checked.
fn assert_deleted_paragraph_and_added_sentence_read_as_prose(env: &Env, since: &str) {
    let run = env.paper(&["--json", "diff", "--since", since]);
    run.assert_code(0);
    let json = run.json();
    let paragraphs = json["files"][0]["paragraphs"].as_array().unwrap();
    let is_break = |s: &serde_json::Value| {
        s["text"]
            .as_str()
            .is_some_and(|t| t.matches('\n').count() >= 2 && t.trim().is_empty())
    };
    let text_of = |s: &serde_json::Value| s["text"].as_str().unwrap_or("").to_owned();
    let segments_of = |p: &serde_json::Value| p["segments"].as_array().unwrap().clone();

    // (a) the deleted paragraph is one contiguous removed run.
    let deleted = "This paragraph will be deleted by the reviewer entirely";
    let in_paragraph = paragraphs
        .iter()
        .find(|p| {
            segments_of(p)
                .iter()
                .any(|s| s["kind"] == "removed" && text_of(s).contains(deleted))
        })
        .unwrap_or_else(|| {
            panic!("(a) no single removed run holds the deleted paragraph: {run:#?}")
        });
    // (b) the new sentence is one contiguous added run.
    let sentence = "They are stable across runs";
    let added_in = paragraphs
        .iter()
        .find(|p| {
            segments_of(p)
                .iter()
                .any(|s| s["kind"] == "added" && text_of(s).contains(sentence))
        })
        .unwrap_or_else(|| panic!("(b) no single added run holds the new sentence: {run:#?}"));
    // (c) in that stretch: at most one removed and one added run (the break aside).
    let stretch = segments_of(in_paragraph);
    let removed_runs = stretch
        .iter()
        .filter(|s| s["kind"] == "removed" && !is_break(s))
        .count();
    let added_runs = segments_of(added_in)
        .iter()
        .filter(|s| s["kind"] == "added" && !is_break(s))
        .count();
    assert_eq!(removed_runs, 1, "(c) removed text is interleaved: {run:#?}");
    assert_eq!(added_runs, 1, "(c) added text is interleaved: {run:#?}");
    // (d) the removed paragraph break is shown.
    assert!(
        paragraphs
            .iter()
            .flat_map(segments_of)
            .any(|s| s["kind"] == "removed" && is_break(&s)),
        "(d) no removed paragraph break: {run:#?}"
    );
    // The same, in the plain markup with the wrap undone.
    let plain = env.paper(&["diff", "--since", since]);
    let joined = plain
        .stdout
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ");
    assert!(joined.contains("[-\u{b6}-]"), "(d) {plain:#?}");
    assert!(joined.contains(deleted), "(a) {plain:#?}");
    assert!(joined.contains(sentence), "(b) {plain:#?}");
}

// Roadmap v0.3 acceptance: a PI runs `paper diff --since <point>` on a co-author's changes
// and reads it as prose.
#[test]
fn a_pi_reads_a_co_authors_changes_as_prose() {
    let env = Env::new();
    env.write("main.tex", MAIN_TEX);
    env.write("sections/intro.tex", PI_BEFORE);
    env.paper(&["init"]).assert_code(0);
    env.paper(&["save", "Draft for the PI"]).assert_code(0);
    let id = env.change_id("HEAD");

    env.write("sections/intro.tex", PI_AFTER);
    let run = env.paper(&["diff", "--since", &id[..4]]);

    run.assert_code(0);
    assert_eq!(run.stderr, "", "{run:#?}");
    assert!(!run.stdout.contains('\x1b'), "no colour: {run:#?}");
    // The header counts only the real edits: 2 + 5 words added, 2 + 9 removed.
    assert!(
        run.stdout
            .contains(&format!("sections/intro.tex  +7 words {MINUS}11 words\n")),
        "{run:#?}"
    );
    // Word markup with paragraph numbers on the new side.
    assert!(run.stdout.contains("[-careful-]{+precise+}"), "{run:#?}");
    assert!(run.stdout.contains("[-three-]{+four+}"), "{run:#?}");
    assert!(run.stdout.contains("  \u{b6} 2\n"), "{run:#?}");
    assert_deleted_paragraph_and_added_sentence_read_as_prose(&env, &id[..4]);
    // The re-wrapped paragraph is not a change: it does not appear at all.
    assert!(!run.stdout.contains("  \u{b6} 1\n"), "{run:#?}");
    assert!(!run.stdout.contains("research question"), "{run:#?}");
    assert!(!run.stdout.contains("motivation"), "{run:#?}");
    // Only this one file changed, and it is prose, not a patch.
    assert!(!run.stdout.contains("@@"), "{run:#?}");
    assert_eq!(
        run.stdout
            .lines()
            .filter(|l| !l.starts_with(' ') && !l.is_empty())
            .count(),
        1,
        "one file header: {run:#?}"
    );

    let stat = env.paper(&["diff", "--stat", "--since", &id[..4]]);
    stat.assert_code(0);
    assert!(
        stat.stdout
            .contains(&format!("1 file changed, +7 words {MINUS}11 words")),
        "{stat:#?}"
    );
    env.fsck();
}

#[test]
fn a_pure_re_wrap_diffs_to_spacing_only() {
    let env = Env::new();
    env.write("main.tex", MAIN_TEX);
    env.write("sections/intro.tex", PI_BEFORE);
    env.paper(&["init"]).assert_code(0);
    env.paper(&["save", "Draft for the PI"]).assert_code(0);
    env.write(
        "sections/intro.tex",
        &PI_BEFORE.replace("states the research", "states\nthe  research"),
    );

    let diff = env.paper(&["diff", "--since", "last"]);
    let status = env.paper(&["status"]);

    diff.assert_code(0);
    assert_eq!(
        diff.stdout, "sections/intro.tex  spacing and line breaks only\n",
        "{diff:#?}"
    );
    assert!(status.stdout.contains("spacing only"), "{status:#?}");
    env.fsck();
}

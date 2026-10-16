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
    let diff = env.paper(&["diff"]);
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

//! `paper config get|set|list`, and the settings taking effect (identity, colour, limits).
//! Every run is isolated: the user config lives under the temp dir (`XDG_CONFIG_HOME` /
//! `APPDATA`), never in the developer's home.

mod common;

use common::{Env, assert_keys, binary, settings};

const TWO_MB: usize = 2 * 1024 * 1024;

fn project() -> Env {
    let env = Env::new();
    env.project_with_three_saves();
    env
}

fn set(env: &Env, args: &[&str]) {
    let mut full = vec!["config", "set"];
    full.extend_from_slice(args);
    env.paper(&full).assert_code(0);
}

/// `%an/%ae` of the newest commit.
fn author(env: &Env) -> String {
    env.git_work(&["log", "-1", "--format=%an/%ae"])
        .trim()
        .to_owned()
}

fn edit_and_save(env: &Env, extra_env: &[(&str, &str)]) {
    env.write("sections/intro.tex", "Intro v3\n");
    env.paper_in(&env.work, extra_env, &["save", "Tighten intro"])
        .assert_code(0);
}

// ---- identity ----

#[test]
fn set_user_name_and_email_write_the_user_config_and_the_next_save_uses_them() {
    let env = project();

    let name = env.paper(&["config", "set", "user.name", "Alex Doe"]);
    name.assert_code(0);
    set(&env, &["user.email", "alex@lab.example"]);

    assert!(env.user_config().is_file(), "{:?}", env.user_config());
    let text = std::fs::read_to_string(env.user_config()).unwrap();
    assert!(
        text.contains("Alex Doe") && text.contains("alex@lab.example"),
        "{text}"
    );
    assert!(
        name.stdout.contains("Set user.name = Alex Doe in "),
        "{name:#?}"
    );
    assert!(name.stdout.contains("config.toml"), "{name:#?}");
    edit_and_save(&env, &[]);
    assert_eq!(author(&env), "Alex Doe/alex@lab.example");
    env.fsck();
}

#[test]
fn identity_comes_from_git_config_when_nothing_else_is_set() {
    let env = project();
    edit_and_save(&env, &[]);
    assert_eq!(author(&env), "Mina Park/mina@uni.example");
}

#[test]
fn user_config_beats_git_config() {
    let env = project();
    set(&env, &["user.name", "Alex Doe"]);
    set(&env, &["user.email", "alex@lab.example"]);
    edit_and_save(&env, &[]);
    assert_eq!(author(&env), "Alex Doe/alex@lab.example");
}

#[test]
fn the_environment_beats_user_config() {
    let env = project();
    set(&env, &["user.name", "Alex Doe"]);
    set(&env, &["user.email", "alex@lab.example"]);
    edit_and_save(
        &env,
        &[
            ("PAPER_USER_NAME", "Env Person"),
            ("PAPER_USER_EMAIL", "env@lab.example"),
        ],
    );
    assert_eq!(author(&env), "Env Person/env@lab.example");
}

#[test]
fn init_uses_an_identity_from_the_user_config_when_git_has_none() {
    let env = Env::without_identity();
    set(&env, &["user.name", "Alex Doe"]);
    set(&env, &["user.email", "alex@lab.example"]);
    env.write("notes.txt", "hello\n");

    let run = env.paper(&["init"]);

    run.assert_code(0);
    assert_eq!(author(&env), "Alex Doe/alex@lab.example");
}

#[test]
fn without_any_identity_and_no_terminal_the_error_suggests_paper_config_first() {
    let env = Env::without_identity();
    env.write("notes.txt", "hello\n");

    let run = env.paper(&["init"]);

    run.assert_three_part_error(1);
    let paper_config = run
        .stderr
        .find("paper config set user.name")
        .expect("paper config");
    let git_config = run
        .stderr
        .find("git config --global user.name")
        .expect("git config");
    assert!(paper_config < git_config, "{run:#?}");
    assert!(!env.work.join(".git").exists());
    assert!(!env.user_config().exists());
}

// ---- writing the files ----

#[test]
fn set_keeps_comments_and_other_keys_byte_for_byte() {
    let env = project();
    let original = "# my settings\n[custom]\nkeep = \"me\"   # untouched\n\n";
    std::fs::create_dir_all(env.user_config().parent().unwrap()).unwrap();
    std::fs::write(env.user_config(), original).unwrap();

    set(&env, &["user.name", "Alex Doe"]);

    let text = std::fs::read_to_string(env.user_config()).unwrap();
    assert!(text.starts_with(original), "{text}");
    assert!(text.contains("name = \"Alex Doe\""), "{text}");
}

#[test]
fn set_project_writes_paper_toml_in_the_project_root_and_names_it() {
    let env = project();
    let run = env.paper(&["config", "set", "--project", "large_files.max_mb", "1"]);

    run.assert_code(0);
    assert!(
        run.stdout.contains("Set large_files.max_mb = 1 in "),
        "{run:#?}"
    );
    assert!(run.stdout.contains("paper.toml"), "{run:#?}");
    let text = std::fs::read_to_string(env.paper_toml()).unwrap();
    assert!(
        text.contains("[large_files]") && text.contains("max_mb = 1"),
        "{text}"
    );
}

#[test]
fn set_project_from_a_subfolder_writes_the_root_paper_toml() {
    let env = project();
    let sub = env.work.join("sections");

    let run = env.paper_in(
        &sub,
        &[],
        &["config", "set", "--project", "profile", "latex"],
    );

    run.assert_code(0);
    assert!(env.paper_toml().is_file());
    assert!(!sub.join("paper.toml").exists());
}

#[test]
fn paper_toml_is_an_ordinary_file_in_the_draft_and_in_the_next_save() {
    let env = project();
    set(&env, &["--project", "large_files.max_mb", "1"]);

    let status = env.paper(&["status"]);
    status.assert_code(0);
    assert!(status.stdout.contains("paper.toml"), "{status:#?}");

    env.paper(&["save", "Add settings"]).assert_code(0);
    let tracked = env.git_work(&["ls-files"]);
    assert!(tracked.lines().any(|l| l == "paper.toml"), "{tracked}");
    assert_eq!(env.git_work(&["status", "--porcelain"]), "");
    env.fsck();
}

#[test]
fn a_2_mb_file_is_skipped_by_save_once_the_project_max_is_1_mb() {
    let env = project();
    set(&env, &["--project", "large_files.max_mb", "1"]);
    env.write_bytes("data/big.bin", &binary(TWO_MB, 3));

    let run = env.paper(&["save", "Add data"]);

    run.assert_code(0);
    let tracked = env.git_work(&["ls-files"]);
    assert!(!tracked.lines().any(|l| l == "data/big.bin"), "{tracked}");
    assert!(tracked.lines().any(|l| l == "paper.toml"), "{tracked}");
    env.fsck();
}

// ---- refusals ----

#[test]
fn invalid_values_and_unknown_keys_are_three_part_errors_and_write_nothing() {
    let env = project();
    set(&env, &["--project", "large_files.warn_mb", "5"]);
    let before = std::fs::read_to_string(env.paper_toml()).unwrap();
    let cases: [&[&str]; 7] = [
        &["ui.color", "purple"],
        &["large_files.max_mb", "0"],
        &["user.email", "nobody"],
        &["nonsense.key", "1"],
        &["ui.editor", "vim"],
        &["large_files.max_mb", "lots"],
        &["user.name", ""],
    ];
    for args in cases {
        let mut full = vec!["config", "set"];
        full.extend_from_slice(args);
        let run = env.paper(&full);
        run.assert_three_part_error(1);
    }
    let below_warn = env.paper(&["config", "set", "--project", "large_files.max_mb", "3"]);
    below_warn.assert_three_part_error(1);
    assert!(below_warn.stderr.contains("warn_mb"), "{below_warn:#?}");

    assert!(!env.user_config().exists(), "user config must stay absent");
    assert_eq!(std::fs::read_to_string(env.paper_toml()).unwrap(), before);
}

#[test]
fn an_unknown_key_names_the_known_keys() {
    let env = project();
    let run = env.paper(&["config", "set", "ui.editor", "vim"]);
    run.assert_three_part_error(1);
    for key in ["user.name", "large_files.max_mb", "profile"] {
        assert!(run.stderr.contains(key), "{key}: {run:#?}");
    }
}

#[test]
fn diff_default_words_is_accepted_and_says_it_has_no_effect_yet() {
    let env = project();
    set(&env, &["diff.default", "words"]);
    let run = env.paper(&["config", "get", "diff.default"]);
    run.assert_code(0);
    assert_eq!(run.stdout.trim(), "words");
    assert!(run.stderr.contains("v0.3"), "{run:#?}");
    let list = env.paper(&["config", "list"]);
    assert!(list.stderr.contains("v0.3"), "{list:#?}");
}

#[test]
fn identity_is_never_written_to_the_project_file() {
    let env = project();
    for key in ["user.name", "user.email", "ui.color"] {
        let run = env.paper(&["config", "set", "--project", key, "x@y"]);
        run.assert_three_part_error(1);
    }
    // Only the profile that `init` wrote is in there.
    assert_eq!(env.read("paper.toml"), "profile = \"latex\"\n");
}

#[test]
fn profile_needs_project() {
    let env = project();
    let before = env.read("paper.toml");
    let run = env.paper(&["config", "set", "profile", "latex"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("--project"), "{run:#?}");
    assert!(!env.user_config().exists());
    assert_eq!(env.read("paper.toml"), before);
}

#[test]
fn project_keys_outside_a_project_are_the_not_a_project_error() {
    let env = Env::new();
    let run = env.paper(&["config", "set", "--project", "large_files.max_mb", "1"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("paper init"), "{run:#?}");
    assert!(!env.paper_toml().exists());
}

#[test]
fn user_keys_work_outside_a_project() {
    let env = Env::new();
    env.paper(&["config", "set", "ui.color", "never"])
        .assert_code(0);
    let run = env.paper(&["config", "get", "ui.color"]);
    run.assert_code(0);
    assert_eq!(run.stdout.trim(), "never");
}

#[test]
fn a_malformed_paper_toml_names_the_file_and_the_line() {
    let env = project();
    std::fs::write(
        env.paper_toml(),
        "[large_files]\nmax_mb = 1\nthis is not toml\n",
    )
    .unwrap();

    for args in [&["status"][..], &["config", "list"][..]] {
        let run = env.paper(args);
        run.assert_three_part_error(1);
        assert!(run.stderr.contains("paper.toml"), "{run:#?}");
        assert!(run.stderr.contains("line 3"), "{run:#?}");
    }
}

#[test]
fn a_malformed_user_config_names_the_file_and_the_line() {
    let env = project();
    std::fs::create_dir_all(env.user_config().parent().unwrap()).unwrap();
    std::fs::write(env.user_config(), "[user]\nname = \"ok\"\nbroken line\n").unwrap();

    let run = env.paper(&["status"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("config.toml"), "{run:#?}");
    assert!(run.stderr.contains("line 3"), "{run:#?}");
}

// ---- get ----

#[test]
fn get_prints_the_effective_value() {
    let env = project();
    let defaults = [
        ("ui.color", "auto"),
        ("large_files.warn_mb", "10"),
        ("large_files.max_mb", "100"),
        ("diff.default", "lines"),
        ("user.name", "Mina Park"),
        ("user.email", "mina@uni.example"),
    ];
    for (key, expected) in defaults {
        let run = env.paper(&["config", "get", key]);
        run.assert_code(0);
        assert_eq!(run.stdout.trim(), expected, "{key}");
    }
    set(&env, &["--project", "large_files.max_mb", "7"]);
    let run = env.paper(&["config", "get", "large_files.max_mb"]);
    assert_eq!(run.stdout.trim(), "7");
    let run = env.paper_in(
        &env.work,
        &[("PAPER_LARGE_FILES_MAX_MB", "9")],
        &["config", "get", "large_files.max_mb"],
    );
    assert_eq!(run.stdout.trim(), "9");
}

#[test]
fn get_an_unset_identity_exits_1_with_a_three_part_error() {
    let env = Env::without_identity();
    let run = env.paper(&["config", "get", "user.name"]);
    run.assert_three_part_error(1);
    assert!(
        run.stderr.contains("paper config set user.name"),
        "{run:#?}"
    );
}

#[test]
fn get_an_unset_profile_exits_1() {
    let env = project();
    std::fs::remove_file(env.paper_toml()).unwrap(); // init wrote the profile there
    env.paper(&["config", "get", "profile"])
        .assert_three_part_error(1);
}

#[test]
fn get_an_unknown_key_exits_1() {
    let env = project();
    env.paper(&["config", "get", "ui.editor"])
        .assert_three_part_error(1);
}

// ---- list ----

#[test]
fn list_shows_every_key_with_its_value_and_source() {
    let env = project();
    set(&env, &["--project", "large_files.max_mb", "7"]);
    set(&env, &["ui.color", "never"]);

    let run = env.paper_in(
        &env.work,
        &[("PAPER_LARGE_FILES_WARN_MB", "2")],
        &["config", "list"],
    );

    run.assert_code(0);
    let row = |key: &str| {
        run.stdout
            .lines()
            .find(|l| l.contains(key))
            .unwrap_or_else(|| panic!("no row for {key}: {run:#?}"))
            .to_owned()
    };
    for key in [
        "user.name",
        "user.email",
        "ui.color",
        "large_files.warn_mb",
        "large_files.max_mb",
        "diff.default",
        "profile",
    ] {
        row(key);
    }
    assert!(row("user.name").contains("Mina Park") && row("user.name").contains("git"));
    assert!(row("ui.color").contains("never") && row("ui.color").contains("user"));
    assert!(
        row("large_files.max_mb").contains('7') && row("large_files.max_mb").contains("project")
    );
    assert!(row("large_files.warn_mb").contains('2') && row("large_files.warn_mb").contains("env"));
    assert!(row("diff.default").contains("lines") && row("diff.default").contains("default"));
    settings().bind(|| {
        insta::assert_snapshot!("config_list", env.redact(&run.stdout));
    });
}

#[test]
fn list_json_has_schema_1_and_a_fixed_key_set() {
    let env = project();
    set(&env, &["--project", "large_files.max_mb", "7"]);

    let run = env.paper(&["config", "list", "--json"]);

    run.assert_code(0);
    let json = run.json();
    assert_keys(&json, &["schema", "settings"]);
    assert_eq!(json["schema"], 1);
    let rows = json["settings"].as_array().unwrap();
    let keys: Vec<&str> = rows.iter().map(|r| r["key"].as_str().unwrap()).collect();
    assert_eq!(
        keys,
        [
            "user.name",
            "user.email",
            "ui.color",
            "large_files.warn_mb",
            "large_files.max_mb",
            "diff.default",
            "profile"
        ]
    );
    for row in rows {
        assert_keys(row, &["key", "value", "source"]);
    }
    assert_eq!(rows[4]["value"], 7);
    assert_eq!(rows[4]["source"], "project");
    assert_eq!(rows[0]["value"], "Mina Park");
    assert_eq!(rows[0]["source"], "git");
    assert_eq!(rows[6]["value"], "latex"); // written by init
    assert_eq!(rows[6]["source"], "project");
    settings().bind(|| {
        insta::assert_snapshot!("config_list_json", env.redact_json(&run.stdout));
    });
}

// ---- colour ----

#[test]
fn ui_color_always_colours_output_without_the_flag() {
    let env = project();
    let plain = env.paper(&["history"]);
    assert!(!plain.stdout.contains('\u{1b}'), "{plain:#?}");

    set(&env, &["ui.color", "always"]);
    let run = env.paper(&["history"]);

    run.assert_code(0);
    assert!(run.stdout.contains('\u{1b}'), "{run:#?}");
}

#[test]
fn the_color_flag_beats_ui_color() {
    let env = project();
    set(&env, &["ui.color", "always"]);
    let run = env.paper(&["--color", "never", "history"]);
    run.assert_code(0);
    assert!(!run.stdout.contains('\u{1b}'), "{run:#?}");
}

#[test]
fn the_environment_beats_ui_color_in_the_user_config() {
    let env = project();
    set(&env, &["ui.color", "always"]);
    let run = env.paper_in(&env.work, &[("PAPER_UI_COLOR", "never")], &["history"]);
    run.assert_code(0);
    assert!(!run.stdout.contains('\u{1b}'), "{run:#?}");
}

// ---- review round 1 ----

#[test]
fn a_bad_value_in_paper_toml_names_the_file_and_suggests_the_project_fix() {
    let env = project();
    std::fs::write(env.paper_toml(), "[large_files]\nmax_mb = 0\n").unwrap();
    let run = env.paper(&["status"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("paper.toml"), "{run:#?}");
    assert!(
        run.stderr
            .contains("paper config set --project large_files.max_mb"),
        "{run:#?}"
    );
}

#[test]
fn a_bad_environment_value_names_the_variable() {
    let env = project();
    let run = env.paper_in(&env.work, &[("PAPER_UI_COLOR", "purple")], &["status"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("PAPER_UI_COLOR"), "{run:#?}");
    assert!(run.stderr.contains("unset PAPER_UI_COLOR"), "{run:#?}");
}

#[test]
fn a_rejected_project_set_suggests_the_project_form() {
    let env = project();
    set(&env, &["--project", "large_files.warn_mb", "5"]);
    let run = env.paper(&["config", "set", "--project", "large_files.max_mb", "3"]);
    run.assert_three_part_error(1);
    assert!(
        run.stderr
            .contains("large_files.max_mb (3) can't be below large_files.warn_mb (5)"),
        "{run:#?}"
    );
    assert!(
        run.stderr.contains("paper config set --project"),
        "{run:#?}"
    );
}

#[test]
fn personal_settings_in_paper_toml_do_not_change_the_author_and_are_reported() {
    let env = project();
    std::fs::write(
        env.paper_toml(),
        "[user]\nname = \"Leaked Person\"\nemail = \"leak@lab.example\"\n",
    )
    .unwrap();
    edit_and_save(&env, &[]);
    assert_eq!(author(&env), "Mina Park/mina@uni.example");

    let get = env.paper(&["config", "get", "user.name"]);
    get.assert_code(0);
    assert_eq!(get.stdout.trim(), "Mina Park");
    assert!(
        get.stderr.contains("user.name in paper.toml is ignored"),
        "{get:#?}"
    );
    let list = env.paper(&["config", "list"]);
    list.assert_code(0);
    assert!(
        list.stderr.contains("user.email in paper.toml is ignored"),
        "{list:#?}"
    );
    assert!(list.stderr.contains("shared"), "{list:#?}");
}

#[test]
fn set_works_in_an_inline_table_and_refuses_a_plain_value_clearly() {
    let env = project();
    std::fs::write(env.paper_toml(), "large_files = { warn_mb = 5 }\n").unwrap();
    set(&env, &["--project", "large_files.max_mb", "8"]);
    let get = env.paper(&["config", "get", "large_files.max_mb"]);
    assert_eq!(get.stdout.trim(), "8");

    std::fs::write(env.paper_toml(), "large_files = 5\n").unwrap();
    let run = env.paper(&["config", "set", "--project", "large_files.max_mb", "8"]);
    run.assert_three_part_error(1);
    assert!(run.stderr.contains("large_files"), "{run:#?}");
    assert_eq!(
        std::fs::read_to_string(env.paper_toml()).unwrap(),
        "large_files = 5\n"
    );
}

#[test]
fn unset_values_show_a_dash_as_their_source() {
    let env = Env::without_identity();
    let run = env.paper(&["config", "list"]);
    run.assert_code(0);
    let row = run
        .stdout
        .lines()
        .find(|l| l.starts_with("profile"))
        .unwrap();
    assert!(row.trim_end().ends_with("(not set)  -"), "{row}");
    let get = env.paper(&["config", "get", "profile"]);
    get.assert_three_part_error(1);
    assert!(!get.stderr.contains("git"), "{get:#?}");
}

#[test]
fn get_and_set_json_have_a_fixed_shape() {
    let env = project();
    let set = env.paper(&["config", "set", "--json", "large_files.max_mb", "7"]);
    set.assert_code(0);
    let json = set.json();
    assert_keys(&json, &["schema", "key", "value", "file"]);
    assert_eq!(json["schema"], 1);
    assert_eq!(json["key"], "large_files.max_mb");
    assert_eq!(json["value"], 7);
    assert!(json["file"].as_str().unwrap().ends_with("config.toml"));

    let get = env.paper(&["config", "get", "--json", "large_files.max_mb"]);
    get.assert_code(0);
    let json = get.json();
    assert_keys(&json, &["schema", "key", "value", "source"]);
    assert_eq!(json["value"], 7);
    assert_eq!(json["source"], "user");
}

// ---- profile updates the .gitignore block (PV-004) ----

#[test]
fn set_project_profile_replaces_the_block_in_gitignore_and_says_so() {
    let env = project();
    assert!(env.read(".gitignore").contains("paper-vcs profile: latex"));
    let existing = env.read(".gitignore");
    env.write(".gitignore", &format!("my-own-line\n\n{existing}"));

    let run = env.paper(&["config", "set", "--project", "profile", "typst"]);

    run.assert_code(0);
    assert!(run.stdout.contains("Set profile = typst in "), "{run:#?}");
    assert!(run.stdout.contains(".gitignore"), "{run:#?}");
    let gitignore = env.read(".gitignore");
    assert!(gitignore.starts_with("my-own-line\n"), "{gitignore}");
    assert!(
        gitignore.contains("paper-vcs profile: typst"),
        "{gitignore}"
    );
    assert!(
        !gitignore.contains("paper-vcs profile: latex"),
        "{gitignore}"
    );
    assert!(env.read("paper.toml").contains("profile = \"typst\""));
}

#[test]
fn set_project_profile_creates_the_gitignore_when_there_is_none() {
    let env = project();
    std::fs::remove_file(env.work.join(".gitignore")).unwrap();

    env.paper(&["config", "set", "--project", "profile", "markdown"])
        .assert_code(0);

    assert!(
        env.read(".gitignore")
            .contains("paper-vcs profile: markdown")
    );
}

#[test]
fn set_project_profile_with_a_broken_block_fails_and_leaves_gitignore_alone() {
    let env = project();
    let broken = "# >>> paper-vcs profile: latex >>>\n*.aux\n";
    env.write(".gitignore", broken);
    let toml = env.read("paper.toml");

    let run = env.paper(&["config", "set", "--project", "profile", "typst"]);

    run.assert_three_part_error(1);
    assert_eq!(env.read(".gitignore"), broken);
    assert_eq!(env.read("paper.toml"), toml);
    assert!(
        run.stderr
            .contains("paper config set --project profile typst"),
        "the retry command should be the one that failed: {run:#?}"
    );
}

#[test]
fn set_project_profile_with_a_gitignore_that_is_not_utf8_is_a_user_error() {
    let env = project();
    std::fs::write(env.work.join(".gitignore"), b"caf\xe9\n").unwrap();
    let toml = env.read("paper.toml");

    let run = env.paper(&["config", "set", "--project", "profile", "typst"]);

    run.assert_three_part_error(1);
    assert!(run.stderr.contains("UTF-8"), "{run:#?}");
    assert!(
        run.stderr
            .contains("paper config set --project profile typst"),
        "{run:#?}"
    );
    assert_eq!(
        std::fs::read(env.work.join(".gitignore")).unwrap(),
        b"caf\xe9\n"
    );
    assert_eq!(env.read("paper.toml"), toml);
}

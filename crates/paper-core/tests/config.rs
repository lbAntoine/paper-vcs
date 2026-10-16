//! Settings: keys, the two config files and precedence. Files are real temp files; the process
//! environment is never read (an env lookup is injected).

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use paper_core::config::{
    ConfigError, ConfigFile, Home, Key, Scope, Settings, Source, Sources, Value,
    user_config_path_for,
};
use paper_core::project::{Project, StatusOptions};
use paper_core::snapshot::SnapshotOptions;
use paper_core::{ErrorKind, PaperError};
use paper_git::Identity;

fn lookup(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> + use<> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

fn file_with(text: &str) -> (tempfile::TempDir, ConfigFile) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, text).unwrap();
    let file = ConfigFile::load(&path).unwrap();
    (dir, file)
}

fn text(value: &str) -> Value {
    Value::Text(value.to_owned())
}

fn git_identity() -> Identity {
    Identity {
        name: "Git Person".to_owned(),
        email: "git@example.org".to_owned(),
    }
}

fn resolve(
    env: &[(&str, &str)],
    project: ConfigFile,
    user: ConfigFile,
    git: Option<Identity>,
) -> Settings {
    Settings::resolve(Sources {
        env: lookup(env),
        project,
        user,
        git_identity: git,
    })
    .unwrap()
}

// ---- keys ----

#[test]
fn every_key_round_trips_through_its_dotted_name() {
    let names: Vec<&str> = Key::ALL.iter().map(|k| k.as_str()).collect();
    assert_eq!(
        names,
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
    for key in Key::ALL {
        assert_eq!(Key::parse(key.as_str()).unwrap(), key);
    }
}

#[test]
fn env_names_are_paper_plus_the_upper_cased_key() {
    assert_eq!(Key::UserName.env_name(), "PAPER_USER_NAME");
    assert_eq!(Key::LargeFilesMaxMb.env_name(), "PAPER_LARGE_FILES_MAX_MB");
    assert_eq!(Key::Profile.env_name(), "PAPER_PROFILE");
}

#[test]
fn unknown_keys_and_ui_editor_name_the_known_keys() {
    for bad in ["ui.editor", "nonsense", "user", "large_files.max"] {
        let error = Key::parse(bad).unwrap_err();
        assert!(matches!(error, ConfigError::UnknownKey { .. }), "{bad}");
        let shown = PaperError::from(error);
        assert_eq!(shown.kind, ErrorKind::User, "{bad}");
        assert!(shown.message.contains(bad), "{shown:?}");
        assert!(shown.why.contains("user.name"), "{shown:?}");
        assert!(!shown.next.is_empty(), "{shown:?}");
    }
}

#[test]
fn each_key_has_a_home() {
    assert_eq!(Key::UserName.home(), Home::UserOnly);
    assert_eq!(Key::UserEmail.home(), Home::UserOnly);
    assert_eq!(Key::UiColor.home(), Home::UserOnly);
    assert_eq!(Key::LargeFilesWarnMb.home(), Home::Either);
    assert_eq!(Key::LargeFilesMaxMb.home(), Home::Either);
    assert_eq!(Key::DiffDefault.home(), Home::Either);
    assert_eq!(Key::Profile.home(), Home::ProjectOnly);
}

#[test]
fn valid_values_are_typed() {
    assert_eq!(
        Key::UserName.parse_value("Mina Park").unwrap(),
        text("Mina Park")
    );
    assert_eq!(
        Key::UserEmail.parse_value("mina@uni.example").unwrap(),
        text("mina@uni.example")
    );
    for color in ["auto", "always", "never"] {
        assert_eq!(Key::UiColor.parse_value(color).unwrap(), text(color));
    }
    assert_eq!(
        Key::LargeFilesMaxMb.parse_value("2").unwrap(),
        Value::Megabytes(2)
    );
    assert_eq!(
        Key::LargeFilesWarnMb.parse_value("10").unwrap(),
        Value::Megabytes(10)
    );
    assert_eq!(
        Key::DiffDefault.parse_value("lines").unwrap(),
        text("lines")
    );
    for profile in ["latex", "markdown", "typst", "quarto", "jupyter", "generic"] {
        assert_eq!(Key::Profile.parse_value(profile).unwrap(), text(profile));
    }
}

#[test]
fn invalid_values_are_user_errors_that_say_what_is_allowed() {
    let cases = [
        (Key::UiColor, "purple"),
        (Key::LargeFilesMaxMb, "0"),
        (Key::LargeFilesMaxMb, "-3"),
        (Key::LargeFilesMaxMb, "lots"),
        (Key::LargeFilesWarnMb, "0"),
        (Key::UserEmail, "nobody"),
        (Key::UserName, ""),
        (Key::UserName, "   "),
        (Key::DiffDefault, "sentences"),
        (Key::Profile, "word"),
    ];
    for (key, value) in cases {
        let error = key.parse_value(value).unwrap_err();
        assert!(
            matches!(error, ConfigError::InvalidValue { .. }),
            "{key:?} {value:?}"
        );
        let shown = PaperError::from(error);
        assert_eq!(shown.kind, ErrorKind::User, "{key:?} {value:?}");
        assert!(
            shown.message.contains(key.as_str()) || shown.why.contains(key.as_str()),
            "{shown:?}"
        );
        assert!(!shown.why.is_empty() && !shown.next.is_empty(), "{shown:?}");
    }
}

#[test]
fn words_for_diff_default_is_accepted_and_has_no_effect_yet() {
    assert_eq!(
        Key::DiffDefault.parse_value("words").unwrap(),
        text("words")
    );
    ConfigFile::empty()
        .with(Key::DiffDefault, text("words"))
        .unwrap();
}

#[test]
fn identity_and_looks_are_refused_in_the_project_file() {
    for key in [Key::UserName, Key::UserEmail, Key::UiColor] {
        let error = key.check_scope(Scope::Project).unwrap_err();
        assert!(matches!(error, ConfigError::WrongHome { .. }), "{key:?}");
        let shown = PaperError::from(error);
        assert_eq!(shown.kind, ErrorKind::User);
        assert!(!shown.why.is_empty() && !shown.next.is_empty(), "{shown:?}");
        key.check_scope(Scope::User).unwrap();
    }
}

#[test]
fn profile_is_refused_in_the_user_file() {
    assert!(matches!(
        Key::Profile.check_scope(Scope::User),
        Err(ConfigError::WrongHome { .. })
    ));
    Key::Profile.check_scope(Scope::Project).unwrap();
}

#[test]
fn shared_keys_are_allowed_in_both_files() {
    for key in [
        Key::LargeFilesWarnMb,
        Key::LargeFilesMaxMb,
        Key::DiffDefault,
    ] {
        key.check_scope(Scope::User).unwrap();
        key.check_scope(Scope::Project).unwrap();
    }
}

// ---- config files ----

#[test]
fn a_missing_file_loads_as_empty() {
    let dir = tempfile::tempdir().unwrap();
    let file = ConfigFile::load(&dir.path().join("nope").join("config.toml")).unwrap();
    for key in Key::ALL {
        assert_eq!(file.get(key).unwrap(), None, "{key:?}");
    }
}

#[test]
fn set_values_can_be_read_back() {
    let file = ConfigFile::empty()
        .with(Key::UserName, text("Mina Park"))
        .unwrap()
        .with(Key::LargeFilesMaxMb, Value::Megabytes(2))
        .unwrap();
    assert_eq!(file.get(Key::UserName).unwrap(), Some(text("Mina Park")));
    assert_eq!(
        file.get(Key::LargeFilesMaxMb).unwrap(),
        Some(Value::Megabytes(2))
    );
    assert_eq!(file.get(Key::UserEmail).unwrap(), None);
}

#[test]
fn with_returns_a_new_file_and_leaves_the_old_one_alone() {
    let before = ConfigFile::empty();
    let after = before.with(Key::UiColor, text("always")).unwrap();
    assert_eq!(before.get(Key::UiColor).unwrap(), None);
    assert_eq!(after.get(Key::UiColor).unwrap(), Some(text("always")));
}

#[test]
fn a_value_of_the_wrong_type_is_refused() {
    let error = ConfigFile::empty()
        .with(Key::LargeFilesMaxMb, text("2"))
        .unwrap_err();
    assert!(matches!(error, ConfigError::InvalidValue { .. }));
    let error = ConfigFile::empty()
        .with(Key::UserName, Value::Megabytes(2))
        .unwrap_err();
    assert!(matches!(error, ConfigError::InvalidValue { .. }));
}

#[test]
fn max_below_warn_in_the_same_file_is_refused_both_ways() {
    let warn5 = ConfigFile::empty()
        .with(Key::LargeFilesWarnMb, Value::Megabytes(5))
        .unwrap();
    assert!(matches!(
        warn5.with(Key::LargeFilesMaxMb, Value::Megabytes(3)),
        Err(ConfigError::InvalidValue { .. })
    ));
    warn5
        .with(Key::LargeFilesMaxMb, Value::Megabytes(5))
        .unwrap();

    let max3 = ConfigFile::empty()
        .with(Key::LargeFilesMaxMb, Value::Megabytes(3))
        .unwrap();
    assert!(matches!(
        max3.with(Key::LargeFilesWarnMb, Value::Megabytes(5)),
        Err(ConfigError::InvalidValue { .. })
    ));
}

#[test]
fn max_alone_below_the_default_warn_is_accepted() {
    // `config set --project large_files.max_mb 1` must work with the default warn of 10.
    ConfigFile::empty()
        .with(Key::LargeFilesMaxMb, Value::Megabytes(1))
        .unwrap();
}

const COMMENTED: &str = "\
# My paper settings
# keep this note

[user]
name = \"Mina Park\"   # my name

[custom]
anything = [1, 2, 3]   # not ours
";

#[test]
fn adding_a_key_keeps_the_existing_text_byte_for_byte() {
    let (dir, file) = file_with(COMMENTED);
    let updated = file.with(Key::UiColor, text("never")).unwrap();
    let path = dir.path().join("out.toml");
    updated.write(&path).unwrap();
    let written = fs::read_to_string(&path).unwrap();
    assert!(written.starts_with(COMMENTED), "{written}");
    assert!(written.contains("[ui]") && written.contains("color = \"never\""));
}

#[test]
fn changing_a_key_changes_only_its_line() {
    let (dir, file) = file_with(COMMENTED);
    let updated = file.with(Key::UserName, text("Alex Doe")).unwrap();
    let path = dir.path().join("out.toml");
    updated.write(&path).unwrap();
    let written = fs::read_to_string(&path).unwrap();
    let old: Vec<&str> = COMMENTED.lines().collect();
    let new: Vec<&str> = written.lines().collect();
    assert_eq!(old.len(), new.len(), "{written}");
    let changed: Vec<usize> = (0..old.len()).filter(|i| old[*i] != new[*i]).collect();
    assert_eq!(changed.len(), 1, "{written}");
    assert!(new[changed[0]].contains("Alex Doe"), "{written}");
    assert!(new[changed[0]].contains("# my name"), "{written}");
}

#[test]
fn write_creates_missing_parent_folders_and_round_trips() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("deep").join("paper").join("config.toml");
    let file = ConfigFile::empty()
        .with(Key::UserEmail, text("mina@uni.example"))
        .unwrap();
    file.write(&path).unwrap();
    let again = ConfigFile::load(&path).unwrap();
    assert_eq!(
        again.get(Key::UserEmail).unwrap(),
        Some(text("mina@uni.example"))
    );
    assert_eq!(file.render(), fs::read_to_string(&path).unwrap());
}

#[test]
fn a_malformed_file_names_the_file_and_the_line() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("paper.toml");
    fs::write(&path, "[large_files]\nmax_mb = 1\nthis is not toml\n").unwrap();
    let error = ConfigFile::load(&path).unwrap_err();
    assert!(
        matches!(&error, ConfigError::Malformed { line: 3, .. }),
        "{error:?}"
    );
    let shown = PaperError::from(error);
    assert_eq!(shown.kind, ErrorKind::User);
    assert!(shown.message.contains("paper.toml"), "{shown:?}");
    assert!(
        format!("{} {}", shown.message, shown.why).contains('3'),
        "{shown:?}"
    );
    assert!(!shown.next.is_empty());
}

#[test]
fn a_bad_value_already_in_the_file_is_reported_by_key() {
    let (_dir, file) = file_with("[large_files]\nmax_mb = \"big\"\n");
    let error = file.get(Key::LargeFilesMaxMb).unwrap_err();
    assert!(matches!(error, ConfigError::InvalidValue { .. }));
    assert!(error.to_string().contains("large_files.max_mb"), "{error}");
}

#[test]
fn an_unreadable_path_is_a_project_error() {
    let dir = tempfile::tempdir().unwrap();
    // A folder where the file should be.
    let error = ConfigFile::load(dir.path()).unwrap_err();
    assert!(matches!(error, ConfigError::Io { .. }), "{error:?}");
    assert_eq!(PaperError::from(error).kind, ErrorKind::Project);
}

// ---- precedence ----

#[test]
fn defaults_apply_when_nothing_is_set() {
    let s = resolve(&[], ConfigFile::empty(), ConfigFile::empty(), None);
    let ui = s.entry(Key::UiColor);
    assert_eq!(
        (ui.value.clone(), ui.source),
        (Some(text("auto")), Source::Default)
    );
    let warn = s.entry(Key::LargeFilesWarnMb);
    assert_eq!(
        (warn.value.clone(), warn.source),
        (Some(Value::Megabytes(10)), Source::Default)
    );
    let max = s.entry(Key::LargeFilesMaxMb);
    assert_eq!(
        (max.value.clone(), max.source),
        (Some(Value::Megabytes(100)), Source::Default)
    );
    let diff = s.entry(Key::DiffDefault);
    assert_eq!(
        (diff.value.clone(), diff.source),
        (Some(text("lines")), Source::Default)
    );
    assert_eq!(s.entry(Key::Profile).value, None);
    assert_eq!(s.entry(Key::UserName).value, None);
    assert_eq!(s.identity(), None);
    assert_eq!(s.snapshot_options(), SnapshotOptions::default());
    let keys: Vec<Key> = s.entries().iter().map(|e| e.key).collect();
    assert_eq!(keys, Key::ALL);
}

#[test]
fn identity_prefers_env_then_user_config_then_git() {
    let user = ConfigFile::empty()
        .with(Key::UserName, text("User Name"))
        .unwrap()
        .with(Key::UserEmail, text("user@example.org"))
        .unwrap();

    let git_only = resolve(
        &[],
        ConfigFile::empty(),
        ConfigFile::empty(),
        Some(git_identity()),
    );
    assert_eq!(git_only.identity(), Some(git_identity()));
    assert_eq!(git_only.entry(Key::UserName).source, Source::Git);

    let with_user = resolve(&[], ConfigFile::empty(), user.clone(), Some(git_identity()));
    assert_eq!(
        with_user.identity(),
        Some(Identity {
            name: "User Name".to_owned(),
            email: "user@example.org".to_owned()
        })
    );
    assert_eq!(with_user.entry(Key::UserEmail).source, Source::User);

    let with_env = resolve(
        &[
            ("PAPER_USER_NAME", "Env Name"),
            ("PAPER_USER_EMAIL", "env@example.org"),
        ],
        ConfigFile::empty(),
        user,
        Some(git_identity()),
    );
    assert_eq!(
        with_env.identity(),
        Some(Identity {
            name: "Env Name".to_owned(),
            email: "env@example.org".to_owned()
        })
    );
    assert_eq!(with_env.entry(Key::UserName).source, Source::Env);
}

#[test]
fn name_and_email_resolve_independently() {
    let user = ConfigFile::empty()
        .with(Key::UserName, text("User Name"))
        .unwrap();
    let s = resolve(&[], ConfigFile::empty(), user, Some(git_identity()));
    assert_eq!(s.entry(Key::UserName).source, Source::User);
    assert_eq!(s.entry(Key::UserEmail).source, Source::Git);
    assert_eq!(
        s.identity(),
        Some(Identity {
            name: "User Name".to_owned(),
            email: "git@example.org".to_owned()
        })
    );
}

#[test]
fn half_an_identity_is_no_identity() {
    let user = ConfigFile::empty()
        .with(Key::UserName, text("User Name"))
        .unwrap();
    let s = resolve(&[], ConfigFile::empty(), user, None);
    assert_eq!(s.identity(), None);
    assert_eq!(s.entry(Key::UserEmail).value, None);
}

#[test]
fn large_file_limits_prefer_env_then_project_then_user_then_default() {
    let max = |mb| {
        ConfigFile::empty()
            .with(Key::LargeFilesMaxMb, Value::Megabytes(mb))
            .unwrap()
    };
    let mb = |s: &Settings| {
        let e = s.entry(Key::LargeFilesMaxMb);
        (e.value.clone(), e.source)
    };

    let user_only = resolve(&[], ConfigFile::empty(), max(50), None);
    assert_eq!(mb(&user_only), (Some(Value::Megabytes(50)), Source::User));

    let project_wins = resolve(&[], max(2), max(50), None);
    assert_eq!(
        mb(&project_wins),
        (Some(Value::Megabytes(2)), Source::Project)
    );
    assert_eq!(
        project_wins.snapshot_options().max_file_bytes,
        2 * 1024 * 1024
    );

    let env_wins = resolve(&[("PAPER_LARGE_FILES_MAX_MB", "7")], max(2), max(50), None);
    assert_eq!(mb(&env_wins), (Some(Value::Megabytes(7)), Source::Env));
    assert_eq!(env_wins.snapshot_options().max_file_bytes, 7 * 1024 * 1024);
}

#[test]
fn snapshot_options_convert_megabytes_and_keep_warn_at_or_below_max() {
    let file = ConfigFile::empty()
        .with(Key::LargeFilesWarnMb, Value::Megabytes(3))
        .unwrap()
        .with(Key::LargeFilesMaxMb, Value::Megabytes(8))
        .unwrap();
    let options = resolve(&[], file, ConfigFile::empty(), None).snapshot_options();
    assert_eq!(options.warn_file_bytes, 3 * 1024 * 1024);
    assert_eq!(options.max_file_bytes, 8 * 1024 * 1024);

    // Only max set (below the default warn of 10): warn is clamped, not an error.
    let only_max = ConfigFile::empty()
        .with(Key::LargeFilesMaxMb, Value::Megabytes(1))
        .unwrap();
    let options = resolve(&[], only_max, ConfigFile::empty(), None).snapshot_options();
    assert_eq!(options.max_file_bytes, 1024 * 1024);
    assert!(options.warn_file_bytes <= options.max_file_bytes);
}

#[test]
fn project_only_keys_come_from_the_project_file() {
    let project = ConfigFile::empty()
        .with(Key::Profile, text("latex"))
        .unwrap();
    let s = resolve(&[], project, ConfigFile::empty(), None);
    let entry = s.entry(Key::Profile);
    assert_eq!(
        (entry.value.clone(), entry.source),
        (Some(text("latex")), Source::Project)
    );
}

#[test]
fn a_bad_env_value_is_an_error_naming_the_variable_or_key() {
    let result = Settings::resolve(Sources {
        env: lookup(&[("PAPER_UI_COLOR", "purple")]),
        project: ConfigFile::empty(),
        user: ConfigFile::empty(),
        git_identity: None,
    });
    let error = result.unwrap_err();
    assert!(matches!(error, ConfigError::InvalidValue { .. }));
    assert!(error.to_string().contains("ui.color"), "{error}");
}

#[test]
fn an_empty_env_value_counts_as_unset() {
    let s = resolve(
        &[("PAPER_USER_NAME", "")],
        ConfigFile::empty(),
        ConfigFile::empty(),
        Some(git_identity()),
    );
    assert_eq!(s.entry(Key::UserName).source, Source::Git);
}

// ---- user config path ----

fn path_for(windows: bool, env: &[(&str, &str)]) -> Option<PathBuf> {
    user_config_path_for(windows, &lookup(env))
}

#[test]
fn unix_uses_xdg_config_home_then_dot_config() {
    let config: PathBuf = ["paper", "config.toml"].iter().collect();
    assert_eq!(
        path_for(false, &[("XDG_CONFIG_HOME", "/x/cfg"), ("HOME", "/h")]),
        Some(Path::new("/x/cfg").join(&config))
    );
    assert_eq!(
        path_for(false, &[("HOME", "/h")]),
        Some(Path::new("/h").join(".config").join(&config))
    );
    assert_eq!(
        path_for(false, &[("XDG_CONFIG_HOME", ""), ("HOME", "/h")]),
        Some(Path::new("/h").join(".config").join(&config))
    );
}

#[test]
fn windows_uses_appdata() {
    assert_eq!(
        path_for(
            true,
            &[
                ("APPDATA", "C:\\Users\\m\\AppData\\Roaming"),
                ("HOME", "/h")
            ]
        ),
        Some(
            Path::new("C:\\Users\\m\\AppData\\Roaming")
                .join("paper")
                .join("config.toml")
        )
    );
}

#[test]
fn no_base_folder_means_no_path() {
    assert_eq!(path_for(false, &[]), None);
    assert_eq!(path_for(true, &[("HOME", "/h")]), None);
}

// ---- project uses the settings ----

#[test]
fn a_project_opened_with_options_skips_new_files_over_the_limit() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("a.tex"), "a\n").unwrap();
    let identity = Identity {
        name: "Mina Park".to_owned(),
        email: "mina@uni.example".to_owned(),
    };
    Project::init(dir.path(), identity.clone(), &SnapshotOptions::default()).unwrap();
    fs::write(dir.path().join("two.bin"), vec![b'x'; 2 * 1024 * 1024]).unwrap();

    let options = resolve(
        &[("PAPER_LARGE_FILES_MAX_MB", "1")],
        ConfigFile::empty(),
        ConfigFile::empty(),
        None,
    )
    .snapshot_options();
    let mut project = Project::open(dir.path(), identity)
        .unwrap()
        .with_options(options);
    let status = project.status(&StatusOptions { snapshot: true }).unwrap();

    assert!(
        status.skipped.iter().any(|s| s.path == "two.bin"),
        "{status:?}"
    );
    assert!(status.changes.iter().all(|c| c.path != "two.bin"));
}

#[test]
fn config_errors_convert_to_the_right_kind() {
    let user = PaperError::from(ConfigError::UnknownKey {
        key: "x".to_owned(),
    });
    assert_eq!(user.kind, ErrorKind::User);
    assert!(!user.message.is_empty() && !user.why.is_empty() && !user.next.is_empty());
}

#[test]
fn the_no_identity_error_suggests_paper_config_before_git_config() {
    let error = PaperError::no_identity();
    let commands: Vec<&str> = error.next.iter().map(|s| s.command.as_str()).collect();
    assert!(
        commands[0].starts_with("paper config set user.name"),
        "{commands:?}"
    );
    assert!(
        commands
            .iter()
            .any(|c| c.starts_with("paper config set user.email")),
        "{commands:?}"
    );
    assert!(
        commands
            .iter()
            .any(|c| c.starts_with("git config --global user.name")),
        "{commands:?}"
    );
}

// ---- review round 1 ----

fn project_file_at(dir: &Path, text: &str) -> ConfigFile {
    let path = dir.join("paper.toml");
    fs::write(&path, text).unwrap();
    ConfigFile::load(&path).unwrap()
}

#[test]
fn personal_keys_in_the_project_file_are_never_read() {
    let dir = tempfile::tempdir().unwrap();
    let project = project_file_at(
        dir.path(),
        "[user]\nname = \"Leaked\"\nemail = \"leak@example.org\"\n[ui]\ncolor = \"always\"\n",
    );
    let s = resolve(&[], project, ConfigFile::empty(), None);
    assert_eq!(s.identity(), None);
    let color = s.entry(Key::UiColor);
    assert_eq!(
        (color.value.clone(), color.source),
        (Some(text("auto")), Source::Default)
    );
    assert_eq!(
        s.ignored_project_keys(),
        [Key::UserName, Key::UserEmail, Key::UiColor]
    );
}

#[test]
fn no_ignored_keys_when_the_project_file_is_clean() {
    let s = resolve(&[], ConfigFile::empty(), ConfigFile::empty(), None);
    assert!(s.ignored_project_keys().is_empty());
}

#[test]
fn a_bad_value_in_a_file_names_the_file_and_suggests_the_project_form() {
    let dir = tempfile::tempdir().unwrap();
    let project = project_file_at(dir.path(), "[large_files]\nmax_mb = 0\n");
    let error = Settings::resolve(Sources {
        env: lookup(&[]),
        project,
        user: ConfigFile::empty(),
        git_identity: None,
    })
    .unwrap_err();
    let shown = PaperError::from(error);
    assert!(shown.message.contains("paper.toml"), "{shown:?}");
    let commands: Vec<&str> = shown.next.iter().map(|s| s.command.as_str()).collect();
    assert!(
        commands
            .iter()
            .any(|c| c.starts_with("paper config set --project large_files.max_mb")),
        "{commands:?}"
    );
    assert!(
        commands.iter().any(|c| c.ends_with("paper.toml")),
        "{commands:?}"
    );
}

#[test]
fn a_bad_user_file_value_suggests_the_user_form() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    fs::write(&path, "[ui]\ncolor = \"purple\"\n").unwrap();
    let error = Settings::resolve(Sources {
        env: lookup(&[]),
        project: ConfigFile::empty(),
        user: ConfigFile::load(&path).unwrap(),
        git_identity: None,
    })
    .unwrap_err();
    let shown = PaperError::from(error);
    assert!(shown.message.contains("config.toml"), "{shown:?}");
    assert_eq!(shown.next[0].command, "paper config set ui.color auto");
}

#[test]
fn a_bad_env_value_names_the_variable_and_says_to_unset_it() {
    let error = Settings::resolve(Sources {
        env: lookup(&[("PAPER_UI_COLOR", "purple")]),
        project: ConfigFile::empty(),
        user: ConfigFile::empty(),
        git_identity: None,
    })
    .unwrap_err();
    let shown = PaperError::from(error);
    assert!(shown.message.contains("PAPER_UI_COLOR"), "{shown:?}");
    let expected = if cfg!(windows) {
        "Remove-Item Env:PAPER_UI_COLOR"
    } else {
        "unset PAPER_UI_COLOR"
    };
    assert_eq!(shown.next[0].command, expected);
}

#[test]
fn order_conflicts_read_as_a_sentence_and_offer_both_fixes() {
    let dir = tempfile::tempdir().unwrap();
    let project = project_file_at(dir.path(), "[large_files]\nwarn_mb = 10\n");
    let error = project
        .with(Key::LargeFilesMaxMb, Value::Megabytes(1))
        .unwrap_err();
    let shown = PaperError::from_config(&error, Scope::Project);
    assert!(
        shown
            .message
            .starts_with("large_files.max_mb (1) can't be below large_files.warn_mb (10) in "),
        "{shown:?}"
    );
    assert!(shown.message.contains("paper.toml"), "{shown:?}");
    let commands: Vec<&str> = shown.next.iter().map(|s| s.command.as_str()).collect();
    assert_eq!(
        commands,
        [
            "paper config set --project large_files.max_mb 10",
            "paper config set --project large_files.warn_mb 1"
        ]
    );
}

#[test]
fn a_rejected_set_aimed_at_the_project_suggests_the_project_form() {
    let error = Key::UiColor.parse_value("purple").unwrap_err();
    let shown = PaperError::from_config(&error, Scope::Project);
    assert_eq!(
        shown.next[0].command,
        "paper config set --project ui.color auto"
    );
}

#[test]
fn values_are_written_into_an_existing_inline_table() {
    let (dir, file) = file_with("large_files = { warn_mb = 5 }   # mine\n");
    let updated = file
        .with(Key::LargeFilesMaxMb, Value::Megabytes(8))
        .unwrap();
    assert_eq!(
        updated.get(Key::LargeFilesMaxMb).unwrap(),
        Some(Value::Megabytes(8))
    );
    assert_eq!(
        updated.get(Key::LargeFilesWarnMb).unwrap(),
        Some(Value::Megabytes(5))
    );
    let path = dir.path().join("out.toml");
    updated.write(&path).unwrap();
    assert!(fs::read_to_string(&path).unwrap().contains("# mine"));
    let changed = updated
        .with(Key::LargeFilesWarnMb, Value::Megabytes(6))
        .unwrap();
    assert_eq!(
        changed.get(Key::LargeFilesWarnMb).unwrap(),
        Some(Value::Megabytes(6))
    );
}

#[test]
fn a_plain_value_where_a_table_belongs_is_a_teaching_error() {
    let (_dir, file) = file_with("large_files = 5\n");
    let error = file
        .with(Key::LargeFilesMaxMb, Value::Megabytes(8))
        .unwrap_err();
    assert!(matches!(error, ConfigError::Blocked { .. }), "{error:?}");
    let shown = PaperError::from(error);
    assert_eq!(shown.kind, ErrorKind::User);
    assert!(shown.message.contains("large_files"), "{shown:?}");
    assert!(!shown.why.is_empty() && !shown.next.is_empty());
}

#[test]
fn lines_added_to_a_crlf_file_use_crlf() {
    let (_dir, file) = file_with("# mine\r\n[user]\r\nname = \"Mina Park\"\r\n");
    let added = file.with(Key::UiColor, text("never")).unwrap().render();
    assert!(added.starts_with("# mine\r\n[user]\r\n"), "{added:?}");
    assert!(!added.replace("\r\n", "").contains('\n'), "{added:?}");
    let in_table = file
        .with(Key::UserEmail, text("m@u.example"))
        .unwrap()
        .render();
    assert!(!in_table.replace("\r\n", "").contains('\n'), "{in_table:?}");
}

#[test]
fn a_relative_xdg_config_home_is_ignored() {
    assert_eq!(
        path_for(
            false,
            &[("XDG_CONFIG_HOME", "relative/cfg"), ("HOME", "/h")]
        ),
        Some(
            Path::new("/h")
                .join(".config")
                .join("paper")
                .join("config.toml")
        )
    );
}

// ---- mixed line endings (PV-014 item 12) ----

#[test]
fn a_mixed_file_keeps_its_existing_line_endings_when_a_key_changes() {
    let original = "# mine\r\n[ui]\r\ncolor = \"auto\"\nother = 1\r\n";
    let (_dir, file) = file_with(original);

    let rendered = file.with(Key::UiColor, text("never")).unwrap().render();

    assert_eq!(
        rendered,
        "# mine\r\n[ui]\r\ncolor = \"never\"\nother = 1\r\n"
    );
}

#[test]
fn a_new_table_in_a_mixed_file_uses_the_ending_of_the_first_line_and_keeps_old_bytes() {
    let original = "# mine\r\n[user]\nname = \"Mina Park\"\r\n";
    let (_dir, file) = file_with(original);

    let rendered = file.with(Key::UiColor, text("never")).unwrap().render();

    assert!(rendered.starts_with(original), "{rendered:?}");
    let added = &rendered[original.len()..];
    assert!(added.contains("color = \"never\"\r\n"), "{added:?}");
    assert!(!added.replace("\r\n", "").contains('\n'), "{added:?}");
}

#[test]
fn a_new_table_in_an_lf_first_file_uses_lf_even_if_later_lines_are_crlf() {
    let original = "# mine\n[user]\r\nname = \"Mina Park\"\r\n";
    let (_dir, file) = file_with(original);

    let rendered = file.with(Key::UiColor, text("never")).unwrap().render();

    assert!(rendered.starts_with(original), "{rendered:?}");
    let added = &rendered[original.len()..];
    assert!(added.contains("color = \"never\"\n"), "{added:?}");
    assert!(!added.contains('\r'), "{added:?}");
}

// ---- no final newline, and comments above a key (PV-014 review round 1) ----

fn reloaded(file: &ConfigFile) -> (tempfile::TempDir, ConfigFile, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    file.write(&path).unwrap();
    let bytes = fs::read_to_string(&path).unwrap();
    let again = ConfigFile::load(&path).unwrap();
    (dir, again, bytes)
}

#[test]
fn an_lf_file_without_a_final_newline_gains_a_key_in_an_existing_table() {
    let (_dir, file) = file_with("[user]\nname = \"A\"");
    let updated = file.with(Key::UserEmail, text("a@b.org")).unwrap();
    let (_d, again, bytes) = reloaded(&updated);
    assert_eq!(bytes, "[user]\nname = \"A\"\nemail = \"a@b.org\"\n");
    assert_eq!(again.get(Key::UserEmail).unwrap(), Some(text("a@b.org")));
    assert_eq!(again.get(Key::UserName).unwrap(), Some(text("A")));
}

#[test]
fn a_crlf_file_without_a_final_newline_gains_a_key_with_crlf() {
    let (_dir, file) = file_with("[user]\r\nname = \"A\"");
    let updated = file.with(Key::UserEmail, text("a@b.org")).unwrap();
    let (_d, again, bytes) = reloaded(&updated);
    assert_eq!(bytes, "[user]\r\nname = \"A\"\r\nemail = \"a@b.org\"\r\n");
    assert_eq!(again.get(Key::UserEmail).unwrap(), Some(text("a@b.org")));
}

#[test]
fn editing_the_last_key_of_a_file_without_a_final_newline_works_for_lf_and_crlf() {
    for (eol, original) in [
        ("\n", "[user]\nname = \"A\""),
        ("\r\n", "[user]\r\nname = \"A\""),
    ] {
        let (_dir, file) = file_with(original);
        let updated = file.with(Key::UserName, text("B")).unwrap();
        let (_d, again, bytes) = reloaded(&updated);
        assert_eq!(bytes, format!("[user]{eol}name = \"B\"{eol}"));
        assert_eq!(again.get(Key::UserName).unwrap(), Some(text("B")));
    }
}

#[test]
fn a_new_table_after_a_file_without_a_final_newline_is_valid() {
    let (_dir, file) = file_with("[user]\nname = \"A\"");
    let updated = file.with(Key::UiColor, text("never")).unwrap();
    let (_d, again, _bytes) = reloaded(&updated);
    assert_eq!(again.get(Key::UiColor).unwrap(), Some(text("never")));
    assert_eq!(again.get(Key::UserName).unwrap(), Some(text("A")));
}

#[test]
fn a_comment_above_a_top_level_key_survives_changing_it() {
    let (_dir, file) = file_with("# why latex\nprofile = \"latex\"\n");
    let rendered = file.with(Key::Profile, text("generic")).unwrap().render();
    assert_eq!(rendered, "# why latex\nprofile = \"generic\"\n");
}

#[test]
fn a_comment_above_a_key_in_a_table_survives_changing_it() {
    let (_dir, file) = file_with("[ui]\n# I like colour\ncolor = \"auto\"\n");
    let rendered = file.with(Key::UiColor, text("never")).unwrap().render();
    assert_eq!(rendered, "[ui]\n# I like colour\ncolor = \"never\"\n");
}

// ---- CRLF inside multi-line strings (PV-014 post-approval) ----

#[test]
fn a_crlf_file_with_a_multi_line_string_keeps_its_bytes_when_a_key_changes() {
    let original = "[user]\r\nname = \"\"\"a\r\nb\"\"\"\r\n";
    let (_dir, file) = file_with(original);
    let updated = file.with(Key::UserEmail, text("a@b.org")).unwrap();
    let (_d, again, bytes) = reloaded(&updated);
    assert!(bytes.starts_with(original), "{bytes:?}");
    assert_eq!(bytes, format!("{original}email = \"a@b.org\"\r\n"));
    assert_eq!(
        again.get(Key::UserName).unwrap(),
        file.get(Key::UserName).unwrap()
    );
}

#[test]
fn an_lf_first_file_keeps_a_crlf_multi_line_string_in_an_unknown_table() {
    let original = "# mine\n[notes]\ntext = \"\"\"x\r\ny\"\"\"\n";
    let (_dir, file) = file_with(original);
    let updated = file.with(Key::UiColor, text("never")).unwrap();
    let (_d, again, bytes) = reloaded(&updated);
    assert!(bytes.starts_with(original), "{bytes:?}");
    assert_eq!(again.get(Key::UiColor).unwrap(), Some(text("never")));
}

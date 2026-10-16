//! The snapshot redaction helpers, checked with Windows-style paths so a Unix machine can
//! catch the mistakes Windows CI would.

mod common;

use common::{redact_ids, redact_root, redact_strings};

const WINDOWS_ROOT: &str = r"C:\Users\runneradmin\AppData\Local\Temp\.tmpVLbVeJ";

#[test]
fn a_windows_root_is_replaced_in_plain_text() {
    let text = format!(r"{WINDOWS_ROOT}\work is not a project");
    assert_eq!(
        redact_root(&text, WINDOWS_ROOT),
        "[TMP]/work is not a project"
    );
}

#[test]
fn a_windows_root_is_replaced_inside_json_strings() {
    // As the CLI prints it: backslashes escaped in the JSON text.
    let stdout =
        serde_json::json!({"why": format!(r"{WINDOWS_ROOT}\work is not a project")}).to_string();
    assert!(
        stdout.contains(r"\\work"),
        "the JSON text is escaped: {stdout}"
    );
    let mut value: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    redact_strings(&mut value, &|text| redact_root(text, WINDOWS_ROOT));

    assert_eq!(value["why"], "[TMP]/work is not a project");
}

#[test]
fn unix_roots_still_work() {
    assert_eq!(
        redact_root("/tmp/.abc/work is here", "/tmp/.abc"),
        "[TMP]/work is here"
    );
}

// ---- whole tokens only (PV-014 item 1) ----

const CHANGE_ID: &str = "yourkmnopqrstuvw";
const OTHER_ID: &str = "mostlykmnopqrstu";
const HASH: &str = "defaced011111111111111111111111111111111";

fn ids() -> Vec<(String, String)> {
    vec![
        (CHANGE_ID.to_owned(), HASH.to_owned()),
        (
            OTHER_ID.to_owned(),
            "0123456789abcdef0123456789abcdef01234567".to_owned(),
        ),
    ]
}

#[test]
fn a_word_that_starts_like_an_id_is_left_alone() {
    let text = "Is it yours yourself or mostly done? Saved as yourkmn.";
    assert_eq!(
        redact_ids(text, &ids()),
        "Is it yours yourself or mostly done? Saved as [ID]."
    );
}

#[test]
fn full_ids_and_their_short_forms_are_replaced_at_token_edges() {
    let text = format!(
        "{CHANGE_ID}\n({}) id={}, then {}",
        &CHANGE_ID[..6],
        OTHER_ID,
        &OTHER_ID[..4]
    );
    assert_eq!(redact_ids(&text, &ids()), "[ID]\n([ID]) id=[ID], then [ID]");
}

#[test]
fn a_token_that_only_starts_with_the_whole_id_is_not_an_id() {
    let text = format!("{CHANGE_ID}xyz and {CHANGE_ID}");
    assert_eq!(
        redact_ids(&text, &ids()),
        format!("{CHANGE_ID}xyz and [ID]")
    );
}

#[test]
fn hashes_are_replaced_whole_or_as_seven_character_prefixes() {
    let text = format!("defacedly speaking, but {} and {HASH} differ.", &HASH[..7]);
    assert_eq!(
        redact_ids(&text, &ids()),
        "defacedly speaking, but [HASH] and [HASH] differ."
    );
}

#[test]
fn json_strings_are_redacted_token_by_token() {
    let stdout = serde_json::json!({
        "message": "Is it yours yourself or mostly done?",
        "id": &CHANGE_ID[..7],
        "hash": HASH,
        "list": ["mostly", OTHER_ID],
    })
    .to_string();
    let mut value: serde_json::Value = serde_json::from_str(&stdout).unwrap();

    redact_strings(&mut value, &|text| redact_ids(text, &ids()));

    assert_eq!(value["message"], "Is it yours yourself or mostly done?");
    assert_eq!(value["id"], "[ID]");
    assert_eq!(value["hash"], "[HASH]");
    assert_eq!(value["list"], serde_json::json!(["mostly", "[ID]"]));
}

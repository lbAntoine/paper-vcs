//! The snapshot redaction helpers, checked with Windows-style paths so a Unix machine can
//! catch the mistakes Windows CI would.

mod common;

use common::{redact_root, redact_strings};

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

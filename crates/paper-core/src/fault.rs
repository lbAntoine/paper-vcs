//! Debug-only fault injection: `crash_point("<name>")` aborts the process when the
//! environment variable `PAPER_DEBUG_CRASH_AT` equals `<name>`, so tests can kill a command
//! at a known moment and check that the next command repairs the project
//! (03-architecture.md § Crash safety).
//!
//! The points are `after-snapshot`, `after-marker`, `after-keep`, `after-op`, `after-index`,
//! `after-refs`, `after-export`, `mid-checkout` (after the first file) and `after-checkout`.
//! Release builds compile the hook out, so they ignore the variable.

/// Aborts the process (`std::process::abort`) if `PAPER_DEBUG_CRASH_AT` is `name`. Does
/// nothing otherwise, and nothing at all in release builds.
pub fn crash_point(_name: &str) {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_crash_point_returns_when_no_crash_was_requested() {
        // The test environment never sets PAPER_DEBUG_CRASH_AT.
        assert!(std::env::var_os("PAPER_DEBUG_CRASH_AT").is_none());
        crash_point("after-op");
        crash_point("a-name-that-is-not-a-point");
    }
}

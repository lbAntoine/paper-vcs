//! Panics: a crash report in the user's cache folder, then exit code 101.

use std::backtrace::Backtrace;
use std::fs;
use std::path::PathBuf;

const ISSUES_URL: &str = "https://github.com/lbAntoine/paper-vcs/issues";

/// Replaces the default panic message with a report file and a pointer to the issue tracker.
pub fn install() {
    std::panic::set_hook(Box::new(|info| {
        let report = format!(
            "paper {} on {} {}\n\n{info}\n\n{}\n",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH,
            Backtrace::force_capture()
        );
        eprintln!("paper hit a bug and stopped. Your files were not changed by the crash.");
        match write_report(&report) {
            Some(path) => eprintln!("A report was saved to {}", path.display()),
            None => eprintln!("The report could not be saved; it is printed below.\n{report}"),
        }
        eprintln!("Please tell us at {ISSUES_URL} and attach the report.");
        std::process::exit(101);
    }));
}

/// In debug builds, `PAPER_DEBUG_PANIC=1` panics on purpose so tests can exercise the hook.
/// Compiled out of release builds.
#[cfg(debug_assertions)]
pub fn debug_trigger() {
    let requested = std::env::var_os("PAPER_DEBUG_PANIC").is_some_and(|value| value == "1");
    assert!(!requested, "PAPER_DEBUG_PANIC is set");
}

#[cfg(not(debug_assertions))]
pub fn debug_trigger() {}

fn write_report(report: &str) -> Option<PathBuf> {
    let dir = cache_dir()?;
    fs::create_dir_all(&dir).ok()?;
    let stamp = jiff::Timestamp::now().strftime("%Y%m%dT%H%M%SZ");
    let path = dir.join(format!("crash-{stamp}-{}.txt", std::process::id()));
    fs::write(&path, report).ok()?;
    Some(path)
}

/// `%LOCALAPPDATA%\paper` on Windows, `$XDG_CACHE_HOME/paper` or `~/.cache/paper` elsewhere.
fn cache_dir() -> Option<PathBuf> {
    let var = |name: &str| std::env::var_os(name).filter(|v| !v.is_empty());
    if cfg!(windows) {
        return var("LOCALAPPDATA").map(|base| PathBuf::from(base).join("paper"));
    }
    var("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .or_else(|| var("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .map(|base| base.join("paper"))
}

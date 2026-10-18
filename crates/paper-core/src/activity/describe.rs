//! The words an operation is recorded with, in researcher vocabulary.

/// How many paths a description names before saying `and N more`.
const NAMED_PATHS: usize = 3;

/// `edited a.tex, b.tex, c.tex and 2 more`: the files found changed, sorted.
#[must_use]
pub fn edited(paths: &[String]) -> String {
    let mut sorted: Vec<&str> = paths.iter().map(String::as_str).collect();
    sorted.sort_unstable();
    format!("edited {}", path_list(&sorted))
}

/// `restore a.tex from last~1`: the point in paper's spelling; the whole project is `everything`.
#[must_use]
pub fn restored(paths: &[String], from: &str) -> String {
    let named: Vec<&str> = paths.iter().map(String::as_str).collect();
    // The project's root is the empty path.
    let what = if named.iter().any(|p| p.is_empty()) {
        "everything".to_owned()
    } else {
        path_list(&named)
    };
    format!("restore {what} from {from}")
}

/// `save "Rewrite related work"`: the first line of the message, so one entry stays one line.
#[must_use]
pub fn saved(message: &str) -> String {
    let subject = message.lines().next().unwrap_or_default().trim();
    format!("save \"{subject}\"")
}

/// `ignore /data/raw/, *.aux`: the rules the command named, in the order given.
#[must_use]
pub fn ignored(rules: &[String]) -> String {
    let named: Vec<&str> = rules.iter().map(String::as_str).collect();
    format!("ignore {}", path_list(&named))
}

fn path_list(paths: &[&str]) -> String {
    let named = paths[..paths.len().min(NAMED_PATHS)].join(", ");
    match paths.len().checked_sub(NAMED_PATHS) {
        Some(more) if more > 0 => format!("{named} and {more} more"),
        _ => named,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(names: &[&str]) -> Vec<String> {
        names.iter().map(|n| (*n).to_owned()).collect()
    }

    #[test]
    fn edited_sorts_and_caps_the_list() {
        assert_eq!(edited(&paths(&["b", "a"])), "edited a, b");
        assert_eq!(
            edited(&paths(&["d", "c", "b", "a"])),
            "edited a, b, c and 1 more"
        );
    }

    #[test]
    fn restoring_the_root_says_everything() {
        assert_eq!(
            restored(&paths(&[""]), "last"),
            "restore everything from last"
        );
    }

    #[test]
    fn the_root_among_other_paths_still_says_everything() {
        assert_eq!(
            restored(&paths(&["", "a.tex"]), "last"),
            "restore everything from last"
        );
    }

    #[test]
    fn ignoring_names_the_rules_in_order_and_caps_the_list() {
        assert_eq!(ignored(&paths(&["/b/", "*.aux"])), "ignore /b/, *.aux");
        assert_eq!(
            ignored(&paths(&["d", "c", "b", "a"])),
            "ignore d, c, b and 1 more"
        );
    }

    #[test]
    fn a_save_names_its_first_line_only() {
        assert_eq!(saved("Fix \"it\"\n\nlong body"), "save \"Fix \"it\"\"");
        assert_eq!(saved("  padded \r\nbody"), "save \"padded\"");
    }

    #[test]
    fn restored_keeps_the_given_order() {
        assert_eq!(
            restored(&paths(&["z", "a"]), "last~1"),
            "restore z, a from last~1"
        );
    }
}

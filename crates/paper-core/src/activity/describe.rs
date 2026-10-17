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

/// `restore a.tex from last~1`: the point is spelled as the researcher gave it.
#[must_use]
pub fn restored(paths: &[String], from: &str) -> String {
    let named: Vec<&str> = paths.iter().map(String::as_str).collect();
    format!("restore {} from {from}", path_list(&named))
}

/// `save "Rewrite related work"`.
#[must_use]
pub fn saved(message: &str) -> String {
    format!("save \"{message}\"")
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
    fn restored_keeps_the_given_order() {
        assert_eq!(
            restored(&paths(&["z", "a"]), "last~1"),
            "restore z, a from last~1"
        );
    }
}

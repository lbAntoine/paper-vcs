//! Paths typed on the command line, relative to where paper is run, turned into the
//! repository-relative `/`-separated paths `paper-core` expects.

use std::path::{Component, Path, PathBuf};

use paper_core::{ErrorKind, PaperError, Suggestion};

/// Where the researcher is, relative to the project folder.
#[derive(Debug, Clone)]
pub struct Paths {
    workdir: PathBuf,
    /// The components of the start folder below the project folder (empty at the top).
    base: Vec<String>,
}

impl Paths {
    /// `start` is the folder paper runs in (`-C` or the current folder), inside `workdir`.
    pub fn new(start: &Path, workdir: &Path) -> Self {
        let base = below(workdir, start).unwrap_or_default();
        Self {
            workdir: workdir.to_owned(),
            base,
        }
    }

    /// `given` as a path from the project folder: `/` separators, no `.` components. An empty
    /// string means the project folder itself.
    ///
    /// # Errors
    ///
    /// A user error if the path leads outside the project, or (on Windows) is root-relative
    /// (`\a`) or drive-relative (`C:a`).
    pub fn project_path(&self, given: &str) -> Result<String, PaperError> {
        if cfg!(windows) && is_root_or_drive_relative(given) {
            return Err(rooted_path_error(given));
        }
        let normalised = if cfg!(windows) {
            given.replace('\\', "/")
        } else {
            given.to_owned()
        };
        let path = Path::new(&normalised);
        if path.is_absolute() {
            let parts = below(&self.workdir, path).ok_or_else(|| self.outside(given))?;
            return Ok(parts.join("/"));
        }
        let mut parts = self.base.clone();
        for component in path.components() {
            match component {
                Component::Normal(part) => parts.push(part.to_string_lossy().into_owned()),
                Component::ParentDir => {
                    parts.pop().ok_or_else(|| self.outside(given))?;
                }
                Component::CurDir | Component::RootDir | Component::Prefix(_) => {}
            }
        }
        Ok(parts.join("/"))
    }

    fn outside(&self, given: &str) -> PaperError {
        PaperError {
            kind: ErrorKind::User,
            message: format!("{given} is outside this project"),
            why: format!(
                "Paper only works with files inside the project folder, {}.",
                self.workdir.display()
            ),
            next: vec![Suggestion {
                command: "paper -C <project folder> <command>".to_owned(),
                explanation: "run paper on the project, and name files inside it".to_owned(),
            }],
        }
    }
}

/// True for the Windows forms that name neither a place under this folder nor a full path:
/// root-relative (`\a`, no drive) and drive-relative (`C:a`, no separator after the colon).
/// `C:\a`, `C:/a`, `\\server\share` and plain relative paths are false.
pub fn is_root_or_drive_relative(given: &str) -> bool {
    let bytes = given.as_bytes();
    let separator = |b: u8| b == b'\\' || b == b'/';
    // `\a` (but not the `\\server\share` form).
    if bytes.first().copied().is_some_and(separator) {
        return !bytes.get(1).copied().is_some_and(separator);
    }
    // `C:a` (but not `C:`, `C:\a` or `C:/a`).
    bytes.len() > 2 && bytes[0].is_ascii_alphabetic() && bytes[1] == b':' && !separator(bytes[2])
}

/// The user error for a root-relative or drive-relative path: tells the researcher to use a
/// path relative to this folder or a full path.
pub fn rooted_path_error(given: &str) -> PaperError {
    PaperError {
        kind: ErrorKind::User,
        message: format!("{given} does not say where the file is"),
        why: "It starts from the root of a drive, or names a drive without a folder, so paper \
              can't tell which folder it is in."
            .to_owned(),
        next: vec![Suggestion {
            command: "paper <command> <path>".to_owned(),
            explanation: "use a path relative to this folder or a full path".to_owned(),
        }],
    }
}

/// The components of `path` below `root`, comparing as written first and then with symlinks
/// resolved (a temp folder can be spelled two ways).
fn below(root: &Path, path: &Path) -> Option<Vec<String>> {
    let strip = |root: &Path, path: &Path| {
        path.strip_prefix(root).ok().map(|rest| {
            rest.components()
                .filter_map(|c| match c {
                    Component::Normal(part) => Some(part.to_string_lossy().into_owned()),
                    _ => None,
                })
                .collect()
        })
    };
    strip(root, path).or_else(|| {
        let root = dunce::canonicalize(root).ok()?;
        let path = dunce::canonicalize(path).ok()?;
        strip(&root, &path)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(base: &[&str]) -> Paths {
        Paths {
            workdir: PathBuf::from("/work"),
            base: base.iter().map(|s| (*s).to_owned()).collect(),
        }
    }

    #[test]
    fn a_path_is_relative_to_where_paper_runs() {
        assert_eq!(
            paths(&["sections"]).project_path("intro.tex").unwrap(),
            "sections/intro.tex"
        );
    }

    #[test]
    fn dot_components_are_dropped() {
        assert_eq!(
            paths(&["sections"]).project_path("./intro.tex").unwrap(),
            "sections/intro.tex"
        );
        assert_eq!(paths(&[]).project_path("./a/./b").unwrap(), "a/b");
    }

    #[test]
    fn dot_dot_climbs_out_of_the_start_folder() {
        assert_eq!(
            paths(&["sections"]).project_path("../main.tex").unwrap(),
            "main.tex"
        );
    }

    #[test]
    fn dot_is_the_whole_folder() {
        assert_eq!(paths(&[]).project_path(".").unwrap(), "");
        assert_eq!(paths(&["sections"]).project_path(".").unwrap(), "sections");
    }

    #[test]
    fn a_path_above_the_project_is_a_user_error() {
        let error = paths(&["sections"]).project_path("../../x").unwrap_err();
        assert_eq!(error.kind, ErrorKind::User);
        assert!(error.message.contains("../../x is outside this project"));
        assert!(!error.next.is_empty());
    }

    #[cfg(unix)]
    #[test]
    fn an_absolute_path_inside_the_project_is_made_relative() {
        assert_eq!(
            paths(&[]).project_path("/work/sections/a.tex").unwrap(),
            "sections/a.tex"
        );
        assert!(paths(&[]).project_path("/elsewhere/a.tex").is_err());
    }

    #[cfg(windows)]
    #[test]
    fn backslashes_are_separators_on_windows() {
        assert_eq!(
            paths(&[]).project_path(".\\sections\\a.tex").unwrap(),
            "sections/a.tex"
        );
    }

    #[test]
    fn root_relative_and_drive_relative_forms_are_recognised() {
        for given in ["\\a", "\\a\\b.tex", "C:a", "c:sections\\a.tex", "C:a/b"] {
            assert!(is_root_or_drive_relative(given), "{given}");
        }
    }

    #[test]
    fn ordinary_and_full_paths_are_not_root_or_drive_relative() {
        for given in [
            "a.tex",
            "./a.tex",
            "../a.tex",
            "sections/a.tex",
            "C:\\a",
            "C:/a",
            "C:",
            "\\\\server\\share\\a",
            "",
            ".",
        ] {
            assert!(!is_root_or_drive_relative(given), "{given}");
        }
    }

    #[test]
    fn a_root_or_drive_relative_path_is_a_three_part_user_error() {
        let error = rooted_path_error("\\a");
        assert_eq!(error.kind, ErrorKind::User);
        assert!(error.message.contains("\\a"), "{error:?}");
        assert!(!error.why.is_empty() && !error.next.is_empty(), "{error:?}");
        let text = format!("{} {} {:?}", error.message, error.why, error.next);
        assert!(
            text.contains("use a path relative to this folder or a full path"),
            "{error:?}"
        );
    }

    #[cfg(windows)]
    #[test]
    fn project_path_refuses_root_relative_and_drive_relative_paths() {
        for given in ["\\a", "C:a"] {
            let error = paths(&[]).project_path(given).unwrap_err();
            assert_eq!(error.kind, ErrorKind::User, "{given}");
            let text = format!("{} {} {:?}", error.message, error.why, error.next);
            assert!(
                text.contains("use a path relative to this folder or a full path"),
                "{given}: {error:?}"
            );
        }
    }
}

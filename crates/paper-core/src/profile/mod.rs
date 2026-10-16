//! Built-in profiles: what a kind of project (LaTeX, Markdown, ...) keeps out of saves, how it
//! is detected, and what tips it gives (04-content.md § Profiles).
//!
//! Pure data and string functions: nothing here prints, prompts or touches git.

pub mod gitignore;

/// A built-in profile, read from `profiles/<name>.toml`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Profile {
    /// `latex`, `markdown`, `typst`, `quarto`, `jupyter` or `generic`.
    pub name: String,
    /// Glob patterns or file names that signal this profile, e.g. `*.tex`.
    pub detect: Vec<String>,
    /// Lines for the `.gitignore` block, shared lines included.
    pub ignore: Vec<String>,
    /// Short advice shown by `paper init`.
    pub tips: Vec<String>,
}

impl Profile {
    /// Every built-in profile: latex, markdown, typst, quarto, jupyter, generic.
    ///
    /// # Panics
    ///
    /// Never (the embedded files are checked by tests).
    pub fn all() -> &'static [Profile] {
        todo!("PV-004: embedded profiles")
    }

    /// The built-in profile called `name`, if there is one.
    pub fn builtin(_name: &str) -> Option<&'static Profile> {
        todo!("PV-004: embedded profiles")
    }
}

/// Picks the profile for a project from the paths (relative, `/`-separated) of the files in it.
///
/// First match in this order: `quarto` (`_quarto.yml`), `latex` (`*.tex`), `typst` (`*.typ`),
/// `jupyter` (`*.ipynb`), `markdown` (`*.md`), else `generic`. Files match by name, in any folder.
///
/// # Panics
///
/// Never.
pub fn detect<S: AsRef<str>>(_files: &[S]) -> &'static Profile {
    todo!("PV-004: detection")
}

#[cfg(test)]
mod tests {
    use super::*;

    const NAMES: [&str; 6] = ["latex", "markdown", "typst", "quarto", "jupyter", "generic"];

    fn builtin(name: &str) -> &'static Profile {
        Profile::builtin(name).unwrap_or_else(|| panic!("no profile {name}"))
    }

    #[test]
    fn there_are_six_profiles_with_unique_names() {
        let names: Vec<&str> = Profile::all().iter().map(|p| p.name.as_str()).collect();
        let mut sorted = names.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), 6, "{names:?}");
        for name in NAMES {
            assert!(names.contains(&name), "{name} missing from {names:?}");
        }
    }

    #[test]
    fn builtin_finds_each_profile_and_refuses_unknown_names() {
        for name in NAMES {
            assert_eq!(builtin(name).name, name);
        }
        assert!(Profile::builtin("word").is_none());
    }

    #[test]
    fn every_profile_has_the_shared_ignore_lines() {
        let shared = [
            ".DS_Store",
            "Thumbs.db",
            ".venv/",
            "__pycache__/",
            ".Rproj.user/",
            "renv/library/",
            "~$*",
            ".~lock.*#",
            "~WRL*.tmp",
        ];
        for profile in Profile::all() {
            for line in shared {
                assert!(
                    profile.ignore.iter().any(|l| l == line),
                    "{} lacks {line}: {:?}",
                    profile.name,
                    profile.ignore
                );
            }
        }
    }

    #[test]
    fn latex_ignores_build_junk_but_not_the_pdf() {
        let latex = builtin("latex");
        for line in ["*.aux", "*.log", "*.synctex.gz", "_minted-*/", "build/"] {
            assert!(latex.ignore.iter().any(|l| l == line), "{line}");
        }
        assert!(!latex.ignore.iter().any(|l| l.contains("pdf")), "pdf kept");
        assert_eq!(latex.detect, ["*.tex"]);
        assert!(!latex.tips.is_empty());
    }

    #[test]
    fn the_other_profiles_ignore_what_the_table_says() {
        let expect = [
            ("markdown", vec!["_site/", ".cache/"], "*.md"),
            (
                "quarto",
                vec!["_freeze/", "_site/", "*_files/", ".quarto/"],
                "_quarto.yml",
            ),
            ("jupyter", vec![".ipynb_checkpoints/"], "*.ipynb"),
            ("typst", vec![], "*.typ"),
        ];
        for (name, lines, pattern) in expect {
            let profile = builtin(name);
            for line in lines {
                assert!(profile.ignore.iter().any(|l| l == line), "{name}: {line}");
            }
            assert_eq!(profile.detect, [pattern], "{name}");
        }
        assert!(builtin("generic").detect.is_empty());
    }

    #[test]
    fn ignore_lines_are_non_empty_and_unique() {
        for profile in Profile::all() {
            let mut seen = std::collections::HashSet::new();
            for line in &profile.ignore {
                assert!(!line.trim().is_empty(), "{}", profile.name);
                assert!(seen.insert(line), "{} repeats {line}", profile.name);
            }
        }
    }

    #[test]
    fn detection_follows_the_documented_order() {
        let cases: [(&[&str], &str); 8] = [
            (
                &["_quarto.yml", "a.tex", "b.typ", "c.ipynb", "d.md"],
                "quarto",
            ),
            (&["a.tex", "b.typ", "c.ipynb", "d.md"], "latex"),
            (&["b.typ", "c.ipynb", "d.md"], "typst"),
            (&["c.ipynb", "d.md"], "jupyter"),
            (&["d.md", "data.csv"], "markdown"),
            (&["data.csv", "notes.txt"], "generic"),
            (&[], "generic"),
            (&["sections/intro.tex", "refs/_quarto.yml"], "quarto"),
        ];
        for (files, expected) in cases {
            assert_eq!(detect(files).name, expected, "{files:?}");
        }
    }

    #[test]
    fn detection_matches_names_in_subfolders() {
        assert_eq!(detect(&["chapters/one/intro.tex"]).name, "latex");
        assert_eq!(detect(&["docs/readme.md".to_owned()]).name, "markdown");
    }

    #[test]
    fn detection_does_not_match_look_alikes() {
        assert_eq!(
            detect(&["texture.txt", "notes.markdown.bak"]).name,
            "generic"
        );
    }
}

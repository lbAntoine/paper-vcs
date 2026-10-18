//! Classifying a command-line argument as a name or a pattern, and the rule it stands for.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use super::{GITIGNORE, IgnoreError};

/// Gitignore metacharacters that make an argument a pattern rather than a name.
const GLOB_CHARS: [char; 3] = ['*', '?', '['];

/// Files `paper ignore` never takes out of the draft, whatever the rules match: paper's settings
/// and every `.gitignore` (without them the rules would be lost).
pub(super) fn is_protected_file(path: &str) -> bool {
    path == "paper.toml" || path == GITIGNORE || path.ends_with("/.gitignore")
}

/// One command-line argument, checked.
pub(super) enum Arg {
    /// Has no gitignore metacharacters (or does, and may still name a file): a name.
    Name {
        parts: Vec<String>,
        typed_slash: bool,
        typed: String,
    },
    /// Has `*`, `?` or `[`: a pattern unless a file of that name exists.
    Maybe {
        parts: Option<Vec<String>>,
        typed: String,
    },
}

impl Arg {
    pub(super) fn parse(arg: &str, from: &str, workdir: &Path) -> Result<Self, IgnoreError> {
        if arg.is_empty() {
            return Err(IgnoreError::Empty);
        }
        let parts = resolve(arg, from, workdir);
        // A leading '!' would unignore, unless it is just the first character of a file name.
        if arg.starts_with('!')
            && !parts.as_ref().is_ok_and(|parts| {
                !parts.is_empty() && fs::symlink_metadata(workdir.join(parts.join("/"))).is_ok()
            })
        {
            return Err(IgnoreError::Unignore {
                arg: arg.to_owned(),
            });
        }
        if arg.contains(GLOB_CHARS) {
            // A pattern written as typed must not be dead on arrival: above the project, or
            // inside .git (gitignore-style `/data/*.csv` is not a path, so it is left alone).
            let parts = match parts {
                Err(error) if !Path::new(arg).is_absolute() => return Err(error),
                other => other.ok(),
            };
            if parts
                .as_ref()
                .and_then(|p| p.first())
                .is_some_and(|first| first.eq_ignore_ascii_case(".git"))
            {
                return Err(IgnoreError::InsideGit {
                    arg: arg.to_owned(),
                });
            }
            return Ok(Self::Maybe {
                parts,
                typed: arg.to_owned(),
            });
        }
        let parts = parts?;
        match parts.as_slice() {
            [] => return Err(IgnoreError::WholeProject),
            [first, ..] if first.eq_ignore_ascii_case(".git") => {
                return Err(IgnoreError::InsideGit {
                    arg: arg.to_owned(),
                });
            }
            [only] if only == GITIGNORE || only == "paper.toml" => {
                return Err(IgnoreError::Protected { path: only.clone() });
            }
            _ => {}
        }
        Ok(Self::Name {
            typed: arg.to_owned(),
            parts,
            typed_slash: arg.ends_with('/') || (cfg!(windows) && arg.ends_with('\\')),
        })
    }

    /// The rule this argument stands for, and whether it is a pattern.
    pub(super) fn rule(
        &self,
        workdir: &Path,
        draft: &BTreeMap<String, paper_git::TreeFile>,
    ) -> (String, bool) {
        match self {
            Self::Name {
                parts,
                typed_slash,
                typed,
            } => {
                let mut bare = false;
                let rule = match exists(workdir, parts, draft) {
                    Some(is_dir) => path_rule(&on_disk_spelling(workdir, parts), is_dir),
                    // Not there yet: a bare name matches at any depth, as it does in
                    // .gitignore; with a slash in it, it is anchored.
                    None if parts.len() == 1 && !*typed_slash && !typed.contains('/') => {
                        bare = true;
                        // A leading '#' would start a comment.
                        let name = escape(&parts[0]);
                        if name.starts_with('#') {
                            format!("\\{name}")
                        } else {
                            name
                        }
                    }
                    None => path_rule(parts, *typed_slash),
                };
                (rule, bare)
            }
            Self::Maybe { parts, typed } => {
                if let Some(parts) = parts.as_ref().filter(|p| !p.is_empty())
                    && let Some(is_dir) = exists(workdir, parts, draft)
                {
                    return (path_rule(&on_disk_spelling(workdir, parts), is_dir), false);
                }
                (typed.clone(), true)
            }
        }
    }
}

/// `parts` spelled as the folders list them: on a filesystem that ignores case, `DATA/raw`
/// finds `data/raw`, and the rule must use the real spelling to match anything. A part that is
/// not found keeps the spelling typed.
fn on_disk_spelling(workdir: &Path, parts: &[String]) -> Vec<String> {
    let mut dir = workdir.to_owned();
    let mut out = Vec::with_capacity(parts.len());
    for part in parts {
        let found = fs::read_dir(&dir).ok().and_then(|entries| {
            let names: Vec<String> = entries
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.iter().find(|n| *n == part).cloned().or_else(|| {
                names
                    .into_iter()
                    .find(|n| n.to_lowercase() == part.to_lowercase())
            })
        });
        let name = found.unwrap_or_else(|| part.clone());
        dir.push(&name);
        out.push(name);
    }
    out
}

/// The path and kind (folder or not) a rule written by [`path_rule`] stands for, undoing its
/// escapes. `None` for anything else: a pattern, or a bare name that matches at any depth.
pub(super) fn rule_target(rule: &str) -> Option<(String, bool)> {
    let body = rule.strip_prefix('/')?;
    let (body, folder) = body.strip_suffix('/').map_or((body, false), |b| (b, true));
    let mut out = String::with_capacity(body.len());
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            out.push(chars.next()?);
        } else if matches!(c, '*' | '?' | '[') {
            return None;
        } else {
            out.push(c);
        }
    }
    (!out.is_empty()).then_some((out, folder))
}

/// `/a/b/` for a folder, `/a/b` for a file, with metacharacters escaped.
fn path_rule(parts: &[String], folder: bool) -> String {
    let joined = parts
        .iter()
        .map(|p| escape(p))
        .collect::<Vec<_>>()
        .join("/");
    if folder {
        format!("/{joined}/")
    } else {
        format!("/{joined}")
    }
}

/// Backslash-escapes what gitignore reads as syntax inside a name. A leading `!` or `#` needs
/// no escape: every rule starts with the anchoring `/`.
fn escape(name: &str) -> String {
    let body = name.trim_end_matches(' ');
    let mut out = String::with_capacity(name.len());
    for c in body.chars() {
        if matches!(c, '\\' | '*' | '?' | '[' | ']') {
            out.push('\\');
        }
        out.push(c);
    }
    for _ in 0..name.len() - body.len() {
        out.push_str("\\ ");
    }
    out
}

/// Whether the path exists (on disk, or in the draft), and if so whether it is a folder.
fn exists(
    workdir: &Path,
    parts: &[String],
    draft: &BTreeMap<String, paper_git::TreeFile>,
) -> Option<bool> {
    let joined = parts.join("/");
    if let Ok(meta) = fs::symlink_metadata(workdir.join(&joined)) {
        return Some(meta.is_dir());
    }
    if draft.contains_key(&joined) {
        return Some(false);
    }
    let prefix = format!("{joined}/");
    draft
        .range(prefix.clone()..)
        .next()
        .is_some_and(|(path, _)| path.starts_with(&prefix))
        .then_some(true)
}

/// `arg` typed in `from`, as components relative to the project folder.
pub(super) fn resolve(arg: &str, from: &str, workdir: &Path) -> Result<Vec<String>, IgnoreError> {
    let outside = || IgnoreError::Outside {
        arg: arg.to_owned(),
    };
    let typed = if cfg!(windows) {
        arg.replace('\\', "/")
    } else {
        arg.to_owned()
    };
    let (mut parts, rest): (Vec<String>, String) = if Path::new(&typed).is_absolute() {
        let inside = Path::new(&typed)
            .strip_prefix(workdir)
            .map_err(|_| outside())?;
        (Vec::new(), inside.to_string_lossy().replace('\\', "/"))
    } else {
        (
            from.split('/')
                .filter(|part| !part.is_empty())
                .map(str::to_owned)
                .collect(),
            typed,
        )
    };
    for part in rest.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                parts.pop().ok_or_else(outside)?;
            }
            other => parts.push(other.to_owned()),
        }
    }
    Ok(parts)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_escaped_where_gitignore_reads_syntax() {
        assert_eq!(escape("a[1].log"), r"a\[1\].log");
        assert_eq!(escape("a*b?c"), r"a\*b\?c");
        assert_eq!(escape(r"back\slash"), r"back\\slash");
        assert_eq!(escape("plain.txt"), "plain.txt");
        assert_eq!(escape("end  "), r"end\ \ ");
    }

    #[test]
    fn a_path_rule_gives_its_target_back() {
        assert_eq!(rule_target("/a/b/"), Some(("a/b".to_owned(), true)));
        assert_eq!(rule_target("/a/b.txt"), Some(("a/b.txt".to_owned(), false)));
        assert_eq!(
            rule_target(r"/a\[1\].log"),
            Some(("a[1].log".to_owned(), false))
        );
        assert_eq!(rule_target("*.aux"), None);
        assert_eq!(rule_target("draft.tmp"), None);
        assert_eq!(rule_target("/data/*.csv"), None);
        assert_eq!(rule_target("/"), None);
    }

    #[test]
    fn a_path_is_resolved_from_where_paper_runs() {
        let root = Path::new("/project");
        let parts = |arg, from| resolve(arg, from, root).unwrap();
        assert_eq!(parts("raw", "data"), ["data", "raw"]);
        assert_eq!(parts("../notes.txt", "data"), ["notes.txt"]);
        assert_eq!(parts("./a//b/", ""), ["a", "b"]);
        assert!(parts(".", "").is_empty());
        assert!(resolve("../x", "", root).is_err());
        assert!(resolve("../../x", "data", root).is_err());
    }

    #[test]
    fn protected_files_are_never_untracked() {
        for path in [".gitignore", "docs/.gitignore", "paper.toml"] {
            assert!(is_protected_file(path), "{path}");
        }
        assert!(!is_protected_file("docs/paper.toml"));
        assert!(!is_protected_file("notes.gitignore"));
    }
}

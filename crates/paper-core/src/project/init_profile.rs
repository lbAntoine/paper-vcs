//! The profile step of `paper init`: pick the profile, then work out (without writing) the
//! `.gitignore` block and the `paper.toml` line, so a problem leaves the folder untouched.

use std::path::Path;

use super::init::{InitError, ProfileSource};
use crate::config::{ConfigFile, Key, Value};
use crate::fs_util::write_atomic;
use crate::profile::{Profile, detect, gitignore};
use crate::snapshot::visible_files;

const PROJECT_FILE: &str = "paper.toml";
const GITIGNORE_FILE: &str = ".gitignore";

/// The profile in effect and the file contents `init` is going to write.
#[derive(Debug)]
pub(super) struct ProfilePlan {
    profile: Profile,
    source: ProfileSource,
    gitignore: Option<String>,
    paper_toml: Option<String>,
}

/// What the profile step did, for the init result.
#[derive(Debug)]
pub(super) struct ProfileReport {
    pub profile: String,
    pub profile_source: ProfileSource,
    pub written: Vec<String>,
    pub tips: Vec<String>,
}

impl ProfilePlan {
    /// Decides the profile for the folder at `workdir` and prepares both files; writes nothing.
    pub(super) fn prepare(workdir: &Path, choice: Option<&Profile>) -> Result<Self, InitError> {
        let config = ConfigFile::load(&workdir.join(PROJECT_FILE))?;
        let existing = match config.get(Key::Profile)? {
            Some(Value::Text(name)) => Profile::builtin(&name),
            _ => None,
        };
        let (profile, source) = match (existing, choice) {
            (Some(existing), Some(requested)) if existing.name != requested.name => {
                return Err(InitError::ProfileConflict {
                    requested: requested.name.clone(),
                    existing: existing.name.clone(),
                });
            }
            (Some(existing), None) => (existing.clone(), ProfileSource::PaperToml),
            (_, Some(requested)) => (requested.clone(), ProfileSource::Chosen),
            (None, None) => {
                let files = visible_files(workdir)?;
                (detect(&files).clone(), ProfileSource::Detected)
            }
        };
        let existing_gitignore = gitignore::read(workdir)?;
        let gitignore = gitignore::apply(existing_gitignore.as_deref(), &profile)?;
        let paper_toml = if existing.is_some() {
            None
        } else {
            let updated = config.with(Key::Profile, Value::Text(profile.name.clone()))?;
            Some(updated.render())
        };
        Ok(Self {
            profile,
            source,
            gitignore,
            paper_toml,
        })
    }

    /// Prepares the plan again from the files as they are now. `init` calls it once it holds
    /// the project lock, so a change made to `paper.toml` or `.gitignore` between the first
    /// `prepare` and the lock (a concurrent `paper config set --project`) is kept, not
    /// overwritten by a stale plan.
    ///
    /// # Errors
    ///
    /// Everything [`ProfilePlan::prepare`] reports.
    // Called from `adopt` and `init_with` once implemented (PV-020 GREEN); then drop the allow.
    #[allow(dead_code)]
    pub(super) fn reprepare(
        self,
        _workdir: &Path,
        _choice: Option<&Profile>,
    ) -> Result<Self, InitError> {
        todo!("PV-020: re-run prepare under the lock")
    }

    /// Writes the prepared files (`.gitignore` first) and reports what was written.
    pub(super) fn write(self, workdir: &Path) -> Result<ProfileReport, InitError> {
        let mut written = Vec::new();
        for (name, text) in [
            (GITIGNORE_FILE, &self.gitignore),
            (PROJECT_FILE, &self.paper_toml),
        ] {
            if let Some(text) = text {
                let path = workdir.join(name);
                write_atomic(&path, text.as_bytes())
                    .map_err(|source| InitError::Io { path, source })?;
                written.push(name.to_owned());
            }
        }
        Ok(ProfileReport {
            profile: self.profile.name.clone(),
            profile_source: self.source,
            written,
            tips: self.profile.tips.clone(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn reprepare_keeps_a_paper_toml_change_made_after_the_first_prepare() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.md"), "hello\n").unwrap();
        let first = ProfilePlan::prepare(dir.path(), None).unwrap();
        // A concurrent `paper config set --project large_files.warn_mb 7` lands before the lock.
        fs::write(
            dir.path().join(PROJECT_FILE),
            "[large_files]\nwarn_mb = 7\n",
        )
        .unwrap();

        let plan = first.reprepare(dir.path(), None).unwrap();
        plan.write(dir.path()).unwrap();

        let config = ConfigFile::load(&dir.path().join(PROJECT_FILE)).unwrap();
        assert_eq!(
            config.get(Key::LargeFilesWarnMb).unwrap(),
            Some(Value::Megabytes(7))
        );
        assert!(matches!(
            config.get(Key::Profile).unwrap(),
            Some(Value::Text(_))
        ));
    }

    #[test]
    fn reprepare_honours_a_profile_written_after_the_first_prepare() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("notes.md"), "hello\n").unwrap();
        let first = ProfilePlan::prepare(dir.path(), None).unwrap();
        fs::write(dir.path().join(PROJECT_FILE), "profile = \"latex\"\n").unwrap();

        let plan = first.reprepare(dir.path(), None).unwrap();
        let report = plan.write(dir.path()).unwrap();

        assert_eq!(report.profile, "latex");
        assert_eq!(report.profile_source, ProfileSource::PaperToml);
        let toml = fs::read_to_string(dir.path().join(PROJECT_FILE)).unwrap();
        assert_eq!(toml, "profile = \"latex\"\n");
    }
}

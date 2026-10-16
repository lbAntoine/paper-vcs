//! The profile step of `paper init`: pick the profile, then work out (without writing) the
//! `.gitignore` block and the `paper.toml` line, so a problem leaves the folder untouched.

use std::path::Path;

use super::init::InitError;
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
    detected: bool,
    from_paper_toml: bool,
    gitignore: Option<String>,
    paper_toml: Option<String>,
}

/// What the profile step did, for the init result.
#[derive(Debug)]
pub(super) struct ProfileReport {
    pub profile: String,
    pub detected: bool,
    pub from_paper_toml: bool,
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
        let (profile, detected, from_paper_toml) = match (existing, choice) {
            (Some(existing), Some(requested)) if existing.name != requested.name => {
                return Err(InitError::ProfileConflict {
                    requested: requested.name.clone(),
                    existing: existing.name.clone(),
                });
            }
            (Some(existing), None) => (existing.clone(), false, true),
            (_, Some(requested)) => (requested.clone(), false, false),
            (None, None) => {
                let files = visible_files(workdir)?;
                (detect(&files).clone(), true, false)
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
            detected,
            from_paper_toml,
            gitignore,
            paper_toml,
        })
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
            detected: self.detected,
            from_paper_toml: self.from_paper_toml,
            written,
            tips: self.profile.tips.clone(),
        })
    }
}

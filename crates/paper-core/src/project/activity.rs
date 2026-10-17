//! Listing the operation log for `paper activity`.

use super::Project;
use crate::activity::{ActivityError, ActivityLog, OpId, OpKind, short_ids_among};

/// One line of `paper activity`: an operation, as the researcher sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActivityEntry {
    pub id: OpId,
    /// The shortest prefix of `id` that no other op in the log starts with (listed or not),
    /// at least 4 characters.
    pub short_id: String,
    /// RFC 3339, as stored in the op.
    pub time: String,
    pub kind: OpKind,
    /// In researcher vocabulary, as the command recorded it.
    pub description: String,
}

impl Project {
    /// The newest `limit` entries of the activity log, newest first: `head`, then back along
    /// `parent`. Reads only the ops on that chain, never the other files in `ops/`. Does not
    /// snapshot and does not record anything.
    ///
    /// # Errors
    ///
    /// [`ActivityError`] if `head` or an op on the chain is missing or damaged.
    ///
    /// # Panics
    ///
    /// Never.
    pub fn activity(&self, limit: usize) -> Result<Vec<ActivityEntry>, ActivityError> {
        let log = ActivityLog::open(&self.paper_dir);
        let mut next = log.head()?.ok_or_else(|| ActivityError::MissingHead {
            path: log.root().join("head"),
        })?;
        let mut ops = Vec::new();
        while ops.len() < limit {
            let op = log.read(&next)?;
            let parent = op.parent.clone();
            ops.push(op);
            match parent {
                Some(parent) => next = parent,
                None => break,
            }
        }
        let ids: Vec<OpId> = ops.iter().map(|op| op.id.clone()).collect();
        Ok(ops
            .into_iter()
            .zip(short_ids_among(&ids, &log.op_file_names()?))
            .map(|(op, short_id)| ActivityEntry {
                id: op.id,
                short_id,
                time: op.time,
                kind: op.kind,
                description: op.description,
            })
            .collect())
    }
}

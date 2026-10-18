//! Core domain model and commands for paper-vcs.
//!
//! This crate never prints or prompts: every command returns data that a front-end renders.
pub mod activity;
pub mod config;
pub mod error;
pub mod fault;
mod fs_util;
pub mod name;
pub mod point;
pub mod profile;
pub mod project;
pub mod snapshot;
pub mod tree_diff;
pub mod view;

pub use error::{ErrorKind, PaperError, Suggestion};
pub use paper_git::{ChangeId, CommitId};

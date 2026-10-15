//! Core domain model and commands for paper-vcs.
//!
//! This crate never prints or prompts: every command returns data that a front-end renders.
mod fs_util;
pub mod name;
pub mod point;
pub mod project;
pub mod snapshot;
pub mod view;

pub use paper_git::{ChangeId, CommitId};

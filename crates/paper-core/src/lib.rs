//! Core domain model and commands for paper-vcs.
//!
//! This crate never prints or prompts: every command returns data that a front-end renders.
pub mod name;
pub mod point;
pub mod view;

pub use paper_git::{ChangeId, CommitId};

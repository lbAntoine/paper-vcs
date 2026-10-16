//! Rendering of errors (three parts) and the mapping to exit codes.
// RED stub: unused until GREEN wires it into `run`.
#![allow(dead_code)]

use paper_core::ErrorKind;

/// The process exit code for an error of this kind: 1, 2 or 3.
pub fn exit_code(_kind: ErrorKind) -> u8 {
    todo!("PV-003")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_kinds_map_to_the_documented_exit_codes() {
        assert_eq!(exit_code(ErrorKind::User), 1);
        assert_eq!(exit_code(ErrorKind::NeedsAttention), 2);
        assert_eq!(exit_code(ErrorKind::Project), 3);
    }
}

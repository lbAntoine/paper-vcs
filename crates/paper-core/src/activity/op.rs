//! One operation and its JSON form.

use serde::{Deserialize, Serialize};

use super::OpId;
use crate::view::View;

/// What an operation did. Serialised in lowercase (`"init"`, `"save"`, ...).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OpKind {
    Init,
    Migrate,
    Snapshot,
    Save,
    Restore,
}

/// One immutable operation: the full view after it, and the undo bookkeeping.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Op {
    pub id: OpId,
    /// `None` (JSON `null`) for the first op.
    pub parent: Option<OpId>,
    pub kind: OpKind,
    /// RFC 3339.
    pub time: String,
    pub view: View,
    /// The op whose state is current: the op itself for a normal op.
    pub position: OpId,
    /// The redo stack, as op ids. Empty for normal ops.
    pub redo: Vec<OpId>,
    pub description: String,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::name::LineName;
    use crate::view::Line;

    fn view() -> View {
        let commit = |c: char| c.to_string().repeat(40).parse().unwrap();
        View::new(
            LineName::main(),
            Line {
                tip: commit('a'),
                draft: commit('b'),
            },
        )
        .with_line(
            "try/intro".parse().unwrap(),
            Line {
                tip: commit('c'),
                draft: commit('d'),
            },
        )
    }

    fn op() -> Op {
        let id = OpId::random();
        Op {
            id: id.clone(),
            parent: Some(OpId::random()),
            kind: OpKind::Save,
            time: "2026-10-17T09:30:00Z".to_owned(),
            view: view(),
            position: id,
            redo: vec![OpId::random()],
            description: "save".to_owned(),
        }
    }

    #[test]
    fn an_op_survives_a_round_trip_through_json_with_its_view() {
        let original = op();
        let json = serde_json::to_vec_pretty(&original).unwrap();
        let back: Op = serde_json::from_slice(&json).unwrap();
        assert_eq!(back, original);
        assert_eq!(back.view, view());
    }

    #[test]
    fn the_json_field_names_are_stable() {
        let json = serde_json::to_value(Op {
            parent: None,
            ..op()
        })
        .unwrap();
        let mut keys: Vec<_> = json.as_object().unwrap().keys().cloned().collect();
        keys.sort();
        assert_eq!(
            keys,
            [
                "description",
                "id",
                "kind",
                "parent",
                "position",
                "redo",
                "time",
                "view"
            ]
        );
        assert!(json["parent"].is_null());
        assert_eq!(json["kind"], "save");
    }

    #[test]
    fn an_op_with_a_malformed_id_is_rejected() {
        let mut json = serde_json::to_value(op()).unwrap();
        json["id"] = "../x".into();
        assert!(serde_json::from_value::<Op>(json).is_err());
    }
}

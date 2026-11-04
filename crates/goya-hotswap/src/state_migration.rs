//! Atomic, deterministic migration of persisted Goya application state.
//!
//! Migrations are explicit one-version steps. The runner clones the input
//! snapshot, applies every step in order, and returns the new map only when
//! the complete chain succeeds; callers can then commit it together with a
//! hot swap.

use goya_state::{Map, Value};

/// One deterministic operation on a persisted state snapshot.
#[derive(Clone, Debug, PartialEq)]
pub enum StateMigrationOp {
    /// Rename a key, refusing to overwrite an existing destination.
    Rename { from: String, to: String },
    /// Remove a key when present.
    Remove { key: String },
    /// Insert a value only when the key is absent.
    SetDefault { key: String, value: Value },
}

/// A migration between adjacent persisted-state contract versions.
#[derive(Clone, Debug, PartialEq)]
pub struct StateMigration {
    /// Version this migration consumes.
    pub from_version: u64,
    /// Version this migration produces; must equal `from_version + 1`.
    pub to_version: u64,
    /// Operations are applied in declaration order.
    pub operations: Vec<StateMigrationOp>,
}

/// Why a state migration chain could not be applied.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MigrationError {
    /// A step does not advance exactly one version or its counter overflows.
    InvalidStep { from: u64, to: u64 },
    /// The next migration does not begin at the currently reached version.
    MissingStep { expected_from: u64 },
    /// The supplied steps ended before reaching the requested version.
    IncompleteChain { reached: u64, target: u64 },
    /// A rename source key was absent.
    MissingSource { key: String },
    /// A rename would overwrite an existing destination key.
    DestinationExists { key: String },
}

/// Apply the complete migration chain to `state` without modifying the input.
///
/// The steps must be supplied in order and cover every version from `from`
/// through `to`. Any validation or operation failure discards the staged
/// snapshot, so callers never observe a partially migrated map.
pub fn migrate_state(
    state: &Map,
    from: u64,
    to: u64,
    migrations: &[StateMigration],
) -> Result<Map, MigrationError> {
    if from == to {
        return if migrations.is_empty() {
            Ok(state.clone())
        } else {
            Err(MigrationError::MissingStep {
                expected_from: from,
            })
        };
    }

    let mut staged = state.clone();
    let mut version = from;
    for migration in migrations {
        let expected_to = version.checked_add(1).unwrap_or(u64::MAX);
        if migration.from_version != version {
            return Err(MigrationError::MissingStep {
                expected_from: version,
            });
        }
        if version == u64::MAX || migration.to_version != expected_to {
            return Err(MigrationError::InvalidStep {
                from: migration.from_version,
                to: migration.to_version,
            });
        }

        for operation in &migration.operations {
            match operation {
                StateMigrationOp::Rename { from, to } => {
                    if staged.contains_key(to) {
                        return Err(MigrationError::DestinationExists { key: to.clone() });
                    }
                    let value = staged
                        .remove(from)
                        .ok_or_else(|| MigrationError::MissingSource { key: from.clone() })?;
                    staged.insert(to.clone(), value);
                }
                StateMigrationOp::Remove { key } => {
                    staged.remove(key);
                }
                StateMigrationOp::SetDefault { key, value } => {
                    staged.entry(key.clone()).or_insert_with(|| value.clone());
                }
            }
        }
        version = migration.to_version;
    }
    if version != to {
        return Err(MigrationError::IncompleteChain {
            reached: version,
            target: to,
        });
    }
    Ok(staged)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn step(from_version: u64, operations: Vec<StateMigrationOp>) -> StateMigration {
        StateMigration {
            from_version,
            to_version: from_version + 1,
            operations,
        }
    }

    #[test]
    fn applies_ordered_chain_and_preserves_existing_defaults() {
        let state = Map::from([
            ("count".into(), Value::Int(4)),
            ("theme".into(), Value::Text("dark".into())),
        ]);
        let result = migrate_state(
            &state,
            1,
            3,
            &[
                step(
                    1,
                    vec![
                        StateMigrationOp::Rename {
                            from: "count".into(),
                            to: "total".into(),
                        },
                        StateMigrationOp::SetDefault {
                            key: "enabled".into(),
                            value: Value::Bool(true),
                        },
                    ],
                ),
                step(
                    2,
                    vec![StateMigrationOp::Remove {
                        key: "theme".into(),
                    }],
                ),
            ],
        )
        .unwrap();
        assert_eq!(result.get("total"), Some(&Value::Int(4)));
        assert_eq!(result.get("enabled"), Some(&Value::Bool(true)));
        assert!(!result.contains_key("theme"));
        assert_eq!(state.get("count"), Some(&Value::Int(4)));
    }

    #[test]
    fn failed_operation_never_returns_a_partially_migrated_map() {
        let state = Map::from([("old".into(), Value::Int(1)), ("new".into(), Value::Int(2))]);
        let result = migrate_state(
            &state,
            1,
            2,
            &[step(
                1,
                vec![StateMigrationOp::Rename {
                    from: "old".into(),
                    to: "new".into(),
                }],
            )],
        );
        assert_eq!(
            result,
            Err(MigrationError::DestinationExists { key: "new".into() })
        );
        assert_eq!(state.get("old"), Some(&Value::Int(1)));
    }

    #[test]
    fn rejects_incomplete_or_misordered_chains() {
        let state = Map::new();
        assert_eq!(
            migrate_state(&state, 1, 3, &[step(1, vec![])]),
            Err(MigrationError::IncompleteChain {
                reached: 2,
                target: 3,
            })
        );
        assert_eq!(
            migrate_state(&state, 1, 2, &[step(2, vec![])]),
            Err(MigrationError::MissingStep { expected_from: 1 })
        );
    }
}

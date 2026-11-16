//! Deterministic validation for proposed capability catalog contracts.
//!
//! Validation checks metadata only. A valid result does not claim that any
//! capability has an executable implementation or is available on a host.

use crate::catalog::{self, Definition, DEFINITION_VERSION};
use std::collections::BTreeMap;

const CONTRACT_PREFIX: &str = "goya.";
const CONTRACT_SUFFIX: &str = ".v1";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ContractRecord<'a> {
    pub definition: &'a Definition,
    /// Contract proposed for this definition; callers can supply serialized
    /// metadata here so mismatches are detectable rather than reconstructed.
    pub contract: &'a str,
    pub version: &'a str,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Field {
    Id,
    DuplicateId,
    Summary,
    Operations,
    Invariants,
    Contract,
    Version,
}

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Problem {
    Empty,
    InvalidGrammar,
    Duplicate,
    TooFewEntries,
    EmptyEntry,
    Mismatch,
    UnsupportedVersion,
    RepeatedText,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ValidationIssue {
    /// Position in the input slice, preserving a stable pointer to source data.
    pub index: usize,
    pub id: &'static str,
    pub field: Field,
    pub problem: Problem,
}

/// Validate records and return every detected problem in deterministic order.
/// The result is fail closed: any issue returns `Err`, and no record is
/// designated executable or host-supported by this audit.
pub fn validate_records(records: &[ContractRecord<'_>]) -> Result<(), Vec<ValidationIssue>> {
    let mut issues = Vec::new();
    let mut first_id = BTreeMap::<&str, usize>::new();

    for (index, record) in records.iter().enumerate() {
        let definition = record.definition;
        let id = definition.id;
        if id.is_empty() {
            issues.push(issue(index, id, Field::Id, Problem::Empty));
        } else if !is_canonical_dotted(id) {
            issues.push(issue(index, id, Field::Id, Problem::InvalidGrammar));
        }
        if first_id.contains_key(id) {
            issues.push(issue(index, id, Field::DuplicateId, Problem::Duplicate));
        } else {
            first_id.insert(id, index);
        }

        if definition.summary.trim().is_empty() {
            issues.push(issue(index, id, Field::Summary, Problem::Empty));
        }
        validate_entries(
            index,
            id,
            Field::Operations,
            definition.operations,
            &mut issues,
        );
        validate_entries(
            index,
            id,
            Field::Invariants,
            definition.invariants,
            &mut issues,
        );
        let invariant_text: Vec<&str> = definition
            .invariants
            .iter()
            .map(|entry| entry.trim())
            .filter(|entry| !entry.is_empty())
            .collect();
        for operation in definition.operations {
            let operation = operation.trim();
            if !operation.is_empty() && invariant_text.contains(&operation) {
                issues.push(issue(index, id, Field::Operations, Problem::RepeatedText));
            }
        }

        if record.contract.is_empty() {
            issues.push(issue(index, id, Field::Contract, Problem::Empty));
        } else if !is_canonical_contract(record.contract) {
            issues.push(issue(index, id, Field::Contract, Problem::InvalidGrammar));
        }
        let expected_contract = format!("{CONTRACT_PREFIX}{id}{CONTRACT_SUFFIX}");
        if record.contract != expected_contract {
            issues.push(issue(index, id, Field::Contract, Problem::Mismatch));
        }
        if record.version.is_empty() {
            issues.push(issue(index, id, Field::Version, Problem::Empty));
        } else if record.version != DEFINITION_VERSION {
            issues.push(issue(
                index,
                id,
                Field::Version,
                Problem::UnsupportedVersion,
            ));
        }
    }

    if issues.is_empty() {
        Ok(())
    } else {
        issues.sort_by_key(|entry| (entry.index, entry.field, entry.problem));
        Err(issues)
    }
}

/// Audit the catalog currently exposed by `catalog::definitions()`.
pub fn validate_catalog() -> Result<(), Vec<ValidationIssue>> {
    let definitions: Vec<_> = catalog::definitions().collect();
    let contracts: Vec<_> = definitions
        .iter()
        .map(|definition| definition.proposed_contract())
        .collect();
    let stable_records: Vec<_> = definitions
        .iter()
        .zip(&contracts)
        .map(|(definition, contract)| ContractRecord {
            definition,
            contract,
            version: definition.version(),
        })
        .collect();
    validate_records(&stable_records)
}

fn validate_entries(
    index: usize,
    id: &'static str,
    field: Field,
    entries: &[&'static str],
    issues: &mut Vec<ValidationIssue>,
) {
    if entries.len() < 2 {
        issues.push(issue(index, id, field, Problem::TooFewEntries));
    }
    let mut seen = std::collections::BTreeSet::new();
    for entry in entries {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            issues.push(issue(index, id, field, Problem::EmptyEntry));
            continue;
        }
        if !seen.insert(trimmed) {
            issues.push(issue(index, id, field, Problem::RepeatedText));
        }
    }
}

fn is_canonical_dotted(value: &str) -> bool {
    let mut count = 0;
    value.split('.').all(|segment| {
        count += 1;
        !segment.is_empty()
            && segment
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    }) && count >= 2
}

fn is_canonical_contract(value: &str) -> bool {
    value
        .strip_prefix(CONTRACT_PREFIX)
        .and_then(|rest| rest.strip_suffix(CONTRACT_SUFFIX))
        .is_some_and(is_canonical_dotted)
}

fn issue(index: usize, id: &'static str, field: Field, problem: Problem) -> ValidationIssue {
    ValidationIssue {
        index,
        id,
        field,
        problem,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    static VALID: Definition = Definition {
        id: "sample.worker",
        summary: "A bounded sample capability.",
        operations: &["submit", "observe"],
        invariants: &["bounded", "deterministic"],
    };
    static INVALID: Definition = Definition {
        id: "Bad..ID",
        summary: "  ",
        operations: &["", "   "],
        invariants: &["only one"],
    };

    #[test]
    fn existing_catalog_validates_and_contracts_match_definitions() {
        assert_eq!(validate_catalog(), Ok(()));
        for definition in catalog::definitions() {
            let record = ContractRecord {
                definition,
                contract: &definition.proposed_contract(),
                version: definition.version(),
            };
            assert!(validate_records(&[record]).is_ok(), "{}", definition.id);
        }
    }

    #[test]
    fn reports_all_record_issues_in_stable_field_order() {
        let records = [
            ContractRecord {
                definition: &INVALID,
                contract: "not a contract",
                version: "9.9.9",
            },
            ContractRecord {
                definition: &VALID,
                contract: "goya.wrong.scope.v1",
                version: DEFINITION_VERSION,
            },
            ContractRecord {
                definition: &VALID,
                contract: "goya.sample.worker.v1",
                version: DEFINITION_VERSION,
            },
        ];
        let issues = validate_records(&records).unwrap_err();
        assert_eq!(
            issues,
            vec![
                issue(0, "Bad..ID", Field::Id, Problem::InvalidGrammar),
                issue(0, "Bad..ID", Field::Summary, Problem::Empty),
                issue(0, "Bad..ID", Field::Operations, Problem::EmptyEntry),
                issue(0, "Bad..ID", Field::Operations, Problem::EmptyEntry),
                issue(0, "Bad..ID", Field::Invariants, Problem::TooFewEntries),
                issue(0, "Bad..ID", Field::Contract, Problem::InvalidGrammar),
                issue(0, "Bad..ID", Field::Contract, Problem::Mismatch),
                issue(0, "Bad..ID", Field::Version, Problem::UnsupportedVersion),
                issue(1, "sample.worker", Field::Contract, Problem::Mismatch),
                issue(2, "sample.worker", Field::DuplicateId, Problem::Duplicate),
            ]
        );
    }

    #[test]
    fn repeated_operation_and_copied_invariant_text_are_rejected() {
        static DUPLICATE: Definition = Definition {
            id: "sample.duplicate_text",
            summary: "Detect repeated catalog text.",
            operations: &["validate value", "validate value"],
            invariants: &[
                "schema identity is explicit",
                "validation does not rewrite the input",
            ],
        };
        static COPIED: Definition = Definition {
            id: "sample.copied_text",
            summary: "Detect an operation copied into an invariant.",
            operations: &["validate value", "report violations"],
            invariants: &["validate value", "validation does not rewrite the input"],
        };
        let records = [
            ContractRecord {
                definition: &DUPLICATE,
                contract: "goya.sample.duplicate_text.v1",
                version: DEFINITION_VERSION,
            },
            ContractRecord {
                definition: &COPIED,
                contract: "goya.sample.copied_text.v1",
                version: DEFINITION_VERSION,
            },
        ];
        assert_eq!(
            validate_records(&records).unwrap_err(),
            vec![
                issue(
                    0,
                    "sample.duplicate_text",
                    Field::Operations,
                    Problem::RepeatedText
                ),
                issue(
                    1,
                    "sample.copied_text",
                    Field::Operations,
                    Problem::RepeatedText
                ),
            ]
        );
    }

    #[test]
    fn rejects_missing_and_noncanonical_identifiers() {
        static EMPTY_ID: Definition = Definition {
            id: "",
            summary: "Valid summary",
            operations: &["one", "two"],
            invariants: &["one", "two"],
        };
        let record = ContractRecord {
            definition: &EMPTY_ID,
            contract: "",
            version: "",
        };
        let issues = validate_records(&[record]).unwrap_err();
        assert!(issues.contains(&issue(0, "", Field::Id, Problem::Empty)));
        assert!(issues.contains(&issue(0, "", Field::Contract, Problem::Empty)));
        assert!(issues.contains(&issue(0, "", Field::Version, Problem::Empty)));
    }

    /// Run with `rustc --test ... --ignored --nocapture` once this module is
    /// wired into the crate to sample validation throughput. Not a stable suite.
    #[test]
    #[ignore]
    fn benchmark_catalog_validation() {
        let start = std::time::Instant::now();
        for _ in 0..10_000 {
            validate_catalog().unwrap();
        }
        println!("10,000 catalog audits in {:?}", start.elapsed());
    }
}

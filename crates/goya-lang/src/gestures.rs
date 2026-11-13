//! Experimental static policy calculation for the proposed Gestures text profile.
//!
//! This is not a complete interaction contract or an execution authorisation
//! boundary. Predicates, intrinsic preconditions, target admission, live
//! context, provenance and source-language integration are not implemented.
//! Callers must not use this calculation alone to enable or execute commands.

/// A deliberately small bound on the number of enclosing policy scopes.
pub const MAX_SCOPES: usize = 64;

/// Provisional text limit for bounded conversion; not a ratified profile limit.
pub const MAX_TEXT_BYTES: usize = 1024 * 1024;

/// A location in three different coordinate systems, all at a scalar boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TextOffset {
    pub scalars: usize,
    pub utf8_bytes: usize,
    pub utf16_units: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OffsetError {
    TextTooLarge,
    OutOfBounds,
    NotScalarBoundary,
}

/// Convert a checked scalar, UTF-8 or UTF-16 offset into all three coordinate
/// systems. A scalar boundary is not necessarily a grapheme boundary; callers
/// must perform the profile's separate grapheme validation before editing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Offset {
    Scalar(usize),
    Utf8(usize),
    Utf16(usize),
}

pub fn convert_offset(text: &str, requested: Offset) -> Result<TextOffset, OffsetError> {
    if text.len() > MAX_TEXT_BYTES {
        return Err(OffsetError::TextTooLarge);
    }
    let requested_value = match requested {
        Offset::Scalar(n) | Offset::Utf8(n) | Offset::Utf16(n) => n,
    };
    let mut location = TextOffset {
        scalars: 0,
        utf8_bytes: 0,
        utf16_units: 0,
    };
    for ch in text.chars() {
        let current = match requested {
            Offset::Scalar(_) => location.scalars,
            Offset::Utf8(_) => location.utf8_bytes,
            Offset::Utf16(_) => location.utf16_units,
        };
        if current == requested_value {
            return Ok(location);
        }
        if current > requested_value {
            return Err(OffsetError::NotScalarBoundary);
        }
        location.scalars += 1;
        location.utf8_bytes += ch.len_utf8();
        location.utf16_units += ch.len_utf16();
    }
    let end = match requested {
        Offset::Scalar(_) => location.scalars,
        Offset::Utf8(_) => location.utf8_bytes,
        Offset::Utf16(_) => location.utf16_units,
    };
    if requested_value == end {
        Ok(location)
    } else if requested_value < end {
        Err(OffsetError::NotScalarBoundary)
    } else {
        Err(OffsetError::OutOfBounds)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Select,
    SelectAll,
    Copy,
    Cut,
    Paste,
    Undo,
    Redo,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    StaticText,
    EditableText,
    ReadOnlyEditor,
    ProtectedEditor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Inherit,
    Allow,
    Deny,
}

/// One enclosing scope's decision for a single operation. Scopes must be
/// supplied outermost first. A hard denial is independent of the default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scope {
    pub default: Decision,
    pub hard_deny: bool,
}

/// Identifies the winning static default or element override. A scope index
/// is only a lookup into the provided slice, not an authored origin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionSource {
    Profile,
    Scope(usize),
    Element,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resolution {
    /// The result before runtime predicates, preconditions and admission.
    pub statically_allowed: bool,
    pub source: DecisionSource,
    /// First hard denial in outer-to-inner order, if any.
    pub hard_denial_scope: Option<usize>,
    /// An allow was overridden by an enclosing hard denial.
    pub shadowed_allow: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolveError {
    TooManyScopes,
}

/// Unconditional element decisions, without scope resolution or predicates.
/// Each operation has exactly one slot, so duplicates must be rejected by the
/// eventual source/schema decoder before constructing this value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Declarations {
    pub select: Decision,
    pub copy: Decision,
    pub cut: Decision,
    pub paste: Decision,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeclarationError {
    CopyRequiresSelection,
    CutRequiresSelection,
    CutRequiresCopy,
    MutationRequiresEditableTarget(Operation),
    ProtectedExportForbidden(Operation),
}

/// Check contradictions in unconditional *explicit* declarations. This does
/// not establish full validity: inherited scopes, conditions, hard masks,
/// related origins and adapter capabilities must also be checked later.
/// `ReadOnlyEditor` denotes a permanently read-only editor for this check;
/// temporary read-only state belongs to runtime availability instead.
pub fn validate_declarations(
    target: Target,
    declarations: Declarations,
) -> Result<(), DeclarationError> {
    if matches!(target, Target::ProtectedEditor) {
        if declarations.copy == Decision::Allow {
            return Err(DeclarationError::ProtectedExportForbidden(Operation::Copy));
        }
        if declarations.cut == Decision::Allow {
            return Err(DeclarationError::ProtectedExportForbidden(Operation::Cut));
        }
    }
    if matches!(target, Target::StaticText | Target::ReadOnlyEditor) {
        if declarations.cut == Decision::Allow {
            return Err(DeclarationError::MutationRequiresEditableTarget(
                Operation::Cut,
            ));
        }
        if declarations.paste == Decision::Allow {
            return Err(DeclarationError::MutationRequiresEditableTarget(
                Operation::Paste,
            ));
        }
    }
    if declarations.copy == Decision::Allow && declarations.select == Decision::Deny {
        return Err(DeclarationError::CopyRequiresSelection);
    }
    if declarations.cut == Decision::Allow && declarations.select == Decision::Deny {
        return Err(DeclarationError::CutRequiresSelection);
    }
    if declarations.cut == Decision::Allow && declarations.copy == Decision::Deny {
        return Err(DeclarationError::CutRequiresCopy);
    }
    Ok(())
}

/// Calculate only the static default and enclosing hard-mask portion of
/// policy. A result of `true` is not sufficient permission to execute.
pub fn resolve_static(
    target: Target,
    operation: Operation,
    scopes: &[Scope],
    element: Decision,
    application_hard_deny: bool,
) -> Result<Resolution, ResolveError> {
    if scopes.len() > MAX_SCOPES {
        return Err(ResolveError::TooManyScopes);
    }
    let mut allowed = match target {
        Target::StaticText => false,
        Target::EditableText => true,
        Target::ReadOnlyEditor => matches!(
            operation,
            Operation::Select | Operation::SelectAll | Operation::Copy
        ),
        Target::ProtectedEditor => !matches!(operation, Operation::Copy | Operation::Cut),
    };
    let mut source = DecisionSource::Profile;
    let mut hard_denial_scope = None;
    for (index, scope) in scopes.iter().enumerate() {
        match scope.default {
            Decision::Inherit => {}
            Decision::Allow => {
                allowed = true;
                source = DecisionSource::Scope(index);
            }
            Decision::Deny => {
                allowed = false;
                source = DecisionSource::Scope(index);
            }
        }
        if scope.hard_deny && hard_denial_scope.is_none() {
            hard_denial_scope = Some(index);
        }
    }
    match element {
        Decision::Inherit => {}
        Decision::Allow => {
            allowed = true;
            source = DecisionSource::Element;
        }
        Decision::Deny => {
            allowed = false;
            source = DecisionSource::Element;
        }
    }
    let denied = hard_denial_scope.is_some() || application_hard_deny;
    Ok(Resolution {
        statically_allowed: allowed && !denied,
        source,
        hard_denial_scope,
        shadowed_allow: allowed && denied,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unicode_offsets_round_trip_and_reject_partial_scalars() {
        for text in ["", "aé😀z", "e\u{301}🇬🇧", "עברית hello 👩‍💻"] {
            for scalar in 0..=text.chars().count() {
                let expected = convert_offset(text, Offset::Scalar(scalar)).unwrap();
                assert_eq!(
                    convert_offset(text, Offset::Utf8(expected.utf8_bytes)),
                    Ok(expected)
                );
                assert_eq!(
                    convert_offset(text, Offset::Utf16(expected.utf16_units)),
                    Ok(expected)
                );
            }
        }
        assert_eq!(
            convert_offset("😀", Offset::Utf8(1)),
            Err(OffsetError::NotScalarBoundary)
        );
        assert_eq!(
            convert_offset("😀", Offset::Utf16(1)),
            Err(OffsetError::NotScalarBoundary)
        );
        assert_eq!(
            convert_offset("😀", Offset::Scalar(2)),
            Err(OffsetError::OutOfBounds)
        );
        assert_eq!(
            convert_offset(&"a".repeat(MAX_TEXT_BYTES + 1), Offset::Scalar(0)),
            Err(OffsetError::TextTooLarge)
        );
    }

    #[test]
    fn profile_defaults_cover_every_operation() {
        let operations = [
            Operation::Select,
            Operation::SelectAll,
            Operation::Copy,
            Operation::Cut,
            Operation::Paste,
            Operation::Undo,
            Operation::Redo,
        ];
        for (target, expected) in [
            (
                Target::StaticText,
                [false, false, false, false, false, false, false],
            ),
            (
                Target::EditableText,
                [true, true, true, true, true, true, true],
            ),
            (
                Target::ReadOnlyEditor,
                [true, true, true, false, false, false, false],
            ),
            (
                Target::ProtectedEditor,
                [true, true, false, false, true, true, true],
            ),
        ] {
            for (op, expected) in operations.into_iter().zip(expected) {
                assert_eq!(
                    resolve_static(target, op, &[], Decision::Inherit, false)
                        .unwrap()
                        .statically_allowed,
                    expected,
                    "{target:?} {op:?}"
                );
            }
        }
    }

    #[test]
    fn nearest_default_and_element_override_cannot_widen_hard_denial() {
        let scopes = [
            Scope {
                default: Decision::Allow,
                hard_deny: true,
            },
            Scope {
                default: Decision::Deny,
                hard_deny: false,
            },
        ];
        let inherited = resolve_static(
            Target::StaticText,
            Operation::Copy,
            &scopes,
            Decision::Inherit,
            false,
        )
        .unwrap();
        assert_eq!(inherited.source, DecisionSource::Scope(1));
        assert!(!inherited.statically_allowed);
        assert!(!inherited.shadowed_allow);
        let explicit = resolve_static(
            Target::StaticText,
            Operation::Copy,
            &scopes,
            Decision::Allow,
            false,
        )
        .unwrap();
        assert_eq!(explicit.source, DecisionSource::Element);
        assert_eq!(explicit.hard_denial_scope, Some(0));
        assert!(explicit.shadowed_allow);
        assert!(!explicit.statically_allowed);
        let security = resolve_static(
            Target::EditableText,
            Operation::Paste,
            &[],
            Decision::Inherit,
            true,
        )
        .unwrap();
        assert!(!security.statically_allowed);
        assert!(security.shadowed_allow);
    }

    #[test]
    fn rejects_unbounded_scope_lists() {
        let scopes = vec![
            Scope {
                default: Decision::Inherit,
                hard_deny: false
            };
            MAX_SCOPES + 1
        ];
        assert_eq!(
            resolve_static(
                Target::EditableText,
                Operation::Copy,
                &scopes,
                Decision::Inherit,
                false
            ),
            Err(ResolveError::TooManyScopes)
        );
    }

    #[test]
    fn unconditional_declarations_reject_known_contradictions() {
        let allowed = Declarations {
            select: Decision::Allow,
            copy: Decision::Allow,
            cut: Decision::Allow,
            paste: Decision::Allow,
        };
        assert_eq!(validate_declarations(Target::EditableText, allowed), Ok(()));
        assert_eq!(
            validate_declarations(
                Target::EditableText,
                Declarations {
                    select: Decision::Deny,
                    ..allowed
                }
            ),
            Err(DeclarationError::CopyRequiresSelection)
        );
        assert_eq!(
            validate_declarations(
                Target::EditableText,
                Declarations {
                    copy: Decision::Deny,
                    ..allowed
                }
            ),
            Err(DeclarationError::CutRequiresCopy)
        );
        assert_eq!(
            validate_declarations(Target::ReadOnlyEditor, allowed),
            Err(DeclarationError::MutationRequiresEditableTarget(
                Operation::Cut
            ))
        );
        assert_eq!(
            validate_declarations(Target::ProtectedEditor, allowed),
            Err(DeclarationError::ProtectedExportForbidden(Operation::Copy))
        );
        let protected = Declarations {
            select: Decision::Inherit,
            copy: Decision::Deny,
            cut: Decision::Deny,
            paste: Decision::Allow,
        };
        assert_eq!(
            validate_declarations(Target::ProtectedEditor, protected),
            Ok(())
        );
    }
}

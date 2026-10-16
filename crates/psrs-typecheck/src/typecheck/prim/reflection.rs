//! Runtime dictionaries for designated, validated library interfaces.
use super::*;
use crate::typecheck::classes::record_field_type;

pub(super) fn rule(class_id: hir::TypeId, identity: hir::CompilerClass) -> PrimitiveRule {
    match identity {
        hir::CompilerClass::IsSymbol => PrimitiveRule {
            class_id,
            evidence: EvidenceClass::RuntimeDictionary,
            arity: 1,
            solve: is_symbol,
        },
    }
}

fn is_symbol(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [InferType::TypeLevelString(value)] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let field = hir::CompilerClass::IsSymbol.method();
    let dictionary_type = checker.resolve_type(args.constraint.dictionary_type.clone());
    let Some(method_type) = record_field_type(&dictionary_type, field) else {
        return PrimitiveOutcome::Undecided;
    };
    let InferType::Application(function, _) = &method_type else {
        return PrimitiveOutcome::Undecided;
    };
    let InferType::Application(_, parameter) = function.as_ref() else {
        return PrimitiveOutcome::Undecided;
    };
    let span = args.span();
    let id = LocalId(checker.state.next_dictionary_local);
    checker.state.next_dictionary_local += 1;
    let method = InferredExpr {
        kind: InferredExprKind::Lambda {
            binder: InferredBinder {
                binder: hir::LocalBinder {
                    id,
                    name: "$reflect".into(),
                    span,
                },
                scheme: Scheme::monomorphic(parameter.as_ref().clone()),
            },
            body: Box::new(InferredExpr {
                kind: InferredExprKind::String(value.clone()),
                ty: InferType::Constructor(TypeConstructor::String),
                span,
            }),
        },
        ty: method_type,
        span,
    };
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::DictionaryValue {
            arguments,
            value: Box::new(InferredExpr {
                kind: InferredExprKind::Record(vec![(field.into(), method)]),
                ty: dictionary_type,
                span,
            }),
        },
        deferred: Vec::new(),
    }
}

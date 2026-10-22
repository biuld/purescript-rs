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
        hir::CompilerClass::Reflectable => PrimitiveRule {
            class_id,
            evidence: EvidenceClass::RuntimeDictionary,
            arity: 2,
            solve: reflectable,
        },
    }
}

fn is_symbol(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [InferType::TypeLevelString(value)] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    reflection_dictionary(
        checker,
        args,
        hir::CompilerClass::IsSymbol,
        arguments.clone(),
        InferredExpr {
            kind: InferredExprKind::String(value.clone()),
            ty: InferType::Constructor(TypeConstructor::String),
            span: args.span(),
        },
    )
}

/// Official `solveReflectable` decides both the reflected value and its term
/// type. The common primitive verifier checks that type against the goal.
fn reflectable(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [value, _] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let (kind, constructor) = match value {
        InferType::TypeLevelString(value) => (
            InferredExprKind::String(value.clone()),
            TypeConstructor::String,
        ),
        InferType::TypeLevelInt(value) => {
            let Ok(value) = i32::try_from(*value) else {
                return PrimitiveOutcome::Failed {
                    code: TypeCheckErrorKind::IntegerOutOfRange,
                    detail: "reflected integer is outside the target Int range".into(),
                };
            };
            (InferredExprKind::Integer(value), TypeConstructor::Int)
        }
        InferType::Constructor(TypeConstructor::User(id))
            if *id == hir::TypeId::PRIM_BOOLEAN_TRUE || *id == hir::TypeId::PRIM_BOOLEAN_FALSE =>
        {
            (
                InferredExprKind::Boolean(*id == hir::TypeId::PRIM_BOOLEAN_TRUE),
                TypeConstructor::Boolean,
            )
        }
        InferType::Constructor(TypeConstructor::User(id)) => {
            let Some(ordering) = checker.env.deriving.ordering() else {
                return PrimitiveOutcome::Undecided;
            };
            let symbol = match *id {
                hir::TypeId::PRIM_ORDERING_LT => ordering.lt,
                hir::TypeId::PRIM_ORDERING_EQ => ordering.eq,
                hir::TypeId::PRIM_ORDERING_GT => ordering.gt,
                _ => return PrimitiveOutcome::Undecided,
            };
            let Some(constructor) = checker.env.constructor_info.get(&symbol) else {
                return PrimitiveOutcome::Undecided;
            };
            (
                InferredExprKind::Global(symbol),
                TypeConstructor::User(constructor.type_id),
            )
        }
        _ => return PrimitiveOutcome::Undecided,
    };
    let ty = InferType::Constructor(constructor);
    reflection_dictionary(
        checker,
        args,
        hir::CompilerClass::Reflectable,
        vec![value.clone(), ty.clone()],
        InferredExpr {
            kind,
            ty,
            span: args.span(),
        },
    )
}

fn reflection_dictionary(
    checker: &mut Checker,
    args: &PrimitiveArgs,
    identity: hir::CompilerClass,
    arguments: Vec<InferType>,
    body: InferredExpr,
) -> PrimitiveOutcome {
    let field = identity.method();
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
            body: Box::new(body),
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

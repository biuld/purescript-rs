use super::*;
use crate::typecheck::classes::deriving::syntax::{
    case_expr, constructor_pattern, if_expr, lambda, wildcard_pattern,
};

impl Checker {
    pub(super) fn derive_eq_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        // A field whose type is an applied type variable `f a` is compared
        // through `Eq1`'s `eq1`, exactly as the official deriving rule does.
        // The lookup is lazy because a data type with no such field does not
        // need `Eq1` to exist in the environment at all.
        let needs_eq1 = class_arguments
            .first()
            .and_then(|argument| {
                let instance_type = self.resolve_type(argument.clone());
                let (head, _) = flatten_spine(&instance_type);
                let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
                    return None;
                };
                self.env.type_declarations.get(type_id).map(|declaration| {
                    declaration.constructors.iter().any(|constructor| {
                        constructor
                            .fields
                            .iter()
                            .any(|field| is_applied_variable(&self.normalize_deriving_type(field)))
                    })
                })
            })
            .unwrap_or(false);
        let eq1_method = if needs_eq1 {
            match self.known_method(KnownClass::Eq1, "eq1") {
                Some(method) => Some(method),
                None => {
                    return self.deriving_error(
                        TypeCheckErrorKind::CannotDerive,
                        span,
                        "cannot find the Eq1 method for Eq deriving",
                    );
                }
            }
        } else {
            None
        };
        self.derive_structural_eq(method, class_arguments, method.symbol, eq1_method, span)
    }

    pub(super) fn derive_eq1_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        // The official rule derives `Eq1` by delegating to `Eq` at the applied
        // argument: `eq1 = eq`. The matching `Eq` instance supplies the
        // dictionary, so its context must be visible alongside the method's.
        let Some(eq_method) = self.known_method(KnownClass::Eq, "eq") else {
            return self.deriving_error(
                TypeCheckErrorKind::CannotDerive,
                span,
                "cannot find the Eq method for Eq1 deriving",
            );
        };
        let implementation = global_expr(eq_method, span);
        self.infer_derived_method(method, class_arguments, &implementation)
    }

    fn derive_structural_eq(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        eq_method: SymbolId,
        eq1_method: Option<SymbolId>,
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidDerivedInstance,
                span,
                "equality deriving requires one type argument",
            );
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                TypeCheckErrorKind::ExpectedTypeConstructor,
                span,
                "Eq deriving requires a local data or newtype constructor",
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(
                TypeCheckErrorKind::CannotFindDerivingType,
                span,
                "cannot find the data declaration to derive equality",
            );
        };
        if type_id.module != self.env.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || declaration.parameters.len() != arguments.len()
        {
            return self.deriving_error(
                TypeCheckErrorKind::ExpectedTypeConstructor,
                span,
                "Eq deriving requires a locally declared, fully applied data type",
            );
        }

        let left = self.fresh_deriving_binder("__derived_left", span);
        let right = self.fresh_deriving_binder("__derived_right", span);
        let mut left_case_branches = Vec::new();
        for constructor in &declaration.constructors {
            let left_fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_l", span))
                .collect::<Vec<_>>();
            let right_fields = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_r", span))
                .collect::<Vec<_>>();
            let field_types = constructor
                .fields
                .iter()
                .map(|field| self.normalize_deriving_type(field))
                .collect::<Vec<_>>();
            let body = derive_eq_field_tests(
                &field_types,
                &left_fields,
                &right_fields,
                eq_method,
                eq1_method,
                span,
            );
            let same_constructor = hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: constructor_pattern(constructor, &right_fields, span),
                value: body,
                span,
            };
            let mismatch = hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: wildcard_pattern(span),
                value: boolean_literal(false, span),
                span,
            };
            let right_case = case_expr(
                local_expr(right.id, span),
                vec![same_constructor, mismatch],
                span,
            );
            left_case_branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: constructor_pattern(constructor, &left_fields, span),
                value: right_case,
                span,
            });
        }
        if left_case_branches.is_empty() {
            left_case_branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: wildcard_pattern(span),
                value: boolean_literal(true, span),
                span,
            });
        }
        let implementation = lambda(
            left.clone(),
            lambda(
                right.clone(),
                case_expr(local_expr(left.id, span), left_case_branches, span),
                span,
            ),
            span,
        );
        self.infer_derived_method(method, class_arguments, &implementation)
    }
}

fn derive_eq_field_tests(
    fields: &[hir::Type],
    left_fields: &[hir::LocalBinder],
    right_fields: &[hir::LocalBinder],
    eq_method: SymbolId,
    eq1_method: Option<SymbolId>,
    span: TextRange,
) -> hir::Expr {
    fields
        .iter()
        .zip(left_fields)
        .zip(right_fields)
        .map(|((field, left), right)| {
            let method = if is_applied_variable(field) {
                eq1_method.unwrap_or(eq_method)
            } else {
                eq_method
            };
            let method = global_expr(method, span);
            apply_expr(
                apply_expr(method, local_expr(left.id, span), span),
                local_expr(right.id, span),
                span,
            )
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .fold(boolean_literal(true, span), |rest, test| {
            if_expr(test, rest, boolean_literal(false, span), span)
        })
}

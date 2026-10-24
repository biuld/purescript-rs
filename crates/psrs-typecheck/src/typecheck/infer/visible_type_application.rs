//! Visible type application `e @T`.
//!
//! Official `TypeChecker/Types.hs` infers `VisibleTypeApp` by stepping past
//! invisible quantifiers, then substituting the written argument for the
//! outermost visible one after a kind check. `e @_` makes that binder
//! invisible without substituting, so a later `@T` steps over it. Quantifiers
//! the application does not select stay quantified. The application is erased:
//! it introduces no node of its own in THIR or Core.

use super::super::*;

/// Infers `expression @ty`.
pub(in crate::typecheck) fn infer_type_application(
    checker: &mut Checker,
    expression: &hir::Expr,
    ty: &hir::Type,
    span: TextRange,
) -> Option<InferredExpr> {
    let skip = matches!(ty.kind, hir::TypeKind::Wildcard);
    let mut variables = checker.scope.annotation_variables.clone();
    let argument = checker.elaborate_type(ty, &mut variables);
    let operand = checker.infer_expr(expression)?;
    let (binding, operand) = checker.prepare_expression_use(operand);
    let mut operand = checker.instantiate_until_visible_expr(operand);
    let resolved = checker.resolve_type(operand.ty.clone());
    let InferType::ForAll {
        variables,
        visible,
        body,
    } = resolved
    else {
        return report_unapplicable(checker, skip, span);
    };
    if !visible.first().copied().unwrap_or(false) {
        return report_unapplicable(checker, skip, span);
    }
    if skip {
        let mut visible = visible;
        visible[0] = false;
        operand.ty = InferType::ForAll {
            variables,
            visible,
            body,
        };
        operand.span = span;
        return Some(checker.finish_expression_use(binding, operand));
    }
    let quantifier = variables[0];
    if let (Some(expected), Some(actual)) = (
        checker.recorded_kind(quantifier),
        checker.kind_of_type(&argument, ty.span),
    ) {
        // A leading `forall` in the quantifier's kind is a kind parameter.
        // The argument's constructor scheme is already instantiated, so the
        // expected kind opens the same way before the equation is solved.
        let expected = checker.instantiate_leading_kind_quantifiers(expected);
        checker.unify_kind(expected, actual, ty.span);
    }
    let replacement = checker.resolve_type(argument);
    let mapping = HashMap::from([(quantifier, replacement)]);
    let mut result = substitute_visible(&body, &mapping);
    if variables.len() > 1 {
        result = InferType::ForAll {
            variables: variables[1..].to_vec(),
            visible: visible[1..].to_vec(),
            body: Box::new(result),
        };
    }
    operand.ty = checker.resolve_type(result);
    operand.span = span;
    let operand = checker.instantiate_head_constraints(operand, span);
    Some(checker.finish_expression_use(binding, operand))
}

fn report_unapplicable(checker: &mut Checker, skip: bool, span: TextRange) -> Option<InferredExpr> {
    checker.state.errors.push(TypeCheckError::new(
        if skip {
            TypeCheckErrorKind::CannotSkipTypeApplication
        } else {
            TypeCheckErrorKind::CannotApplyExpressionOfTypeOnType
        },
        span,
        if skip {
            "an expression whose type has no quantified variable cannot be skipped"
        } else {
            "a visible type application needs a quantified type variable to apply its argument to"
        },
    ));
    None
}

fn substitute_visible(ty: &InferType, mapping: &HashMap<u32, InferType>) -> InferType {
    crate::typecheck::unify::substitute(ty, mapping)
}

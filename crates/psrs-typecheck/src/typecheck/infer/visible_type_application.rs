//! Visible type application `e @T`.
//!
//! Official `TypeChecker/Types.hs` infers `VisibleTypeApp valFn tyArg` by
//! inferring `valFn`, stepping past any quantifiers that are already invisible,
//! and then substituting the written argument for the outermost *visible*
//! quantifier, having first checked that argument against that quantifier's
//! kind. `e @_` is the same step with no argument: it consumes the outermost
//! visible quantifier, which a later `@T` then steps over.
//!
//! Two properties of this compiler shape the implementation. Inference
//! instantiates a scheme's quantified variables the moment a name is used, so
//! the quantifiers a visible application selects are already gone by the time
//! inference returns a type; they are read from the operand's *scheme*, which is
//! where ordinary application reads them from too. And a visible application is
//! erased at runtime exactly like an ascription, so it introduces no node of
//! its own in THIR or Core: the value is the operand.
//!
//! Official marks a skipped quantifier invisible and defers replacing it with an
//! unknown until a later application steps over it, and it keeps the quantifiers
//! an application did not name *rigid* so that a later use cannot solve them.
//! This implementation instantiates a skipped quantifier immediately and
//! instantiates the ones left over, so both are flexible. Three consequences
//! follow, and each is a divergence in the permissive direction — this compiler
//! accepts programs `purs` rejects — rather than a mistyping:
//!
//!   * `purs` makes a plain `forall a.` binder invisible and answers `f @Int`
//!     for `f :: forall a. a -> a` with `CannotApplyExpressionOfTypeOnType`. This
//!     compiler's CST does not carry binder visibility at all, so it accepts.
//!   * `purs` leaves a quantifier an application did not name rigid, so it
//!     rejects `applySecond (f @Int)` against a monomorphic function type. This
//!     compiler instantiates it and accepts.
//!   * `purs` can step over an invisible binder, so `f @_ @Int` resolves. This
//!     compiler cannot, because the remaining quantifiers are scheme variables
//!     rather than a structural `forall`, so a chain is reported.
//!
//! Making these agree needs the scheme to record which of its variables a
//! visible application has consumed and whether the source binder was visible;
//! that is frontend work recorded under FE-17 rather than something to imitate
//! here.

use super::super::*;
use crate::typecheck::unify::substitute;

/// Infers `expression @ty`.
///
/// The written type is elaborated first, so a wildcard inside it becomes a fresh
/// unknown as it does in any other annotation position. The operand is then read
/// through its scheme, its outermost visible quantifier is selected, and the
/// result is the operand's type with that quantifier replaced.
pub(in crate::typecheck) fn infer_type_application(
    checker: &mut Checker,
    expression: &hir::Expr,
    ty: &hir::Type,
    span: TextRange,
) -> Option<InferredExpr> {
    let skip = matches!(ty.kind, hir::TypeKind::Wildcard);

    // A chained application `f @A @B` selects a quantifier the first
    // application left behind. This compiler instantiates a scheme the moment a
    // name is used, so those remaining quantifiers are scheme variables here
    // rather than a structural `forall`, and choosing between them needs the
    // scheme to record which variables a visible application has not yet
    // consumed. It does not, so the chain is reported instead of resolved against
    // a reconstructed binder list that no other stage would agree with.
    if matches!(expression.kind, hir::ExprKind::TypeApplication { .. }) {
        checker.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::UnsupportedExpression,
            span,
            "a chained visible type application is not supported yet",
        ));
        return None;
    }

    let mut variables = checker.scope.annotation_variables.clone();
    let argument = checker.elaborate_type(ty, &mut variables);

    let operand = checker.infer_expr(expression)?;
    let (quantifiers, mut result) = visible_quantifiers(checker, expression, &operand);

    let Some((quantifier, kind)) = quantifiers.first().cloned() else {
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
        return None;
    };

    let replacement = if skip {
        // A wildcard argument consumes the quantifier without choosing a type
        // for it. The quantifier's recorded kind carries over to the unknown, so
        // the skipped position cannot later be used where its kind would not fit.
        let InferType::Variable(fresh) = checker.fresh() else {
            unreachable!("fresh inference types are variables")
        };
        if let Some(kind) = kind {
            checker.record_variable_kind(fresh, kind);
        }
        InferType::Variable(fresh)
    } else {
        // The argument is checked against the quantifier's own kind through the
        // shared kind denotation, so a visible application cannot put an `Int`
        // where a `Symbol` is bound. A quantifier with no recorded kind is left
        // to unification, which is what an unannotated binder leaves to infer.
        if let (Some(expected), Some(actual)) = (kind, checker.kind_of_type(&argument, ty.span)) {
            checker.unify_kind(expected, actual, ty.span);
        }
        argument
    };
    let replacement = checker.resolve_type(replacement);

    // The selected quantifier is replaced by the argument; the ones the
    // application did not name are instantiated, exactly as an ordinary use of
    // the same name instantiates them. `purs` instead leaves them quantified and
    // rigid, so it rejects `f @Int` where a monomorphic function type is
    // expected while this accepts it. That difference is the permissive
    // direction and is recorded in the design documents.
    let mut mapping = HashMap::from([(quantifier, replacement)]);
    let remaining = checker.instantiate_type_variables(
        quantifiers.iter().skip(1).map(|(variable, _)| *variable),
        &HashMap::new(),
    );
    mapping.extend(remaining);
    result = checker.resolve_type(substitute(&result, &mapping));

    Some(InferredExpr {
        kind: operand.kind,
        ty: result,
        span,
    })
}

/// The quantifiers a visible application may select, outermost first, together
/// with the operand's type with those quantifiers peeled off.
///
/// A name carries its quantifiers in its scheme. A name with no scheme — an
/// intrinsic, or an expression that is not a name at all — carries them
/// structurally, as `forall`s already present in the inferred type; a type
/// ascription is the ordinary way a structural `forall` reaches an expression.
fn visible_quantifiers(
    checker: &Checker,
    expression: &hir::Expr,
    operand: &InferredExpr,
) -> (Vec<(u32, Option<Kind>)>, InferType) {
    let scheme = match &expression.kind {
        hir::ExprKind::Global(symbol) => checker.scope.globals.get(symbol),
        hir::ExprKind::Local(id) => checker.scope.locals.get(id),
        _ => None,
    };
    if let Some(scheme) = scheme {
        let quantifiers = scheme
            .variables
            .iter()
            .map(|variable| {
                let kind = scheme
                    .variable_kinds
                    .get(variable)
                    .cloned()
                    .or_else(|| checker.recorded_kind(*variable));
                (*variable, kind)
            })
            .collect();
        return (quantifiers, scheme.ty.clone());
    }
    let mut quantifiers = Vec::new();
    let mut result = operand.ty.clone();
    while let InferType::ForAll { variables, body } = result.clone() {
        for variable in variables {
            let kind = checker.recorded_kind(variable);
            quantifiers.push((variable, kind));
        }
        result = *body;
    }
    (quantifiers, result)
}

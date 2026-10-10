//! `Foldable` and `Bifoldable` deriving.
//!
//! `foldMap` combines each field's monoidal contribution; `foldr`/`foldl`
//! thread an accumulator through the contributing fields in right or left
//! order. The field usage tree decides which occurrences fold through the
//! nested `Foldable`/`Bifoldable` instance and which are inert.

use crate::typecheck::classes::deriving::syntax::{case_expr, constructor_pattern, lambda};
mod branches;

use super::super::super::*;
use super::usage::FieldUsage;
use super::{KnownClass, MappingClasses, apply_expr, flatten_spine, global_expr, local_expr};

/// The method symbols a fold rule uses.
#[derive(Clone, Copy)]
struct FoldOps {
    fold_map: Option<SymbolId>,
    fold_r: Option<SymbolId>,
    fold_l: Option<SymbolId>,
    bifold_map: Option<SymbolId>,
    bifold_r: Option<SymbolId>,
    bifold_l: Option<SymbolId>,
    append: Option<SymbolId>,
    mempty: Option<SymbolId>,
}

/// The shared inputs of the per-method branch builders.
#[derive(Clone, Copy)]
struct FoldContext<'a> {
    ops: FoldOps,
    left: &'a hir::Expr,
    right: &'a hir::Expr,
    span: TextRange,
}

impl Checker {
    pub(super) fn derive_foldable_method(
        &mut self,
        known: KnownClass,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let is_bi = known == KnownClass::Bifoldable;
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidDerivedInstance,
                span,
                "fold deriving requires one type argument",
            );
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                TypeCheckErrorKind::ExpectedTypeConstructor,
                span,
                "fold deriving requires a local type constructor",
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(
                TypeCheckErrorKind::CannotFindDerivingType,
                span,
                "cannot find the data declaration to derive a fold",
            );
        };
        let arity = if is_bi { 2 } else { 1 };
        if type_id.module != self.env.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || declaration.parameters.len() < arity
            || arguments.len() + arity != declaration.parameters.len()
        {
            return self.deriving_error(
                TypeCheckErrorKind::ExpectedTypeConstructor,
                span,
                "fold deriving requires a local type constructor applied to all but its final parameters",
            );
        }
        let lparam = is_bi.then(|| {
            declaration.parameters[declaration.parameters.len() - 2]
                .name
                .as_str()
        });
        let param = &declaration.parameters[declaration.parameters.len() - 1].name;
        let usages = match self.validate_field_usage(
            &declaration,
            MappingClasses::foldable(),
            lparam,
            param,
            (false, false),
            &arguments,
        ) {
            Ok(usages) => usages,
            Err(offending) => {
                return self.deriving_error(
                    TypeCheckErrorKind::CannotDeriveInvalidConstructorArg,
                    offending,
                    "fold deriving cannot map a parameter occurrence in this field",
                );
            }
        };
        let ops = self.fold_ops();
        let left = self.fresh_deriving_binder("__derived_f", span);
        let right = if is_bi {
            self.fresh_deriving_binder("__derived_g", span)
        } else {
            left.clone()
        };
        let accumulator = matches!(
            method.name.as_str(),
            "foldr" | "bifoldr" | "foldl" | "bifoldl"
        )
        .then(|| self.fresh_deriving_binder("__derived_z", span));
        let left_expr = local_expr(left.id, span);
        let right_expr = local_expr(right.id, span);
        let accumulator_expr = accumulator
            .as_ref()
            .map(|binder| local_expr(binder.id, span));
        let context = FoldContext {
            ops,
            left: &left_expr,
            right: &right_expr,
            span,
        };
        let mut branches = Vec::with_capacity(declaration.constructors.len());
        for (constructor, field_usages) in declaration.constructors.iter().zip(&usages) {
            let binders = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_field", span))
                .collect::<Vec<_>>();
            let value = match method.name.as_str() {
                "foldMap" | "bifoldMap" => {
                    self.fold_map_branch(&binders, field_usages, &context)?
                }
                "foldr" | "bifoldr" => self.fold_r_branch(
                    &binders,
                    field_usages,
                    accumulator_expr.as_ref()?,
                    &context,
                )?,
                "foldl" | "bifoldl" => self.fold_l_branch(
                    &binders,
                    field_usages,
                    accumulator_expr.as_ref()?,
                    &context,
                )?,
                _ => {
                    return self.deriving_error(
                        TypeCheckErrorKind::CannotDerive,
                        span,
                        "the known-class deriving rule is unavailable for this class method",
                    );
                }
            };
            branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: constructor_pattern(constructor, &binders, span),
                value,
                span,
            });
        }
        let value = self.fresh_deriving_binder("__derived_value", span);
        let mut body = case_expr(local_expr(value.id, span), branches, span);
        body = lambda(value, body, span);
        if let Some(accumulator) = accumulator {
            body = lambda(accumulator, body, span);
        }
        if is_bi {
            body = lambda(right, body, span);
        }
        let implementation = lambda(left, body, span);
        self.infer_derived_method(method, class_arguments, &implementation)
    }

    fn fold_ops(&self) -> FoldOps {
        FoldOps {
            fold_map: self.known_method(KnownClass::Foldable, "foldMap"),
            fold_r: self.known_method(KnownClass::Foldable, "foldr"),
            fold_l: self.known_method(KnownClass::Foldable, "foldl"),
            bifold_map: self.known_method(KnownClass::Bifoldable, "bifoldMap"),
            bifold_r: self.known_method(KnownClass::Bifoldable, "bifoldr"),
            bifold_l: self.known_method(KnownClass::Bifoldable, "bifoldl"),
            append: self.env.deriving.append(),
            mempty: self.env.deriving.mempty(),
        }
    }
}

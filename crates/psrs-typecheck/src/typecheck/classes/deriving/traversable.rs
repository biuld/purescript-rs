//! `Traversable` and `Bitraversable` deriving.
//!
//! `traverse` runs an effect for each parameter occurrence and rebuilds the
//! constructor inside the applicative: `map C e1 <*> e2 ...`. Inert fields are
//! carried through unchanged, and a nested occurrence recurses through the
//! `Traversable`/`Bitraversable` instance. `sequence` is `traverse identity`.

use super::super::super::*;
use super::syntax::{constructor_pattern, field_expr, record_update};
use super::usage::FieldUsage;
use super::{KnownClass, MappingClasses, apply_expr, flatten_spine, global_expr, local_expr};
use crate::typecheck::classes::deriving::syntax::{case_expr, lambda};

#[derive(Clone, Copy)]
struct TraverseOps {
    traverse: Option<SymbolId>,
    bitraverse: Option<SymbolId>,
    map: Option<SymbolId>,
    apply: Option<SymbolId>,
    pure: Option<SymbolId>,
    identity: Option<SymbolId>,
}

#[derive(Clone, Copy)]
struct TraverseContext<'a> {
    ops: TraverseOps,
    left: &'a hir::Expr,
    right: &'a hir::Expr,
    span: TextRange,
}

impl Checker {
    pub(super) fn derive_traversable_method(
        &mut self,
        known: KnownClass,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let is_bi = known == KnownClass::Bitraversable;
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidDerivedInstance,
                span,
                "traversal deriving requires one type argument",
            );
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                TypeCheckErrorKind::ExpectedTypeConstructor,
                span,
                "traversal deriving requires a local type constructor",
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(
                TypeCheckErrorKind::CannotFindDerivingType,
                span,
                "cannot find the data declaration to derive a traversal",
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
                "traversal deriving requires a local type constructor applied to all but its final parameters",
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
            MappingClasses::traversable(),
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
                    "traversal deriving cannot map a parameter occurrence in this field",
                );
            }
        };
        let ops = self.traverse_ops();
        if matches!(method.name.as_str(), "traverse" | "bitraverse") {
            let traversal_method = if is_bi { ops.bitraverse } else { ops.traverse };
            if ops.map.is_none()
                || ops.apply.is_none()
                || ops.pure.is_none()
                || traversal_method.is_none()
            {
                return self.deriving_error(
                    TypeCheckErrorKind::CannotFindDerivingType,
                    span,
                    "traversal deriving requires the Functor, Apply, and Applicative classes",
                );
            }
        } else {
            let traversal_method = if is_bi { ops.bitraverse } else { ops.traverse };
            if ops.identity.is_none() || traversal_method.is_none() {
                return self.deriving_error(
                    TypeCheckErrorKind::CannotFindDerivingType,
                    span,
                    "sequence deriving requires the registered identity and the traversal method",
                );
            }
        }
        let value = self.fresh_deriving_binder("__derived_value", span);
        let implementation = if matches!(method.name.as_str(), "sequence" | "bisequence") {
            let identity = global_expr(ops.identity?, span);
            let traversing = if is_bi {
                apply_expr(
                    apply_expr(global_expr(ops.bitraverse?, span), identity.clone(), span),
                    identity,
                    span,
                )
            } else {
                apply_expr(global_expr(ops.traverse?, span), identity, span)
            };
            lambda(
                value.clone(),
                apply_expr(traversing, local_expr(value.id, span), span),
                span,
            )
        } else {
            let left = self.fresh_deriving_binder("__derived_f", span);
            let right = if is_bi {
                self.fresh_deriving_binder("__derived_g", span)
            } else {
                left.clone()
            };
            let left_expr = local_expr(left.id, span);
            let right_expr = local_expr(right.id, span);
            let context = TraverseContext {
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
                let body = self.traverse_branch(constructor, &binders, field_usages, &context)?;
                branches.push(hir::CaseBranch {
                    coverage: hir::CaseBranchCoverage::Source,
                    pattern: constructor_pattern(constructor, &binders, span),
                    value: body,
                    span,
                });
            }
            let mut body = case_expr(local_expr(value.id, span), branches, span);
            body = lambda(value, body, span);
            if is_bi {
                body = lambda(right, body, span);
            }
            lambda(left, body, span)
        };
        self.infer_derived_method(method, class_arguments, &implementation)
    }

    fn traverse_ops(&self) -> TraverseOps {
        TraverseOps {
            traverse: self.known_method(KnownClass::Traversable, "traverse"),
            bitraverse: self.known_method(KnownClass::Bitraversable, "bitraverse"),
            map: self.known_method(KnownClass::Functor, "map"),
            apply: self.env.deriving.apply(),
            pure: self.env.deriving.pure(),
            identity: self.env.deriving.identity(),
        }
    }

    fn traverse_branch(
        &mut self,
        constructor: &hir::Constructor,
        binders: &[hir::LocalBinder],
        usages: &[FieldUsage],
        context: &TraverseContext<'_>,
    ) -> Option<hir::Expr> {
        let TraverseContext {
            ref ops,
            left,
            right,
            span,
        } = *context;
        let mut arguments = Vec::new();
        let mut effects = Vec::new();
        let mut effect_binders = Vec::new();
        for (binder, usage) in binders.iter().zip(usages) {
            let field = local_expr(binder.id, span);
            if *usage == FieldUsage::Inert {
                arguments.push(field);
            } else {
                let effect_binder = self.fresh_deriving_binder("__derived_effect", span);
                arguments.push(local_expr(effect_binder.id, span));
                effects.push(self.traverse_effect(usage, &field, ops, left, right, span)?);
                effect_binders.push(effect_binder);
            }
        }
        let mut constructed = global_expr(constructor.symbol, span);
        for argument in arguments {
            constructed = apply_expr(constructed, argument, span);
        }
        self.combine_traversal(constructed, effects, effect_binders, ops, span)
    }

    fn combine_traversal(
        &mut self,
        constructed: hir::Expr,
        effects: Vec<hir::Expr>,
        effect_binders: Vec<hir::LocalBinder>,
        ops: &TraverseOps,
        span: TextRange,
    ) -> Option<hir::Expr> {
        if effects.is_empty() {
            return Some(apply_expr(global_expr(ops.pure?, span), constructed, span));
        }
        let mut combine = constructed;
        for binder in effect_binders.into_iter().rev() {
            combine = lambda(binder, combine, span);
        }
        let mut effects = effects.into_iter();
        let first = effects.next()?;
        let mut body = apply_expr(
            apply_expr(global_expr(ops.map?, span), combine, span),
            first,
            span,
        );
        for effect in effects {
            body = apply_expr(
                apply_expr(global_expr(ops.apply?, span), body, span),
                effect,
                span,
            );
        }
        Some(body)
    }

    /// The effectful expression for one field occurrence.
    fn traverse_effect(
        &mut self,
        usage: &FieldUsage,
        field: &hir::Expr,
        ops: &TraverseOps,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        let function = match usage {
            FieldUsage::Record(fields) => {
                return self.traverse_record(fields, field, ops, left, right, span);
            }
            FieldUsage::Param => right.clone(),
            FieldUsage::LParam => left.clone(),
            FieldUsage::Mono(inner) => apply_expr(
                global_expr(ops.traverse?, span),
                self.traverse_function(inner, ops, left, right, span)?,
                span,
            ),
            FieldUsage::Bi(l, r) => apply_expr(
                apply_expr(
                    global_expr(ops.bitraverse?, span),
                    self.traverse_argument(l, ops, left, right, span)?,
                    span,
                ),
                self.traverse_argument(r, ops, left, right, span)?,
                span,
            ),
            FieldUsage::Contra(inner) | FieldUsage::Pro(inner, _) => {
                return self.traverse_effect(inner, field, ops, left, right, span);
            }
            FieldUsage::Inert => return None,
        };
        Some(apply_expr(function, field.clone(), span))
    }

    /// A traversing function `x -> f y` for a nested occurrence.
    fn traverse_function(
        &mut self,
        usage: &FieldUsage,
        ops: &TraverseOps,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        Some(match usage {
            FieldUsage::Record(fields) => {
                let binder = self.fresh_deriving_binder("__derived_record", span);
                let body = self.traverse_record(
                    fields,
                    &local_expr(binder.id, span),
                    ops,
                    left,
                    right,
                    span,
                )?;
                lambda(binder, body, span)
            }
            FieldUsage::Param => right.clone(),
            FieldUsage::LParam => left.clone(),
            FieldUsage::Mono(inner) => apply_expr(
                global_expr(ops.traverse?, span),
                self.traverse_function(inner, ops, left, right, span)?,
                span,
            ),
            FieldUsage::Bi(l, r) => apply_expr(
                apply_expr(
                    global_expr(ops.bitraverse?, span),
                    self.traverse_argument(l, ops, left, right, span)?,
                    span,
                ),
                self.traverse_argument(r, ops, left, right, span)?,
                span,
            ),
            FieldUsage::Inert => global_expr(ops.pure?, span),
            FieldUsage::Contra(inner) | FieldUsage::Pro(inner, _) => {
                self.traverse_function(inner, ops, left, right, span)?
            }
        })
    }

    fn traverse_record(
        &mut self,
        fields: &[(String, FieldUsage)],
        record: &hir::Expr,
        ops: &TraverseOps,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        let mut updates = Vec::new();
        let mut effects = Vec::new();
        let mut binders = Vec::new();
        for (label, usage) in fields {
            if *usage == FieldUsage::Inert {
                continue;
            }
            let binder = self.fresh_deriving_binder("__derived_effect", span);
            updates.push((label.clone(), local_expr(binder.id, span)));
            effects.push(self.traverse_effect(
                usage,
                &field_expr(record.clone(), label, span),
                ops,
                left,
                right,
                span,
            )?);
            binders.push(binder);
        }
        self.combine_traversal(
            record_update(record.clone(), updates, span),
            effects,
            binders,
            ops,
            span,
        )
    }

    /// A traversing argument, using `pure` for an inert side.
    fn traverse_argument(
        &mut self,
        usage: &FieldUsage,
        ops: &TraverseOps,
        left: &hir::Expr,
        right: &hir::Expr,
        span: TextRange,
    ) -> Option<hir::Expr> {
        if *usage == FieldUsage::Inert {
            Some(global_expr(ops.pure?, span))
        } else {
            self.traverse_function(usage, ops, left, right, span)
        }
    }
}

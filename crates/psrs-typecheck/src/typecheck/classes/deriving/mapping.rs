//! Usage-driven generation shared by the four structural mapping classes.
use super::super::super::*;
use super::syntax::{apply_expr, case_expr, constructor_pattern, global_expr, lambda, local_expr};
use super::usage::{MappingClasses, MappingMethods};
use super::{KnownClass, flatten_spine};

impl Checker {
    pub(super) fn derive_mapping_method(
        &mut self,
        known: KnownClass,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(
                TypeCheckErrorKind::InvalidDerivedInstance,
                span,
                "mapping deriving requires one type argument",
            );
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                TypeCheckErrorKind::ExpectedTypeConstructor,
                span,
                "mapping deriving requires a local type constructor",
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(
                TypeCheckErrorKind::CannotFindDerivingType,
                span,
                "cannot find the data declaration to derive a mapping",
            );
        };
        let is_bi = matches!(known, KnownClass::Bifunctor | KnownClass::Profunctor);
        let arity = if is_bi { 2 } else { 1 };
        if type_id.module != self.env.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || declaration.parameters.len() < arity
            || arguments.len() + arity != declaration.parameters.len()
        {
            return self.deriving_error(TypeCheckErrorKind::ExpectedTypeConstructor, span, "mapping deriving requires a local type constructor applied to all but its mapped parameters");
        }
        let left_parameter = is_bi.then(|| {
            declaration.parameters[declaration.parameters.len() - 2]
                .name
                .as_str()
        });
        let right_parameter = &declaration.parameters.last()?.name;
        let usages = match self.validate_field_usage(
            &declaration,
            MappingClasses::covariant(),
            left_parameter,
            right_parameter,
            (
                known == KnownClass::Profunctor,
                known == KnownClass::Contravariant,
            ),
            &arguments,
        ) {
            Ok(usages) => usages,
            Err(offending) => {
                return self.deriving_error(
                    TypeCheckErrorKind::CannotDeriveInvalidConstructorArg,
                    offending,
                    "mapping deriving cannot map a parameter occurrence in this field",
                );
            }
        };
        let methods = MappingMethods {
            mono: self.known_method(KnownClass::Functor, "map"),
            bi: self.known_method(KnownClass::Bifunctor, "bimap"),
            contra: self.known_method(KnownClass::Contravariant, "cmap"),
            pro: self.known_method(KnownClass::Profunctor, "dimap"),
            pro_left: self.known_method(KnownClass::Profunctor, "lcmap"),
        };
        let left = self.fresh_deriving_binder("__derived_f", span);
        let right = if is_bi {
            self.fresh_deriving_binder("__derived_g", span)
        } else {
            left.clone()
        };
        let left_expr = local_expr(left.id, span);
        let right_expr = local_expr(right.id, span);
        let value = self.fresh_deriving_binder("__derived_value", span);
        let mut branches = Vec::new();
        for (constructor, usages) in declaration.constructors.iter().zip(usages) {
            let binders = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_field", span))
                .collect::<Vec<_>>();
            let mut result = global_expr(constructor.symbol, span);
            for (binder, usage) in binders.iter().zip(usages) {
                let field = local_expr(binder.id, span);
                let Some(mapped) =
                    self.map_field_usage(&usage, &methods, &left_expr, &right_expr, &field, span)
                else {
                    return self.deriving_error(
                        TypeCheckErrorKind::CannotFindDerivingType,
                        span,
                        "cannot find the class method required by the checked field usage",
                    );
                };
                result = apply_expr(result, mapped, span);
            }
            branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
                pattern: constructor_pattern(constructor, &binders, span),
                value: result,
                span,
            });
        }
        let mut body = lambda(
            value.clone(),
            case_expr(local_expr(value.id, span), branches, span),
            span,
        );
        if is_bi {
            body = lambda(right, body, span);
        }
        let implementation = lambda(left, body, span);
        self.infer_derived_method(method, class_arguments, &implementation)
    }
}

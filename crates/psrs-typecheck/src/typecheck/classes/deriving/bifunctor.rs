use super::super::super::*;
use super::types::{contains_parameter, flatten_type_application};
use super::{apply_expr, flatten_spine, global_expr, local_expr};

struct BifunctorFieldMap<'a> {
    left_parameter: &'a str,
    right_parameter: &'a str,
    left_mapper: &'a hir::Expr,
    right_mapper: &'a hir::Expr,
    bimap_method: SymbolId,
    span: TextRange,
}

impl Checker {
    pub(super) fn derive_bifunctor_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(span, "Bifunctor deriving requires one type argument");
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self
                .deriving_error(span, "Bifunctor deriving requires a local type constructor");
        };
        let Some(declaration) = self.type_declarations.get(type_id).cloned() else {
            return self
                .deriving_error(span, "cannot find the data declaration to derive Bifunctor");
        };
        if type_id.module != self.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || declaration.parameters.len() < 2
            || arguments.len() + 2 != declaration.parameters.len()
        {
            return self.deriving_error(
                span,
                "Bifunctor deriving requires a local type constructor applied to all but its final two parameters",
            );
        }
        if method.name != "bimap" {
            return self.deriving_error(span, "Bifunctor deriving requires a bimap method");
        }
        let left_parameter = &declaration.parameters[declaration.parameters.len() - 2].name;
        let right_parameter = &declaration.parameters[declaration.parameters.len() - 1].name;
        let left_mapper = self.fresh_deriving_binder("__derived_lmap", span);
        let right_mapper = self.fresh_deriving_binder("__derived_rmap", span);
        let left_mapper_expr = local_expr(left_mapper.id, span);
        let right_mapper_expr = local_expr(right_mapper.id, span);
        let field_map = BifunctorFieldMap {
            left_parameter,
            right_parameter,
            left_mapper: &left_mapper_expr,
            right_mapper: &right_mapper_expr,
            bimap_method: method.symbol,
            span,
        };
        let value = self.fresh_deriving_binder("__derived_value", span);
        let mut branches = Vec::with_capacity(declaration.constructors.len());
        for constructor in &declaration.constructors {
            let binders = constructor
                .fields
                .iter()
                .map(|_| self.fresh_deriving_binder("__derived_field", span))
                .collect::<Vec<_>>();
            let mut result = global_expr(constructor.symbol, span);
            for (field_type, binder) in constructor.fields.iter().zip(&binders) {
                let field_type = self.normalize_deriving_type(field_type);
                let field_value = local_expr(binder.id, span);
                let mapped = match map_bifunctor_field(&field_type, &field_map, &field_value) {
                    Ok(mapped) => mapped,
                    Err(message) => return self.deriving_error(span, &message),
                };
                result = apply_expr(result, mapped, span);
            }
            branches.push(hir::CaseBranch {
                pattern: hir::Pattern {
                    kind: hir::PatternKind::Constructor {
                        symbol: constructor.symbol,
                        name_span: constructor.name_span,
                        arguments: binders
                            .into_iter()
                            .map(|binder| hir::Pattern {
                                kind: hir::PatternKind::Var(binder),
                                span,
                            })
                            .collect(),
                    },
                    span,
                },
                value: result,
                span,
            });
        }
        let implementation = hir::Expr {
            kind: hir::ExprKind::Lambda {
                binder: left_mapper.clone(),
                body: Box::new(hir::Expr {
                    kind: hir::ExprKind::Lambda {
                        binder: right_mapper.clone(),
                        body: Box::new(hir::Expr {
                            kind: hir::ExprKind::Lambda {
                                binder: value.clone(),
                                body: Box::new(hir::Expr {
                                    kind: hir::ExprKind::Case {
                                        scrutinee: Box::new(local_expr(value.id, span)),
                                        branches,
                                    },
                                    span,
                                }),
                            },
                            span,
                        }),
                    },
                    span,
                }),
            },
            span,
        };
        self.infer_expr(&implementation)
    }
}

fn map_bifunctor_field(
    ty: &hir::Type,
    context: &BifunctorFieldMap<'_>,
    value: &hir::Expr,
) -> Result<hir::Expr, String> {
    let BifunctorFieldMap {
        left_parameter,
        right_parameter,
        left_mapper,
        right_mapper,
        bimap_method,
        span,
    } = *context;
    let uses_left = contains_parameter(ty, left_parameter);
    let uses_right = contains_parameter(ty, right_parameter);
    if !uses_left && !uses_right {
        return Ok(value.clone());
    }
    if matches!(&ty.kind, hir::TypeKind::Variable(name) if name == left_parameter) {
        return Ok(apply_expr(left_mapper.clone(), value.clone(), span));
    }
    if matches!(&ty.kind, hir::TypeKind::Variable(name) if name == right_parameter) {
        return Ok(apply_expr(right_mapper.clone(), value.clone(), span));
    }
    let (head, arguments) = flatten_type_application(ty);
    if !contains_parameter(head, left_parameter)
        && !contains_parameter(head, right_parameter)
        && arguments.len() >= 2
        && matches!(&arguments[arguments.len() - 2].kind, hir::TypeKind::Variable(name) if name == left_parameter)
        && matches!(&arguments[arguments.len() - 1].kind, hir::TypeKind::Variable(name) if name == right_parameter)
        && arguments[..arguments.len() - 2].iter().all(|argument| {
            !contains_parameter(argument, left_parameter)
                && !contains_parameter(argument, right_parameter)
        })
    {
        return Ok(apply_expr(
            apply_expr(
                apply_expr(global_expr(bimap_method, span), left_mapper.clone(), span),
                right_mapper.clone(),
                span,
            ),
            value.clone(),
            span,
        ));
    }
    if uses_left && !uses_right {
        return Err(
            "Bifunctor deriving does not support this nested left-parameter occurrence".to_owned(),
        );
    }
    if uses_right && !uses_left {
        return Err(
            "Bifunctor deriving does not support this nested right-parameter occurrence".to_owned(),
        );
    }
    Err("Bifunctor deriving does not support this parameter occurrence".to_owned())
}

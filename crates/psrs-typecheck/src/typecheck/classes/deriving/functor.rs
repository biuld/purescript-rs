use super::super::super::*;
use super::types::contains_parameter;
use super::{apply_expr, flatten_spine, global_expr, local_expr};

impl Checker {
    pub(super) fn derive_functor_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(span, "Functor deriving requires one type argument");
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(span, "Functor deriving requires a local type constructor");
        };
        let Some(declaration) = self.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(span, "cannot find the data declaration to derive Functor");
        };
        if type_id.module != self.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || declaration.parameters.is_empty()
            || arguments.len() + 1 != declaration.parameters.len()
        {
            return self.deriving_error(
                span,
                "Functor deriving requires a local type constructor applied to all but its final parameter",
            );
        }
        if method.name != "map" {
            return self.deriving_error(span, "Functor deriving requires a map method");
        }

        let parameter = declaration
            .parameters
            .last()
            .map(|parameter| parameter.name.as_str())?;
        let mapper = self.fresh_deriving_binder("__derived_map", span);
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
                let input = local_expr(binder.id, span);
                let field_type = self.normalize_deriving_type(field_type);
                let mapped = match map_field_value(
                    &field_type,
                    parameter,
                    &local_expr(mapper.id, span),
                    &input,
                    method.symbol,
                    span,
                ) {
                    Ok(mapped) => mapped,
                    Err(message) => return self.deriving_error(span, &message),
                };
                result = apply_expr(result, mapped, span);
            }
            branches.push(hir::CaseBranch {
                coverage: hir::CaseBranchCoverage::Source,
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
                binder: mapper.clone(),
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
        };
        self.infer_derived_method(method, class_arguments, &implementation)
    }
}

fn map_field_value(
    ty: &hir::Type,
    parameter: &str,
    mapper: &hir::Expr,
    value: &hir::Expr,
    map_method: SymbolId,
    span: TextRange,
) -> Result<hir::Expr, String> {
    if !contains_parameter(ty, parameter) {
        return Ok(value.clone());
    }
    if matches!(&ty.kind, hir::TypeKind::Variable(name) if name == parameter) {
        return Ok(apply_expr(mapper.clone(), value.clone(), span));
    }
    match &ty.kind {
        hir::TypeKind::Application(function, argument)
            if contains_parameter(argument, parameter)
                && !contains_parameter(function, parameter) =>
        {
            let mapper = mapping_function(argument, parameter, mapper, map_method, span)?;
            Ok(apply_expr(
                apply_expr(global_expr(map_method, span), mapper, span),
                value.clone(),
                span,
            ))
        }
        hir::TypeKind::Function {
            parameter: input,
            result,
        } if !contains_parameter(input, parameter) && contains_parameter(result, parameter) => {
            let mapper = mapping_function(result, parameter, mapper, map_method, span)?;
            Ok(apply_expr(
                apply_expr(global_expr(map_method, span), mapper, span),
                value.clone(),
                span,
            ))
        }
        hir::TypeKind::Function {
            parameter: input, ..
        } if contains_parameter(input, parameter) => {
            Err("Functor deriving cannot map a type parameter in a function input".to_owned())
        }
        hir::TypeKind::Application(_, _) => Err(
            "Functor deriving cannot map a type parameter in a higher-kinded application head"
                .to_owned(),
        ),
        hir::TypeKind::Record { .. } | hir::TypeKind::Row { .. } => {
            Err("Functor deriving does not support a record containing its parameter".to_owned())
        }
        _ => Err("Functor deriving does not support this parameter occurrence".to_owned()),
    }
}

fn mapping_function(
    ty: &hir::Type,
    parameter: &str,
    mapper: &hir::Expr,
    map_method: SymbolId,
    span: TextRange,
) -> Result<hir::Expr, String> {
    if matches!(&ty.kind, hir::TypeKind::Variable(name) if name == parameter) {
        return Ok(mapper.clone());
    }
    if !contains_parameter(ty, parameter) {
        return Err("Functor deriving expected a parameter-containing field type".to_owned());
    }
    match &ty.kind {
        hir::TypeKind::Application(function, argument)
            if contains_parameter(argument, parameter)
                && !contains_parameter(function, parameter) =>
        {
            let inner = mapping_function(argument, parameter, mapper, map_method, span)?;
            Ok(apply_expr(global_expr(map_method, span), inner, span))
        }
        hir::TypeKind::Function {
            parameter: input,
            result,
        } if !contains_parameter(input, parameter) && contains_parameter(result, parameter) => {
            let result_mapper = mapping_function(result, parameter, mapper, map_method, span)?;
            Ok(apply_expr(
                global_expr(map_method, span),
                result_mapper,
                span,
            ))
        }
        _ => Err("Functor deriving does not support this nested parameter occurrence".to_owned()),
    }
}

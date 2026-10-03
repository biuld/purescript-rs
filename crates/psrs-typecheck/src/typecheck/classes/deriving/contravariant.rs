use super::super::super::*;
use super::types::{contains_parameter, flatten_type_application};
use super::{apply_expr, flatten_spine, global_expr, local_expr};

impl Checker {
    pub(super) fn derive_contravariant_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        let Some(instance_type) = class_arguments.first() else {
            return self.deriving_error(span, "Contravariant deriving requires one type argument");
        };
        let instance_type = self.resolve_type(instance_type.clone());
        let (head, arguments) = flatten_spine(&instance_type);
        let InferType::Constructor(TypeConstructor::User(type_id)) = head else {
            return self.deriving_error(
                span,
                "Contravariant deriving requires a local type constructor",
            );
        };
        let Some(declaration) = self.env.type_declarations.get(type_id).cloned() else {
            return self.deriving_error(
                span,
                "cannot find the data declaration to derive Contravariant",
            );
        };
        if type_id.module != self.env.module_id
            || !matches!(
                declaration.kind,
                hir::TypeDeclarationKind::Data | hir::TypeDeclarationKind::Newtype
            )
            || declaration.parameters.is_empty()
            || arguments.len() + 1 != declaration.parameters.len()
        {
            return self.deriving_error(
                span,
                "Contravariant deriving requires a local type constructor applied to all but its final parameter",
            );
        }
        if method.name != "cmap" {
            return self.deriving_error(span, "Contravariant deriving requires a cmap method");
        }
        let parameter = &declaration.parameters.last()?.name;
        let profunctor_lcmap = self.profunctor_lcmap_symbol();
        let mapper = self.fresh_deriving_binder("__derived_contramap", span);
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
                let mapped = match contramap_field_value(
                    &field_type,
                    parameter,
                    &local_expr(mapper.id, span),
                    &field_value,
                    profunctor_lcmap,
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

    fn profunctor_lcmap_symbol(&self) -> Option<SymbolId> {
        self.env.classes.iter().find_map(|(class_id, class)| {
            (self.env.type_names.get(class_id).map(String::as_str) == Some("Profunctor")
                && self.env.type_modules.get(class_id).map(String::as_str)
                    == Some("Data.Profunctor"))
            .then(|| {
                class
                    .methods
                    .iter()
                    .find(|method| method.name == "lcmap")
                    .map(|method| method.symbol)
            })
            .flatten()
        })
    }
}

fn contramap_field_value(
    ty: &hir::Type,
    parameter: &str,
    mapper: &hir::Expr,
    value: &hir::Expr,
    profunctor_lcmap: Option<SymbolId>,
    span: TextRange,
) -> Result<hir::Expr, String> {
    if !contains_parameter(ty, parameter) {
        return Ok(value.clone());
    }
    if let Some(lcmap) =
        contravariant_profunctor_application(ty, parameter, mapper, value, profunctor_lcmap, span)?
    {
        return Ok(lcmap);
    }
    if let hir::TypeKind::Function {
        parameter: input,
        result,
    } = &ty.kind
        && matches!(&input.kind, hir::TypeKind::Variable(name) if name == parameter)
        && !contains_parameter(result, parameter)
    {
        let Some(lcmap) = profunctor_lcmap else {
            return Err(
                "Contravariant deriving through a function requires Data.Profunctor.lcmap".into(),
            );
        };
        return Ok(apply_expr(
            apply_expr(global_expr(lcmap, span), mapper.clone(), span),
            value.clone(),
            span,
        ));
    }
    Err("Contravariant deriving requires each occurrence to be a direct function input".into())
}

fn contravariant_profunctor_application(
    ty: &hir::Type,
    parameter: &str,
    mapper: &hir::Expr,
    value: &hir::Expr,
    profunctor_lcmap: Option<SymbolId>,
    span: TextRange,
) -> Result<Option<hir::Expr>, String> {
    let (head, arguments) = flatten_type_application(ty);
    if arguments.len() < 2
        || contains_parameter(head, parameter)
        || !matches!(
            &arguments[arguments.len() - 2].kind,
            hir::TypeKind::Variable(name) if name == parameter
        )
        || arguments[..arguments.len() - 2]
            .iter()
            .any(|argument| contains_parameter(argument, parameter))
        || arguments[arguments.len() - 1..]
            .iter()
            .any(|argument| contains_parameter(argument, parameter))
    {
        return Ok(None);
    }
    let Some(lcmap) = profunctor_lcmap else {
        return Err(
            "Contravariant deriving through a profunctor requires Data.Profunctor.lcmap".into(),
        );
    };
    Ok(Some(apply_expr(
        apply_expr(global_expr(lcmap, span), mapper.clone(), span),
        value.clone(),
        span,
    )))
}

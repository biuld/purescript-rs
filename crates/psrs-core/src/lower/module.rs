use crate::{Declaration, LowerError, Module, Type, TypeId};
use std::collections::HashMap;

fn lower_type_constructor(constructor: psrs_thir::TypeConstructor) -> crate::TypeConstructor {
    match constructor {
        psrs_thir::TypeConstructor::Function => crate::TypeConstructor::Function,
        psrs_thir::TypeConstructor::Record => crate::TypeConstructor::Record,
        psrs_thir::TypeConstructor::Row => crate::TypeConstructor::Row,
        psrs_thir::TypeConstructor::Array => crate::TypeConstructor::Array,
        psrs_thir::TypeConstructor::Int => crate::TypeConstructor::Int,
        psrs_thir::TypeConstructor::Number => crate::TypeConstructor::Number,
        psrs_thir::TypeConstructor::Boolean => crate::TypeConstructor::Boolean,
        psrs_thir::TypeConstructor::String => crate::TypeConstructor::String,
        psrs_thir::TypeConstructor::Char => crate::TypeConstructor::Char,
        psrs_thir::TypeConstructor::Unit => crate::TypeConstructor::Unit,
        psrs_thir::TypeConstructor::Type => crate::TypeConstructor::Type,
        psrs_thir::TypeConstructor::Constraint => crate::TypeConstructor::Constraint,
        psrs_thir::TypeConstructor::Symbol => crate::TypeConstructor::Symbol,
        psrs_thir::TypeConstructor::User(id) => crate::TypeConstructor::User(id),
    }
}

pub(super) fn lower_module_inner(module: psrs_thir::Module) -> Result<Module, Vec<LowerError>> {
    if let Err(errors) = module.verify() {
        return Err(errors
            .into_iter()
            .map(|error| LowerError {
                span: error.span,
                message: "invalid THIR input",
            })
            .collect());
    }
    let externals = module
        .externals
        .iter()
        .map(|external| (external.symbol, external.kind.clone()))
        .collect::<HashMap<_, _>>();
    let constructors = module
        .constructors
        .iter()
        .map(|constructor| (constructor.symbol, constructor.clone()))
        .collect::<HashMap<_, _>>();
    let source_types = module.types;
    let types = source_types
        .iter()
        .cloned()
        .map(|ty| match ty {
            psrs_thir::Type::Variable(variable) => Type::Variable(variable),
            psrs_thir::Type::Constructor(constructor) => {
                Type::Constructor(lower_type_constructor(constructor))
            }
            psrs_thir::Type::Application(function, argument) => {
                Type::Application(TypeId(function.0), TypeId(argument.0))
            }
            psrs_thir::Type::ForAll { variables, body } => Type::ForAll {
                variables,
                body: TypeId(body.0),
            },
            psrs_thir::Type::RowEmpty => Type::RowEmpty,
            psrs_thir::Type::RowExtend { label, ty, tail } => Type::RowExtend {
                label,
                ty: TypeId(ty.0),
                tail: TypeId(tail.0),
            },
            psrs_thir::Type::TypeLevelString(value) => Type::TypeLevelString(value),
            psrs_thir::Type::TypeLevelInt(value) => Type::TypeLevelInt(value),
        })
        .collect();
    let mut declarations = Vec::with_capacity(module.declarations.len());
    for declaration in module.declarations {
        let value = super::lower_expr(declaration.value, &externals, &constructors, &source_types)
            .map_err(|error| vec![error])?;
        declarations.push(Declaration {
            symbol: declaration.symbol,
            name: declaration.name,
            name_span: declaration.name_span,
            quantified: declaration.quantified,
            ty: TypeId(declaration.ty.0),
            value,
            span: declaration.span,
        });
    }
    let lowered = Module {
        id: module.id,
        name: module.name,
        externals: module.externals,
        types,
        newtype_ids: module.newtype_ids,
        opaque_ids: module.opaque_ids,
        callable_types: module.callable_types,
        constructors: module
            .constructors
            .iter()
            .map(|constructor| crate::ConstructorInfo {
                symbol: constructor.symbol,
                name: constructor.name.clone(),
                type_id: constructor.type_id,
                tag: constructor.tag,
                field_count: constructor.field_count,
                field_types: constructor
                    .field_types
                    .iter()
                    .map(|field| TypeId(field.0))
                    .collect(),
                parameters: constructor.parameters.clone(),
            })
            .collect(),
        declarations,
        type_names: module.type_names,
        entry: None,
        span: module.span,
    };
    Ok(lowered)
}

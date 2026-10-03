use crate::{Module, Type, TypeId, VerifyError};
use psrs_hir::LocalId;
use std::collections::HashMap;

mod expr;
mod patterns;
mod scopes;
mod types;

pub(crate) use types::equivalent_types;

use expr::verify_expr;
use patterns::verify_pattern;
use types::{
    array_element, compatible, error, primitive_type_id, record_field, restore_local, verify_type,
};

#[derive(Clone)]
pub(super) struct SchemeType {
    pub ty: TypeId,
    pub quantified: Vec<psrs_hir::TypeVariableId>,
}

type Locals = HashMap<LocalId, SchemeType>;

pub(crate) fn module(module: &Module) -> Result<(), Vec<VerifyError>> {
    let globals = module
        .declarations
        .iter()
        .map(|declaration| {
            (
                declaration.symbol,
                Some(SchemeType {
                    ty: declaration.ty,
                    quantified: declaration.quantified.clone(),
                }),
            )
        })
        .chain(
            module
                .externals
                .iter()
                .map(|external| (external.symbol, None)),
        )
        .collect::<HashMap<_, _>>();
    let mut errors = Vec::new();
    for (index, ty) in module.types.iter().enumerate() {
        let id = TypeId(index as u32);
        match ty {
            Type::Application(parameter, result) => {
                verify_type(*parameter, module, module.id, module.span, &mut errors);
                verify_type(*result, module, module.id, module.span, &mut errors);
                verify_type(id, module, module.id, module.span, &mut errors);
            }
            Type::ForAll { body, .. } => {
                verify_type(*body, module, module.id, module.span, &mut errors);
            }
            Type::RowExtend { ty, tail, .. } => {
                verify_type(*ty, module, module.id, module.span, &mut errors);
                verify_type(*tail, module, module.id, module.span, &mut errors);
            }
            Type::Closure { parameters, result } => {
                for parameter in parameters {
                    verify_type(*parameter, module, module.id, module.span, &mut errors);
                }
                verify_type(*result, module, module.id, module.span, &mut errors);
            }
            _ => {}
        }
    }
    errors.extend(scopes::verify_module(module));
    if !errors.is_empty() {
        return Err(errors);
    }
    for constructor in &module.constructors {
        if module.opaque_ids.contains(&constructor.type_id) {
            errors.push(error(
                module.id,
                module.span,
                "an opaque type has no constructors",
            ));
        }
    }
    for declaration in &module.declarations {
        let owner = declaration.symbol.module;
        verify_type(
            declaration.ty,
            module,
            owner,
            declaration.name_span,
            &mut errors,
        );
        let mut locals = Locals::new();
        verify_expr(
            &declaration.value,
            Some(declaration.ty),
            module,
            owner,
            &globals,
            &mut locals,
            &mut errors,
        );
    }
    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
}

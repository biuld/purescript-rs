use crate::{Module, Type, TypeId, VerifyError};
use psrs_hir::LocalId;
use std::collections::HashMap;

mod expr;
mod patterns;
mod types;

use expr::verify_expr;
use patterns::verify_pattern;
use types::{
    array_element, compatible, error, primitive_types, record_field, restore_local, type_id_for,
    unary_primitive_types, user_type_constructor, verify_type,
};

type Locals = HashMap<LocalId, TypeId>;

pub(crate) fn module(module: &Module) -> Result<(), Vec<VerifyError>> {
    let globals = module
        .declarations
        .iter()
        .map(|declaration| (declaration.symbol, Some(declaration.ty)))
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
            Type::Function { parameter, result } | Type::Application(parameter, result) => {
                verify_type(*parameter, module, module.id, module.span, &mut errors);
                verify_type(*result, module, module.id, module.span, &mut errors);
                if matches!(ty, Type::Application(..)) {
                    verify_type(id, module, module.id, module.span, &mut errors);
                }
            }
            Type::Record(fields) => {
                for (_, field) in fields {
                    verify_type(*field, module, module.id, module.span, &mut errors);
                }
            }
            Type::OpenRecord { fields, tail } => {
                for (_, field) in fields {
                    verify_type(*field, module, module.id, module.span, &mut errors);
                }
                verify_type(*tail, module, module.id, module.span, &mut errors);
            }
            _ => {}
        }
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

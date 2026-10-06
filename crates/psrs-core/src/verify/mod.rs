use crate::{Module, Type, TypeId, VerifyError};
use psrs_hir::LocalId;
use std::collections::HashMap;

mod expr;
mod patterns;
mod scopes;
mod types;

pub(crate) use types::equivalent_types;
pub(crate) use types::instantiation;

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

pub(crate) fn module(module: &Module, source: Option<&Module>) -> Result<(), Vec<VerifyError>> {
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
        .chain(module.externals.iter().map(|external| {
            let signature = module
                .external_types
                .iter()
                .find(|checked| checked.symbol == external.symbol)
                .and_then(|checked| external_scheme(module, checked.ty));
            (external.symbol, signature)
        }))
        .collect::<HashMap<_, _>>();
    let mut errors = Vec::new();
    let mut external_type_symbols = std::collections::HashSet::new();
    for external_type in &module.external_types {
        let span = module
            .externals
            .iter()
            .find(|external| external.symbol == external_type.symbol)
            .and_then(|external| external.signature.as_ref())
            .map_or(module.span, |signature| signature.span);
        if !external_type_symbols.insert(external_type.symbol) {
            errors.push(error(
                external_type.source_module,
                span,
                "a foreign symbol has more than one checked signature",
            ));
        }
        if !module
            .externals
            .iter()
            .any(|external| external.symbol == external_type.symbol)
        {
            errors.push(error(
                external_type.source_module,
                span,
                "a checked foreign signature has no external declaration",
            ));
        }
        verify_type(
            external_type.ty,
            module,
            external_type.source_module,
            span,
            &mut errors,
        );
    }
    for external in &module.externals {
        if external.kind.requires_checked_signature()
            && !external_type_symbols.contains(&external.symbol)
        {
            errors.push(error(
                external.symbol.module,
                external
                    .signature
                    .as_ref()
                    .map_or(module.span, |ty| ty.span),
                "a foreign declaration has no checked signature",
            ));
        }
    }
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
            source,
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

fn external_scheme(module: &Module, ty: TypeId) -> Option<SchemeType> {
    let mut current = ty;
    let mut quantified = Vec::new();
    let mut seen = std::collections::HashSet::new();
    while seen.insert(current) {
        let Some((variables, body)) = crate::forall_parts(&module.types, current) else {
            return Some(SchemeType {
                ty: current,
                quantified,
            });
        };
        quantified.extend_from_slice(variables);
        current = body;
    }
    None
}

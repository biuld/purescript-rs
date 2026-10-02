use super::{
    Locals, SchemeType, array_element, compatible, error, primitive_type_id, record_field,
    verify_type,
};
use crate::{Literal, Module, Pattern, PatternKind, TypeConstructor, TypeId, VerifyError};
use psrs_hir::ModuleId;

pub(super) fn verify_pattern(
    pattern: &Pattern,
    scrutinee_type: TypeId,
    module: &Module,
    owner: ModuleId,
    locals: &mut Locals,
    errors: &mut Vec<VerifyError>,
) {
    verify_type(pattern.ty, module, owner, pattern.span, errors);
    compatible(
        pattern.ty,
        scrutinee_type,
        module,
        owner,
        pattern.span,
        errors,
    );
    match &pattern.kind {
        PatternKind::Wildcard => {}
        PatternKind::Var { id, ty } => {
            compatible(*ty, pattern.ty, module, owner, pattern.span, errors);
            bind_local(*id, *ty, pattern.span, owner, locals, errors);
        }
        PatternKind::Literal { value } => {
            let constructor = match value {
                Literal::Integer(_) => TypeConstructor::Int,
                Literal::Number(_) => TypeConstructor::Number,
                Literal::String(_) => TypeConstructor::String,
                Literal::Char(_) => TypeConstructor::Char,
                Literal::Boolean(_) => TypeConstructor::Boolean,
            };
            let expected = primitive_type_id(module, constructor);
            compatible(expected, pattern.ty, module, owner, pattern.span, errors);
            if let Literal::Number(value) = value
                && value
                    .parse::<f64>()
                    .ok()
                    .is_none_or(|number| !number.is_finite())
            {
                errors.push(error(
                    owner,
                    pattern.span,
                    "number pattern literal is not a finite number",
                ));
            }
        }
        PatternKind::Array { elements } => {
            let Some(element_type) = array_element(pattern.ty, module) else {
                errors.push(error(
                    owner,
                    pattern.span,
                    "array pattern type is not an array",
                ));
                return;
            };
            for element in elements {
                verify_pattern(element, element_type, module, owner, locals, errors);
            }
        }
        PatternKind::Named {
            id,
            pattern: nested,
        } => {
            bind_local(*id, pattern.ty, pattern.span, owner, locals, errors);
            verify_pattern(nested, pattern.ty, module, owner, locals, errors);
        }
        PatternKind::Constructor { symbol, arguments } => {
            let Some(constructor) = module
                .constructors
                .iter()
                .find(|candidate| candidate.symbol == *symbol)
            else {
                errors.push(error(
                    owner,
                    pattern.span,
                    "pattern constructor is not declared",
                ));
                return;
            };
            if constructor.field_count != arguments.len() {
                errors.push(error(
                    owner,
                    pattern.span,
                    "pattern constructor has the wrong field count",
                ));
            }
            let Some((result_constructor, type_arguments)) = module.applied_constructor(pattern.ty)
            else {
                errors.push(error(
                    owner,
                    pattern.span,
                    "pattern constructor type is not an applied user type",
                ));
                return;
            };
            if result_constructor != crate::TypeConstructor::User(constructor.type_id) {
                errors.push(error(
                    owner,
                    pattern.span,
                    "pattern constructor type does not match its parent type",
                ));
            }
            let field_instances = arguments
                .iter()
                .map(|argument| argument.ty)
                .collect::<Vec<_>>();
            if !super::types::constructor_fields_match(
                module,
                &constructor.parameters,
                &type_arguments,
                &constructor.field_types,
                &field_instances,
            ) {
                errors.push(error(
                    owner,
                    pattern.span,
                    "constructor pattern fields do not match one consistent type instantiation",
                ));
            }
            for argument in arguments {
                verify_pattern(argument, argument.ty, module, owner, locals, errors);
            }
        }
        PatternKind::Record { fields } => {
            for (label, field) in fields {
                let Some(field_type) = record_field(pattern.ty, label, module) else {
                    errors.push(error(
                        owner,
                        field.span,
                        "record pattern field is not declared",
                    ));
                    continue;
                };
                verify_pattern(field, field_type, module, owner, locals, errors);
            }
        }
    }
}

fn bind_local(
    id: psrs_hir::LocalId,
    ty: TypeId,
    span: psrs_span::TextRange,
    owner: ModuleId,
    locals: &mut Locals,
    errors: &mut Vec<VerifyError>,
) {
    if locals.contains_key(&id) {
        errors.push(error(
            owner,
            span,
            "pattern local ID is already bound in this scope",
        ));
        return;
    }
    locals.insert(
        id,
        SchemeType {
            ty,
            quantified: Vec::new(),
        },
    );
}

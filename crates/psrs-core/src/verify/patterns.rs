use super::{Locals, SchemeType, compatible, error, record_field, verify_type};
use crate::{Module, Pattern, PatternKind, TypeId, VerifyError};
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
            locals.insert(
                *id,
                SchemeType {
                    ty: *ty,
                    quantified: Vec::new(),
                },
            );
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

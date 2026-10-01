use super::TypeMatcher;
use crate::{Module, Type, TypeId};
use std::collections::{HashMap, HashSet};

pub(in crate::verify) fn constructor_fields_match(
    module: &Module,
    parameters: &[psrs_hir::TypeVariableId],
    type_arguments: &[TypeId],
    field_templates: &[TypeId],
    field_instances: &[TypeId],
) -> bool {
    if parameters.len() != type_arguments.len() || field_templates.len() != field_instances.len() {
        return false;
    }
    let mut matcher = TypeMatcher {
        module,
        flexible: parameters.iter().copied().collect(),
        replacements: HashMap::new(),
        row_forms: HashMap::new(),
        alpha: HashMap::new(),
        active: HashSet::new(),
    };
    // A parameter may never appear as its own type node. Nullary constructors
    // still quantify it, and the pattern's type arguments are that instantiation.
    for (parameter, argument) in parameters.iter().zip(type_arguments) {
        if matches!(
            module.types.get(argument.0 as usize),
            Some(Type::ForAll { .. })
        ) {
            return false;
        }
        if !matcher.bind_flexible(*parameter, *argument) {
            return false;
        }
    }
    field_templates
        .iter()
        .zip(field_instances)
        .all(|(template, instance)| matcher.matches(*template, *instance, false))
}

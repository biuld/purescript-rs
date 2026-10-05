use super::TypeMatcher;
use crate::{Instantiation, Module, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

pub(crate) fn instantiation<'a>(
    module: &'a Module,
    scheme: TypeId,
    quantified: &[TypeVariableId],
    instance: TypeId,
) -> Option<Instantiation<'a>> {
    let mut matcher = TypeMatcher {
        module,
        flexible: quantified.iter().copied().collect(),
        replacements: HashMap::new(),
        row_forms: HashMap::new(),
        alpha: HashMap::new(),
        active: HashSet::new(),
    };
    if !matcher.subsumes(scheme, instance, true) {
        return None;
    }
    Some(Instantiation {
        module,
        replacements: matcher.replacements,
    })
}

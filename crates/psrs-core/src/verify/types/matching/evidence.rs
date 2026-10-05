use super::{TypeMatcher, Variance};
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
    if !matcher.relate(scheme, instance, Variance::Subsumption, true) {
        return None;
    }
    Some(Instantiation {
        module,
        replacements: matcher.replacements,
    })
}

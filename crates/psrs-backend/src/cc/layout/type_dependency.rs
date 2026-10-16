use super::CoreModule;
use psrs_core::{Type, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

pub(super) fn depends_on_type_variable(module: &CoreModule, id: TypeId) -> bool {
    fn visit(
        module: &CoreModule,
        id: TypeId,
        bound: &mut HashMap<TypeVariableId, usize>,
        visiting: &mut HashSet<TypeId>,
    ) -> bool {
        if !visiting.insert(id) {
            return false;
        }
        let result = match module.types.get(id.0 as usize) {
            Some(Type::Variable(variable)) => !bound.contains_key(variable),
            Some(Type::Application(function, argument)) => {
                visit(module, *function, bound, visiting)
                    || visit(module, *argument, bound, visiting)
            }
            Some(Type::ForAll { variables, body }) => {
                for variable in variables {
                    *bound.entry(*variable).or_default() += 1;
                }
                let dependent = visit(module, *body, bound, visiting);
                for variable in variables {
                    if let Some(count) = bound.get_mut(variable) {
                        *count -= 1;
                        if *count == 0 {
                            bound.remove(variable);
                        }
                    }
                }
                dependent
            }
            Some(Type::RowExtend { ty, tail, .. }) => {
                visit(module, *ty, bound, visiting) || visit(module, *tail, bound, visiting)
            }
            Some(Type::Closure { parameters, result }) => {
                parameters
                    .iter()
                    .any(|parameter| visit(module, *parameter, bound, visiting))
                    || visit(module, *result, bound, visiting)
            }
            Some(Type::RowEmpty) => false,
            _ => false,
        };
        visiting.remove(&id);
        result
    }

    visit(module, id, &mut HashMap::new(), &mut HashSet::new())
}

use super::super::*;
use super::fundeps::covering_sets;
use std::collections::{BTreeSet, HashSet};

impl Checker {
    /// Checks visible instance declarations for overlaps, excluding branches
    /// that share one ordered chain identity. This prevents a later wanted
    /// constraint from turning ordinary declaration order into dispatch.
    pub(super) fn validate_instance_overlaps(&mut self, module: &hir::Module) {
        let instances = self.env.instances.clone();
        for (index, left) in instances.iter().enumerate() {
            for right in instances.iter().skip(index + 1) {
                if left.class_id != right.class_id
                    || (left.symbol.module == right.symbol.module
                        && left.chain_id == right.chain_id)
                {
                    continue;
                }
                let class = &self.env.classes[&left.class_id];
                let covering_sets = covering_sets(class.parameters.len(), &class.fundeps);
                if instances_are_apart(&covering_sets, &left.head_arguments, &right.head_arguments)
                {
                    continue;
                }
                let mut candidate_modules = HashSet::from([module.id, left.class_id.module]);
                for argument in left.head_arguments.iter().chain(&right.head_arguments) {
                    collect_user_type_modules(argument, &mut candidate_modules);
                }
                if !candidate_modules.contains(&left.symbol.module)
                    || !candidate_modules.contains(&right.symbol.module)
                {
                    continue;
                }
                let class_name = self
                    .env
                    .type_names
                    .get(&left.class_id)
                    .cloned()
                    .unwrap_or_else(|| format!("Class{}", left.class_id.index));
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::OverlappingInstances,
                    module.span,
                    format!("overlapping instances of class `{class_name}` are visible here"),
                ));
            }
        }
    }
}

fn collect_user_type_modules(ty: &InferType, modules: &mut HashSet<hir::ModuleId>) {
    match ty {
        InferType::Constructor(TypeConstructor::User(id)) => {
            modules.insert(id.module);
        }
        InferType::Application(function, argument) => {
            collect_user_type_modules(function, modules);
            collect_user_type_modules(argument, modules);
        }
        InferType::RowExtend { ty, tail, .. } => {
            collect_user_type_modules(ty, modules);
            collect_user_type_modules(tail, modules);
        }
        InferType::ForAll { body, .. } => collect_user_type_modules(body, modules),
        InferType::Constrained { constraints, body } => {
            for argument in constraints
                .iter()
                .flat_map(|constraint| &constraint.arguments)
            {
                collect_user_type_modules(argument, modules);
            }
            collect_user_type_modules(body, modules);
        }
        InferType::Variable(_)
        | InferType::Constructor(_)
        | InferType::RowEmpty
        | InferType::TypeLevelString(_)
        | InferType::TypeLevelInt(_) => {}
    }
}

/// The module that declares an argument's leftmost spine-head type
/// constructor: applications recurse into the function side, so an applied
/// constructor names its nominal head's module, while type variables and
/// decided literals name no module. This is `purs`'s `typeModule` inside
/// `TypeChecker.findNonOrphanModules`.
fn leftmost_head_module(ty: &InferType) -> Option<hir::ModuleId> {
    match ty {
        InferType::Constructor(TypeConstructor::User(type_id)) => Some(type_id.module),
        InferType::Application(function, _) => leftmost_head_module(function),
        _ => None,
    }
}

impl Checker {
    /// Reports the official orphan instance error for one local instance
    /// declaration, under the rule `purs` implements in
    /// `TypeChecker.typeCheckAll`: the declaring module is non-orphan when it
    /// declares the instance's class, or when for *every* covering set of the
    /// class's parameters at least one covered head argument's leftmost type
    /// constructor is declared in the declaring module (`purs` intersects the
    /// modules found through each covering set and checks its containment).
    ///
    /// `checkOrphanInstance` throws after head synthesis — synonym resolution
    /// included — and before the instance joins the searchable environment, so
    /// an orphan instance is rejected here and never recorded: it cannot
    /// overlap with another visible instance or submit a derived dictionary,
    /// exactly as the official throw aborts the remaining checks.
    ///
    /// Returns whether the instance was reported as an orphan.
    pub(super) fn report_orphan_instance(
        &mut self,
        instance: &hir::InstanceDeclaration,
        class: &ClassInfo,
        head_arguments: &[InferType],
    ) -> bool {
        let declaring = self.env.module_id;
        if instance.class_id.module == declaring {
            return false;
        }
        let sets = covering_sets(class.parameters.len(), &class.fundeps);
        // The declaring module is non-orphan when every covering set contains
        // a covered argument whose leftmost head constructor it declares; the
        // instance is an orphan only when no covering set does.
        let is_orphan = !sets.iter().all(|set| {
            set.iter().any(|&index| {
                head_arguments
                    .get(index)
                    .and_then(leftmost_head_module)
                    .is_some_and(|module| module == declaring)
            })
        });
        if !is_orphan {
            return false;
        }
        let class_name = self
            .env
            .type_names
            .get(&instance.class_id)
            .cloned()
            .unwrap_or_else(|| format!("Class{}", instance.class_id.index));
        self.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::OrphanInstance,
            instance.span,
            format!(
                "orphan instance of class `{class_name}`: neither the class nor a type \
                 constructor covering its head is declared in this module"
            ),
        ));
        true
    }
}

fn instances_are_apart(
    covering_sets: &BTreeSet<BTreeSet<usize>>,
    left: &[InferType],
    right: &[InferType],
) -> bool {
    !covering_sets.is_empty()
        && covering_sets.iter().all(|set| {
            set.iter().any(|&index| {
                left.get(index)
                    .zip(right.get(index))
                    .is_some_and(|(left, right)| type_heads_apart(left, right))
            })
        })
}

/// Instance heads are apart when at least one nominal constructor or closed
/// row shape cannot unify. A type variable can still match any shape.
fn type_heads_apart(left: &InferType, right: &InferType) -> bool {
    match (left, right) {
        (InferType::Variable(_), _) | (_, InferType::Variable(_)) => false,
        (InferType::Constructor(left), InferType::Constructor(right)) => left != right,
        (InferType::Application(lf, la), InferType::Application(rf, ra)) => {
            type_heads_apart(lf, rf) || type_heads_apart(la, ra)
        }
        (InferType::RowEmpty, InferType::RowEmpty) => false,
        (
            InferType::RowExtend {
                label: left_label,
                ty: left_ty,
                tail: left_tail,
            },
            InferType::RowExtend {
                label: right_label,
                ty: right_ty,
                tail: right_tail,
            },
        ) => {
            left_label != right_label
                || type_heads_apart(left_ty, right_ty)
                || type_heads_apart(left_tail, right_tail)
        }
        // Two decided literals that are equal are the same head; unequal ones
        // cannot unify.
        (InferType::TypeLevelString(l), InferType::TypeLevelString(r)) => l != r,
        (InferType::TypeLevelInt(l), InferType::TypeLevelInt(r)) => l != r,
        _ => true,
    }
}

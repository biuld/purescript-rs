//! Immutable kind templates shared by checked module interfaces.

use crate::{Kind, KindState};
use std::collections::HashMap;

/// Jointly closed kinds. Variable identities are lexical template binders,
/// independent of the solver that produced them. Fields are private so only
/// the kind owner can construct the closure and its parameter list together.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClosedKinds {
    parameters: Vec<u32>,
    kinds: Vec<Kind>,
}

impl KindState {
    /// Resolves and closes a set of kinds jointly, preserving shared parameters
    /// and explicit nested quantifier scopes.
    pub fn close_kinds(&self, kinds: &[Kind]) -> ClosedKinds {
        let mut next = 0;
        let mut renamer = Renamer {
            parameters: HashMap::new(),
            bound: HashMap::new(),
            allow_parameters: true,
            resolve: |id, bound: &HashMap<u32, Kind>| {
                self.resolve_head_with(Kind::Variable(id), |variable| bound.contains_key(&variable))
            },
            allocate: |_| {
                let id = next;
                next += 1;
                Kind::Variable(id)
            },
        };
        let kinds = kinds
            .iter()
            .map(|kind| renamer.rename(kind.clone()))
            .collect();
        let mut parameters = renamer
            .parameters
            .values()
            .map(|kind| match kind {
                Kind::Variable(id) => *id,
                _ => unreachable!("template parameters are variables"),
            })
            .collect::<Vec<_>>();
        parameters.sort_unstable();
        ClosedKinds { parameters, kinds }
    }

    /// Opens one fresh instance of a closed set. All sharing is local to this
    /// instance; nested explicit binders become fresh rigid variables.
    pub fn instantiate_closed_kinds(&mut self, closed: &ClosedKinds) -> Vec<Kind> {
        let parameters = closed
            .parameters
            .iter()
            .map(|id| (*id, self.fresh()))
            .collect();
        let mut renamer = Renamer {
            parameters,
            bound: HashMap::new(),
            allow_parameters: false,
            resolve: |id, _: &HashMap<u32, Kind>| Kind::Variable(id),
            allocate: |rigid| {
                let kind = self.fresh();
                if rigid && let Kind::Variable(id) = kind {
                    self.make_rigid(id);
                }
                kind
            },
        };
        closed
            .kinds
            .iter()
            .cloned()
            .map(|kind| renamer.rename(kind))
            .collect()
    }
}

struct Renamer<F, R> {
    parameters: HashMap<u32, Kind>,
    bound: HashMap<u32, Kind>,
    allow_parameters: bool,
    allocate: F,
    resolve: R,
}

impl<F: FnMut(bool) -> Kind, R: FnMut(u32, &HashMap<u32, Kind>) -> Kind> Renamer<F, R> {
    fn rename(&mut self, kind: Kind) -> Kind {
        match kind {
            Kind::Variable(id) => {
                if let Some(kind) = self.bound.get(&id).or_else(|| self.parameters.get(&id)) {
                    return kind.clone();
                }
                assert!(
                    self.allow_parameters,
                    "closed kind has an unbound template variable"
                );
                let resolved = (self.resolve)(id, &self.bound);
                if resolved != Kind::Variable(id) {
                    return self.rename(resolved);
                }
                let kind = (self.allocate)(false);
                self.parameters.insert(id, kind.clone());
                kind
            }
            Kind::ForAll { variable, body } => {
                let fresh = (self.allocate)(true);
                let Kind::Variable(id) = fresh else {
                    unreachable!("kind allocation produces a variable")
                };
                let previous = self.bound.insert(variable, fresh);
                let body = self.rename(*body);
                match previous {
                    Some(kind) => {
                        self.bound.insert(variable, kind);
                    }
                    None => {
                        self.bound.remove(&variable);
                    }
                }
                Kind::ForAll {
                    variable: id,
                    body: Box::new(body),
                }
            }
            Kind::App(head, argument) => Kind::app(self.rename(*head), self.rename(*argument)),
            Kind::Function(parameter, result) => Kind::Function(
                Box::new(self.rename(*parameter)),
                Box::new(self.rename(*result)),
            ),
            Kind::RowExtend { label, field, tail } => Kind::RowExtend {
                label,
                field: Box::new(self.rename(*field)),
                tail: Box::new(self.rename(*tail)),
            },
            atomic => atomic,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_hir::BuiltinType;
    use psrs_span::TextRange;

    #[test]
    fn instances_preserve_joint_sharing_without_sharing_solver_solutions() {
        let mut producer = KindState::default();
        let variable = producer.fresh();
        let closed = producer.close_kinds(&[variable.clone(), Kind::row(variable)]);
        let mut consumer = KindState::default();
        let first = consumer.instantiate_closed_kinds(&closed);
        let second = consumer.instantiate_closed_kinds(&closed);
        assert_eq!(first[1], Kind::row(first[0].clone()));
        assert_eq!(second[1], Kind::row(second[0].clone()));
        assert_ne!(first[0], second[0]);
        consumer
            .unify(
                first[0].clone(),
                Kind::Builtin(BuiltinType::Symbol),
                TextRange::new(0, 1),
            )
            .unwrap();
        assert_eq!(
            consumer.resolve(first[1].clone()),
            Kind::row(Kind::Builtin(BuiltinType::Symbol))
        );
        assert_eq!(consumer.resolve(second[1].clone()), second[1]);
    }

    #[test]
    fn closing_reads_solutions_and_nested_binders_do_not_capture_parameters() {
        let mut producer = KindState::default();
        let solved = producer.fresh();
        producer
            .unify(
                solved.clone(),
                Kind::Builtin(BuiltinType::Type),
                TextRange::new(0, 1),
            )
            .unwrap();
        let closed = producer.close_kinds(&[
            solved,
            Kind::ForAll {
                variable: 17,
                body: Box::new(Kind::Variable(17)),
            },
            Kind::Variable(17),
        ]);
        let mut consumer = KindState::default();
        let reserved = consumer.fresh();
        let opened = consumer.instantiate_closed_kinds(&closed);
        assert_eq!(opened[0], Kind::Builtin(BuiltinType::Type));
        let Kind::ForAll { variable, body } = &opened[1] else {
            panic!("explicit forall retained")
        };
        assert_eq!(**body, Kind::Variable(*variable));
        assert!(consumer.is_rigid(*variable));
        assert_ne!(opened[2], Kind::Variable(*variable));
        assert_ne!(opened[2], reserved);
        assert_ne!(Kind::Variable(*variable), reserved);
    }
    #[test]
    fn lexical_binders_shadow_solved_producer_cells_including_solved_templates() {
        let mut producer = KindState::default();
        let Kind::Variable(bound) = producer.fresh() else {
            unreachable!()
        };
        let Kind::Variable(template) = producer.fresh() else {
            unreachable!()
        };
        let quantified = Kind::ForAll {
            variable: bound,
            body: Box::new(Kind::Variable(bound)),
        };
        producer
            .unify(Kind::Variable(bound), Kind::RowEmpty, TextRange::new(0, 1))
            .unwrap();
        producer
            .unify(
                Kind::Variable(template),
                quantified.clone(),
                TextRange::new(0, 1),
            )
            .unwrap();
        let closed =
            producer.close_kinds(&[quantified, Kind::Variable(template), Kind::Variable(bound)]);
        let mut consumer = KindState::default();
        let opened = consumer.instantiate_closed_kinds(&closed);
        for kind in &opened[..2] {
            let Kind::ForAll { variable, body } = kind else {
                panic!("quantifier retained")
            };
            assert_eq!(**body, Kind::Variable(*variable));
            assert!(consumer.is_rigid(*variable));
        }
        assert_eq!(opened[2], Kind::RowEmpty);
    }
    #[test]
    fn free_variable_solutions_can_refer_to_the_enclosing_lexical_binder() {
        let mut producer = KindState::default();
        let free = producer.fresh();
        let Kind::Variable(bound) = producer.fresh() else {
            unreachable!()
        };
        producer
            .unify(free.clone(), Kind::Variable(bound), TextRange::new(0, 1))
            .unwrap();
        let kind = Kind::ForAll {
            variable: bound,
            body: Box::new(free.clone()),
        };
        assert_eq!(
            producer.resolve(kind.clone()),
            Kind::ForAll {
                variable: bound,
                body: Box::new(Kind::Variable(bound))
            }
        );
        let closed = producer.close_kinds(&[kind, free]);
        let opened = KindState::default().instantiate_closed_kinds(&closed);
        let Kind::ForAll { variable, body } = &opened[0] else {
            panic!("quantifier retained")
        };
        assert_eq!(**body, Kind::Variable(*variable));
        assert_ne!(opened[1], Kind::Variable(*variable));
    }
}

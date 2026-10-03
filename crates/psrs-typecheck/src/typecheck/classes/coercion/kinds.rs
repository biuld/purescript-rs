use crate::typecheck::{Checker, InferType, TypeConstructor};
use psrs_hir as hir;
use psrs_kind::{Kind, KindScheme};
use std::collections::HashMap;

#[derive(Default)]
struct KindState {
    substitutions: HashMap<u32, Kind>,
    next_variable: u32,
}

impl KindState {
    fn fresh(&mut self) -> Kind {
        let variable = self.next_variable;
        self.next_variable += 1;
        Kind::Variable(variable)
    }

    fn resolve(&self, kind: Kind) -> Kind {
        match kind {
            Kind::Variable(variable) => self
                .substitutions
                .get(&variable)
                .cloned()
                .map(|kind| self.resolve(kind))
                .unwrap_or(Kind::Variable(variable)),
            Kind::App(function, argument) => Kind::App(
                Box::new(self.resolve(*function)),
                Box::new(self.resolve(*argument)),
            ),
            Kind::Function(parameter, result) => Kind::Function(
                Box::new(self.resolve(*parameter)),
                Box::new(self.resolve(*result)),
            ),
            other => other,
        }
    }

    fn unify(&mut self, left: Kind, right: Kind) -> bool {
        let left = self.resolve(left);
        let right = self.resolve(right);
        match (left, right) {
            (Kind::Variable(left), Kind::Variable(right)) if left == right => true,
            (Kind::Variable(variable), kind) | (kind, Kind::Variable(variable)) => {
                if kind_occurs(variable, &kind) {
                    return false;
                }
                self.substitutions.insert(variable, kind);
                true
            }
            (
                Kind::App(left_function, left_argument),
                Kind::App(right_function, right_argument),
            ) => {
                self.unify(*left_function, *right_function)
                    && self.unify(*left_argument, *right_argument)
            }
            (
                Kind::Function(left_parameter, left_result),
                Kind::Function(right_parameter, right_result),
            ) => {
                self.unify(*left_parameter, *right_parameter)
                    && self.unify(*left_result, *right_result)
            }
            (Kind::Builtin(left), Kind::Builtin(right)) => left == right,
            (Kind::Named(left), Kind::Named(right)) => left == right,
            _ => false,
        }
    }
}

impl Checker {
    pub(in crate::typecheck) fn fresh_kind(&mut self) -> Kind {
        let variable = self.state.next_kind_variable;
        self.state.next_kind_variable += 1;
        Kind::Variable(variable)
    }

    pub(in crate::typecheck) fn kind_from_hir(
        &mut self,
        ty: &hir::Type,
        scope: &HashMap<String, Kind>,
    ) -> Kind {
        match &ty.kind {
            hir::TypeKind::Wildcard => self.fresh_kind(),
            hir::TypeKind::Variable(name) => scope
                .get(name)
                .cloned()
                .unwrap_or_else(|| self.fresh_kind()),
            hir::TypeKind::Constructor(builtin) => kind_for_builtin(*builtin),
            hir::TypeKind::Named(id) | hir::TypeKind::Opaque(id) => Kind::Named(*id),
            hir::TypeKind::Application(function, argument) => Kind::App(
                Box::new(self.kind_from_hir(function, scope)),
                Box::new(self.kind_from_hir(argument, scope)),
            ),
            hir::TypeKind::OperatorChain { .. } => {
                self.state
                    .errors
                    .push(crate::typecheck::TypeCheckError::new(
                        crate::typecheck::TypeCheckErrorKind::UnsupportedType,
                        ty.span,
                        "type operator chain reached type checking before P4",
                    ));
                self.fresh_kind()
            }
            hir::TypeKind::Function { parameter, result } => Kind::Function(
                Box::new(self.kind_from_hir(parameter, scope)),
                Box::new(self.kind_from_hir(result, scope)),
            ),
            hir::TypeKind::Forall { variables, body } => {
                let mut nested = scope.clone();
                for variable in variables {
                    let kind = variable
                        .kind
                        .as_ref()
                        .map(|kind| self.kind_from_hir(kind, &nested))
                        .unwrap_or_else(|| self.fresh_kind());
                    nested.insert(variable.name.clone(), kind);
                }
                self.kind_from_hir(body, &nested)
            }
            hir::TypeKind::Constrained { body, .. } => self.kind_from_hir(body, scope),
            hir::TypeKind::Row { .. } => row_kind(),
            hir::TypeKind::Record { .. } => type_kind(),
            hir::TypeKind::Integer(_) => Kind::Builtin(hir::BuiltinType::Int),
            hir::TypeKind::String(_) => Kind::Builtin(hir::BuiltinType::Symbol),
        }
    }

    pub(in crate::typecheck) fn type_argument_kinds(
        &mut self,
        id: hir::TypeId,
        arity: usize,
    ) -> Vec<Kind> {
        let Some(scheme) = self.env.checked_kinds.kind(id).cloned() else {
            return (0..arity).map(|_| self.fresh_kind()).collect();
        };
        let mut state = KindState {
            substitutions: HashMap::new(),
            next_variable: self.state.next_kind_variable,
        };
        let kind = instantiate_kind_scheme(&scheme, &mut state);
        self.state.next_kind_variable = state.next_variable;
        let mut arguments = Vec::new();
        let mut current = kind;
        while let Kind::Function(argument, result) = current {
            arguments.push(*argument);
            current = *result;
        }
        if arguments.len() < arity {
            arguments.extend((arguments.len()..arity).map(|_| self.fresh_kind()));
        }
        arguments.truncate(arity);
        arguments
    }

    pub(super) fn coercion_kinds_compatible(
        &mut self,
        source: &InferType,
        target: &InferType,
    ) -> bool {
        let mut state = KindState {
            substitutions: HashMap::new(),
            next_variable: self.state.next_kind_variable,
        };
        let Some(source_kind) = self.infer_kind(source, &mut state) else {
            return false;
        };
        let Some(target_kind) = self.infer_kind(target, &mut state) else {
            return false;
        };
        let compatible = state.unify(source_kind, target_kind);
        self.state.next_kind_variable = state.next_variable;
        if compatible {
            for kind in self.state.variable_kinds.values_mut() {
                *kind = state.resolve(kind.clone());
            }
        }
        compatible
    }

    fn infer_kind(&mut self, ty: &InferType, state: &mut KindState) -> Option<Kind> {
        let ty = self.resolve_type(ty.clone());
        match ty {
            InferType::Variable(variable) => Some(
                self.state
                    .variable_kinds
                    .get(&variable)
                    .cloned()
                    .unwrap_or_else(|| {
                        let kind = state.fresh();
                        self.state.variable_kinds.insert(variable, kind.clone());
                        kind
                    }),
            ),
            InferType::Constructor(constructor) => self.kind_of_constructor(constructor, state),
            InferType::Application(function, argument) => {
                let function_kind = self.infer_kind(&function, state)?;
                let argument_kind = self.infer_kind(&argument, state)?;
                let result_kind = state.fresh();
                if !state.unify(
                    function_kind,
                    Kind::Function(Box::new(argument_kind), Box::new(result_kind.clone())),
                ) {
                    return None;
                }
                Some(state.resolve(result_kind))
            }
            InferType::ForAll { body, .. } => self.infer_kind(&body, state),
            InferType::Constrained { body, .. } => self.infer_kind(&body, state),
            InferType::RowEmpty => Some(row_kind()),
            // A type-level literal denotes its declared kind: a string is a
            // `Symbol` and an integer is an `Int`.
            InferType::TypeLevelString(_) => Some(Kind::Builtin(hir::BuiltinType::Symbol)),
            InferType::TypeLevelInt(_) => Some(Kind::Builtin(hir::BuiltinType::Int)),
            InferType::RowExtend { ty, tail, .. } => {
                let field_kind = self.infer_kind(&ty, state)?;
                let tail_kind = self.infer_kind(&tail, state)?;
                if !state.unify(field_kind, type_kind()) || !state.unify(tail_kind, row_kind()) {
                    return None;
                }
                Some(row_kind())
            }
        }
    }

    fn kind_of_constructor(
        &mut self,
        constructor: TypeConstructor,
        state: &mut KindState,
    ) -> Option<Kind> {
        Some(match constructor {
            TypeConstructor::Function => kind_for_builtin(hir::BuiltinType::Function),
            TypeConstructor::Record => kind_for_builtin(hir::BuiltinType::Record),
            TypeConstructor::Row => kind_for_builtin(hir::BuiltinType::Row),
            TypeConstructor::Array => kind_for_builtin(hir::BuiltinType::Array),
            TypeConstructor::Int
            | TypeConstructor::Number
            | TypeConstructor::Boolean
            | TypeConstructor::String
            | TypeConstructor::Char
            | TypeConstructor::Unit => type_kind(),
            TypeConstructor::User(id) => {
                let scheme = self.env.checked_kinds.kind(id)?.clone();
                instantiate_kind_scheme(&scheme, state)
            }
        })
    }
}

fn instantiate_kind_scheme(scheme: &KindScheme, state: &mut KindState) -> Kind {
    let mapping = scheme
        .variables
        .iter()
        .map(|variable| (*variable, state.fresh()))
        .collect::<HashMap<_, _>>();
    substitute_kind(&scheme.kind, &mapping)
}

fn substitute_kind(kind: &Kind, mapping: &HashMap<u32, Kind>) -> Kind {
    match kind {
        Kind::Variable(variable) => mapping
            .get(variable)
            .cloned()
            .unwrap_or(Kind::Variable(*variable)),
        Kind::App(function, argument) => Kind::App(
            Box::new(substitute_kind(function, mapping)),
            Box::new(substitute_kind(argument, mapping)),
        ),
        Kind::Function(parameter, result) => Kind::Function(
            Box::new(substitute_kind(parameter, mapping)),
            Box::new(substitute_kind(result, mapping)),
        ),
        other => other.clone(),
    }
}

fn kind_for_builtin(builtin: hir::BuiltinType) -> Kind {
    match builtin {
        hir::BuiltinType::Row => Kind::Function(Box::new(type_kind()), Box::new(type_kind())),
        hir::BuiltinType::Record => Kind::Function(Box::new(row_kind()), Box::new(type_kind())),
        hir::BuiltinType::Function => Kind::Function(
            Box::new(type_kind()),
            Box::new(Kind::Function(Box::new(type_kind()), Box::new(type_kind()))),
        ),
        hir::BuiltinType::Array => Kind::Function(Box::new(type_kind()), Box::new(type_kind())),
        other => type_kind_for(other),
    }
}

fn type_kind() -> Kind {
    Kind::Builtin(hir::BuiltinType::Type)
}

fn type_kind_for(builtin: hir::BuiltinType) -> Kind {
    match builtin {
        hir::BuiltinType::Type
        | hir::BuiltinType::Constraint
        | hir::BuiltinType::Symbol
        | hir::BuiltinType::Int
        | hir::BuiltinType::Number
        | hir::BuiltinType::Boolean
        | hir::BuiltinType::String
        | hir::BuiltinType::Char
        | hir::BuiltinType::Unit => type_kind(),
        other => Kind::Builtin(other),
    }
}

fn row_kind() -> Kind {
    Kind::App(
        Box::new(Kind::Builtin(hir::BuiltinType::Row)),
        Box::new(type_kind()),
    )
}

fn kind_occurs(variable: u32, kind: &Kind) -> bool {
    match kind {
        Kind::Variable(other) => variable == *other,
        Kind::App(function, argument) => {
            kind_occurs(variable, function) || kind_occurs(variable, argument)
        }
        Kind::Function(parameter, result) => {
            kind_occurs(variable, parameter) || kind_occurs(variable, result)
        }
        Kind::Builtin(_) | Kind::Named(_) => false,
    }
}

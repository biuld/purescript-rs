use psrs_hir::{BuiltinType, TypeId, TypeKind};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

/// A checked kind. Kinds are PureScript types of kind `Type`, so a named type
/// may denote a kind; `Builtin` covers primitive type constructors used as
/// kinds, and `Named` covers user declarations used as kinds.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    Type,
    Constraint,
    Symbol,
    /// The `Row` kind constructor, before application.
    Row,
    /// A primitive type constructor used as a kind, such as `Int` or `Array`.
    Builtin(BuiltinType),
    /// A user type declaration used as a kind.
    Named(TypeId),
    App(Box<Kind>, Box<Kind>),
    Function(Box<Kind>, Box<Kind>),
    /// An inference kind variable. Rigid variables come from `forall` binders.
    Variable(u32),
}

/// A possibly polymorphic kind: `variables` are rigid `forall` binders.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindScheme {
    pub variables: Vec<u32>,
    pub kind: Kind,
}

impl KindScheme {
    pub fn monomorphic(kind: Kind) -> Self {
        Self {
            variables: Vec::new(),
            kind,
        }
    }
}

/// A kind diagnostic, aligned to an official PureScript `errorCode`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindDiagnostic {
    pub code: &'static str,
    pub span: TextRange,
    pub message: String,
}

impl KindDiagnostic {
    pub fn new(code: &'static str, span: TextRange, message: impl Into<String>) -> Self {
        Self {
            code,
            span,
            message: message.into(),
        }
    }
}

/// The kind a primitive type constructor has, when used as a type.
pub fn builtin_type_kind(builtin: BuiltinType) -> Kind {
    match builtin {
        BuiltinType::Int
        | BuiltinType::Boolean
        | BuiltinType::String
        | BuiltinType::Unit
        | BuiltinType::Type
        | BuiltinType::Constraint
        | BuiltinType::Symbol => Kind::Type,
        BuiltinType::Function => Kind::Function(
            Box::new(Kind::Type),
            Box::new(Kind::Function(Box::new(Kind::Type), Box::new(Kind::Type))),
        ),
        BuiltinType::Row => Kind::Function(Box::new(Kind::Type), Box::new(Kind::Type)),
        BuiltinType::Record => Kind::Function(
            Box::new(Kind::App(Box::new(Kind::Row), Box::new(Kind::Type))),
            Box::new(Kind::Type),
        ),
        BuiltinType::Array => Kind::Function(Box::new(Kind::Type), Box::new(Kind::Type)),
    }
}

pub fn substitute(kind: &Kind, mapping: &HashMap<u32, Kind>) -> Kind {
    match kind {
        Kind::Variable(variable) => mapping
            .get(variable)
            .cloned()
            .unwrap_or(Kind::Variable(*variable)),
        Kind::App(function, argument) => Kind::App(
            Box::new(substitute(function, mapping)),
            Box::new(substitute(argument, mapping)),
        ),
        Kind::Function(parameter, result) => Kind::Function(
            Box::new(substitute(parameter, mapping)),
            Box::new(substitute(result, mapping)),
        ),
        primitive => primitive.clone(),
    }
}

pub fn occurs(variable: u32, kind: &Kind) -> bool {
    match kind {
        Kind::Variable(other) => variable == *other,
        Kind::App(function, argument) => occurs(variable, function) || occurs(variable, argument),
        Kind::Function(parameter, result) => {
            occurs(variable, parameter) || occurs(variable, result)
        }
        _ => false,
    }
}

/// Peels a nested application into its head and arguments, outermost first.
pub fn flatten_spine(ty: &psrs_hir::Type) -> (&psrs_hir::Type, Vec<&psrs_hir::Type>) {
    let mut arguments = Vec::new();
    let mut head = ty;
    while let TypeKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function.as_ref();
    }
    arguments.reverse();
    (head, arguments)
}

/// Collects every user type reference in a type expression. Kind annotations on
/// `forall` binders and rows are included.
pub fn collect_type_ids(ty: &psrs_hir::Type, out: &mut Vec<TypeId>) {
    match &ty.kind {
        TypeKind::Named(id) => out.push(*id),
        TypeKind::Application(function, argument) => {
            collect_type_ids(function, out);
            collect_type_ids(argument, out);
        }
        TypeKind::Function { parameter, result } => {
            collect_type_ids(parameter, out);
            collect_type_ids(result, out);
        }
        TypeKind::Forall { variables, body } => {
            for variable in variables {
                if let Some(kind) = &variable.kind {
                    collect_type_ids(kind, out);
                }
            }
            collect_type_ids(body, out);
        }
        TypeKind::Constrained { constraint, body } => {
            collect_type_ids(constraint, out);
            collect_type_ids(body, out);
        }
        TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
            for field in fields {
                collect_type_ids(&field.ty, out);
            }
            if let Some(tail) = tail {
                collect_type_ids(tail, out);
            }
        }
        TypeKind::Variable(_)
        | TypeKind::Constructor(_)
        | TypeKind::Integer(_)
        | TypeKind::String(_) => {}
    }
}

/// A dependency graph over type declarations, used for cycle detection.
pub fn detect_cycle(
    nodes: impl IntoIterator<Item = TypeId>,
    edges: &HashMap<TypeId, Vec<(TypeId, TextRange)>>,
) -> Vec<(TypeId, TextRange)> {
    let nodes: Vec<TypeId> = nodes.into_iter().collect();
    let mut state: HashMap<TypeId, u8> = HashMap::new();
    let mut reported = HashSet::new();
    let mut cycles = Vec::new();
    for node in nodes {
        visit_cycle(node, edges, &mut state, &mut reported, &mut cycles);
    }
    cycles
}

fn visit_cycle(
    node: TypeId,
    edges: &HashMap<TypeId, Vec<(TypeId, TextRange)>>,
    state: &mut HashMap<TypeId, u8>,
    reported: &mut HashSet<(TypeId, TypeId)>,
    cycles: &mut Vec<(TypeId, TextRange)>,
) {
    match state.get(&node) {
        Some(1) | Some(2) => return,
        _ => {}
    }
    state.insert(node, 1);
    if let Some(targets) = edges.get(&node) {
        for (target, span) in targets {
            match state.get(target) {
                Some(1) => {
                    if reported.insert((node, *target)) {
                        cycles.push((node, *span));
                    }
                }
                _ => visit_cycle(*target, edges, state, reported, cycles),
            }
        }
    }
    state.insert(node, 2);
}

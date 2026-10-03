//! The kind model: one representation of a checked kind, the one primitive kind
//! table, and the diagnostic that carries a kind error back to the module that
//! declares the offending type.

use psrs_hir::{BuiltinType, ModuleId, Role, TypeId, TypeKind};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

/// A checked kind. Kinds are PureScript types, so a kind is a type expression
/// read in a kind position and the two grammars cannot drift apart. A primitive
/// constructor read as a kind is [`Kind::Builtin`], a user declaration read as a
/// kind is its resolved identity, and everything else is an application, an
/// arrow, or a kind variable.
///
/// There is deliberately no reserved constant for one primitive and no
/// dedicated head for `Row`: a constant only for `Type` would make the same
/// kind expression denote two different things depending on which module reads
/// it, and the two readings do not unify.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A primitive constructor read as a kind. `Builtin(Type)` and
    /// `Builtin(Constraint)` are the primitive kinds, `Builtin(Symbol)` is the
    /// kind of a type-level string, `Builtin(Int)` the kind of a type-level
    /// integer, and `Builtin(Row)` the row kind constructor.
    Builtin(BuiltinType),
    /// A user type declaration read as a kind.
    Named(TypeId),
    App(Box<Kind>, Box<Kind>),
    Function(Box<Kind>, Box<Kind>),
    /// An inference kind variable. Rigid variables come from `forall` binders.
    Variable(u32),
}

impl Kind {
    /// Applies a kind constructor head to an argument kind.
    pub fn app(head: Kind, argument: Kind) -> Kind {
        Kind::App(Box::new(head), Box::new(argument))
    }

    /// `Row element`: the kind of a row whose entries have kind `element`.
    pub fn row(element: Kind) -> Kind {
        Kind::app(Kind::Builtin(BuiltinType::Row), element)
    }
}

/// The primitive `Type` kind, which every value signature ends at.
pub fn type_kind() -> Kind {
    Kind::Builtin(BuiltinType::Type)
}

/// The primitive `Constraint` kind, which every class constraint ends at.
pub fn constraint_kind() -> Kind {
    Kind::Builtin(BuiltinType::Constraint)
}

/// The kind of a primitive type constructor, exactly as official PureScript's
/// `Environment.hs` states it in `primTypes`.
///
/// This is the only primitive kind table in the compiler. A kind annotation, a
/// constraint argument, and a coerced boundary all read a primitive's kind
/// here, so they cannot disagree about what `Row`, `Record`, `Array`, or
/// `Function` mean. Reading a primitive *as a kind* is a different operation and
/// is [`Kind::Builtin`]; this table answers what kind a primitive has when it is
/// used as a type.
pub fn primitive_kind(builtin: BuiltinType) -> Kind {
    match builtin {
        BuiltinType::Int
        | BuiltinType::Number
        | BuiltinType::Boolean
        | BuiltinType::String
        | BuiltinType::Char
        | BuiltinType::Unit
        | BuiltinType::Type
        | BuiltinType::Constraint
        | BuiltinType::Symbol => type_kind(),
        BuiltinType::Function => Kind::Function(
            Box::new(type_kind()),
            Box::new(Kind::Function(Box::new(type_kind()), Box::new(type_kind()))),
        ),
        BuiltinType::Row | BuiltinType::Array => {
            Kind::Function(Box::new(type_kind()), Box::new(type_kind()))
        }
        BuiltinType::Record => {
            Kind::Function(Box::new(Kind::row(type_kind())), Box::new(type_kind()))
        }
    }
}

/// A possibly polymorphic kind. Every variable in `variables` is rigid at every
/// use, so instantiating the scheme is what makes the kind usable.
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

/// The checked kind environment of one program. It is the only source of kind
/// and role metadata for a declaration that another module uses, and it is
/// produced once by [`check_program`](crate::check_program) rather than rebuilt
/// per module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CheckedKindEnv {
    pub roles: HashMap<TypeId, Vec<Role>>,
    /// Kind schemes for named constructors, including imports. Keeping these
    /// alongside roles lets coercion checking validate argument kinds after
    /// the source type tree has been elaborated.
    pub kinds: HashMap<TypeId, KindScheme>,
    /// The module that declares each entry, so a use in one module is checked
    /// against, and reported against, the module that declared the type.
    pub declaring_module: HashMap<TypeId, ModuleId>,
}

impl CheckedKindEnv {
    pub fn roles(&self, id: TypeId) -> Option<&[Role]> {
        self.roles.get(&id).map(Vec::as_slice)
    }

    /// The checked kind scheme for a declaration, if the program produced one.
    /// This and [`CheckedKindEnv::roles`] are the only lookups a consumer needs:
    /// a kind or a role is never reconstructed from surface syntax.
    pub fn kind_scheme(&self, id: TypeId) -> Option<&KindScheme> {
        self.kinds.get(&id)
    }

    /// The checked kind scheme for a declaration.
    ///
    /// Retained under its previous name for callers that already read it; it is
    /// the same lookup as [`CheckedKindEnv::kind_scheme`].
    pub fn kind(&self, id: TypeId) -> Option<&KindScheme> {
        self.kind_scheme(id)
    }
}

/// A kind diagnostic, aligned to an official PureScript `errorCode`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindDiagnostic {
    pub code: &'static str,
    pub span: TextRange,
    pub message: String,
    /// The module that declares the offending type. This is not necessarily the
    /// module whose use exposed the problem: a conflict in module A is reported
    /// against A's source even when B's use exposed it.
    pub origin: ModuleId,
}

impl KindDiagnostic {
    pub fn new(
        origin: ModuleId,
        code: &'static str,
        span: TextRange,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            span,
            message: message.into(),
            origin,
        }
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
        TypeKind::Named(id) | TypeKind::Opaque(id) => out.push(*id),
        TypeKind::Application(function, argument) => {
            collect_type_ids(function, out);
            collect_type_ids(argument, out);
        }
        TypeKind::OperatorChain {
            operands,
            operators,
        } => {
            out.extend(operators.iter().filter_map(|operator| match operator.head {
                psrs_hir::ResolvedTypeHead::Builtin(_) => None,
                psrs_hir::ResolvedTypeHead::Named(id) | psrs_hir::ResolvedTypeHead::Opaque(id) => {
                    Some(id)
                }
            }));
            for operand in operands {
                collect_type_ids(operand, out);
            }
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
        | TypeKind::Wildcard
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

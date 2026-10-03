use psrs_hir::{
    CaseBranchCoverage, ExternalSymbol, LocalId, ModuleId, SymbolId, TypeId as HirTypeId,
    TypeVariableId,
};
use psrs_span::TextRange;

mod evidence;
mod scope;
mod verify;

pub use evidence::{Evidence, EvidenceKind};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TypeId(pub u32);

/// A type constructor reference. `Function` is the arrow head; `Record` is the
/// record head applied to a row; `Array` is the array head; the scalar
/// constructors name the source primitives; user constructors are identified by
/// their resolved HIR declaration.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TypeConstructor {
    Function,
    Record,
    /// The `Prim.Row` type constructor of kind `Type -> Type`, declared with a
    /// phantom role. Its application is a nominal type, not a row: a row value
    /// is [`Type::RowEmpty`], [`Type::RowExtend`], or a row-polymorphic
    /// [`Type::Variable`].
    Row,
    Array,
    Int,
    Number,
    Boolean,
    String,
    Char,
    Unit,
    /// The `Prim.Type` kind constructor. Official PureScript declares it with
    /// kind `Type`, so a type position that names it is an ordinary nominal
    /// type on the same spine as every other head.
    Type,
    /// The `Prim.Constraint` kind constructor, of kind `Type`.
    Constraint,
    /// The `Prim.Symbol` kind constructor. It is the kind of a type-level string
    /// and, like `Type` and `Constraint`, a type of kind `Type`.
    Symbol,
    User(HirTypeId),
}

impl TypeConstructor {
    /// Whether this constructor names one of the source primitive scalars.
    pub fn is_primitive(self) -> bool {
        matches!(
            self,
            Self::Int | Self::Number | Self::Boolean | Self::String | Self::Char | Self::Unit
        )
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Type {
    /// A generalized type variable. See [`Declaration::quantified`] and
    /// [`Binding::quantified`] for the variables bound at each site.
    Variable(TypeVariableId),
    Constructor(TypeConstructor),
    Application(TypeId, TypeId),
    /// A lexical type-level quantifier. Each node owns only its listed binders;
    /// nested `ForAll` nodes stay nested so instantiation preserves scope.
    ForAll {
        variables: Vec<TypeVariableId>,
        body: TypeId,
    },
    /// The empty row. A closed record's row ends here.
    RowEmpty,
    /// A row extended with one labeled field. A record type is
    /// `Application(Constructor(Record), row)`; a closed row ends in
    /// [`Type::RowEmpty`] and an open row ends in a [`Type::Variable`].
    RowExtend {
        label: String,
        ty: TypeId,
        tail: TypeId,
    },
    /// A type-level string literal, of kind `Symbol`. The payload is a sequence
    /// of Unicode scalar values (DEC-16), so it never holds an unpaired
    /// surrogate. Two literals are equal when their scalar sequences are equal.
    TypeLevelString(String),
    /// A type-level integer literal, of kind `Int`. Two literals are equal when
    /// their values are equal.
    TypeLevelInt(i64),
}

/// A row flattened into its fields and its tail. The tail is `None` for a
/// closed row and `Some(variable)` for an open row.
pub type RowFields = (Vec<(String, TypeId)>, Option<TypeId>);

/// The row of a record type `Application(Constructor(Record), row)`, or `None`
/// when `id` is not a record type.
pub fn record_row(types: &[Type], id: TypeId) -> Option<TypeId> {
    let Type::Application(function, row) = types.get(id.0 as usize)? else {
        return None;
    };
    matches!(
        types.get(function.0 as usize),
        Some(Type::Constructor(TypeConstructor::Record))
    )
    .then_some(*row)
}

/// Flattens a row into its fields and its tail. The tail is `None` for a closed
/// row and `Some(variable)` for an open row. `None` is returned when `row`
/// reaches a node that is neither a row constructor nor a row variable.
pub fn row_fields(types: &[Type], mut row: TypeId) -> Option<RowFields> {
    let mut fields = Vec::new();
    loop {
        match types.get(row.0 as usize)? {
            Type::RowEmpty => return Some((fields, None)),
            Type::RowExtend { label, ty, tail } => {
                fields.push((label.clone(), *ty));
                row = *tail;
            }
            Type::Variable(_) => return Some((fields, Some(row))),
            _ => return None,
        }
    }
}

/// The fields of a record type in canonical (label-sorted) order, or `None`
/// when `id` is not a record type. An open row's fields are those present
/// before its tail variable.
pub fn record_fields(types: &[Type], id: TypeId) -> Option<Vec<(String, TypeId)>> {
    let row = record_row(types, id)?;
    let (mut fields, _) = row_fields(types, row)?;
    fields.sort_by(|left, right| left.0.cmp(&right.0));
    Some(fields)
}

/// The parameter and result of an arrow type `a -> b`, spelled as the
/// application spine `Application(Application(Constructor(Function), a), b)`.
pub fn arrow_parts(types: &[Type], id: TypeId) -> Option<(TypeId, TypeId)> {
    let Type::Application(inner, result) = types.get(id.0 as usize)? else {
        return None;
    };
    let Type::Application(head, parameter) = types.get(inner.0 as usize)? else {
        return None;
    };
    matches!(
        types.get(head.0 as usize),
        Some(Type::Constructor(TypeConstructor::Function))
    )
    .then_some((*parameter, *result))
}

/// The binders and body of a type-level universal quantifier.
pub fn forall_parts(types: &[Type], id: TypeId) -> Option<(&[TypeVariableId], TypeId)> {
    match types.get(id.0 as usize)? {
        Type::ForAll { variables, body } => Some((variables, *body)),
        _ => None,
    }
}

/// A data constructor known to the module. `tag` is its zero-based position in
/// the declaration; `field_count` is the number of fields it takes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConstructorInfo {
    pub symbol: SymbolId,
    /// Source constructor name, retained for diagnostics and external type mapping.
    pub name: String,
    pub type_id: HirTypeId,
    pub tag: u32,
    pub field_count: usize,
    /// The elaborated field types, in constructor order. Keeping these in
    /// THIR lets later representations choose a runtime layout without
    /// consulting HIR again.
    pub field_types: Vec<TypeId>,
    /// The declaration's ordered type parameters, as the variables that
    /// `field_types` templates refer to. The variable at `parameters[i]` is the
    /// constructor's field type when the enclosing application's argument `i` is
    /// substituted, so a resolved application can instantiate the templates.
    pub parameters: Vec<TypeVariableId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub id: ModuleId,
    pub name: String,
    pub externals: Vec<ExternalSymbol>,
    pub types: Vec<Type>,
    /// Nominal types whose single constructor is erased at runtime. The type
    /// checker keeps these types distinct; later lowering uses this metadata
    /// to pass their field value through without allocating a wrapper.
    pub newtype_ids: Vec<HirTypeId>,
    /// Foreign data declarations. Their type node is still
    /// `Constructor(User(id))`; this set, with an empty constructor list, is
    /// what keeps the type opaque. It is not a runtime layout.
    pub opaque_ids: Vec<HirTypeId>,
    /// Type constructors whose application has a callable closure
    /// representation, registered by the trusted elaboration through their
    /// resolved type identity. Each entry names the number of hidden
    /// calling-convention parameters; the call result is the application's last
    /// type argument. This is representation metadata, not a type.
    pub callable_types: Vec<(HirTypeId, u32)>,
    pub constructors: Vec<ConstructorInfo>,
    pub declarations: Vec<Declaration>,
    /// Qualified names of the type declarations this module declares, keyed by
    /// their stable id. Carried so later stages can recognize well-known
    /// library types (for example `Data.Maybe.Maybe`) after names are otherwise
    /// dropped.
    pub type_names: Vec<(HirTypeId, String)>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declaration {
    pub symbol: SymbolId,
    pub name: String,
    pub name_span: TextRange,
    pub quantified: Vec<TypeVariableId>,
    pub ty: TypeId,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binder {
    pub id: LocalId,
    pub name: String,
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Binding {
    pub binder: Binder,
    pub quantified: Vec<TypeVariableId>,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Expr {
    pub kind: ExprKind,
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExprKind {
    Local(LocalId),
    Global(SymbolId),
    Integer(i32),
    Number(String),
    Boolean(bool),
    String(String),
    Char(char),
    Array(Vec<Expr>),
    Record(Vec<(String, Expr)>),
    RecordUpdate {
        expression: Box<Expr>,
        fields: Vec<(String, Expr)>,
    },
    FieldAccess {
        expression: Box<Expr>,
        field: String,
    },
    /// Selected class evidence. Core lowering erases this to ordinary values,
    /// applications, and record projections.
    Evidence(Evidence),
    /// A typechecked representational conversion authorized by `Coercible`
    /// evidence. The source and result types are carried by the value and this
    /// expression respectively.
    Coerce {
        value: Box<Expr>,
        evidence: Evidence,
        source_type: TypeId,
        target_type: TypeId,
    },
    Application(Box<Expr>, Box<Expr>),
    Lambda {
        binder: Binder,
        body: Box<Expr>,
    },
    Let {
        bindings: Vec<Binding>,
        body: Box<Expr>,
    },
    If {
        condition: Box<Expr>,
        then_branch: Box<Expr>,
        else_branch: Box<Expr>,
    },
    Case {
        scrutinee: Box<Expr>,
        branches: Vec<CaseBranch>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CaseBranch {
    pub pattern: Pattern,
    pub value: Expr,
    pub span: TextRange,
    pub coverage: CaseBranchCoverage,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pattern {
    pub kind: PatternKind,
    pub ty: TypeId,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternLiteral {
    Integer(i32),
    Number(String),
    String(String),
    Char(char),
    Boolean(bool),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PatternKind {
    Wildcard,
    Literal {
        literal: PatternLiteral,
    },
    Array {
        elements: Vec<Pattern>,
    },
    Named {
        id: LocalId,
        pattern: Box<Pattern>,
    },
    Var {
        id: LocalId,
        ty: TypeId,
    },
    Constructor {
        symbol: SymbolId,
        arguments: Vec<Pattern>,
    },
    Record {
        fields: Vec<(String, Pattern)>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VerifyError {
    pub span: TextRange,
    pub message: &'static str,
}

impl Module {
    pub fn verify(&self) -> Result<(), Vec<VerifyError>> {
        verify::verify_module(self)
    }
}

#[cfg(test)]
mod rank_n_tests;
#[cfg(test)]
mod tests;

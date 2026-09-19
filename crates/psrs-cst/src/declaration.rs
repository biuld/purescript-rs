use super::{CstName, DeclarationBlock, Expr, Pattern, TypeExpr, TypeVarBinder};
use psrs_span::TextRange;

/// A top-level or local declaration. Keeping the concrete form lets the parser
/// grow ahead of lowering: variants without an AST representation lower to a
/// `psrs_ast::LowerError` instead of being dropped.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Declaration {
    Value(ValueDeclaration),
    TypeSignature(TypeSignature),
    Data(DataDeclaration),
    Newtype(NewtypeDeclaration),
    TypeSynonym(TypeSynonymDeclaration),
    KindSignature(KindSignature),
    Class(ClassDeclaration),
    Instance(InstanceDeclaration),
    Derive(DeriveDeclaration),
    Foreign(ForeignDeclaration),
    Fixity(FixityDeclaration),
    Role(RoleDeclaration),
    Pattern(PatternDeclaration),
}

impl Declaration {
    pub fn span(&self) -> TextRange {
        match self {
            Self::Value(declaration) => declaration.span,
            Self::TypeSignature(declaration) => declaration.span,
            Self::Data(declaration) => declaration.span,
            Self::Newtype(declaration) => declaration.span,
            Self::TypeSynonym(declaration) => declaration.span,
            Self::KindSignature(declaration) => declaration.span,
            Self::Class(declaration) => declaration.span,
            Self::Instance(declaration) => declaration.span,
            Self::Derive(declaration) => declaration.span,
            Self::Foreign(declaration) => declaration.span,
            Self::Fixity(declaration) => declaration.span,
            Self::Role(declaration) => declaration.span,
            Self::Pattern(declaration) => declaration.span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ValueDeclaration {
    pub name: CstName,
    pub parameters: Vec<Pattern>,
    pub rhs: ValueRhs,
    pub where_block: Option<DeclarationBlock>,
    pub span: TextRange,
    pub annotation: Option<TypeExpr>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ValueRhs {
    Plain { equals_span: TextRange, value: Expr },
    Guarded(Vec<GuardedRhs>),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GuardedRhs {
    pub bar_span: TextRange,
    pub guards: Vec<Guard>,
    pub equals_span: TextRange,
    pub value: Expr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Guard {
    Boolean(Expr),
    Pattern {
        pattern: Pattern,
        left_arrow_span: TextRange,
        value: Expr,
    },
    Let {
        let_keyword_span: TextRange,
        declarations: Vec<Declaration>,
        layout_start_span: TextRange,
        layout_end_span: TextRange,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSignature {
    pub name: CstName,
    pub double_colon_span: TextRange,
    pub type_expr: TypeExpr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataDeclaration {
    pub keyword_span: TextRange,
    pub name: CstName,
    pub parameters: Vec<TypeVarBinder>,
    pub kind: Option<TypeExpr>,
    pub equals_span: Option<TextRange>,
    pub constructors: Vec<DataConstructor>,
    pub derives: Vec<DerivingClause>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataConstructor {
    pub name: CstName,
    pub fields: Vec<TypeExpr>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DerivingClause {
    pub derive_keyword_span: TextRange,
    pub open_paren_span: TextRange,
    pub constraints: Vec<TypeExpr>,
    pub close_paren_span: TextRange,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewtypeDeclaration {
    pub keyword_span: TextRange,
    pub name: CstName,
    pub parameters: Vec<TypeVarBinder>,
    pub kind: Option<TypeExpr>,
    pub equals_span: Option<TextRange>,
    pub constructor: Option<DataConstructor>,
    pub derives: Vec<DerivingClause>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSynonymDeclaration {
    pub keyword_span: TextRange,
    pub name: CstName,
    pub parameters: Vec<TypeVarBinder>,
    pub equals_span: TextRange,
    pub body: TypeExpr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct KindSignature {
    pub keyword_span: TextRange,
    pub name: CstName,
    pub double_colon_span: TextRange,
    pub kind: TypeExpr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDeclaration {
    pub keyword_span: TextRange,
    pub superclasses: Option<Box<TypeExpr>>,
    pub superclass_arrow_span: Option<TextRange>,
    pub head: TypeExpr,
    pub name: CstName,
    pub fundeps: Vec<FunctionalDependency>,
    pub where_block: Option<DeclarationBlock>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionalDependency {
    pub bar_span: TextRange,
    pub from: Vec<CstName>,
    pub arrow_span: TextRange,
    pub to: Vec<CstName>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceDeclaration {
    pub else_keyword_span: Option<TextRange>,
    pub keyword_span: TextRange,
    pub name: Option<CstName>,
    pub constraints: Option<Box<TypeExpr>>,
    pub constraint_arrow_span: Option<TextRange>,
    pub head: TypeExpr,
    pub where_block: Option<DeclarationBlock>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeriveDeclaration {
    pub derive_keyword_span: TextRange,
    pub newtype_keyword_span: Option<TextRange>,
    pub instance_keyword_span: TextRange,
    pub name: Option<CstName>,
    pub constraints: Option<Box<TypeExpr>>,
    pub constraint_arrow_span: Option<TextRange>,
    pub head: TypeExpr,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignDeclaration {
    pub foreign_keyword_span: TextRange,
    pub import_keyword_span: TextRange,
    pub data_keyword_span: Option<TextRange>,
    pub name: CstName,
    pub double_colon_span: TextRange,
    pub type_expr: TypeExpr,
    pub span: TextRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Fixity {
    Left,
    Right,
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FixityDeclaration {
    pub associativity: Fixity,
    pub associativity_span: TextRange,
    pub precedence: String,
    pub precedence_span: TextRange,
    pub namespace_span: Option<TextRange>,
    pub operator: CstName,
    pub alias: Option<CstName>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RoleDeclaration {
    pub type_keyword_span: TextRange,
    pub role_keyword_span: TextRange,
    pub name: CstName,
    pub roles: Vec<CstName>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PatternDeclaration {
    pub pattern: Pattern,
    pub equals_span: TextRange,
    pub value: Expr,
    pub span: TextRange,
}

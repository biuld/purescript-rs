use psrs_span::TextRange;

mod declaration;
mod expr;
mod import;
mod type_expr;

pub use declaration::*;
pub use expr::*;
pub use import::*;
pub use type_expr::*;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub module_keyword_span: TextRange,
    pub name: CstName,
    pub exports: Option<ExportList>,
    pub where_keyword_span: TextRange,
    pub imports: Vec<ImportDeclaration>,
    pub declarations: Vec<Declaration>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CstName {
    pub text: String,
    pub span: TextRange,
}

impl CstName {
    pub fn new(text: impl Into<String>, span: TextRange) -> Self {
        Self {
            text: text.into(),
            span,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeVarBinder {
    pub name: CstName,
    pub kind: Option<TypeExpr>,
    /// `true` when the source wrote `@name`. A plain binder is invisible and is
    /// instantiated before a visible type application can select it.
    pub visible: bool,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeclarationBlock {
    pub where_keyword_span: TextRange,
    pub layout_start_span: TextRange,
    pub declarations: Vec<Declaration>,
    pub layout_end_span: TextRange,
    pub span: TextRange,
}

use super::CstName;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportList {
    pub open_paren_span: TextRange,
    pub items: Vec<ExportRef>,
    pub close_paren_span: TextRange,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportRef {
    Value(CstName),
    Operator(CstName),
    Type {
        name: CstName,
        members: Option<TypeMembers>,
    },
    Module(CstName),
}

impl ExportRef {
    pub fn span(&self) -> TextRange {
        match self {
            Self::Value(name) | Self::Operator(name) | Self::Module(name) => name.span,
            Self::Type { name, members } => match members {
                Some(members) => TextRange::new(name.span.start, members.span.end),
                None => name.span,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportDeclaration {
    pub import_keyword_span: TextRange,
    pub module: CstName,
    pub alias: Option<CstName>,
    pub list: Option<ImportList>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportList {
    pub hiding_keyword_span: Option<TextRange>,
    pub open_paren_span: TextRange,
    pub items: Vec<ImportRef>,
    pub close_paren_span: TextRange,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportRef {
    Value(CstName),
    Operator(CstName),
    Type {
        name: CstName,
        members: Option<TypeMembers>,
    },
    Class(CstName),
    Module(CstName),
}

impl ImportRef {
    pub fn span(&self) -> TextRange {
        match self {
            Self::Value(name) | Self::Operator(name) | Self::Module(name) | Self::Class(name) => {
                name.span
            }
            Self::Type { name, members } => match members {
                Some(members) => TextRange::new(name.span.start, members.span.end),
                None => name.span,
            },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeMembers {
    pub open_paren_span: TextRange,
    pub all: bool,
    pub names: Vec<CstName>,
    pub close_paren_span: TextRange,
    pub span: TextRange,
}

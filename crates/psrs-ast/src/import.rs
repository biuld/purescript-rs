use crate::export::{TypeMembers, lower_type_members};
use crate::{Name, lower_name};
use psrs_cst as cst;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Import {
    pub module: Name,
    pub alias: Option<Name>,
    pub list: Option<ImportList>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ImportList {
    pub hiding: bool,
    pub items: Vec<ImportRef>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImportRef {
    Value(Name),
    Operator(Name),
    Type {
        name: Name,
        members: Option<TypeMembers>,
    },
    Class(Name),
    Module(Name),
}

pub(crate) fn lower_import(import: cst::ImportDeclaration) -> Import {
    Import {
        module: lower_name(import.module),
        alias: import.alias.map(lower_name),
        list: import.list.map(lower_import_list),
        span: import.span,
    }
}

fn lower_import_list(list: cst::ImportList) -> ImportList {
    ImportList {
        hiding: list.hiding_keyword_span.is_some(),
        items: list.items.into_iter().map(lower_import_ref).collect(),
        span: list.span,
    }
}

fn lower_import_ref(reference: cst::ImportRef) -> ImportRef {
    match reference {
        cst::ImportRef::Value(name) => ImportRef::Value(lower_name(name)),
        cst::ImportRef::Operator(name) => ImportRef::Operator(lower_name(name)),
        cst::ImportRef::Type { name, members } => ImportRef::Type {
            name: lower_name(name),
            members: members.map(lower_type_members),
        },
        cst::ImportRef::Class(name) => ImportRef::Class(lower_name(name)),
        cst::ImportRef::Module(name) => ImportRef::Module(lower_name(name)),
    }
}

impl ImportRef {
    pub fn name(&self) -> &Name {
        match self {
            Self::Value(name) | Self::Operator(name) | Self::Class(name) | Self::Module(name) => {
                name
            }
            Self::Type { name, .. } => name,
        }
    }
}

use crate::{Name, lower_name};
use psrs_cst as cst;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExportList {
    pub items: Vec<ExportRef>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ExportRef {
    Value(Name),
    Operator(Name),
    Type {
        name: Name,
        members: Option<TypeMembers>,
    },
    Module(Name),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeMembers {
    pub all: bool,
    pub names: Vec<Name>,
    pub span: TextRange,
}

pub(crate) fn lower_export_list(list: cst::ExportList) -> ExportList {
    ExportList {
        items: list.items.into_iter().map(lower_export_ref).collect(),
        span: list.span,
    }
}

fn lower_export_ref(reference: cst::ExportRef) -> ExportRef {
    match reference {
        cst::ExportRef::Value(name) => ExportRef::Value(lower_name(name)),
        cst::ExportRef::Operator(name) => ExportRef::Operator(lower_name(name)),
        cst::ExportRef::Type { name, members } => ExportRef::Type {
            name: lower_name(name),
            members: members.map(lower_type_members),
        },
        cst::ExportRef::Module(name) => ExportRef::Module(lower_name(name)),
    }
}

pub(crate) fn lower_type_members(members: cst::TypeMembers) -> TypeMembers {
    TypeMembers {
        all: members.all,
        names: members.names.into_iter().map(lower_name).collect(),
        span: members.span,
    }
}

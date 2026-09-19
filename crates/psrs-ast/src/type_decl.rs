use crate::{LowerError, Name, Type, lower_name, lower_type};
use psrs_cst as cst;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeDeclaration {
    Data(DataDeclaration),
    Newtype(NewtypeDeclaration),
    TypeSynonym(TypeSynonymDeclaration),
    Class(ClassDeclaration),
}

impl TypeDeclaration {
    pub fn name(&self) -> &Name {
        match self {
            Self::Data(declaration) => &declaration.name,
            Self::Newtype(declaration) => &declaration.name,
            Self::TypeSynonym(declaration) => &declaration.name,
            Self::Class(declaration) => &declaration.name,
        }
    }

    pub fn span(&self) -> TextRange {
        match self {
            Self::Data(declaration) => declaration.span,
            Self::Newtype(declaration) => declaration.span,
            Self::TypeSynonym(declaration) => declaration.span,
            Self::Class(declaration) => declaration.span,
        }
    }

    pub fn has_kind_signature(&self) -> bool {
        match self {
            Self::Data(declaration) => declaration.has_kind_signature,
            Self::Newtype(declaration) => declaration.has_kind_signature,
            Self::TypeSynonym(declaration) => declaration.has_kind_signature,
            Self::Class(declaration) => declaration.has_kind_signature,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeParameter {
    pub name: Name,
    pub kind: Option<Type>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataDeclaration {
    pub name: Name,
    pub parameters: Vec<TypeParameter>,
    pub constructors: Vec<DataConstructor>,
    pub has_kind_signature: bool,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewtypeDeclaration {
    pub name: Name,
    pub parameters: Vec<TypeParameter>,
    pub constructor: Option<DataConstructor>,
    pub has_kind_signature: bool,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSynonymDeclaration {
    pub name: Name,
    pub parameters: Vec<TypeParameter>,
    pub body: Type,
    pub has_kind_signature: bool,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDeclaration {
    pub name: Name,
    pub parameters: Vec<TypeParameter>,
    pub superclasses: Vec<Type>,
    pub members: Vec<ClassMember>,
    pub has_kind_signature: bool,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassMember {
    pub name: Name,
    pub signature: Option<Type>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DataConstructor {
    pub name: Name,
    pub fields: Vec<Type>,
    pub span: TextRange,
}

/// Lowers a `data`, `newtype`, `type`, or `class` declaration. The optional
/// kind signature is the `data X :: ...` form that immediately precedes it.
pub(crate) fn lower_type_declaration(
    kind_signature: Option<cst::KindSignature>,
    declaration: cst::Declaration,
) -> Result<TypeDeclaration, LowerError> {
    let has_kind_signature = kind_signature.is_some();
    match declaration {
        cst::Declaration::Data(declaration) => Ok(TypeDeclaration::Data(DataDeclaration {
            name: lower_name(declaration.name),
            parameters: lower_type_parameters(declaration.parameters)?,
            constructors: declaration
                .constructors
                .into_iter()
                .map(lower_constructor)
                .collect::<Result<_, _>>()?,
            has_kind_signature,
            span: declaration.span,
        })),
        cst::Declaration::Newtype(declaration) => {
            Ok(TypeDeclaration::Newtype(NewtypeDeclaration {
                name: lower_name(declaration.name),
                parameters: lower_type_parameters(declaration.parameters)?,
                constructor: declaration.constructor.map(lower_constructor).transpose()?,
                has_kind_signature,
                span: declaration.span,
            }))
        }
        cst::Declaration::TypeSynonym(declaration) => {
            Ok(TypeDeclaration::TypeSynonym(TypeSynonymDeclaration {
                name: lower_name(declaration.name),
                parameters: lower_type_parameters(declaration.parameters)?,
                body: lower_type(declaration.body)?,
                has_kind_signature,
                span: declaration.span,
            }))
        }
        cst::Declaration::Class(declaration) => Ok(TypeDeclaration::Class(ClassDeclaration {
            name: lower_name(declaration.name),
            parameters: lower_class_parameters(&declaration.head),
            superclasses: match declaration.superclasses {
                Some(superclasses) => vec![lower_type(*superclasses)?],
                None => Vec::new(),
            },
            members: declaration
                .where_block
                .map(|block| lower_class_members(block.declarations))
                .transpose()?
                .unwrap_or_default(),
            has_kind_signature,
            span: declaration.span,
        })),
        other => Err(LowerError::new(
            other.span(),
            "expected a data, newtype, type, or class declaration",
        )),
    }
}

fn lower_type_parameters(
    parameters: Vec<cst::TypeVarBinder>,
) -> Result<Vec<TypeParameter>, LowerError> {
    parameters
        .into_iter()
        .map(|parameter| {
            Ok(TypeParameter {
                name: lower_name(parameter.name),
                kind: parameter.kind.map(lower_type).transpose()?,
                span: parameter.span,
            })
        })
        .collect()
}

fn lower_constructor(constructor: cst::DataConstructor) -> Result<DataConstructor, LowerError> {
    Ok(DataConstructor {
        name: lower_name(constructor.name),
        fields: constructor
            .fields
            .into_iter()
            .map(lower_type)
            .collect::<Result<_, _>>()?,
        span: constructor.span,
    })
}

fn lower_class_parameters(head: &cst::TypeExpr) -> Vec<TypeParameter> {
    let mut parameters = Vec::new();
    collect_parameters(head, &mut parameters);
    parameters
}

fn collect_parameters(expression: &cst::TypeExpr, out: &mut Vec<TypeParameter>) {
    if let cst::TypeExprKind::Application(function, arguments) = &expression.kind {
        collect_parameters(function, out);
        for argument in arguments {
            if let cst::TypeExprKind::Name(name) = &argument.kind
                && is_type_variable(&name.text)
            {
                out.push(TypeParameter {
                    name: lower_name(name.clone()),
                    kind: None,
                    span: name.span,
                });
            }
        }
    }
}

fn lower_class_members(
    declarations: Vec<cst::Declaration>,
) -> Result<Vec<ClassMember>, LowerError> {
    let mut members: Vec<ClassMember> = Vec::new();
    for declaration in declarations {
        let (name, signature, span) = match declaration {
            cst::Declaration::TypeSignature(signature) => (
                signature.name,
                Some(lower_type(signature.type_expr)?),
                signature.span,
            ),
            cst::Declaration::Value(value) => (
                value.name,
                value.annotation.map(lower_type).transpose()?,
                value.span,
            ),
            _ => continue,
        };
        if let Some(existing) = members
            .iter_mut()
            .find(|member| member.name.text == name.text)
        {
            if existing.signature.is_none() {
                existing.signature = signature;
            }
            existing.span = TextRange::new(existing.span.start, span.end);
        } else {
            members.push(ClassMember {
                name: lower_name(name),
                signature,
                span,
            });
        }
    }
    Ok(members)
}

fn is_type_variable(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|first| first.is_ascii_lowercase())
}

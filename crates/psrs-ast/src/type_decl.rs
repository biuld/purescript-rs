use crate::{LowerError, Name, Type, lower_name, lower_type};
use psrs_cst as cst;
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeDeclaration {
    Data(DataDeclaration),
    Newtype(NewtypeDeclaration),
    TypeSynonym(TypeSynonymDeclaration),
    Class(ClassDeclaration),
    /// An opaque type introduced by `foreign import data`. It has a kind and
    /// no constructors, so source cannot build a value of the type.
    Foreign(ForeignDataDeclaration),
}

impl TypeDeclaration {
    pub fn name(&self) -> &Name {
        match self {
            Self::Data(declaration) => &declaration.name,
            Self::Newtype(declaration) => &declaration.name,
            Self::TypeSynonym(declaration) => &declaration.name,
            Self::Class(declaration) => &declaration.name,
            Self::Foreign(declaration) => &declaration.name,
        }
    }

    pub fn span(&self) -> TextRange {
        match self {
            Self::Data(declaration) => declaration.span,
            Self::Newtype(declaration) => declaration.span,
            Self::TypeSynonym(declaration) => declaration.span,
            Self::Class(declaration) => declaration.span,
            Self::Foreign(declaration) => declaration.span,
        }
    }

    pub fn has_kind_signature(&self) -> bool {
        self.kind_signature().is_some()
    }

    pub fn kind_signature(&self) -> Option<&Type> {
        match self {
            Self::Data(declaration) => declaration.kind_signature.as_ref(),
            Self::Newtype(declaration) => declaration.kind_signature.as_ref(),
            Self::TypeSynonym(declaration) => declaration.kind_signature.as_ref(),
            Self::Class(declaration) => declaration.kind_signature.as_ref(),
            // The kind is inline on the foreign declaration, not a preceding
            // standalone kind signature.
            Self::Foreign(_) => None,
        }
    }
}

/// `foreign import data Name :: Kind`. The kind is written after `::`; the
/// declaration introduces no value constructor.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ForeignDataDeclaration {
    pub name: Name,
    pub declared_kind: Type,
    pub span: TextRange,
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
    pub kind_signature: Option<Type>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct NewtypeDeclaration {
    pub name: Name,
    pub parameters: Vec<TypeParameter>,
    pub constructor: Option<DataConstructor>,
    pub kind_signature: Option<Type>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TypeSynonymDeclaration {
    pub name: Name,
    pub parameters: Vec<TypeParameter>,
    pub body: Type,
    pub kind_signature: Option<Type>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassDeclaration {
    pub name: Name,
    pub parameters: Vec<TypeParameter>,
    pub superclasses: Vec<Type>,
    pub fundeps: Vec<FunctionalDependency>,
    pub members: Vec<ClassMember>,
    pub kind_signature: Option<Type>,
    pub span: TextRange,
}

/// A functional dependency `from -> to` on a class head. Both sides name class
/// type parameters, before resolution to parameter positions.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FunctionalDependency {
    pub from: Vec<Name>,
    pub to: Vec<Name>,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassMember {
    pub name: Name,
    pub signature: Option<Type>,
    pub span: TextRange,
}

/// An `instance` declaration before name resolution. `context` holds the
/// instance's context constraints (empty for a nullary instance) and `members`
/// are its method implementations as ordinary value declarations.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstanceDeclaration {
    pub name: Name,
    /// Module-local identity shared by the ordered alternatives in one
    /// `instance ... else instance ...` chain. Ordinary instances each get a
    /// singleton chain identity so later phases can group without guessing
    /// from source spans or names.
    pub chain_id: u32,
    /// Zero-based source order within `chain_id`.
    pub chain_position: u32,
    pub context: Vec<Type>,
    pub head: Type,
    pub members: Vec<crate::Declaration>,
    /// The compiler derivation strategy requested by `derive instance`.
    /// Generated methods are elaborated after name resolution, when the class
    /// and constructor identities are stable.
    pub derivation: Option<DerivationStrategy>,
    pub span: TextRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DerivationStrategy {
    KnownClass,
    Newtype,
}

/// Assigns module-local identities and positions to singleton instances and
/// `else instance` branches while AST lowering walks declarations in order.
#[derive(Default)]
pub(crate) struct InstanceChainTracker {
    next_chain_id: u32,
    previous: Option<(u32, u32)>,
}

impl InstanceChainTracker {
    pub(crate) fn reset(&mut self) {
        self.previous = None;
    }

    pub(crate) fn next(
        &mut self,
        declaration: &cst::InstanceDeclaration,
    ) -> Result<(u32, u32), LowerError> {
        let next = if declaration.else_keyword_span.is_some() {
            match self.previous {
                Some((chain_id, previous_position)) => (chain_id, previous_position + 1),
                None => {
                    return Err(LowerError::new(
                        declaration.span,
                        "an `else instance` must follow an instance in the same chain",
                    ));
                }
            }
        } else {
            let chain_id = self.next_chain_id;
            self.next_chain_id += 1;
            (chain_id, 0)
        };
        self.previous = Some(next);
        Ok(next)
    }

    pub(crate) fn next_singleton(&mut self) -> u32 {
        let chain_id = self.next_chain_id;
        self.next_chain_id += 1;
        self.previous = None;
        chain_id
    }
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
    let kind_signature = kind_signature
        .map(|signature| lower_type(signature.kind))
        .transpose()?;
    match declaration {
        cst::Declaration::Data(declaration) => Ok(TypeDeclaration::Data(DataDeclaration {
            name: lower_name(declaration.name),
            parameters: lower_type_parameters(declaration.parameters)?,
            constructors: declaration
                .constructors
                .into_iter()
                .map(lower_constructor)
                .collect::<Result<_, _>>()?,
            kind_signature,
            span: declaration.span,
        })),
        cst::Declaration::Newtype(declaration) => {
            Ok(TypeDeclaration::Newtype(NewtypeDeclaration {
                name: lower_name(declaration.name),
                parameters: lower_type_parameters(declaration.parameters)?,
                constructor: declaration.constructor.map(lower_constructor).transpose()?,
                kind_signature,
                span: declaration.span,
            }))
        }
        cst::Declaration::TypeSynonym(declaration) => {
            Ok(TypeDeclaration::TypeSynonym(TypeSynonymDeclaration {
                name: lower_name(declaration.name),
                parameters: lower_type_parameters(declaration.parameters)?,
                body: lower_type(declaration.body)?,
                kind_signature,
                span: declaration.span,
            }))
        }
        cst::Declaration::Class(declaration) => Ok(TypeDeclaration::Class(ClassDeclaration {
            name: lower_name(declaration.name),
            parameters: lower_class_parameters(&declaration.head)?,
            superclasses: match declaration.superclasses {
                Some(superclasses) => lower_constraints(*superclasses)?,
                None => Vec::new(),
            },
            fundeps: lower_fundeps(&declaration.fundeps)?,
            members: declaration
                .where_block
                .map(|block| lower_class_members(block.declarations))
                .transpose()?
                .unwrap_or_default(),
            kind_signature,
            span: declaration.span,
        })),
        other => Err(LowerError::new(
            other.span(),
            "expected a data, newtype, type, or class declaration",
        )),
    }
}

/// Lowers `foreign import data Name :: Kind`. A WIT binding belongs to a
/// foreign value import, not to an opaque type.
pub(crate) fn lower_foreign_data(
    declaration: cst::ForeignDeclaration,
) -> Result<TypeDeclaration, LowerError> {
    if declaration.binding.is_some() {
        return Err(LowerError::new(
            declaration.span,
            "a foreign data declaration does not take a WIT binding",
        ));
    }
    Ok(TypeDeclaration::Foreign(ForeignDataDeclaration {
        name: lower_name(declaration.name),
        declared_kind: lower_type(declaration.type_expr)?,
        span: declaration.span,
    }))
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

fn lower_class_parameters(head: &cst::TypeExpr) -> Result<Vec<TypeParameter>, LowerError> {
    let mut parameters = Vec::new();
    collect_parameters(head, &mut parameters)?;
    Ok(parameters)
}

fn lower_fundeps(
    fundeps: &[cst::FunctionalDependency],
) -> Result<Vec<FunctionalDependency>, LowerError> {
    fundeps
        .iter()
        .map(|fundep| {
            Ok(FunctionalDependency {
                from: fundep.from.iter().cloned().map(lower_name).collect(),
                to: fundep.to.iter().cloned().map(lower_name).collect(),
                span: fundep.span,
            })
        })
        .collect()
}

fn collect_parameters(
    expression: &cst::TypeExpr,
    out: &mut Vec<TypeParameter>,
) -> Result<(), LowerError> {
    if let cst::TypeExprKind::Application(function, arguments) = &expression.kind {
        collect_parameters(function, out)?;
        for argument in arguments {
            let argument = strip_parens(argument);
            match &argument.kind {
                cst::TypeExprKind::Name(name) if is_type_variable(&name.text) => {
                    out.push(TypeParameter {
                        name: lower_name(name.clone()),
                        kind: None,
                        span: name.span,
                    });
                }
                cst::TypeExprKind::KindAnnotation {
                    expression: value,
                    kind,
                    ..
                } => {
                    let value = strip_parens(value);
                    if let cst::TypeExprKind::Name(name) = &value.kind
                        && is_type_variable(&name.text)
                    {
                        out.push(TypeParameter {
                            name: lower_name(name.clone()),
                            kind: Some(lower_type(kind.as_ref().clone())?),
                            span: value.span,
                        });
                    }
                }
                // In a class head, `(a :: k)` is a kinded class parameter.
                // The CST parser represents this parenthesized form as a
                // one-field row because the same token sequence also spells a
                // row type. At this boundary the class-head context makes the
                // binder interpretation explicit.
                cst::TypeExprKind::Row { fields, tail, .. }
                    if tail.is_none()
                        && fields.len() == 1
                        && is_type_variable(&fields[0].label.text) =>
                {
                    let field = &fields[0];
                    out.push(TypeParameter {
                        name: lower_name(field.label.clone()),
                        kind: Some(lower_type(field.type_expr.clone())?),
                        span: field.label.span,
                    });
                }
                _ => {}
            }
        }
    }
    Ok(())
}

fn strip_parens(expression: &cst::TypeExpr) -> &cst::TypeExpr {
    match &expression.kind {
        cst::TypeExprKind::Parens { expression, .. } => strip_parens(expression),
        _ => expression,
    }
}

/// Lowers a class superclass or instance context type expression. A
/// parenthesized tuple `(C a, D a)` denotes several constraints and is
/// flattened into one type per item; any other expression is a single
/// constraint.
fn lower_constraints(expression: cst::TypeExpr) -> Result<Vec<Type>, LowerError> {
    match expression.kind {
        cst::TypeExprKind::Tuple { items, .. } => items.into_iter().map(lower_type).collect(),
        cst::TypeExprKind::Parens { expression, .. } => lower_constraints(*expression),
        kind => Ok(vec![lower_type(cst::TypeExpr {
            kind,
            span: expression.span,
        })?]),
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
            cst::Declaration::Value(value) => {
                return Err(LowerError::new(
                    value.span,
                    "class bodies permit method signatures only; implementations belong in instances",
                ));
            }
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

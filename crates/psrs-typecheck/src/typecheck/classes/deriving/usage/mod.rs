//! Field-usage and variance analysis for structural deriving rules.
//!
//! A rule does not decide per field whether to map, recurse, or reject from the
//! field's shape alone. It first walks the field with the class's variance and
//! the visible instance environment: a field occurrence is acceptable only when
//! the mapping class it would use has a visible instance, and a parameter used
//! at the wrong polarity is rejected. The walk returns a [`FieldUsage`] tree so
//! the generator maps exactly the occurrences the analysis accepted. The
//! official compiler performs the same walk in `TypeChecker/Deriving.hs`
//! (`validateParamsInTypeConstructors` / `typeToUsageOf`).

use super::types::{contains_parameter, flatten_type_application};
use super::{KnownClass, flatten_spine};
use crate::typecheck::*;

mod mapping;
pub(super) use mapping::MappingMethods;

/// The classes a `Functor`-shaped rule uses to map a field: a monomorphic
/// class, a bipartite class, and the contravariant/profunctor counterparts used
/// when a field occurrence flips polarity.
#[derive(Clone, Copy)]
pub(super) struct MappingClasses {
    pub mono: KnownClass,
    pub bi: KnownClass,
    pub contra: Option<KnownClass>,
    pub pro: Option<KnownClass>,
}

impl MappingClasses {
    /// The covariant classes shared by `Functor`, `Bifunctor`, `Contravariant`,
    /// and `Profunctor` deriving.
    pub(super) fn covariant() -> Self {
        Self {
            mono: KnownClass::Functor,
            bi: KnownClass::Bifunctor,
            contra: Some(KnownClass::Contravariant),
            pro: Some(KnownClass::Profunctor),
        }
    }

    /// The fold classes (`Foldable`/`Bifoldable`), which have no contravariant
    /// counterpart.
    pub(super) fn foldable() -> Self {
        Self {
            mono: KnownClass::Foldable,
            bi: KnownClass::Bifoldable,
            contra: None,
            pro: None,
        }
    }

    /// The traversal classes (`Traversable`/`Bitraversable`).
    pub(super) fn traversable() -> Self {
        Self {
            mono: KnownClass::Traversable,
            bi: KnownClass::Bitraversable,
            contra: None,
            pro: None,
        }
    }
}

/// How one constructor field uses the parameters. The generator maps exactly
/// the occurrences this tree records.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum FieldUsage {
    /// The field does not mention a mapped parameter.
    Inert,
    /// The field is the final (covariant) parameter itself.
    Param,
    /// The field is the left parameter itself (bipartite rules).
    LParam,
    /// The field maps through the monomorphic class, e.g. `Array a`.
    Mono(Box<FieldUsage>),
    /// The field maps through the bipartite class, e.g. `Either e a`.
    Bi(Box<FieldUsage>, Box<FieldUsage>),
    /// The field maps through the contravariant class.
    Contra(Box<FieldUsage>),
    /// The field maps through the profunctor class, e.g. `p a c`.
    Pro(Box<FieldUsage>, Box<FieldUsage>),
    /// Each record field is traversed independently in canonical label order.
    Record(Vec<(String, FieldUsage)>),
}

/// The head a field application maps through. `Arrow` is the function type,
/// which every mapping class reaches through its own instance.
#[derive(Clone, Copy)]
enum MappingHead<'a> {
    Arrow,
    Constructor(&'a hir::Type),
}

struct UsageContext<'a> {
    classes: MappingClasses,
    fixed: HashMap<String, InferType>,
    lparam: Option<&'a str>,
    param: &'a str,
    lparam_contra: bool,
    param_contra: bool,
}

impl Checker {
    /// Validates every constructor field of a structural derivation against the
    /// class's variance and the visible instance environment, returning the
    /// usage tree per constructor and field. The first unmappable occurrence's
    /// span is the error; the caller reports it as
    /// `CannotDeriveInvalidConstructorArg`.
    pub(super) fn validate_field_usage(
        &self,
        declaration: &hir::TypeDeclaration,
        classes: MappingClasses,
        lparam: Option<&str>,
        param: &str,
        variance: (bool, bool),
        prefix: &[InferType],
    ) -> Result<Vec<Vec<FieldUsage>>, TextRange> {
        let context = UsageContext {
            classes,
            fixed: declaration
                .parameters
                .iter()
                .zip(prefix)
                .map(|(parameter, argument)| (parameter.name.clone(), argument.clone()))
                .collect(),
            lparam,
            param,
            lparam_contra: variance.0,
            param_contra: variance.1,
        };
        let mut constructors = Vec::with_capacity(declaration.constructors.len());
        for constructor in &declaration.constructors {
            let mut fields = Vec::with_capacity(constructor.fields.len());
            for field in &constructor.fields {
                let field = self.normalize_deriving_type(field);
                fields.push(self.check_field(&field, &context, false)?);
            }
            constructors.push(fields);
        }
        Ok(constructors)
    }

    fn check_field(
        &self,
        ty: &hir::Type,
        context: &UsageContext<'_>,
        negative: bool,
    ) -> Result<FieldUsage, TextRange> {
        if !mentions_parameter(ty, context.lparam, context.param) {
            return Ok(FieldUsage::Inert);
        }
        match &ty.kind {
            hir::TypeKind::Variable(name) => {
                let contra = if name == context.param {
                    context.param_contra
                } else if context.lparam == Some(name.as_str()) {
                    context.lparam_contra
                } else {
                    return Ok(FieldUsage::Inert);
                };
                if contra != negative {
                    return Err(ty.span);
                }
                if name == context.param {
                    Ok(FieldUsage::Param)
                } else {
                    Ok(FieldUsage::LParam)
                }
            }
            hir::TypeKind::Function { parameter, result } => {
                self.try_bipartite(context, MappingHead::Arrow, parameter, result, negative)
            }
            hir::TypeKind::Record { fields, tail } => {
                if tail
                    .as_deref()
                    .is_some_and(|tail| mentions_parameter(tail, context.lparam, context.param))
                {
                    return Err(ty.span);
                }
                let mut fields = fields
                    .iter()
                    .map(|field| {
                        Ok((
                            field.label.clone(),
                            self.check_field(&field.ty, context, negative)?,
                        ))
                    })
                    .collect::<Result<Vec<_>, TextRange>>()?;
                fields.sort_by(|left, right| left.0.cmp(&right.0));
                Ok(FieldUsage::Record(fields))
            }
            hir::TypeKind::Row { .. } => Err(ty.span),
            hir::TypeKind::Forall { variables, body } => {
                let shadows = |name: &str| variables.iter().any(|variable| variable.name == name);
                let scoped = UsageContext {
                    classes: context.classes,
                    fixed: context
                        .fixed
                        .iter()
                        .filter(|(name, _)| !shadows(name))
                        .map(|(name, ty)| (name.clone(), ty.clone()))
                        .collect(),
                    lparam: context.lparam.filter(|name| !shadows(name)),
                    param: if shadows(context.param) {
                        ""
                    } else {
                        context.param
                    },
                    lparam_contra: context.lparam_contra,
                    param_contra: context.param_contra,
                };
                self.check_field(body, &scoped, negative)
            }
            hir::TypeKind::Constrained { body, .. } => self.check_field(body, context, negative),
            hir::TypeKind::Application(function, argument) => {
                let (head, _) = flatten_type_application(ty);
                if mentions_parameter(head, context.lparam, context.param) {
                    return Err(head.span);
                }
                match &function.kind {
                    hir::TypeKind::Application(inner, l_arg) => {
                        if mentions_parameter(inner, context.lparam, context.param) {
                            return Err(inner.span);
                        }
                        let head = flatten_type_application(inner).0;
                        self.try_bipartite(
                            context,
                            MappingHead::Constructor(head),
                            l_arg,
                            argument,
                            negative,
                        )
                    }
                    _ => {
                        if mentions_parameter(function, context.lparam, context.param) {
                            return Err(function.span);
                        }
                        let head = flatten_type_application(function).0;
                        self.try_mono(context, MappingHead::Constructor(head), argument, negative)
                    }
                }
            }
            _ => Ok(FieldUsage::Inert),
        }
    }

    /// A two-argument application: map through the bipartite class, or the
    /// profunctor class with the left argument contravariant, or fall back to
    /// the mono class on the final argument with the left one inert.
    fn try_bipartite(
        &self,
        context: &UsageContext<'_>,
        head: MappingHead<'_>,
        l_arg: &hir::Type,
        r_arg: &hir::Type,
        negative: bool,
    ) -> Result<FieldUsage, TextRange> {
        if self.has_mapping_instance(context.classes.bi, head, context) {
            let left = self.check_field(l_arg, context, negative)?;
            let right = self.check_field(r_arg, context, negative)?;
            if left == FieldUsage::Inert
                && self.has_mapping_instance(context.classes.mono, head, context)
            {
                return Ok(FieldUsage::Mono(Box::new(right)));
            }
            return Ok(FieldUsage::Bi(Box::new(left), Box::new(right)));
        }
        if let Some(pro) = context.classes.pro
            && self.has_mapping_instance(pro, head, context)
        {
            let left = self.check_field(l_arg, context, !negative)?;
            let right = self.check_field(r_arg, context, negative)?;
            if left == FieldUsage::Inert
                && self.has_mapping_instance(context.classes.mono, head, context)
            {
                return Ok(FieldUsage::Mono(Box::new(right)));
            }
            return Ok(FieldUsage::Pro(Box::new(left), Box::new(right)));
        }
        if mentions_parameter(l_arg, context.lparam, context.param) {
            return Err(l_arg.span);
        }
        self.try_mono(context, head, r_arg, negative)
    }

    /// A one-argument application. When no mapping instance is visible, the
    /// occurrence is only inert if it does not mention a parameter.
    fn try_mono(
        &self,
        context: &UsageContext<'_>,
        head: MappingHead<'_>,
        argument: &hir::Type,
        negative: bool,
    ) -> Result<FieldUsage, TextRange> {
        if self.has_mapping_instance(context.classes.mono, head, context) {
            let inner = self.check_field(argument, context, negative)?;
            return Ok(FieldUsage::Mono(Box::new(inner)));
        }
        if let Some(contra) = context.classes.contra
            && self.has_mapping_instance(contra, head, context)
        {
            let inner = self.check_field(argument, context, !negative)?;
            return Ok(FieldUsage::Contra(Box::new(inner)));
        }
        if mentions_parameter(argument, context.lparam, context.param) {
            Err(argument.span)
        } else {
            Ok(FieldUsage::Inert)
        }
    }

    /// Whether the visible instance environment proves `known` at `head`.
    fn has_mapping_instance(
        &self,
        known: KnownClass,
        head: MappingHead<'_>,
        context: &UsageContext<'_>,
    ) -> bool {
        let Some(class_id) = self.env.deriving.class_id(known) else {
            return false;
        };
        let wanted = match head {
            MappingHead::Arrow => InferType::Constructor(TypeConstructor::Function),
            MappingHead::Constructor(ty) => {
                if let Some(constructor) = field_head_constructor(ty) {
                    InferType::Constructor(constructor)
                } else if let hir::TypeKind::Variable(name) = &ty.kind {
                    let Some(actual) = context.fixed.get(name) else {
                        return false;
                    };
                    flatten_spine(&self.resolve_type(actual.clone())).0.clone()
                } else {
                    return false;
                }
            }
        };
        let agrees = |argument: &InferType| {
            let resolved = self.resolve_type(argument.clone());
            flatten_spine(&resolved).0 == &wanted
        };
        if self.scope.givens.iter().any(|(constraint, _)| {
            constraint.class_id == class_id
                && constraint.arguments.len() == 1
                && agrees(&constraint.arguments[0])
        }) {
            return true;
        }
        let visible = self.instance_candidate_modules(class_id, std::slice::from_ref(&wanted));
        self.env.instances.iter().any(|info| {
            info.class_id == class_id
                && visible.contains(&info.symbol.module)
                && info.head_arguments.len() == 1
                && agrees(&info.head_arguments[0])
        })
    }
}

fn mentions_parameter(ty: &hir::Type, lparam: Option<&str>, param: &str) -> bool {
    contains_parameter(ty, param) || lparam.is_some_and(|lparam| contains_parameter(ty, lparam))
}

/// The constructor a field head names, or `None` when the head is a type
/// variable or a form that cannot carry an instance.
fn field_head_constructor(ty: &hir::Type) -> Option<TypeConstructor> {
    match &ty.kind {
        hir::TypeKind::Constructor(builtin) => Some(builtin_constructor(*builtin)),
        hir::TypeKind::Named(id) | hir::TypeKind::Opaque(id) => Some(TypeConstructor::User(*id)),
        _ => None,
    }
}

fn builtin_constructor(builtin: hir::BuiltinType) -> TypeConstructor {
    match builtin {
        hir::BuiltinType::Int => TypeConstructor::Int,
        hir::BuiltinType::Number => TypeConstructor::Number,
        hir::BuiltinType::Boolean => TypeConstructor::Boolean,
        hir::BuiltinType::String => TypeConstructor::String,
        hir::BuiltinType::Char => TypeConstructor::Char,
        hir::BuiltinType::Unit => TypeConstructor::Unit,
        hir::BuiltinType::Array => TypeConstructor::Array,
        hir::BuiltinType::Function => TypeConstructor::Function,
        hir::BuiltinType::Record => TypeConstructor::Record,
        hir::BuiltinType::Row => TypeConstructor::Row,
        hir::BuiltinType::Type => TypeConstructor::Type,
        hir::BuiltinType::Constraint => TypeConstructor::Constraint,
        hir::BuiltinType::Symbol => TypeConstructor::Symbol,
    }
}

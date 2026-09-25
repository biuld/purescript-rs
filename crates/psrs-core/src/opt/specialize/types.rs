use crate::{
    Binding, CaseBranch, Declaration, Expr, ExprKind, Module, Pattern, PatternKind, Type,
    TypeConstructor, TypeId,
};
use psrs_hir::{SymbolId, TypeVariableId};
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum TypeKey {
    I32,
    F64,
    Boolean,
    String,
    Char,
    Unit,
    Constructor(TypeConstructor),
    Application(Box<TypeKey>, Box<TypeKey>),
    Record(Vec<(String, TypeKey)>),
    Function(Box<TypeKey>, Box<TypeKey>),
}

pub(super) fn concrete_type_key(module: &Module, id: TypeId) -> Option<TypeKey> {
    type_key(module, id, &mut HashSet::new())
}

pub(super) fn match_instantiation(
    module: &Module,
    declaration: &Declaration,
    call_type: TypeId,
) -> Option<(HashMap<TypeVariableId, TypeId>, Vec<TypeKey>)> {
    if declaration.quantified.is_empty() || concrete_type_key(module, call_type).is_none() {
        return None;
    }
    let quantifiers = declaration
        .quantified
        .iter()
        .copied()
        .collect::<HashSet<_>>();
    let mut replacements = HashMap::new();
    if !match_type(
        module,
        declaration.ty,
        call_type,
        &quantifiers,
        &mut replacements,
        &mut HashSet::new(),
    ) || replacements.len() != quantifiers.len()
    {
        return None;
    }
    let key = declaration
        .quantified
        .iter()
        .map(|variable| concrete_type_key(module, *replacements.get(variable)?))
        .collect::<Option<Vec<_>>>()?;
    Some((replacements, key))
}

pub(super) fn instantiate_declaration(
    module: &mut Module,
    declaration: &Declaration,
    replacements: &HashMap<TypeVariableId, TypeId>,
    symbol: SymbolId,
    serial: usize,
) -> Option<Declaration> {
    let mut substitution = TypeSubstitution {
        module,
        replacements,
        cache: HashMap::new(),
        active: HashSet::new(),
    };
    let ty = substitution.type_id(declaration.ty)?;
    let value = substitution.expression(&declaration.value)?;
    Some(Declaration {
        symbol,
        name: format!("{}$p7_{serial}", declaration.name),
        name_span: declaration.name_span,
        quantified: Vec::new(),
        ty,
        value,
        span: declaration.span,
    })
}

fn type_key(module: &Module, id: TypeId, active: &mut HashSet<TypeId>) -> Option<TypeKey> {
    if !active.insert(id) {
        return None;
    }
    let result = match module.types.get(id.0 as usize)? {
        Type::Variable(_) => None,
        Type::I32 => Some(TypeKey::I32),
        Type::F64 => Some(TypeKey::F64),
        Type::Boolean => Some(TypeKey::Boolean),
        Type::String => Some(TypeKey::String),
        Type::Char => Some(TypeKey::Char),
        Type::Unit => Some(TypeKey::Unit),
        Type::Constructor(constructor) => Some(TypeKey::Constructor(*constructor)),
        Type::Application(function, argument) => Some(TypeKey::Application(
            Box::new(type_key(module, *function, active)?),
            Box::new(type_key(module, *argument, active)?),
        )),
        Type::Function { parameter, result } => Some(TypeKey::Function(
            Box::new(type_key(module, *parameter, active)?),
            Box::new(type_key(module, *result, active)?),
        )),
        Type::Record(fields) => {
            let mut keys = fields
                .iter()
                .map(|(label, field)| Some((label.clone(), type_key(module, *field, active)?)))
                .collect::<Option<Vec<_>>>()?;
            keys.sort_by(|left, right| left.0.cmp(&right.0));
            Some(TypeKey::Record(keys))
        }
    };
    active.remove(&id);
    result
}

fn match_type(
    module: &Module,
    generic: TypeId,
    concrete: TypeId,
    quantifiers: &HashSet<TypeVariableId>,
    replacements: &mut HashMap<TypeVariableId, TypeId>,
    active: &mut HashSet<(TypeId, TypeId)>,
) -> bool {
    if !active.insert((generic, concrete)) {
        return true;
    }
    let (Some(generic_type), Some(concrete_type)) = (
        module.types.get(generic.0 as usize),
        module.types.get(concrete.0 as usize),
    ) else {
        return false;
    };
    if let Type::Variable(variable) = generic_type
        && quantifiers.contains(variable)
    {
        let Some(key) = concrete_type_key(module, concrete) else {
            return false;
        };
        if let Some(previous) = replacements.get(variable) {
            return concrete_type_key(module, *previous).as_ref() == Some(&key);
        }
        replacements.insert(*variable, concrete);
        return true;
    }
    match (generic_type, concrete_type) {
        (Type::I32, Type::I32)
        | (Type::F64, Type::F64)
        | (Type::Boolean, Type::Boolean)
        | (Type::String, Type::String)
        | (Type::Char, Type::Char)
        | (Type::Unit, Type::Unit) => true,
        (Type::Constructor(left), Type::Constructor(right)) => left == right,
        (Type::Application(gf, ga), Type::Application(cf, ca)) => {
            match_type(module, *gf, *cf, quantifiers, replacements, active)
                && match_type(module, *ga, *ca, quantifiers, replacements, active)
        }
        (
            Type::Function {
                parameter: gp,
                result: gr,
            },
            Type::Function {
                parameter: cp,
                result: cr,
            },
        ) => {
            match_type(module, *gp, *cp, quantifiers, replacements, active)
                && match_type(module, *gr, *cr, quantifiers, replacements, active)
        }
        (Type::Record(generic_fields), Type::Record(concrete_fields)) => {
            if generic_fields.len() != concrete_fields.len() {
                return false;
            }
            generic_fields.iter().all(|(label, generic_field)| {
                concrete_fields
                    .iter()
                    .find(|(other, _)| other == label)
                    .is_some_and(|(_, concrete_field)| {
                        match_type(
                            module,
                            *generic_field,
                            *concrete_field,
                            quantifiers,
                            replacements,
                            active,
                        )
                    })
            })
        }
        _ => false,
    }
}

struct TypeSubstitution<'a> {
    module: &'a mut Module,
    replacements: &'a HashMap<TypeVariableId, TypeId>,
    cache: HashMap<TypeId, TypeId>,
    active: HashSet<TypeId>,
}

impl TypeSubstitution<'_> {
    fn type_id(&mut self, id: TypeId) -> Option<TypeId> {
        if let Some(substituted) = self.cache.get(&id) {
            return Some(*substituted);
        }
        if !self.active.insert(id) {
            return None;
        }
        let ty = self.module.types.get(id.0 as usize)?.clone();
        let substituted = match ty {
            Type::Variable(variable) => self.replacements.get(&variable).copied().unwrap_or(id),
            Type::Application(function, argument) => {
                let function = self.type_id(function)?;
                let argument = self.type_id(argument)?;
                self.intern(Type::Application(function, argument))?
            }
            Type::Function { parameter, result } => {
                let parameter = self.type_id(parameter)?;
                let result = self.type_id(result)?;
                self.intern(Type::Function { parameter, result })?
            }
            Type::Record(fields) => {
                let fields = fields
                    .into_iter()
                    .map(|(label, field)| Some((label, self.type_id(field)?)))
                    .collect::<Option<Vec<_>>>()?;
                self.intern(Type::Record(fields))?
            }
            other => id_for_existing(self.module, &other).unwrap_or(id),
        };
        self.active.remove(&id);
        self.cache.insert(id, substituted);
        Some(substituted)
    }

    fn intern(&mut self, ty: Type) -> Option<TypeId> {
        if let Some(index) = self
            .module
            .types
            .iter()
            .position(|candidate| *candidate == ty)
        {
            return u32::try_from(index).ok().map(TypeId);
        }
        let id = TypeId(u32::try_from(self.module.types.len()).ok()?);
        self.module.types.push(ty);
        Some(id)
    }

    fn expression(&mut self, expression: &Expr) -> Option<Expr> {
        let kind = match &expression.kind {
            ExprKind::Local(id) => ExprKind::Local(*id),
            ExprKind::Global(symbol) => ExprKind::Global(*symbol),
            ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
                symbol: *symbol,
                arguments: arguments
                    .iter()
                    .map(|argument| self.expression(argument))
                    .collect::<Option<Vec<_>>>()?,
            },
            ExprKind::Integer(value) => ExprKind::Integer(*value),
            ExprKind::Number(value) => ExprKind::Number(value.clone()),
            ExprKind::Boolean(value) => ExprKind::Boolean(*value),
            ExprKind::String(value) => ExprKind::String(value.clone()),
            ExprKind::Char(value) => ExprKind::Char(*value),
            ExprKind::Array { elements } => ExprKind::Array {
                elements: elements
                    .iter()
                    .map(|element| self.expression(element))
                    .collect::<Option<Vec<_>>>()?,
            },
            ExprKind::Record { fields } => ExprKind::Record {
                fields: fields
                    .iter()
                    .map(|(label, value)| Some((label.clone(), self.expression(value)?)))
                    .collect::<Option<Vec<_>>>()?,
            },
            ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
                record: Box::new(self.expression(record)?),
                fields: fields
                    .iter()
                    .map(|(label, value)| Some((label.clone(), self.expression(value)?)))
                    .collect::<Option<Vec<_>>>()?,
            },
            ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
                record: Box::new(self.expression(record)?),
                field: field.clone(),
            },
            ExprKind::ArrayLength(array) => {
                ExprKind::ArrayLength(Box::new(self.expression(array)?))
            }
            ExprKind::ArrayIndex { array, index } => ExprKind::ArrayIndex {
                array: Box::new(self.expression(array)?),
                index: Box::new(self.expression(index)?),
            },
            ExprKind::ArrayUpdate {
                array,
                index,
                value,
            } => ExprKind::ArrayUpdate {
                array: Box::new(self.expression(array)?),
                index: Box::new(self.expression(index)?),
                value: Box::new(self.expression(value)?),
            },
            ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
                op: *op,
                left: Box::new(self.expression(left)?),
                right: Box::new(self.expression(right)?),
            },
            ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
                op: *op,
                value: Box::new(self.expression(value)?),
            },
            ExprKind::Application(function, argument) => ExprKind::Application(
                Box::new(self.expression(function)?),
                Box::new(self.expression(argument)?),
            ),
            ExprKind::Lambda { binder, body } => ExprKind::Lambda {
                binder: crate::Binder {
                    id: binder.id,
                    name: binder.name.clone(),
                    ty: self.type_id(binder.ty)?,
                    span: binder.span,
                },
                body: Box::new(self.expression(body)?),
            },
            ExprKind::Let { bindings, body } => ExprKind::Let {
                bindings: bindings
                    .iter()
                    .map(|binding| self.binding(binding))
                    .collect::<Option<Vec<_>>>()?,
                body: Box::new(self.expression(body)?),
            },
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => ExprKind::If {
                condition: Box::new(self.expression(condition)?),
                then_branch: Box::new(self.expression(then_branch)?),
                else_branch: Box::new(self.expression(else_branch)?),
            },
            ExprKind::Case {
                scrutinee,
                branches,
            } => ExprKind::Case {
                scrutinee: Box::new(self.expression(scrutinee)?),
                branches: branches
                    .iter()
                    .map(|branch| self.branch(branch))
                    .collect::<Option<Vec<_>>>()?,
            },
        };
        Some(Expr {
            kind,
            ty: self.type_id(expression.ty)?,
            span: expression.span,
        })
    }

    fn binding(&mut self, binding: &Binding) -> Option<Binding> {
        Some(Binding {
            binder: crate::Binder {
                id: binding.binder.id,
                name: binding.binder.name.clone(),
                ty: self.type_id(binding.binder.ty)?,
                span: binding.binder.span,
            },
            quantified: binding.quantified.clone(),
            value: self.expression(&binding.value)?,
            span: binding.span,
        })
    }

    fn branch(&mut self, branch: &CaseBranch) -> Option<CaseBranch> {
        Some(CaseBranch {
            pattern: self.pattern(&branch.pattern)?,
            value: self.expression(&branch.value)?,
            span: branch.span,
        })
    }

    fn pattern(&mut self, pattern: &Pattern) -> Option<Pattern> {
        let kind = match &pattern.kind {
            PatternKind::Wildcard => PatternKind::Wildcard,
            PatternKind::Var { id, ty } => PatternKind::Var {
                id: *id,
                ty: self.type_id(*ty)?,
            },
            PatternKind::Constructor { symbol, arguments } => PatternKind::Constructor {
                symbol: *symbol,
                arguments: arguments
                    .iter()
                    .map(|argument| self.pattern(argument))
                    .collect::<Option<Vec<_>>>()?,
            },
            PatternKind::Record { fields } => PatternKind::Record {
                fields: fields
                    .iter()
                    .map(|(label, pattern)| Some((label.clone(), self.pattern(pattern)?)))
                    .collect::<Option<Vec<_>>>()?,
            },
        };
        Some(Pattern {
            kind,
            ty: self.type_id(pattern.ty)?,
            span: pattern.span,
        })
    }
}

fn id_for_existing(module: &Module, ty: &Type) -> Option<TypeId> {
    module
        .types
        .iter()
        .position(|candidate| candidate == ty)
        .and_then(|index| u32::try_from(index).ok())
        .map(TypeId)
}

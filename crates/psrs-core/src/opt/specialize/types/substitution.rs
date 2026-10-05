use crate::{
    Binding, CaseBranch, Declaration, Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeId,
};
use psrs_hir::{SymbolId, TypeVariableId};
use std::collections::{HashMap, HashSet};

pub(in crate::opt::specialize) fn instantiate_declaration(
    module: &mut Module,
    declaration: &Declaration,
    replacements: &HashMap<TypeVariableId, TypeId>,
    symbol: SymbolId,
    serial: usize,
) -> Option<Declaration> {
    let mut replacement_free_variables = HashSet::new();
    for id in replacements.values() {
        free_variables(
            module,
            *id,
            &mut HashSet::new(),
            &mut HashSet::new(),
            &mut replacement_free_variables,
        );
    }
    let used_variables = all_variables(module);
    let mut substitution = TypeSubstitution {
        module,
        replacements,
        replacement_free_variables,
        used_variables,
        shadowed: HashSet::new(),
        renamed: HashMap::new(),
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

struct TypeSubstitution<'a> {
    module: &'a mut Module,
    replacements: &'a HashMap<TypeVariableId, TypeId>,
    replacement_free_variables: HashSet<TypeVariableId>,
    used_variables: HashSet<TypeVariableId>,
    shadowed: HashSet<TypeVariableId>,
    renamed: HashMap<TypeVariableId, TypeVariableId>,
    active: HashSet<TypeId>,
}

impl TypeSubstitution<'_> {
    fn type_id(&mut self, id: TypeId) -> Option<TypeId> {
        if !self.active.insert(id) {
            return None;
        }
        let ty = self.module.types.get(id.0 as usize)?.clone();
        let substituted = match ty {
            Type::Variable(variable) => {
                if self.shadowed.contains(&variable) {
                    if let Some(renamed) = self.renamed.get(&variable).copied() {
                        self.intern(Type::Variable(renamed))?
                    } else {
                        id
                    }
                } else {
                    self.replacements.get(&variable).copied().unwrap_or(id)
                }
            }
            Type::Application(function, argument) => {
                let function = self.type_id(function)?;
                let argument = self.type_id(argument)?;
                self.intern(Type::Application(function, argument))?
            }
            Type::ForAll { variables, body } => {
                let mut renamed_variables = variables.clone();
                let mut previous_renamings = Vec::new();
                let mut inserted_shadowed = Vec::new();
                for (index, variable) in variables.iter().copied().enumerate() {
                    if !self.shadowed.contains(&variable)
                        && self.replacement_free_variables.contains(&variable)
                    {
                        let fresh = self.fresh_variable()?;
                        renamed_variables[index] = fresh;
                        previous_renamings.push((variable, self.renamed.insert(variable, fresh)));
                    }
                    if self.shadowed.insert(variable) {
                        inserted_shadowed.push(variable);
                    }
                }
                let body = self.type_id(body)?;
                for variable in inserted_shadowed {
                    self.shadowed.remove(&variable);
                }
                for (variable, previous) in previous_renamings.into_iter().rev() {
                    if let Some(previous) = previous {
                        self.renamed.insert(variable, previous);
                    } else {
                        self.renamed.remove(&variable);
                    }
                }
                self.intern(Type::ForAll {
                    variables: renamed_variables,
                    body,
                })?
            }
            Type::RowExtend { label, ty, tail } => {
                let ty = self.type_id(ty)?;
                let tail = self.type_id(tail)?;
                self.intern(Type::RowExtend { label, ty, tail })?
            }
            Type::Closure { parameters, result } => {
                let parameters = parameters
                    .into_iter()
                    .map(|parameter| self.type_id(parameter))
                    .collect::<Option<Vec<_>>>()?;
                let result = self.type_id(result)?;
                self.intern(Type::Closure { parameters, result })?
            }
            Type::RowEmpty
            | Type::Constructor(_)
            | Type::TypeLevelString(_)
            | Type::TypeLevelInt(_) => id,
        };
        self.active.remove(&id);
        Some(substituted)
    }

    fn fresh_variable(&mut self) -> Option<TypeVariableId> {
        let mut candidate = match self.used_variables.iter().map(|variable| variable.0).max() {
            Some(maximum) => maximum.checked_add(1)?,
            None => 0,
        };
        loop {
            let variable = TypeVariableId(candidate);
            if self.used_variables.insert(variable) {
                return Some(variable);
            }
            candidate = candidate.checked_add(1)?;
        }
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
            ExprKind::IntrinsicCall {
                intrinsic,
                arguments,
            } => ExprKind::IntrinsicCall {
                intrinsic: *intrinsic,
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
            ExprKind::Unit => ExprKind::Unit,
            ExprKind::StateToken => ExprKind::StateToken,
            ExprKind::Trap => ExprKind::Trap,
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
            ExprKind::RepresentationCast {
                value,
                source_type,
                target_type,
            } => ExprKind::RepresentationCast {
                value: Box::new(self.expression(value)?),
                source_type: self.type_id(*source_type)?,
                target_type: self.type_id(*target_type)?,
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
            coverage: branch.coverage,
        })
    }

    fn pattern(&mut self, pattern: &Pattern) -> Option<Pattern> {
        let kind = match &pattern.kind {
            PatternKind::Wildcard => PatternKind::Wildcard,
            PatternKind::Literal { value } => PatternKind::Literal {
                value: value.clone(),
            },
            PatternKind::Array { elements } => PatternKind::Array {
                elements: elements
                    .iter()
                    .map(|element| self.pattern(element))
                    .collect::<Option<Vec<_>>>()?,
            },
            PatternKind::Named { id, pattern } => PatternKind::Named {
                id: *id,
                pattern: Box::new(self.pattern(pattern)?),
            },
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

fn free_variables(
    module: &Module,
    id: TypeId,
    bound: &mut HashSet<TypeVariableId>,
    active: &mut HashSet<TypeId>,
    out: &mut HashSet<TypeVariableId>,
) {
    if !active.insert(id) {
        return;
    }
    match module.types.get(id.0 as usize) {
        Some(Type::Variable(variable)) if !bound.contains(variable) => {
            out.insert(*variable);
        }
        Some(Type::Application(function, argument)) => {
            free_variables(module, *function, bound, active, out);
            free_variables(module, *argument, bound, active, out);
        }
        Some(Type::ForAll { variables, body }) => {
            let inserted = variables
                .iter()
                .copied()
                .filter(|variable| bound.insert(*variable))
                .collect::<Vec<_>>();
            free_variables(module, *body, bound, active, out);
            for variable in inserted {
                bound.remove(&variable);
            }
        }
        Some(Type::RowExtend { ty, tail, .. }) => {
            free_variables(module, *ty, bound, active, out);
            free_variables(module, *tail, bound, active, out);
        }
        _ => {}
    }
    active.remove(&id);
}

fn all_variables(module: &Module) -> HashSet<TypeVariableId> {
    let mut variables = HashSet::new();
    for ty in &module.types {
        match ty {
            Type::Variable(variable) => {
                variables.insert(*variable);
            }
            Type::ForAll {
                variables: binders, ..
            } => {
                variables.extend(binders.iter().copied());
            }
            _ => {}
        }
    }
    for declaration in &module.declarations {
        variables.extend(declaration.quantified.iter().copied());
    }
    variables
}

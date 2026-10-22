use super::*;
use crate::kind::type_kind;

impl Checker<'_> {
    /// Walks annotations inside the expressions the module's own values and
    /// instance members carry.
    ///
    /// A type variable an annotation names is in scope when one of the
    /// binders the enclosing declaration *as written* quantifies names it,
    /// which is the official `withScopedTypeVars` scoping: synonym expansion
    /// must not bring a variable into scope (official #2542), so only the
    /// `forall` binders written in the declaration's signature count. Instance
    /// members additionally see the free variables the instance head and its
    /// context are written with, which official `checkInstanceDeclaration`
    /// binds for the whole instance. Everything else is an undefined type
    /// variable, reported before the kind itself is read.
    pub(super) fn check_local_type_annotations(&mut self, module: &psrs_hir::Module) {
        for declaration in &module.declarations {
            let mut bound = Vec::new();
            if let Some(signature) = &declaration.signature {
                self.collect_written_binders(signature, &mut bound);
            }
            self.check_expression_annotations(&declaration.value, &mut bound);
        }
        for instance in &module.instances {
            let mut bound = Vec::new();
            self.collect_free_variable_names(&instance.head, &mut bound);
            for constraint in &instance.context {
                self.collect_free_variable_names(constraint, &mut bound);
            }
            for member in &instance.members {
                self.check_expression_annotations(&member.value, &mut bound.clone());
            }
        }
    }

    /// Collects the `forall` binders a written type quantifies, at every
    /// nesting level, without expanding synonyms.
    fn collect_written_binders(&self, ty: &psrs_hir::Type, bound: &mut Vec<String>) {
        match &ty.kind {
            psrs_hir::TypeKind::Forall { variables, body } => {
                for variable in variables {
                    if let Some(annotation) = &variable.kind {
                        self.collect_written_binders(annotation, bound);
                    }
                    bound.push(variable.name.clone());
                }
                self.collect_written_binders(body, bound);
            }
            psrs_hir::TypeKind::Application(function, argument) => {
                self.collect_written_binders(function, bound);
                self.collect_written_binders(argument, bound);
            }
            psrs_hir::TypeKind::OperatorChain { operands, .. } => {
                for operand in operands {
                    self.collect_written_binders(operand, bound);
                }
            }
            psrs_hir::TypeKind::Function { parameter, result } => {
                self.collect_written_binders(parameter, bound);
                self.collect_written_binders(result, bound);
            }
            psrs_hir::TypeKind::Constrained { constraint, body } => {
                self.collect_written_binders(constraint, bound);
                self.collect_written_binders(body, bound);
            }
            psrs_hir::TypeKind::Row { fields, tail }
            | psrs_hir::TypeKind::Record { fields, tail } => {
                for field in fields {
                    self.collect_written_binders(&field.ty, bound);
                }
                if let Some(tail) = tail {
                    self.collect_written_binders(tail, bound);
                }
            }
            psrs_hir::TypeKind::Kinded { expression, kind } => {
                self.collect_written_binders(expression, bound);
                self.collect_written_binders(kind, bound);
            }
            _ => {}
        }
    }

    /// Collects the type-variable names written in a type, whatever scope
    /// binds them. Used for an instance's head and context, which official
    /// PureScript binds wholesale for the members' checking.
    fn collect_free_variable_names(&self, ty: &psrs_hir::Type, bound: &mut Vec<String>) {
        match &ty.kind {
            psrs_hir::TypeKind::Variable(name) => bound.push(name.clone()),
            psrs_hir::TypeKind::Forall { variables, body } => {
                self.collect_free_variable_names(body, bound);
                for variable in variables {
                    if let Some(annotation) = &variable.kind {
                        self.collect_free_variable_names(annotation, bound);
                    }
                }
            }
            psrs_hir::TypeKind::Application(function, argument) => {
                self.collect_free_variable_names(function, bound);
                self.collect_free_variable_names(argument, bound);
            }
            psrs_hir::TypeKind::OperatorChain { operands, .. } => {
                for operand in operands {
                    self.collect_free_variable_names(operand, bound);
                }
            }
            psrs_hir::TypeKind::Function { parameter, result } => {
                self.collect_free_variable_names(parameter, bound);
                self.collect_free_variable_names(result, bound);
            }
            psrs_hir::TypeKind::Constrained { constraint, body } => {
                self.collect_free_variable_names(constraint, bound);
                self.collect_free_variable_names(body, bound);
            }
            psrs_hir::TypeKind::Row { fields, tail }
            | psrs_hir::TypeKind::Record { fields, tail } => {
                for field in fields {
                    self.collect_free_variable_names(&field.ty, bound);
                }
                if let Some(tail) = tail {
                    self.collect_free_variable_names(tail, bound);
                }
            }
            psrs_hir::TypeKind::Kinded { expression, kind } => {
                self.collect_free_variable_names(expression, bound);
                self.collect_free_variable_names(kind, bound);
            }
            _ => {}
        }
    }

    fn check_expression_annotations(
        &mut self,
        expression: &psrs_hir::Expr,
        bound: &mut Vec<String>,
    ) {
        psrs_span::with_sufficient_stack(|| {
            self.check_expression_annotations_inner(expression, bound)
        })
    }

    fn check_expression_annotations_inner(
        &mut self,
        expression: &psrs_hir::Expr,
        bound: &mut Vec<String>,
    ) {
        use psrs_hir::ExprKind;
        match &expression.kind {
            ExprKind::Typed { expression, ty } => {
                self.check_ascription_annotation(ty, bound);
                self.check_expression_annotations(expression, bound);
            }
            ExprKind::TypeApplication { expression, ty } => {
                // The argument's kind is the quantifier it instantiates.
                // Official `infer' (VisibleTypeApp)` checks that equation
                // once the operand's scheme is known, and the type checker
                // does the same. Solving the argument against `Type` here
                // rejects a `Symbol` or a higher-kinded argument before that
                // quantifier exists. An undefined variable is still named
                // here, the same way an ascription names one.
                self.check_references(ty, bound);
                self.check_expression_annotations(expression, bound);
            }
            ExprKind::Array(elements) => {
                for element in elements {
                    self.check_expression_annotations(element, bound);
                }
            }
            ExprKind::Record(fields) | ExprKind::MatchProduct(fields) => {
                for (_, value) in fields {
                    self.check_expression_annotations(value, bound);
                }
            }
            ExprKind::RecordUpdate { expression, fields } => {
                self.check_expression_annotations(expression, bound);
                for (_, value) in fields {
                    self.check_expression_annotations(value, bound);
                }
            }
            ExprKind::FieldAccess { expression, .. } => {
                self.check_expression_annotations(expression, bound)
            }
            ExprKind::Application(function, argument) => {
                self.check_expression_annotations(function, bound);
                self.check_expression_annotations(argument, bound);
            }
            ExprKind::Operator { left, right, .. } => {
                self.check_expression_annotations(left, bound);
                self.check_expression_annotations(right, bound);
            }
            ExprKind::Negate {
                function,
                expression,
                ..
            } => {
                self.check_expression_annotations(function, bound);
                self.check_expression_annotations(expression, bound);
            }
            ExprKind::OperatorChain { operands, .. } => {
                for operand in operands {
                    self.check_expression_annotations(operand, bound);
                }
            }
            ExprKind::OperatorSection { operand, .. } => {
                self.check_expression_annotations(operand, bound)
            }
            ExprKind::Lambda { body, .. } => self.check_expression_annotations(body, bound),
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    self.check_expression_annotations(&binding.value, bound);
                }
                self.check_expression_annotations(body, bound);
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expression_annotations(condition, bound);
                self.check_expression_annotations(then_branch, bound);
                self.check_expression_annotations(else_branch, bound);
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                self.check_expression_annotations(scrutinee, bound);
                for branch in branches {
                    self.check_pattern_annotations(&branch.pattern, bound);
                    self.check_expression_annotations(&branch.value, bound);
                }
            }
            ExprKind::Guarded(clauses) => {
                for clause in clauses {
                    for binding in &clause.where_bindings {
                        self.check_expression_annotations(&binding.value, bound);
                    }
                    for guard in &clause.guards {
                        match guard {
                            psrs_hir::Guard::Boolean(value) => {
                                self.check_expression_annotations(value, bound)
                            }
                            psrs_hir::Guard::Pattern { pattern, value } => {
                                self.check_pattern_annotations(pattern, bound);
                                self.check_expression_annotations(value, bound);
                            }
                            psrs_hir::Guard::Let { bindings, .. } => {
                                for binding in bindings {
                                    self.check_expression_annotations(&binding.value, bound);
                                }
                            }
                        }
                    }
                    self.check_expression_annotations(&clause.value, bound);
                }
            }
            ExprKind::Local(_)
            | ExprKind::Global(_)
            | ExprKind::Integer(_)
            | ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Char(_)
            | ExprKind::Hole(_) => {}
        }
    }

    fn check_pattern_annotations(&mut self, pattern: &psrs_hir::Pattern, bound: &mut Vec<String>) {
        match &pattern.kind {
            psrs_hir::PatternKind::Typed { pattern, ty } => {
                self.check_ascription_annotation(ty, bound);
                self.check_pattern_annotations(pattern, bound);
            }
            psrs_hir::PatternKind::Array(elements) => {
                for element in elements {
                    self.check_pattern_annotations(element, bound);
                }
            }
            psrs_hir::PatternKind::Named { pattern, .. } => {
                self.check_pattern_annotations(pattern, bound);
            }
            psrs_hir::PatternKind::Constructor { arguments, .. }
            | psrs_hir::PatternKind::OperatorChain {
                operands: arguments,
                ..
            } => {
                for argument in arguments {
                    self.check_pattern_annotations(argument, bound);
                }
            }
            psrs_hir::PatternKind::Record { fields, .. } => {
                for (_, field) in fields {
                    self.check_pattern_annotations(field, bound);
                }
            }
            psrs_hir::PatternKind::Wildcard
            | psrs_hir::PatternKind::Boolean(_)
            | psrs_hir::PatternKind::Integer(_)
            | psrs_hir::PatternKind::Number(_)
            | psrs_hir::PatternKind::String(_)
            | psrs_hir::PatternKind::Char(_)
            | psrs_hir::PatternKind::Var(_) => {}
        }
    }

    /// The written type of a type ascription is checked against `Type`, and a
    /// type variable it names must be one of the binders the enclosing
    /// declaration wrote.
    fn check_ascription_annotation(&mut self, ty: &psrs_hir::Type, bound: &mut Vec<String>) {
        self.check_references(ty, bound);
        self.check_kind_of_annotation(ty);
    }

    /// Reads a local annotation's kind and solves it against kind `Type`.
    fn check_kind_of_annotation(&mut self, ty: &psrs_hir::Type) {
        let mut kinds = std::collections::HashMap::new();
        let kind = self.kind_of_type(ty, &mut kinds);
        self.check_type_annotation_kind(kind, type_kind(), ty.span);
    }
}

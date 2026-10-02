use super::*;

impl Checker<'_> {
    pub(super) fn check_local_type_annotations(&mut self, module: &psrs_hir::Module) {
        let mut expressions = module
            .declarations
            .iter()
            .map(|declaration| declaration.value.clone())
            .collect::<Vec<_>>();
        expressions.extend(
            module
                .instances
                .iter()
                .flat_map(|instance| instance.members.iter().map(|member| member.value.clone())),
        );
        for expression in &expressions {
            self.check_expression_annotations(expression);
        }
    }

    fn check_expression_annotations(&mut self, expression: &psrs_hir::Expr) {
        use psrs_hir::ExprKind;
        match &expression.kind {
            ExprKind::Typed { expression, ty } => {
                self.check_annotation(ty);
                self.check_expression_annotations(expression);
            }
            ExprKind::Array(elements) => {
                for element in elements {
                    self.check_expression_annotations(element);
                }
            }
            ExprKind::Record(fields) => {
                for (_, value) in fields {
                    self.check_expression_annotations(value);
                }
            }
            ExprKind::RecordUpdate { expression, fields } => {
                self.check_expression_annotations(expression);
                for (_, value) in fields {
                    self.check_expression_annotations(value);
                }
            }
            ExprKind::FieldAccess { expression, .. } => {
                self.check_expression_annotations(expression)
            }
            ExprKind::Application(function, argument) => {
                self.check_expression_annotations(function);
                self.check_expression_annotations(argument);
            }
            ExprKind::Operator { left, right, .. } => {
                self.check_expression_annotations(left);
                self.check_expression_annotations(right);
            }
            ExprKind::Negate {
                function,
                expression,
                ..
            } => {
                self.check_expression_annotations(function);
                self.check_expression_annotations(expression);
            }
            ExprKind::OperatorChain { operands, .. } => {
                for operand in operands {
                    self.check_expression_annotations(operand);
                }
            }
            ExprKind::OperatorSection { operand, .. } => self.check_expression_annotations(operand),
            ExprKind::Lambda { body, .. } => self.check_expression_annotations(body),
            ExprKind::Let { bindings, body } => {
                for binding in bindings {
                    self.check_expression_annotations(&binding.value);
                }
                self.check_expression_annotations(body);
            }
            ExprKind::If {
                condition,
                then_branch,
                else_branch,
            } => {
                self.check_expression_annotations(condition);
                self.check_expression_annotations(then_branch);
                self.check_expression_annotations(else_branch);
            }
            ExprKind::Case {
                scrutinee,
                branches,
            } => {
                self.check_expression_annotations(scrutinee);
                for branch in branches {
                    self.check_pattern_annotations(&branch.pattern);
                    self.check_expression_annotations(&branch.value);
                }
            }
            ExprKind::Guarded(clauses) => {
                for clause in clauses {
                    for binding in &clause.where_bindings {
                        self.check_expression_annotations(&binding.value);
                    }
                    for guard in &clause.guards {
                        match guard {
                            psrs_hir::Guard::Boolean(value) => {
                                self.check_expression_annotations(value)
                            }
                            psrs_hir::Guard::Pattern { pattern, value } => {
                                self.check_pattern_annotations(pattern);
                                self.check_expression_annotations(value);
                            }
                            psrs_hir::Guard::Let { bindings, .. } => {
                                for binding in bindings {
                                    self.check_expression_annotations(&binding.value);
                                }
                            }
                        }
                    }
                    self.check_expression_annotations(&clause.value);
                }
            }
            ExprKind::Local(_)
            | ExprKind::Global(_)
            | ExprKind::Integer(_)
            | ExprKind::Number(_)
            | ExprKind::String(_)
            | ExprKind::Char(_) => {}
        }
    }

    fn check_pattern_annotations(&mut self, pattern: &psrs_hir::Pattern) {
        match &pattern.kind {
            psrs_hir::PatternKind::Typed { pattern, ty } => {
                self.check_annotation(ty);
                self.check_pattern_annotations(pattern);
            }
            psrs_hir::PatternKind::Array(elements) => {
                for element in elements {
                    self.check_pattern_annotations(element);
                }
            }
            psrs_hir::PatternKind::Named { pattern, .. } => {
                self.check_pattern_annotations(pattern);
            }
            psrs_hir::PatternKind::Constructor { arguments, .. }
            | psrs_hir::PatternKind::OperatorChain {
                operands: arguments,
                ..
            } => {
                for argument in arguments {
                    self.check_pattern_annotations(argument);
                }
            }
            psrs_hir::PatternKind::Record { fields, .. } => {
                for (_, field) in fields {
                    self.check_pattern_annotations(field);
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

    fn check_annotation(&mut self, ty: &psrs_hir::Type) {
        let kind = self.kind_of_type(ty, &mut std::collections::HashMap::new());
        self.unify(kind, crate::kind::type_kind(), ty.span);
    }
}

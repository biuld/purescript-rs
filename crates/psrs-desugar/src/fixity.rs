use crate::DesugarError;
use psrs_hir::{
    Associativity, Expr, ExprKind, Pattern, PatternKind, ResolvedOperator, ResolvedTypeHead,
    ResolvedTypeOperator, Type, TypeKind,
};
use psrs_span::TextRange;

pub(super) fn validate_module(module: &psrs_hir::Module) -> Vec<DesugarError> {
    let mut errors = Vec::new();
    for declaration in &module.declarations {
        validate_expr(&declaration.value, &mut errors);
        if let Some(signature) = &declaration.signature {
            validate_type(signature, &mut errors);
        }
    }
    for external in &module.externals {
        if let Some(signature) = &external.signature {
            validate_type(signature, &mut errors);
        }
    }
    for declaration in &module.types {
        for parameter in &declaration.parameters {
            if let Some(kind) = &parameter.kind {
                validate_type(kind, &mut errors);
            }
        }
        for constructor in &declaration.constructors {
            for field in &constructor.fields {
                validate_type(field, &mut errors);
            }
        }
        for superclass in &declaration.superclasses {
            validate_type(superclass, &mut errors);
        }
        for member in &declaration.members {
            if let Some(signature) = &member.signature {
                validate_type(signature, &mut errors);
            }
        }
        if let Some(body) = &declaration.body {
            validate_type(body, &mut errors);
        }
        if let Some(kind) = &declaration.declared_kind {
            validate_type(kind, &mut errors);
        }
    }
    for instance in &module.instances {
        for constraint in &instance.context {
            validate_type(constraint, &mut errors);
        }
        validate_type(&instance.head, &mut errors);
        for member in &instance.members {
            validate_expr(&member.value, &mut errors);
        }
    }
    errors
}

fn validate_expr(expression: &Expr, errors: &mut Vec<DesugarError>) {
    match &expression.kind {
        ExprKind::Array(elements) => {
            for element in elements {
                validate_expr(element, errors);
            }
        }
        ExprKind::Record(fields) => {
            for (_, value) in fields {
                validate_expr(value, errors);
            }
        }
        ExprKind::RecordUpdate { expression, fields } => {
            validate_expr(expression, errors);
            for (_, value) in fields {
                validate_expr(value, errors);
            }
        }
        ExprKind::FieldAccess { expression, .. } => validate_expr(expression, errors),
        ExprKind::Application(function, argument) => {
            validate_expr(function, errors);
            validate_expr(argument, errors);
        }
        ExprKind::Typed { expression, ty } | ExprKind::TypeApplication { expression, ty } => {
            validate_expr(expression, errors);
            validate_type(ty, errors);
        }
        ExprKind::Operator { left, right, .. } => {
            validate_expr(left, errors);
            validate_expr(right, errors);
        }
        ExprKind::OperatorChain {
            operands,
            operators,
        } => {
            errors.extend(validate_fixity_group(operators.iter().map(|operator| {
                (
                    operator.associativity,
                    operator.precedence,
                    operator.operator_span,
                )
            })));
            for operand in operands {
                validate_expr(operand, errors);
            }
        }
        ExprKind::OperatorSection { operand, .. } => validate_expr(operand, errors),
        ExprKind::Negate {
            function,
            expression,
            ..
        } => {
            validate_expr(function, errors);
            validate_expr(expression, errors);
        }
        ExprKind::Lambda { body, .. } => validate_expr(body, errors),
        ExprKind::Let { bindings, body } => {
            for binding in bindings {
                validate_expr(&binding.value, errors);
            }
            validate_expr(body, errors);
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            validate_expr(condition, errors);
            validate_expr(then_branch, errors);
            validate_expr(else_branch, errors);
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            validate_expr(scrutinee, errors);
            for branch in branches {
                validate_pattern(&branch.pattern, errors);
                validate_expr(&branch.value, errors);
            }
        }
        ExprKind::Guarded(clauses) => {
            for clause in clauses {
                for binding in &clause.where_bindings {
                    validate_expr(&binding.value, errors);
                }
                for guard in &clause.guards {
                    match guard {
                        psrs_hir::Guard::Boolean(value) => validate_expr(value, errors),
                        psrs_hir::Guard::Pattern { pattern, value } => {
                            validate_pattern(pattern, errors);
                            validate_expr(value, errors);
                        }
                        psrs_hir::Guard::Let { bindings, .. } => {
                            for binding in bindings {
                                validate_expr(&binding.value, errors);
                            }
                        }
                    }
                }
                validate_expr(&clause.value, errors);
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

fn validate_pattern(pattern: &Pattern, errors: &mut Vec<DesugarError>) {
    match &pattern.kind {
        PatternKind::Constructor { arguments, .. } | PatternKind::Array(arguments) => {
            for argument in arguments {
                validate_pattern(argument, errors);
            }
        }
        PatternKind::Named { pattern, .. } => validate_pattern(pattern, errors),
        PatternKind::Typed { pattern, ty } => {
            validate_type(ty, errors);
            validate_pattern(pattern, errors);
        }
        PatternKind::OperatorChain {
            operands,
            operators,
        } => {
            errors.extend(validate_fixity_group(operators.iter().map(|operator| {
                (
                    operator.associativity,
                    operator.precedence,
                    operator.operator_span,
                )
            })));
            for operand in operands {
                validate_pattern(operand, errors);
            }
        }
        PatternKind::Record { fields, .. } => {
            for (_, pattern) in fields {
                validate_pattern(pattern, errors);
            }
        }
        PatternKind::Wildcard
        | PatternKind::Boolean(_)
        | PatternKind::Integer(_)
        | PatternKind::Number(_)
        | PatternKind::String(_)
        | PatternKind::Char(_)
        | PatternKind::Var(_) => {}
    }
}

fn validate_type(ty: &Type, errors: &mut Vec<DesugarError>) {
    match &ty.kind {
        TypeKind::Application(function, argument) => {
            validate_type(function, errors);
            validate_type(argument, errors);
        }
        TypeKind::OperatorChain {
            operands,
            operators,
        } => {
            errors.extend(validate_fixity_group(operators.iter().map(|operator| {
                (
                    operator.associativity,
                    operator.precedence,
                    operator.operator_span,
                )
            })));
            for operand in operands {
                validate_type(operand, errors);
            }
        }
        TypeKind::Function { parameter, result } => {
            validate_type(parameter, errors);
            validate_type(result, errors);
        }
        TypeKind::Forall { variables, body } => {
            for variable in variables {
                if let Some(kind) = &variable.kind {
                    validate_type(kind, errors);
                }
            }
            validate_type(body, errors);
        }
        TypeKind::Constrained { constraint, body } => {
            validate_type(constraint, errors);
            validate_type(body, errors);
        }
        TypeKind::Row { fields, tail } | TypeKind::Record { fields, tail } => {
            for field in fields {
                validate_type(&field.ty, errors);
            }
            if let Some(tail) = tail {
                validate_type(tail, errors);
            }
        }
        TypeKind::Wildcard
        | TypeKind::Variable(_)
        | TypeKind::Constructor(_)
        | TypeKind::Named(_)
        | TypeKind::Opaque(_)
        | TypeKind::Integer(_)
        | TypeKind::String(_) => {}
    }
}

fn validate_fixity_group(
    operators: impl Iterator<Item = (Associativity, u32, TextRange)>,
) -> Vec<DesugarError> {
    let mut pending = Vec::<(Associativity, u32, TextRange)>::new();
    let mut errors = Vec::new();
    for (associativity, precedence, span) in operators {
        while pending
            .last()
            .is_some_and(|(_, previous_precedence, _)| *previous_precedence > precedence)
        {
            pending.pop();
        }

        if let Some((previous_associativity, previous_precedence, _)) = pending.last()
            && *previous_precedence == precedence
        {
            let conflict =
                *previous_associativity != associativity || associativity == Associativity::None;
            if *previous_associativity != associativity {
                errors.push(DesugarError {
                    span,
                    message: "operators of the same precedence have mixed associativity; use parentheses",
                });
            } else if associativity == Associativity::None {
                errors.push(DesugarError {
                    span,
                    message: "a non-associative operator cannot be chained; use parentheses",
                });
            }

            // Left-associative operators reduce the previous operator before
            // this one is shifted. Right-associative operators stay pending.
            // Invalid pairs recover as left-associative so later diagnostics
            // don't treat one error as several nested conflicts.
            if conflict || associativity != Associativity::Right {
                pending.pop();
            }
        }
        pending.push((associativity, precedence, span));
    }
    errors
}

/// Reassociates a source-order operator chain using the fixities attached by
/// name resolution. Parenthesized subchains are operands and are already
/// resolved independently.
pub(super) fn reassociate(
    operands: Vec<Expr>,
    operators: Vec<ResolvedOperator>,
    span: TextRange,
) -> Expr {
    assert_eq!(operands.len(), operators.len() + 1);
    let mut values = Vec::with_capacity(operands.len());
    let mut pending = Vec::new();
    let mut operands = operands.into_iter();
    values.push(
        operands
            .next()
            .expect("verified operator chain has an operand"),
    );

    for (operator, operand) in operators.into_iter().zip(operands) {
        while pending.last().is_some_and(|previous: &ResolvedOperator| {
            previous.precedence > operator.precedence
                || (previous.precedence == operator.precedence
                    && operator.associativity != Associativity::Right)
        }) {
            reduce(&mut values, &mut pending);
        }
        pending.push(operator);
        values.push(operand);
    }
    while !pending.is_empty() {
        reduce(&mut values, &mut pending);
    }
    let mut expression = values.pop().expect("reassociation produces one expression");
    expression.span = span;
    expression
}

fn reduce(values: &mut Vec<Expr>, operators: &mut Vec<ResolvedOperator>) {
    let operator = operators.pop().expect("an operator is pending");
    let right = values.pop().expect("operator has a right operand");
    let left = values.pop().expect("operator has a left operand");
    let span = TextRange::new(left.span.start, right.span.end);
    values.push(Expr {
        kind: ExprKind::Operator {
            operator: operator.symbol,
            operator_span: operator.operator_span,
            left: Box::new(left),
            right: Box::new(right),
        },
        span,
    });
}

pub(super) fn reassociate_pattern(
    operands: Vec<Pattern>,
    operators: Vec<ResolvedOperator>,
    span: TextRange,
) -> Pattern {
    assert_eq!(operands.len(), operators.len() + 1);
    let mut values = Vec::with_capacity(operands.len());
    let mut pending = Vec::new();
    let mut operands = operands.into_iter();
    values.push(
        operands
            .next()
            .expect("verified operator pattern has an operand"),
    );
    for (operator, operand) in operators.into_iter().zip(operands) {
        while pending.last().is_some_and(|previous: &ResolvedOperator| {
            previous.precedence > operator.precedence
                || (previous.precedence == operator.precedence
                    && operator.associativity != Associativity::Right)
        }) {
            reduce_pattern(&mut values, &mut pending);
        }
        pending.push(operator);
        values.push(operand);
    }
    while !pending.is_empty() {
        reduce_pattern(&mut values, &mut pending);
    }
    let mut pattern = values.pop().expect("reassociation produces one pattern");
    pattern.span = span;
    pattern
}

fn reduce_pattern(values: &mut Vec<Pattern>, operators: &mut Vec<ResolvedOperator>) {
    let operator = operators.pop().expect("an operator is pending");
    let right = values.pop().expect("operator has a right pattern");
    let left = values.pop().expect("operator has a left pattern");
    let span = TextRange::new(left.span.start, right.span.end);
    values.push(Pattern {
        kind: PatternKind::Constructor {
            symbol: operator.symbol,
            name_span: operator.operator_span,
            arguments: vec![left, right],
        },
        span,
    });
}

pub(super) fn reassociate_type(
    operands: Vec<Type>,
    operators: Vec<ResolvedTypeOperator>,
    span: TextRange,
) -> Type {
    assert_eq!(operands.len(), operators.len() + 1);
    let mut values = Vec::with_capacity(operands.len());
    let mut pending = Vec::new();
    let mut operands = operands.into_iter();
    values.push(operands.next().expect("verified type chain has an operand"));

    for (operator, operand) in operators.into_iter().zip(operands) {
        while pending
            .last()
            .is_some_and(|previous: &ResolvedTypeOperator| {
                previous.precedence > operator.precedence
                    || (previous.precedence == operator.precedence
                        && operator.associativity != Associativity::Right)
            })
        {
            reduce_type(&mut values, &mut pending);
        }
        pending.push(operator);
        values.push(operand);
    }
    while !pending.is_empty() {
        reduce_type(&mut values, &mut pending);
    }
    let mut ty = values.pop().expect("reassociation produces one type");
    ty.span = span;
    ty
}

fn reduce_type(values: &mut Vec<Type>, operators: &mut Vec<ResolvedTypeOperator>) {
    let operator = operators.pop().expect("an operator is pending");
    let right = values.pop().expect("operator has a right type");
    let left = values.pop().expect("operator has a left type");
    let span = TextRange::new(left.span.start, right.span.end);
    let kind = match operator.head {
        ResolvedTypeHead::Builtin(builtin) => TypeKind::Constructor(builtin),
        ResolvedTypeHead::Named(id) => TypeKind::Named(id),
        ResolvedTypeHead::Opaque(id) => TypeKind::Opaque(id),
    };
    let constructor = Type {
        kind,
        span: operator.operator_span,
    };
    let applied = Type {
        kind: TypeKind::Application(Box::new(constructor), Box::new(left)),
        span,
    };
    values.push(Type {
        kind: TypeKind::Application(Box::new(applied), Box::new(right)),
        span,
    });
}

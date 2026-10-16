use crate::{Binder, CaseBranch, Declaration, Expr, ExprKind, Guard, LowerError, Name, Pattern};
use psrs_cst as cst;
use psrs_span::TextRange;

/// Combines adjacent equations for one value into one ordered case matrix.
pub(super) fn lower_value_declarations(
    declarations: Vec<cst::ValueDeclaration>,
    duplicate_code: &'static str,
) -> Result<Declaration, LowerError> {
    let first = declarations
        .first()
        .expect("a value-equation group is nonempty")
        .clone();
    if declarations.len() == 1 && matches!(&first.rhs, cst::ValueRhs::Plain { .. }) {
        return lower_plain_declaration(first.clone());
    }

    let arity = first.parameters.len();
    for declaration in &declarations {
        if let Some(error) = super::check_argument_names(&declaration.parameters) {
            return Err(error);
        }
        if declaration.parameters.len() != arity {
            return Err(LowerError::coded(
                declaration.span,
                "ArgListLengthsDiffer",
                "all equations for a value must have the same number of arguments",
            ));
        }
    }
    if arity == 0 && declarations.len() > 1 && matches!(&first.rhs, cst::ValueRhs::Plain { .. }) {
        return Err(LowerError::coded(
            first.span,
            duplicate_code,
            "a value without arguments may have only one unguarded declaration",
        ));
    }

    let group_span = TextRange::new(first.span.start, declarations.last().unwrap().span.end);
    let arguments = (0..arity)
        .map(|index| Binder {
            // `$` cannot begin a source identifier, so this binder is hygienic.
            name: format!("$psrs_equation_arg_{}_{}", first.span.start, index),
            span: first.span,
        })
        .collect::<Vec<_>>();
    let scrutinee = argument_record(&arguments, group_span);
    let mut branches = Vec::with_capacity(declarations.len());
    for declaration in declarations {
        let (pattern, guards) = equation_pattern(declaration.parameters, declaration.span)?;
        let value = lower_rhs(declaration.rhs, declaration.where_block, declaration.span)?;
        let value = super::expr::prepend_guards(value, guards, declaration.span);
        branches.push(CaseBranch {
            pattern,
            value,
            span: declaration.span,
        });
    }
    let mut value = Expr {
        kind: ExprKind::Case {
            scrutinee: Box::new(scrutinee),
            branches,
        },
        span: group_span,
    };
    for binder in arguments.into_iter().rev() {
        let lambda_span = TextRange::new(binder.span.start, value.span.end);
        value = Expr {
            kind: ExprKind::Lambda {
                binder,
                body: Box::new(value),
            },
            span: lambda_span,
        };
    }
    Ok(Declaration {
        name: super::lower_name(first.name),
        value,
        span: group_span,
        annotation: first
            .annotation
            .clone()
            .map(super::lower_type)
            .transpose()?,
    })
}

fn lower_plain_declaration(declaration: cst::ValueDeclaration) -> Result<Declaration, LowerError> {
    if let Some(error) = super::check_argument_names(&declaration.parameters) {
        return Err(error);
    }
    let value = lower_rhs(declaration.rhs, declaration.where_block, declaration.span)?;
    let mut value = value;
    for parameter in declaration.parameters.into_iter().rev() {
        value = super::expr::lower_pattern_lambda(parameter, value)?;
    }
    Ok(Declaration {
        name: super::lower_name(declaration.name),
        value,
        span: declaration.span,
        annotation: declaration.annotation.map(super::lower_type).transpose()?,
    })
}

fn lower_rhs(
    rhs: cst::ValueRhs,
    where_block: Option<cst::DeclarationBlock>,
    span: TextRange,
) -> Result<Expr, LowerError> {
    let value = match rhs {
        cst::ValueRhs::Plain { value, .. } => super::lower_expr(value)?,
        cst::ValueRhs::Guarded(clauses) => Expr {
            kind: ExprKind::Guarded(super::expr::lower_guarded_rhs(clauses)?),
            span,
        },
    };
    super::wrap_where(where_block, value)
}

fn argument_record(arguments: &[Binder], span: TextRange) -> Expr {
    let fields = arguments
        .iter()
        .enumerate()
        .map(|(index, binder)| {
            (
                super::tuple_label(index),
                Expr {
                    kind: ExprKind::Name(Name {
                        text: binder.name.clone(),
                        span: binder.span,
                    }),
                    span: binder.span,
                },
            )
        })
        .collect::<Vec<_>>();
    Expr {
        kind: ExprKind::MatchProduct(fields),
        span,
    }
}

fn equation_pattern(
    parameters: Vec<cst::Pattern>,
    span: TextRange,
) -> Result<(Pattern, Vec<Guard>), LowerError> {
    if parameters.is_empty() {
        return Ok((
            Pattern {
                kind: crate::PatternKind::Record {
                    fields: Vec::new(),
                    mode: crate::RecordPatternMode::Exact,
                },
                span,
            },
            Vec::new(),
        ));
    }
    let fields = parameters
        .into_iter()
        .enumerate()
        .map(|(index, parameter)| {
            let pattern = super::expr::lower_pattern(parameter)?;
            Ok((super::tuple_label(index), pattern))
        })
        .collect::<Result<Vec<_>, LowerError>>()?;
    Ok((
        Pattern {
            kind: crate::PatternKind::Record {
                fields,
                mode: crate::RecordPatternMode::Exact,
            },
            span,
        },
        Vec::new(),
    ))
}

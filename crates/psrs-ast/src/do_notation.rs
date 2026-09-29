use super::{Binder, Expr, ExprKind, LowerError, Name};
use psrs_cst as cst;
use psrs_span::TextRange;

/// Lowers a `do` block by desugaring its statements to `bind`, `discard`, and
/// `let`, matching the official PureScript `Sugar.DoNotation` pass.
///
/// * `let` declarations become a `let` around the desugared rest.
/// * `x <- e` becomes `bind e (\x -> rest)`. A non-variable pattern becomes
///   `bind e (\v -> case v of pattern -> rest)`.
/// * `e` becomes `discard e (\_ -> rest)`, the `Control.Bind.discard` shape.
/// * A final value statement is the block's result; a final `bind` or `let`
///   is rejected.
///
/// The names `bind` and `discard` stay unresolved here and resolve from the
/// enclosing scope, exactly like any other source name.
pub(super) fn lower_do(
    statements: Vec<cst::DoStatement>,
    do_keyword_span: TextRange,
    span: TextRange,
) -> Result<Expr, LowerError> {
    let mut expression = desugar_do(&statements, do_keyword_span, span)?;
    expression.span = span;
    Ok(expression)
}

fn desugar_do(
    statements: &[cst::DoStatement],
    do_keyword_span: TextRange,
    span: TextRange,
) -> Result<Expr, LowerError> {
    let Some((statement, rest)) = statements.split_first() else {
        return Err(LowerError::new(span, "an empty `do` block is not allowed"));
    };
    match statement {
        cst::DoStatement::Let { declarations, .. } => {
            if rest.is_empty() {
                return Err(LowerError::coded(
                    span,
                    "InvalidDoLet",
                    "a `do` block cannot end with a `let` statement",
                ));
            }
            let body = desugar_do(rest, do_keyword_span, span)?;
            let declarations = declarations
                .iter()
                .cloned()
                .map(super::lower_declaration)
                .collect::<Result<Vec<_>, _>>()?;
            Ok(Expr {
                kind: ExprKind::Let {
                    declarations,
                    body: Box::new(body),
                },
                span,
            })
        }
        cst::DoStatement::Discard(value) => {
            if rest.is_empty() {
                return super::lower_expr(value.clone());
            }
            let value = super::lower_expr(value.clone())?;
            let value_span = value.span;
            let body = desugar_do(rest, do_keyword_span, span)?;
            let continuation = wildcard_lambda(value_span, body);
            Ok(application(
                application(name("discard", do_keyword_span), value, span),
                continuation,
                span,
            ))
        }
        cst::DoStatement::Bind { pattern, value, .. } => {
            if rest.is_empty() {
                return Err(LowerError::coded(
                    span,
                    "InvalidDoBind",
                    "a `do` block cannot end with a `<-` statement",
                ));
            }
            let value = super::lower_expr(value.clone())?;
            let body = desugar_do(rest, do_keyword_span, span)?;
            let continuation = super::expr::lower_pattern_lambda(pattern.clone(), body)?;
            Ok(application(
                application(name("bind", do_keyword_span), value, span),
                continuation,
                span,
            ))
        }
    }
}

/// Lowers an `ado` block using the official PureScript `Sugar.AdoNotation`
/// desugaring: each statement contributes an argument and a lambda around the
/// `in` result, then the collected arguments are combined with `map`, `apply`,
/// and `pure`.
///
/// For `ado x <- a; y <- b; in f x y` this produces
/// `apply (map (\x -> \y -> f x y) a) b`. A `let` statement wraps only the
/// result; a discarded expression uses a wildcard binder. With no argument
/// statements the block is `pure` of its result.
///
/// As with `do`, `map`, `apply`, and `pure` stay unresolved here and resolve
/// from the enclosing scope.
pub(super) fn lower_ado(
    statements: Vec<cst::DoStatement>,
    result: cst::Expr,
    span: TextRange,
) -> Result<Expr, LowerError> {
    let yield_expr = super::lower_expr(result)?;
    let mut function = yield_expr;
    let mut arguments: Vec<Expr> = Vec::new();
    for statement in statements.iter().rev() {
        match statement {
            cst::DoStatement::Let { declarations, .. } => {
                let declarations = declarations
                    .iter()
                    .cloned()
                    .map(super::lower_declaration)
                    .collect::<Result<Vec<_>, _>>()?;
                function = Expr {
                    kind: ExprKind::Let {
                        declarations,
                        body: Box::new(function),
                    },
                    span,
                };
            }
            cst::DoStatement::Discard(value) => {
                let value = super::lower_expr(value.clone())?;
                let value_span = value.span;
                arguments.insert(0, value);
                function = wildcard_lambda(value_span, function);
            }
            cst::DoStatement::Bind { pattern, value, .. } => {
                let value = super::lower_expr(value.clone())?;
                arguments.insert(0, value);
                function = super::expr::lower_pattern_lambda(pattern.clone(), function)?;
            }
        }
    }
    let mut expression = if arguments.is_empty() {
        application(name("pure", span), function, span)
    } else {
        let mut arguments = arguments.into_iter();
        let head = arguments
            .next()
            .expect("a non-empty ado argument list has a head");
        let mut expression =
            application(application(name("map", span), function, span), head, span);
        for argument in arguments {
            expression = application(
                application(name("apply", span), expression, span),
                argument,
                span,
            );
        }
        expression
    };
    expression.span = span;
    Ok(expression)
}

fn name(text: &str, span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Name(Name {
            text: text.to_string(),
            span,
        }),
        span,
    }
}

fn application(function: Expr, argument: Expr, span: TextRange) -> Expr {
    Expr {
        kind: ExprKind::Application(Box::new(function), Box::new(argument)),
        span,
    }
}

fn wildcard_lambda(span: TextRange, body: Expr) -> Expr {
    let binder = Binder {
        name: format!("__psrs_do_wildcard_{}", span.start),
        span,
    };
    super::lower_lambda(binder, body)
}

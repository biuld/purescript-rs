mod analysis;
mod call;

use super::super::util::{FreshLocals, next_locals};
use crate::{Declaration, Expr, ExprKind, Module, Type};
use psrs_hir::SymbolId;
use std::collections::{HashMap, HashSet};

use analysis::{collect_globals, contains_case, is_recursive};
use call::inline_named_global;

pub(super) fn run(mut module: Module, max_body_nodes: usize, sites_left: &mut usize) -> Module {
    let all_declarations = module
        .declarations
        .iter()
        .cloned()
        .map(|declaration| (declaration.symbol, declaration))
        .collect::<HashMap<_, _>>();
    let graph = all_declarations
        .iter()
        .map(|(symbol, declaration)| {
            let mut references = Vec::new();
            collect_globals(&declaration.value, &mut references);
            (*symbol, references)
        })
        .collect::<HashMap<_, _>>();
    let recursive = all_declarations
        .keys()
        .copied()
        .filter(|symbol| is_recursive(*symbol, &graph))
        .collect::<HashSet<_>>();
    let declarations = all_declarations
        .into_iter()
        .filter(|(_, declaration)| !contains_case(&declaration.value))
        .collect::<HashMap<_, _>>();
    let mut fresh = next_locals(&module);
    for (declaration, fresh) in module.declarations.iter_mut().zip(&mut fresh) {
        if *sites_left == 0 {
            break;
        }
        declaration.value = inline_expr(
            declaration.value.clone(),
            fresh,
            sites_left,
            max_body_nodes,
            &declarations,
            &recursive,
            &module.types,
        );
    }
    module
}

#[allow(clippy::too_many_arguments)]
fn inline_expr(
    mut expression: Expr,
    fresh: &mut FreshLocals,
    sites_left: &mut usize,
    max_body_nodes: usize,
    declarations: &HashMap<SymbolId, Declaration>,
    recursive: &HashSet<SymbolId>,
    types: &[Type],
) -> Expr {
    expression.kind = match expression.kind {
        ExprKind::Constructor { symbol, arguments } => ExprKind::Constructor {
            symbol,
            arguments: arguments
                .into_iter()
                .map(|argument| {
                    inline_expr(
                        argument,
                        fresh,
                        sites_left,
                        max_body_nodes,
                        declarations,
                        recursive,
                        types,
                    )
                })
                .collect(),
        },
        ExprKind::Array { elements } => ExprKind::Array {
            elements: elements
                .into_iter()
                .map(|element| {
                    inline_expr(
                        element,
                        fresh,
                        sites_left,
                        max_body_nodes,
                        declarations,
                        recursive,
                        types,
                    )
                })
                .collect(),
        },
        ExprKind::Record { fields } => ExprKind::Record {
            fields: fields
                .into_iter()
                .map(|(label, value)| {
                    (
                        label,
                        inline_expr(
                            value,
                            fresh,
                            sites_left,
                            max_body_nodes,
                            declarations,
                            recursive,
                            types,
                        ),
                    )
                })
                .collect(),
        },
        ExprKind::RecordUpdate { record, fields } => ExprKind::RecordUpdate {
            record: Box::new(inline_expr(
                *record,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            fields: fields
                .into_iter()
                .map(|(label, value)| {
                    (
                        label,
                        inline_expr(
                            value,
                            fresh,
                            sites_left,
                            max_body_nodes,
                            declarations,
                            recursive,
                            types,
                        ),
                    )
                })
                .collect(),
        },
        ExprKind::FieldAccess { record, field } => ExprKind::FieldAccess {
            record: Box::new(inline_expr(
                *record,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            field,
        },
        ExprKind::ArrayLength(array) => ExprKind::ArrayLength(Box::new(inline_expr(
            *array,
            fresh,
            sites_left,
            max_body_nodes,
            declarations,
            recursive,
            types,
        ))),
        ExprKind::UnaryPrimitive { op, value } => ExprKind::UnaryPrimitive {
            op,
            value: Box::new(inline_expr(
                *value,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
        },
        ExprKind::ArrayIndex { array, index } => ExprKind::ArrayIndex {
            array: Box::new(inline_expr(
                *array,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            index: Box::new(inline_expr(
                *index,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
        },
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => ExprKind::ArrayUpdate {
            array: Box::new(inline_expr(
                *array,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            index: Box::new(inline_expr(
                *index,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            value: Box::new(inline_expr(
                *value,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
        },
        ExprKind::Primitive { op, left, right } => ExprKind::Primitive {
            op,
            left: Box::new(inline_expr(
                *left,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            right: Box::new(inline_expr(
                *right,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
        },
        ExprKind::Application(function, argument) => {
            let function = inline_expr(
                *function,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            );
            let argument = inline_expr(
                *argument,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            );
            let application = Expr {
                kind: ExprKind::Application(Box::new(function), Box::new(argument)),
                ty: expression.ty,
                span: expression.span,
            };
            if *sites_left > 0
                && let Some(inlined) = inline_named_global(
                    &application,
                    fresh,
                    max_body_nodes,
                    declarations,
                    recursive,
                    types,
                )
            {
                *sites_left -= 1;
                inlined.kind
            } else {
                application.kind
            }
        }
        ExprKind::Lambda { binder, body } => ExprKind::Lambda {
            binder,
            body: Box::new(inline_expr(
                *body,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
        },
        ExprKind::Let { bindings, body } => ExprKind::Let {
            bindings: bindings
                .into_iter()
                .map(|mut binding| {
                    binding.value = inline_expr(
                        binding.value,
                        fresh,
                        sites_left,
                        max_body_nodes,
                        declarations,
                        recursive,
                        types,
                    );
                    binding
                })
                .collect(),
            body: Box::new(inline_expr(
                *body,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
        },
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => ExprKind::If {
            condition: Box::new(inline_expr(
                *condition,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            then_branch: Box::new(inline_expr(
                *then_branch,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            else_branch: Box::new(inline_expr(
                *else_branch,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
        },
        ExprKind::Case {
            scrutinee,
            branches,
        } => ExprKind::Case {
            scrutinee: Box::new(inline_expr(
                *scrutinee,
                fresh,
                sites_left,
                max_body_nodes,
                declarations,
                recursive,
                types,
            )),
            branches: branches
                .into_iter()
                .map(|mut branch| {
                    branch.value = inline_expr(
                        branch.value,
                        fresh,
                        sites_left,
                        max_body_nodes,
                        declarations,
                        recursive,
                        types,
                    );
                    branch
                })
                .collect(),
        },
        ExprKind::Local(id) => ExprKind::Local(id),
        ExprKind::Global(symbol) => ExprKind::Global(symbol),
        ExprKind::Integer(value) => ExprKind::Integer(value),
        ExprKind::Number(value) => ExprKind::Number(value),
        ExprKind::Boolean(value) => ExprKind::Boolean(value),
        ExprKind::String(value) => ExprKind::String(value),
        ExprKind::Char(value) => ExprKind::Char(value),
    };
    expression
}

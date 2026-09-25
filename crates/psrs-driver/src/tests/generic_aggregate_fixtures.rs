pub(super) fn clear_array_literals(expression: &mut psrs_core::Expr) -> bool {
    use psrs_core::ExprKind;
    match &mut expression.kind {
        ExprKind::Array { elements } => {
            let had_elements = !elements.is_empty();
            elements.clear();
            had_elements
        }
        ExprKind::Constructor { arguments, .. } => arguments.iter_mut().any(clear_array_literals),
        ExprKind::Record { fields } => fields
            .iter_mut()
            .any(|(_, value)| clear_array_literals(value)),
        ExprKind::RecordUpdate { record, fields } => {
            clear_array_literals(record)
                || fields
                    .iter_mut()
                    .any(|(_, value)| clear_array_literals(value))
        }
        ExprKind::FieldAccess { record, .. } | ExprKind::ArrayLength(record) => {
            clear_array_literals(record)
        }
        ExprKind::ArrayIndex { array, index } => {
            clear_array_literals(array) || clear_array_literals(index)
        }
        ExprKind::ArrayUpdate {
            array,
            index,
            value,
        } => {
            clear_array_literals(array)
                || clear_array_literals(index)
                || clear_array_literals(value)
        }
        ExprKind::Primitive { left, right, .. } => {
            clear_array_literals(left) || clear_array_literals(right)
        }
        ExprKind::UnaryPrimitive { value, .. } => clear_array_literals(value),
        ExprKind::Application(function, argument) => {
            clear_array_literals(function) || clear_array_literals(argument)
        }
        ExprKind::Lambda { body, .. } => clear_array_literals(body),
        ExprKind::Let { bindings, body } => {
            bindings
                .iter_mut()
                .any(|binding| clear_array_literals(&mut binding.value))
                || clear_array_literals(body)
        }
        ExprKind::If {
            condition,
            then_branch,
            else_branch,
        } => {
            clear_array_literals(condition)
                || clear_array_literals(then_branch)
                || clear_array_literals(else_branch)
        }
        ExprKind::Case {
            scrutinee,
            branches,
        } => {
            clear_array_literals(scrutinee)
                || branches
                    .iter_mut()
                    .any(|branch| clear_array_literals(&mut branch.value))
        }
        ExprKind::Local(_)
        | ExprKind::Global(_)
        | ExprKind::Integer(_)
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::String(_)
        | ExprKind::Char(_) => false,
    }
}

pub(super) fn array_new_default_count(module: &psrs_backend::mir::Module) -> usize {
    module
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.instructions)
        .filter(|instruction| {
            matches!(
                instruction,
                psrs_backend::mir::Instruction::ArrayNewDefault { .. }
            )
        })
        .count()
}

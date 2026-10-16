use super::super::decision::SurfacePattern;
use psrs_core::{Literal, Module};

pub(super) fn render(module: &Module, pattern: &SurfacePattern) -> String {
    match pattern {
        SurfacePattern::Any { .. } | SurfacePattern::Var { .. } => "_".to_owned(),
        SurfacePattern::Literal { value, .. } => render_literal(value),
        SurfacePattern::Array { elements, .. } => format!(
            "[{}]",
            elements
                .iter()
                .map(|element| render(module, element))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SurfacePattern::Constructor {
            symbol, arguments, ..
        } => {
            let name = module
                .constructors
                .iter()
                .find(|constructor| constructor.symbol == *symbol)
                .map_or_else(
                    || format!("Constructor{}", symbol.index),
                    |constructor| constructor.name.clone(),
                );
            if arguments.is_empty() {
                name
            } else {
                let arguments = arguments
                    .iter()
                    .map(|argument| match argument {
                        SurfacePattern::Any { .. } => "_".to_owned(),
                        SurfacePattern::Constructor { arguments, .. } if arguments.is_empty() => {
                            render(module, argument)
                        }
                        nested => format!("({})", render(module, nested)),
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{name} {arguments}")
            }
        }
        SurfacePattern::Record { fields, .. } => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(label, value)| format!("{label}: {}", render(module, value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        SurfacePattern::Named { pattern, .. } => render(module, pattern),
    }
}

fn render_literal(literal: &Literal) -> String {
    match literal {
        Literal::Integer(value) => value.to_string(),
        Literal::Number(value) => value.clone(),
        Literal::String(value) => format!("{value:?}"),
        Literal::Char(value) => format!("{value:?}"),
        Literal::Boolean(value) => value.to_string(),
    }
}

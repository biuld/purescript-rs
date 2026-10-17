//! Witness rendering: one pattern as the missing-case atom the diagnostic
//! layer prints. Constructor names come from the supplying `Types` so the
//! rendering stays qualified per stage's convention without the engine
//! knowing any module table.

use crate::pattern::{Literal, Pattern};
use crate::types::Types;

pub(crate) fn render<Ty, T: Types<Ty>>(types: &T, pattern: &Pattern<Ty>) -> String
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    match pattern {
        Pattern::Any { .. } => "_".to_owned(),
        Pattern::Literal { literal, .. } => render_literal(literal),
        Pattern::Array { elements, .. } => format!(
            "[{}]",
            elements
                .iter()
                .map(|element| render(types, element))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Pattern::Constructor {
            symbol, arguments, ..
        } => {
            let name = types.constructor_name(*symbol);
            if arguments.is_empty() {
                name
            } else {
                let arguments = arguments
                    .iter()
                    .map(|argument| match argument {
                        Pattern::Any { .. } => "_".to_owned(),
                        Pattern::Constructor { arguments, .. } if arguments.is_empty() => {
                            render(types, argument)
                        }
                        nested => format!("({})", render(types, nested)),
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{name} {arguments}")
            }
        }
        Pattern::Record { fields, .. } => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(label, value)| format!("{label}: {}", render(types, value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
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

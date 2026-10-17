//! Shared pattern-matrix usefulness and witness reconstruction.
//!
//! This is the one usefulness engine. Its inputs are the neutral [`Pattern`]
//! algebra and the [`Types`] facts; no stage's internal representation reaches
//! here. The algorithm is unchanged from the decision compiler's original
//! analysis and keeps its documented behaviors: an incomplete constructor
//! signature uses the default matrix path, uninhabited user types stay
//! unknown, and the active-state set bounds recursive product expansion.

mod literal;
pub(crate) use literal::canonical_literal;

use std::collections::HashSet;

use psrs_hir::SymbolId;

use crate::pattern::{Literal, Pattern};
use crate::types::Types;

/// The shared engine parameterized by a stage's type identity `Ty`.
pub(crate) type EngineResult<Ty> = Option<Vec<Pattern<Ty>>>;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Head {
    Constructor(SymbolId),
    Record,
    Literal(Literal),
    ArrayLength(usize),
}

#[derive(Clone, Debug)]
struct Shape<Ty> {
    ty: Ty,
    head: Head,
    fields: Vec<(Option<String>, Ty)>,
}

type Matrix<Ty> = Vec<Vec<Pattern<Ty>>>;
type AnalysisState<Ty> = (Matrix<Ty>, Vec<Pattern<Ty>>);

pub(crate) fn useful<Ty, T: Types<Ty>>(
    types: &T,
    matrix: &Matrix<Ty>,
    query: &[Pattern<Ty>],
) -> EngineResult<Ty>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    useful_with_active(types, matrix, query, &mut HashSet::new())
}

fn useful_with_active<Ty, T: Types<Ty>>(
    types: &T,
    matrix: &Matrix<Ty>,
    query: &[Pattern<Ty>],
    active: &mut HashSet<AnalysisState<Ty>>,
) -> EngineResult<Ty>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    // A query a row of wildcards already proves is never worth expanding: the
    // guard bounds specialization growth before the query matrix can grow it.
    if matrix.iter().any(|row| {
        row.len() == query.len()
            && row
                .iter()
                .all(|pattern| matches!(pattern, Pattern::Any { .. }))
    }) {
        return None;
    }
    let state = (matrix.clone(), query.to_vec());
    if !active.insert(state.clone()) {
        return None;
    }
    let result = useful_inner(types, matrix, query, active);
    active.remove(&state);
    result
}

fn useful_inner<Ty, T: Types<Ty>>(
    types: &T,
    matrix: &Matrix<Ty>,
    query: &[Pattern<Ty>],
    active: &mut HashSet<AnalysisState<Ty>>,
) -> EngineResult<Ty>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    let Some(head) = query.first() else {
        return matrix.is_empty().then(Vec::new);
    };
    let tail = &query[1..];
    if let Pattern::Any { ty } = head {
        if let Some(element_type) = types.array_element(*ty) {
            return useful_array_any(types, matrix, query, tail, *ty, element_type, active);
        }
        if let Some(shapes) = signature(types, *ty) {
            // An incomplete signature uses the default matrix. Expanding every
            // constructor here grows unconstrained recursive product columns
            // forever, even when the matrix is empty. Only a complete set of
            // observed heads needs field-by-field specialization.
            let missing = shapes
                .iter()
                .filter(|shape| {
                    !matrix
                        .iter()
                        .filter_map(|row| row.first())
                        .any(|pattern| head_of(pattern).as_ref() == Some(&shape.head))
                })
                .collect::<Vec<_>>();
            if !missing.is_empty() {
                let witness = useful_with_active(types, &default_matrix(matrix), tail, active)?;
                for shape in missing {
                    if let Some(head) = inhabited_shape(types, shape, &mut Default::default()) {
                        return Some(std::iter::once(head).chain(witness).collect());
                    }
                }
                // Missing uninhabited constructors leave observed fields to
                // check; their partial patterns can still expose a witness.
            }
            for shape in shapes {
                let specialized = specialize(matrix, &shape);
                let mut specialized_query = expanded_any(&shape, matrix, query);
                specialized_query.extend_from_slice(tail);
                if let Some(mut witness) =
                    useful_with_active(types, &specialized, &specialized_query, active)
                {
                    let rest = witness.split_off(shape.fields.len());
                    return Some(
                        std::iter::once(reconstruct(&shape, witness))
                            .chain(rest)
                            .collect(),
                    );
                }
            }
            return None;
        }
        let default = default_matrix(matrix);
        return useful_with_active(types, &default, tail, active)
            .map(|witness| std::iter::once(head.clone()).chain(witness).collect());
    }

    let ty = head.pattern_type();
    let shape = match head {
        Pattern::Array { elements, .. } => array_shape(types, ty, elements.len())?,
        Pattern::Literal { literal, .. } => Shape {
            ty,
            head: Head::Literal(canonical_literal(literal)),
            fields: Vec::new(),
        },
        _ => {
            let selected = head_of(head)?;
            signature(types, ty)?
                .into_iter()
                .find(|shape| shape.head == selected)?
        }
    };
    let specialized = specialize(matrix, &shape);
    let mut specialized_query = expand(head, &shape, matrix, query);
    specialized_query.extend_from_slice(tail);
    useful_with_active(types, &specialized, &specialized_query, active).map(|mut witness| {
        let rest = witness.split_off(shape.fields.len());
        std::iter::once(reconstruct(&shape, witness))
            .chain(rest)
            .collect()
    })
}

/// Constructs a finite inhabitant independently of the pattern matrix. The
/// active path is keyed by type, so recursive products cannot grow a sequence
/// of distinct query states while looking for a base constructor.
fn inhabited_type<Ty, T: Types<Ty>>(
    types: &T,
    ty: Ty,
    active: &mut HashSet<Ty>,
) -> Option<Pattern<Ty>>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    let ty = types.shape(ty);
    if !active.insert(ty) {
        return None;
    }
    let result = if types.array_element(ty).is_some() {
        Some(Pattern::Array {
            elements: Vec::new(),
            ty,
        })
    } else if let Some(shapes) = signature(types, ty) {
        shapes
            .iter()
            .find_map(|shape| inhabited_shape(types, shape, active))
    } else {
        Some(Pattern::Any { ty })
    };
    active.remove(&ty);
    result
}

fn inhabited_shape<Ty, T: Types<Ty>>(
    types: &T,
    shape: &Shape<Ty>,
    active: &mut HashSet<Ty>,
) -> Option<Pattern<Ty>>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    let fields = shape
        .fields
        .iter()
        .map(|(_, ty)| inhabited_type(types, *ty, active))
        .collect::<Option<Vec<_>>>()?;
    Some(reconstruct(shape, fields))
}

fn useful_array_any<Ty, T: Types<Ty>>(
    types: &T,
    matrix: &Matrix<Ty>,
    query: &[Pattern<Ty>],
    tail: &[Pattern<Ty>],
    ty: Ty,
    element_type: Ty,
    active: &mut HashSet<AnalysisState<Ty>>,
) -> EngineResult<Ty>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    let lengths = matrix
        .iter()
        .filter_map(|row| row.first())
        .filter_map(|pattern| match pattern {
            Pattern::Array { elements, .. } => Some(elements.len()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut other_length = 0;
    while lengths.contains(&other_length) {
        other_length += 1;
    }
    let default = default_matrix(matrix);
    if let Some(witness) = useful_with_active(types, &default, tail, active) {
        let array = Pattern::Array {
            elements: (0..other_length)
                .map(|_| Pattern::Any { ty: element_type })
                .collect(),
            ty,
        };
        return Some(std::iter::once(array).chain(witness).collect());
    }
    for length in lengths {
        let shape = Shape {
            ty,
            head: Head::ArrayLength(length),
            fields: vec![(None, element_type); length],
        };
        let specialized = specialize(matrix, &shape);
        let mut specialized_query = expanded_any(&shape, matrix, query);
        specialized_query.extend_from_slice(tail);
        if let Some(mut witness) =
            useful_with_active(types, &specialized, &specialized_query, active)
        {
            let rest = witness.split_off(length);
            return Some(
                std::iter::once(reconstruct(&shape, witness))
                    .chain(rest)
                    .collect(),
            );
        }
    }
    None
}

fn signature<Ty, T: Types<Ty>>(types: &T, ty: Ty) -> Option<Vec<Shape<Ty>>>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    let ty = types.shape(ty);
    if let Some(rows) = types.constructors_of(ty) {
        return Some(
            rows.into_iter()
                .map(|row| Shape {
                    ty,
                    head: Head::Constructor(row.symbol),
                    fields: row.fields.iter().copied().map(|ty| (None, ty)).collect(),
                })
                .collect(),
        );
    }
    if let Some(fields) = types.record_fields(ty) {
        return Some(vec![Shape {
            ty,
            head: Head::Record,
            fields: fields
                .iter()
                .map(|(label, ty)| (Some(label.clone()), *ty))
                .collect(),
        }]);
    }
    if types.boolean_type(ty) {
        return Some(
            [false, true]
                .into_iter()
                .map(|value| Shape {
                    ty,
                    head: Head::Literal(Literal::Boolean(value)),
                    fields: Vec::new(),
                })
                .collect(),
        );
    }
    None
}

fn array_shape<Ty, T: Types<Ty>>(types: &T, ty: Ty, length: usize) -> Option<Shape<Ty>>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    let element_type = types.array_element(ty)?;
    Some(Shape {
        ty,
        head: Head::ArrayLength(length),
        fields: vec![(None, element_type); length],
    })
}

fn head_of<Ty>(pattern: &Pattern<Ty>) -> Option<Head> {
    match pattern {
        Pattern::Constructor { symbol, .. } => Some(Head::Constructor(*symbol)),
        Pattern::Record { .. } => Some(Head::Record),
        Pattern::Literal { literal, .. } => Some(Head::Literal(canonical_literal(literal))),
        Pattern::Array { elements, .. } => Some(Head::ArrayLength(elements.len())),
        Pattern::Any { .. } => None,
    }
}

fn expand<Ty>(
    pattern: &Pattern<Ty>,
    shape: &Shape<Ty>,
    matrix: &Matrix<Ty>,
    query: &[Pattern<Ty>],
) -> Vec<Pattern<Ty>>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    match pattern {
        Pattern::Constructor {
            symbol, arguments, ..
        } if shape.head == Head::Constructor(*symbol) => arguments.clone(),
        Pattern::Record { fields, .. } if shape.head == Head::Record => shape
            .fields
            .iter()
            .map(|(label, ty)| {
                let label = label.as_deref().expect("record shapes have field labels");
                fields
                    .iter()
                    .find(|(field, _)| field == label)
                    .map_or_else(|| Pattern::Any { ty: *ty }, |(_, pat)| pat.clone())
            })
            .collect(),
        Pattern::Literal { literal, .. }
            if shape.head == Head::Literal(canonical_literal(literal)) =>
        {
            Vec::new()
        }
        Pattern::Array { elements, .. } if shape.head == Head::ArrayLength(elements.len()) => {
            elements.clone()
        }
        Pattern::Any { .. } => expanded_any(shape, matrix, query),
        _ => Vec::new(),
    }
}

fn expanded_any<Ty>(
    shape: &Shape<Ty>,
    matrix: &Matrix<Ty>,
    query: &[Pattern<Ty>],
) -> Vec<Pattern<Ty>>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    let exemplar = query
        .iter()
        .chain(matrix.iter().filter_map(|row| row.first()))
        .find(|pattern| head_of(pattern) == Some(shape.head.clone()));
    shape
        .fields
        .iter()
        .enumerate()
        .map(|(index, (label, fallback_ty))| {
            let ty = exemplar.and_then(|pattern| match pattern {
                Pattern::Constructor { arguments, .. } if label.is_none() => {
                    arguments.get(index).map(Pattern::pattern_type)
                }
                Pattern::Array { elements, .. } if label.is_none() => {
                    elements.get(index).map(Pattern::pattern_type)
                }
                Pattern::Record { fields, .. } => fields
                    .iter()
                    .find(|(field, _)| Some(field) == label.as_ref())
                    .map(|(_, pattern)| pattern.pattern_type()),
                _ => None,
            });
            Pattern::Any {
                ty: ty.unwrap_or(*fallback_ty),
            }
        })
        .collect()
}

fn specialize<Ty>(matrix: &Matrix<Ty>, shape: &Shape<Ty>) -> Matrix<Ty>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    let mut output = Vec::new();
    let candidates = matrix
        .iter()
        .filter_map(|row| row.first().cloned())
        .collect::<Vec<_>>();
    for row in matrix {
        let Some(head) = row.first() else {
            continue;
        };
        let expanded = if matches!(head, Pattern::Any { .. }) {
            expanded_any(shape, matrix, &candidates)
        } else if head_of(head) == Some(shape.head.clone()) {
            expand(head, shape, matrix, &candidates)
        } else {
            continue;
        };
        output.push(
            expanded
                .into_iter()
                .chain(row[1..].iter().cloned())
                .collect(),
        );
    }
    output
}

fn default_matrix<Ty>(matrix: &Matrix<Ty>) -> Matrix<Ty>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    matrix
        .iter()
        .filter(|row| {
            row.first()
                .is_some_and(|pattern| matches!(pattern, Pattern::Any { .. }))
        })
        .map(|row| row[1..].to_vec())
        .collect()
}

fn reconstruct<Ty>(shape: &Shape<Ty>, arguments: Vec<Pattern<Ty>>) -> Pattern<Ty>
where
    Ty: Clone + Copy + std::fmt::Debug + Eq + std::hash::Hash,
{
    match &shape.head {
        Head::Constructor(symbol) => Pattern::Constructor {
            symbol: *symbol,
            arguments,
            ty: shape.ty,
        },
        Head::Record => Pattern::Record {
            fields: shape
                .fields
                .iter()
                .zip(arguments)
                .map(|((label, _), pattern)| {
                    (label.clone().expect("record shapes have labels"), pattern)
                })
                .collect(),
            ty: shape.ty,
        },
        Head::Literal(value) => Pattern::Literal {
            literal: value.clone(),
            ty: shape.ty,
        },
        Head::ArrayLength(_) => Pattern::Array {
            elements: arguments,
            ty: shape.ty,
        },
    }
}

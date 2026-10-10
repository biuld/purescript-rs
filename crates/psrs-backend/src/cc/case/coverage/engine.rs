//! Shared pattern-matrix usefulness and witness reconstruction.

use super::super::decision::{SurfacePattern, canonical_literal};
use psrs_core::{Literal, Module, Type, TypeConstructor, TypeId};
use psrs_hir::{SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Head {
    Constructor(SymbolId),
    Record,
    Literal(Literal),
    ArrayLength(usize),
}

#[derive(Clone, Debug)]
struct Shape {
    ty: TypeId,
    head: Head,
    fields: Vec<(Option<String>, TypeId)>,
}

type Matrix = Vec<Vec<SurfacePattern>>;
type AnalysisState = (Matrix, Vec<SurfacePattern>);

pub(super) fn useful(
    module: &Module,
    matrix: &Matrix,
    query: &[SurfacePattern],
) -> Option<Vec<SurfacePattern>> {
    useful_with_active(module, matrix, query, &mut std::collections::HashSet::new())
}

fn useful_with_active(
    module: &Module,
    matrix: &Matrix,
    query: &[SurfacePattern],
    active: &mut std::collections::HashSet<AnalysisState>,
) -> Option<Vec<SurfacePattern>> {
    if matrix.iter().any(|row| {
        row.len() == query.len()
            && row
                .iter()
                .all(|pattern| matches!(pattern, SurfacePattern::Any { .. }))
    }) {
        return None;
    }
    let state = (matrix.clone(), query.to_vec());
    if !active.insert(state.clone()) {
        return None;
    }
    let result = useful_inner(module, matrix, query, active);
    active.remove(&state);
    result
}

fn useful_inner(
    module: &Module,
    matrix: &Matrix,
    query: &[SurfacePattern],
    active: &mut std::collections::HashSet<AnalysisState>,
) -> Option<Vec<SurfacePattern>> {
    let Some(head) = query.first() else {
        return matrix.is_empty().then(Vec::new);
    };
    let tail = &query[1..];
    if let SurfacePattern::Any { ty } = head {
        if let Some(element_type) = array_element(module, *ty) {
            return useful_array_any(module, matrix, query, tail, *ty, element_type, active);
        }
        if let Some(shapes) = signature(module, *ty) {
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
                let witness = useful_with_active(module, &default_matrix(matrix), tail, active)?;
                for shape in missing {
                    if let Some(head) = inhabited_shape(module, shape, &mut Default::default()) {
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
                    useful_with_active(module, &specialized, &specialized_query, active)
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
        return useful_with_active(module, &default, tail, active)
            .map(|witness| std::iter::once(head.clone()).chain(witness).collect());
    }

    let ty = pattern_type(head);
    let shape = match head {
        SurfacePattern::Array { elements, .. } => array_shape(module, ty, elements.len())?,
        SurfacePattern::Literal { value, .. } => Shape {
            ty,
            head: Head::Literal(canonical_literal(value)),
            fields: Vec::new(),
        },
        _ => {
            let selected = head_of(head)?;
            signature(module, ty)?
                .into_iter()
                .find(|shape| shape.head == selected)?
        }
    };
    let specialized = specialize(matrix, &shape);
    let mut specialized_query = expand(head, &shape, matrix, query);
    specialized_query.extend_from_slice(tail);
    useful_with_active(module, &specialized, &specialized_query, active).map(|mut witness| {
        let rest = witness.split_off(shape.fields.len());
        std::iter::once(reconstruct(&shape, witness))
            .chain(rest)
            .collect()
    })
}

/// Constructs a finite inhabitant independently of the pattern matrix. The
/// active path is keyed by type, so recursive products cannot grow a sequence
/// of distinct query states while looking for a base constructor.
fn inhabited_type(
    module: &Module,
    ty: TypeId,
    active: &mut std::collections::HashSet<TypeId>,
) -> Option<SurfacePattern> {
    let ty = crate::cc::layout::unquantified_type(module, ty);
    if !active.insert(ty) {
        return None;
    }
    let result = if array_element(module, ty).is_some() {
        Some(SurfacePattern::Array {
            elements: Vec::new(),
            ty,
            span: TextRange::default(),
        })
    } else if let Some(shapes) = signature(module, ty) {
        shapes
            .iter()
            .find_map(|shape| inhabited_shape(module, shape, active))
    } else {
        Some(SurfacePattern::Any { ty })
    };
    active.remove(&ty);
    result
}

fn inhabited_shape(
    module: &Module,
    shape: &Shape,
    active: &mut std::collections::HashSet<TypeId>,
) -> Option<SurfacePattern> {
    let fields = shape
        .fields
        .iter()
        .map(|(_, ty)| inhabited_type(module, *ty, active))
        .collect::<Option<Vec<_>>>()?;
    Some(reconstruct(shape, fields))
}

fn useful_array_any(
    module: &Module,
    matrix: &Matrix,
    query: &[SurfacePattern],
    tail: &[SurfacePattern],
    ty: TypeId,
    element_type: TypeId,
    active: &mut std::collections::HashSet<AnalysisState>,
) -> Option<Vec<SurfacePattern>> {
    let lengths = matrix
        .iter()
        .filter_map(|row| row.first())
        .filter_map(|pattern| match pattern {
            SurfacePattern::Array { elements, .. } => Some(elements.len()),
            _ => None,
        })
        .collect::<std::collections::BTreeSet<_>>();
    let mut other_length = 0;
    while lengths.contains(&other_length) {
        other_length += 1;
    }
    let default = default_matrix(matrix);
    if let Some(witness) = useful_with_active(module, &default, tail, active) {
        let array = SurfacePattern::Array {
            elements: (0..other_length)
                .map(|_| SurfacePattern::Any { ty: element_type })
                .collect(),
            ty,
            span: TextRange::new(0, 0),
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
            useful_with_active(module, &specialized, &specialized_query, active)
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

fn signature(module: &Module, ty: TypeId) -> Option<Vec<Shape>> {
    let ty = crate::cc::layout::unquantified_type(module, ty);
    if let Some(type_id) = user_type_id(module, ty) {
        return Some(
            module
                .constructors
                .iter()
                .filter(|constructor| constructor.type_id == type_id)
                .map(|constructor| Shape {
                    ty,
                    head: Head::Constructor(constructor.symbol),
                    fields: constructor
                        .field_types
                        .iter()
                        .copied()
                        .map(|ty| (None, ty))
                        .collect(),
                })
                .collect(),
        );
    }
    if let Some(fields) = module.record_fields(ty) {
        return Some(vec![Shape {
            ty,
            head: Head::Record,
            fields: fields
                .iter()
                .map(|(label, ty)| (Some(label.clone()), *ty))
                .collect(),
        }]);
    }
    if matches!(
        module.types.get(ty.0 as usize),
        Some(Type::Constructor(TypeConstructor::Boolean))
    ) {
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

fn array_shape(module: &Module, ty: TypeId, length: usize) -> Option<Shape> {
    let element_type = array_element(module, ty)?;
    Some(Shape {
        ty,
        head: Head::ArrayLength(length),
        fields: vec![(None, element_type); length],
    })
}

fn array_element(module: &Module, ty: TypeId) -> Option<TypeId> {
    let ty = crate::cc::layout::unquantified_type(module, ty);
    let Type::Application(function, element) = module.types.get(ty.0 as usize)? else {
        return None;
    };
    matches!(
        module.types.get(function.0 as usize),
        Some(Type::Constructor(TypeConstructor::Array))
    )
    .then_some(*element)
}

fn user_type_id(module: &Module, mut ty: TypeId) -> Option<HirTypeId> {
    ty = crate::cc::layout::unquantified_type(module, ty);
    loop {
        match module.types.get(ty.0 as usize)? {
            Type::Constructor(TypeConstructor::User(id)) => return Some(*id),
            Type::Application(function, _) => ty = *function,
            _ => return None,
        }
    }
}

fn pattern_type(pattern: &SurfacePattern) -> TypeId {
    match pattern {
        SurfacePattern::Any { ty }
        | SurfacePattern::Var { ty, .. }
        | SurfacePattern::Literal { ty, .. }
        | SurfacePattern::Array { ty, .. }
        | SurfacePattern::Named { ty, .. }
        | SurfacePattern::Constructor { ty, .. }
        | SurfacePattern::Record { ty, .. } => *ty,
    }
}

fn head_of(pattern: &SurfacePattern) -> Option<Head> {
    match pattern {
        SurfacePattern::Constructor { symbol, .. } => Some(Head::Constructor(*symbol)),
        SurfacePattern::Record { .. } => Some(Head::Record),
        SurfacePattern::Literal { value, .. } => Some(Head::Literal(canonical_literal(value))),
        SurfacePattern::Array { elements, .. } => Some(Head::ArrayLength(elements.len())),
        SurfacePattern::Named { pattern, .. } => head_of(pattern),
        SurfacePattern::Any { .. } | SurfacePattern::Var { .. } => None,
    }
}

fn expand(
    pattern: &SurfacePattern,
    shape: &Shape,
    matrix: &Matrix,
    query: &[SurfacePattern],
) -> Vec<SurfacePattern> {
    match pattern {
        SurfacePattern::Constructor {
            symbol, arguments, ..
        } if shape.head == Head::Constructor(*symbol) => arguments.clone(),
        SurfacePattern::Record { fields, .. } if shape.head == Head::Record => shape
            .fields
            .iter()
            .map(|(label, ty)| {
                let label = label.as_deref().expect("record shapes have field labels");
                fields
                    .iter()
                    .find(|(field, _)| field == label)
                    .map_or_else(|| SurfacePattern::Any { ty: *ty }, |(_, pat)| pat.clone())
            })
            .collect(),
        SurfacePattern::Literal { value, .. }
            if shape.head == Head::Literal(canonical_literal(value)) =>
        {
            Vec::new()
        }
        SurfacePattern::Array { elements, .. }
            if shape.head == Head::ArrayLength(elements.len()) =>
        {
            elements.clone()
        }
        SurfacePattern::Any { .. } | SurfacePattern::Var { .. } => {
            expanded_any(shape, matrix, query)
        }
        _ => Vec::new(),
    }
}

fn expanded_any(shape: &Shape, matrix: &Matrix, query: &[SurfacePattern]) -> Vec<SurfacePattern> {
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
                SurfacePattern::Constructor { arguments, .. } if label.is_none() => {
                    arguments.get(index).map(pattern_type)
                }
                SurfacePattern::Array { elements, .. } if label.is_none() => {
                    elements.get(index).map(pattern_type)
                }
                SurfacePattern::Record { fields, .. } => fields
                    .iter()
                    .find(|(field, _)| Some(field) == label.as_ref())
                    .map(|(_, pattern)| pattern_type(pattern)),
                _ => None,
            });
            SurfacePattern::Any {
                ty: ty.unwrap_or(*fallback_ty),
            }
        })
        .collect()
}

fn specialize(matrix: &Matrix, shape: &Shape) -> Matrix {
    let mut output = Vec::new();
    let candidates = matrix
        .iter()
        .filter_map(|row| row.first().cloned())
        .collect::<Vec<_>>();
    for row in matrix {
        let Some(head) = row.first() else {
            continue;
        };
        let expanded = if matches!(
            head,
            SurfacePattern::Any { .. } | SurfacePattern::Var { .. }
        ) {
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

fn default_matrix(matrix: &Matrix) -> Matrix {
    matrix
        .iter()
        .filter(|row| {
            row.first()
                .is_some_and(|pattern| matches!(pattern, SurfacePattern::Any { .. }))
        })
        .map(|row| row[1..].to_vec())
        .collect()
}

fn reconstruct(shape: &Shape, arguments: Vec<SurfacePattern>) -> SurfacePattern {
    let span = TextRange::new(0, 0);
    match &shape.head {
        Head::Constructor(symbol) => SurfacePattern::Constructor {
            symbol: *symbol,
            arguments,
            ty: shape.ty,
            span,
        },
        Head::Record => SurfacePattern::Record {
            fields: shape
                .fields
                .iter()
                .zip(arguments)
                .map(|((label, _), pattern)| {
                    (label.clone().expect("record shapes have labels"), pattern)
                })
                .collect(),
            ty: shape.ty,
            span,
        },
        Head::Literal(value) => SurfacePattern::Literal {
            value: value.clone(),
            ty: shape.ty,
            span,
        },
        Head::ArrayLength(_) => SurfacePattern::Array {
            elements: arguments,
            ty: shape.ty,
            span,
        },
    }
}

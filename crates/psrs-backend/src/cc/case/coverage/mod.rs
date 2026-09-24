//! Pattern-matrix usefulness analysis for closed constructors and records.
//!
//! This analysis is target-neutral: it reads Core types and patterns, and does
//! not depend on CC representations or Wasm layout.

use psrs_core::{CaseBranch, Module, Pattern, PatternKind, Type, TypeConstructor, TypeId};
use psrs_hir::{SymbolId, TypeId as HirTypeId};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct CoverageReport {
    pub(super) exhaustive: bool,
    pub(super) witness: Option<String>,
    pub(super) redundant_branches: Vec<usize>,
}

impl CoverageReport {
    pub(super) fn non_exhaustive_message(&self, fallback: &'static str) -> String {
        match &self.witness {
            Some(witness) => format!("non-exhaustive case; missing pattern {witness}"),
            None => fallback.to_owned(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Pat {
    Any(Option<TypeId>),
    Constructor {
        symbol: SymbolId,
        arguments: Vec<Pat>,
        ty: Option<TypeId>,
    },
    Record {
        fields: Vec<(String, Pat)>,
        ty: Option<TypeId>,
    },
}

impl Pat {
    fn ty(&self) -> Option<TypeId> {
        match self {
            Self::Any(ty) | Self::Constructor { ty, .. } | Self::Record { ty, .. } => *ty,
        }
    }

    fn is_any(&self) -> bool {
        matches!(self, Self::Any(_))
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Head {
    Constructor(SymbolId),
    Record,
}

#[derive(Clone, Debug)]
struct Shape {
    head: Head,
    fields: Vec<(Option<String>, TypeId)>,
}

type Matrix = Vec<Vec<Pat>>;

pub(super) fn analyze(
    module: &Module,
    scrutinee_type: TypeId,
    branches: &[CaseBranch],
) -> CoverageReport {
    let matrix = branches
        .iter()
        .map(|branch| vec![convert(&branch.pattern)])
        .collect::<Matrix>();
    let query = vec![Pat::Any(Some(scrutinee_type))];
    let witness = useful(module, &matrix, &query)
        .map(|mut patterns| patterns.remove(0))
        .map(|pattern| render(module, &pattern));
    let mut prior = Vec::new();
    let mut redundant_branches = Vec::new();
    for (index, branch) in branches.iter().enumerate() {
        let query = vec![convert(&branch.pattern)];
        if useful(module, &prior, &query).is_none() {
            redundant_branches.push(index);
        }
        prior.push(query);
    }
    CoverageReport {
        exhaustive: witness.is_none(),
        witness,
        redundant_branches,
    }
}

fn convert(pattern: &Pattern) -> Pat {
    let ty = Some(pattern.ty);
    match &pattern.kind {
        PatternKind::Wildcard | PatternKind::Var { .. } => Pat::Any(ty),
        PatternKind::Constructor { symbol, arguments } => Pat::Constructor {
            symbol: *symbol,
            arguments: arguments.iter().map(convert).collect(),
            ty,
        },
        PatternKind::Record { fields } => Pat::Record {
            fields: fields
                .iter()
                .map(|(label, pattern)| (label.clone(), convert(pattern)))
                .collect(),
            ty,
        },
    }
}

/// Returns a value described by `query` that does not match any matrix row.
/// `None` means that every value described by the query is already covered.
fn useful(module: &Module, matrix: &Matrix, query: &[Pat]) -> Option<Vec<Pat>> {
    let Some(head) = query.first() else {
        return matrix.is_empty().then(Vec::new);
    };
    let tail = &query[1..];
    if head.is_any() {
        let ty = head
            .ty()
            .or_else(|| matrix.iter().find_map(|row| row.first().and_then(Pat::ty)));
        if let Some(shapes) = ty.and_then(|ty| signature(module, ty)) {
            for shape in shapes {
                let specialized = specialize(matrix, &shape);
                let query_head = expanded_any(&shape, matrix, query);
                let mut specialized_query = query_head;
                specialized_query.extend_from_slice(tail);
                if let Some(mut witness) = useful(module, &specialized, &specialized_query) {
                    let arity = shape.fields.len();
                    let rest = witness.split_off(arity);
                    let constructor_arguments = witness;
                    let reconstructed = reconstruct(&shape, constructor_arguments);
                    return Some(std::iter::once(reconstructed).chain(rest).collect());
                }
            }
            return None;
        }
        let default = default_matrix(matrix);
        return useful(module, &default, tail)
            .map(|witness| std::iter::once(Pat::Any(ty)).chain(witness).collect());
    }

    let ty = head.ty()?;
    let shapes = signature(module, ty)?;
    let selected_head = head_of(head)?;
    let shape = shapes
        .into_iter()
        .find(|shape| shape.head == selected_head)?;
    let specialized = specialize(matrix, &shape);
    let mut specialized_query = expand(head, &shape, matrix, query);
    specialized_query.extend_from_slice(tail);
    useful(module, &specialized, &specialized_query).map(|mut witness| {
        let arity = shape.fields.len();
        let rest = witness.split_off(arity);
        let reconstructed = reconstruct(&shape, witness);
        std::iter::once(reconstructed).chain(rest).collect()
    })
}

fn signature(module: &Module, ty: TypeId) -> Option<Vec<Shape>> {
    if let Some(type_id) = user_type_id(module, ty) {
        return Some(
            module
                .constructors
                .iter()
                .filter(|constructor| constructor.type_id == type_id)
                .map(|constructor| Shape {
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
    match module.types.get(ty.0 as usize)? {
        Type::Record(fields) => Some(vec![Shape {
            head: Head::Record,
            fields: fields
                .iter()
                .map(|(label, ty)| (Some(label.clone()), *ty))
                .collect(),
        }]),
        _ => None,
    }
}

fn user_type_id(module: &Module, mut ty: TypeId) -> Option<HirTypeId> {
    loop {
        match module.types.get(ty.0 as usize)? {
            Type::Constructor(TypeConstructor::User(id)) => return Some(*id),
            Type::Application(function, _) => ty = *function,
            _ => return None,
        }
    }
}

fn head_of(pattern: &Pat) -> Option<Head> {
    match pattern {
        Pat::Constructor { symbol, .. } => Some(Head::Constructor(*symbol)),
        Pat::Record { .. } => Some(Head::Record),
        Pat::Any(_) => None,
    }
}

fn expand(pattern: &Pat, shape: &Shape, matrix: &Matrix, query: &[Pat]) -> Vec<Pat> {
    match pattern {
        Pat::Constructor {
            symbol, arguments, ..
        } if shape.head == Head::Constructor(*symbol) => arguments.clone(),
        Pat::Record { fields, .. } if shape.head == Head::Record => shape
            .fields
            .iter()
            .map(|(label, ty)| {
                let label = label.as_deref().expect("record shapes have field labels");
                fields
                    .iter()
                    .find(|(field, _)| field == label)
                    .map_or_else(|| Pat::Any(Some(*ty)), |(_, value)| value.clone())
            })
            .collect(),
        Pat::Any(_) => expanded_any(shape, matrix, query),
        _ => Vec::new(),
    }
}

fn expanded_any(shape: &Shape, matrix: &Matrix, query: &[Pat]) -> Vec<Pat> {
    let exemplar = query
        .iter()
        .chain(matrix.iter().filter_map(|row| row.first()))
        .find(|pattern| head_of(pattern) == Some(shape.head));
    shape
        .fields
        .iter()
        .enumerate()
        .map(|(index, (label, fallback_ty))| {
            let ty = exemplar.and_then(|pattern| match pattern {
                Pat::Constructor { arguments, .. } if label.is_none() => {
                    arguments.get(index).and_then(Pat::ty)
                }
                Pat::Record { fields, .. } => fields
                    .iter()
                    .find(|(field, _)| Some(field) == label.as_ref())
                    .and_then(|(_, pattern)| pattern.ty()),
                _ => None,
            });
            Pat::Any(Some(ty.unwrap_or(*fallback_ty)))
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
        let expanded = if head.is_any() {
            expanded_any(shape, matrix, &candidates)
        } else if head_of(head) == Some(shape.head) {
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
        .filter(|row| row.first().is_some_and(Pat::is_any))
        .map(|row| row[1..].to_vec())
        .collect()
}

fn reconstruct(shape: &Shape, arguments: Vec<Pat>) -> Pat {
    match shape.head {
        Head::Constructor(symbol) => Pat::Constructor {
            symbol,
            arguments,
            ty: None,
        },
        Head::Record => Pat::Record {
            fields: shape
                .fields
                .iter()
                .zip(arguments)
                .map(|((label, _), pattern)| {
                    (label.clone().expect("record shapes have labels"), pattern)
                })
                .collect(),
            ty: None,
        },
    }
}

fn render(module: &Module, pattern: &Pat) -> String {
    match pattern {
        Pat::Any(_) => "_".to_owned(),
        Pat::Constructor {
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
                        Pat::Any(_) => "_".to_owned(),
                        Pat::Constructor { arguments, .. } if arguments.is_empty() => {
                            render(module, argument)
                        }
                        nested => format!("({})", render(module, nested)),
                    })
                    .collect::<Vec<_>>()
                    .join(" ");
                format!("{name} {arguments}")
            }
        }
        Pat::Record { fields, .. } => format!(
            "{{ {} }}",
            fields
                .iter()
                .map(|(label, value)| format!("{label}: {}", render(module, value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

#[cfg(test)]
mod tests;

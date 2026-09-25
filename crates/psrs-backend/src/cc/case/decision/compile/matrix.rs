use super::super::{Action, Column, ColumnKey, Row, SurfacePattern};
use psrs_core::{Pattern, PatternKind, TypeId};
use psrs_span::TextRange;
use std::collections::HashSet;

pub(super) fn surface_pattern(pattern: &Pattern) -> SurfacePattern {
    match &pattern.kind {
        PatternKind::Wildcard => SurfacePattern::Any { ty: pattern.ty },
        PatternKind::Var { id, .. } => SurfacePattern::Var {
            id: *id,
            ty: pattern.ty,
            span: pattern.span,
        },
        PatternKind::Constructor { symbol, arguments } => SurfacePattern::Constructor {
            symbol: *symbol,
            arguments: arguments.iter().map(surface_pattern).collect(),
            ty: pattern.ty,
            span: pattern.span,
        },
        PatternKind::Record { fields } => SurfacePattern::Record {
            fields: fields
                .iter()
                .map(|(name, pattern)| (name.clone(), surface_pattern(pattern)))
                .collect(),
            ty: pattern.ty,
            span: pattern.span,
        },
    }
}

pub(super) fn pattern_type(pattern: &SurfacePattern) -> TypeId {
    match pattern {
        SurfacePattern::Any { ty }
        | SurfacePattern::Var { ty, .. }
        | SurfacePattern::Constructor { ty, .. }
        | SurfacePattern::Record { ty, .. } => *ty,
    }
}

pub(super) fn choose_column(rows: &[Row], count: usize) -> usize {
    (0..count)
        .min_by_key(|column| {
            let mut constructors = HashSet::new();
            for row in rows {
                if let SurfacePattern::Constructor { symbol, .. } = &row.patterns[*column] {
                    constructors.insert(*symbol);
                }
            }
            (constructors.len(), *column)
        })
        .unwrap_or(0)
}

pub(super) fn needed_fields(rows: &[Row], column: usize, count: usize) -> Vec<usize> {
    (0..count)
        .filter(|field| {
            rows.iter().any(|row| {
                matches!(
                    row.patterns.get(column + field),
                    Some(
                        SurfacePattern::Var { .. }
                            | SurfacePattern::Constructor { .. }
                            | SurfacePattern::Record { .. }
                    )
                )
            })
        })
        .collect()
}

pub(super) fn available_inputs(columns: &[Column], rows: &[Row]) -> HashSet<ColumnKey> {
    columns
        .iter()
        .enumerate()
        .filter(|(index, _)| {
            rows.iter().any(|row| {
                matches!(
                    row.patterns.get(*index),
                    Some(
                        SurfacePattern::Var { .. }
                            | SurfacePattern::Constructor { .. }
                            | SurfacePattern::Record { .. }
                    )
                )
            })
        })
        .map(|(_, column)| column.key.clone())
        .chain(
            rows.iter()
                .flat_map(|row| row.bindings.iter().map(|(_, key)| key.clone())),
        )
        .collect()
}

pub(super) fn canonicalize(
    columns: &mut [Column],
    rows: &mut [Row],
) -> Vec<(ColumnKey, ColumnKey)> {
    let mut sources = Vec::new();
    let mut seen = HashSet::new();
    for key in columns.iter().map(|column| &column.key).chain(
        rows.iter()
            .flat_map(|row| row.bindings.iter().map(|(_, key)| key)),
    ) {
        if seen.insert(key.clone()) {
            sources.push(key.clone());
        }
    }
    let mapping = sources
        .iter()
        .enumerate()
        .map(|(index, source)| (source.clone(), ColumnKey::slot(index)))
        .collect::<Vec<_>>();
    for column in columns {
        column.key = mapped_key(&mapping, &column.key);
    }
    for row in rows {
        for (_, key) in &mut row.bindings {
            *key = mapped_key(&mapping, key);
        }
    }
    mapping
}

pub(super) fn map_actions(
    mapping: &[(ColumnKey, ColumnKey)],
    available: &HashSet<ColumnKey>,
    span: TextRange,
) -> Vec<Action> {
    mapping
        .iter()
        .filter(|(source, target)| available.contains(source) && source != target)
        .map(|(source, target)| Action::Map {
            source: source.clone(),
            target: target.clone(),
            span,
        })
        .collect()
}

fn mapped_key(mapping: &[(ColumnKey, ColumnKey)], key: &ColumnKey) -> ColumnKey {
    mapping
        .iter()
        .find(|(source, _)| source == key)
        .map(|(_, target)| target.clone())
        .expect("matrix input has a canonical slot")
}

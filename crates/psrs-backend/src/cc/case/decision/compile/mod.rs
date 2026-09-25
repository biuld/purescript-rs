use super::*;
use psrs_core::{CaseBranch, Module};
use psrs_hir::TypeId as HirTypeId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

mod constructors;
mod matrix;
#[cfg(test)]
mod oracle_record_tests;
#[cfg(test)]
mod oracle_tests;
#[cfg(test)]
mod tests;
use matrix::{
    available_inputs, canonicalize, choose_column, map_actions, pattern_type, surface_pattern,
};

struct Compiler<'a> {
    module: &'a Module,
    newtypes: HashSet<HirTypeId>,
    memo: HashMap<MatrixKey, NodeId>,
    nodes: Vec<Decision>,
}

pub fn compile_dag(
    module: &Module,
    newtypes: &HashSet<HirTypeId>,
    scrutinee_type: TypeId,
    branches: &[CaseBranch],
    span: TextRange,
) -> Result<DecisionDag, &'static str> {
    if branches.is_empty() {
        return Err("case has no alternatives");
    }
    let rows = branches
        .iter()
        .enumerate()
        .map(|(branch, case)| Row {
            patterns: vec![surface_pattern(&case.pattern)],
            bindings: Vec::new(),
            branch,
            span: case.span,
        })
        .collect();
    let mut compiler = Compiler {
        module,
        newtypes: newtypes.clone(),
        memo: HashMap::new(),
        nodes: Vec::new(),
    };
    let columns = vec![Column {
        key: ColumnKey::root(),
        ty: scrutinee_type,
    }];
    let root = compiler.compile_matrix(columns, rows, span)?;
    Ok(DecisionDag {
        root,
        nodes: compiler.nodes,
    })
}

impl Compiler<'_> {
    fn compile_matrix(
        &mut self,
        columns: Vec<Column>,
        rows: Vec<Row>,
        span: TextRange,
    ) -> Result<NodeId, &'static str> {
        let key = MatrixKey {
            columns: columns.clone(),
            rows: rows.clone(),
        };
        if let Some(node) = self.memo.get(&key) {
            return Ok(*node);
        }
        let node = self.compile_uncached(columns, rows, span)?;
        self.memo.insert(key, node);
        Ok(node)
    }

    fn compile_uncached(
        &mut self,
        mut columns: Vec<Column>,
        mut rows: Vec<Row>,
        span: TextRange,
    ) -> Result<NodeId, &'static str> {
        if rows.is_empty() {
            return Ok(self.push(Decision::Fail { span }));
        }
        if columns.is_empty() {
            return Ok(self.leaf(&rows[0]));
        }

        if let Some(column) = (0..columns.len()).find(|column| {
            rows.iter().all(|row| {
                matches!(
                    row.patterns[*column],
                    SurfacePattern::Any { .. } | SurfacePattern::Var { .. }
                )
            })
        }) {
            let available = available_inputs(&columns, &rows);
            let key = columns[column].key.clone();
            let selected_type = columns[column].ty;
            for row in &mut rows {
                let pattern = row.patterns.remove(column);
                if let SurfacePattern::Var { id, .. } = pattern {
                    row.bindings.push((id, key.clone()));
                }
            }
            columns.remove(column);
            let aliases = canonicalize(&mut columns, &mut rows);
            let actions = map_actions(&aliases, &available, span);
            let target = self.compile_matrix(columns, rows, span)?;
            return Ok(self.push(Decision::Switch {
                column: key.clone(),
                ty: selected_type,
                edges: vec![DecisionEdge {
                    test: Test::Irrefutable,
                    actions,
                    target,
                    span,
                }],
                default_actions: Vec::new(),
                default: None,
                span,
            }));
        }

        let column = choose_column(&rows, columns.len());
        let ty = columns[column].ty;
        if let Some((field_type, symbol)) = self.newtype_field(ty) {
            let inferred_field_type = rows.iter().find_map(|row| match &row.patterns[column] {
                SurfacePattern::Constructor {
                    symbol: found,
                    arguments,
                    ..
                } if *found == symbol => arguments.first().map(pattern_type),
                _ => None,
            });
            for row in &mut rows {
                let pat = std::mem::replace(&mut row.patterns[column], SurfacePattern::Any { ty });
                row.patterns[column] = match pat {
                    SurfacePattern::Constructor {
                        symbol: found,
                        mut arguments,
                        ..
                    } if found == symbol => {
                        if arguments.len() != 1 {
                            return Err("newtype pattern must have exactly one field");
                        }
                        arguments.remove(0)
                    }
                    SurfacePattern::Any { .. } | SurfacePattern::Var { .. } => pat,
                    SurfacePattern::Constructor { .. } => {
                        return Err("case pattern constructor does not belong to the newtype");
                    }
                    SurfacePattern::Record { .. } => {
                        return Err("record pattern does not match a newtype");
                    }
                };
            }
            columns[column].ty = inferred_field_type.unwrap_or(field_type);
            return self.compile_matrix(columns, rows, span);
        }

        let surface = self.surface(ty)?;
        if surface.is_record {
            return self.compile_product(columns, rows, column, surface, None, span);
        }
        if let Some(case) = surface.cases.iter().find(|case| case.irrefutable).cloned() {
            return self.compile_product(columns, rows, column, surface, Some(case), span);
        }
        self.compile_sum(columns, rows, column, surface, span)
    }

    fn leaf(&mut self, row: &Row) -> NodeId {
        let mut actions = row
            .bindings
            .iter()
            .map(|(id, source)| Action::Bind {
                id: *id,
                source: source.clone(),
            })
            .collect::<Vec<_>>();
        self.push(Decision::Leaf {
            branch: row.branch,
            actions: std::mem::take(&mut actions),
            span: row.span,
        })
    }

    fn push(&mut self, node: Decision) -> NodeId {
        let id = NodeId(self.nodes.len());
        self.nodes.push(node);
        id
    }
}

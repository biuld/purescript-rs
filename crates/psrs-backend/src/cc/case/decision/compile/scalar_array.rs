use super::matrix::{available_inputs, canonicalize, map_actions, needed_fields, pattern_type};
use super::*;
use psrs_core::{Literal, Type, TypeConstructor};

impl Compiler<'_> {
    pub(super) fn compile_literals(
        &mut self,
        columns: Vec<Column>,
        rows: Vec<Row>,
        column: usize,
        span: TextRange,
    ) -> Result<NodeId, &'static str> {
        let parent = columns[column].clone();
        let available = available_inputs(&columns, &rows);
        let boolean = is_boolean_type(self.module, parent.ty);
        let mut literals = Vec::<Literal>::new();
        for row in &rows {
            match &row.patterns[column] {
                SurfacePattern::Literal { value, .. } => {
                    if !literals.contains(value) {
                        literals.push(value.clone());
                    }
                }
                SurfacePattern::Any { .. } | SurfacePattern::Var { .. } => {}
                _ => return Err("literal pattern does not match its scrutinee type"),
            }
        }

        let mut edges = Vec::with_capacity(literals.len());
        let complete_boolean = boolean
            && literals.contains(&Literal::Boolean(false))
            && literals.contains(&Literal::Boolean(true));
        for literal in literals {
            let mut specialized = Vec::new();
            let action_span = rows
                .iter()
                .find_map(|row| match &row.patterns[column] {
                    SurfacePattern::Literal { value, span, .. } if value == &literal => Some(*span),
                    _ => None,
                })
                .unwrap_or(span);
            for mut row in rows.clone() {
                let pattern = row.patterns.remove(column);
                match pattern {
                    SurfacePattern::Any { .. } => specialized.push(row),
                    SurfacePattern::Var { id, .. } => {
                        row.bindings.push((id, parent.key.clone()));
                        specialized.push(row);
                    }
                    SurfacePattern::Literal { value, .. } if value == literal => {
                        specialized.push(row);
                    }
                    SurfacePattern::Literal { .. } => {}
                    _ => return Err("literal pattern does not match its scrutinee type"),
                }
            }
            let mut child_columns = columns.clone();
            child_columns.remove(column);
            let aliases = canonicalize(&mut child_columns, &mut specialized);
            let actions = map_actions(&aliases, &available, action_span);
            let target = self.compile_matrix(child_columns, specialized, action_span)?;
            edges.push(DecisionEdge {
                test: Test::Literal {
                    value: literal,
                    ty: parent.ty,
                },
                actions,
                target,
                span: action_span,
            });
        }

        let mut default = default_rows(&rows, column, &parent.key);
        let mut default_columns = columns;
        default_columns.remove(column);
        let aliases = canonicalize(&mut default_columns, &mut default);
        let default_actions = map_actions(&aliases, &available, span);
        let target = if complete_boolean {
            None
        } else {
            Some(self.compile_matrix(default_columns, default, span)?)
        };
        Ok(self.push(Decision::Switch {
            column: parent.key,
            ty: parent.ty,
            edges,
            default_actions,
            default: target,
            span,
        }))
    }

    pub(super) fn compile_arrays(
        &mut self,
        columns: Vec<Column>,
        rows: Vec<Row>,
        column: usize,
        span: TextRange,
    ) -> Result<NodeId, &'static str> {
        let parent = columns[column].clone();
        let element_type = array_element_type(self.module, parent.ty)
            .ok_or("array pattern type is not an array")?;
        let available = available_inputs(&columns, &rows);
        let mut lengths = Vec::<usize>::new();
        for row in &rows {
            match &row.patterns[column] {
                SurfacePattern::Array { elements, .. } => {
                    if !lengths.contains(&elements.len()) {
                        lengths.push(elements.len());
                    }
                }
                SurfacePattern::Any { .. } | SurfacePattern::Var { .. } => {}
                _ => return Err("array pattern does not match its scrutinee type"),
            }
        }

        let mut edges = Vec::with_capacity(lengths.len());
        for length in lengths {
            let fields = (0..length)
                .map(|index| Column {
                    key: parent.key.child(PathStep::ArrayElement(index as u32)),
                    ty: rows
                        .iter()
                        .find_map(|row| match &row.patterns[column] {
                            SurfacePattern::Array { elements, .. } if elements.len() == length => {
                                elements.get(index).map(pattern_type)
                            }
                            _ => None,
                        })
                        .unwrap_or(element_type),
                })
                .collect::<Vec<_>>();
            let action_span = rows
                .iter()
                .find_map(|row| match &row.patterns[column] {
                    SurfacePattern::Array { elements, span, .. } if elements.len() == length => {
                        Some(*span)
                    }
                    _ => None,
                })
                .unwrap_or(span);
            let mut specialized = Vec::new();
            for mut row in rows.clone() {
                let pattern = row.patterns.remove(column);
                let elements = match pattern {
                    SurfacePattern::Any { .. } => (0..length)
                        .map(|_| SurfacePattern::Any { ty: element_type })
                        .collect(),
                    SurfacePattern::Var { id, .. } => {
                        row.bindings.push((id, parent.key.clone()));
                        (0..length)
                            .map(|_| SurfacePattern::Any { ty: element_type })
                            .collect()
                    }
                    SurfacePattern::Array { elements, .. } if elements.len() == length => elements,
                    SurfacePattern::Array { .. } => continue,
                    _ => return Err("array pattern does not match its scrutinee type"),
                };
                row.patterns.splice(column..column, elements).for_each(drop);
                specialized.push(row);
            }
            let needed = needed_fields(&specialized, column, fields.len());
            let mut child_columns = columns.clone();
            child_columns.splice(column..column + 1, fields);
            let aliases = canonicalize(&mut child_columns, &mut specialized);
            let mut actions = map_actions(&aliases, &available, action_span);
            for index in needed {
                let source = parent.key.clone();
                let raw_key = parent.key.child(PathStep::ArrayElement(index as u32));
                let target = aliases
                    .iter()
                    .find(|(key, _)| key == &raw_key)
                    .map(|(_, key)| key.clone())
                    .ok_or("array projection is missing its child matrix column")?;
                actions.push(Action::ArrayGet {
                    source,
                    target,
                    index: index as u32,
                    source_type: parent.ty,
                    target_type: child_columns[column + index].ty,
                    span: action_span,
                });
            }
            let target = self.compile_matrix(child_columns, specialized, action_span)?;
            edges.push(DecisionEdge {
                test: Test::ArrayLength { length },
                actions,
                target,
                span: action_span,
            });
        }

        let mut default = default_rows(&rows, column, &parent.key);
        let mut default_columns = columns;
        default_columns.remove(column);
        let aliases = canonicalize(&mut default_columns, &mut default);
        let default_actions = map_actions(&aliases, &available, span);
        let default = Some(self.compile_matrix(default_columns, default, span)?);
        Ok(self.push(Decision::Switch {
            column: parent.key,
            ty: parent.ty,
            edges,
            default_actions,
            default,
            span,
        }))
    }
}

fn default_rows(rows: &[Row], column: usize, key: &ColumnKey) -> Vec<Row> {
    let mut default = Vec::new();
    for mut row in rows.iter().cloned() {
        match row.patterns.remove(column) {
            SurfacePattern::Any { .. } => default.push(row),
            SurfacePattern::Var { id, .. } => {
                row.bindings.push((id, key.clone()));
                default.push(row);
            }
            _ => {}
        }
    }
    default
}

fn array_element_type(module: &psrs_core::Module, ty: TypeId) -> Option<TypeId> {
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

fn is_boolean_type(module: &psrs_core::Module, ty: TypeId) -> bool {
    let ty = crate::cc::layout::unquantified_type(module, ty);
    matches!(
        module.types.get(ty.0 as usize),
        Some(Type::Constructor(TypeConstructor::Boolean))
    )
}

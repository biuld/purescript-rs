use super::matrix::{available_inputs, canonicalize, map_actions, needed_fields, pattern_type};
use super::*;
use psrs_core::Type;

impl Compiler<'_> {
    pub(super) fn compile_product(
        &mut self,
        columns: Vec<Column>,
        rows: Vec<Row>,
        column: usize,
        surface: DecisionSurface,
        case: Option<CaseSignature>,
        span: TextRange,
    ) -> Result<NodeId, &'static str> {
        let parent = columns[column].clone();
        let available = available_inputs(&columns, &rows);
        let (symbol, constructor, field_types, record) = if let Some(case) = case {
            (
                Some(case.symbol),
                Some((case.symbol, case.tag)),
                case.field_types,
                false,
            )
        } else {
            (
                None,
                None,
                surface.record_fields.iter().map(|(_, ty)| *ty).collect(),
                true,
            )
        };
        let action_span = rows
            .iter()
            .find_map(|row| match &row.patterns[column] {
                SurfacePattern::Constructor {
                    symbol: found,
                    span,
                    ..
                } if Some(*found) == symbol => Some(*span),
                SurfacePattern::Record { span, .. } if record => Some(*span),
                _ => None,
            })
            .unwrap_or(rows[0].span);
        let mut fields = Vec::with_capacity(field_types.len());
        for (index, declared_type) in field_types.iter().copied().enumerate() {
            let explicit = rows.iter().find_map(|row| {
                let pattern = row.patterns.get(column)?;
                match pattern {
                    SurfacePattern::Constructor {
                        symbol: found,
                        arguments,
                        ..
                    } if Some(*found) == symbol => arguments.get(index),
                    SurfacePattern::Record { fields, .. } if record => {
                        let label = &surface.record_fields[index].0;
                        fields
                            .iter()
                            .find(|(name, _)| name == label)
                            .map(|(_, pat)| pat)
                    }
                    _ => None,
                }
            });
            let ty = explicit.map_or(declared_type, pattern_type);
            let key = if let Some(symbol) = symbol {
                parent
                    .key
                    .child(PathStep::ConstructorField(symbol, index as u32))
            } else {
                parent.key.child(PathStep::RecordField(index as u32))
            };
            fields.push(Column { key, ty });
        }
        let mut specialized = Vec::with_capacity(rows.len());
        for mut row in rows {
            let parent_pattern = row.patterns.remove(column);
            let child_patterns = match parent_pattern {
                SurfacePattern::Any { .. } => fields
                    .iter()
                    .map(|field| SurfacePattern::Any { ty: field.ty })
                    .collect(),
                SurfacePattern::Var { id, .. } => {
                    row.bindings.push((id, parent.key.clone()));
                    fields
                        .iter()
                        .map(|field| SurfacePattern::Any { ty: field.ty })
                        .collect()
                }
                SurfacePattern::Constructor {
                    symbol: found,
                    arguments,
                    ..
                } if Some(found) == symbol => {
                    if arguments.len() != fields.len() {
                        return Err("constructor pattern has the wrong field count");
                    }
                    arguments
                }
                SurfacePattern::Record {
                    fields: pattern_fields,
                    ..
                } if record => surface
                    .record_fields
                    .iter()
                    .map(|(label, _)| {
                        pattern_fields
                            .iter()
                            .find(|(name, _)| name == label)
                            .map_or(
                                SurfacePattern::Any {
                                    ty: surface
                                        .record_fields
                                        .iter()
                                        .find(|(field, _)| field == label)
                                        .map_or(parent.ty, |(_, ty)| *ty),
                                },
                                |(_, pattern)| pattern.clone(),
                            )
                    })
                    .collect(),
                SurfacePattern::Constructor { .. } | SurfacePattern::Record { .. } => {
                    return Err("pattern does not match its scrutinee type");
                }
            };
            let index = column.min(row.patterns.len());
            row.patterns.splice(index..index, child_patterns);
            specialized.push(row);
        }
        let needed = needed_fields(&specialized, column, fields.len());
        let mut child_columns = columns;
        child_columns.splice(column..column + 1, fields);
        let aliases = canonicalize(&mut child_columns, &mut specialized);
        let mut actions = map_actions(&aliases, &available, action_span);
        for index in needed {
            let raw_key = parent.key.child(if let Some(symbol) = symbol {
                PathStep::ConstructorField(symbol, index as u32)
            } else {
                PathStep::RecordField(index as u32)
            });
            let target = aliases
                .iter()
                .find(|(source, _)| source == &raw_key)
                .map(|(_, target)| target.clone())
                .expect("projected field belongs to the child matrix");
            actions.push(Action::Project {
                source: parent.key.clone(),
                target,
                field: index as u32,
                source_type: parent.ty,
                declared_type: field_types[index],
                target_type: child_columns[column + index].ty,
                constructor,
                newtype: false,
                span: action_span,
            });
        }
        let target = self.compile_matrix(child_columns, specialized, span)?;
        let edge = DecisionEdge {
            test: Test::Irrefutable,
            actions,
            target,
            span: action_span,
        };
        Ok(self.push(Decision::Switch {
            column: parent.key.clone(),
            ty: parent.ty,
            edges: vec![edge],
            default_actions: Vec::new(),
            default: None,
            span,
        }))
    }

    pub(super) fn compile_sum(
        &mut self,
        columns: Vec<Column>,
        rows: Vec<Row>,
        column: usize,
        surface: DecisionSurface,
        span: TextRange,
    ) -> Result<NodeId, &'static str> {
        let parent = columns[column].clone();
        let available = available_inputs(&columns, &rows);
        let mut used = Vec::<CaseSignature>::new();
        for row in &rows {
            match &row.patterns[column] {
                SurfacePattern::Constructor { symbol, .. } => {
                    let Some(case) = surface.cases.iter().find(|case| case.symbol == *symbol)
                    else {
                        return Err(
                            "case pattern constructor does not belong to the scrutinee type",
                        );
                    };
                    if !used.iter().any(|known| known.symbol == *symbol) {
                        used.push(case.clone());
                    }
                }
                SurfacePattern::Any { .. } | SurfacePattern::Var { .. } => {}
                SurfacePattern::Record { .. } => {
                    return Err("record pattern does not match a data type");
                }
            }
        }
        let mut edges = Vec::with_capacity(used.len());
        for case in &used {
            let fields = self.constructor_columns(&parent, &rows, column, case);
            let mut specialized = Vec::new();
            let action_span = rows
                .iter()
                .find_map(|row| match &row.patterns[column] {
                    SurfacePattern::Constructor { symbol, span, .. } if *symbol == case.symbol => {
                        Some(*span)
                    }
                    _ => None,
                })
                .unwrap_or(span);
            for mut row in rows.clone() {
                let pattern = row.patterns.remove(column);
                let args = match pattern {
                    SurfacePattern::Any { .. } => fields
                        .iter()
                        .map(|field| SurfacePattern::Any { ty: field.ty })
                        .collect(),
                    SurfacePattern::Var { id, .. } => {
                        row.bindings.push((id, parent.key.clone()));
                        fields
                            .iter()
                            .map(|field| SurfacePattern::Any { ty: field.ty })
                            .collect()
                    }
                    SurfacePattern::Constructor {
                        symbol, arguments, ..
                    } if symbol == case.symbol => {
                        if arguments.len() != fields.len() {
                            return Err("constructor pattern has the wrong field count");
                        }
                        arguments
                    }
                    SurfacePattern::Constructor { .. } => continue,
                    SurfacePattern::Record { .. } => {
                        return Err("record pattern does not match a data type");
                    }
                };
                row.patterns.splice(column..column, args);
                specialized.push(row);
            }
            let needed = needed_fields(&specialized, column, fields.len());
            let mut child_columns = columns.clone();
            child_columns.splice(column..column + 1, fields);
            let aliases = canonicalize(&mut child_columns, &mut specialized);
            let mut actions = map_actions(&aliases, &available, action_span);
            actions.push(Action::TestTag {
                type_id: parent.ty,
                tag: case.tag,
            });
            for field in needed {
                let raw_key = parent
                    .key
                    .child(PathStep::ConstructorField(case.symbol, field as u32));
                let target = aliases
                    .iter()
                    .find(|(source, _)| source == &raw_key)
                    .map(|(_, target)| target.clone())
                    .expect("projected field belongs to the child matrix");
                actions.push(Action::Project {
                    source: parent.key.clone(),
                    target,
                    field: field as u32,
                    source_type: parent.ty,
                    declared_type: case.field_types[field],
                    target_type: child_columns[column + field].ty,
                    constructor: Some((case.symbol, case.tag)),
                    newtype: false,
                    span: action_span,
                });
            }
            let target = self.compile_matrix(child_columns, specialized, span)?;
            edges.push(DecisionEdge {
                test: Test::Constructor {
                    symbol: case.symbol,
                    tag: case.tag,
                },
                actions,
                target,
                span: action_span,
            });
        }

        let has_irrefutable = rows.iter().any(|row| {
            matches!(
                row.patterns[column],
                SurfacePattern::Any { .. } | SurfacePattern::Var { .. }
            )
        });
        let covered = used.len() == surface.cases.len();
        let mut default_actions = Vec::new();
        let default = if has_irrefutable || !covered {
            let mut defaults = Vec::new();
            for mut row in rows {
                match row.patterns.remove(column) {
                    SurfacePattern::Any { .. } => {}
                    SurfacePattern::Var { id, .. } => row.bindings.push((id, parent.key.clone())),
                    SurfacePattern::Constructor { .. } | SurfacePattern::Record { .. } => continue,
                }
                defaults.push(row);
            }
            let mut default_columns = columns;
            default_columns.remove(column);
            let aliases = canonicalize(&mut default_columns, &mut defaults);
            default_actions = map_actions(&aliases, &available, span);
            Some(self.compile_matrix(default_columns, defaults, span)?)
        } else {
            None
        };
        Ok(self.push(Decision::Switch {
            column: parent.key.clone(),
            ty: parent.ty,
            edges,
            default_actions,
            default,
            span,
        }))
    }

    pub(super) fn constructor_columns(
        &self,
        parent: &Column,
        rows: &[Row],
        column: usize,
        case: &CaseSignature,
    ) -> Vec<Column> {
        (0..case.field_types.len())
            .map(|index| {
                let explicit_type = rows.iter().find_map(|row| match &row.patterns[column] {
                    SurfacePattern::Constructor {
                        symbol, arguments, ..
                    } if *symbol == case.symbol => arguments.get(index).map(pattern_type),
                    _ => None,
                });
                Column {
                    key: parent
                        .key
                        .child(PathStep::ConstructorField(case.symbol, index as u32)),
                    ty: explicit_type.unwrap_or(case.field_types[index]),
                }
            })
            .collect()
    }

    pub(super) fn surface(&self, ty: TypeId) -> Result<DecisionSurface, &'static str> {
        match self.module.types.get(ty.0 as usize) {
            Some(Type::Record(fields)) => Ok(DecisionSurface {
                cases: Vec::new(),
                record_fields: fields.clone(),
                is_record: true,
            }),
            Some(_) => {
                let Some(type_id) = crate::cc::layout::user_type_id(self.module, ty) else {
                    return Err("refutable pattern has no constructor signature");
                };
                let mut cases = self
                    .module
                    .constructors
                    .iter()
                    .filter(|constructor| constructor.type_id == type_id)
                    .map(|constructor| CaseSignature {
                        symbol: constructor.symbol,
                        tag: constructor.tag,
                        arity: constructor.field_count,
                        irrefutable: false,
                        field_types: constructor.field_types.clone(),
                    })
                    .collect::<Vec<_>>();
                if cases.is_empty() {
                    return Err("case scrutinee type has no record of its constructors");
                }
                if cases
                    .iter()
                    .any(|case| case.arity != case.field_types.len())
                {
                    return Err("constructor field metadata is inconsistent");
                }
                if cases.len() == 1 {
                    cases[0].irrefutable = true;
                }
                Ok(DecisionSurface {
                    cases,
                    record_fields: Vec::new(),
                    is_record: false,
                })
            }
            None => Err("case scrutinee type is missing"),
        }
    }

    pub(super) fn newtype_field(&self, ty: TypeId) -> Option<(TypeId, SymbolId)> {
        let type_id = crate::cc::layout::user_type_id(self.module, ty)?;
        if !self.newtypes.contains(&type_id) {
            return None;
        }
        let mut constructors = self
            .module
            .constructors
            .iter()
            .filter(|item| item.type_id == type_id);
        let constructor = constructors.next()?;
        if constructors.next().is_some() || constructor.field_count != 1 {
            return None;
        }
        Some((constructor.field_types[0], constructor.symbol))
    }
}

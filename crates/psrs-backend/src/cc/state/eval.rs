use super::*;
use crate::cc::{AssignmentKind, ValueConversion};

pub(super) fn function<'a>(
    module: &'a Module,
    function: &'a Function,
    signatures: &HashMap<SymbolId, Signature>,
    passthrough: &HashSet<SymbolId>,
) -> Result<Option<FunctionFlow<'a>>, Vec<BackendError>> {
    let shapes = function
        .values
        .iter()
        .map(|value| (value.id, value.ty))
        .collect::<HashMap<_, _>>();
    let roots = function
        .parameters
        .iter()
        .filter(|id| shapes[id] == ValueShape::State)
        .copied()
        .collect::<Vec<_>>();
    let state_root = !roots.is_empty();
    let has_state = shapes.values().any(|shape| *shape == ValueShape::State);
    if !has_state && !execution::contains(&function.assignments) {
        return Ok(None);
    }
    if (has_state && roots.len() != 1) || roots.len() > 1 {
        return Err(error(
            function,
            "CC state body requires one checked dependency parameter",
        ));
    }
    if state_root {
        StateCallProjection::checked(&signatures[&function.symbol], &module.representations)
            .map_err(|message| error(function, message))?;
    }
    let input = DependencyId(0);
    let mut builder = Builder {
        module,
        function,
        signatures,
        passthrough,
        shapes,
        state_root,
        executions: Vec::new(),
        next: 1,
        operations: Vec::new(),
        passthrough_safe: true,
        blocks: vec![Block {
            id: 0,
            parameters: if state_root {
                vec![Dependency {
                    id: input,
                    region: RegionId(0),
                }]
            } else {
                Vec::new()
            },
            transitions: Vec::new(),
            terminator: Terminator::Trap,
            span: function.span,
        }],
    };
    let mut path = Path {
        block: 0,
        current: input,
        revision: 0,
        values: roots
            .first()
            .map(|id| HashMap::from([(*id, Value::State(input))]))
            .unwrap_or_default(),
        terminated: false,
    };
    builder.assignments(&function.assignments, &mut path)?;
    if !path.terminated {
        let mut returned = Vec::new();
        builder.value(&path, function.result).states(&mut returned);
        let expected = if state_root {
            vec![path.current]
        } else {
            Vec::new()
        };
        if returned != expected {
            return Err(error(
                function,
                "CC state return loses or replays an executed dependency",
            ));
        }
        builder.blocks[path.block].terminator = Terminator::Return(returned);
    }
    let graph = Graph {
        entry: 0,
        regions: if state_root {
            vec![RegionId(0)]
        } else {
            Vec::new()
        },
        blocks: builder.blocks,
    };
    graph.verify().map_err(|err| {
        vec![
            BackendError::invalid_ir("P8 CC state verification", err.span, err.message)
                .with_module(function.symbol.module),
        ]
    })?;
    Ok(Some(FunctionFlow {
        source: module,
        function,
        graph,
        operations: builder.operations,
        executions: builder.executions,
        passthrough_safe: builder.passthrough_safe,
    }))
}

impl<'a> Builder<'a, '_> {
    pub(super) fn assignments(
        &mut self,
        assignments: &'a [Assignment],
        path: &mut Path,
    ) -> Result<(), Vec<BackendError>> {
        for assignment in assignments {
            if path.terminated {
                break;
            }
            let value = match &assignment.kind {
                AssignmentKind::StateExecution { signature, .. } => {
                    self.execution(assignment, *signature)?
                }
                AssignmentKind::ProductNew { arguments, .. } => {
                    Value::Product(arguments.iter().map(|id| self.value(path, *id)).collect())
                }
                AssignmentKind::ProductGet { value, field, .. } => match self.value(path, *value) {
                    Value::Product(fields) => fields
                        .get(*field as usize)
                        .cloned()
                        .unwrap_or(Value::Opaque),
                    _ => Value::Opaque,
                },
                AssignmentKind::FunctionRef { function, .. } => Value::Closure(*function),
                AssignmentKind::DirectCall {
                    function,
                    arguments,
                } => {
                    let signature = self.signatures.get(function).ok_or_else(|| {
                        error(
                            self.function,
                            "CC state invocation has no checked call signature",
                        )
                    })?;
                    self.call(
                        assignment,
                        arguments,
                        signature,
                        self.passthrough.contains(function),
                        path,
                    )?
                }
                AssignmentKind::IndirectCall {
                    function,
                    signature,
                    arguments,
                } => {
                    let signature = self
                        .module
                        .representations
                        .signature(*signature)
                        .ok_or_else(|| {
                            error(
                                self.function,
                                "CC state invocation has no checked closure signature",
                            )
                        })?;
                    let pure = matches!(self.value(path, *function), Value::Closure(symbol)
                        if self.passthrough.contains(&symbol));
                    self.call(assignment, arguments, signature, pure, path)?
                }
                AssignmentKind::If {
                    then_assignments,
                    then_value,
                    else_assignments,
                    else_value,
                    ..
                } => self.branches(
                    assignment,
                    then_assignments,
                    *then_value,
                    else_assignments,
                    *else_value,
                    path,
                )?,
                AssignmentKind::TagSwitch { cases, default_assignments, default_value, .. } => {
                    let mut arms = cases.iter().map(|case| (case.assignments.as_slice(), case.value)).collect::<Vec<_>>();
                    arms.push((default_assignments.as_slice(), *default_value));
                    self.alternatives(assignment, &arms, true, path)?
                }
                AssignmentKind::AggregateConvert {
                    value, conversion, ..
                } => Self::convert(self.value(path, *value), &conversion.plan),
                AssignmentKind::Unreachable => {
                    path.terminated = true;
                    self.blocks[path.block].terminator = Terminator::Trap;
                    Value::Opaque
                }
                AssignmentKind::Constant(_)
                | AssignmentKind::NumberConstant(_)
                | AssignmentKind::StringConstant(_)
                | AssignmentKind::Primitive { .. }
                | AssignmentKind::Unary { .. }
                | AssignmentKind::ClosureGetCapture { .. }
                // Construction preserves the incoming State. This summary
                // does not erase allocation effects or authorize call CSE.
                | AssignmentKind::ArrayNew { .. }
                | AssignmentKind::VariantNew { .. }
                | AssignmentKind::VariantTag { .. }
                | AssignmentKind::VariantGet { .. } => Value::Opaque,
                _ => {
                    self.passthrough_safe = false;
                    Value::Opaque
                }
            };
            if self.shapes[&assignment.destination] == ValueShape::State
                && !matches!(value, Value::State(_))
                && !path.terminated
            {
                return Err(error(
                    self.function,
                    "CC state value has no checked instruction producer",
                ));
            }
            path.values.insert(assignment.destination, value);
        }
        Ok(())
    }

    fn call(
        &mut self,
        assignment: &'a Assignment,
        arguments: &[ValueId],
        signature: &Signature,
        pure: bool,
        path: &mut Path,
    ) -> Result<Value, Vec<BackendError>> {
        let Some(projection) =
            StateCallProjection::checked(signature, &self.module.representations)
                .map_err(|message| error(self.function, message))?
        else {
            self.passthrough_safe = false;
            return Ok(Value::Opaque);
        };
        let input_value = arguments.get(projection.state_parameter).ok_or_else(|| {
            error(
                self.function,
                "CC state invocation is missing its dependency operand",
            )
        })?;
        let Value::State(input) = self.value(path, *input_value) else {
            return Err(error(
                self.function,
                "CC state invocation has no checked operand provenance",
            ));
        };
        if input != path.current {
            return Err(error(
                self.function,
                "CC state invocation consumes a stale dependency",
            ));
        }
        let output = if pure {
            input
        } else {
            self.passthrough_safe = false;
            let output = self.fresh();
            let operation = self.operations.len() as u32;
            self.operations.push(Operation {
                assignment,
                input,
                output,
            });
            self.blocks[path.block].transitions.push(Transition {
                operation,
                input,
                output: Dependency {
                    id: output,
                    region: RegionId(0),
                },
                span: assignment.span,
            });
            path.current = output;
            path.revision += 1;
            output
        };
        let mut fields = vec![Value::Opaque; 2];
        fields[projection.state_field] = Value::State(output);
        Ok(Value::Product(fields))
    }

    fn convert(value: Value, plan: &ValueConversion) -> Value {
        match plan {
            ValueConversion::Identity => value,
            ValueConversion::ProductMap { fields: plans, .. } => match value {
                Value::Product(fields) if fields.len() == plans.len() => Value::Product(
                    fields
                        .into_iter()
                        .zip(plans)
                        .map(|(field, plan)| Self::convert(field, plan))
                        .collect(),
                ),
                _ => Value::Opaque,
            },
            ValueConversion::Sequence(plans) => plans.iter().fold(value, Self::convert),
            _ => Value::Opaque,
        }
    }
}

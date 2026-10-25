//! Eager memoized initialization of recursive values using ordinary CC storage.
use super::plan::RecursiveLayout;
use super::*;
use crate::cc::lower::scope::LocalBinding;
use crate::cc::{ReprId, TagCase};
use std::rc::Rc;

struct Member<'a> {
    binding: &'a Binding,
    symbol: SymbolId,
    layout: RecursiveLayout,
    shape: ValueShape,
    prefix: Rc<str>,
}

impl FunctionLowerer<'_> {
    pub(super) fn lower_recursive_values(
        &mut self,
        bindings: &[Binding],
        group: &[usize],
        assignments: &mut Vec<Assignment>,
    ) -> Result<(), Vec<BackendError>> {
        let mut members = Vec::new();
        let mut ordered = group.to_vec();
        ordered.sort_unstable();
        for index in &ordered {
            let binding = &bindings[*index];
            let shape = self.value_shape(binding.binder.ty, binding.span)?;
            let layout = self
                .representations
                .recursion
                .get(&shape)
                .copied()
                .ok_or_else(|| {
                    lowering_error(binding.span, "recursive value has no planned memo layout")
                })?;
            let symbol = self.generated_symbols.borrow_mut().fresh(self.owner);
            self.generated_signatures.borrow_mut().insert(
                symbol,
                Signature {
                    parameters: vec![aggregate_shape(), ValueShape::String],
                    result: shape,
                },
            );
            let location =
                self.module
                    .sources
                    .location(binding.binder.span)
                    .map_err(|message| {
                        vec![BackendError::invalid_ir(
                            "P8 source location",
                            binding.binder.span,
                            message,
                        )]
                    })?;
            let prefix = format!(
                "{} was needed before it finished initializing (module {}, line ",
                binding.binder.name, location.name
            )
            .into();
            members.push(Member {
                prefix,
                binding,
                symbol,
                layout,
                shape,
            });
        }
        let recursive = group.iter().copied().collect();
        let ids = members.iter().map(|m| m.binding.binder.id).collect();
        let external = recursive_captures(bindings, &recursive, &ids);
        let span = members[0].binding.span;
        let mut captures = Vec::new();
        for member in &members {
            let empty = self.memo_variant(member.layout.state, 0, vec![], span, assignments);
            let cell = self.fresh(repr(member.layout.cell));
            assignments.push(Assignment {
                destination: cell,
                span,
                kind: AssignmentKind::ArrayNew {
                    destination: cell,
                    representation: member.layout.cell,
                    elements: vec![empty],
                },
            });
            captures.push(cell);
        }
        for local in &external {
            captures.push(
                self.locals
                    .get(local)
                    .cloned()
                    .ok_or_else(|| lowering_error(span, "recursive value capture is unavailable"))?
                    .storage(),
            );
        }
        let shapes = captures
            .iter()
            .map(|value| self.local_value_shape(*value, span))
            .collect::<Result<Vec<_>, _>>()?;
        for member in &members {
            self.lower_memo_getter(member, &members, &external, &shapes)?;
        }
        self.install_memo_getters(&members, &captures, span, assignments);
        // Within an SCC, force initializers in their original binding order.
        // Force every initializer now, even when the let body never reads it.
        for member in &members {
            let binding = self
                .locals
                .get(&member.binding.binder.id)
                .expect("installed getter")
                .clone();
            self.read_local_binding(binding, member.binding.span, assignments)?;
        }
        Ok(())
    }

    fn install_memo_getters(
        &mut self,
        members: &[Member<'_>],
        captures: &[ValueId],
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) {
        for member in members {
            let destination = self.fresh(closure_shape(member.layout.getter));
            assignments.push(Assignment {
                destination,
                span,
                kind: AssignmentKind::FunctionRef {
                    function: member.symbol,
                    signature: member.layout.getter,
                    captures: captures.to_vec(),
                },
            });
            self.locals.bind(
                member.binding.binder.id,
                LocalBinding::Getter {
                    prefix: Rc::clone(&member.prefix),
                    value: destination,
                    signature: member.layout.getter,
                },
            );
            self.local_types
                .insert(member.binding.binder.id, member.binding.binder.ty);
        }
    }

    fn lower_memo_getter(
        &mut self,
        member: &Member<'_>,
        members: &[Member<'_>],
        external: &[LocalId],
        shapes: &[ValueShape],
    ) -> Result<(), Vec<BackendError>> {
        let mut nested = self.child_lowerer();
        let closure = nested.fresh(aggregate_shape());
        let context = nested.fresh(ValueShape::String);
        let span = member.binding.span;
        let mut body = Vec::new();
        let mut captures = Vec::new();
        for (index, shape) in shapes.iter().enumerate() {
            let value = nested.fresh(*shape);
            body.push(Assignment {
                destination: value,
                span,
                kind: AssignmentKind::ClosureGetCapture {
                    closure,
                    index: index as u32,
                },
            });
            captures.push(value);
        }
        for (local, value) in external.iter().zip(&captures[members.len()..]) {
            let binding =
                self.locals.get(local).cloned().ok_or_else(|| {
                    lowering_error(span, "memo capture access mode is unavailable")
                })?;
            let ty =
                self.local_types.get(local).copied().ok_or_else(|| {
                    lowering_error(span, "memo capture source type is unavailable")
                })?;
            nested.locals.bind(*local, binding.with_storage(*value));
            nested.local_types.insert(*local, ty);
        }
        let index = members
            .iter()
            .position(|m| m.symbol == member.symbol)
            .expect("group member");
        let cell = captures[index];
        let zero = nested.fresh(ValueShape::Integer);
        body.push(Assignment {
            destination: zero,
            span,
            kind: AssignmentKind::Constant(0),
        });
        let state = nested.fresh(repr(member.layout.state));
        body.push(Assignment {
            destination: state,
            span,
            kind: AssignmentKind::ArrayGet {
                destination: state,
                representation: member.layout.cell,
                value: cell,
                index: zero,
            },
        });
        let tag = nested.fresh(ValueShape::Integer);
        body.push(Assignment {
            destination: tag,
            span,
            kind: AssignmentKind::VariantTag {
                destination: tag,
                representation: member.layout.state,
                value: state,
            },
        });
        let mut ready = Vec::new();
        let cached = nested.fresh(member.shape);
        ready.push(Assignment {
            destination: cached,
            span,
            kind: AssignmentKind::VariantGet {
                destination: cached,
                representation: member.layout.state,
                case: 2,
                field: 0,
                value: state,
            },
        });
        let mut initialize = Vec::new();
        let busy = nested.memo_variant(member.layout.state, 1, vec![], span, &mut initialize);
        nested.memo_write(member.layout.cell, cell, zero, busy, span, &mut initialize);
        nested.install_memo_getters(members, &captures, span, &mut initialize);
        let result = nested.lower_value(&member.binding.value, &mut initialize)?;
        let complete =
            nested.memo_variant(member.layout.state, 2, vec![result], span, &mut initialize);
        nested.memo_write(
            member.layout.cell,
            cell,
            zero,
            complete,
            span,
            &mut initialize,
        );
        let raised = nested.fresh(member.shape);
        let failure = vec![Assignment {
            destination: raised,
            span,
            kind: AssignmentKind::RaiseSourceError { message: context },
        }];
        let trapped = nested.fresh(member.shape);
        let trap = vec![Assignment {
            destination: trapped,
            span,
            kind: AssignmentKind::Unreachable,
        }];
        let destination = nested.fresh(member.shape);
        body.push(Assignment {
            destination,
            span,
            kind: AssignmentKind::TagSwitch {
                value: tag,
                cases: vec![
                    TagCase {
                        tag: 0,
                        assignments: initialize,
                        value: result,
                    },
                    TagCase {
                        tag: 1,
                        assignments: failure,
                        value: raised,
                    },
                    TagCase {
                        tag: 2,
                        assignments: ready,
                        value: cached,
                    },
                ],
                default_assignments: trap,
                default_value: trapped,
            },
        });
        let function = Function {
            symbol: member.symbol,
            name: format!("{}_memo_{}", member.binding.binder.name, span.start),
            parameters: vec![closure, context],
            values: nested.values,
            assignments: body,
            result: destination,
            result_type: member.shape,
            span,
        };
        let FunctionLowerer {
            generated,
            warnings,
            ..
        } = nested;
        self.generated.extend(generated);
        self.generated.push(function);
        self.warnings.extend(warnings);
        Ok(())
    }

    fn memo_variant(
        &mut self,
        representation: ReprId,
        case: u32,
        fields: Vec<ValueId>,
        span: TextRange,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let destination = self.fresh(repr(representation));
        body.push(Assignment {
            destination,
            span,
            kind: AssignmentKind::VariantNew {
                destination,
                representation,
                case,
                fields,
            },
        });
        destination
    }

    fn memo_write(
        &mut self,
        representation: ReprId,
        cell: ValueId,
        index: ValueId,
        new_value: ValueId,
        span: TextRange,
        body: &mut Vec<Assignment>,
    ) {
        body.push(Assignment {
            destination: cell,
            span,
            kind: AssignmentKind::ArraySet {
                destination: cell,
                representation,
                value: cell,
                index,
                new_value,
            },
        });
    }
}

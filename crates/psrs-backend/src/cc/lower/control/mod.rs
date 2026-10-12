//! Lower the inert control surface to a persistent continuation machine.

use super::{FunctionLowerer, LambdaLowering};
use crate::BackendError;
use crate::cc::layout::control::ControlLayout;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, AssignmentKind, SignatureId, ValueId, ValueShape};
use psrs_core::Expr;
use psrs_hir::{Intrinsic, SymbolId};
use psrs_span::TextRange;

mod builder;
mod executor;
mod fields;
mod lifetime;
mod owned;
mod replay;
mod scope;
use builder::Machine;

#[derive(Clone, Copy)]
pub(in crate::cc) struct ControlFunctions {
    emitted: bool,
    executor: SymbolId,
    resume: SymbolId,
    clone_sequence: SymbolId,
    rebuild_sequence: SymbolId,
    start: SymbolId,
    check: SymbolId,
    close: SymbolId,
    once: SymbolId,
    cancel: SymbolId,
}

impl ControlFunctions {
    fn contracts(
        self,
        layout: ControlLayout,
    ) -> [(SymbolId, SignatureId, crate::cc::ControlHelperRole); 8] {
        use crate::cc::ControlHelperRole::*;
        [
            (self.executor, layout.executor, Executor),
            (self.clone_sequence, layout.clone_sequence, CloneSequence),
            (
                self.rebuild_sequence,
                layout.rebuild_sequence,
                RebuildSequence,
            ),
            (self.start, layout.start, Start),
            (self.check, layout.boundary_check, Check),
            (self.close, layout.boundary_check, Close),
            (self.once, layout.once, Once),
            (self.cancel, layout.cancel, Cancel),
        ]
    }

    fn signature(self, layout: ControlLayout, function: SymbolId) -> SignatureId {
        self.contracts(layout)
            .into_iter()
            .find(|(symbol, _, _)| *symbol == function)
            .map(|(_, signature, _)| signature)
            .expect("predeclared control function")
    }
}

impl FunctionLowerer<'_> {
    pub(super) fn lower_control(
        &mut self,
        expression: &Expr,
        intrinsic: Intrinsic,
        arguments: &[Expr],
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let proof = psrs_core::checked_control_contract(
            self.module,
            intrinsic,
            expression.ty,
            &arguments
                .iter()
                .map(|argument| argument.ty)
                .collect::<Vec<_>>(),
        )
        .map_err(|message| control_error(expression.span, message))?;
        let contract =
            crate::cc::layout::control_contracts::lookup(self.module, self.representations, &proof)
                .ok_or_else(|| {
                    control_error(
                        expression.span,
                        "control use has no predeclared checked contract",
                    )
                })?;
        let fields = arguments
            .iter()
            .map(|argument| self.lower_array_element(argument, assignments))
            .collect::<Result<Vec<_>, _>>()?;
        let (value, implementation) =
            self.lower_control_frame(expression, intrinsic, fields.clone(), ty, assignments)?;
        let destination = self.fresh(ty);
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::ControlEvidence {
                value,
                implementation,
                arguments: fields,
                contract,
            },
            span: expression.span,
        });
        Ok(destination)
    }

    fn lower_control_frame(
        &mut self,
        expression: &Expr,
        intrinsic: Intrinsic,
        fields: Vec<ValueId>,
        ty: ValueShape,
        assignments: &mut Vec<Assignment>,
    ) -> Result<(ValueId, ValueId), Vec<BackendError>> {
        let layout = self.control_layout.ok_or_else(|| {
            control_error(
                expression.span,
                "control primitive used without a frame representation",
            )
        })?;
        if matches!(intrinsic, Intrinsic::ControlOnce | Intrinsic::ControlCancel) {
            let functions = self.ensure_control_functions(layout, expression.span)?;
            let function = if intrinsic == Intrinsic::ControlOnce {
                functions.once
            } else {
                functions.cancel
            };
            let result = self.control_call(
                function,
                functions.signature(layout, function),
                fields,
                expression.span,
                assignments,
            )?;
            if intrinsic == Intrinsic::ControlCancel {
                let unit = self.fresh(ty);
                assignments.push(Assignment {
                    destination: unit,
                    kind: AssignmentKind::Constant(0),
                    span: expression.span,
                });
                return Ok((unit, result));
            }
            let conversion = self.recover_payload(ty, expression.span)?;
            let source = builder::repr(layout.owner);
            return Ok((
                self.emit_conversion(
                    result,
                    source,
                    ty,
                    crate::cc::ValueConversion::Sequence(vec![
                        crate::cc::ValueConversion::EraseReference,
                        conversion,
                    ]),
                    expression.span,
                    assignments,
                ),
                result,
            ));
        }
        if intrinsic == Intrinsic::ControlRun {
            let functions = self.ensure_control_functions(layout, expression.span)?;
            let empty = self.control_empty_sequence(layout, expression.span, assignments);
            let boundary = self.control_call(
                functions.start,
                layout.start,
                vec![],
                expression.span,
                assignments,
            )?;
            let shape = builder::repr(layout.context);
            let context = self.fresh(shape);
            let context_fields = self.control_fields(
                layout.context,
                0,
                vec![boundary],
                expression.span,
                assignments,
            )?;
            assignments.push(Assignment {
                destination: context,
                kind: AssignmentKind::VariantNew {
                    destination: context,
                    representation: layout.context,
                    case: 0,
                    fields: context_fields,
                },
                span: expression.span,
            });
            let result = self.control_call(
                functions.executor,
                layout.executor,
                vec![fields[0], fields[1], fields[2], empty, boundary, context],
                expression.span,
                assignments,
            )?;
            self.control_call(
                functions.close,
                layout.boundary_check,
                vec![boundary],
                expression.span,
                assignments,
            )?;
            let conversion = self.recover_payload(ty, expression.span)?;
            return Ok((
                self.emit_conversion(
                    result,
                    erased_shape(),
                    ty,
                    conversion,
                    expression.span,
                    assignments,
                ),
                result,
            ));
        }
        let case = match intrinsic {
            Intrinsic::ControlReturn => 0,
            Intrinsic::ControlPerform => 1,
            Intrinsic::ControlBind => 2,
            Intrinsic::ControlHandle => 4,
            Intrinsic::ControlResume => 6,
            _ => return Err(control_error(expression.span, "not a control primitive")),
        };
        let destination = self.fresh(ty);
        let fields = self.control_fields(
            layout.computation,
            case,
            fields,
            expression.span,
            assignments,
        )?;
        assignments.push(Assignment {
            destination,
            kind: AssignmentKind::VariantNew {
                destination,
                representation: layout.computation,
                case,
                fields,
            },
            span: expression.span,
        });
        Ok((destination, destination))
    }

    fn control_empty_sequence(
        &mut self,
        layout: ControlLayout,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> ValueId {
        let shape = builder::repr(layout.sequence);
        let value = self.fresh(shape);
        assignments.push(Assignment {
            destination: value,
            kind: AssignmentKind::VariantNew {
                destination: value,
                representation: layout.sequence,
                case: 0,
                fields: vec![],
            },
            span,
        });
        value
    }

    fn ensure_control_functions(
        &mut self,
        layout: ControlLayout,
        span: TextRange,
    ) -> Result<ControlFunctions, Vec<BackendError>> {
        let functions = self
            .control_functions
            .borrow()
            .ok_or_else(|| control_error(span, "control helpers were not predeclared"))?;
        if functions.emitted {
            return Ok(functions);
        }
        for (symbol, id, _) in functions.contracts(layout) {
            let signature = self
                .representations
                .signature(id)
                .ok_or_else(|| {
                    control_error(
                        span,
                        "control lifetime function has no predeclared signature",
                    )
                })?
                .clone();
            self.generated_signatures
                .borrow_mut()
                .insert(symbol, signature);
        }
        let mut resume = Machine::new(self.child_lowerer(), layout, functions, span);
        let receiver = resume.lower.fresh(super::lambda::closure_value_type());
        let value = resume.lower.fresh(erased_shape());
        let mut assignments = Vec::new();
        let saved = resume.capture(
            receiver,
            0,
            builder::repr(layout.sequence),
            &mut assignments,
        );
        let boundary = resume.capture(
            receiver,
            1,
            builder::repr(layout.boundary),
            &mut assignments,
        );
        resume.check_boundary(boundary, &mut assignments);
        let result = resume.node(
            layout.computation,
            3,
            vec![value, saved, boundary],
            &mut assignments,
        );
        let resume = resume.finish(
            functions.resume,
            "control_resume",
            vec![receiver, value],
            assignments,
            result,
        );
        self.verify_synthesized(&resume)?;
        self.generated.push(resume);
        let executor = Machine::new(self.child_lowerer(), layout, functions, span).executor();
        self.verify_synthesized(&executor)?;
        self.generated.push(executor);
        for function in [
            Machine::new(self.child_lowerer(), layout, functions, span).clone_sequence(),
            Machine::new(self.child_lowerer(), layout, functions, span).rebuild_sequence(),
            Machine::new(self.child_lowerer(), layout, functions, span).start_boundary(),
            Machine::new(self.child_lowerer(), layout, functions, span).boundary_check(),
            Machine::new(self.child_lowerer(), layout, functions, span).boundary_close(),
            Machine::new(self.child_lowerer(), layout, functions, span).owned(),
            Machine::new(self.child_lowerer(), layout, functions, span).cancel(),
        ] {
            self.verify_synthesized(&function)?;
            self.generated.push(function);
        }
        *self.control_functions.borrow_mut() = Some(ControlFunctions {
            emitted: true,
            ..functions
        });
        Ok(functions)
    }
}

fn control_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::invalid_ir(
        "P8 closure conversion",
        span,
        message,
    )]
}

pub(in crate::cc) fn plan_control_helpers(
    layout: &mut crate::cc::layout::TypeLayout,
    allocator: &std::rc::Rc<std::cell::RefCell<super::GeneratedSymbolAllocator>>,
) -> Option<ControlFunctions> {
    let control = layout.control_layout?;
    let mut allocator = allocator.borrow_mut();
    let owner = layout.control_owner?;
    let functions = ControlFunctions {
        emitted: false,
        executor: allocator.fresh(owner),
        resume: allocator.fresh(owner),
        clone_sequence: allocator.fresh(owner),
        rebuild_sequence: allocator.fresh(owner),
        start: allocator.fresh(owner),
        check: allocator.fresh(owner),
        close: allocator.fresh(owner),
        once: allocator.fresh(owner),
        cancel: allocator.fresh(owner),
    };
    for (function, signature, role) in functions.contracts(control) {
        layout
            .representations
            .control_helpers
            .push(crate::cc::ControlHelperContract {
                role,
                function,
                signature,
            });
    }
    Some(functions)
}

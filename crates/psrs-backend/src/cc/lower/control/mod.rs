//! Lower the inert control surface to a persistent continuation machine.

use super::{FunctionLowerer, LambdaLowering};
use crate::BackendError;
use crate::cc::layout::control::ControlLayout;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, AssignmentKind, ValueId, ValueShape};
use psrs_core::Expr;
use psrs_hir::{Intrinsic, SymbolId};
use psrs_span::TextRange;

mod builder;
mod executor;
mod lifetime;
mod open;
mod owned;
use builder::Machine;

#[derive(Clone, Copy)]
pub(in crate::cc) struct ControlFunctions {
    executor: SymbolId,
    resume: SymbolId,
    open_executor: SymbolId,
    forward: SymbolId,
    project_left: SymbolId,
    project_right: SymbolId,
    start: SymbolId,
    check: SymbolId,
    close: SymbolId,
    once: SymbolId,
    cancel: SymbolId,
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
        let layout = self.control_layout.ok_or_else(|| {
            control_error(
                expression.span,
                "control primitive used without a frame representation",
            )
        })?;
        let fields = arguments
            .iter()
            .map(|argument| self.lower_array_element(argument, assignments))
            .collect::<Result<Vec<_>, _>>()?;
        if matches!(intrinsic, Intrinsic::ControlOnce | Intrinsic::ControlCancel) {
            let functions = self.ensure_control_functions(layout, expression.span)?;
            let function = if intrinsic == Intrinsic::ControlOnce {
                functions.once
            } else {
                functions.cancel
            };
            let result = self.fresh(erased_shape());
            assignments.push(Assignment {
                destination: result,
                kind: AssignmentKind::DirectCall {
                    function,
                    arguments: fields,
                },
                span: expression.span,
            });
            if intrinsic == Intrinsic::ControlCancel {
                let unit = self.fresh(ty);
                assignments.push(Assignment {
                    destination: unit,
                    kind: AssignmentKind::Constant(0),
                    span: expression.span,
                });
                return Ok(unit);
            }
            let conversion = self.recover_payload(ty, expression.span)?;
            return Ok(self.emit_conversion(
                result,
                erased_shape(),
                ty,
                conversion,
                expression.span,
                assignments,
            ));
        }
        if intrinsic == Intrinsic::ControlRun {
            let functions = self.ensure_control_functions(layout, expression.span)?;
            let empty = self.control_empty_sequence(layout, expression.span, assignments);
            let boundary = self.fresh(erased_shape());
            assignments.push(Assignment {
                destination: boundary,
                kind: AssignmentKind::DirectCall {
                    function: functions.start,
                    arguments: vec![],
                },
                span: expression.span,
            });
            let result = self.fresh(erased_shape());
            assignments.push(Assignment {
                destination: result,
                kind: AssignmentKind::DirectCall {
                    function: functions.executor,
                    arguments: vec![fields[0], fields[1], fields[2], empty, boundary],
                },
                span: expression.span,
            });
            let closed = self.fresh(erased_shape());
            assignments.push(Assignment {
                destination: closed,
                kind: AssignmentKind::DirectCall {
                    function: functions.close,
                    arguments: vec![boundary],
                },
                span: expression.span,
            });
            let conversion = self.recover_payload(ty, expression.span)?;
            return Ok(self.emit_conversion(
                result,
                erased_shape(),
                ty,
                conversion,
                expression.span,
                assignments,
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
        Ok(destination)
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
        self.emit_conversion(
            value,
            shape,
            erased_shape(),
            crate::cc::ValueConversion::EraseReference,
            span,
            assignments,
        )
    }

    fn ensure_control_functions(
        &mut self,
        layout: ControlLayout,
        span: TextRange,
    ) -> Result<ControlFunctions, Vec<BackendError>> {
        if let Some(functions) = *self.control_functions.borrow() {
            return Ok(functions);
        }
        let functions = {
            let mut symbols = self.generated_symbols.borrow_mut();
            ControlFunctions {
                executor: symbols.fresh(self.owner),
                resume: symbols.fresh(self.owner),
                open_executor: symbols.fresh(self.owner),
                forward: symbols.fresh(self.owner),
                project_left: symbols.fresh(self.owner),
                project_right: symbols.fresh(self.owner),
                start: symbols.fresh(self.owner),
                check: symbols.fresh(self.owner),
                close: symbols.fresh(self.owner),
                once: symbols.fresh(self.owner),
                cancel: symbols.fresh(self.owner),
            }
        };
        let signature = self
            .representations
            .signature(layout.executor)
            .ok_or_else(|| control_error(span, "control executor has no predeclared signature"))?
            .clone();
        self.generated_signatures
            .borrow_mut()
            .insert(functions.executor, signature);
        let open_signature = self
            .representations
            .signature(layout.open_executor)
            .ok_or_else(|| {
                control_error(span, "open control executor has no predeclared signature")
            })?
            .clone();
        self.generated_signatures
            .borrow_mut()
            .insert(functions.open_executor, open_signature);
        for (symbol, id) in [
            (functions.start, layout.start),
            (functions.check, layout.boundary_check),
            (functions.close, layout.boundary_check),
            (functions.once, layout.boundary_check),
            (functions.cancel, layout.boundary_check),
        ] {
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
        let saved = resume.capture(receiver, 0, &mut assignments);
        let boundary = resume.capture(receiver, 1, &mut assignments);
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
        let executor = Machine::new(self.child_lowerer(), layout, functions, span).executor(false);
        self.verify_synthesized(&executor)?;
        self.generated.push(executor);
        let open = Machine::new(self.child_lowerer(), layout, functions, span).executor(true);
        self.verify_synthesized(&open)?;
        self.generated.push(open);
        let forward = Machine::new(self.child_lowerer(), layout, functions, span).forward();
        self.verify_synthesized(&forward)?;
        self.generated.push(forward);
        for (case, symbol) in [(0, functions.project_left), (1, functions.project_right)] {
            let selector =
                Machine::new(self.child_lowerer(), layout, functions, span).selector(case, symbol);
            self.verify_synthesized(&selector)?;
            self.generated.push(selector);
        }
        for function in [
            Machine::new(self.child_lowerer(), layout, functions, span).start_boundary(),
            Machine::new(self.child_lowerer(), layout, functions, span).boundary_check(),
            Machine::new(self.child_lowerer(), layout, functions, span).boundary_close(),
            Machine::new(self.child_lowerer(), layout, functions, span).owned(),
            Machine::new(self.child_lowerer(), layout, functions, span).cancel(),
        ] {
            self.verify_synthesized(&function)?;
            self.generated.push(function);
        }
        *self.control_functions.borrow_mut() = Some(functions);
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

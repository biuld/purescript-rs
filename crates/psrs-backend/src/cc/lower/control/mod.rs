//! Lower the inert control surface to a persistent continuation machine.

use super::FunctionLowerer;
use crate::BackendError;
use crate::cc::layout::control::ControlLayout;
use crate::cc::payload::erased_shape;
use crate::cc::{Assignment, AssignmentKind, ValueId, ValueShape};
use psrs_core::Expr;
use psrs_hir::Intrinsic;
use psrs_span::TextRange;

pub(crate) use plan::{Family, Shape};

mod builder;
mod executor;
mod fields;
mod foreign;
mod lifetime;
mod operands;
mod owned;
mod plan;
mod planning;
mod replay;
mod scope;
use builder::Machine;
use fields::ControlEmitter;
pub(in crate::cc) use planning::ControlFunctions;
pub(in crate::cc) use planning::plan_control_helpers;
pub(crate) use planning::{plan_external_targets, plan_shape_for};

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
            crate::cc::layout::control::lookup(self.module, self.representations, &proof)
                .ok_or_else(|| {
                    control_error(
                        expression.span,
                        "control use has no predeclared checked contract",
                    )
                })?;
        let fields = self.lower_control_operands(contract, arguments, assignments)?;
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
            if ty != builder::repr(layout.owner) {
                return Err(control_error(
                    expression.span,
                    "resumption result has no nominal owner shape",
                ));
            }
            return Ok((result, result));
        }
        if intrinsic == Intrinsic::ControlRun {
            let functions = self.ensure_control_functions(layout, expression.span)?;
            let result = self.control_call(
                functions.run,
                layout.run,
                fields,
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
            Intrinsic::ControlReturn => crate::control::schema::ComputationCase::Return,
            Intrinsic::ControlPerform => crate::control::schema::ComputationCase::Perform,
            Intrinsic::ControlBind => crate::control::schema::ComputationCase::Bind,
            Intrinsic::ControlHandle => crate::control::schema::ComputationCase::Handle,
            Intrinsic::ControlResume => crate::control::schema::ComputationCase::OwnedResume,
            _ => return Err(control_error(expression.span, "not a control primitive")),
        }
        .tag();
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
        let mut resume = Machine::new(self.representations, layout, functions, span);
        let receiver = resume.fresh_value(super::lambda::closure_value_type());
        let value = resume.fresh_value(erased_shape());
        let params = [receiver, value];
        let program = plan::build(plan::Program::RawResume);
        let mut assignments = Vec::new();
        let result = resume.expand(&program, &params, &mut assignments);
        let representations = resume.representations;
        let resume = resume.finish(
            functions.resume,
            "control_resume",
            vec![receiver, value],
            assignments,
            result,
        );
        plan::check_boundary(&resume, representations, &program)
            .expect("raw resume boundary check must pass");
        self.verify_synthesized(&resume)?;
        self.generated.push(resume);
        let executor = Machine::new(self.representations, layout, functions, span).executor();
        self.verify_synthesized(&executor)?;
        self.generated.push(executor);
        for function in [
            Machine::new(self.representations, layout, functions, span).run(),
            Machine::new(self.representations, layout, functions, span).clone_sequence(),
            Machine::new(self.representations, layout, functions, span).rebuild_sequence(),
            Machine::new(self.representations, layout, functions, span).start_boundary(),
            Machine::new(self.representations, layout, functions, span).boundary_check(),
            Machine::new(self.representations, layout, functions, span).boundary_close(),
            Machine::new(self.representations, layout, functions, span).owned(),
            Machine::new(self.representations, layout, functions, span).cancel(),
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

/// Re-check a generated machine body against its checked plan. This is the
/// replaced recognizer's entry point: it runs on the emitted CC at the module
/// verification boundary and rejects any body whose ordered operations,
/// operands, dispatch or returned value disagree with the construction plan.
pub(crate) fn verify_boundary(
    function: &crate::cc::Function,
    table: &crate::cc::RepresentationTable,
) -> Result<(), &'static str> {
    match planning::plan_for_symbol(function.symbol, table) {
        Some(plan) => plan::check_boundary(function, table, &plan),
        None => Ok(()),
    }
}

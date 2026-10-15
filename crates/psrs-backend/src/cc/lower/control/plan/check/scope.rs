//! Scope and logical storage roles are checked before allocating CC values.
use super::super::ir::Family;
use super::*;
use std::collections::HashMap;

#[derive(Clone, Copy)]
pub(super) struct Info {
    pub kind: FieldKind,
    pub tag: Option<u32>,
}

impl Info {
    pub(super) fn new(kind: FieldKind) -> Self {
        Self { kind, tag: None }
    }
    pub(super) fn node(case: Case) -> Self {
        Self {
            kind: case.kind(),
            tag: Some(case.tag()),
        }
    }
}

#[derive(Clone)]
pub(super) struct Scope {
    pub program: Program,
    pub temps: Vec<Info>,
    pub refinements: HashMap<Operand, (Family, u32)>,
}

impl Scope {
    pub(super) fn new(program: Program) -> Self {
        Self {
            program,
            temps: Vec::new(),
            refinements: HashMap::new(),
        }
    }

    fn key(&self, operand: &Operand) -> Checked<Operand> {
        Ok(match operand {
            Operand::State(state) => {
                if self.program != Program::Executor {
                    return Err("helper plan cannot read executor state");
                }
                Operand::Param(state_index(*state))
            }
            _ => operand.clone(),
        })
    }

    pub(super) fn info(&self, operand: &Operand) -> Checked<Info> {
        let key = self.key(operand)?;
        let mut info = match key {
            Operand::Param(index) => self.program.parameters().get(index).copied().map(Info::new),
            Operand::Temp(index) => self.temps.get(index).copied(),
            Operand::State(_) => unreachable!(),
        }
        .ok_or("machine plan operand is outside its lexical scope")?;
        if let Some((_, tag)) = self.refinements.get(&key) {
            info.tag = Some(*tag);
        }
        Ok(info)
    }

    pub(super) fn require(&self, operand: &Operand, kind: FieldKind) -> Checked<()> {
        let actual = self.info(operand)?.kind;
        // Payload erasure accepts references. Callback results may recover a
        // computation, callable or result pair at their checked protocol use.
        if actual == kind
            || kind == FieldKind::Payload
            || (actual == FieldKind::Payload
                && matches!(
                    kind,
                    FieldKind::Computation | FieldKind::Callable | FieldKind::ResultPair
                ))
        {
            Ok(())
        } else {
            Err("machine plan operand has the wrong logical storage role")
        }
    }

    pub(super) fn arguments(&self, args: &[Operand], expected: &[FieldKind]) -> Checked<()> {
        if args.len() != expected.len() {
            return Err("machine plan call or allocation has the wrong arity");
        }
        for (arg, kind) in args.iter().zip(expected) {
            self.require(arg, *kind)?;
        }
        Ok(())
    }

    pub(super) fn projection(&self, source: &Operand, case: Case, field: u32) -> Checked<Info> {
        self.require(source, case.kind())?;
        let info = self.info(source)?;
        let singleton = case == Case::Owner(OwnerCase::OneShot);
        if !singleton && info.tag != Some(case.tag()) {
            return Err("machine plan projection lacks its own case refinement");
        }
        let kind = *case
            .fields()
            .get(field as usize)
            .ok_or("machine plan projection names an invalid field")?;
        Ok(Info::new(kind))
    }

    pub(super) fn exec(mut self, exec: &Exec) -> Checked<()> {
        match exec {
            Exec::Steps { ops, next } => {
                for op in ops {
                    let info = self.op(op)?;
                    self.temps.push(info);
                }
                self.exec(next)
            }
            Exec::Switch { source, case, arms } => {
                self.require(source, case.kind())?;
                let key = self.key(source)?;
                for (tag, arm) in arms {
                    let mut branch = self.clone();
                    branch
                        .refinements
                        .insert(key.clone(), (case.family(), *tag));
                    branch.exec(arm)?;
                }
                Ok(())
            }
            Exec::Advance {
                state,
                handler,
                computation,
                sequence,
                extent,
                context,
            } => {
                if self.program != Program::Executor {
                    return Err("only the executor may advance machine state");
                }
                self.arguments(
                    &[
                        state.clone(),
                        handler.clone(),
                        computation.clone(),
                        sequence.clone(),
                        extent.clone(),
                        context.clone(),
                    ],
                    crate::cc::ControlHelperRole::Executor.parameters(),
                )
            }
            Exec::Tail { role, args } => {
                self.arguments(args, role.parameters())?;
                if role.result() != self.program.result() {
                    return Err("machine plan tail call has the wrong result role");
                }
                Ok(())
            }
            Exec::Publish { result } => self.require(result, FieldKind::ResultPair),
            Exec::Return { value } => self.require(value, self.program.result()),
            Exec::Reject { .. } => Ok(()),
        }
    }
}

//! The complete normal lifetime of a source Run, before payload recovery.
use super::ir::{Case, Cursor, Exec, Op, Operand, steps};
use crate::cc::ControlHelperRole as R;
use crate::control::schema::ContextCase;

pub(super) fn program() -> Exec {
    let mut cursor = Cursor::new();
    let mut ops = Vec::new();
    let empty = cursor.bind(&mut ops, Op::EmptySequence);
    let root = cursor.bind(
        &mut ops,
        Op::Helper {
            role: R::Start,
            args: vec![],
        },
    );
    let context = cursor.bind(
        &mut ops,
        Op::Node {
            case: Case::Context(ContextCase::Root),
            fields: vec![root.clone()],
        },
    );
    let result = cursor.bind(
        &mut ops,
        Op::Helper {
            role: R::Executor,
            args: vec![
                Operand::Param(0),
                Operand::Param(1),
                Operand::Param(2),
                empty,
                root.clone(),
                context,
            ],
        },
    );
    cursor.bind(&mut ops, Op::CloseBoundary { extent: root });
    steps(ops, Exec::Return { value: result })
}

impl super::super::builder::Machine<'_> {
    pub(in crate::cc::lower::control) fn run(mut self) -> crate::cc::Function {
        use super::super::fields::ControlEmitter;
        let parameters = R::Run
            .parameters()
            .iter()
            .map(|kind| {
                let shape = self
                    .representations
                    .control_schema()
                    .unwrap()
                    .shape(*kind, self.layout.continuation);
                self.fresh_value(shape)
            })
            .collect::<Vec<_>>();
        let plan = super::build(super::Program::Run);
        let mut body = Vec::new();
        let result = self.expand(&plan, &parameters, &mut body);
        let table = self.representations;
        let symbol = self.functions.run;
        let function = self.finish(symbol, "control_run", parameters, body, result);
        super::check_boundary(&function, table, &plan)
            .expect("Run construction must match its checked plan");
        function
    }
}

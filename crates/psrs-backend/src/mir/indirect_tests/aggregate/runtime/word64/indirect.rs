//! Exact words must also survive the canonical indirect-parameter threshold.
use super::super::super::fixtures::{reference, span};
use super::super::super::scalars::wide_scalar_fixture;
use super::super::component;
use crate::cc::{
    self, Assignment, AssignmentKind as A, ReprId, Representation, Signature, ValueDecl, ValueShape,
};
use crate::types::ValueId;
use wit_parser::Resolve;

#[test]
fn exact_word64_indirect_parameters_preserve_all_words() {
    let (mut module, mut bindings, _) = wide_scalar_fixture();
    let mut resolve = Resolve::default();
    let parameters = (0..17)
        .map(|i| format!("a{i}: u64"))
        .collect::<Vec<_>>()
        .join(", ");
    resolve
        .push_str(
            "word64-indirect.wit",
            &format!(
                "package wasi:io@0.2.12; interface streams {{ get: func({parameters}) -> u64; }}"
            ),
        )
        .unwrap();
    module.representations = cc::RepresentationTable::from_parts(
        vec![Representation::Product {
            fields: vec![ValueShape::Integer; 2],
        }],
        Vec::new(),
        Default::default(),
    );
    module.representations.product_labels = [(ReprId(0), vec!["high".into(), "low".into()])]
        .into_iter()
        .collect();
    module.externals[0].signature = Some(Signature {
        parameters: vec![reference(0); 17],
        result: reference(0),
    });
    bindings.imports[0].function = "get".into();
    let symbol = module.externals[0].symbol;
    let function = &mut module.functions[0];
    function.values = [
        ValueShape::Integer,
        ValueShape::Integer,
        reference(0),
        reference(0),
        ValueShape::Integer,
    ]
    .into_iter()
    .enumerate()
    .map(|(id, ty)| ValueDecl {
        id: ValueId(id as u32),
        ty,
    })
    .collect();
    let assignment = |id, kind| Assignment {
        destination: ValueId(id),
        kind,
        span: span(),
    };
    function.assignments = vec![
        assignment(0, A::Constant(0x12345678)),
        assignment(1, A::Constant(-1)),
        assignment(
            2,
            A::ProductNew {
                destination: ValueId(2),
                representation: ReprId(0),
                arguments: vec![ValueId(0), ValueId(1)],
            },
        ),
        assignment(
            3,
            A::DirectCall {
                contract: None,
                function: symbol,
                arguments: vec![ValueId(2); 17],
            },
        ),
        assignment(
            4,
            A::ProductGet {
                destination: ValueId(4),
                representation: ReprId(0),
                field: 0,
                value: ValueId(3),
            },
        ),
    ];
    function.result = ValueId(4);
    let checks = (0..17).map(|i| format!(
        "local.get 0 i64.load offset={} i64.const 0x12345678ffffffff i64.ne if unreachable end", i*8))
        .collect::<Vec<_>>().join("\n");
    component::execute(
        "word64-indirect",
        module,
        bindings,
        resolve,
        &format!(
            r#"(func (export "get") (param i32) (result i64) {checks} i64.const 0x8765432100000001)"#
        ),
        0x87654321u32 as i32,
    );
}

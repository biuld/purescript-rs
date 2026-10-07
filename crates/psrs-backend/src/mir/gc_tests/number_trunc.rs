use super::*;

#[test]
fn optimized_and_unoptimized_number_trunc_preserve_zero_sign_and_range() {
    use crate::cc::{BinaryOp, UnaryOp, ValueDecl};
    use ValueShape::{Boolean as B, Integer as I, Number as N};
    let symbol = SymbolId::new(ModuleId(0), 0);
    let unary = |destination, op, value| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Unary {
            op,
            value: ValueId(value),
        },
        span: span(),
    };
    let number = |destination, value: f64| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::NumberConstant(value.to_string()),
        span: span(),
    };
    let binary = |destination, op, left, right| Assignment {
        destination: ValueId(destination),
        kind: AssignmentKind::Primitive {
            op,
            left: ValueId(left),
            right: ValueId(right),
        },
        span: span(),
    };
    let module = CcModule {
        name: "NumberTruncDifferential".into(),
        externals: Vec::new(),
        representations: RepresentationTable::default(),
        functions: vec![CcFunction {
            symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values: [N, N, N, N, N, B, N, N, N, B, B, I, I, I]
                .into_iter()
                .enumerate()
                .map(|(id, ty)| ValueDecl {
                    id: ValueId(id as u32),
                    ty,
                })
                .collect(),
            assignments: vec![
                number(0, -0.9),
                unary(1, UnaryOp::NumberTrunc, 0),
                number(2, 1.0),
                binary(3, BinaryOp::NumberDiv, 2, 1),
                number(4, 0.0),
                binary(5, BinaryOp::NumberLt, 3, 4),
                number(6, 4294967296.5),
                unary(7, UnaryOp::NumberTrunc, 6),
                number(8, 4294967296.0),
                binary(9, BinaryOp::NumberEq, 7, 8),
                binary(10, BinaryOp::BooleanAnd, 5, 9),
                unary(11, UnaryOp::BooleanToInt, 10),
                Assignment {
                    destination: ValueId(12),
                    kind: AssignmentKind::Constant(41),
                    span: span(),
                },
                binary(13, BinaryOp::IntAdd, 11, 12),
            ],
            result: ValueId(13),
            result_type: I,
            span: span(),
        }],
        entry: Some(symbol),
        span: span(),
    };
    let target = crate::TargetCapabilities::default();
    let (mir, _) = crate::mir::lower_module_with_capabilities(module, target)
        .expect("Number truncation should lower to MIR");
    run_gc(&mir, 42);
    let optimized = crate::mir::opt::optimize(mir, target).expect("valid optimization");
    run_gc(&optimized, 42);
}

use super::*;

#[test]
fn decimal_conversion_checks_both_shapes_before_abi_erasure() {
    let input = super::super::super::ValueId(0);
    let output = super::super::super::ValueId(1);
    for (operand, result, valid) in [
        (ValueShape::String, ValueShape::Number, true),
        (ValueShape::Integer, ValueShape::Number, false),
        (ValueShape::Number, ValueShape::Number, false),
        (ValueShape::String, ValueShape::Integer, false),
    ] {
        let function = Function {
            symbol: symbol(0),
            name: "decimal".into(),
            parameters: vec![input],
            values: vec![
                ValueDecl {
                    id: input,
                    ty: operand,
                },
                ValueDecl {
                    id: output,
                    ty: result,
                },
            ],
            assignments: vec![Assignment {
                destination: output,
                kind: AssignmentKind::RuntimeCall {
                    intrinsic: psrs_hir::Intrinsic::NumberFromDecimal,
                    arguments: vec![input],
                },
                span: TextRange::new(0, 1),
            }],
            result: output,
            result_type: result,
            span: TextRange::new(0, 1),
        };
        assert_eq!(
            verify_function(&function, &HashMap::new(), &table()).is_ok(),
            valid
        );
    }
}

#[test]
fn runtime_calls_reject_wrong_arity_and_non_artifact_identities() {
    use psrs_hir::Intrinsic;
    let input = super::super::super::ValueId(0);
    let output = super::super::super::ValueId(1);
    for (intrinsic, arguments) in [
        (Intrinsic::NumberFromDecimal, vec![]),
        (Intrinsic::NumberFromDecimal, vec![input, input]),
        (Intrinsic::NumberAbs, vec![input]),
        (Intrinsic::Undefined, vec![]),
    ] {
        let function = Function {
            symbol: symbol(0),
            name: "invalid_runtime_call".into(),
            parameters: vec![input],
            values: vec![
                ValueDecl {
                    id: input,
                    ty: ValueShape::String,
                },
                ValueDecl {
                    id: output,
                    ty: ValueShape::Number,
                },
            ],
            assignments: vec![Assignment {
                destination: output,
                kind: AssignmentKind::RuntimeCall {
                    intrinsic,
                    arguments,
                },
                span: TextRange::new(0, 1),
            }],
            result: output,
            result_type: ValueShape::Number,
            span: TextRange::new(0, 1),
        };
        assert!(verify_function(&function, &HashMap::new(), &table()).is_err());
    }
}

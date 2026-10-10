use super::*;
use crate::cc::{AssignmentKind, ValueShape};
use crate::types::{ValueDecl, ValueId, ValueType};

fn fixture() -> (Arc<cc::Module>, Function) {
    let (mut source, mut function) = super::tests::fixture();
    let logical = Arc::make_mut(&mut source);
    let signature = logical.externals[0].signature.as_mut().unwrap();
    signature
        .parameters
        .splice(0..0, [ValueShape::Integer, ValueShape::Integer]);
    let body = &mut logical.functions[0];
    body.parameters
        .splice(0..0, [ValueId(4), ValueId(5), ValueId(6)]);
    function.parameters = vec![ValueId(4), ValueId(5), ValueId(6)];
    for id in [6, 5, 4] {
        body.values.insert(
            0,
            crate::cc::ValueDecl {
                id: ValueId(id),
                ty: ValueShape::Integer,
            },
        );
        function.values.insert(
            0,
            ValueDecl {
                id: ValueId(id),
                ty: ValueType::I32,
            },
        );
    }
    for (index, prefix) in [(0, [ValueId(4), ValueId(5)]), (2, [ValueId(5), ValueId(6)])] {
        let AssignmentKind::DirectCall { arguments, .. } = &mut body.assignments[index].kind else {
            panic!("fixture must have a direct call");
        };
        arguments.splice(0..0, prefix);
        let Instruction::Call { arguments, .. } = &mut function.blocks[0].instructions[index / 2]
        else {
            panic!("fixture must have a lowered call");
        };
        *arguments = prefix.to_vec();
    }
    (source, function)
}

#[test]
fn same_typed_operand_substitution_invalidates_the_dependency_certificate() {
    let (source, mut function) = fixture();
    let certificate = DependencyFlow::checked(source, &function).unwrap();
    let Instruction::Call { arguments, .. } = &mut function.blocks[0].instructions[0] else {
        panic!("fixture must have a lowered call");
    };
    arguments[0] = ValueId(6);
    assert!(certificate.verify(&function).is_err());
}

#[test]
fn changed_operand_order_or_arity_invalidates_the_dependency_certificate() {
    for change in 0..3 {
        let (source, mut function) = fixture();
        let certificate = DependencyFlow::checked(source, &function).unwrap();
        let Instruction::Call { arguments, .. } = &mut function.blocks[0].instructions[1] else {
            panic!("fixture must have a lowered call");
        };
        match change {
            0 => arguments.swap(0, 1),
            1 => {
                arguments.pop();
            }
            _ => arguments.push(ValueId(4)),
        }
        assert!(certificate.verify(&function).is_err(), "change {change}");
    }
}

#[test]
fn indirect_calls_preserve_ordinary_operands_after_state_erasure() {
    let (source, _) = fixture();
    let mut source = (*source).clone();
    let signature = source.externals[0].signature.clone().unwrap();
    source.externals.clear();
    let signature = source.representations.add_signature(signature);
    let body = &mut source.functions[0];
    body.parameters.insert(3, ValueId(7));
    body.values.insert(
        3,
        crate::cc::ValueDecl {
            id: ValueId(7),
            ty: ValueShape::Reference(crate::cc::Reference {
                nullable: false,
                heap: crate::cc::RefShape::Closure(signature),
            }),
        },
    );
    for index in [0, 2] {
        let AssignmentKind::DirectCall { arguments, .. } = &body.assignments[index].kind else {
            panic!("fixture must have a direct call");
        };
        body.assignments[index].kind = AssignmentKind::IndirectCall {
            function: ValueId(7),
            signature,
            arguments: arguments.clone(),
        };
    }
    let (mut module, _) = crate::mir::lower_module(source).unwrap();
    crate::mir::verify_module(&module).unwrap();
    let function = &mut module.functions[0];
    let certificate = function.state.clone().unwrap();
    let Instruction::ClosureCall { arguments, .. } = &mut function.blocks[0].instructions[0] else {
        panic!("fixture must lower to a closure call");
    };
    arguments[0] = ValueId(6);
    assert!(certificate.verify(function).is_err());
    assert!(
        crate::mir::verify_module(&module)
            .unwrap_err()
            .iter()
            .any(|error| error.message.contains("producer or operands"))
    );
}

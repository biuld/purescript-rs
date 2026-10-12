//! Transparent aliases preserve the caller identity at source context entry.
use super::{R, SOURCE, binding};
use psrs_backend::cc::{Assignment, AssignmentKind as A, ValueId, ValueShape};
use psrs_backend::mir::Instruction as I;

fn alias_fields(body: &mut [Assignment], original: ValueId, alias: ValueId) -> bool {
    for assignment in body {
        match &mut assignment.kind {
            A::VariantNew {
                case: 1, fields, ..
            } if fields.len() == 6 && fields[4] == original => {
                fields[4] = alias;
                return true;
            }
            A::TagSwitch { cases, .. } => {
                for case in cases {
                    if alias_fields(&mut case.assignments, original, alias) {
                        return true;
                    }
                }
            }
            _ => {}
        }
    }
    false
}

#[test]
fn cc_source_context_accepts_a_reference_alias_of_its_caller() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let symbol = binding(&input.cc.representations, R::Executor).0;
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let original = function.parameters[5];
    let ty = function
        .values
        .iter()
        .find(|value| value.id == original)
        .unwrap()
        .ty;
    let ValueShape::Reference(reference) = ty else {
        unreachable!()
    };
    let alias = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    function
        .values
        .push(psrs_backend::cc::ValueDecl { id: alias, ty });
    function.assignments.insert(
        0,
        Assignment {
            destination: alias,
            kind: A::RepresentationCast {
                destination: alias,
                value: original,
                reference,
            },
            span: function.span,
        },
    );
    assert!(alias_fields(&mut function.assignments, original, alias));
    psrs_backend::cc::verify_module(&input.cc).unwrap();
    psrs_backend::compile_cc(input).unwrap();
}

#[test]
fn mir_source_context_accepts_a_copy_of_its_caller() {
    let mut stages = crate::compile_main_stages(SOURCE).unwrap();
    let symbol = binding(&stages.cc.representations, R::Executor).0;
    let function = stages
        .mir
        .functions
        .iter_mut()
        .find(|function| function.symbol == symbol)
        .unwrap();
    let alias = ValueId(
        function
            .values
            .iter()
            .map(|value| value.id.0)
            .max()
            .unwrap()
            + 1,
    );
    let mut changed = false;
    for block in &mut function.blocks {
        if let Some(index) = block.instructions.iter().position(|instruction| matches!(instruction, I::StructNew { arguments, .. } if arguments.len() == 7)) {
            let I::StructNew { arguments, .. } = &mut block.instructions[index] else { unreachable!() };
            let original = arguments[5];
            let ty = function.values.iter().find(|value| value.id == original).unwrap().ty;
            function.values.push(psrs_backend::types::ValueDecl { id: alias, ty });
            arguments[5] = alias;
            block.instructions.insert(index, I::Copy { destination: alias, value: original, span: function.span });
            changed = true;
            break;
        }
    }
    assert!(changed);
    psrs_backend::mir::verify_module(&stages.mir).unwrap();
}

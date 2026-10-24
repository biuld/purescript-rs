//! Independent host observations of outgoing discriminants and linear arrays.

use super::super::collections::non_byte_list_fixture;
use super::super::fixtures::{erased, parameter_fixture, reference, span};
use super::component;
use crate::cc::{
    self, Assignment, AssignmentKind as A, ReprId, Representation, ValueDecl, ValueShape,
    VariantCase,
};
use crate::types::ValueId;
use wit_parser::Resolve;

fn variants(wit: &str, swapped: bool) {
    for guest_tag in 0..2 {
        let (mut module, bindings, _) = parameter_fixture();
        let mut resolve = Resolve::default();
        resolve.push_str("parameter.wit", wit).unwrap();
        module.representations = cc::RepresentationTable::from_parts(
            vec![
                Representation::Box {
                    value: ValueShape::Integer,
                },
                Representation::Variant {
                    cases: (0..2)
                        .map(|tag| VariantCase {
                            tag,
                            fields: vec![erased()],
                        })
                        .collect(),
                },
            ],
            Vec::new(),
            Default::default(),
        );
        let A::VariantNew { case, .. } = &mut module.functions[0].assignments[2].kind else {
            unreachable!()
        };
        *case = guest_tag;
        let canonical_tag = if swapped { 1 - guest_tag } else { guest_tag };
        component::execute(
            "variant-parameter",
            module,
            bindings,
            resolve,
            &format!(
                r#"(func (export "take") (param i32 i32)
                local.get 0 i32.const {canonical_tag} i32.ne if unreachable end
                local.get 1 i32.const 42 i32.ne if unreachable end)"#
            ),
            0,
        );
    }
}

#[test]
fn result_parameters_execute_both_source_constructors_with_swapped_tags() {
    variants(
        "package wasi:io@0.2.12; interface streams { take: func(value: result<s32, s32>); }",
        true,
    );
}

#[test]
fn variant_parameters_execute_both_constructors_in_wit_order() {
    variants(
        "package wasi:io@0.2.12; interface streams { variant item { a(s32), b(s32) } take: func(value: item); }",
        false,
    );
}

#[test]
fn list_parameter_executes_with_length_and_signed_elements() {
    let (mut module, mut bindings, _) = non_byte_list_fixture();
    let mut resolve = Resolve::default();
    resolve
        .push_str(
            "list-parameter.wit",
            "package wasi:io@0.2.12; interface streams { take: func(value: list<s32>); }",
        )
        .unwrap();
    let signature = module.externals[0].signature.as_mut().unwrap();
    signature.parameters = vec![reference(1)];
    signature.result = ValueShape::Integer;
    let symbol = module.externals[0].symbol;
    bindings.imports[0].function = "take".into();
    let function = &mut module.functions[0];
    function.values.extend((4..7).map(|id| ValueDecl {
        id: ValueId(id),
        ty: ValueShape::Integer,
    }));
    function.assignments = [7, 42, -5]
        .into_iter()
        .enumerate()
        .map(|(index, value)| Assignment {
            destination: ValueId(index as u32 + 4),
            kind: A::Constant(value),
            span: span(),
        })
        .collect();
    function.assignments.push(Assignment {
        destination: ValueId(2),
        kind: A::ArrayNew {
            destination: ValueId(2),
            representation: ReprId(1),
            elements: vec![ValueId(4), ValueId(5), ValueId(6)],
        },
        span: span(),
    });
    function.assignments.push(Assignment {
        destination: ValueId(3),
        kind: A::DirectCall {
            contract: None,
            function: symbol,
            arguments: vec![ValueId(2)],
        },
        span: span(),
    });
    component::execute(
        "list-parameter",
        module,
        bindings,
        resolve,
        r#"(func (export "take") (param i32 i32)
            local.get 1 i32.const 3 i32.ne if unreachable end
            local.get 0 i32.load i32.const 7 i32.ne if unreachable end
            local.get 0 i32.load offset=4 i32.const 42 i32.ne if unreachable end
            local.get 0 i32.load offset=8 i32.const -5 i32.ne if unreachable end)"#,
        0,
    );
}

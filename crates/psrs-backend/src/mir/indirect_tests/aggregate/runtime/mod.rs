//! Execute synthesized ABI fixtures against an independent canonical host.

mod collections;
mod component;
mod parameters;
mod results;
mod word64;

use super::fixtures::parameter_fixture;

#[test]
fn option_parameter_executes_with_its_tag_and_payload() {
    for value in [None, Some(-37), Some(42)] {
        let (mut module, bindings, resolve) = parameter_fixture();
        module.functions[0].assignments[0].kind =
            crate::cc::AssignmentKind::Constant(value.unwrap_or(0));
        if value.is_none() {
            let crate::cc::AssignmentKind::VariantNew { case, fields, .. } =
                &mut module.functions[0].assignments[2].kind
            else {
                unreachable!()
            };
            *case = 0;
            fields.clear();
        }
        component::execute(
            "option-parameter",
            module,
            bindings,
            resolve,
            &format!(
                r#"(func (export "take") (param i32 i32)
                  local.get 0 i32.const {} i32.ne if unreachable end
                  local.get 1 i32.const {} i32.ne if unreachable end)"#,
                i32::from(value.is_some()),
                value.unwrap_or(0)
            ),
            0,
        );
    }
}

#[test]
fn record_parameter_executes_with_field_order_and_boolean_payload() {
    let (module, bindings, resolve) = super::nested::nested_record_parameter_fixture();
    component::execute(
        "record-parameter",
        module,
        bindings,
        resolve,
        r#"(func (export "take") (param i32 i32 i32)
            local.get 0 i32.const 1 i32.ne if unreachable end
            local.get 1 i32.const 42 i32.ne if unreachable end
            local.get 2 i32.const 1 i32.ne if unreachable end)"#,
        0,
    );
}

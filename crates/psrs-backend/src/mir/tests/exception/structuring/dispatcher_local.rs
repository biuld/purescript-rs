//! EH-10: reducible try-bearing input structures without the irreducible
//! dispatcher.

use super::*;

// --------------------------------------------------------- dispatcher local

/// Reducible try-bearing input structures without the irreducible
/// dispatcher: no extra state local is allocated beyond the lowered values.
#[test]
fn does_not_allocate_a_dispatcher_local_on_reducible_input() {
    let mir = module(
        function(nested_blocks(), &nested_values(), vec![], 0),
        vec![tag(0, Vec::new())],
    );
    let mut wasi = crate::abi::WasiRegistry::load_with_capabilities(enabled_target())
        .expect("the vendored WASI WIT should load");
    let context = crate::linking::default_context().expect("default target context");
    let link = crate::linking::plan_for_module(&context, &mir, &mut wasi, enabled_target())
        .expect("checked target plan");
    let wasm = crate::wasm::lower_module_with_plan(&mir, &mut wasi, enabled_target(), &link)
        .expect("the MIR should lower to Wasm");
    let non_parameters = nested_values().len();
    assert_eq!(
        wasm.functions[0].locals.len(),
        non_parameters,
        "no dispatcher local"
    );
}

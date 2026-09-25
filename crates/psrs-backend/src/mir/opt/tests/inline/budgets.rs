use super::*;

#[test]
fn inlining_obeys_the_module_wide_call_site_budget() {
    let first_symbol = SymbolId::new(ModuleId(0), 0);
    let second_symbol = SymbolId::new(ModuleId(0), 1);
    let callee_symbol = SymbolId::new(ModuleId(0), 2);
    let first_caller = caller_with_calls(FunctionId(0), first_symbol, callee_symbol, 70);
    let second_caller = caller_with_calls(FunctionId(1), second_symbol, callee_symbol, 70);
    let callee = constant_callee(FunctionId(2), callee_symbol, vec![7]);

    let optimized = optimize(
        module(
            vec![first_caller, second_caller, callee],
            Vec::new(),
            first_symbol,
        ),
        TargetCapabilities::default(),
    )
    .expect("budgeted inlining must preserve valid MIR");

    assert_eq!(call_count(&optimized.functions[0]), 0);
    assert_eq!(call_count(&optimized.functions[1]), 12);
}

#[test]
fn inlining_obeys_the_total_code_growth_budget() {
    let caller_symbol = SymbolId::new(ModuleId(0), 0);
    let mixed_caller_symbol = SymbolId::new(ModuleId(0), 1);
    let large_symbol = SymbolId::new(ModuleId(0), 2);
    let medium_symbol = SymbolId::new(ModuleId(0), 3);
    let oversized_symbol = SymbolId::new(ModuleId(0), 4);
    let small_symbol = SymbolId::new(ModuleId(0), 5);
    let caller = caller_with_calls(FunctionId(0), caller_symbol, large_symbol, 63);
    let mixed_caller = caller_with_targets(
        FunctionId(1),
        mixed_caller_symbol,
        &[medium_symbol, oversized_symbol, small_symbol],
    );
    let large_callee = constant_callee(FunctionId(2), large_symbol, (0_i32..16).collect());
    let medium_callee = constant_callee(FunctionId(3), medium_symbol, (0_i32..12).collect());
    let oversized_callee = constant_callee(FunctionId(4), oversized_symbol, (0_i32..8).collect());
    let small_callee = constant_callee(FunctionId(5), small_symbol, (0_i32..4).collect());

    let optimized = optimize(
        module(
            vec![
                caller,
                mixed_caller,
                large_callee,
                medium_callee,
                oversized_callee,
                small_callee,
            ],
            Vec::new(),
            caller_symbol,
        ),
        TargetCapabilities::default(),
    )
    .expect("growth-bounded inlining must preserve valid MIR");

    let remaining_calls = optimized.functions[1].blocks[0]
        .instructions
        .iter()
        .filter_map(|instruction| match instruction {
            Instruction::Call { function, .. } => Some(*function),
            _ => None,
        })
        .collect::<Vec<_>>();
    assert_eq!(remaining_calls, vec![oversized_symbol]);
}

#[test]
fn inlining_keeps_callees_over_the_per_callee_limit() {
    let caller_symbol = SymbolId::new(ModuleId(0), 0);
    let callee_symbol = SymbolId::new(ModuleId(0), 1);
    let caller = caller_with_calls(FunctionId(0), caller_symbol, callee_symbol, 1);
    let callee = constant_callee(FunctionId(1), callee_symbol, (0_i32..17).collect());

    let optimized = optimize(
        module(vec![caller, callee], Vec::new(), caller_symbol),
        TargetCapabilities::default(),
    )
    .expect("an ineligible inline candidate must leave valid MIR");

    assert_eq!(call_count(&optimized.functions[0]), 1);
}

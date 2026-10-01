use super::{
    app, boolean, declaration, execute_core_module, global, integer, lambda, local, module,
    push_arrow, push_forall,
};
use psrs_core::{Type, TypeConstructor, TypeId};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeVariableId};

#[test]
fn each_returned_quantifier_starts_a_new_call_arity() {
    let int = TypeId(0);
    let bool_ty = TypeId(1);
    let variable_a = TypeId(2);
    let variable_b = TypeId(3);
    let mut types = vec![
        Type::Constructor(TypeConstructor::Int),
        Type::Constructor(TypeConstructor::Boolean),
        Type::Variable(TypeVariableId(0)),
        Type::Variable(TypeVariableId(1)),
    ];
    let b_body = push_arrow(&mut types, variable_b, variable_b);
    let forall_b = push_forall(&mut types, TypeVariableId(1), b_body);
    let a_body = push_arrow(&mut types, variable_a, forall_b);
    let forall_a = push_forall(&mut types, TypeVariableId(0), a_body);
    let producer_type = push_arrow(&mut types, int, forall_a);
    let module_id = ModuleId(0);
    let producer_symbol = SymbolId::new(module_id, 0);
    let main_symbol = SymbolId::new(module_id, 1);
    let ignored = LocalId(0);
    let first_value = LocalId(1);
    let second_value = LocalId(2);
    let returned = lambda(
        "second",
        second_value,
        variable_b,
        local(second_value, variable_b),
        forall_b,
    );
    let polymorphic_first = lambda("first", first_value, variable_a, returned, forall_a);
    let producer = declaration(
        producer_symbol,
        "makeNested",
        producer_type,
        lambda("ignored", ignored, int, polymorphic_first, producer_type),
    );

    let first_result = app(
        global(producer_symbol, producer_type),
        integer(0, int),
        forall_a,
    );
    let second_result = app(first_result, boolean(true, bool_ty), forall_b);
    let result = app(second_result, integer(42, int), int);
    let main = declaration(main_symbol, "main", int, result);
    let core = module(types, vec![producer, main], main_symbol);

    execute_core_module(
        core,
        "nested forall results must keep separate callable arities",
        42,
    );
}

use super::*;

#[test]
fn source_state_choices_preserve_callback_blocks_through_wasm_encoding() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
choose :: Boolean -> (State RealWorld -> Step RealWorld Int) -> (State RealWorld -> Step RealWorld Int) -> State RealWorld -> Step RealWorld Int
choose flag left right state = if flag then left state else right state
ignore :: (State RealWorld -> Step RealWorld Int) -> Int
ignore action = 42
main :: Int
main = ignore (choose true (\state -> { state: state, value: 40 }) (\state -> { state: state, value: 41 }))
"#;
    let core = lower_program_to_core(&[("Main.purs", source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let target = psrs_backend::TargetCapabilities::default();
    let (mir, mut wasi) =
        psrs_backend::mir::lower_module_with_bindings(input.cc, input.externals, target).unwrap();
    let branch = mir
        .functions
        .iter()
        .find(|function| {
            function.state.as_ref().is_some_and(|flow| {
                flow.graph().blocks.len() == 4
                    && flow
                        .graph()
                        .blocks
                        .iter()
                        .map(|block| block.transitions.len())
                        .sum::<usize>()
                        == 2
            })
        })
        .unwrap();
    let owner = branch.symbol;
    assert_eq!(branch.blocks.len(), 4);
    let mir = psrs_backend::mir::opt::optimize(mir, target).unwrap();
    assert_eq!(
        mir.functions
            .iter()
            .find(|function| function.symbol == owner)
            .unwrap()
            .blocks
            .len(),
        4
    );
    let wasm = psrs_backend::wasm::lower_module(&mir, &mut wasi).unwrap();
    psrs_backend::wasm::encode_module(&wasm).unwrap();
}

#[test]
fn source_state_callbacks_publish_mir_dependencies_without_step_storage() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
apply :: (State RealWorld -> Step RealWorld Int) -> State RealWorld -> Step RealWorld Int
apply action state = action state
ignore :: (State RealWorld -> Step RealWorld Int) -> Int
ignore action = 42
main :: Int
main = ignore (apply (\state -> { state: state, value: 40 }))
"#;
    let core = lower_program_to_core(&[("Main.purs", source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let (mir, mut wasi) = psrs_backend::mir::lower_module_with_bindings(
        input.cc,
        input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .unwrap();
    let projected = mir
        .functions
        .iter()
        .filter(|function| function.state.is_some())
        .collect::<Vec<_>>();
    assert!(!projected.is_empty());
    assert!(projected.iter().any(|function| {
        function.state.as_ref().unwrap().graph().blocks[0]
            .transitions
            .len()
            == 1
    }));
    for function in projected {
        assert_eq!(function.result_type, psrs_backend::types::ValueType::I32);
        assert!(
            !function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|instruction| matches!(
                    instruction,
                    psrs_backend::mir::Instruction::StructNew { .. }
                ))
        );
    }
    psrs_backend::mir::verify_module(&mir).unwrap();
    let mir =
        psrs_backend::mir::opt::optimize(mir, psrs_backend::TargetCapabilities::default()).unwrap();
    assert!(
        mir.functions
            .iter()
            .any(|function| function.state.is_some())
    );
    let wasm = psrs_backend::wasm::lower_module(&mir, &mut wasi).unwrap();
    psrs_backend::wasm::encode_module(&wasm).unwrap();
}

#[test]
fn pure_state_array_construction_preserves_incoming_dependency_aliases() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
produce :: State RealWorld -> Step RealWorld (Array Int)
produce state = { state: state, value: [40] }
action :: State RealWorld -> Step RealWorld Int
action state = case produce state of
  { state: next, value: values } -> { state: state, value: arrayIndex values 0 }
ignore :: (State RealWorld -> Step RealWorld Int) -> Int
ignore callback = 42
main :: Int
main = ignore action
"#;
    let core = lower_program_to_core(&[("Main.purs", source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let flows = psrs_backend::cc::state::check(&input.cc).unwrap();
    let action = flows
        .iter()
        .find(|flow| {
            flow.function.assignments.iter().any(|assignment| {
                matches!(
                    assignment.kind,
                    psrs_backend::cc::AssignmentKind::ArrayGet { .. }
                )
            })
        })
        .expect("the source action retains its array payload read");
    assert!(action.operations.is_empty());
    let (mir, _) = psrs_backend::mir::lower_module_with_bindings(
        input.cc,
        input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .unwrap();
    assert!(mir.functions.iter().any(|function| {
        function.state.is_some()
            && function
                .blocks
                .iter()
                .flat_map(|block| &block.instructions)
                .any(|instruction| {
                    matches!(instruction, psrs_backend::mir::Instruction::ArrayGet { .. })
                })
    }));
}

#[test]
fn ordinary_newtype_pure_and_bind_produce_checked_cc_state_bodies() {
    let source = r#"module Main where
import Prim.State (State, RealWorld)
type Step s a = { state :: State s, value :: a }
newtype Task a = Task (State RealWorld -> Step RealWorld a)
pureTask :: forall a. a -> Task a
pureTask value = Task (\state -> { state: state, value: value })
bindTask :: forall a b. Task a -> (a -> Task b) -> Task b
bindTask (Task first) next = Task (\state0 ->
  case first state0 of
    { state: state1, value: value } ->
      case next value of
        Task second -> second state1)
ignore :: Task Int -> Int
ignore action = 42
main :: Int
main = ignore (bindTask (pureTask 40) (\value -> pureTask (intAdd value 2)))
"#;
    let core = lower_program_to_core(&[("Main.purs", source)]).unwrap();
    let input = psrs_backend::cc::lower_module(core).unwrap();
    let flows = psrs_backend::cc::state::check(&input.cc).unwrap();
    assert!(flows.iter().any(|flow| flow.operations.is_empty()));
    assert!(flows.iter().any(|flow| flow.operations.len() == 2));
    for flow in &flows {
        flow.verify(&input.cc).unwrap();
    }
    let (mir, mut wasi) = psrs_backend::mir::lower_module_with_bindings(
        input.cc,
        input.externals,
        psrs_backend::TargetCapabilities::default(),
    )
    .unwrap();
    assert!(mir.functions.iter().any(|function| {
        function
            .state
            .as_ref()
            .is_some_and(|state| state.graph().blocks[0].transitions.len() == 2)
    }));
    let mir =
        psrs_backend::mir::opt::optimize(mir, psrs_backend::TargetCapabilities::default()).unwrap();
    let wasm = psrs_backend::wasm::lower_module(&mir, &mut wasi).unwrap();
    psrs_backend::wasm::encode_module(&wasm).unwrap();
}

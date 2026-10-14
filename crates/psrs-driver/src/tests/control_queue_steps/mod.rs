//! The root fold and every persistent queue action retain their actual operands.
use super::control_checkpoints::{SOURCE, binding};
use psrs_backend::cc::ControlHelperRole as R;

mod cc;
mod mir;

#[derive(Clone, Copy, Debug)]
enum Mutation {
    HandlerState,
    Request,
    Handler,
    Raw,
    ResultState,
    ResultComputation,
    SourceQueue,
    ReturnState,
    ReturnAnswer,
    Callback,
    Answer,
    Tail,
    EmptyAppend,
    AppendTail,
    AppendRight,
    AppendFirst,
    AppendSecond,
    FinishExtent,
    FinishParent,
    FinishRest,
    QueueFinishRest,
    QueueFinishParent,
    TrapRoot,
    TrapCons,
    ExtraRootCall,
    ExtraQueueCall,
    EagerBind,
}

fn path(mutation: Mutation) -> &'static [i32] {
    use Mutation::*;
    match mutation {
        HandlerState | Request | Handler | Raw | ResultState | ResultComputation | SourceQueue => {
            &[1, 0]
        }
        ReturnState | ReturnAnswer | TrapRoot => &[0, 0, 0],
        Callback | Answer | Tail | TrapCons | ExtraQueueCall => &[0, 1],
        EagerBind => &[2],
        ExtraRootCall => &[1, 0],
        EmptyAppend => &[0, 2, 0],
        AppendTail | AppendRight => &[0, 2, 1],
        AppendFirst | AppendSecond => &[0, 2, 2],
        FinishExtent | FinishParent | FinishRest => &[0, 2, 3],
        QueueFinishRest | QueueFinishParent => &[0, 3],
    }
}

const MUTATIONS: &[Mutation] = &[
    Mutation::HandlerState,
    Mutation::Request,
    Mutation::Handler,
    Mutation::Raw,
    Mutation::ResultState,
    Mutation::ResultComputation,
    Mutation::SourceQueue,
    Mutation::ReturnState,
    Mutation::ReturnAnswer,
    Mutation::Callback,
    Mutation::Answer,
    Mutation::Tail,
    Mutation::EmptyAppend,
    Mutation::AppendTail,
    Mutation::AppendRight,
    Mutation::AppendFirst,
    Mutation::AppendSecond,
    Mutation::FinishExtent,
    Mutation::FinishParent,
    Mutation::FinishRest,
    Mutation::QueueFinishRest,
    Mutation::QueueFinishParent,
    Mutation::TrapRoot,
    Mutation::TrapCons,
    Mutation::ExtraRootCall,
    Mutation::ExtraQueueCall,
    Mutation::EagerBind,
];

#[test]
fn cc_root_and_queue_steps_reject_same_shaped_substitutions() {
    for mutation in MUTATIONS {
        cc::reject(*mutation);
    }
}

#[test]
fn mir_root_and_queue_steps_reject_same_shaped_substitutions() {
    for mutation in MUTATIONS {
        mir::reject(*mutation);
    }
}

#[test]
fn execution_requires_the_placed_machine_schema() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    input.cc.representations.control_schema = None;
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("control helpers have no placed machine schema")),
        "{errors:?}"
    );
}

#[test]
fn root_state_and_pending_callbacks_execute_in_order() {
    let Some(()) = super::wasmtime_available() else {
        return;
    };
    let source = r"module Main where
import Prim.Control as C
data Op x = Op (Int -> x)
handler :: forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
handler state (Op k) raw = { _1: case state of
                                    7 -> 1
                                    _ -> 0
                         , _2: raw (k 42) }
work :: C.Control Op Int
work = C.bind (C.bind (C.return 2) (\x -> case x of
                                          2 -> C.return 40
                                          _ -> C.return 0))
       (\x -> case x of
                40 -> C.bind (C.perform (Op \y -> y)) (\y -> case y of
                                                             42 -> C.return 43
                                                             _ -> C.return 0)
                _ -> C.return 0)
main :: Int
main = let result = C.run 7 handler work
       in case result._1 of
            1 -> result._2
            _ -> 0
";
    let stages = crate::compile_main_stages(source).unwrap();
    let path = std::env::temp_dir().join("psrs-control-root-queue-order.wasm");
    std::fs::write(&path, stages.artifact.wasm).unwrap();
    let output = std::process::Command::new("wasmtime")
        .arg(path)
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(43), "{output:?}");
}

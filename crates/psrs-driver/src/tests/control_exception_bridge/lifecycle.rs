//! Observe exceptional lifecycle outcomes in emitted guests, not only the model.
use super::*;
use psrs_backend::cc::{
    Assignment, AssignmentKind as A, BinaryOp, ControlHelperRole as R, Function, RefShape,
    Representation, ValueDecl, ValueId, ValueShape,
};

const OWNED: &str = r"
main :: Int
main = let owner = C.once (\n -> C.return (rec n))
       in if caught (\_ -> (C.run 0 handler (C.resume owner 100000))._2)
          then 42 else 0
";

fn emit(f: &mut Function, ty: ValueShape, kind: impl FnOnce(ValueId) -> A) -> ValueId {
    let id = ValueId(f.values.iter().map(|v| v.id.0).max().unwrap() + 1);
    f.values.push(ValueDecl { id, ty });
    f.assignments.push(Assignment {
        destination: id,
        kind: kind(id),
        span: f.span,
    });
    id
}

#[test]
fn exceptional_owner_is_consumed_and_no_longer_retains_its_callback() {
    if wasmtime_available().is_none() {
        return;
    }
    let mut input = crate::lower_main_to_cc(&format!("{HEADER}\n{OWNED}")).unwrap();
    let once = input
        .cc
        .representations
        .control_helpers()
        .iter()
        .find(|h| h.role() == R::Once)
        .unwrap()
        .function();
    let (index, owner) = input
        .cc
        .functions
        .iter()
        .enumerate()
        .find_map(|(i, f)| {
            f.assignments.iter().find_map(|a| match a.kind {
                A::DirectCall { function, .. } if function == once => Some((i, a.destination)),
                _ => None,
            })
        })
        .expect("source owner allocation");
    let function = &input.cc.functions[index];
    let ValueShape::Reference(owner_ref) =
        function.values.iter().find(|v| v.id == owner).unwrap().ty
    else {
        panic!("nominal owner");
    };
    let RefShape::Repr(owner_repr) = owner_ref.heap else {
        panic!("owner representation")
    };
    let Some(Representation::Variant { cases }) =
        input.cc.representations.representation(owner_repr)
    else {
        panic!("owner variant");
    };
    let callback_ty = cases[0].fields[0];
    let status_ty = cases[0].fields[1];
    let ValueShape::Reference(status_ref) = status_ty else {
        panic!("status cell")
    };
    let RefShape::Repr(status_repr) = status_ref.heap else {
        panic!("status representation")
    };
    let f = &mut input.cc.functions[index];
    // Append observations to the ordinary source function, leaving the checked
    // machine helpers, guards, cleanup stores and exception transport unchanged.
    let answer = f.result;
    let status = emit(f, status_ty, |destination| A::VariantGet {
        destination,
        representation: owner_repr,
        case: 0,
        field: 1,
        value: owner,
    });
    let zero = emit(f, ValueShape::Integer, |_| A::Constant(0));
    let consumed = emit(f, ValueShape::Integer, |destination| A::ArrayGet {
        destination,
        representation: status_repr,
        value: status,
        index: zero,
    });
    let callbacks = emit(f, callback_ty, |destination| A::VariantGet {
        destination,
        representation: owner_repr,
        case: 0,
        field: 0,
        value: owner,
    });
    let ValueShape::Reference(callback_ref) = callback_ty else {
        panic!("callback cell")
    };
    let RefShape::Repr(callback_repr) = callback_ref.heap else {
        panic!("callback representation")
    };
    let Some(Representation::Array { element }) =
        input.cc.representations.representation(callback_repr)
    else {
        panic!("callback array");
    };
    let element = *element;
    let callback = emit(f, element, |destination| A::ArrayGet {
        destination,
        representation: callback_repr,
        value: callbacks,
        index: zero,
    });
    let cleared = emit(f, ValueShape::Boolean, |destination| {
        A::RepresentationTest {
            destination,
            value: callback,
            reference: status_ref,
        }
    });
    let observed = emit(f, ValueShape::Integer, |_| A::Primitive {
        op: BinaryOp::IntMul,
        left: consumed,
        right: answer,
    });
    f.result = emit(f, ValueShape::Integer, |_| A::If {
        condition: cleared,
        then_assignments: vec![],
        then_value: observed,
        else_assignments: vec![],
        else_value: zero,
    });
    let artifact = psrs_backend::compile_cc(input).expect("instrumented source verifies and emits");
    let output = run_artifact_with_wasmtime(&artifact.wasm);
    assert_eq!(
        output.status.code(),
        Some(84),
        "Consumed (2) times caught answer (42), with callback replaced by a status cell: {output:?}"
    );
}

#[test]
fn exceptional_owner_rejects_both_resume_and_cancel_in_a_fresh_runner() {
    if wasmtime_available().is_none() {
        return;
    }
    for later in [
        r"(C.run 0 handler (C.resume owner 42))._2",
        r"(C.run 0 handler (C.bind (C.return (C.cancel owner)) (\_ -> C.return 42)))._2",
    ] {
        let source = OWNED.replace("then 42 else 0", &format!("then {later} else 0"));
        let output = execute(&source);
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("wasm `unreachable` instruction executed"),
            "spent owner must reject {later}: {output:?}"
        );
        assert!(!output.status.success());
    }
}

#[test]
fn an_exception_crossing_two_runners_expires_both_saved_checkpoints() {
    if wasmtime_available().is_none() {
        return;
    }
    for saved in ["outerSaved", "innerSaved"] {
        let source = r#"
foreign import "psrs:intrinsic#arrayFill" filled :: forall a. Int -> a -> Array a
foreign import "psrs:intrinsic#arrayWrite" slot :: forall a. Array a -> Int -> a -> Array a
foreign import "psrs:intrinsic#arrayIndex" at :: forall a. Array a -> Int -> a
foreign import "psrs:intrinsic#arrayLength" size :: forall a. Array a -> Int
inner :: Array (Int -> C.Control Op Int) -> forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
inner saved s (Op k) raw =
  { _1: s, _2: C.bind (C.return (size (slot saved 0 (\n -> raw (k n))))) (\_ -> C.return (rec 100000)) }
outer :: Array (Int -> C.Control Op Int) -> Array (Int -> C.Control Op Int) -> forall x. Int -> Op x -> (x -> C.Control Op Int) -> { _1 :: Int, _2 :: C.Control Op Int }
outer saved innerSaved s (Op k) raw =
  { _1: s, _2: C.bind (C.return (size (slot saved 0 (\n -> raw (k n)))))
      (\_ -> C.return ((C.run 0 (inner innerSaved) (C.perform (Op (\x -> x))))._2)) }
main :: Int
main = let outerSaved = filled 1 (\n -> C.return n)
           innerSaved = filled 1 (\n -> C.return n)
       in if caught (\_ -> (C.run 0 (outer outerSaved innerSaved) (C.perform (Op (\x -> x))))._2)
          then (C.run 0 handler ((at SELECTED 0) 42))._2 else 0
"#.replace("SELECTED", saved);
        let output = execute(&source);
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("wasm `unreachable` instruction executed"),
            "{saved} must expire when both runners abort: {output:?}"
        );
        assert!(!output.status.success());
    }
}

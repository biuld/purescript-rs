use super::*;

#[test]
fn recursive_function_captures_an_earlier_same_let_value_and_escapes() {
    let backend = crate::cc::lower_module(escaping_recursive_module())
        .expect("a closure may escape while capturing a recursive local function");
    let main = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name == "main")
        .expect("main CC function");
    let recursive = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name.starts_with("loop_letrec_"))
        .expect("lifted recursive function");
    let escaped = backend
        .cc
        .functions
        .iter()
        .find(|function| function.name.starts_with("lambda_"))
        .expect("lifted escaping closure");

    let seed_value = main
        .assignments
        .iter()
        .find_map(|assignment| match assignment.kind {
            AssignmentKind::Constant(17) => {
                assert_eq!(assignment.span, range(22, 23));
                Some(assignment.destination)
            }
            _ => None,
        })
        .expect("main evaluates the same-Let seed before the recursive function");
    let recursive_value = main
        .assignments
        .iter()
        .find_map(|assignment| match &assignment.kind {
            AssignmentKind::FunctionRef {
                function, captures, ..
            } if *function == recursive.symbol => {
                assert_eq!(captures, &[seed_value]);
                assert_eq!(assignment.span, range(30, 92));
                Some(assignment.destination)
            }
            _ => None,
        })
        .expect("main creates the recursive function value");
    let seed_index = main
        .assignments
        .iter()
        .position(|assignment| matches!(assignment.kind, AssignmentKind::Constant(17)))
        .expect("seed assignment exists");
    let recursive_index = main
        .assignments
        .iter()
        .position(|assignment| {
            matches!(
                assignment.kind,
                AssignmentKind::FunctionRef { function, .. } if function == recursive.symbol
            )
        })
        .expect("recursive function reference exists");
    assert!(seed_index < recursive_index);
    let escaping_reference = main
        .assignments
        .iter()
        .find(|assignment| {
            matches!(
                assignment.kind,
                AssignmentKind::FunctionRef { function, .. } if function == escaped.symbol
            )
        })
        .expect("main creates the escaping closure");
    let AssignmentKind::FunctionRef { captures, .. } = &escaping_reference.kind else {
        unreachable!("the selected assignment is a function reference");
    };
    assert_eq!(captures, &[recursive_value]);
    assert_eq!(escaping_reference.span, range(100, 124));
    assert!(escaped.assignments.iter().any(|assignment| {
        matches!(
            assignment.kind,
            AssignmentKind::ClosureGetCapture { index: 0, .. }
        )
    }));
    assert!(contains_indirect_call(&escaped.assignments));
    let escaped_capture = escaped
        .assignments
        .iter()
        .find_map(|assignment| match assignment.kind {
            AssignmentKind::ClosureGetCapture { index: 0, .. } => Some(assignment.destination),
            _ => None,
        })
        .expect("escaping closure restores the captured recursive function");
    assert_eq!(
        value_shape(escaped, escaped_capture),
        value_shape(main, recursive_value)
    );

    let captured_seed = recursive
        .assignments
        .iter()
        .find_map(|assignment| match assignment.kind {
            AssignmentKind::ClosureGetCapture { index: 0, .. } => Some(assignment.destination),
            _ => None,
        })
        .expect("recursive closure restores its same-Let seed capture");
    assert!(recursive.assignments.iter().any(|assignment| {
        matches!(
            &assignment.kind,
            AssignmentKind::FunctionRef { function, captures, .. }
                if *function == recursive.symbol && captures == &[captured_seed]
        ) && assignment.span == range(30, 92)
    }));
    assert_eq!(recursive.span, range(40, 90));
    assert_eq!(
        value_shape(recursive, captured_seed),
        Some(ValueShape::Integer)
    );
    crate::mir::lower_module(backend.cc.clone())
        .expect("P9 should accept the escaping recursive closure module");
}

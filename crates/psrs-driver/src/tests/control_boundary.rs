//! Operand and ordered-effect obligations at the P8 construction boundary.
use psrs_backend::cc::{Assignment, AssignmentKind as A, ControlHelperRole as R, Function};

const SOURCE: &str = include_str!("../../tests/control_support/nominal_carriers.purs");

fn mutate(body: &mut [Assignment], edit: &mut impl FnMut(&mut A) -> bool) -> bool {
    for assignment in body {
        if edit(&mut assignment.kind) {
            return true;
        }
        match &mut assignment.kind {
            A::TagSwitch {
                cases,
                default_assignments,
                ..
            } => {
                for case in cases {
                    if mutate(&mut case.assignments, edit) {
                        return true;
                    }
                }
                if mutate(default_assignments, edit) {
                    return true;
                }
            }
            A::Protect { body, handler, .. } => {
                if mutate(&mut body.assignments, edit) || mutate(&mut handler.assignments, edit) {
                    return true;
                }
            }
            A::If {
                then_assignments,
                else_assignments,
                ..
            } => {
                if mutate(then_assignments, edit) || mutate(else_assignments, edit) {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

fn rejected(role: R, edit: impl FnOnce(&mut Function)) {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let symbol = input
        .cc
        .representations
        .control_helpers()
        .iter()
        .find(|h| h.role() == role)
        .unwrap()
        .function();
    let function = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    edit(function);
    let errors =
        psrs_backend::cc::verify_module(&input.cc).expect_err("mutated machine must fail P8");
    assert!(
        errors.iter().any(|error| error
            .message
            .contains("emitted machine operands or ordered operations")),
        "{errors:?}"
    );
}

#[test]
fn projected_clause_field_must_match_the_plan() {
    rejected(R::Executor, |f| {
        let callable: std::collections::HashSet<_> = f.values.iter().filter(|v| matches!(v.ty, psrs_backend::cc::ValueShape::Reference(r) if matches!(r.heap, psrs_backend::cc::RefShape::Closure(_)))).map(|v| v.id).collect();
        assert!(mutate(&mut f.assignments, &mut |a| {
            if let A::VariantGet {
                case: 1,
                field,
                destination,
                ..
            } = a
                && *field == 0
                && callable.contains(destination)
            {
                // Scoped return and operation clauses have the same shape.
                *field = 1;
                return true;
            }
            false
        }));
    });
}

#[test]
fn replay_projection_index_must_match_the_plan() {
    rejected(R::Executor, |f| {
        assert!(mutate(&mut f.assignments, &mut |a| {
            if let A::ProductGet { field, .. } = a {
                *field = 1 - *field;
                return true;
            }
            false
        }));
    });
}

#[test]
fn helper_arguments_must_preserve_source_identity() {
    rejected(R::CloneSequence, |f| {
        assert!(mutate(&mut f.assignments, &mut |a| {
            if let A::DirectCall { arguments, .. } = a
                && arguments.len() >= 2
            {
                arguments.swap(0, 1);
                return true;
            }
            false
        }));
    });
}

#[test]
fn dispatch_selector_must_preserve_source_identity() {
    rejected(R::CloneSequence, |f| {
        let other_queue = f.parameters[1];
        assert!(mutate(&mut f.assignments, &mut |a| {
            if let A::VariantTag { value, .. } = a {
                *value = other_queue;
                return true;
            }
            false
        }));
    });
}

#[test]
fn clause_application_must_consume_its_planned_argument() {
    rejected(R::Executor, |f| {
        let state = f.parameters[0];
        assert!(mutate(&mut f.assignments, &mut |a| {
            if let A::IndirectCall { arguments, .. } = a {
                arguments[0] = state;
                return true;
            }
            false
        }));
    });
}

#[test]
fn allocation_fields_must_preserve_source_identity() {
    rejected(R::Executor, |f| {
        assert!(mutate(&mut f.assignments, &mut |a| {
            if let A::VariantNew {
                case: 2, fields, ..
            } = a
                && fields.len() == 2
            {
                fields.swap(0, 1);
                return true;
            }
            false
        }));
    });
}

#[test]
fn callback_capture_index_must_match_the_plan() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let symbol = input
        .cc
        .representations
        .control_resumption()
        .unwrap()
        .function();
    let f = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    assert!(mutate(&mut f.assignments, &mut |a| {
        if let A::ClosureGetCapture { index, .. } = a {
            *index = 1 - *index;
            return true;
        }
        false
    }));
    let errors = psrs_backend::cc::verify_module(&input.cc).unwrap_err();
    assert!(
        errors.iter().any(|e| e.pass == "P8 CC verification"),
        "{errors:?}"
    );
}

#[test]
fn status_write_value_and_index_must_match_the_plan() {
    for wrong_index in [false, true] {
        rejected(R::Cancel, |f| {
            assert!(mutate(&mut f.assignments, &mut |a| {
                if let A::ArraySet {
                    index, new_value, ..
                } = a
                {
                    if wrong_index {
                        *index = *new_value;
                    } else {
                        *new_value = *index;
                    }
                    return true;
                }
                false
            }));
        });
    }
}

#[test]
fn discarded_close_effect_is_required() {
    rejected(R::Close, |f| {
        let A::TagSwitch { cases, .. } = &mut f.assignments.last_mut().unwrap().kind else {
            panic!("close must dispatch on the extent");
        };
        let mut removed = 0;
        for case in cases {
            case.assignments.retain(|a| {
                if matches!(a.kind, A::ArraySet { .. }) {
                    removed += 1;
                    false
                } else {
                    true
                }
            });
        }
        assert!(removed > 0);
    });
}

#[test]
fn extra_observable_operation_is_rejected() {
    rejected(R::Start, |f| {
        let allocation = f
            .assignments
            .iter()
            .find(|a| matches!(a.kind, A::ArrayNew { .. }))
            .unwrap()
            .clone();
        // ArraySet destinations may reuse an existing cell; this adds a legal
        // store using the fresh status cell and its already-bound zero value.
        let A::ArrayNew {
            representation,
            elements,
            ..
        } = allocation.kind
        else {
            unreachable!()
        };
        f.assignments.push(Assignment {
            destination: allocation.destination,
            kind: A::ArraySet {
                destination: allocation.destination,
                representation,
                value: allocation.destination,
                index: elements[0],
                new_value: elements[0],
            },
            span: allocation.span,
        });
    });
}

#[test]
fn returned_helper_value_must_match_the_plan() {
    rejected(R::Check, |f| f.result = f.parameters[0]);
}

#[test]
fn unused_narrowing_cast_is_rejected_even_when_its_result_is_erased() {
    for erase_result in [false, true] {
        rejected(R::Executor, |f| {
            let mut declaration = f
                .values
                .iter()
                .find(|v| v.id == f.parameters[2])
                .unwrap()
                .clone();
            declaration.id =
                psrs_backend::cc::ValueId(f.values.iter().map(|v| v.id.0).max().unwrap() + 1);
            let psrs_backend::cc::ValueShape::Reference(reference) = declaration.ty else {
                unreachable!()
            };
            let narrowed = declaration.id;
            f.values.push(declaration.clone());
            f.assignments.push(Assignment {
                destination: narrowed,
                kind: A::RepresentationCast {
                    destination: narrowed,
                    value: f.parameters[0],
                    reference,
                },
                span: f.span,
            });
            if erase_result {
                declaration.id = psrs_backend::cc::ValueId(narrowed.0 + 1);
                let reference = psrs_backend::cc::Reference {
                    nullable: false,
                    heap: psrs_backend::cc::RefShape::Erased,
                };
                declaration.ty = psrs_backend::cc::ValueShape::Reference(reference);
                f.assignments.push(Assignment {
                    destination: declaration.id,
                    kind: A::RepresentationCast {
                        destination: declaration.id,
                        value: narrowed,
                        reference,
                    },
                    span: f.span,
                });
                f.values.push(declaration);
            }
        });
    }
}

#[test]
fn reference_alias_on_the_actual_machine_exit_is_accepted() {
    let mut input = crate::lower_main_to_cc(SOURCE).unwrap();
    let symbol = input
        .cc
        .representations
        .control_helpers()
        .iter()
        .find(|h| h.role() == R::Start)
        .unwrap()
        .function();
    let f = input
        .cc
        .functions
        .iter_mut()
        .find(|f| f.symbol == symbol)
        .unwrap();
    let mut declaration = f.values.iter().find(|v| v.id == f.result).unwrap().clone();
    declaration.id = psrs_backend::cc::ValueId(f.values.iter().map(|v| v.id.0).max().unwrap() + 1);
    let psrs_backend::cc::ValueShape::Reference(reference) = declaration.ty else {
        unreachable!()
    };
    f.assignments.push(Assignment {
        destination: declaration.id,
        kind: A::RepresentationCast {
            destination: declaration.id,
            value: f.result,
            reference,
        },
        span: f.span,
    });
    f.result = declaration.id;
    f.values.push(declaration);
    psrs_backend::cc::verify_module(&input.cc).unwrap();
    psrs_backend::compile_cc(input).unwrap();
}

#[test]
fn callback_recovery_cannot_move_before_the_callback_clear() {
    rejected(R::Executor, |f| {
        let A::TagSwitch { cases, .. } = &mut f.assignments.last_mut().unwrap().kind else {
            panic!("executor must dispatch on computation");
        };
        let body = &mut cases
            .iter_mut()
            .find(|case| case.tag == 5)
            .unwrap()
            .assignments;
        let A::Protect { body, .. } = &mut body
            .iter_mut()
            .find(|assignment| matches!(assignment.kind, A::Protect { .. }))
            .unwrap()
            .kind
        else {
            panic!("owned callback is protected")
        };
        let body = &mut body.assignments;
        let recovery = body
            .iter()
            .position(|a| {
                matches!(
                    a.kind,
                    A::RepresentationCast {
                        reference: psrs_backend::cc::Reference {
                            heap: psrs_backend::cc::RefShape::Closure(_),
                            ..
                        },
                        ..
                    }
                )
            })
            .unwrap();
        let clear = body[..recovery]
            .iter()
            .rposition(|a| matches!(a.kind, A::ArraySet { .. }))
            .unwrap();
        let cast = body.remove(recovery);
        body.insert(clear, cast);
    });
}

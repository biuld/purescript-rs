//! Compiler-generated raises share the checked native source-error channel.
use super::*;
use crate::cc::{self, Assignment, AssignmentKind as A, Region, RegionExit, ValueShape};

fn fixture() -> cc::Module {
    let assignment = |destination, kind| Assignment {
        destination: ValueId(destination),
        kind,
        span: at(1),
    };
    cc::Module {
        name: "SourceRaise".into(),
        externals: vec![],
        representations: Default::default(),
        functions: vec![cc::Function {
            symbol: SymbolId::new(ModuleId(0), 0),
            name: "main".into(),
            parameters: vec![],
            values: [
                ValueShape::String,
                ValueShape::Integer,
                ValueShape::Reference(cc::Reference {
                    nullable: false,
                    heap: cc::RefShape::Exception,
                }),
                ValueShape::Integer,
                ValueShape::Integer,
            ]
            .into_iter()
            .enumerate()
            .map(|(id, ty)| cc::ValueDecl {
                id: ValueId(id as u32),
                ty,
            })
            .collect(),
            assignments: vec![
                assignment(0, A::StringConstant("generated source error".into())),
                assignment(
                    4,
                    A::Protect {
                        body: Region {
                            assignments: vec![assignment(
                                1,
                                A::RaiseSourceError {
                                    message: ValueId(0),
                                },
                            )],
                            exit: RegionExit::Yield(ValueId(1)),
                        },
                        exception: ValueId(2),
                        handler: Region {
                            assignments: vec![assignment(3, A::Constant(42))],
                            exit: RegionExit::Yield(ValueId(3)),
                        },
                    },
                ),
            ],
            result: ValueId(4),
            result_type: ValueShape::Integer,
            span: at(0),
        }],
        entry: Some(SymbolId::new(ModuleId(0), 0)),
        span: at(0),
    }
}

#[test]
fn provider_free_nested_raise_plans_the_source_tag_and_executes() {
    let (mir, _) = crate::mir::lower_module(fixture()).expect("checked CC raise");
    assert_eq!(mir.tags.len(), 1);
    assert_eq!(mir.tags[0].name, crate::bindings::SOURCE_ERROR_TAG_NAME);
    assert_eq!(mir.tags[0].payload.len(), 1);
    assert!(mir.imports.is_empty());
    assert!(
        mir.functions[0]
            .blocks
            .iter()
            .any(|b| matches!(b.terminator, Some(Terminator::Throw { .. })))
    );
    if execution::require_engine() {
        let output = execution::run_output(&mir, enabled_target()).unwrap();
        assert_eq!(output.status.code(), Some(42), "{output:?}");
        assert!(
            output.stdout.is_empty() && output.stderr.is_empty(),
            "{output:?}"
        );
    }
}

#[test]
fn compiler_raise_rejects_non_string_or_unavailable_payloads() {
    let mut wrong = fixture();
    wrong.functions[0].values[0].ty = ValueShape::Integer;
    wrong.functions[0].assignments[0].kind = A::Constant(7);
    let errors = crate::cc::verify_module(&wrong).unwrap_err();
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("payload must be String")),
        "{errors:?}"
    );
    let mut unavailable = fixture();
    unavailable.functions[0].assignments.remove(0);
    assert!(crate::cc::verify_module(&unavailable).is_err());
}

#[test]
fn compiler_raise_rejects_a_disabled_exception_profile() {
    let errors = crate::mir::lower_module_with_capabilities(
        fixture(),
        crate::TargetCapabilities {
            exceptions: false,
            ..Default::default()
        },
    )
    .err()
    .expect("disabled exceptions reject source raising");
    assert!(
        errors
            .iter()
            .any(|e| e.message.contains("exception-handling proposal")),
        "{errors:?}"
    );
}

#[test]
fn mir_rechecks_the_generated_raise_payload_type() {
    let (mut mir, _) = crate::mir::lower_module(fixture()).unwrap();
    let function = &mut mir.functions[0];
    function.values.push(ValueDecl {
        id: ValueId(5),
        ty: ValueType::I32,
    });
    let block = function
        .blocks
        .iter_mut()
        .find(|b| matches!(b.terminator, Some(Terminator::Throw { .. })))
        .unwrap();
    block.instructions.push(Instruction::Constant {
        destination: ValueId(5),
        value: 7,
        span: at(1),
    });
    let Some(Terminator::Throw { arguments, .. }) = &mut block.terminator else {
        unreachable!()
    };
    *arguments = vec![ValueId(5)];
    assert!(crate::mir::verify_module_with_capabilities(&mir, enabled_target()).is_err());
}

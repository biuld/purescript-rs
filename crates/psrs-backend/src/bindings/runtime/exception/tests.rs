//! Source validation precedes reachability and representation erasure.
use super::*;
use psrs_core::ExternalType;
use psrs_hir::{ExternalKind, ExternalSymbol, SymbolId, TypeVariableId};

fn fixture() -> (Module, TypeId) {
    let mut module = crate::bindings::tests::module(Vec::new());
    module.types = vec![
        Type::Constructor(TypeConstructor::Unit),
        Type::Constructor(TypeConstructor::String),
        Type::Variable(TypeVariableId(0)),
        Type::Closure {
            parameters: vec![TypeId(0)],
            result: TypeId(2),
        },
        Type::Closure {
            parameters: vec![TypeId(1)],
            result: TypeId(2),
        },
        Type::Closure {
            parameters: vec![TypeId(3), TypeId(4)],
            result: TypeId(2),
        },
        Type::Constructor(TypeConstructor::Int),
    ];
    let symbol = SymbolId::new(module.id, 17);
    module.externals.push(ExternalSymbol {
        source_module: None,
        symbol,
        name: "recover".into(),
        kind: ExternalKind::Runtime {
            module: RUNTIME_EXCEPTION_MODULE.into(),
            function: CATCH_EXPORT.into(),
        },
        signature: None,
    });
    module.external_types.push(ExternalType {
        symbol,
        source_module: ModuleId(42),
        ty: TypeId(5),
    });
    (module, TypeId(5))
}

#[test]
fn validates_unused_catch_results_before_erasure() {
    let (module, signature) = fixture();
    super::super::validate(&module).unwrap();
    for change in 0..7 {
        let mut wrong = module.clone();
        match change {
            0 => {
                wrong.types[5] = Type::Closure {
                    parameters: vec![TypeId(3)],
                    result: TypeId(2),
                }
            }
            1 => {
                wrong.types[3] = Type::Closure {
                    parameters: vec![TypeId(1)],
                    result: TypeId(2),
                }
            }
            2 => {
                wrong.types[4] = Type::Closure {
                    parameters: vec![TypeId(0)],
                    result: TypeId(2),
                }
            }
            3 => {
                wrong.types[3] = Type::Closure {
                    parameters: vec![TypeId(0)],
                    result: TypeId(6),
                }
            }
            4 => {
                wrong.types[4] = Type::Closure {
                    parameters: vec![TypeId(1)],
                    result: TypeId(6),
                }
            }
            5 => {
                wrong.types[5] = Type::Closure {
                    parameters: vec![TypeId(3), TypeId(4)],
                    result: TypeId(6),
                }
            }
            _ => {
                wrong.types[4] = Type::Closure {
                    parameters: vec![TypeId(1), TypeId(0)],
                    result: TypeId(2),
                }
            }
        }
        assert!(
            ExceptionOperation::Catch
                .validate_source(&wrong, signature)
                .is_err(),
            "change {change}"
        );
        let errors = super::super::validate(&wrong).expect_err("unused declaration is checked");
        assert!(
            errors.iter().all(|e| e.module == Some(ModuleId(42))),
            "{errors:?}"
        );
    }
}

#[test]
fn source_throw_requires_one_string_and_checked_metadata() {
    let (mut module, signature) = fixture();
    module.externals[0].kind = ExternalKind::Runtime {
        module: RUNTIME_EXCEPTION_MODULE.into(),
        function: THROW_EXPORT.into(),
    };
    module.types[5] = Type::Closure {
        parameters: vec![TypeId(1)],
        result: TypeId(2),
    };
    super::super::validate(&module).unwrap();
    for parameters in [vec![], vec![TypeId(0)], vec![TypeId(1), TypeId(1)]] {
        module.types[5] = Type::Closure {
            parameters,
            result: TypeId(2),
        };
        assert!(
            ExceptionOperation::Throw
                .validate_source(&module, signature)
                .is_err()
        );
    }
    module.external_types.clear();
    assert!(super::super::validate(&module).is_err());
    module.externals[0].kind = ExternalKind::Runtime {
        module: RUNTIME_EXCEPTION_MODULE.into(),
        function: "unregistered".into(),
    };
    assert!(
        super::super::validate(&module).unwrap_err()[0]
            .message
            .contains("no export")
    );
}

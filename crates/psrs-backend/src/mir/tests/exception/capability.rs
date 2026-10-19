//! EH-08 capability inference for the exception channel.

use super::{at, block, module, simple_function, tag, tag_only_module};
use crate::capability::TargetCapabilities;
use crate::mir::{BlockId, Terminator, ValueId, verify_module_with_capabilities};

fn try_module() -> crate::mir::Module {
    module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        continuation: None,
                        clauses: vec![crate::mir::CatchClause::CatchAll { target: BlockId(2) }],
                        span: at(10),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(11),
                    },
                ),
                block(
                    2,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(12),
                    },
                ),
            ],
            Vec::new(),
        ),
        Vec::new(),
    )
}

#[test]
fn exception_terminators_require_the_exceptions_capability() {
    let exception_module = try_module();
    let disabled = TargetCapabilities {
        exceptions: false,
        ..TargetCapabilities::default()
    };
    let errors = verify_module_with_capabilities(&exception_module, disabled)
        .expect_err("try regions require the exceptions gate");
    assert!(errors.iter().any(|error| {
        error
            .message
            .contains("exception-handling proposal required by this module")
            && error.span == exception_module.span
            && error.module == exception_module.entry.map(|entry| entry.module)
    }));

    let enabled = TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    };
    verify_module_with_capabilities(&exception_module, enabled)
        .expect("the enabled profile admits the try region");
}

#[test]
fn a_declared_tag_requires_the_exceptions_capability() {
    let module = tag_only_module(vec![tag(0, Vec::new())]);
    let disabled = TargetCapabilities {
        exceptions: false,
        ..TargetCapabilities::default()
    };
    let errors = verify_module_with_capabilities(&module, disabled)
        .expect_err("a declared tag requires the exceptions gate");
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("exception-handling proposal")),
        "{errors:?}"
    );

    let enabled = TargetCapabilities {
        exceptions: true,
        ..TargetCapabilities::default()
    };
    verify_module_with_capabilities(&module, enabled).expect("the enabled profile admits the tag");
}

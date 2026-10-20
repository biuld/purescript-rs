//! EH-06 structural region fixtures: normal predecessors, handler
//! ownership, back edges, and forced tail calls in protected regions.

use super::{at, block, module, simple_function, tag};
use crate::mir::{BlockId, TagId, Terminator, ValueId, verify_module};

#[test]
fn rejects_a_handler_with_a_normal_predecessor() {
    let module = module(
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
                    Terminator::Jump {
                        target: BlockId(2),
                        arguments: vec![],
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
    );
    let errors = verify_module(&module).expect_err("a normal predecessor of a handler is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("handler block has a normal predecessor")
                && error.span == at(11)
        }),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_handler_claimed_by_two_try_regions() {
    let module = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        continuation: None,
                        clauses: vec![crate::mir::CatchClause::Catch {
                            tag: TagId(0),
                            target: BlockId(4),
                        }],
                        span: at(10),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Jump {
                        target: BlockId(2),
                        arguments: vec![],
                        span: at(11),
                    },
                ),
                block(
                    2,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(3),
                        continuation: None,
                        clauses: vec![crate::mir::CatchClause::Catch {
                            tag: TagId(0),
                            target: BlockId(4),
                        }],
                        span: at(12),
                    },
                ),
                block(
                    3,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(13),
                    },
                ),
                block(
                    4,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(14),
                    },
                ),
            ],
            Vec::new(),
        ),
        vec![tag(0, Vec::new())],
    );
    let errors = verify_module(&module).expect_err("a handler claimed twice is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("claimed by more than one try region")
                && error.span == at(12)
        }),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_back_edge_targeting_a_try_block() {
    let module = module(
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
                    Terminator::Jump {
                        target: BlockId(3),
                        arguments: vec![],
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
                block(
                    3,
                    vec![],
                    vec![],
                    Terminator::Jump {
                        target: BlockId(0),
                        arguments: vec![],
                        span: at(13),
                    },
                ),
            ],
            Vec::new(),
        ),
        Vec::new(),
    );
    let errors = verify_module(&module).expect_err("a back edge into a try block is rejected");
    assert!(
        errors.iter().any(|error| {
            error.message.contains("back edge targets a try block") && error.span == at(13)
        }),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_forced_tail_call_in_a_protected_region() {
    let span = at(20);
    let module = module(
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
                    Terminator::ReturnCall {
                        contract: None,
                        function: psrs_hir::SymbolId::new(psrs_hir::ModuleId(0), 0),
                        arguments: vec![ValueId(0)],
                        span,
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
    );
    let errors = verify_module(&module).expect_err("a forced tail call in a region is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("protected try-region block ends in a forced tail call")
                && error.span == span
        }),
        "{errors:?}"
    );
}

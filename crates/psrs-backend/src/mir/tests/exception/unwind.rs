//! EH-06 unwind-edge verification fixtures.

use super::{at, block, module, simple_function, tag};
use crate::mir::{BlockId, TagId, Terminator, ValueId, verify_module};

#[test]
fn accepts_a_well_formed_try_region_and_nested_tries() {
    // A single `catch_all` region.
    let plain = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        catches: vec![],
                        catch_all: Some(BlockId(2)),
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
    );
    verify_module(&plain).expect("a catch_all region verifies");

    // Nested regions: the inner region catches tag 1, the outer tag 0. The
    // protected block throws to the inner handler; the inner handler's throw
    // of the outer tag resolves outward, past the disarmed inner region.
    let nested = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        catches: vec![(TagId(0), BlockId(5))],
                        catch_all: None,
                        span: at(10),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(2),
                        catches: vec![(TagId(1), BlockId(4))],
                        catch_all: None,
                        span: at(11),
                    },
                ),
                block(
                    2,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(1),
                        arguments: vec![],
                        handler: Some(BlockId(4)),
                        span: at(12),
                    },
                ),
                block(
                    4,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(0),
                        arguments: vec![],
                        handler: Some(BlockId(5)),
                        span: at(14),
                    },
                ),
                block(
                    5,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(15),
                    },
                ),
            ],
            Vec::new(),
        ),
        vec![tag(0, Vec::new()), tag(1, Vec::new())],
    );
    verify_module(&nested).expect("nested try regions verify");
}

#[test]
fn rejects_a_throw_whose_handler_is_not_the_matching_innermost_handler() {
    let module = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        catches: vec![(TagId(0), BlockId(5))],
                        catch_all: None,
                        span: at(10),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(2),
                        catches: vec![(TagId(1), BlockId(4))],
                        catch_all: None,
                        span: at(11),
                    },
                ),
                block(
                    2,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(1),
                        arguments: vec![],
                        handler: Some(BlockId(5)),
                        span: at(12),
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
                block(
                    5,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(15),
                    },
                ),
            ],
            Vec::new(),
        ),
        vec![tag(0, Vec::new()), tag(1, Vec::new())],
    );
    let errors = verify_module(&module).expect_err("a mismatched inner handler is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("does not resolve to the innermost enclosing region's matching handler")
                && error.span == at(12)
        }),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_throw_tag_no_enclosing_region_catches() {
    // The inner region catches tag 1 only; a tag-0 throw inside it cannot
    // name a handler because no enclosing region catches tag 0 either.
    let module = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        catches: vec![(TagId(1), BlockId(2))],
                        catch_all: None,
                        span: at(10),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(0),
                        arguments: vec![],
                        handler: Some(BlockId(2)),
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
        vec![tag(0, Vec::new()), tag(1, Vec::new())],
    );
    let errors = verify_module(&module).expect_err("an uncaught tag inside a region is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("not caught by any enclosing try region")
                && error.span == at(11)
        }),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_throw_outside_every_region_that_names_a_handler() {
    let module = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(0),
                        arguments: vec![],
                        handler: Some(BlockId(1)),
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
            ],
            Vec::new(),
        ),
        vec![tag(0, Vec::new())],
    );
    let errors =
        verify_module(&module).expect_err("a handler named outside every region is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("not caught by any enclosing try region")
                && error.span == at(10)
        }),
        "{errors:?}"
    );
}

#[test]
fn rejects_a_throw_without_a_handler_inside_a_protected_region() {
    // The region catches a different tag, so no handler exists to name: the
    // handler-less throw lies inside a protected region and is rejected.
    let uncaught = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        catches: vec![(TagId(1), BlockId(2))],
                        catch_all: None,
                        span: at(10),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(0),
                        arguments: vec![],
                        handler: None,
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
        vec![tag(0, Vec::new()), tag(1, Vec::new())],
    );
    let errors =
        verify_module(&uncaught).expect_err("a handler-less throw inside a region is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("without a handler lies inside a protected try region")
                && error.span == at(11)
        }),
        "{errors:?}"
    );

    // When the region does catch the tag, the handler-less throw is rejected
    // with the resolution rule instead.
    let caught = module(
        simple_function(
            vec![
                block(
                    0,
                    vec![],
                    vec![],
                    Terminator::Try {
                        protected: BlockId(1),
                        catches: vec![],
                        catch_all: Some(BlockId(2)),
                        span: at(15),
                    },
                ),
                block(
                    1,
                    vec![],
                    vec![],
                    Terminator::Throw {
                        tag: TagId(0),
                        arguments: vec![],
                        handler: None,
                        span: at(16),
                    },
                ),
                block(
                    2,
                    vec![],
                    vec![],
                    Terminator::Return {
                        value: ValueId(0),
                        span: at(17),
                    },
                ),
            ],
            Vec::new(),
        ),
        vec![tag(0, Vec::new())],
    );
    let errors = verify_module(&caught)
        .expect_err("a handler-less throw that the region catches is rejected");
    assert!(
        errors.iter().any(|error| {
            error
                .message
                .contains("must name the matching handler of the innermost enclosing try region")
                && error.span == at(16)
        }),
        "{errors:?}"
    );
}

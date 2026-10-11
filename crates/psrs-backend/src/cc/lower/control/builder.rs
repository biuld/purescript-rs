//! CC construction helpers for steps over predeclared machine layouts.

use super::{ControlFunctions, ControlLayout};
use crate::cc::lower::FunctionLowerer;
use crate::cc::payload::erased_shape;
use crate::cc::{
    Assignment, AssignmentKind, Function, RefShape, Reference, ReprId, SignatureId, TagCase,
    ValueId, ValueShape,
};
use psrs_hir::SymbolId;
use psrs_span::TextRange;

pub(super) struct Machine<'a> {
    pub lower: FunctionLowerer<'a>,
    pub layout: ControlLayout,
    pub functions: ControlFunctions,
    pub span: TextRange,
    pub context: Option<ValueId>,
    pub boundary: Option<ValueId>,
}

pub(super) fn repr(id: ReprId) -> ValueShape {
    reference(RefShape::Repr(id))
}

fn reference(heap: RefShape) -> ValueShape {
    ValueShape::Reference(Reference {
        nullable: false,
        heap,
    })
}

impl<'a> Machine<'a> {
    pub fn new(
        lower: FunctionLowerer<'a>,
        layout: ControlLayout,
        functions: ControlFunctions,
        span: TextRange,
    ) -> Self {
        Self {
            lower,
            layout,
            functions,
            span,
            context: None,
            boundary: None,
        }
    }

    pub fn emit(
        &mut self,
        shape: ValueShape,
        body: &mut Vec<Assignment>,
        kind: impl FnOnce(ValueId) -> AssignmentKind,
    ) -> ValueId {
        let destination = self.lower.fresh(shape);
        body.push(Assignment {
            destination,
            kind: kind(destination),
            span: self.span,
        });
        destination
    }

    pub fn cast(
        &mut self,
        value: ValueId,
        shape: ValueShape,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let ValueShape::Reference(reference) = shape else {
            unreachable!("machine casts only references");
        };
        self.emit(shape, body, |destination| {
            AssignmentKind::RepresentationCast {
                destination,
                value,
                reference,
            }
        })
    }

    pub fn node(
        &mut self,
        representation: ReprId,
        case: u32,
        fields: Vec<ValueId>,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let value = self.emit(repr(representation), body, |destination| {
            AssignmentKind::VariantNew {
                destination,
                representation,
                case,
                fields,
            }
        });
        self.cast(value, erased_shape(), body)
    }

    pub fn field(
        &mut self,
        value: ValueId,
        representation: ReprId,
        case: u32,
        field: u32,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let value = self.cast(value, repr(representation), body);
        self.emit(erased_shape(), body, |destination| {
            AssignmentKind::VariantGet {
                destination,
                representation,
                case,
                field,
                value,
            }
        })
    }

    pub fn pair(&mut self, state: ValueId, value: ValueId, body: &mut Vec<Assignment>) -> ValueId {
        let representation = self.layout.pair;
        let value = self.emit(repr(representation), body, |destination| {
            AssignmentKind::ProductNew {
                destination,
                representation,
                arguments: vec![state, value],
            }
        });
        self.cast(value, erased_shape(), body)
    }

    pub fn pair_field(
        &mut self,
        value: ValueId,
        field: u32,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let representation = self.layout.pair;
        let value = self.cast(value, repr(representation), body);
        self.emit(erased_shape(), body, |destination| {
            AssignmentKind::ProductGet {
                destination,
                representation,
                field,
                value,
            }
        })
    }

    pub fn capture(&mut self, closure: ValueId, index: u32, body: &mut Vec<Assignment>) -> ValueId {
        self.emit(erased_shape(), body, |_| {
            AssignmentKind::ClosureGetCapture { closure, index }
        })
    }

    pub fn closure(
        &mut self,
        function: SymbolId,
        signature: SignatureId,
        captures: Vec<ValueId>,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let value = self.emit(reference(RefShape::Closure(signature)), body, |_| {
            AssignmentKind::FunctionRef {
                function,
                signature,
                captures,
            }
        });
        self.cast(value, erased_shape(), body)
    }

    /// Use the common curry protocol; every intermediate result is erased.
    pub fn apply(
        &mut self,
        mut function: ValueId,
        arguments: &[ValueId],
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let signature = self.layout.continuation;
        for argument in arguments {
            let callable = self.cast(function, reference(RefShape::Closure(signature)), body);
            function = self.emit(erased_shape(), body, |_| AssignmentKind::IndirectCall {
                function: callable,
                signature,
                arguments: vec![*argument],
            });
        }
        function
    }

    pub fn recur(
        &mut self,
        state: ValueId,
        handler: ValueId,
        computation: ValueId,
        sequence: ValueId,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let function = self.functions.executor;
        let arguments = vec![
            state,
            handler,
            computation,
            sequence,
            self.boundary.unwrap(),
            self.context.unwrap(),
        ];
        self.emit(erased_shape(), body, |_| AssignmentKind::DirectCall {
            function,
            arguments,
        })
    }

    pub fn switch(
        &mut self,
        value: ValueId,
        representation: ReprId,
        cases: Vec<TagCase>,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let value = self.cast(value, repr(representation), body);
        let tag = self.emit(ValueShape::Integer, body, |destination| {
            AssignmentKind::VariantTag {
                destination,
                representation,
                value,
            }
        });
        let trap = self.lower.fresh(erased_shape());
        let default_assignments = vec![Assignment {
            destination: trap,
            kind: AssignmentKind::Unreachable,
            span: self.span,
        }];
        self.emit(erased_shape(), body, |_| AssignmentKind::TagSwitch {
            value: tag,
            cases,
            default_assignments,
            default_value: trap,
        })
    }

    pub fn finish(
        self,
        symbol: SymbolId,
        name: &str,
        parameters: Vec<ValueId>,
        assignments: Vec<Assignment>,
        result: ValueId,
    ) -> Function {
        Function {
            symbol,
            name: name.into(),
            parameters,
            assignments,
            result,
            result_type: erased_shape(),
            values: self.lower.values,
            span: self.span,
        }
    }
}

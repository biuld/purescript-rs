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
        if self.value_shape(value) == shape {
            return value;
        }
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
        let fields = self
            .lower
            .control_fields(representation, case, fields, self.span, body)
            .expect("machine construction must match its predeclared frame schema");
        self.emit(repr(representation), body, |destination| {
            AssignmentKind::VariantNew {
                destination,
                representation,
                case,
                fields,
            }
        })
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
        let shape = self.lower.control_field_shape(representation, case, field);
        self.emit(shape, body, |destination| AssignmentKind::VariantGet {
            destination,
            representation,
            case,
            field,
            value,
        })
    }

    pub fn pair(&mut self, state: ValueId, value: ValueId, body: &mut Vec<Assignment>) -> ValueId {
        let state = self.cast(state, erased_shape(), body);
        let value = self.cast(value, erased_shape(), body);
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

    pub fn capture(
        &mut self,
        closure: ValueId,
        index: u32,
        shape: ValueShape,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        self.emit(shape, body, |_| AssignmentKind::ClosureGetCapture {
            closure,
            index,
        })
    }

    pub fn closure(
        &mut self,
        function: SymbolId,
        signature: SignatureId,
        captures: Vec<ValueId>,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        self.emit(reference(RefShape::Closure(signature)), body, |_| {
            AssignmentKind::FunctionRef {
                function,
                signature,
                captures,
            }
        })
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
            let argument = self.cast(*argument, erased_shape(), body);
            let callable = self.cast(function, reference(RefShape::Closure(signature)), body);
            function = self.emit(erased_shape(), body, |_| AssignmentKind::IndirectCall {
                function: callable,
                signature,
                arguments: vec![argument],
            });
        }
        function
    }

    pub fn call(
        &mut self,
        function: SymbolId,
        arguments: Vec<ValueId>,
        body: &mut Vec<Assignment>,
    ) -> ValueId {
        let signature = self.functions.signature(self.layout, function);
        self.lower
            .control_call(function, signature, arguments, self.span, body)
            .expect("generated call must match its predeclared signature")
    }

    pub fn value_shape(&self, value: ValueId) -> ValueShape {
        self.lower
            .values
            .iter()
            .find(|declaration| declaration.id == value)
            .map(|declaration| declaration.ty)
            .expect("declared machine value")
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
        let shape = self.value_shape(cases.first().expect("machine switch has cases").value);
        let trap = self.lower.fresh(shape);
        let default_assignments = vec![Assignment {
            destination: trap,
            kind: AssignmentKind::Unreachable,
            span: self.span,
        }];
        self.emit(shape, body, |_| AssignmentKind::TagSwitch {
            value: tag,
            cases,
            default_assignments,
            default_value: trap,
        })
    }

    pub fn finish(
        mut self,
        symbol: SymbolId,
        name: &str,
        parameters: Vec<ValueId>,
        mut assignments: Vec<Assignment>,
        result: ValueId,
    ) -> Function {
        let result_type = if symbol == self.functions.resume {
            erased_shape()
        } else {
            self.lower
                .representations
                .signature(self.functions.signature(self.layout, symbol))
                .expect("predeclared machine signature")
                .result
        };
        let result = self
            .lower
            .control_values(vec![result], vec![result_type], self.span, &mut assignments)
            .expect("machine result must match its predeclared signature")[0];
        Function {
            symbol,
            name: name.into(),
            parameters,
            assignments,
            result,
            result_type,
            values: self.lower.values,
            span: self.span,
        }
    }
}

//! Type verification of the `String`/`Array Int` byte conversions.
//!
//! `stringToBytes` reads a `String` and produces `Array Int`; `bytesToString`
//! reads `Array Int` and produces a `String`. Both element and result types are
//! fixed by the operation, so a mismatch is a lowering bug rather than a source
//! error ([DEC-16](../../../../../decision/DEC-16-scalar-strings-and-utf8-storage.md)).

use super::super::types::primitive_type_id;
use super::Context;
use crate::{Expr, TypeConstructor};

impl Context<'_> {
    pub(super) fn verify_string_to_bytes(&mut self, expression: &Expr, value: &Expr) {
        self.expr(
            value,
            Some(primitive_type_id(self.module, TypeConstructor::String)),
        );
        self.array_of_ints(expression, "stringToBytes expects an Array Int result");
    }

    pub(super) fn verify_bytes_to_string(&mut self, expression: &Expr, value: &Expr) {
        self.array_of_ints(value, "bytesToString expects an Array Int value");
        self.shape(expression, TypeConstructor::String);
    }
}

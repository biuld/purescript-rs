use super::*;
use crate::cc::{BinaryOp, ValueShape};
use std::collections::HashMap;

#[test]
fn string_equality_requires_two_strings_and_returns_boolean() {
    let valid =
        binary_operation_function(BinaryOp::StringEq, ValueShape::String, ValueShape::Boolean);
    assert!(verify_function(&valid, &HashMap::new(), &table()).is_ok());

    let wrong_operand =
        binary_operation_function(BinaryOp::StringEq, ValueShape::Integer, ValueShape::Boolean);
    assert!(verify_function(&wrong_operand, &HashMap::new(), &table()).is_err());

    let wrong_result =
        binary_operation_function(BinaryOp::StringEq, ValueShape::String, ValueShape::Integer);
    assert!(verify_function(&wrong_result, &HashMap::new(), &table()).is_err());
}

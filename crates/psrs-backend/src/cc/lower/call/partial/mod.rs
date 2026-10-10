use super::super::super::layout::{function_arrow_parameters, function_type_signature};
use super::super::super::{
    Assignment, AssignmentKind, Function, RefShape, Reference, SignatureId, ValueConversion,
    ValueId,
};
use super::super::lambda::LambdaLowering;
use super::super::{FunctionLowerer, Signature, ValueShape};
use super::helpers::{
    callable_instantiation, callable_parameter_types, callable_result_type, closure_value_type,
    closure_value_type_for, conversion_reconstructs_aggregate, function_result_type,
    persist_reference, restore_reference,
};
use crate::BackendError;
use psrs_core::Expr;
use psrs_core::TypeId;
use psrs_hir::SymbolId;

pub(super) struct PartialApplication<'a> {
    pub(super) expression: &'a Expr,
    pub(super) function: SymbolId,
    pub(super) source_signature: &'a Signature,
    pub(super) arguments: Vec<&'a Expr>,
    pub(super) result_type: ValueShape,
    pub(super) callable_type: TypeId,
}

/// An under-applied callee that is not a top-level declaration: a local
/// closure or a dictionary method reached through a field access. The supplied
/// arguments are captured and the remaining parameters are exposed by a
/// generated closure that calls the callee indirectly.
pub(super) struct IndirectPartialApplication<'a> {
    pub(super) expression: &'a Expr,
    pub(super) head: &'a Expr,
    pub(super) arguments: Vec<&'a Expr>,
    pub(super) signature: &'a Signature,
    pub(super) signature_id: SignatureId,
    pub(super) result_type: ValueShape,
}

mod global;
mod indirect;

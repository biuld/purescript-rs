use super::super::{Locals, SchemeType};
use super::Context;
use crate::{Expr, Module, TypeId, VerifyError};
use psrs_hir::{ModuleId, SymbolId};
use std::collections::HashMap;

#[allow(clippy::too_many_arguments)]
pub(in crate::verify) fn verify_expr(
    expression: &Expr,
    expected: Option<TypeId>,
    module: &Module,
    source: Option<&Module>,
    owner: ModuleId,
    globals: &HashMap<SymbolId, Option<SchemeType>>,
    locals: &mut Locals,
    errors: &mut Vec<VerifyError>,
) {
    let mut context = Context {
        module,
        source,
        owner,
        globals,
        locals,
        errors,
    };
    context.expr(expression, expected);
}

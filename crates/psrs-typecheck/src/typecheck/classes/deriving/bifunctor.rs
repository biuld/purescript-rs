use super::super::super::*;
use super::KnownClass;

impl Checker {
    pub(super) fn derive_bifunctor_method(
        &mut self,
        method: &MethodInfo,
        class_arguments: &[InferType],
        span: TextRange,
    ) -> Option<InferredExpr> {
        self.derive_mapping_method(KnownClass::Bifunctor, method, class_arguments, span)
    }
}

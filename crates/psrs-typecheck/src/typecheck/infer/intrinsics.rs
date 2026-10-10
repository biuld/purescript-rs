use super::super::{Checker, ClassConstraint, InferType, InferredExprKind, arrow};
use psrs_hir::Intrinsic;
use psrs_span::TextRange;

impl Checker {
    pub(super) fn coercion_function(&mut self, span: TextRange) -> (InferredExprKind, InferType) {
        let source = self.fresh();
        let target = self.fresh();
        let constraint = ClassConstraint {
            class_id: psrs_hir::TypeId::COERCIBLE,
            arguments: vec![source.clone(), target.clone()],
            span,
        };
        let dictionary_type = self.dictionary_type(&constraint);
        let wanted = self.push_wanted(constraint, dictionary_type);
        (
            InferredExprKind::CoerceFunction {
                wanted,
                source: source.clone(),
                target: target.clone(),
            },
            arrow(source, target),
        )
    }

    /// `Unsafe.Coerce.unsafeCoerce`: the same value shape as `coerce` with no
    /// `Coercible` wanted. Its result type is fixed by the context.
    pub(super) fn unsafe_coercion_function(
        &mut self,
        _span: TextRange,
    ) -> (InferredExprKind, InferType) {
        let source = self.fresh();
        let target = self.fresh();
        (
            InferredExprKind::UnsafeCoerceFunction {
                source: source.clone(),
                target: target.clone(),
                origin: psrs_thir::UncheckedCoercionOrigin::UnsafeCoerce,
            },
            arrow(source, target),
        )
    }

    /// The type of an intrinsic, instantiated from its descriptor's scheme.
    ///
    /// `Coerce` and `UnsafeCoerce` are handled by the caller before this point,
    /// because they need a distinct expression node (and, for `Coerce`, a
    /// `Coercible` wanted). `Prim.undefined`'s scheme `forall a. a` needs no
    /// special case: instantiating it yields the fresh variable the use decides.
    pub(super) fn intrinsic_type(&mut self, intrinsic: Intrinsic) -> InferType {
        let descriptor = intrinsic.descriptor();
        self.elaborate_imported_signature(&(descriptor.scheme)())
    }
}

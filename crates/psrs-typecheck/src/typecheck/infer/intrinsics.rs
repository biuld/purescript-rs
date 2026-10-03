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

    /// The type of an intrinsic, instantiated from its descriptor's scheme.
    ///
    /// `Coerce` is handled by the caller before this point, because it needs a
    /// `Coercible` wanted and a distinct expression node. `Prim.undefined`'s
    /// scheme `forall a. a` needs no special case: instantiating it yields the
    /// fresh variable the use decides.
    pub(super) fn intrinsic_type(&mut self, intrinsic: Intrinsic) -> InferType {
        let descriptor = intrinsic.descriptor();
        self.elaborate_imported_signature(&(descriptor.scheme)())
    }
}

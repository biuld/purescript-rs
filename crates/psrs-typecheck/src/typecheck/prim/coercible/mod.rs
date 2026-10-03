//! The `Prim.Coerce.Coercible` rule: the one `Proof` member, and the one
//! member whose evidence is a compile-time boundary rather than a dictionary.
//!
//! It is entered from the rule table by class identity and *before* the lexical
//! givens, because nothing in scope discharges a proof: a `Coercible` given
//! supplies an assumption that two types are convertible, while this rule
//! *derives* the conversion from role analysis, equality, the visible newtypes,
//! and those same givens. Deriving it first is what official solving does —
//! `forClassNameM` tries `solveCoercible` before falling back to the instance
//! lookup — and it is what lets a `coerce` boundary carry an explicit proof
//! rather than a dictionary parameter, which THIR verifies requires.
//!
//! The mechanism reads roles through the checked kind environment and reads
//! kinds through the one kind solver, so it shares the foundations every other
//! rule will share: no private kind table, no private row representation, and no
//! private reader for source syntax.

use super::{EvidenceClass, PrimitiveArgs, PrimitiveEvidence, PrimitiveOutcome, PrimitiveRule};
use crate::typecheck::*;

mod givens;

/// `Coercible`'s entry in the rule table.
pub(in crate::typecheck) const RULE: PrimitiveRule = PrimitiveRule {
    class_id: hir::TypeId::COERCIBLE,
    evidence: EvidenceClass::CompileTimeProof,
    arity: 2,
    solve,
};

/// Whether `Coercible a b` is derivable from what is known now.
///
/// This member has exactly one answer shape, which is why its evidence class is
/// what the dispatch order is keyed on rather than a coincidence of its
/// identity: a proof obligation is either derived from the role analysis or it
/// is not, so there is no partial answer to record and no further obligation to
/// emit. `Undecided` is therefore this rule's only alternative to `Solved`, and
/// it is the honest one — an argument that is still an unsolved inference
/// variable yields `Undecided`, not `Failed`, because a convertible pair is a
/// claim about determined types and reporting it as impossible while a type is
/// unknown is the conflation the four-outcome contract exists to prevent.
/// `Undecided` sends the obligation into instance search, which is where an
/// assumption about the unknown types would be found.
fn solve(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [source, target] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    if !checker.proves_coercible(source, target, args.span()) {
        return PrimitiveOutcome::Undecided;
    }
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Proof {
            source: source.clone(),
            target: target.clone(),
        },
        deferred: Vec::new(),
    }
}

impl Checker {
    pub(super) fn proves_coercible(
        &mut self,
        source: &InferType,
        target: &InferType,
        span: TextRange,
    ) -> bool {
        self.proves_coercible_inner(source, target, span, 0, &mut HashSet::new())
    }

    fn proves_coercible_inner(
        &mut self,
        source: &InferType,
        target: &InferType,
        span: TextRange,
        depth: usize,
        path: &mut HashSet<(String, String)>,
    ) -> bool {
        if depth > 64 {
            return false;
        }
        let source = self.resolve_type(source.clone());
        let target = self.resolve_type(target.clone());
        if !self.coercion_kinds_compatible(&source, &target, span) {
            return false;
        }
        if self.infer_types_equal(&source, &target) || self.given_coercible(&source, &target) {
            return true;
        }
        match (&source, &target) {
            (InferType::RowEmpty, InferType::RowEmpty) => return true,
            (
                InferType::RowExtend {
                    label: source_label,
                    ty: source_field,
                    tail: source_tail,
                },
                InferType::RowExtend {
                    label: target_label,
                    ty: target_field,
                    tail: target_tail,
                },
            ) if source_label == target_label => {
                let mut field_path = path.clone();
                let mut tail_path = path.clone();
                return self.proves_coercible_inner(
                    source_field,
                    target_field,
                    span,
                    depth + 1,
                    &mut field_path,
                ) && self.proves_coercible_inner(
                    source_tail,
                    target_tail,
                    span,
                    depth + 1,
                    &mut tail_path,
                );
            }
            (InferType::RowExtend { .. }, _)
            | (_, InferType::RowExtend { .. })
            | (InferType::RowEmpty, _)
            | (_, InferType::RowEmpty) => return false,
            _ => {}
        }
        let key = (format!("{source:?}"), format!("{target:?}"));
        if !path.insert(key.clone()) {
            return false;
        }

        if let Some(underlying) = self.unwrap_visible_newtype(&source) {
            let mut nested = path.clone();
            if self.proves_coercible_inner(&underlying, &target, span, depth + 1, &mut nested) {
                return true;
            }
        }
        if let Some(underlying) = self.unwrap_visible_newtype(&target) {
            let mut nested = path.clone();
            if self.proves_coercible_inner(&source, &underlying, span, depth + 1, &mut nested) {
                return true;
            }
        }

        let (source_head, source_arguments) = flatten_infer_spine(&source);
        let (target_head, target_arguments) = flatten_infer_spine(&target);
        let (
            InferType::Constructor(source_constructor),
            InferType::Constructor(target_constructor),
        ) = (source_head, target_head)
        else {
            return false;
        };
        if source_constructor != target_constructor
            || source_arguments.len() != target_arguments.len()
        {
            return false;
        }
        let roles = self.roles_for_constructor(*source_constructor, source_arguments.len());
        if roles.len() != source_arguments.len() {
            return false;
        }
        for ((source_argument, target_argument), role) in
            source_arguments.iter().zip(&target_arguments).zip(roles)
        {
            let coercible = match role {
                hir::Role::Nominal => self.infer_types_equal(source_argument, target_argument),
                hir::Role::Representational => {
                    let mut nested = path.clone();
                    self.proves_coercible_inner(
                        source_argument,
                        target_argument,
                        span,
                        depth + 1,
                        &mut nested,
                    )
                }
                hir::Role::Phantom => true,
            };
            if !coercible {
                return false;
            }
        }
        true
    }

    fn roles_for_constructor(&self, constructor: TypeConstructor, arity: usize) -> Vec<hir::Role> {
        match constructor {
            TypeConstructor::Array | TypeConstructor::Record => {
                vec![hir::Role::Representational; arity]
            }
            TypeConstructor::Function => vec![hir::Role::Representational; arity],
            TypeConstructor::User(id) => self
                .env
                .checked_kinds
                .roles(id)
                .map(<[hir::Role]>::to_vec)
                .unwrap_or_else(|| vec![hir::Role::Nominal; arity]),
            TypeConstructor::Int
            | TypeConstructor::Number
            | TypeConstructor::Boolean
            | TypeConstructor::String
            | TypeConstructor::Char
            | TypeConstructor::Unit
            | TypeConstructor::Type
            | TypeConstructor::Constraint
            | TypeConstructor::Symbol => Vec::new(),
            // Official PureScript declares `Prim.Row` with a phantom role.
            TypeConstructor::Row => vec![hir::Role::Phantom; arity],
        }
    }

    fn unwrap_visible_newtype(&mut self, ty: &InferType) -> Option<InferType> {
        let (head, arguments) = flatten_infer_spine(ty);
        let InferType::Constructor(TypeConstructor::User(id)) = head else {
            return None;
        };
        if !self.env.visible_newtypes.contains(id) {
            return None;
        }
        let declaration = self.env.type_declarations.get(id)?.clone();
        if declaration.kind != hir::TypeDeclarationKind::Newtype
            || arguments.len() != declaration.parameters.len()
        {
            return None;
        }
        let constructor = declaration.constructors.first()?;
        if constructor.fields.len() != 1 {
            return None;
        }
        let field = constructor.fields[0].clone();
        let mut variables = declaration
            .parameters
            .iter()
            .map(|parameter| parameter.name.clone())
            .zip(arguments)
            .collect::<HashMap<_, _>>();
        Some(self.elaborate_type(&field, &mut variables))
    }
}

pub(super) fn flatten_infer_spine(ty: &InferType) -> (&InferType, Vec<InferType>) {
    let mut head = ty;
    let mut arguments = Vec::new();
    while let InferType::Application(function, argument) = head {
        arguments.push((**argument).clone());
        head = function;
    }
    arguments.reverse();
    (head, arguments)
}

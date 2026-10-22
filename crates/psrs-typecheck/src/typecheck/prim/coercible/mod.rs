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
use crate::typecheck::kind::CoercionKinds;
use crate::typecheck::*;

mod givens;
mod rows;

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
/// emit. An argument that is still an unsolved inference variable yields
/// `Undecided`, not `Failed`, because a convertible pair is a claim about
/// determined types and reporting it as impossible while a type is unknown is
/// the conflation the four-outcome contract exists to prevent. `Undecided`
/// sends the obligation into instance search, which is where an assumption
/// about the unknown types would be found.
///
/// `Failed` is the answer official solving reports itself. Its
/// `Entailment/Coercible.hs` unifies the kinds of a picked-up constraint and
/// raises `KindsDoNotUnify` when they disagree. `decompose` raises
/// `TypesDoNotUnify` for a nominal argument that is not equal, and `canonRow`
/// raises it when closed rows do not align. A flexible unknown, a
/// representational mismatch, and two different type constructors stay
/// `Undecided`, so instance search can still discharge them and otherwise
/// reports `NoInstanceFound`.
/// The answers one coercion trial can record.
///
/// A kind disagreement is `KindsDoNotUnify`. A determined type disagreement is
/// `TypesDoNotUnify`. Both stay on this trial so a declined newtype unwrap can
/// restore the type answer without a second side channel. `infinite` is set
/// when decomposition re-enters a wanted already on the ancestor path, which
/// is the `PossiblyInfiniteCoercibleInstance` loop. `from_decomposition`
/// distinguishes that loop from a direct self-wrap, whose unwrap is declined
/// so ordinary decomposition can still succeed.
pub(in crate::typecheck::prim::coercible) struct CoercionVerdict<'a> {
    pub(in crate::typecheck::prim::coercible) kind: &'a mut Option<psrs_kind::KindError>,
    pub(in crate::typecheck::prim::coercible) types: &'a mut Option<(InferType, InferType)>,
    pub(in crate::typecheck::prim::coercible) infinite: &'a mut bool,
    from_decomposition: bool,
}

fn solve(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    let [source, target] = arguments.as_slice() else {
        return PrimitiveOutcome::Undecided;
    };
    let mut kind_conflict = None;
    let mut type_conflict = None;
    let mut infinite = false;
    if !checker.proves_coercible(
        source,
        target,
        args.span(),
        &mut kind_conflict,
        &mut type_conflict,
        &mut infinite,
    ) {
        return match (kind_conflict, infinite, type_conflict) {
            (Some(error), _, _) => PrimitiveOutcome::Failed {
                code: TypeCheckErrorKind::KindsDoNotUnify,
                detail: error.message.to_owned(),
            },
            (None, true, _) => PrimitiveOutcome::Failed {
                code: TypeCheckErrorKind::PossiblyInfiniteCoercibleInstance,
                detail: "possibly infinite Coercible instance".to_owned(),
            },
            (None, false, Some((left, right))) => {
                let left = checker.display_type(&left);
                let right = checker.display_type(&right);
                PrimitiveOutcome::Failed {
                    code: TypeCheckErrorKind::TypeMismatch,
                    detail: format!("type mismatch: expected {left}, found {right}"),
                }
            }
            (None, false, None) => PrimitiveOutcome::Undecided,
        };
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
        kind_conflict: &mut Option<psrs_kind::KindError>,
        type_conflict: &mut Option<(InferType, InferType)>,
        infinite: &mut bool,
    ) -> bool {
        let mut verdict = CoercionVerdict {
            kind: kind_conflict,
            types: type_conflict,
            infinite,
            from_decomposition: false,
        };
        self.proves_coercible_inner(source, target, span, 0, &mut HashSet::new(), &mut verdict)
    }

    pub(in crate::typecheck::prim::coercible) fn proves_coercible_inner(
        &mut self,
        source: &InferType,
        target: &InferType,
        span: TextRange,
        depth: usize,
        path: &mut HashSet<(String, String)>,
        verdict: &mut CoercionVerdict<'_>,
    ) -> bool {
        if depth > 64 {
            return false;
        }
        let source = self.resolve_type(source.clone());
        let target = self.resolve_type(target.clone());
        // Reflexivity does not depend on learning a kind: identical types
        // necessarily have the same representation, even while their kind is
        // still being inferred.
        if self.infer_types_equal(&source, &target) {
            return true;
        }
        // A constrained function is represented as one dictionary argument per
        // constraint followed by its body. When the constraints are identical,
        // the dictionary prefix has the same representation on both sides and
        // the body can be checked using the ordinary coercion rules.
        if let (
            InferType::Constrained {
                constraints: source_constraints,
                body: source_body,
            },
            InferType::Constrained {
                constraints: target_constraints,
                body: target_body,
            },
        ) = (&source, &target)
        {
            if !constraints_equal(source_constraints, target_constraints, self) {
                // Constraints are part of the representation prefix, so
                // different dictionaries cannot be skipped.
                return false;
            }
            let mut nested = path.clone();
            return self.proves_coercible_inner(
                source_body,
                target_body,
                span,
                depth + 1,
                &mut nested,
                verdict,
            );
        }
        // The kinds the pair would have to share. Official solving checks a
        // pair's kinds exactly here, when it picks a constraint up: the shared
        // solver holds kinds neither binding nor derivation invented, so a
        // conflict it finds is the constraint's answer, not a side finding. The
        // verdict is remembered for this proof attempt and reported once, from
        // the outermost pair — the first constraint official solving would have
        // picked up — and only for a genuine `KindsDoNotUnify` verdict: a kind
        // that is not derivable is the kind pass's diagnostic, and this trial
        // must not read it as a coercion verdict either way.
        //
        // The verdict stays a trial about the solver state, so a route this
        // attempt declines can still leave no binding behind; the report is
        // re-derived in one step in `solve`, through `Failed`.
        match self.coercion_kinds_trial(&source, &target, span) {
            CoercionKinds::Compatible => {}
            CoercionKinds::Mismatch(error) => {
                if verdict.kind.is_none() {
                    *verdict.kind = Some(error);
                }
                return false;
            }
            CoercionKinds::Undetermined => return false,
        }
        if self.given_coercible(&source, &target) {
            return true;
        }
        if matches!(
            (&source, &target),
            (InferType::RowEmpty | InferType::RowExtend { .. }, _)
                | (_, InferType::RowEmpty | InferType::RowExtend { .. })
        ) {
            return self.prove_concrete_rows(&source, &target, span, depth, path, verdict);
        }
        let key = (format!("{source:?}"), format!("{target:?}"));
        if !path.insert(key.clone()) {
            // Re-entering an ancestor from role decomposition is the solver
            // loop `solveWanteds` bounds with `PossiblyInfiniteCoercibleInstance`.
            // The same pair reached by unwrapping a direct self-wrap is not:
            // that unwrap is declined and decomposition still runs.
            if verdict.from_decomposition {
                *verdict.infinite = true;
            }
            return false;
        }

        // Unwrapping is speculative. Official commits to it only when the
        // constructor unwraps; a failed attempt must not leak a nominal
        // disagreement from the underlying type into a later decomposition.
        // A loop found under the unwrap is the answer, so it is not retried.
        let saved_conflict = verdict.types.clone();
        if let Some(underlying) = self.unwrap_visible_newtype(&source) {
            let mut nested = path.clone();
            if self.proves_coercible_inner(
                &underlying,
                &target,
                span,
                depth + 1,
                &mut nested,
                verdict,
            ) {
                return true;
            }
            let looping = *verdict.infinite;
            *verdict.types = saved_conflict;
            if looping {
                return false;
            }
        }
        let saved_conflict = verdict.types.clone();
        if let Some(underlying) = self.unwrap_visible_newtype(&target) {
            let mut nested = path.clone();
            if self.proves_coercible_inner(
                &source,
                &underlying,
                span,
                depth + 1,
                &mut nested,
                verdict,
            ) {
                return true;
            }
            let looping = *verdict.infinite;
            *verdict.types = saved_conflict;
            if looping {
                return false;
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
        // Official `decompose` emits every role constraint, and a nominal
        // mismatch aborts that set. An irreducible sibling does not hide a
        // later argument that walks back to this wanted.
        let arrived_by_decomposition = verdict.from_decomposition;
        verdict.from_decomposition = true;
        let mut irreducible = false;
        for ((source_argument, target_argument), role) in
            source_arguments.iter().zip(&target_arguments).zip(roles)
        {
            let coercible = match role {
                hir::Role::Nominal => {
                    if self.infer_types_equal(source_argument, target_argument) {
                        true
                    } else {
                        // `decompose` compares nominal arguments now and raises
                        // `TypesDoNotUnify` when they differ. A flexible unknown
                        // is not that disagreement: instantiation may still
                        // make the two sides equal.
                        self.note_determined_disagreement(
                            source_argument,
                            target_argument,
                            verdict.types,
                        );
                        false
                    }
                }
                hir::Role::Representational => {
                    let mut nested = path.clone();
                    self.proves_coercible_inner(
                        source_argument,
                        target_argument,
                        span,
                        depth + 1,
                        &mut nested,
                        verdict,
                    )
                }
                hir::Role::Phantom => true,
            };
            if verdict.kind.is_some() || verdict.types.is_some() {
                *verdict.infinite = false;
                verdict.from_decomposition = arrived_by_decomposition;
                return false;
            }
            if !coercible && !*verdict.infinite {
                irreducible = true;
            }
        }
        verdict.from_decomposition = arrived_by_decomposition;
        !irreducible && !*verdict.infinite
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

    /// Records `TypesDoNotUnify` for two determined types. A flexible unknown
    /// stays undecided so a later instantiation can still make them equal.
    pub(in crate::typecheck::prim::coercible) fn note_determined_disagreement(
        &self,
        left: &InferType,
        right: &InferType,
        type_conflict: &mut Option<(InferType, InferType)>,
    ) {
        if type_conflict.is_some() || self.mentions_flexible(left) || self.mentions_flexible(right)
        {
            return;
        }
        *type_conflict = Some((left.clone(), right.clone()));
    }

    fn mentions_flexible(&self, ty: &InferType) -> bool {
        self.mentions_flexible_outside(ty, &[])
    }

    fn mentions_flexible_outside(&self, ty: &InferType, bound: &[u32]) -> bool {
        match self.resolve_type(ty.clone()) {
            InferType::Variable(variable) => {
                !bound.contains(&variable) && !self.state.rigid.contains(&variable)
            }
            InferType::Application(function, argument) => {
                self.mentions_flexible_outside(&function, bound)
                    || self.mentions_flexible_outside(&argument, bound)
            }
            InferType::ForAll {
                variables, body, ..
            } => {
                let mut nested = bound.to_vec();
                nested.extend(variables);
                self.mentions_flexible_outside(&body, &nested)
            }
            InferType::Constrained { constraints, body } => {
                constraints.iter().any(|constraint| {
                    constraint
                        .arguments
                        .iter()
                        .any(|argument| self.mentions_flexible_outside(argument, bound))
                }) || self.mentions_flexible_outside(&body, bound)
            }
            InferType::RowExtend { ty, tail, .. } => {
                self.mentions_flexible_outside(&ty, bound)
                    || self.mentions_flexible_outside(&tail, bound)
            }
            InferType::Constructor(_)
            | InferType::RowEmpty
            | InferType::TypeLevelString(_)
            | InferType::TypeLevelInt(_) => false,
        }
    }
}

fn constraints_equal(
    left: &[ClassConstraint],
    right: &[ClassConstraint],
    checker: &Checker,
) -> bool {
    left.len() == right.len()
        && left.iter().zip(right).all(|(left, right)| {
            left.class_id == right.class_id
                && left.arguments.len() == right.arguments.len()
                && left
                    .arguments
                    .iter()
                    .zip(&right.arguments)
                    .all(|(left, right)| checker.infer_types_equal(left, right))
        })
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

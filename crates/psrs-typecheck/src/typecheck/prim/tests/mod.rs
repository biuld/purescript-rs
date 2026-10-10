//! The rule table's contract, exercised through synthetic rules and through the
//! `Prim.Symbol` relations.
//!
//! The synthetic rules stand in for a relation — they are keyed by identities the
//! registry does not declare — so the cases in `outcomes` reach each of the four
//! outcomes, prove that a declined or refused rule leaves no state behind, and
//! prove the two refusals that keep an unsolved argument from ever being decisive.
//! They are the framework's contract; a real rule is written against the same
//! contract, and `symbol_rules` is a real one.

use super::*;

mod outcomes;
mod probe;
mod symbol_rules;
use crate::typecheck::classes::SolveDepth;
use crate::typecheck::{ClassConstraint, TypeConstructor, TypecheckContext};
use psrs_hir::ModuleId;
use psrs_span::SourceFile;

/// The identity of the rule that decides everything and defers nothing.
const SOLVING: hir::TypeId = hir::TypeId::new(ModuleId(0), 900);
/// The identity of the rule that does speculative work and then declines.
const DECLINING: hir::TypeId = hir::TypeId::new(ModuleId(0), 901);
/// The identity of the rule that defers with evidence.
const DEFERRING: hir::TypeId = hir::TypeId::new(ModuleId(0), 902);
/// The identity of the rule that defers to the obligation it was asked about.
const DEFERRING_TO_ITSELF: hir::TypeId = hir::TypeId::new(ModuleId(0), 903);
/// The identity of the rule that reports the relation as impossible.
const FAILING: hir::TypeId = hir::TypeId::new(ModuleId(0), 904);
/// The identity of the rule that reports under a kind with no official code.
const FAILING_WITH_AN_INVENTED_CODE: hir::TypeId = hir::TypeId::new(ModuleId(0), 905);
/// The identity of the rule that claims a decision without making one.
const SOLVING_WITHOUT_PROGRESS: hir::TypeId = hir::TypeId::new(ModuleId(0), 906);
/// The identity of the rule that decides the known part and leaves the rest to a
/// deferred obligation, the shape `Row.Lacks` has over an open tail.
const DECIDING_AND_DEFERRING: hir::TypeId = hir::TypeId::new(ModuleId(0), 907);
/// The identity of the rule that states a decision for an argument the goal left
/// open, binding nothing itself.
const DECIDING: hir::TypeId = hir::TypeId::new(ModuleId(0), 908);
/// The identity of the rule that decides a value contradicting an argument the
/// goal already fixed, and binds nothing.
const CONTRADICTING: hir::TypeId = hir::TypeId::new(ModuleId(0), 909);
/// The identity of a rule that returns the same solved child twice.
const DEFERRING_TO_SIBLINGS: hir::TypeId = hir::TypeId::new(ModuleId(0), 910);
/// The identity of a rule that branches to fresh residuals until the shared
/// deferral-tree width bound stops it.
const GROWING_DEFERRAL: hir::TypeId = hir::TypeId::new(ModuleId(0), 911);

/// Decides the relation, recording the arguments it decided.
fn solving(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary { arguments },
        deferred: Vec::new(),
    }
}

/// Binds an argument, allocates an unknown, reports a diagnostic, and declines.
///
/// This is the shape a rule has when it has recognised a candidate it cannot use,
/// and every one of those four effects has to be undone by the framework.
fn declining(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    if let InferType::Variable(variable) = &arguments[0] {
        checker.bind_type_variable(
            *variable,
            InferType::Constructor(TypeConstructor::Int),
            args.span(),
        );
    }
    checker.fresh();
    checker.build_constraint(DECLINING, arguments, args.span());
    checker.state.errors.push(TypeCheckError::new(
        TypeCheckErrorKind::TypeMismatch,
        args.span(),
        "a candidate that did not apply",
    ));
    PrimitiveOutcome::Undecided
}

/// Defers with evidence, deferring the rest of the relation to a decided
/// obligation.
fn deferring(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    PrimitiveOutcome::Deferred {
        evidence: Some(PrimitiveEvidence::Dictionary {
            arguments: arguments.clone(),
        }),
        deferred: vec![resolved_obligation(
            checker,
            SOLVING,
            vec![int(), int()],
            args.span(),
        )],
    }
}

/// Defers to the obligation it was asked about, which is the deferral that makes
/// no progress.
fn deferring_to_itself(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    PrimitiveOutcome::Deferred {
        evidence: None,
        deferred: vec![resolved_obligation(
            checker,
            DEFERRING_TO_ITSELF,
            arguments,
            args.span(),
        )],
    }
}

/// Reports the relation as impossible.
fn failing(_checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    PrimitiveOutcome::Failed {
        code: TypeCheckErrorKind::NoInstance,
        detail: format!("the relation does not hold at {}", args.span().start),
    }
}

/// Reports under a kind that carries no official `errorCode`.
fn failing_with_an_invented_code(
    _checker: &mut Checker,
    _args: &PrimitiveArgs,
) -> PrimitiveOutcome {
    PrimitiveOutcome::Failed {
        code: TypeCheckErrorKind::FundepConflict,
        detail: "a code purs never raises".into(),
    }
}

/// Claims a relation's decision while binding nothing, leaving an argument
/// unknown, and stating no decision the goal contradicts.
fn solving_without_progress(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary {
            arguments: args.resolved(checker),
        },
        deferred: vec![resolved_obligation(
            checker,
            SOLVING,
            vec![int(), int()],
            args.span(),
        )],
    }
}

/// Binds the last argument to the second and defers an obligation about the first,
/// the shape the design gives `Row.Lacks "a" ("b" | r)`.
fn deciding_and_deferring(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    if let InferType::Variable(variable) = &arguments[2] {
        checker.bind_type_variable(*variable, arguments[1].clone(), args.span());
    }
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary {
            arguments: arguments.clone(),
        },
        deferred: vec![resolved_obligation(
            checker,
            SOLVING,
            vec![arguments[0].clone(), int()],
            args.span(),
        )],
    }
}

/// States a decision for an argument the goal left open, and binds nothing
/// itself.
///
/// This is the shape every landed relation has: the rule's dictionary carries the
/// type it decided, and the framework's own unification against the goal's
/// arguments is what binds the obligation. A rule that assigned its decision
/// directly would be doing the framework's work, and the framework's check would
/// have nothing to check.
fn deciding(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let arguments = args.resolved(checker);
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary {
            arguments: vec![boolean(), arguments[1].clone()],
        },
        deferred: Vec::new(),
    }
}

/// Decides `Boolean` for both arguments where the goal fixed them to `Int`,
/// allocates a variable, and reports nothing about the disagreement.
///
/// This is the case neither of the two refusals can see: the rule bound nothing,
/// so "did anything become more determined?" says it decided nothing, while the
/// decision it states contradicts arguments that were already known. Only the
/// unifier can tell those apart, which is why the framework runs it.
fn contradicting(checker: &mut Checker, _args: &PrimitiveArgs) -> PrimitiveOutcome {
    // An allocation the restore in `solve_primitive` would take back, so a case
    // can see whether the snapshot survived.
    checker.fresh();
    PrimitiveOutcome::Solved {
        evidence: PrimitiveEvidence::Dictionary {
            arguments: vec![boolean(), boolean()],
        },
        deferred: Vec::new(),
    }
}

/// Returns two equal residual goals, which are separate valid obligations rather
/// than a cycle: the first one must leave the active path before the second runs.
fn deferring_to_siblings(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let child = resolved_obligation(checker, SOLVING, vec![int(), int()], args.span());
    PrimitiveOutcome::Deferred {
        evidence: None,
        deferred: vec![child.clone(), child],
    }
}

/// Branches to two fresh residual obligations on every call. Their changing
/// arguments avoid cycle detection, so only the shared total-width bound stops
/// the tree.
fn growing_deferral(checker: &mut Checker, args: &PrimitiveArgs) -> PrimitiveOutcome {
    let left = checker.fresh();
    let right = checker.fresh();
    PrimitiveOutcome::Deferred {
        evidence: None,
        deferred: vec![
            resolved_obligation(checker, GROWING_DEFERRAL, vec![left, int()], args.span()),
            resolved_obligation(checker, GROWING_DEFERRAL, vec![right, int()], args.span()),
        ],
    }
}

pub(in crate::typecheck) const SYNTHETIC: [PrimitiveRule; 12] = [
    PrimitiveRule {
        class_id: SOLVING,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: solving,
    },
    PrimitiveRule {
        class_id: DECLINING,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: declining,
    },
    PrimitiveRule {
        class_id: DEFERRING,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: deferring,
    },
    PrimitiveRule {
        class_id: DEFERRING_TO_ITSELF,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: deferring_to_itself,
    },
    PrimitiveRule {
        class_id: FAILING,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: failing,
    },
    PrimitiveRule {
        class_id: FAILING_WITH_AN_INVENTED_CODE,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: failing_with_an_invented_code,
    },
    PrimitiveRule {
        class_id: SOLVING_WITHOUT_PROGRESS,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: solving_without_progress,
    },
    PrimitiveRule {
        class_id: DECIDING_AND_DEFERRING,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 3,
        solve: deciding_and_deferring,
    },
    PrimitiveRule {
        class_id: DECIDING,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: deciding,
    },
    PrimitiveRule {
        class_id: CONTRADICTING,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: contradicting,
    },
    PrimitiveRule {
        class_id: DEFERRING_TO_SIBLINGS,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: deferring_to_siblings,
    },
    PrimitiveRule {
        class_id: GROWING_DEFERRAL,
        evidence: EvidenceClass::RuntimeDictionary,
        arity: 2,
        solve: growing_deferral,
    },
];

/// The parts of solver state a declined or refused rule must leave untouched.
#[derive(Debug, PartialEq, Eq)]
struct SolverFingerprint {
    substitutions: HashMap<u32, InferType>,
    variable_kinds: HashMap<u32, Kind>,
    rigid: HashSet<u32>,
    next_variable: u32,
    next_wanted_id: u32,
    level: u32,
    errors: usize,
}

impl SolverFingerprint {
    fn of(checker: &Checker) -> Self {
        Self {
            substitutions: checker.state.substitutions.clone(),
            variable_kinds: checker.state.variable_kinds.clone(),
            rigid: checker.state.rigid.clone(),
            next_variable: checker.state.next_variable,
            next_wanted_id: checker.state.next_wanted_id,
            level: checker.state.level,
            errors: checker.state.errors.len(),
        }
    }
}

fn int() -> InferType {
    InferType::Constructor(TypeConstructor::Int)
}

fn boolean() -> InferType {
    InferType::Constructor(TypeConstructor::Boolean)
}

fn resolve_one(source: &str) -> hir::Module {
    let file = SourceFile::new("Main.purs", source);
    let (tokens, errors) = psrs_syntax::lex(file.text());
    assert!(errors.is_empty(), "lex errors: {errors:?}");
    let layout = psrs_syntax::add_layout(&file, &tokens);
    let cst = psrs_syntax::parse_module(&layout).expect("parse");
    let ast = psrs_ast::lower_module(cst).expect("surface lowering");
    psrs_resolve::resolve_module_with_externals(
        ast,
        ModuleId(0),
        &psrs_resolve::bootstrap_externals(),
    )
    .expect("the program should resolve")
}

/// A checker over an empty module, for exercising a rule directly.
fn checker() -> Checker {
    let program = [resolve_one("module Main where\n")];
    Checker::new(
        &program[0],
        &HashMap::new(),
        TypecheckContext {
            known_types: &[],
            known_values: &[],
            imported_instances: &[],
            module_names: &HashMap::from([(ModuleId(0), "Main".to_owned())]),
            checked_kinds: &psrs_kind::CheckedKindEnv::default(),
        },
    )
}

/// A wanted obligation over `arguments`, with the range a rule's evidence and
/// every diagnostic about it will carry.
fn obligation(
    checker: &mut Checker,
    class_id: hir::TypeId,
    arguments: Vec<InferType>,
) -> WantedConstraint {
    let constraint = ClassConstraint {
        class_id,
        arguments,
        span: TextRange::new(12, 20),
    };
    let dictionary_type = checker.dictionary_type(&constraint);
    WantedConstraint {
        id: checker.fresh_wanted_id(),
        class_id,
        arguments: constraint.arguments,
        dictionary_type,
        span: constraint.span,
        report_span: constraint.span,
        givens: Vec::new(),
        solution: None,
    }
}

/// The same, with every argument already read through the substitution, which is
/// what a rule receives after improvement has run again.
fn resolved_obligation(
    checker: &mut Checker,
    class_id: hir::TypeId,
    arguments: Vec<InferType>,
    span: TextRange,
) -> WantedConstraint {
    let mut constraint = obligation(checker, class_id, arguments);
    constraint.span = span;
    constraint.report_span = span;
    constraint.arguments = constraint
        .arguments
        .iter()
        .map(|argument| checker.resolve_type(argument.clone()))
        .collect();
    constraint
}

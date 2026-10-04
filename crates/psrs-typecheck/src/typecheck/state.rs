//! The three owners of inference state, and the operations that enter, leave,
//! or roll back each of them.
//!
//! `SemanticEnv` is immutable for the module being checked: imported
//! signatures, visible instances, the checked kind environment, the class and
//! constructor tables, synonyms, and type names. `InferState` is the mutable
//! solver: substitutions, levels, the kind state, wanted constraints, solved
//! evidence, and diagnostics. `Scope` is the lexical chain: globals, locals,
//! givens, and annotation variables.
//!
//! `Checker` owns all three and is what inference code talks to. Nothing else
//! saves or restores state: [`Checker::speculate`] is the only operation that
//! rolls the solver back, so a feature cannot save a different subset of fields
//! than its neighbour, and [`InferState::snapshot`] is the only way a snapshot
//! is taken.

use super::*;

/// The read-only semantic inputs of one module being checked.
///
/// It is built once from the program-wide checked kind environment and the
/// module's own declarations, and is read-only afterwards. Inference never
/// reconstructs a kind, a role, or an instance head from surface syntax: it
/// reads the environment the kind pass published.
pub(super) struct SemanticEnv {
    /// Identity of the module currently being checked. It scopes instance
    /// visibility: a local instance is always a candidate, and an imported
    /// instance is a candidate only when its defining module is the class's
    /// module or a module of a type constructor in the wanted arguments.
    pub(super) module_id: hir::ModuleId,
    pub(super) external_kinds: HashMap<SymbolId, ExternalKind>,
    pub(super) external_signatures: HashMap<SymbolId, hir::Type>,
    /// Declared types of values imported from other modules, keyed by the
    /// exporting declaration's symbol.
    pub(super) imported: HashMap<SymbolId, hir::Type>,
    pub(super) type_names: HashMap<hir::TypeId, String>,
    pub(super) type_modules: HashMap<hir::TypeId, String>,
    pub(super) type_declarations: HashMap<hir::TypeId, hir::TypeDeclaration>,
    pub(super) visible_newtypes: HashSet<hir::TypeId>,
    pub(super) synonyms: HashMap<hir::TypeId, Synonym>,
    pub(super) checked_kinds: CheckedKindEnv,
    pub(super) constructor_info: HashMap<SymbolId, ConstructorInfo>,
    pub(super) classes: HashMap<hir::TypeId, ClassInfo>,
    pub(super) class_methods: HashMap<SymbolId, (hir::TypeId, MethodInfo)>,
    pub(super) instances: Vec<InstanceInfo>,
}

/// The mutable solver: everything a binding changes and a speculation must undo.
///
/// `rigid` lives here rather than in [`Scope`] because rigidity is keyed by a
/// type-variable identity the solver allocates. Rolling back an allocation
/// recycles that identity, so a rolled-back rigidity has to be recycled with it;
/// otherwise a later unknown would inherit the rigidity of an unrelated binder.
#[derive(Clone)]
pub(super) struct InferState {
    pub(super) substitutions: HashMap<u32, InferType>,
    pub(super) levels: HashMap<u32, u32>,
    pub(super) rigid: HashSet<u32>,
    /// Every variable generalization has quantified, for finalization.
    pub(super) generic_variables: HashSet<u32>,
    /// The one kind solver: the substitution over kind variables, the next kind
    /// variable to allocate, and the rigid kind variables a `forall` binder
    /// introduced. Every kind equation inference solves goes through it.
    pub(super) kinds: psrs_kind::KindState,
    /// The kind recorded for each inference type variable. Every type unknown
    /// carries a kind, and a binding maintains it, so a type and its kind
    /// cannot disagree afterwards.
    pub(super) variable_kinds: HashMap<u32, Kind>,
    pub(super) wanted: Vec<WantedConstraint>,
    /// Stable identity for wanted constraints, including nested instance
    /// contexts that later join the declaration's root worklist.
    pub(super) next_wanted_id: u32,
    pub(super) errors: Vec<TypeCheckError>,
    /// Reports emitted by accepted primitive rules. They participate in the
    /// same snapshot contract as substitutions and errors.
    pub(super) warnings: Vec<TypeCheckWarning>,
    /// Functional-dependency conflicts already reported, keyed by span and
    /// message, so the fixed-point improvement pass does not duplicate them.
    pub(super) reported_fundep_conflicts: HashSet<(TextRange, String)>,
    /// Synonyms currently being expanded, so a cycle is reported rather than
    /// expanded forever.
    pub(super) expanding: HashSet<hir::TypeId>,
    /// Dictionary parameters synthesized for the declaration being checked.
    pub(super) pending_signatures: HashMap<SymbolId, Vec<(LocalId, InferType)>>,
    pub(super) next_variable: u32,
    pub(super) next_dictionary_local: u32,
    pub(super) level: u32,
}

/// The lexical chain: what is in scope where.
#[derive(Clone, Default)]
pub(super) struct Scope {
    pub(super) globals: HashMap<SymbolId, Scheme>,
    pub(super) locals: HashMap<LocalId, Scheme>,
    /// The constraints a constrained declaration may discharge from its
    /// dictionary parameters while checking its body.
    pub(super) givens: Vec<(ClassConstraint, WantedSolution)>,
    /// Variables made rigid while checking a local given context. Signature
    /// variables are already rigid; instance-head variables enter here.
    pub(super) given_rigid: Vec<u32>,
    /// Source names for type variables in the signature scope currently being
    /// checked. Typed patterns and expression ascriptions reuse these exact
    /// variables instead of elaborating a second rigid variable by name.
    pub(super) annotation_variables: HashMap<String, InferType>,
    /// Source names attached to quantified variables so entering a nested
    /// forall can extend the annotation scope at the matching expression.
    pub(super) type_variable_names: HashMap<u32, String>,
    /// Nearest top-level value or instance declaration that owns diagnostics
    /// emitted while this scope's wanteds are solved.
    pub(super) report_origin: Option<TextRange>,
}

/// Inference state, with each owner named. The three have distinct lifetimes,
/// and the operations below are the only way each is entered, left, or undone.
pub(super) struct Checker {
    pub(super) env: SemanticEnv,
    pub(super) state: InferState,
    pub(super) scope: Scope,
}

impl InferState {
    /// A saved copy of the whole solver. This is the only way solver state is
    /// saved: taking it through one operation is what stops a speculative path
    /// from omitting the kind table a later binding depends on.
    pub(super) fn snapshot(&self) -> InferState {
        self.clone()
    }

    /// The only way solver state is restored, and only from a
    /// [`InferState::snapshot`].
    pub(super) fn restore(&mut self, snapshot: InferState) {
        *self = snapshot;
    }
}

impl Checker {
    /// Runs `f` as a candidate whose effects must not survive a failure: an
    /// instance match, a subsumption trial, or an annotation that has to be
    /// re-checked.
    ///
    /// On success the candidate's substitutions, kinds, evidence, and
    /// diagnostics all stay. On failure the whole mutable state is restored,
    /// diagnostics included: a candidate that did not match is not itself an
    /// error. A caller whose candidate *is* the error uses
    /// [`Self::speculate_reporting`].
    pub(super) fn speculate<T>(&mut self, f: impl FnOnce(&mut Self) -> Option<T>) -> Option<T> {
        self.speculate_impl(false, f)
    }

    /// The same trial as [`Self::speculate`], for a candidate whose failure the
    /// caller has to report. On failure the state is restored and the
    /// diagnostics are re-emitted deliberately, so a diagnostic survives because
    /// it was kept rather than by accident.
    pub(super) fn speculate_reporting<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Option<T> {
        self.speculate_impl(true, f)
    }

    /// Checks a candidate and always restores its solver and lexical effects.
    ///
    /// A successful probe contributes no substitutions, evidence, warnings, or
    /// diagnostics to the caller. A failed probe keeps its diagnostics, because
    /// the caller is checking an annotation whose failure is itself reportable.
    pub(super) fn probe_reporting(
        &mut self,
        f: impl FnOnce(&mut Self) -> Option<()>,
    ) -> Option<()> {
        let state = self.state.snapshot();
        let scope = self.scope.clone();
        let errors_before = self.state.errors.len();
        let warnings_before = self.state.warnings.len();
        let result = f(self);
        let errors = self.state.errors.split_off(errors_before);
        let warnings = self.state.warnings.split_off(warnings_before);
        self.state.restore(state);
        self.scope = scope;
        match result {
            Some(value) if errors.is_empty() => Some(value),
            _ => {
                self.state.errors.extend(errors);
                self.state.warnings.extend(warnings);
                None
            }
        }
    }

    fn speculate_impl<T>(
        &mut self,
        keep_diagnostics: bool,
        f: impl FnOnce(&mut Self) -> Option<T>,
    ) -> Option<T> {
        let snapshot = self.state.snapshot();
        let errors_before = self.state.errors.len();
        let warnings_before = self.state.warnings.len();
        match f(self) {
            Some(value) => Some(value),
            None => {
                let kept = self.state.errors.split_off(errors_before);
                let kept_warnings = self.state.warnings.split_off(warnings_before);
                self.state.restore(snapshot);
                if keep_diagnostics {
                    self.state.errors.extend(kept);
                    self.state.warnings.extend(kept_warnings);
                }
                None
            }
        }
    }

    /// Runs `f` with `givens` added to the enclosing given evidence, and
    /// restores what leaving the scope restores: the givens, the rigid
    /// variables their arguments added, and the given-rigid record that tracks
    /// them.
    ///
    /// A given's variables are rigid only while the given is in scope, which is
    /// why this is not [`Self::with_skolem_scope`]: a skolem outlives its scope,
    /// a given's arguments do not.
    pub(super) fn with_givens<T>(
        &mut self,
        givens: Vec<(ClassConstraint, WantedSolution)>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous_givens = self.scope.givens.clone();
        self.scope.givens.extend(givens);
        let previous_rigid = self.state.rigid.clone();
        let previous_given_rigid = std::mem::take(&mut self.scope.given_rigid);
        for (constraint, _) in self.scope.givens[previous_givens.len()..].to_vec() {
            for argument in &constraint.arguments {
                let mut variables = HashSet::new();
                classes::collect_infer_variables(argument, &mut variables);
                for variable in variables {
                    if self.state.rigid.insert(variable) {
                        self.scope.given_rigid.push(variable);
                    }
                }
            }
        }
        let result = f(self);
        self.scope.givens = previous_givens;
        self.state.rigid = previous_rigid;
        self.scope.given_rigid = previous_given_rigid;
        result
    }

    /// Runs `f` with `givens` as the given-evidence chain in scope, and restores
    /// the chain on the way out.
    ///
    /// Entering a chain is deliberately not [`Self::with_givens`]: a chain swap
    /// during constraint solving keeps whatever rigidity the givens' variables
    /// already had, because a wanted argument that one of them mentions must
    /// still be solvable.
    pub(super) fn with_given_chain<T>(
        &mut self,
        givens: Vec<(ClassConstraint, WantedSolution)>,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = std::mem::replace(&mut self.scope.givens, givens);
        let result = f(self);
        self.scope.givens = previous;
        result
    }

    /// Runs `f` with `variables` as rigid representatives of an expected
    /// `forall`'s binders: each binder takes the nested level for the duration
    /// and is rigid for as long as the checked type keeps it.
    ///
    /// Leaving restores the level and each binder's own level. The binders stay
    /// rigid on purpose, so a substitution that outlives the scope cannot bind
    /// them, which is what makes a skolem escape an error.
    pub(super) fn with_skolem_scope<T>(
        &mut self,
        variables: &[u32],
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let outer_level = self.state.level;
        self.state.level = outer_level + 1;
        let previous_levels = variables
            .iter()
            .map(|variable| {
                let previous = self.state.levels.insert(*variable, self.state.level);
                self.state.rigid.insert(*variable);
                (*variable, previous)
            })
            .collect::<Vec<_>>();
        let result = f(self);
        self.state.level = outer_level;
        for (variable, previous) in previous_levels {
            if let Some(previous) = previous {
                self.state.levels.insert(variable, previous);
            }
        }
        result
    }

    /// Runs `f` in a nested lexical scope and restores what that scope owns: the
    /// annotation variables in scope and the solver level. Entering and leaving
    /// is this operation's whole contract, so a signature scope cannot leave an
    /// annotation variable behind for the next declaration to pick up.
    ///
    /// Givens and rigid variables are deliberately not restored here, because
    /// the two scopes that add them disagree about whether they outlive the
    /// scope; they use [`Self::with_givens`] and [`Self::with_skolem_scope`]
    /// instead.
    pub(super) fn with_scope<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let annotation_variables = self.scope.annotation_variables.clone();
        let level = self.state.level;
        let result = f(self);
        self.scope.annotation_variables = annotation_variables;
        self.state.level = level;
        result
    }

    /// Runs inference with an owning declaration location for report diagnostics.
    /// Wanted constraints capture this range when they are created, so solving a
    /// retained context later cannot accidentally inherit another declaration's
    /// location.
    pub(super) fn with_report_origin<T>(
        &mut self,
        origin: TextRange,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let previous = self.scope.report_origin.replace(origin);
        let result = f(self);
        self.scope.report_origin = previous;
        result
    }

    /// Runs `f` one level deeper than the caller, so the unknowns it allocates
    /// belong to a nested scope and generalization measures them against this
    /// level. The level is restored on the way out.
    pub(super) fn in_nested_level<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let level = self.state.level;
        self.state.level = level + 1;
        let result = f(self);
        self.state.level = level;
        result
    }

    /// Runs `f` and discards the diagnostics it reports, keeping everything it
    /// solved.
    ///
    /// A declaration another module already checked is re-elaborated here only to
    /// become searchable, so the identifiers it allocated must survive while its
    /// diagnostics do not. A speculative path may not use this: rolling back
    /// would recycle an identity the environment still refers to.
    pub(super) fn without_diagnostics<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let errors_before = self.state.errors.len();
        let warnings_before = self.state.warnings.len();
        let result = f(self);
        self.state.errors.truncate(errors_before);
        self.state.warnings.truncate(warnings_before);
        result
    }
}

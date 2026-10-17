//! The checked environment's direct projection onto the shared coverage
//! engine.
//!
//! The engine consumes a neutral pattern algebra and a [`Types`] fact table.
//! The checker's types are trees with inference variables, so one analysis
//! run first interns its types into skeleton identities — structural shapes
//! with unknowns keyed by their solver stamp — and then answers every engine
//! question from those skeletons plus the environment's constructor rows.
//! Constructor rows are elaborated with the same helper pattern checking
//! uses, on fresh throwaway unknowns, so coverage never unifies anything.
//!
//! One run visits only the scrutinee-rooted shape family, so the elaboration
//! cost stays proportional to the case's own types, not to the environment.

use super::super::*;
use psrs_coverage::{Branch, ConstructorRow, Pattern as SharedPattern, Types as CoverageTypes};

/// The interned skeleton of one checked type.
#[derive(Clone, PartialEq, Eq, Hash)]
enum Skeleton {
    Constructor(TypeConstructor),
    Applied(u32, u32),
    ForAll(u32),
    Constrained(u32),
    RowEmpty,
    RowExtend {
        label: String,
        ty: u32,
        tail: u32,
    },
    TypeLevelString(String),
    TypeLevelInt(i64),
    /// A solver unknown. The stamp keys equality, so every mention of the
    /// same inference variable shares one coverage identity and structurally
    /// distinct unknowns never do.
    Unknown(u32),
}

/// The interned shape table one analysis consumes, implementing the shared
/// [`CoverageTypes`] facts over the supervisor's environment.
#[derive(Default)]
pub(super) struct CaseCoverage {
    ids: HashMap<Skeleton, u32>,
    skeletons: Vec<Skeleton>,
    rows: HashMap<hir::TypeId, Vec<ConstructorRow<u32>>>,
    names: HashMap<SymbolId, String>,
}

impl Checker {
    /// Runs the shared engine over one case's checked alternatives.
    ///
    /// The scrutinee is resolved first, so branch-time unification the
    /// alternatives performed decides the shape the analysis queries.
    pub(super) fn case_coverage(
        &mut self,
        scrutinee_type: &InferType,
        branches: &[InferredCaseBranch],
    ) -> psrs_coverage::Analysis {
        let scrutinee = self.resolve_type(scrutinee_type.clone());
        let mut coverage = CaseCoverage::default();
        let scrutinee_id = coverage.intern(&scrutinee);
        let mut shared = Vec::with_capacity(branches.len());
        for branch in branches {
            // Built lazily, but engine queries on the alternatives' own types
            // precede any row lookup, so intern the patterns first: a shape
            // the engine reaches through a row field is interned by the row's
            // own build below.
            let pattern = coverage.shared_pattern(self, &branch.pattern);
            shared.push(Branch {
                coverage: branch.coverage,
                pattern,
            });
        }
        // Elaborate every reachable user type's rows. Reachability grows while
        // interning, so the loop runs to a fixpoint; a cycle in data types
        // terminates because each iteration interns new rows and the next scan
        // finds nothing unvisited.
        loop {
            let pending = coverage.unexamined_user_types();
            if pending.is_empty() {
                break;
            }
            for type_id in pending {
                let infos = self
                    .env
                    .constructor_info
                    .values()
                    .filter(|info| info.type_id == type_id)
                    .cloned()
                    .collect::<Vec<_>>();
                let mut built = Vec::new();
                for info in infos {
                    let (_, fields) = self.instantiate_constructor(&info);
                    let field_ids = fields
                        .iter()
                        .map(|field| {
                            let resolved = self.resolve_type(field.clone());
                            coverage.intern(&resolved)
                        })
                        .collect();
                    built.push(ConstructorRow {
                        symbol: info.symbol,
                        fields: field_ids,
                    });
                    coverage.names.insert(info.symbol, info.name.clone());
                }
                // An uninhabited or foreign type keeps no usable rows: the
                // `constructors_of` answer stays unknown (an empty entry),
                // matching the backend's own policy. The entry still records
                // the type as examined so the fixpoint scan terminates.
                coverage.rows.insert(type_id, built);
            }
        }
        psrs_coverage::analyze(&coverage, scrutinee_id, &shared)
    }
}

impl CoverageTypes<u32> for CaseCoverage {
    fn shape(&self, ty: u32) -> u32 {
        let mut ty = ty;
        loop {
            match self.skeletons[ty as usize] {
                Skeleton::ForAll(body) | Skeleton::Constrained(body) => ty = body,
                _ => return ty,
            }
        }
    }

    fn array_element(&self, ty: u32) -> Option<u32> {
        let shaped = self.shape(ty);
        let Skeleton::Applied(function, _) = self.skeletons[shaped as usize] else {
            return None;
        };
        let shaped_head = self.shape(function);
        matches!(
            self.skeletons[shaped_head as usize],
            Skeleton::Constructor(TypeConstructor::Array)
        )
        .then_some(self.shape_argument_of(shaped))
    }

    fn constructors_of(&self, ty: u32) -> Option<Vec<ConstructorRow<u32>>> {
        let head = self.constructor_head(ty)?;
        let Skeleton::Constructor(TypeConstructor::User(type_id)) = self.skeletons[head as usize]
        else {
            return None;
        };
        self.rows
            .get(&type_id)
            .filter(|rows| !rows.is_empty())
            .cloned()
    }

    fn record_fields(&self, ty: u32) -> Option<Vec<(String, u32)>> {
        let shaped = self.shape(ty);
        let Skeleton::Applied(function, argument) = self.skeletons[shaped as usize] else {
            return None;
        };
        let is_record = matches!(
            self.skeletons[self.shape(function) as usize],
            Skeleton::Constructor(TypeConstructor::Record)
        );
        if !is_record {
            return None;
        }
        let row = self.shape(argument);
        let mut fields = Vec::new();
        let mut current = row;
        loop {
            match self.skeletons[current as usize].clone() {
                Skeleton::RowExtend { label, ty, tail } => {
                    fields.push((label, ty));
                    current = tail;
                }
                // A row variable tail means the record may gain fields, and a
                // pattern reading the known labels still covers every value of
                // the row: row unification keeps the listed fields and only
                // grows beyond them. The backend's own closed-record answer is
                // for core types whose rows are already solved.
                _ => return Some(fields),
            }
        }
    }

    fn boolean_type(&self, ty: u32) -> bool {
        self.constructor_head(ty).is_some_and(|head| {
            matches!(
                self.skeletons[head as usize],
                Skeleton::Constructor(TypeConstructor::Boolean)
            )
        })
    }

    fn constructor_name(&self, symbol: SymbolId) -> String {
        self.names
            .get(&symbol)
            .cloned()
            .unwrap_or_else(|| format!("Constructor{}", symbol.index))
    }
}

impl CaseCoverage {
    fn intern(&mut self, ty: &InferType) -> u32 {
        let key = match ty {
            InferType::Constructor(constructor) => Skeleton::Constructor(*constructor),
            InferType::Application(function, argument) => {
                Skeleton::Applied(self.intern(function), self.intern(argument))
            }
            InferType::ForAll { body, .. } => Skeleton::ForAll(self.intern(body)),
            InferType::Constrained { body, .. } => Skeleton::Constrained(self.intern(body)),
            InferType::RowEmpty => Skeleton::RowEmpty,
            InferType::RowExtend { label, ty, tail } => Skeleton::RowExtend {
                label: label.clone(),
                ty: self.intern(ty),
                tail: self.intern(tail),
            },
            InferType::TypeLevelString(value) => Skeleton::TypeLevelString(value.clone()),
            InferType::TypeLevelInt(value) => Skeleton::TypeLevelInt(*value),
            InferType::Variable(variable) => Skeleton::Unknown(*variable),
        };
        if let Some(&id) = self.ids.get(&key) {
            return id;
        }
        let id = self.skeletons.len() as u32;
        self.skeletons.push(key.clone());
        self.ids.insert(key, id);
        id
    }

    /// Shared spine-walk: strips quantified prefixes and, at an application,
    /// follows the head, returning the id whose skeleton is the outer
    /// constructor. `None` for rows and unknowns, which name no type head.
    fn constructor_head(&self, ty: u32) -> Option<u32> {
        let mut ty = self.shape(ty);
        loop {
            match self.skeletons[ty as usize] {
                Skeleton::Constructor(_) => return Some(ty),
                Skeleton::Applied(function, _) => ty = self.shape(function),
                _ => return None,
            }
        }
    }

    /// The argument identity of an application skeleton.
    fn shape_argument_of(&self, ty: u32) -> u32 {
        match &self.skeletons[ty as usize] {
            Skeleton::Applied(_, argument) => *argument,
            _ => ty,
        }
    }

    fn unexamined_user_types(&self) -> Vec<hir::TypeId> {
        let mut pending = Vec::new();
        for skeleton in &self.skeletons {
            let Skeleton::Constructor(TypeConstructor::User(type_id)) = skeleton else {
                continue;
            };
            if !self.rows.contains_key(type_id) {
                pending.push(*type_id);
            }
        }
        pending
    }

    fn shared_pattern(
        &mut self,
        checker: &Checker,
        pattern: &InferredPattern,
    ) -> SharedPattern<u32> {
        let ty = self.intern(&checker.resolve_type(pattern.ty.clone()));
        self.shared_pattern_kind(checker, &pattern.kind, ty)
    }

    fn shared_pattern_kind(
        &mut self,
        checker: &Checker,
        kind: &InferredPatternKind,
        ty: u32,
    ) -> SharedPattern<u32> {
        match kind {
            InferredPatternKind::Wildcard => SharedPattern::Any { ty },
            InferredPatternKind::Literal { literal } => SharedPattern::Literal {
                literal: shared_literal(literal),
                ty,
            },
            InferredPatternKind::Var { .. } => {
                // A variable binder covers every value the scrutinee allows,
                // so the binder itself carries no coverage fact.
                SharedPattern::Any { ty }
            }
            InferredPatternKind::Named { pattern, .. } => {
                // An as-pattern's inner shape is the fact.
                self.shared_pattern(checker, pattern)
            }
            InferredPatternKind::Array { elements } => SharedPattern::Array {
                elements: elements
                    .iter()
                    .map(|element| self.shared_pattern(checker, element))
                    .collect(),
                ty,
            },
            InferredPatternKind::Constructor { symbol, arguments } => SharedPattern::Constructor {
                symbol: *symbol,
                arguments: arguments
                    .iter()
                    .map(|argument| self.shared_pattern(checker, argument))
                    .collect(),
                ty,
            },
            InferredPatternKind::Record { fields } => SharedPattern::Record {
                fields: fields
                    .iter()
                    .map(|(label, value)| (label.clone(), self.shared_pattern(checker, value)))
                    .collect(),
                ty,
            },
        }
    }
}

fn shared_literal(literal: &thir::PatternLiteral) -> psrs_coverage::Literal {
    match literal {
        thir::PatternLiteral::Integer(value) => psrs_coverage::Literal::Integer(*value),
        thir::PatternLiteral::Number(value) => psrs_coverage::Literal::Number(value.clone()),
        thir::PatternLiteral::String(value) => psrs_coverage::Literal::String(value.clone()),
        thir::PatternLiteral::Char(value) => psrs_coverage::Literal::Char(*value),
        thir::PatternLiteral::Boolean(value) => psrs_coverage::Literal::Boolean(*value),
    }
}

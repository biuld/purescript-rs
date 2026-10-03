//! The order relation `Prim.Int.Compare` closes over what is in scope.
//!
//! This is official `TypeChecker/Entailment/IntCompare.hs`, which is a separate
//! module from `Entailment.hs` for a reason: `Int.Compare` is not a comparison
//! of two integers but a *relation* over the orderings that happen to be in
//! scope. `solveIntCompare` reads two known integers and, failing that, hands the
//! goal to `solveRelation` together with the facts it collects. Everything below
//! is that module: the facts, the graph, and the reachability that is the answer.
//!
//! The rule this supports is one member of a pair; the other,
//! `Prim.Symbol.Compare`, has no relation to close and reads only two literals.

use super::{Ordering, known_ordering, literal_int};
use crate::typecheck::*;

/// One edge of the order relation: `left` is strictly less than `right`.
///
/// The edge set is the whole of `Int.Compare`'s relation state. An `EQ` ordering
/// in scope contributes an edge in each direction, a `LT` contributes one, and a
/// `GT` contributes the edge that reading it the other way round gives — which
/// is official `mkRelation`'s `GT -> LessThan rhs lhs`.
#[derive(Clone, Debug, PartialEq, Eq)]
struct Edge(InferType, InferType);

/// Closes the order relation over what is in scope and decides `left right`.
///
/// This is official `solveRelation`, with `mkFacts` and `mkRelation` as its
/// inputs, step for step:
///
/// 1. `lhs == rhs` is `EQ` outright, without consulting anything else. This is
///    what decides `Compare n n` for a rigid `n`.
/// 2. Every in-scope `Prim.Int.Compare` dictionary that carries one of the three
///    orderings contributes an edge. A dictionary whose ordering is still unknown
///    contributes none, which is official `mkRelation` returning `Nothing` for a
///    type that is not a `Prim.Ordering` constructor.
/// 3. Every type-level literal anywhere in scope contributes a fact: the
///    literals of the goal's own arguments and of each in-scope dictionary's
///    arguments are sorted and every earlier one is asserted to be less than
///    every later one. That is official `mkFacts`, and the sort is the shared
///    `Ord` on a type-level integer, which is its numeric value.
/// 4. The answer is then reachability: a path from `left` to `right` only is
///    `LT`, a path from `right` to `left` only is `GT`, both paths are `EQ`, and
///    an operand that is not a node of the graph at all decides nothing.
///
/// A type-level literal is therefore evidence about its neighbours even when no
/// given orders them: `Compare a 10 LT` in scope together with a goal of
/// `Compare a 20 o` gives the path `a -> 10 -> 20`, while a goal of
/// `Compare a 5 o` does not, because the sorted literals put `10` before `5`.
pub(super) fn close_relation(checker: &Checker, arguments: &[InferType]) -> Option<Ordering> {
    let [left, right, _] = arguments else {
        return None;
    };
    if left == right {
        return Some(Ordering::Eq);
    }
    let mut edges = Vec::new();
    let mut literals = Vec::new();
    // The facts come from the goal's own arguments first, then from each
    // in-scope dictionary: that is official's `mkFacts (args : (tcdInstanceTypes
    // <$> compareDictsInScope))`, and it is why the goal's own literal operand is
    // a fact about the goal.
    for arguments in std::iter::once(arguments.to_vec()).chain(in_scope_comparisons(checker)) {
        // `mkFacts` reads at most one literal per obligation, and none when both
        // operands are literals: that obligation is already decided by value.
        match (literal_int(&arguments[0]), literal_int(&arguments[1])) {
            (Some(_), Some(_)) => {}
            (Some(left), None) => literals.push(left),
            (None, Some(right)) => literals.push(right),
            (None, None) => {}
        }
    }
    // The relations come only from the dictionaries in scope. The obligation
    // being decided is deliberately *not* among them: official builds `givens`
    // from `findDicts ctx C.IntCompare` alone, so a goal never states its own
    // answer.
    for arguments in in_scope_comparisons(checker) {
        let Some(ordering) = known_ordering(&arguments[super::ORDERING]) else {
            continue;
        };
        match ordering {
            Ordering::Eq => {
                edges.push(Edge(arguments[0].clone(), arguments[1].clone()));
                edges.push(Edge(arguments[1].clone(), arguments[0].clone()));
            }
            Ordering::Lt => edges.push(Edge(arguments[0].clone(), arguments[1].clone())),
            Ordering::Gt => edges.push(Edge(arguments[1].clone(), arguments[0].clone())),
        }
    }
    literals.sort_unstable();
    for earlier in 0..literals.len() {
        for later in earlier + 1..literals.len() {
            edges.push(Edge(
                InferType::TypeLevelInt(literals[earlier]),
                InferType::TypeLevelInt(literals[later]),
            ));
        }
    }
    // A node is a type some edge mentions. An operand that is not one has no
    // path out of it, which is official's `search` returning `Nothing`.
    if !is_node(&edges, left) || !is_node(&edges, right) {
        return None;
    }
    match (
        reachable(&edges, left, right),
        reachable(&edges, right, left),
    ) {
        (true, true) => Some(Ordering::Eq),
        (true, false) => Some(Ordering::Lt),
        (false, true) => Some(Ordering::Gt),
        (false, false) => None,
    }
}

/// Every lexical given or instance-context dictionary for this member, each read
/// through the shared substitution.
///
/// This is official's `findDicts ctx C.IntCompare`. The dictionaries in scope are
/// the shared `Scope`'s own record of the dictionary parameters a declaration
/// assumes, so a rule cannot read a different set than the given lookup reads.
fn in_scope_comparisons<'a>(checker: &'a Checker) -> impl Iterator<Item = Vec<InferType>> + 'a {
    checker
        .scope
        .givens
        .iter()
        .filter(|(given, _)| given.class_id == hir::TypeId::PRIM_INT_COMPARE)
        .map(|(given, _)| {
            given
                .arguments
                .iter()
                .map(|argument| checker.resolve_type(argument.clone()))
                .collect()
        })
}

/// Whether some edge mentions `ty`, which is what makes it a node of the graph.
fn is_node(edges: &[Edge], ty: &InferType) -> bool {
    edges
        .iter()
        .any(|Edge(left, right)| left == ty || right == ty)
}

/// Whether `to` is reachable from `from` along the edges. A node reaches itself,
/// which is what `Data.Graph`'s `path` does and what makes a cycle read as
/// equality.
fn reachable(edges: &[Edge], from: &InferType, to: &InferType) -> bool {
    if from == to {
        return true;
    }
    let mut visited = vec![from.clone()];
    let mut pending = vec![from.clone()];
    while let Some(current) = pending.pop() {
        for Edge(left, right) in edges {
            if left == &current && !visited.iter().any(|seen| seen == right) {
                visited.push(right.clone());
                if right == to {
                    return true;
                }
                pending.push(right.clone());
            }
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::super::tests::*;
    use super::*;

    /// `Int.Compare` closes the relation rather than only comparing literals.
    /// Given `a == b` and `b < c`, the path `a -> b -> c` decides `a < c`, which
    /// is what `failing/CompareInt3.purs` states and what a literal-only rule
    /// would decline.
    #[test]
    fn a_transitive_chain_of_givens_decides_the_goal() {
        let mut checker = checker();
        let (first, second, third) = (
            rigid(&mut checker),
            rigid(&mut checker),
            rigid(&mut checker),
        );
        checker.scope.givens.push(given(vec![
            first.clone(),
            second.clone(),
            ordering(Ordering::Eq),
        ]));
        checker
            .scope
            .givens
            .push(given(vec![second, third.clone(), ordering(Ordering::Lt)]));
        let free = unknown(&mut checker);
        let goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_COMPARE,
            vec![first.clone(), third.clone(), free],
        );

        assert_eq!(
            decided_arguments(&mut checker, &goal),
            vec![first, third, ordering(Ordering::Lt)],
            "`Compare a b EQ` and `Compare b c LT` give the path a -> b -> c"
        );
        assert!(checker.state.errors.is_empty());
    }

    /// Symmetry and equality fall out of the same graph: reading an `LT` the
    /// other way round gives `GT`, and a node reaches itself.
    #[test]
    fn a_reversed_path_is_greater_and_a_node_reaches_itself() {
        let mut checker = checker();
        let (first, second) = (rigid(&mut checker), rigid(&mut checker));
        checker.scope.givens.push(given(vec![
            first.clone(),
            second.clone(),
            ordering(Ordering::Lt),
        ]));

        let reversed_order = unknown(&mut checker);
        let reversed = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_COMPARE,
            vec![second.clone(), first.clone(), reversed_order],
        );
        assert_eq!(
            decided_arguments(&mut checker, &reversed),
            vec![second, first.clone(), ordering(Ordering::Gt)]
        );

        let reflexive_order = unknown(&mut checker);
        let equality = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_COMPARE,
            vec![first.clone(), first.clone(), reflexive_order],
        );
        assert_eq!(
            decided_arguments(&mut checker, &equality),
            vec![first.clone(), first, ordering(Ordering::Eq)],
            "a node reaches itself, which is what official's `lhs == rhs` \
             branch says before the graph is consulted at all"
        );
        assert!(checker.state.errors.is_empty());
    }

    /// A type-level literal in scope is a fact about its neighbours even when no
    /// given orders them. `Compare a 10 LT` gives `a -> 10`, and the sorted
    /// literals `10 < 20` add `10 -> 20`, so the goal `a < 20` has a path; the
    /// goal `a < 5` does not, because the sorted literals put `10` before `5` and
    /// no edge reaches `5`. That is `passing/SolvingCompareInt.purs`'s
    /// `litTransLT` and `failing/CompareInt11.purs`.
    #[test]
    fn a_literal_in_scope_extends_the_relation_to_its_neighbours() {
        let outcome = |comparable: i64| {
            let mut checker = checker();
            let anchor = rigid(&mut checker);
            checker
                .scope
                .givens
                .push(given(vec![anchor.clone(), int(10), ordering(Ordering::Lt)]));
            let free = unknown(&mut checker);
            let goal = goal(
                &mut checker,
                hir::TypeId::PRIM_INT_COMPARE,
                vec![anchor, int(comparable), free],
            );
            checker.solve_primitive(&goal, SolveDepth::new())
        };

        assert!(
            matches!(
                outcome(20),
                PrimitiveDispatch::Solved(WantedSolution::Primitive { .. })
            ),
            "the path a -> 10 -> 20 exists"
        );
        assert!(
            matches!(outcome(5), PrimitiveDispatch::None),
            "no path reaches 5, so the rule declines rather than guessing"
        );
    }

    /// A goal states nothing about itself: official builds the relation from the
    /// dictionaries *in scope*, so an obligation whose own ordering is known is
    /// not evidence for its own answer. Without this, every obligation would
    /// answer itself and the rule would never decline.
    #[test]
    fn a_goal_is_not_a_fact_about_itself() {
        let mut checker = checker();
        let anchor = rigid(&mut checker);
        let goal = goal(
            &mut checker,
            hir::TypeId::PRIM_INT_COMPARE,
            vec![anchor, int(5), ordering(Ordering::Lt)],
        );
        let before = checker.state.substitutions.clone();

        assert!(
            matches!(
                checker.solve_primitive(&goal, SolveDepth::new()),
                PrimitiveDispatch::None
            ),
            "the goal's own LT is not an in-scope dictionary"
        );
        assert_eq!(checker.state.substitutions, before);
        assert!(checker.state.errors.is_empty());
    }
}

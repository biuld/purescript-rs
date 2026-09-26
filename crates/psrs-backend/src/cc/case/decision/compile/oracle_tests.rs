//! Exhaustive first-match oracle for the decision compiler and coverage.
//!
//! The enumerator builds every small matrix over a two-type ADT
//! (`T = A | B | C U`, `U = X | Y`) and checks that
//!
//! * the compiled DAG selects the same branch as a straightforward first-match
//!   interpreter for every value, and
//! * coverage agrees with the interpreter on exhaustiveness and redundancy.
//!
//! This is the oracle comparison PM-03/PM-04 require. It catches sharing,
//! specialization, and source-order regressions that targeted unit tests miss.

use super::*;
use crate::cc::case::coverage;
use psrs_core::{CaseBranch, Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeConstructor};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::collections::HashMap;

const T: psrs_core::TypeId = psrs_core::TypeId(0);
const U: psrs_core::TypeId = psrs_core::TypeId(1);
const I32: psrs_core::TypeId = psrs_core::TypeId(2);

fn symbol(index: u32) -> SymbolId {
    SymbolId::new(ModuleId(0), index)
}

fn module() -> Module {
    let t = HirTypeId::new(ModuleId(0), 0);
    let u = HirTypeId::new(ModuleId(0), 1);
    Module {
        id: ModuleId(0),
        name: "OracleTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Constructor(TypeConstructor::User(t)),
            Type::Constructor(TypeConstructor::User(u)),
            Type::I32,
        ],
        newtype_ids: Vec::new(),
        opaque_ids: Vec::new(),
        constructors: vec![
            psrs_core::ConstructorInfo {
                symbol: symbol(0),
                name: "A".into(),
                type_id: t,
                tag: 0,
                field_count: 0,
                field_types: Vec::new(),
            },
            psrs_core::ConstructorInfo {
                symbol: symbol(1),
                name: "B".into(),
                type_id: t,
                tag: 1,
                field_count: 0,
                field_types: Vec::new(),
            },
            psrs_core::ConstructorInfo {
                symbol: symbol(2),
                name: "C".into(),
                type_id: t,
                tag: 2,
                field_count: 1,
                field_types: vec![U],
            },
            psrs_core::ConstructorInfo {
                symbol: symbol(3),
                name: "X".into(),
                type_id: u,
                tag: 0,
                field_count: 0,
                field_types: Vec::new(),
            },
            psrs_core::ConstructorInfo {
                symbol: symbol(4),
                name: "Y".into(),
                type_id: u,
                tag: 1,
                field_count: 0,
                field_types: Vec::new(),
            },
        ],
        declarations: Vec::new(),
        entry: None,
        span: TextRange::new(0, 100),
    }
}

#[derive(Clone, Copy, Debug)]
enum PatU {
    Any,
    X,
    Y,
}

#[derive(Clone, Copy, Debug)]
enum PatT {
    Any,
    A,
    B,
    C(PatU),
}

const PATTERNS: [PatT; 6] = [
    PatT::Any,
    PatT::A,
    PatT::B,
    PatT::C(PatU::Any),
    PatT::C(PatU::X),
    PatT::C(PatU::Y),
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Value {
    symbol: SymbolId,
    children: Vec<Value>,
}

fn values() -> Vec<Value> {
    let leaf = |index| Value {
        symbol: symbol(index),
        children: Vec::new(),
    };
    vec![
        leaf(0),
        leaf(1),
        Value {
            symbol: symbol(2),
            children: vec![leaf(3)],
        },
        Value {
            symbol: symbol(2),
            children: vec![leaf(4)],
        },
    ]
}

impl PatT {
    fn matches(self, value: &Value) -> bool {
        match self {
            PatT::Any => true,
            PatT::A => value.symbol == symbol(0),
            PatT::B => value.symbol == symbol(1),
            PatT::C(inner) => {
                value.symbol == symbol(2)
                    && value
                        .children
                        .first()
                        .is_some_and(|child| inner.matches(child))
            }
        }
    }

    fn core(self, span: TextRange) -> Pattern {
        let kind = match self {
            PatT::Any => PatternKind::Wildcard,
            PatT::A | PatT::B => PatternKind::Constructor {
                symbol: if matches!(self, PatT::A) {
                    symbol(0)
                } else {
                    symbol(1)
                },
                arguments: Vec::new(),
            },
            PatT::C(inner) => PatternKind::Constructor {
                symbol: symbol(2),
                arguments: vec![inner.core(span)],
            },
        };
        Pattern { kind, ty: T, span }
    }
}

impl PatU {
    fn matches(self, value: &Value) -> bool {
        match self {
            PatU::Any => true,
            PatU::X => value.symbol == symbol(3),
            PatU::Y => value.symbol == symbol(4),
        }
    }

    fn core(self, span: TextRange) -> Pattern {
        let kind = match self {
            PatU::Any => PatternKind::Wildcard,
            PatU::X | PatU::Y => PatternKind::Constructor {
                symbol: if matches!(self, PatU::X) {
                    symbol(3)
                } else {
                    symbol(4)
                },
                arguments: Vec::new(),
            },
        };
        Pattern { kind, ty: U, span }
    }
}

fn branches(matrix: &[PatT]) -> Vec<CaseBranch> {
    matrix
        .iter()
        .enumerate()
        .map(|(index, pattern)| {
            let span = TextRange::new(index as u32 * 10, index as u32 * 10 + 5);
            CaseBranch {
                pattern: pattern.core(span),
                value: Expr {
                    kind: ExprKind::Integer(index as i32),
                    ty: I32,
                    span,
                },
                span,
            }
        })
        .collect()
}

/// Interprets the compiled DAG, returning the selected branch or `None` when
/// evaluation reaches `Fail`.
fn eval(dag: &DecisionDag, node: NodeId, env: &HashMap<ColumnKey, Value>) -> Option<usize> {
    match &dag.nodes[node.0] {
        Decision::Leaf { branch, .. } => Some(*branch),
        Decision::Fail { .. } => None,
        Decision::Switch {
            column,
            edges,
            default_actions,
            default,
            ..
        } => {
            // The realizer short-circuits a single irrefutable edge (a product
            // specialization or a dropped all-irrefutable column) without
            // reading the switch column, so the interpreter must too.
            if edges.len() == 1 && matches!(edges[0].test, Test::Irrefutable) {
                let child = apply_actions(&edges[0].actions, env);
                return eval(dag, edges[0].target, &child);
            }
            let value = env.get(column).expect("switch column is bound").clone();
            for edge in edges {
                let selected = match edge.test {
                    Test::Constructor { symbol, .. } => value.symbol == symbol,
                    Test::Irrefutable => true,
                };
                if selected {
                    let child = apply_actions(&edge.actions, env);
                    return eval(dag, edge.target, &child);
                }
            }
            match default {
                Some(target) => {
                    let child = apply_actions(default_actions, env);
                    eval(dag, *target, &child)
                }
                None => None,
            }
        }
    }
}

fn apply_actions(actions: &[Action], env: &HashMap<ColumnKey, Value>) -> HashMap<ColumnKey, Value> {
    let mut projected = env.clone();
    for action in actions {
        match action {
            Action::Map { source, target, .. } => {
                if let Some(value) = env.get(source) {
                    projected.insert(target.clone(), value.clone());
                }
            }
            Action::Project {
                source,
                target,
                field,
                ..
            } => {
                if let Some(value) = env.get(source)
                    && let Some(child) = value.children.get(*field as usize)
                {
                    projected.insert(target.clone(), child.clone());
                }
            }
            Action::Bind { .. } | Action::TestTag { .. } => {}
        }
    }
    projected
}

fn oracle_branch(matrix: &[PatT], value: &Value) -> Option<usize> {
    matrix.iter().position(|pattern| pattern.matches(value))
}

fn render(matrix: &[PatT]) -> String {
    format!("{matrix:?}")
}

fn all_matrices() -> Vec<Vec<PatT>> {
    let mut matrices = Vec::new();
    for first in PATTERNS {
        matrices.push(vec![first]);
        for second in PATTERNS {
            matrices.push(vec![first, second]);
            for third in PATTERNS {
                matrices.push(vec![first, second, third]);
            }
        }
    }
    matrices
}

#[test]
fn decision_dag_matches_the_first_match_oracle_for_every_small_matrix() {
    let module = module();
    let all_values = values();
    for matrix in all_matrices() {
        let branches = branches(&matrix);
        let dag = compile_dag(
            &module,
            &std::collections::HashSet::new(),
            T,
            &branches,
            TextRange::new(0, 100),
        )
        .unwrap_or_else(|error| panic!("{} should compile: {error}", render(&matrix)));
        for value in &all_values {
            let mut env = HashMap::new();
            env.insert(ColumnKey::root(), value.clone());
            let compiled = eval(&dag, dag.root, &env);
            let expected = oracle_branch(&matrix, value);
            assert_eq!(
                compiled,
                expected,
                "matrix {} selected {compiled:?} for {value:?}, oracle {expected:?}",
                render(&matrix)
            );
        }
    }
}

#[test]
fn coverage_agrees_with_the_first_match_oracle() {
    let module = module();
    let all_values = values();
    for matrix in all_matrices() {
        let branches = branches(&matrix);
        let report = coverage::analyze(&module, T, &branches);
        let exhaustive = all_values
            .iter()
            .all(|value| oracle_branch(&matrix, value).is_some());
        assert_eq!(
            report.exhaustive,
            exhaustive,
            "exhaustiveness disagreed for {}: {report:?}",
            render(&matrix)
        );
        let mut expected_redundant = Vec::new();
        for index in 0..matrix.len() {
            let useful = all_values.iter().any(|value| {
                matrix[index].matches(value) && oracle_branch(&matrix[..index], value).is_none()
            });
            if !useful {
                expected_redundant.push(index);
            }
        }
        assert_eq!(
            report.redundant_branches,
            expected_redundant,
            "redundancy disagreed for {}",
            render(&matrix)
        );
    }
}

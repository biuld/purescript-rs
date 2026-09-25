//! Exhaustive first-match oracle for record (product) pattern compilation.
//!
//! Records specialize without a tag test and project canonical, label-sorted
//! fields. This enumerates every small closed-record matrix over
//! `R = { x: U, y: U }` with `U = X | Y` and checks the compiled DAG against a
//! straightforward first-match interpreter, including partial field patterns.

use super::*;
use psrs_core::{CaseBranch, Expr, ExprKind, Module, Pattern, PatternKind, Type, TypeConstructor};
use psrs_hir::{ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::collections::HashMap;

const R: psrs_core::TypeId = psrs_core::TypeId(0);
const U: psrs_core::TypeId = psrs_core::TypeId(1);
const I32: psrs_core::TypeId = psrs_core::TypeId(2);

fn symbol(index: u32) -> SymbolId {
    SymbolId::new(ModuleId(0), index)
}

fn module() -> Module {
    let u = HirTypeId::new(ModuleId(0), 1);
    Module {
        id: ModuleId(0),
        name: "RecordOracleTest".into(),
        externals: Vec::new(),
        types: vec![
            Type::Record(vec![("x".into(), U), ("y".into(), U)]),
            Type::Constructor(TypeConstructor::User(u)),
            Type::I32,
        ],
        newtype_ids: Vec::new(),
        constructors: vec![
            psrs_core::ConstructorInfo {
                symbol: symbol(0),
                name: "X".into(),
                type_id: u,
                tag: 0,
                field_count: 0,
                field_types: Vec::new(),
            },
            psrs_core::ConstructorInfo {
                symbol: symbol(1),
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
enum Field {
    Any,
    X,
    Y,
}

#[derive(Clone, Copy, Debug)]
enum Rec {
    Any,
    Fields { x: Option<Field>, y: Option<Field> },
}

const PATTERNS: [Rec; 16] = [
    Rec::Any,
    Rec::Fields {
        x: Some(Field::Any),
        y: None,
    },
    Rec::Fields {
        x: Some(Field::X),
        y: None,
    },
    Rec::Fields {
        x: Some(Field::Y),
        y: None,
    },
    Rec::Fields {
        x: None,
        y: Some(Field::Any),
    },
    Rec::Fields {
        x: None,
        y: Some(Field::X),
    },
    Rec::Fields {
        x: None,
        y: Some(Field::Y),
    },
    Rec::Fields {
        x: Some(Field::X),
        y: Some(Field::X),
    },
    Rec::Fields {
        x: Some(Field::X),
        y: Some(Field::Y),
    },
    Rec::Fields {
        x: Some(Field::Y),
        y: Some(Field::X),
    },
    Rec::Fields {
        x: Some(Field::Y),
        y: Some(Field::Y),
    },
    Rec::Fields {
        x: Some(Field::X),
        y: Some(Field::Any),
    },
    Rec::Fields {
        x: Some(Field::Y),
        y: Some(Field::Any),
    },
    Rec::Fields {
        x: Some(Field::Any),
        y: Some(Field::X),
    },
    Rec::Fields {
        x: Some(Field::Any),
        y: Some(Field::Y),
    },
    Rec::Fields {
        x: Some(Field::Any),
        y: Some(Field::Any),
    },
];

#[derive(Clone, Debug, PartialEq, Eq)]
struct Value {
    symbol: SymbolId,
    /// Canonical field order for a record: `[x, y]`.
    children: Vec<Value>,
}

fn nullary(symbol_index: u32) -> Value {
    Value {
        symbol: symbol(symbol_index),
        children: Vec::new(),
    }
}

fn values() -> Vec<Value> {
    let mut values = Vec::new();
    for x in [nullary(0), nullary(1)] {
        for y in [nullary(0), nullary(1)] {
            values.push(Value {
                symbol: symbol(9),
                children: vec![x.clone(), y],
            });
        }
    }
    values
}

impl Field {
    fn matches(self, value: &Value) -> bool {
        match self {
            Field::Any => true,
            Field::X => value.symbol == symbol(0),
            Field::Y => value.symbol == symbol(1),
        }
    }

    fn core(self, span: TextRange) -> Option<Pattern> {
        match self {
            Field::Any => None,
            Field::X | Field::Y => Some(Pattern {
                kind: PatternKind::Constructor {
                    symbol: if matches!(self, Field::X) {
                        symbol(0)
                    } else {
                        symbol(1)
                    },
                    arguments: Vec::new(),
                },
                ty: U,
                span,
            }),
        }
    }
}

impl Rec {
    fn matches(self, value: &Value) -> bool {
        match self {
            Rec::Any => true,
            Rec::Fields { x, y } => {
                x.is_none_or(|field| field.matches(&value.children[0]))
                    && y.is_none_or(|field| field.matches(&value.children[1]))
            }
        }
    }

    fn core(self, span: TextRange) -> Pattern {
        let fields = match self {
            Rec::Any => Vec::new(),
            Rec::Fields { x, y } => [
                ("x", x.and_then(|field| field.core(span))),
                ("y", y.and_then(|field| field.core(span))),
            ]
            .into_iter()
            .filter_map(|(label, pattern)| pattern.map(|pattern| (label.to_owned(), pattern)))
            .collect(),
        };
        Pattern {
            kind: PatternKind::Record { fields },
            ty: R,
            span,
        }
    }
}

fn branches(matrix: &[Rec]) -> Vec<CaseBranch> {
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

fn oracle_branch(matrix: &[Rec], value: &Value) -> Option<usize> {
    matrix.iter().position(|pattern| pattern.matches(value))
}

#[test]
fn record_dag_matches_the_first_match_oracle_for_every_small_matrix() {
    let module = module();
    let all_values = values();
    let mut matrices: Vec<Vec<Rec>> = Vec::new();
    for first in PATTERNS {
        matrices.push(vec![first]);
        for second in PATTERNS {
            matrices.push(vec![first, second]);
        }
    }
    for matrix in matrices {
        let branches = branches(&matrix);
        let dag = compile_dag(
            &module,
            &std::collections::HashSet::new(),
            R,
            &branches,
            TextRange::new(0, 100),
        )
        .unwrap_or_else(|error| panic!("{matrix:?} should compile: {error}"));
        for value in &all_values {
            let mut env = HashMap::new();
            env.insert(ColumnKey::root(), value.clone());
            let compiled = eval(&dag, dag.root, &env);
            let expected = oracle_branch(&matrix, value);
            assert_eq!(
                compiled, expected,
                "matrix {matrix:?} selected {compiled:?} for {value:?}, oracle {expected:?}"
            );
        }
    }
}

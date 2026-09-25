//! Pattern-matrix compilation into a shared, target-neutral decision DAG.

mod compile;
mod realize;

use psrs_core::TypeId;
use psrs_hir::{LocalId, SymbolId};
use psrs_span::TextRange;

pub(super) use compile::compile_dag;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) struct ColumnKey(Vec<PathStep>);

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum PathStep {
    Slot(u32),
    ConstructorField(SymbolId, u32),
    /// Field index in the canonical, label-sorted product representation.
    RecordField(u32),
}

impl ColumnKey {
    pub(super) fn root() -> Self {
        Self(vec![PathStep::Slot(0)])
    }

    fn slot(index: usize) -> Self {
        Self(vec![PathStep::Slot(index as u32)])
    }

    fn child(&self, step: PathStep) -> Self {
        let mut path = self.0.clone();
        path.push(step);
        Self(path)
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(super) enum SurfacePattern {
    Any {
        ty: TypeId,
    },
    Var {
        id: LocalId,
        ty: TypeId,
        span: TextRange,
    },
    Constructor {
        symbol: SymbolId,
        arguments: Vec<SurfacePattern>,
        ty: TypeId,
        span: TextRange,
    },
    Record {
        fields: Vec<(String, SurfacePattern)>,
        ty: TypeId,
        span: TextRange,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Row {
    patterns: Vec<SurfacePattern>,
    bindings: Vec<(LocalId, ColumnKey)>,
    branch: usize,
    span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct Column {
    key: ColumnKey,
    ty: TypeId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct MatrixKey {
    columns: Vec<Column>,
    rows: Vec<Row>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DecisionDag {
    pub(super) root: NodeId,
    pub(super) nodes: Vec<Decision>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) struct NodeId(pub(super) usize);

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Decision {
    Leaf {
        branch: usize,
        actions: Vec<Action>,
        span: TextRange,
    },
    Fail {
        span: TextRange,
    },
    Switch {
        column: ColumnKey,
        ty: TypeId,
        edges: Vec<DecisionEdge>,
        default_actions: Vec<Action>,
        default: Option<NodeId>,
        span: TextRange,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) struct DecisionEdge {
    pub(super) test: Test,
    pub(super) actions: Vec<Action>,
    pub(super) target: NodeId,
    pub(super) span: TextRange,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(super) enum Test {
    Constructor { symbol: SymbolId, tag: u32 },
    Irrefutable,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum Action {
    Map {
        source: ColumnKey,
        target: ColumnKey,
        span: TextRange,
    },
    Project {
        source: ColumnKey,
        target: ColumnKey,
        /// Constructor ordinal or canonical record field index.
        field: u32,
        source_type: TypeId,
        declared_type: TypeId,
        target_type: TypeId,
        constructor: Option<(SymbolId, u32)>,
        newtype: bool,
        span: TextRange,
    },
    Bind {
        id: LocalId,
        source: ColumnKey,
    },
    TestTag {
        type_id: TypeId,
        tag: u32,
    },
}

#[derive(Clone, Debug)]
pub(super) struct DecisionSurface {
    pub(super) cases: Vec<CaseSignature>,
    pub(super) record_fields: Vec<(String, TypeId)>,
    pub(super) is_record: bool,
}

#[derive(Clone, Debug)]
pub(super) struct CaseSignature {
    pub(super) symbol: SymbolId,
    pub(super) tag: u32,
    pub(super) arity: usize,
    pub(super) irrefutable: bool,
    pub(super) field_types: Vec<TypeId>,
}

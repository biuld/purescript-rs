//! Checked state flow produced from immutable source Core, before projection.
//!
//! Operation identities address actual call expressions in this arena. Each
//! state lambda supplies a parameter root; evaluation never invents a state.
use super::dependency::{Block, Dependency, DependencyId, Graph, RegionId, Terminator, Transition};
use crate::{Expr, ExprKind, Module, TypeId, VerifyError};
use psrs_hir::{LocalId, ModuleId};
use psrs_span::TextRange;
use std::collections::HashMap;

mod branches;
mod eval;
mod scan;
mod summary;
#[cfg(test)]
mod tests;
mod verify;

/// A source invocation and the dependency it consumes/produces on return.
#[derive(Debug)]
pub struct Operation<'a> {
    pub expression: &'a Expr,
    pub input: DependencyId,
    pub output: DependencyId,
}

/// Evidence for one lexical state-function body. Region IDs are local to this
/// function, while `region` retains its checked source type identity.
#[derive(Debug)]
pub struct FunctionFlow<'a> {
    source: &'a Module,
    pub owner: ModuleId,
    pub lambda: &'a Expr,
    pub region: TypeId,
    pub graph: Graph,
    pub operations: Vec<Operation<'a>>,
}

/// Validate State-to-Step lambdas, including nested and uncalled library bodies.
/// Other raw-state callable/storage shapes still require a checked lowering
/// contract before projection; this does not publish evidence for those shapes.
/// Ordinary Core type and lexical-scope checks precede this state-flow check.
/// No source type, expression, or local identity is changed.
pub fn check(module: &Module) -> Result<Vec<FunctionFlow<'_>>, Vec<VerifyError>> {
    let mut flows = Vec::new();
    let mut errors = Vec::new();
    for declaration in &module.declarations {
        scan::expression(
            module,
            declaration.symbol.module,
            &declaration.value,
            &mut flows,
            &mut errors,
        );
    }
    if errors.is_empty() {
        Ok(flows)
    } else {
        Err(errors)
    }
}

#[derive(Clone, Debug)]
enum Value {
    Opaque,
    State(DependencyId),
    Record(HashMap<String, Value>),
    Constructor(Vec<Value>),
    Terminated,
}

impl Value {
    fn state(&self) -> Option<DependencyId> {
        if let Self::State(id) = self {
            Some(*id)
        } else {
            None
        }
    }

    fn remap(&mut self, old: DependencyId, new: DependencyId) {
        match self {
            Self::State(id) if *id == old => *id = new,
            Self::Record(fields) => {
                for field in fields.values_mut() {
                    field.remap(old, new);
                }
            }
            Self::Constructor(fields) => {
                for field in fields {
                    field.remap(old, new);
                }
            }
            _ => {}
        }
    }

    fn resolve(&mut self, path: &Path) {
        match self {
            Self::State(id) => *id = path.resolve(*id),
            Self::Record(fields) => {
                for field in fields.values_mut() {
                    field.resolve(path);
                }
            }
            Self::Constructor(fields) => {
                for field in fields {
                    field.resolve(path);
                }
            }
            _ => {}
        }
    }
}

#[derive(Clone)]
struct Path {
    block: usize,
    current: DependencyId,
    revision: u32,
    locals: HashMap<LocalId, Value>,
    /// Pure control-flow joins preserve the incoming dependency's meaning.
    /// Edges always point to newer IDs, including aliases evaluated earlier.
    aliases: HashMap<DependencyId, DependencyId>,
}

impl Path {
    fn resolve(&self, mut id: DependencyId) -> DependencyId {
        while let Some(next) = self.aliases.get(&id) {
            debug_assert!(next.0 > id.0);
            id = *next;
        }
        id
    }
}

struct Builder<'a> {
    module: &'a Module,
    owner: ModuleId,
    region: TypeId,
    next: u32,
    blocks: Vec<Block>,
    operations: Vec<Operation<'a>>,
}

impl<'a> Builder<'a> {
    fn fresh(&mut self) -> DependencyId {
        let id = DependencyId(self.next);
        self.next += 1;
        id
    }

    fn error(&self, span: TextRange, message: &'static str) -> VerifyError {
        VerifyError {
            module: self.owner,
            span,
            message,
        }
    }

    fn transition(
        &mut self,
        expression: &'a Expr,
        input: DependencyId,
        path: &mut Path,
    ) -> Result<DependencyId, VerifyError> {
        if input != path.current {
            return Err(self.error(expression.span, "state call consumes a stale dependency"));
        }
        let output = self.fresh();
        let operation = self.operations.len() as u32;
        self.operations.push(Operation {
            expression,
            input,
            output,
        });
        self.blocks[path.block].transitions.push(Transition {
            operation,
            input,
            output: Dependency {
                id: output,
                region: RegionId(0),
            },
            span: expression.span,
        });
        path.current = output;
        path.revision += 1;
        Ok(output)
    }

    fn state_value(
        &self,
        value: &Value,
        path: &Path,
        span: TextRange,
    ) -> Result<DependencyId, VerifyError> {
        value
            .state()
            .map(|id| path.resolve(id))
            .ok_or_else(|| self.error(span, "state value has no checked producer provenance"))
    }

    fn returned(&self, value: &Value, path: &Path, span: TextRange) -> Result<(), VerifyError> {
        let Value::Record(fields) = value else {
            return Err(self.error(span, "state function result has no checked state field"));
        };
        let state = fields
            .get("state")
            .and_then(Value::state)
            .ok_or_else(|| self.error(span, "state return has no checked producer provenance"))?;
        if path.resolve(state) != path.current {
            return Err(self.error(span, "state return discards an executed operation"));
        }
        Ok(())
    }
}

fn function<'a>(
    module: &'a Module,
    owner: ModuleId,
    lambda: &'a Expr,
) -> Result<FunctionFlow<'a>, VerifyError> {
    let flow = derive(module, owner, lambda)?;
    flow.verify(module)?;
    Ok(flow)
}

fn derive<'a>(
    module: &'a Module,
    owner: ModuleId,
    lambda: &'a Expr,
) -> Result<FunctionFlow<'a>, VerifyError> {
    let ExprKind::Lambda { binder, body } = &lambda.kind else {
        unreachable!()
    };
    let region = super::region(module, binder.ty).expect("scan selected a checked State parameter");
    let (state, _) = super::step(module, body.ty).ok_or(VerifyError {
        module: owner,
        span: body.span,
        message: "state function must return a closed state/value record",
    })?;
    if !module.types_equivalent(binder.ty, state) {
        return Err(VerifyError {
            module: owner,
            span: body.span,
            message: "state function returns a different region",
        });
    }
    let input = DependencyId(0);
    let mut builder = Builder {
        module,
        owner,
        region,
        next: 1,
        blocks: vec![Block {
            id: 0,
            parameters: vec![Dependency {
                id: input,
                region: RegionId(0),
            }],
            transitions: Vec::new(),
            terminator: Terminator::Trap,
            span: lambda.span,
        }],
        operations: Vec::new(),
    };
    let mut path = Path {
        block: 0,
        current: input,
        revision: 0,
        locals: HashMap::from([(binder.id, Value::State(input))]),
        aliases: HashMap::new(),
    };
    let value = builder.eval(body, &mut path)?;
    if !matches!(value, Value::Terminated) {
        builder.returned(&value, &path, body.span)?;
        builder.blocks[path.block].terminator = Terminator::Return(vec![path.current]);
    }
    let graph = Graph {
        entry: 0,
        regions: vec![RegionId(0)],
        blocks: builder.blocks,
    };
    let flow = FunctionFlow {
        source: module,
        owner,
        lambda,
        region,
        graph,
        operations: builder.operations,
    };
    Ok(flow)
}

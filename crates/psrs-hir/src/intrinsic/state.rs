//! Explicit state operations and their source signatures.
use crate::{Type, TypeField, TypeId, TypeKind, TypeParameter};
use psrs_span::TextRange;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StateOperation {
    RunWorld,
    RunRegion,
}

fn ty(kind: TypeKind) -> Type {
    Type {
        kind,
        span: TextRange::new(0, 0),
    }
}
fn variable(name: &str) -> Type {
    ty(TypeKind::Variable(name.into()))
}
fn named(value: TypeId) -> Type {
    ty(TypeKind::Named(value))
}
fn apply(head: Type, argument: Type) -> Type {
    ty(TypeKind::Application(Box::new(head), Box::new(argument)))
}
fn arrow(parameter: Type, result: Type) -> Type {
    ty(TypeKind::Function {
        parameter: Box::new(parameter),
        result: Box::new(result),
    })
}
fn forall(names: &[&str], body: Type) -> Type {
    ty(TypeKind::Forall {
        variables: names
            .iter()
            .map(|name| TypeParameter {
                name: (*name).into(),
                name_span: TextRange::new(0, 0),
                kind: None,
            })
            .collect(),
        body: Box::new(body),
    })
}
fn state(region: Type) -> Type {
    apply(named(TypeId::PRIM_STATE), region)
}
fn step(region: Type, value: Type) -> Type {
    let field = |label: &str, ty| TypeField {
        label: label.into(),
        label_span: TextRange::new(0, 0),
        ty,
        span: TextRange::new(0, 0),
    };
    ty(TypeKind::Record {
        fields: vec![field("state", state(region)), field("value", value)],
        tail: None,
    })
}

pub(super) fn run_world() -> Type {
    let region = named(TypeId::PRIM_REAL_WORLD);
    forall(
        &["a"],
        arrow(
            arrow(state(region.clone()), step(region, variable("a"))),
            variable("a"),
        ),
    )
}
pub(super) fn run_region() -> Type {
    forall(
        &["a"],
        arrow(
            forall(
                &["s"],
                arrow(state(variable("s")), step(variable("s"), variable("a"))),
            ),
            variable("a"),
        ),
    )
}

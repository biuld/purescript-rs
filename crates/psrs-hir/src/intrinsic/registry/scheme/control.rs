use super::*;

/// `Control f a`: a computation over the opaque request functor `f` that
/// yields an `a`, possibly after suspending at `perform`.
fn control(of: Type, value: Type) -> Type {
    apply(apply(named(TypeId::CONTROL), of), value)
}

/// `Prim.Control.return :: forall f a. a -> Control f a`.
pub(in crate::intrinsic::registry) fn control_return() -> Type {
    forall(
        &["f", "a"],
        arrow(variable("a"), control(variable("f"), variable("a"))),
    )
}

/// `Prim.Control.bind :: forall f a b. Control f a -> (a -> Control f b) -> Control f b`.
pub(in crate::intrinsic::registry) fn control_bind() -> Type {
    forall(
        &["f", "a", "b"],
        arrow(
            control(variable("f"), variable("a")),
            arrow(
                arrow(variable("a"), control(variable("f"), variable("b"))),
                control(variable("f"), variable("b")),
            ),
        ),
    )
}

/// `Prim.Control.perform :: forall f a. f a -> Control f a`.
pub(in crate::intrinsic::registry) fn control_perform() -> Type {
    forall(
        &["f", "a"],
        arrow(
            apply(variable("f"), variable("a")),
            control(variable("f"), variable("a")),
        ),
    )
}

/// A closed two-field record, used for the `(state, value)` result.
fn record(fields: &[(&str, Type)]) -> Type {
    Type {
        kind: TypeKind::Record {
            fields: fields
                .iter()
                .map(|(label, ty)| TypeField {
                    label: (*label).to_owned(),
                    label_span: empty_span(),
                    ty: ty.clone(),
                    span: empty_span(),
                })
                .collect(),
            tail: None,
        },
        span: empty_span(),
    }
}

/// `Prim.Control.run`, the closed state-threading fold:
/// `forall f a s. s -> (forall x. s -> f x -> (x -> Control f a) ->
/// (s, Control f a)) -> Control f a -> (s, a)`.
pub(in crate::intrinsic::registry) fn control_run() -> Type {
    let handler = forall(
        &["x"],
        arrow(
            variable("s"),
            arrow(
                apply(variable("f"), variable("x")),
                arrow(
                    arrow(variable("x"), control(variable("f"), variable("a"))),
                    record(&[
                        ("_1", variable("s")),
                        ("_2", control(variable("f"), variable("a"))),
                    ]),
                ),
            ),
        ),
    );
    forall(
        &["f", "a", "s"],
        arrow(
            variable("s"),
            arrow(
                handler,
                arrow(
                    control(variable("f"), variable("a")),
                    record(&[("_1", variable("s")), ("_2", variable("a"))]),
                ),
            ),
        ),
    )
}

/// `Prim.Control.handle`, the open eliminator:
/// `forall f g u a s. s`
///   `-> (forall x. s -> f x -> (x -> Control u a) -> (s, Control u a))`
///   `-> (forall x r. u x -> (f x -> r) -> (g x -> r) -> r)`  (project)
///   `-> (forall x. g x -> u x)`                              (inject)
///   `-> Control u a -> Control g (s, a)`.
pub(in crate::intrinsic::registry) fn control_handle() -> Type {
    let handler = forall(
        &["x"],
        arrow(
            variable("s"),
            arrow(
                apply(variable("f"), variable("x")),
                arrow(
                    arrow(variable("x"), control(variable("u"), variable("a"))),
                    record(&[
                        ("_1", variable("s")),
                        ("_2", control(variable("u"), variable("a"))),
                    ]),
                ),
            ),
        ),
    );
    let project = forall(
        &["x", "r"],
        arrow(
            apply(variable("u"), variable("x")),
            arrow(
                arrow(apply(variable("f"), variable("x")), variable("r")),
                arrow(
                    arrow(apply(variable("g"), variable("x")), variable("r")),
                    variable("r"),
                ),
            ),
        ),
    );
    let inject = forall(
        &["x"],
        arrow(
            apply(variable("g"), variable("x")),
            apply(variable("u"), variable("x")),
        ),
    );
    forall(
        &["f", "g", "u", "a", "s"],
        arrow(
            variable("s"),
            arrow(
                handler,
                arrow(
                    project,
                    arrow(
                        inject,
                        arrow(
                            control(variable("u"), variable("a")),
                            control(
                                variable("g"),
                                record(&[("_1", variable("s")), ("_2", variable("a"))]),
                            ),
                        ),
                    ),
                ),
            ),
        ),
    )
}

fn resumption(f: Type, a: Type, b: Type) -> Type {
    apply(apply(apply(named(TypeId::RESUMPTION), f), a), b)
}

/// Wrap an ordinary continuation with independently checked one-shot ownership.
pub(in crate::intrinsic::registry) fn control_once() -> Type {
    forall(
        &["f", "a", "b"],
        arrow(
            arrow(variable("a"), control(variable("f"), variable("b"))),
            resumption(variable("f"), variable("a"), variable("b")),
        ),
    )
}

/// Construct a resume node; ownership is consumed when it is interpreted.
pub(in crate::intrinsic::registry) fn control_resume() -> Type {
    forall(
        &["f", "a", "b"],
        arrow(
            resumption(variable("f"), variable("a"), variable("b")),
            arrow(variable("a"), control(variable("f"), variable("b"))),
        ),
    )
}

/// Cancel pending ownership immediately; cancelling an active/used owner traps.
pub(in crate::intrinsic::registry) fn control_cancel() -> Type {
    forall(
        &["f", "a", "b"],
        arrow(
            resumption(variable("f"), variable("a"), variable("b")),
            builtin(BuiltinType::Unit),
        ),
    )
}

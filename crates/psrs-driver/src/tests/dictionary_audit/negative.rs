//! Full-module negative fixtures at the Typed Core boundary: the backend must
//! reject unresolved or malformed class evidence instead of searching for an
//! instance (DICT-01) or inventing a dictionary.

use super::fixtures::{declaration, typed};
use psrs_hir::{LocalId, ModuleId, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use psrs_thir as thir;

struct Types {
    integer: thir::TypeId,
    boolean: thir::TypeId,
    method: thir::TypeId,
    dictionary: thir::TypeId,
}

impl Types {
    fn new() -> Self {
        Self {
            integer: thir::TypeId(0),
            boolean: thir::TypeId(1),
            method: thir::TypeId(2),
            dictionary: thir::TypeId(3),
        }
    }

    fn list(&self) -> Vec<thir::Type> {
        vec![
            thir::Type::I32,
            thir::Type::Boolean,
            thir::Type::Function {
                parameter: self.integer,
                result: self.boolean,
            },
            thir::Type::Record(vec![("isPositive".into(), self.method)]),
        ]
    }
}

fn module_with(bad: thir::Declaration, span: TextRange) -> thir::Module {
    let types = Types::new();
    let main = typed(thir::ExprKind::Integer(42), types.integer, span);
    thir::Module {
        id: ModuleId(0),
        name: "Main".into(),
        externals: Vec::new(),
        types: types.list(),
        newtype_ids: Vec::new(),
        constructors: Vec::new(),
        declarations: vec![
            declaration(
                SymbolId::new(ModuleId(0), 0),
                "main",
                types.integer,
                main,
                span,
            ),
            bad,
        ],
        span,
    }
}

#[test]
fn rejects_unresolved_given_evidence() {
    let span = TextRange::new(0, 16);
    let types = Types::new();
    let eq_class = HirTypeId::new(ModuleId(0), 0);
    let superclass = thir::Evidence {
        kind: thir::EvidenceKind::Superclass {
            parent: Box::new(thir::Evidence {
                kind: thir::EvidenceKind::Given(LocalId(9)),
                class_id: eq_class,
                ty: types.dictionary,
                span,
            }),
            field: "isPositive".into(),
        },
        class_id: eq_class,
        ty: types.method,
        span,
    };
    let bad = declaration(
        SymbolId::new(ModuleId(0), 1),
        "bad",
        types.method,
        typed(thir::ExprKind::Evidence(superclass), types.method, span),
        span,
    );
    let fixture = module_with(bad, span);
    // The THIR evidence is well shaped, but its `Given` local is never bound,
    // so Core lowering must reject it rather than resolve a dictionary.
    let core = psrs_core::lower_module_unverified(fixture).expect("THIR should lower structurally");
    let errors = core.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("local reference is not in scope")),
        "{errors:?}"
    );
    let module = module_with(
        declaration(
            SymbolId::new(ModuleId(0), 1),
            "bad",
            types.method,
            typed(
                thir::ExprKind::Evidence(thir::Evidence {
                    kind: thir::EvidenceKind::Superclass {
                        parent: Box::new(thir::Evidence {
                            kind: thir::EvidenceKind::Given(LocalId(9)),
                            class_id: eq_class,
                            ty: types.dictionary,
                            span,
                        }),
                        field: "isPositive".into(),
                    },
                    class_id: eq_class,
                    ty: types.method,
                    span,
                }),
                types.method,
                span,
            ),
            span,
        ),
        span,
    );
    assert!(psrs_core::lower_module(module).is_err());
}

#[test]
fn rejects_superclass_evidence_with_a_missing_field() {
    let span = TextRange::new(0, 16);
    let types = Types::new();
    let eq_class = HirTypeId::new(ModuleId(0), 0);
    let evidence = thir::Evidence {
        kind: thir::EvidenceKind::Superclass {
            parent: Box::new(thir::Evidence {
                kind: thir::EvidenceKind::Given(LocalId(0)),
                class_id: eq_class,
                ty: types.dictionary,
                span,
            }),
            field: "missing".into(),
        },
        class_id: eq_class,
        ty: types.method,
        span,
    };
    let fixture = module_with(
        declaration(
            SymbolId::new(ModuleId(0), 1),
            "bad",
            types.method,
            typed(thir::ExprKind::Evidence(evidence), types.method, span),
            span,
        ),
        span,
    );
    let errors = fixture.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("superclass evidence field")),
        "{errors:?}"
    );
}

#[test]
fn rejects_instance_evidence_with_an_unknown_constructor() {
    let span = TextRange::new(0, 16);
    let types = Types::new();
    let eq_class = HirTypeId::new(ModuleId(0), 0);
    let evidence = thir::Evidence {
        kind: thir::EvidenceKind::Instance {
            constructor: SymbolId::new(ModuleId(0), 99),
            constructor_type: types.dictionary,
            context: Vec::new(),
        },
        class_id: eq_class,
        ty: types.dictionary,
        span,
    };
    let fixture = module_with(
        declaration(
            SymbolId::new(ModuleId(0), 1),
            "bad",
            types.dictionary,
            typed(thir::ExprKind::Evidence(evidence), types.dictionary, span),
            span,
        ),
        span,
    );
    let core = psrs_core::lower_module_unverified(fixture).expect("THIR should lower structurally");
    let errors = core.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("global reference is not declared")),
        "{errors:?}"
    );
}

#[test]
fn rejects_instance_evidence_with_a_context_arity_mismatch() {
    let span = TextRange::new(0, 16);
    let types = Types::new();
    let eq_class = HirTypeId::new(ModuleId(0), 0);
    let evidence = thir::Evidence {
        kind: thir::EvidenceKind::Instance {
            constructor: SymbolId::new(ModuleId(0), 1),
            // A construction result is not a function, so a non-empty context
            // cannot be applied to it.
            constructor_type: types.dictionary,
            context: vec![thir::Evidence {
                kind: thir::EvidenceKind::Given(LocalId(0)),
                class_id: eq_class,
                ty: types.dictionary,
                span,
            }],
        },
        class_id: eq_class,
        ty: types.dictionary,
        span,
    };
    let fixture = module_with(
        declaration(
            SymbolId::new(ModuleId(0), 1),
            "bad",
            types.dictionary,
            typed(thir::ExprKind::Evidence(evidence), types.dictionary, span),
            span,
        ),
        span,
    );
    let errors = fixture.verify().unwrap_err();
    assert!(
        errors
            .iter()
            .any(|error| error.message.contains("too few context arguments")),
        "{errors:?}"
    );
}

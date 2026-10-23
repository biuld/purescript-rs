use super::*;

fn checker() -> (Checker, hir::Module) {
    let module = hir::Module {
        id: hir::ModuleId(0),
        name: "Provider".into(),
        externals: Vec::new(),
        imports: Vec::new(),
        exports: None,
        declarations: Vec::new(),
        types: Vec::new(),
        instances: Vec::new(),
        fixities: Vec::new(),
        span: TextRange::new(0, 1),
    };
    let checker = Checker::new(
        &module,
        &HashMap::new(),
        TypecheckContext {
            known_types: &[],
            known_values: &[],
            imported_instances: &[],
            module_names: &HashMap::new(),
            checked_kinds: &CheckedKindEnv::default(),
            checked_values: &HashMap::new(),
        },
    );
    (checker, module)
}

#[test]
fn malformed_imports_report_unbound_variables_and_missing_kinds() {
    let kinds = psrs_kind::KindState::default().close_kinds(&[]);
    for ty in [
        ClosedType::Variable(0),
        ClosedType::ForAll {
            binders: vec![ClosedBinder {
                variable: 0,
                visible: false,
                kind: 7,
            }],
            body: Box::new(ClosedType::Variable(0)),
        },
    ] {
        let (mut checker, module) = checker();
        let signature = CheckedValueSignature {
            source_span: module.span,
            name_span: None,
            kinds: kinds.clone(),
            ty,
        };
        assert!(
            checker
                .synthesize_checked_use(SymbolId::new(module.id, 0), &signature, module.span)
                .is_none()
        );
        assert!(
            checker
                .state
                .errors
                .iter()
                .all(|error| error.kind == TypeCheckErrorKind::InvalidHir)
        );
        assert!(!checker.state.errors.is_empty());
    }
}

#[test]
fn a_producer_cannot_freeze_an_unknown_or_a_binder_without_its_kind() {
    for missing_kind in [false, true] {
        let (mut checker, mut module) = checker();
        let ty = checker.fresh();
        let InferType::Variable(id) = ty else {
            unreachable!()
        };
        let symbol = SymbolId::new(module.id, 0);
        let source = hir::Declaration {
            symbol,
            name: "value".into(),
            name_span: module.span,
            span: module.span,
            signature: None,
            value: hir::Expr {
                kind: hir::ExprKind::Integer("0".into()),
                span: module.span,
            },
        };
        module.declarations.push(source);
        let mut scheme = Scheme::monomorphic(ty.clone());
        if missing_kind {
            scheme.variables.push(id);
            checker.state.variable_kinds.remove(&id);
        }
        let declaration = InferredDeclaration {
            symbol,
            name: "value".into(),
            name_span: module.span,
            span: module.span,
            scheme,
            value: InferredExpr {
                kind: InferredExprKind::Integer(0),
                ty,
                span: module.span,
            },
        };
        assert!(
            checker
                .freeze_value_interfaces(&module, &[declaration])
                .is_empty()
        );
        assert!(
            checker
                .state
                .errors
                .iter()
                .any(|error| error.kind == TypeCheckErrorKind::InvalidHir)
        );
    }
}

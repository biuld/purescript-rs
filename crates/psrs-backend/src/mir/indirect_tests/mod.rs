use super::lower_module_with_registry;
use crate::abi::{self, SourceSignature, SourceType};
use crate::cc::{self, Assignment, AssignmentKind, External, Signature, ValueDecl, ValueShape};
use crate::types::ValueId;
use crate::{ExternalBinding, ExternalBindings, TargetCapabilities};
use psrs_hir::{FOREIGN_SYMBOL_BASE, ModuleId, SymbolId};
use psrs_span::TextRange;
use wit_parser::Resolve;

fn span() -> TextRange {
    TextRange::new(0, 1)
}

fn indirect_fixture() -> (cc::Module, ExternalBindings, Resolve) {
    let wit = "package wasi:io@0.2.12; interface streams { take: func(a0: s32, a1: s32, a2: s32, a3: s32, a4: s32, a5: s32, a6: s32, a7: s32, a8: s32, a9: s32, a10: s32, a11: s32, a12: s32, a13: s32, amount: f64, enabled: bool, wide: u64, ratio: f32); }";
    let mut resolve = Resolve::default();
    resolve
        .push_str("indirect-params.wit", wit)
        .expect("the indirect WIT fixture should resolve");

    let external_symbol = SymbolId::new(ModuleId::INTRINSICS, FOREIGN_SYMBOL_BASE);
    let main_symbol = SymbolId::new(ModuleId(0), 0);
    let source_parameters = (0..14)
        .map(|_| SourceType::Int)
        .chain([
            SourceType::Number,
            SourceType::Boolean,
            SourceType::Int,
            SourceType::Number,
        ])
        .collect::<Vec<_>>();
    let mut values = Vec::new();
    let mut assignments = Vec::new();
    let mut arguments = Vec::new();
    let mut external_parameters = Vec::new();

    for index in 0..18 {
        let id = ValueId(index);
        let (shape, kind) = match index {
            0..=13 => (ValueShape::Integer, AssignmentKind::Constant(index as i32)),
            14 => (
                ValueShape::Number,
                AssignmentKind::NumberConstant("3.25".into()),
            ),
            15 => (ValueShape::Boolean, AssignmentKind::Constant(1)),
            16 => (ValueShape::Integer, AssignmentKind::Constant(91)),
            _ => (
                ValueShape::Number,
                AssignmentKind::NumberConstant("2.5".into()),
            ),
        };
        values.push(ValueDecl { id, ty: shape });
        assignments.push(Assignment {
            destination: id,
            kind,
            span: span(),
        });
        arguments.push(id);
        external_parameters.push(shape);
    }

    let call_result = ValueId(18);
    values.push(ValueDecl {
        id: call_result,
        ty: ValueShape::Integer,
    });
    assignments.push(Assignment {
        destination: call_result,
        kind: AssignmentKind::DirectCall {
            function: external_symbol,
            arguments,
        },
        span: span(),
    });

    let module = cc::Module {
        name: "IndirectAbi".into(),
        externals: vec![External {
            symbol: external_symbol,
            signature: Some(Signature {
                parameters: external_parameters,
                result: ValueShape::Integer,
            }),
        }],
        representations: cc::RepresentationTable::default(),
        functions: vec![cc::Function {
            symbol: main_symbol,
            name: "main".into(),
            parameters: Vec::new(),
            values,
            assignments,
            result: call_result,
            result_type: ValueShape::Integer,
            span: span(),
        }],
        entry: Some(main_symbol),
        span: span(),
    };
    let bindings = ExternalBindings {
        imports: vec![ExternalBinding {
            symbol: external_symbol,
            interface: "wasi:io/streams".into(),
            function: "take".into(),
            signature: Some(SourceSignature {
                parameters: source_parameters,
                result: SourceType::Unit,
                span: span(),
            }),
        }],
    };
    (module, bindings, resolve)
}

mod composite;

#[test]
fn indirect_canonical_parameters_lower_to_an_artifact() {
    let (cc, bindings, resolve) = indirect_fixture();
    let target = TargetCapabilities {
        wasi_cli: false,
        ..TargetCapabilities::default()
    };
    let registry = abi::WasiRegistry::from_resolve(resolve, target);
    let (mir, mut registry) = lower_module_with_registry(cc, bindings, target, registry)
        .expect("P9 should lower indirect canonical parameters");
    assert!(
        mir.imports
            .iter()
            .any(|import| import.symbol == abi::REALLOC_SYMBOL)
    );

    let mir = super::opt::optimize(mir, target).expect("P10 MIR optimization should preserve ABI");
    let wasm = crate::wasm::lower_module_with_capabilities(&mir, &mut registry, target)
        .expect("P10 should lower the indirect call and allocator");
    let binary = crate::wasm::encode_module(&wasm).expect("the Wasm artifact should encode");
    crate::validator_for(target)
        .validate_all(&binary)
        .expect("the generated Wasm artifact should validate");
    let wat = wasmprinter::print_bytes(&binary).expect("the artifact should print as WAT");

    assert!(wat.contains("wasi:io/streams@0.2.12\" \"take\""));
    assert!(wat.contains("(param i32)"), "the import is indirect: {wat}");
    assert!(
        wat.contains("i32.store8"),
        "Boolean uses its canonical byte slot: {wat}"
    );
    assert!(
        wat.contains("i32.store8 offset=64"),
        "the byte slot is aligned: {wat}"
    );
    assert!(
        wat.contains("i64.store"),
        "u64 uses an eight-byte slot: {wat}"
    );
    assert!(
        wat.contains("i64.store offset=72"),
        "u64 is aligned to eight: {wat}"
    );
    assert!(
        wat.contains("f32.store"),
        "f32 uses a four-byte slot: {wat}"
    );
    assert!(
        wat.contains("f32.store offset=80"),
        "f32 follows the padded u64: {wat}"
    );
    assert!(
        wat.contains("f64.store"),
        "f64 uses an eight-byte slot: {wat}"
    );
    assert!(
        wat.contains("f64.store offset=56"),
        "f64 is aligned to eight: {wat}"
    );
    assert!(
        wat.contains("i32.const 88"),
        "the aligned record occupies 88 bytes: {wat}"
    );
    assert!(wat.contains("(export \"cabi_realloc\""));
}

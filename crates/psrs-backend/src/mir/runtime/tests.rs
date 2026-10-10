use super::*;
use crate::cc::{
    Assignment, AssignmentKind, RefShape, Reference, ReprId, Representation, Signature, ValueShape,
};
use crate::mir::planner::{GcPlanner, RepresentationPlanner};
use crate::types::{CompositeType, FieldType, StorageType};
use psrs_hir::{ModuleId, SymbolId};
use psrs_runtime::{StorageOperation, StorageValue};
use psrs_span::TextRange;

pub(super) fn fixture(operation: StorageOperation) -> (RuntimeBinding, cc::Module) {
    let span = TextRange::new(0, 1);
    let binding = RuntimeBinding {
        symbol: SymbolId::new(ModuleId(0), 1),
        source_module: ModuleId(0),
        module: psrs_runtime::STORAGE_MODULE.into(),
        function: operation.abi().export.into(),
        type_id: Some(psrs_core::TypeId(0)),
        span,
    };
    let reference = |id| {
        ValueShape::Reference(Reference {
            nullable: false,
            heap: RefShape::Repr(ReprId(id)),
        })
    };
    let shape = |role| match role {
        StorageValue::Int | StorageValue::Unit => ValueShape::Integer,
        StorageValue::Array => reference(0),
        StorageValue::Element | StorageValue::Any => cc::payload::erased_shape(),
    };
    let source = operation.source_contract();
    let mut parameters = source
        .parameters
        .iter()
        .copied()
        .map(shape)
        .collect::<Vec<_>>();
    parameters.push(ValueShape::State);
    let signature = Signature {
        parameters,
        result: reference(1),
    };
    let arguments = (0..signature.parameters.len())
        .map(|id| cc::ValueId(id as u32))
        .collect::<Vec<_>>();
    let result = cc::ValueId(arguments.len() as u32);
    let mut values = signature
        .parameters
        .iter()
        .enumerate()
        .map(|(id, ty)| cc::ValueDecl {
            id: cc::ValueId(id as u32),
            ty: *ty,
        })
        .collect::<Vec<_>>();
    values.push(cc::ValueDecl {
        id: result,
        ty: signature.result,
    });
    let owner = SymbolId::new(ModuleId(0), 0);
    let module = cc::Module {
        name: "RawStorageSignature".into(),
        externals: vec![cc::External {
            symbol: binding.symbol,
            signature: Some(signature),
            projection: None,
        }],
        representations: cc::RepresentationTable {
            representations: vec![
                Representation::Array {
                    element: cc::payload::erased_shape(),
                },
                Representation::Product {
                    fields: vec![ValueShape::State, shape(source.payload)],
                },
            ],
            ..Default::default()
        },
        functions: vec![cc::Function {
            symbol: owner,
            name: "action".into(),
            parameters: arguments.clone(),
            values,
            assignments: vec![Assignment {
                destination: result,
                kind: AssignmentKind::DirectCall {
                    function: binding.symbol,
                    arguments,
                },
                span,
            }],
            result,
            result_type: reference(1),
            span,
        }],
        entry: Some(owner),
        span,
    };
    (binding, module)
}

#[test]
fn raw_signatures_use_application_gc_types_and_distinguish_void_from_never() {
    for operation in StorageOperation::ALL {
        let (binding, logical) = fixture(operation);
        let projected =
            crate::mir::state::project(logical, std::slice::from_ref(&binding)).unwrap();
        let layout = GcPlanner::default()
            .plan_module(&projected.physical)
            .unwrap();
        let (import, result) = plan_import(&binding, &projected.logical, &layout).unwrap();
        let eq = ValueType::Ref(RefType {
            nullable: true,
            heap: HeapType::Eq,
        });
        let array = || {
            layout
                .value_type(&ValueShape::Reference(Reference {
                    nullable: false,
                    heap: RefShape::Repr(ReprId(0)),
                }))
                .unwrap()
        };
        let (parameters, raw_result, result_kind) = match operation {
            StorageOperation::Fill => (
                vec![ValueType::I32, eq],
                Some(array()),
                RawCallResult::Value,
            ),
            StorageOperation::Read => (
                vec![array(), ValueType::I32],
                Some(eq),
                RawCallResult::Value,
            ),
            StorageOperation::Write => {
                (vec![array(), ValueType::I32, eq], None, RawCallResult::Unit)
            }
            StorageOperation::Trap => (vec![], None, RawCallResult::Never),
        };
        assert_eq!(import.symbol, binding.symbol);
        assert_eq!(import.parameters, parameters);
        assert_eq!(import.result, raw_result);
        assert_eq!(result, result_kind);
        assert!(
            projected.physical.externals[0]
                .signature
                .as_ref()
                .unwrap()
                .parameters
                .iter()
                .all(|parameter| *parameter != ValueShape::State)
        );
        let context = RuntimeContext::checked(
            &[binding],
            projected.logical.clone(),
            &projected.physical,
            &layout,
        );
        assert_eq!(context.unwrap().imports.len(), 1);
    }
}

#[test]
fn raw_signature_rejects_a_contradictory_planned_array_definition() {
    for field in [
        FieldType {
            mutable: false,
            storage: StorageType::Ref(RefType {
                nullable: true,
                heap: HeapType::Eq,
            }),
        },
        FieldType {
            mutable: true,
            storage: StorageType::Ref(RefType {
                nullable: false,
                heap: HeapType::Eq,
            }),
        },
        FieldType {
            mutable: true,
            storage: StorageType::I32,
        },
    ] {
        let (binding, logical) = fixture(StorageOperation::Read);
        let projected =
            crate::mir::state::project(logical, std::slice::from_ref(&binding)).unwrap();
        let mut layout = GcPlanner::default()
            .plan_module(&projected.physical)
            .unwrap();
        let id = layout.repr_index(ReprId(0)).unwrap();
        layout
            .types
            .iter_mut()
            .flat_map(|group| &mut group.0)
            .nth(id.0 as usize)
            .unwrap()
            .composite = CompositeType::Array(field);
        let errors = plan_import(&binding, &projected.logical, &layout).unwrap_err();
        assert!(errors[0].message.contains("mutable nullable-eqref storage"));
    }
}

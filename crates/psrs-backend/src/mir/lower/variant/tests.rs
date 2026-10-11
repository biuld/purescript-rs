//! CC admits both abstract and concrete common variant result shapes.

use crate::cc::{
    Assignment, AssignmentKind, Function, Module, RefShape, Reference, Representation,
    RepresentationTable, ValueDecl, ValueId, ValueShape, VariantCase,
};
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;

#[test]
fn variant_construction_lowers_at_abstract_and_common_representation_shapes() {
    for case in 0..2 {
        let mut table = RepresentationTable::default();
        let variant = table.reserve();
        table.set(
            variant,
            Representation::Variant {
                cases: vec![
                    VariantCase {
                        tag: 0,
                        fields: vec![ValueShape::Integer],
                    },
                    VariantCase {
                        tag: 1,
                        fields: vec![],
                    },
                ],
            },
        );
        for heap in [RefShape::Aggregate, RefShape::Repr(variant)] {
            let shape = ValueShape::Reference(Reference {
                nullable: false,
                heap,
            });
            let span = TextRange::new(0, 1);
            let value = ValueId(0);
            let result = ValueId(1);
            let module = Module {
                name: "VariantResult".into(),
                externals: vec![],
                representations: table.clone(),
                entry: None,
                span,
                functions: vec![Function {
                    symbol: SymbolId::new(ModuleId(0), 0),
                    name: "construct".into(),
                    parameters: vec![],
                    span,
                    result,
                    result_type: shape,
                    values: vec![
                        ValueDecl {
                            id: value,
                            ty: ValueShape::Integer,
                        },
                        ValueDecl {
                            id: result,
                            ty: shape,
                        },
                    ],
                    assignments: vec![
                        Assignment {
                            destination: value,
                            kind: AssignmentKind::Constant(42),
                            span,
                        },
                        Assignment {
                            destination: result,
                            kind: AssignmentKind::VariantNew {
                                destination: result,
                                representation: variant,
                                case,
                                fields: if case == 0 { vec![value] } else { vec![] },
                            },
                            span,
                        },
                    ],
                }],
            };
            let (mir, _) = crate::mir::lower_module(module).expect("valid variant construction");
            crate::mir::verify_module(&mir)
                .expect("the constructed case widens to its declared shape");
        }
    }
}

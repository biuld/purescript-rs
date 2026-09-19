use crate::abi::{self, SourceSignature, SourceType};
use crate::types::{RecGroup, RefType};
use crate::{BackendError, annotate_errors};
use psrs_core::{Module as CoreModule, Primitive};
use psrs_hir::{ExternalKind, SymbolId, TypeId as HirTypeId};
use psrs_span::TextRange;
use std::collections::HashMap;

mod case;
mod layout;
mod lower;
mod verify;

use layout::{
    Signature, aggregate_type_ids, declaration_shape, enum_type_ids, runtime_signature, type_layout,
};
use lower::{LoweringContext, lower_function};

pub use crate::types::{ValueDecl, ValueId, ValueType};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Module {
    pub name: String,
    pub externals: Vec<External>,
    pub types: Vec<RecGroup>,
    pub functions: Vec<Function>,
    /// The program entry declaration, if selected by the driver. A stable symbol
    /// rather than a source name, per `docs/design/D-02-wasm-lowering.md`.
    pub entry: Option<SymbolId>,
    pub span: TextRange,
}

/// A source WIT binding after its HIR type has been reduced to the ABI
/// vocabulary. MIR resolves the binding and stores only canonical imports.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct External {
    pub symbol: SymbolId,
    pub interface: String,
    pub function: String,
    pub signature: Option<SourceSignature>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Function {
    pub symbol: SymbolId,
    pub name: String,
    pub parameters: Vec<ValueId>,
    pub values: Vec<ValueDecl>,
    pub assignments: Vec<Assignment>,
    pub result: ValueId,
    pub result_type: ValueType,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Assignment {
    pub destination: ValueId,
    pub kind: AssignmentKind,
    pub span: TextRange,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AssignmentKind {
    Constant(i32),
    StringConstant(String),
    Primitive {
        op: Primitive,
        left: ValueId,
        right: ValueId,
    },
    DirectCall {
        function: SymbolId,
        arguments: Vec<ValueId>,
    },
    FunctionRef {
        function: SymbolId,
        type_index: u32,
    },
    IndirectCall {
        function: ValueId,
        type_index: u32,
        arguments: Vec<ValueId>,
    },
    RefTest {
        destination: ValueId,
        value: ValueId,
        reference: RefType,
    },
    RefCast {
        destination: ValueId,
        value: ValueId,
        reference: RefType,
    },
    StructNew {
        destination: ValueId,
        type_index: u32,
        arguments: Vec<ValueId>,
    },
    StructGet {
        destination: ValueId,
        type_index: u32,
        field: u32,
        value: ValueId,
    },
    ArrayNew {
        destination: ValueId,
        type_index: u32,
        elements: Vec<ValueId>,
    },
    ArrayLen {
        destination: ValueId,
        value: ValueId,
    },
    ArrayGet {
        destination: ValueId,
        type_index: u32,
        value: ValueId,
        index: ValueId,
    },
    ArraySet {
        destination: ValueId,
        type_index: u32,
        value: ValueId,
        index: ValueId,
        new_value: ValueId,
    },
    If {
        condition: ValueId,
        then_assignments: Vec<Assignment>,
        then_value: ValueId,
        else_assignments: Vec<Assignment>,
        else_value: ValueId,
    },
}

pub fn lower_module(module: CoreModule) -> Result<Module, Vec<BackendError>> {
    if let Err(errors) = module.verify() {
        return Err(annotate_errors(
            errors
                .into_iter()
                .map(|error| BackendError::new("P8 Core verification", error.span, error.message))
                .collect(),
            module.entry.map(|entry| entry.module),
        ));
    }
    let newtype_ids = module.newtype_ids.iter().copied().collect();
    let enum_types = enum_type_ids(&module, &newtype_ids);
    let aggregate_types = aggregate_type_ids(&module, &newtype_ids);
    let layout = type_layout(&module, &enum_types, &aggregate_types, &newtype_ids)?;
    let mut constructor_tags = HashMap::new();
    let mut constructors_by_type: HashMap<HirTypeId, Vec<(SymbolId, u32)>> = HashMap::new();
    for constructor in &module.constructors {
        constructor_tags.insert(constructor.symbol, constructor.tag);
        constructors_by_type
            .entry(constructor.type_id)
            .or_default()
            .push((constructor.symbol, constructor.tag));
    }
    let mut signatures = HashMap::new();
    for declaration in &module.declarations {
        let signature = declaration_shape(
            declaration,
            &module,
            &enum_types,
            &aggregate_types,
            &newtype_ids,
            &layout.array_types,
            &layout.record_types,
            &layout.function_types,
        )
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(declaration.symbol.module))
                .collect::<Vec<_>>()
        })?;
        signatures.insert(declaration.symbol, signature);
    }
    let mut externals = Vec::new();
    for external in &module.externals {
        if let ExternalKind::Wit {
            interface,
            function,
        } = &external.kind
        {
            let source_signature = external.signature.as_ref().and_then(abi::source_signature);
            if let Some(signature) = source_signature.as_ref().and_then(cc_signature) {
                signatures.insert(external.symbol, signature);
            }
            externals.push(External {
                symbol: external.symbol,
                interface: interface.clone(),
                function: function.clone(),
                signature: source_signature,
            });
        } else if let Some(signature) = runtime_signature(external) {
            signatures.insert(external.symbol, signature);
        }
    }
    let mut functions = Vec::with_capacity(module.declarations.len());
    let context = LoweringContext {
        module: &module,
        signatures: &signatures,
        enum_types: &enum_types,
        aggregate_types: &aggregate_types,
        newtype_ids: &newtype_ids,
        boxed_i32_type: layout.boxed_i32_type,
        array_types: &layout.array_types,
        record_types: &layout.record_types,
        constructor_tags: &constructor_tags,
        constructors_by_type: &constructors_by_type,
        constructor_types: &layout.constructor_types,
        function_types: &layout.function_types,
    };
    for declaration in &module.declarations {
        let (lowered, generated) = lower_function(declaration, &context).map_err(|errors| {
            errors
                .into_iter()
                .map(|error| error.with_module(declaration.symbol.module))
                .collect::<Vec<_>>()
        })?;
        functions.push(lowered);
        functions.extend(generated);
    }
    let cc = Module {
        name: module.name,
        externals,
        types: layout.types,
        functions,
        entry: module.entry,
        span: module.span,
    };
    verify::verify_module(&cc)?;
    Ok(cc)
}

fn cc_signature(signature: &SourceSignature) -> Option<Signature> {
    Some(Signature {
        parameters: vec![ValueType::I32; signature.parameters.len()],
        result: scalar_source_type(signature.result)?,
    })
}

fn scalar_source_type(ty: SourceType) -> Option<ValueType> {
    Some(match ty {
        SourceType::Int | SourceType::String | SourceType::Unit => ValueType::I32,
        SourceType::Boolean => ValueType::Boolean,
    })
}

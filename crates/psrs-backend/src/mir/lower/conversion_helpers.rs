use crate::cc::{self, AggregateConvert, ValueConversion};
use crate::mir::Function;
use crate::types::ValueId;
use psrs_hir::{ModuleId, SymbolId};
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

/// One generated MIR function for each complete conversion plan that rebuilds
/// an aggregate. Scalar and identity conversions remain at the call site.
pub(in crate::mir) struct ConversionHelpers {
    symbols: HashMap<AggregateConvert, SymbolId>,
    functions: Vec<cc::Function>,
    used: HashSet<SymbolId>,
    next_symbol: u32,
}

impl ConversionHelpers {
    pub(in crate::mir) fn new(module: &cc::Module, scalar_helpers: &[Function]) -> Self {
        let used = module
            .functions
            .iter()
            .map(|function| function.symbol)
            .chain(module.externals.iter().map(|external| external.symbol))
            .chain(scalar_helpers.iter().map(|function| function.symbol))
            .collect();
        Self {
            symbols: HashMap::new(),
            functions: Vec::new(),
            used,
            next_symbol: u32::MAX,
        }
    }

    pub(super) fn intern(
        &mut self,
        conversion: &AggregateConvert,
        span: TextRange,
    ) -> Option<SymbolId> {
        if !rebuilds_aggregate(&conversion.plan) {
            return None;
        }
        if let Some(symbol) = self.symbols.get(conversion) {
            return Some(*symbol);
        }
        let symbol = loop {
            let symbol = SymbolId::new(ModuleId::INTRINSICS, self.next_symbol);
            self.next_symbol = self
                .next_symbol
                .checked_sub(1)
                .expect("conversion helper symbol space exhausted");
            if self.used.insert(symbol) {
                break symbol;
            }
        };
        let source = ValueId(0);
        let destination = ValueId(1);
        self.functions.push(cc::Function {
            symbol,
            name: format!("aggregate_convert_{}", self.functions.len()),
            parameters: vec![source],
            values: vec![
                cc::ValueDecl {
                    id: source,
                    ty: conversion.source,
                },
                cc::ValueDecl {
                    id: destination,
                    ty: conversion.destination,
                },
            ],
            assignments: vec![cc::Assignment {
                destination,
                kind: cc::AssignmentKind::AggregateConvert {
                    destination,
                    value: source,
                    conversion: conversion.clone(),
                },
                span,
            }],
            result: destination,
            result_type: conversion.destination,
            span,
        });
        self.symbols.insert(conversion.clone(), symbol);
        Some(symbol)
    }

    pub(in crate::mir) fn into_functions(self) -> Vec<cc::Function> {
        self.functions
    }
}

fn rebuilds_aggregate(plan: &ValueConversion) -> bool {
    match plan {
        ValueConversion::ArrayMap { .. } | ValueConversion::ProductMap { .. } => true,
        ValueConversion::Sequence(steps) => steps.iter().any(rebuilds_aggregate),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cc::{RefShape, Reference, ReprId, ValueShape};

    fn array_conversion(element: ValueConversion) -> AggregateConvert {
        AggregateConvert {
            source: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(ReprId(0)),
            }),
            destination: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Repr(ReprId(1)),
            }),
            plan: ValueConversion::ArrayMap {
                source: ReprId(0),
                target: ReprId(1),
                element: Box::new(element),
            },
        }
    }

    #[test]
    fn shares_equal_full_plans_but_not_different_nested_plans() {
        let module = cc::Module {
            name: "Helpers".into(),
            externals: Vec::new(),
            representations: cc::RepresentationTable::default(),
            functions: Vec::new(),
            entry: None,
            span: TextRange::new(0, 1),
        };
        let mut helpers = ConversionHelpers::new(&module, &[]);
        let span = TextRange::new(0, 1);
        let first = array_conversion(ValueConversion::Identity);
        let different = array_conversion(ValueConversion::EraseReference);
        assert!(
            helpers
                .intern(
                    &AggregateConvert {
                        source: first.source,
                        destination: first.source,
                        plan: ValueConversion::Identity,
                    },
                    span
                )
                .is_none()
        );
        let first_symbol = helpers.intern(&first, span).unwrap();
        assert_eq!(helpers.intern(&first, span), Some(first_symbol));
        assert_ne!(helpers.intern(&different, span), Some(first_symbol));
        assert_eq!(helpers.into_functions().len(), 2);
    }
}

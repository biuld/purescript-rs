use super::*;
use crate::cc::{RecordOperation, RecordStorage};

impl ConversionHelpers {
    pub(in crate::mir::lower) fn intern_record(
        &mut self,
        operation: &RecordOperation,
        storage: RecordStorage,
        span: TextRange,
    ) -> SymbolId {
        let operation = operation.with_argument(ValueId(1));
        let key = (operation.clone(), storage.representation());
        if let Some(symbol) = self.record_symbols.get(&key) {
            return *symbol;
        }
        let symbol = self.allocate_symbol();
        let record = ValueId(0);
        let result = ValueId(2);
        let mut parameters = vec![record];
        let mut values = vec![cc::ValueDecl {
            id: record,
            ty: cc::ValueShape::Reference(cc::Reference {
                nullable: false,
                heap: cc::RefShape::Repr(storage.representation()),
            }),
        }];
        if let Some((id, ty)) = operation.argument() {
            parameters.push(id);
            values.push(cc::ValueDecl { id, ty });
        }
        let result_type = operation.result_shape();
        values.push(cc::ValueDecl {
            id: result,
            ty: result_type,
        });
        self.functions.push(cc::Function {
            symbol,
            name: format!("record_op_{}", self.record_symbols.len()),
            parameters,
            values,
            assignments: vec![cc::Assignment {
                destination: result,
                kind: cc::AssignmentKind::Record {
                    destination: result,
                    record,
                    operation,
                },
                span,
            }],
            result,
            result_type,
            span,
        });
        self.record_symbols.insert(key, symbol);
        symbol
    }
}

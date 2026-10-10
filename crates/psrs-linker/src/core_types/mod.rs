//! Module-independent raw signatures retaining complete GC recursion groups.
use crate::{CoreSignature, CoreType};
use std::sync::Arc;
use wasmparser::{
    AbstractHeapType, CompositeInnerType, HeapType, PackedIndex, RecGroup, StorageType, SubType,
    ValType,
};

/// A closed reference contract. Construction resolves module-local indices;
/// consumers cannot fabricate an unresolved reference or erase its group.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CoreReference {
    nullable: bool,
    heap: Heap,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum Heap {
    Abstract {
        shared: bool,
        kind: AbstractHeapType,
    },
    Defined {
        group: Arc<[Definition]>,
        member: u32,
    },
    Bound(u32),
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct Definition {
    final_type: bool,
    supertype: Option<Heap>,
    composite: Composite,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Composite {
    Function {
        parameters: Vec<CoreType>,
        results: Vec<CoreType>,
    },
    Struct(Vec<Field>),
    Array(Field),
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Field {
    mutable: bool,
    storage: FieldStorage,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum FieldStorage {
    I8,
    I16,
    Value(CoreType),
}

/// The owner of conversion from a parsed module type space to raw contracts.
#[derive(Default)]
pub struct CoreTypes {
    definitions: Vec<Heap>,
}

impl CoreTypes {
    pub fn from_module(bytes: &[u8]) -> Result<Self, String> {
        wasmparser::Validator::new()
            .validate_all(bytes)
            .map_err(|error| error.to_string())?;
        let mut types = Self::default();
        for payload in wasmparser::Parser::new(0).parse_all(bytes) {
            if let wasmparser::Payload::TypeSection(section) =
                payload.map_err(|error| error.to_string())?
            {
                for group in section {
                    types.push(group.map_err(|error| error.to_string())?)?;
                }
            }
        }
        Ok(types)
    }

    pub(crate) fn push(&mut self, group: RecGroup) -> Result<(), String> {
        let start = self.definitions.len() as u32;
        let raw = group.into_types().collect::<Vec<_>>();
        if raw.is_empty() {
            return Err("raw contract has an empty recursion group".into());
        }
        let end = start + raw.len() as u32;
        let definitions = raw
            .iter()
            .map(|ty| self.definition(ty, start, end))
            .collect::<Result<Vec<_>, _>>()?;
        let group: Arc<[Definition]> = definitions.into();
        self.definitions
            .extend((0..raw.len() as u32).map(|member| Heap::Defined {
                group: Arc::clone(&group),
                member,
            }));
        Ok(())
    }

    pub fn value(&self, value: ValType) -> Result<CoreType, String> {
        self.convert_value(value, 0, 0)
    }

    pub fn signature(&self, index: u32) -> Result<CoreSignature, String> {
        let Some(Heap::Defined { group, member }) = self.definitions.get(index as usize) else {
            return Err("raw function has no defined type".into());
        };
        let Composite::Function {
            parameters,
            results,
        } = &group[*member as usize].composite
        else {
            return Err("raw function refers to a non-function type".into());
        };
        let close = |ty: &CoreType| match ty {
            CoreType::Ref(reference) => CoreType::Ref(CoreReference {
                nullable: reference.nullable,
                heap: match &reference.heap {
                    Heap::Bound(member) => Heap::Defined {
                        group: Arc::clone(group),
                        member: *member,
                    },
                    heap => heap.clone(),
                },
            }),
            ty => ty.clone(),
        };
        let result = match results.as_slice() {
            [] => None,
            [result] => Some(close(result)),
            _ => return Err("raw function has multiple results".into()),
        };
        Ok(CoreSignature {
            parameters: parameters.iter().map(close).collect(),
            result,
        })
    }

    fn convert_value(&self, value: ValType, start: u32, end: u32) -> Result<CoreType, String> {
        Ok(match value {
            ValType::I32 => CoreType::I32,
            ValType::I64 => CoreType::I64,
            ValType::F32 => CoreType::F32,
            ValType::F64 => CoreType::F64,
            ValType::V128 => CoreType::V128,
            ValType::Ref(reference) => CoreType::Ref(CoreReference {
                nullable: reference.is_nullable(),
                heap: self.heap(reference.heap_type(), start, end)?,
            }),
        })
    }

    fn heap(&self, heap: HeapType, start: u32, end: u32) -> Result<Heap, String> {
        match heap {
            HeapType::Exact(_) => Err("raw exact-reference contracts are unsupported".into()),
            HeapType::Abstract { shared, ty } => Ok(Heap::Abstract { shared, kind: ty }),
            HeapType::Concrete(index) => {
                let index = index
                    .as_module_index()
                    .ok_or("raw reference has a non-module index")?;
                if (start..end).contains(&index) {
                    return Ok(Heap::Bound(index - start));
                }
                self.definitions
                    .get(index as usize)
                    .cloned()
                    .ok_or_else(|| "raw reference crosses an unresolved recursion group".into())
            }
        }
    }

    fn definition(&self, ty: &SubType, start: u32, end: u32) -> Result<Definition, String> {
        if ty.composite_type.shared
            || ty.composite_type.descriptor_idx.is_some()
            || ty.composite_type.describes_idx.is_some()
        {
            return Err("raw shared or descriptor composite contracts are unsupported".into());
        }
        let index = |index: PackedIndex| self.heap(HeapType::Concrete(index.unpack()), start, end);
        let field = |field: &wasmparser::FieldType| {
            Ok(Field {
                mutable: field.mutable,
                storage: match field.element_type {
                    StorageType::I8 => FieldStorage::I8,
                    StorageType::I16 => FieldStorage::I16,
                    StorageType::Val(value) => {
                        FieldStorage::Value(self.convert_value(value, start, end)?)
                    }
                },
            })
        };
        let composite = match &ty.composite_type.inner {
            CompositeInnerType::Func(function) => Composite::Function {
                parameters: function
                    .params()
                    .iter()
                    .map(|ty| self.convert_value(*ty, start, end))
                    .collect::<Result<_, _>>()?,
                results: function
                    .results()
                    .iter()
                    .map(|ty| self.convert_value(*ty, start, end))
                    .collect::<Result<_, _>>()?,
            },
            CompositeInnerType::Struct(ty) => {
                Composite::Struct(ty.fields.iter().map(field).collect::<Result<_, String>>()?)
            }
            CompositeInnerType::Array(ty) => Composite::Array(field(&ty.0)?),
            CompositeInnerType::Cont(_) => {
                return Err("raw continuation contracts are unsupported".into());
            }
        };
        Ok(Definition {
            final_type: ty.is_final,
            supertype: ty.supertype_idx.map(index).transpose()?,
            composite,
        })
    }
}

#[cfg(test)]
mod tests;

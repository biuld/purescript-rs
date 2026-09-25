use crate::{Expr, Module, Type, TypeId};
use std::collections::HashSet;

/// Checked field order for a class dictionary represented by a Core record.
/// The class solver's roles are erased; backend consumers use these labels,
/// field types, and logical product indices only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassLayout {
    dictionary_type: TypeId,
    fields: Vec<ClassField>,
}

/// One named Core field and its stable zero-based product index.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassField {
    pub label: String,
    pub ty: TypeId,
    pub index: u32,
}

impl ClassLayout {
    /// Builds a dictionary layout from a verified Core record type.
    pub fn from_record_type(
        module: &Module,
        dictionary_type: TypeId,
    ) -> Result<Self, &'static str> {
        let Some(Type::Record(fields)) = module.types.get(dictionary_type.0 as usize) else {
            return Err("class dictionary type is not a Core record");
        };
        let mut labels = HashSet::with_capacity(fields.len());
        let fields = fields
            .iter()
            .enumerate()
            .map(|(index, (label, ty))| {
                if !labels.insert(label.as_str()) {
                    return Err("class dictionary record has duplicate field labels");
                }
                if module.types.get(ty.0 as usize).is_none() {
                    return Err("class dictionary field type is outside the Core type table");
                }
                let index = u32::try_from(index)
                    .map_err(|_| "class dictionary has too many product fields")?;
                Ok(ClassField {
                    label: label.clone(),
                    ty: *ty,
                    index,
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Ok(Self {
            dictionary_type,
            fields,
        })
    }

    pub fn dictionary_type(&self) -> TypeId {
        self.dictionary_type
    }

    pub fn fields(&self) -> &[ClassField] {
        &self.fields
    }

    pub fn field(&self, label: &str) -> Option<&ClassField> {
        self.fields.iter().find(|field| field.label == label)
    }

    /// Checks a Core record value against the checked labels and field types.
    pub fn validate_record_value(
        &self,
        module: &Module,
        values: &[(String, Expr)],
    ) -> Result<(), &'static str> {
        if values.len() != self.fields.len() {
            return Err("dictionary value field count differs from its class layout");
        }
        let mut seen = HashSet::with_capacity(values.len());
        for (label, value) in values {
            if !seen.insert(label.as_str()) {
                return Err("dictionary value contains a duplicate field");
            }
            let Some(field) = self.field(label) else {
                return Err("dictionary value contains a field outside its class layout");
            };
            if !types_compatible(value.ty, field.ty, module, &mut HashSet::new()) {
                return Err("dictionary value field type differs from its class layout");
            }
        }
        if self
            .fields
            .iter()
            .any(|field| !seen.contains(field.label.as_str()))
        {
            return Err("dictionary value is missing a class layout field");
        }
        Ok(())
    }
}

fn types_compatible(
    left: TypeId,
    right: TypeId,
    module: &Module,
    seen: &mut HashSet<(TypeId, TypeId)>,
) -> bool {
    if left == right || !seen.insert((left, right)) {
        return true;
    }
    let (Some(left), Some(right)) = (
        module.types.get(left.0 as usize),
        module.types.get(right.0 as usize),
    ) else {
        return false;
    };
    match (left, right) {
        (Type::Variable(_), _) | (_, Type::Variable(_)) => true,
        (Type::I32, Type::I32)
        | (Type::F64, Type::F64)
        | (Type::Boolean, Type::Boolean)
        | (Type::String, Type::String)
        | (Type::Char, Type::Char)
        | (Type::Unit, Type::Unit) => true,
        (Type::Constructor(a), Type::Constructor(b)) => a == b,
        (Type::Application(a1, a2), Type::Application(b1, b2))
        | (
            Type::Function {
                parameter: a1,
                result: a2,
            },
            Type::Function {
                parameter: b1,
                result: b2,
            },
        ) => types_compatible(*a1, *b1, module, seen) && types_compatible(*a2, *b2, module, seen),
        (Type::Record(a), Type::Record(b)) => {
            a.len() == b.len()
                && a.iter().all(|(label, ty)| {
                    b.iter()
                        .find(|(other, _)| other == label)
                        .is_some_and(|(_, other)| types_compatible(*ty, *other, module, seen))
                })
        }
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use psrs_hir::ModuleId;
    use psrs_span::TextRange;

    #[test]
    fn layout_keeps_declared_indices_and_accepts_compatible_core_field_types() {
        let module = module(vec![
            Type::I32,
            Type::I32,
            Type::Record(vec![
                ("method".into(), TypeId(0)),
                ("super".into(), TypeId(0)),
            ]),
        ]);
        let layout = ClassLayout::from_record_type(&module, TypeId(2)).unwrap();
        assert_eq!(layout.field("method").unwrap().index, 0);
        assert_eq!(layout.field("super").unwrap().index, 1);
        let values = vec![
            (
                "super".into(),
                Expr {
                    kind: crate::ExprKind::Integer(0),
                    ty: TypeId(1),
                    span: TextRange::new(0, 1),
                },
            ),
            (
                "method".into(),
                Expr {
                    kind: crate::ExprKind::Integer(1),
                    ty: TypeId(1),
                    span: TextRange::new(1, 2),
                },
            ),
        ];
        layout.validate_record_value(&module, &values).unwrap();
    }

    #[test]
    fn layout_rejects_duplicate_field_labels() {
        let module = module(vec![
            Type::I32,
            Type::Record(vec![("method".into(), TypeId(0)); 2]),
        ]);
        assert_eq!(
            ClassLayout::from_record_type(&module, TypeId(1)).unwrap_err(),
            "class dictionary record has duplicate field labels"
        );
    }

    fn module(types: Vec<Type>) -> Module {
        Module {
            id: ModuleId(0),
            name: "DictionaryLayout".into(),
            externals: Vec::new(),
            types,
            newtype_ids: Vec::new(),
            constructors: Vec::new(),
            declarations: Vec::new(),
            entry: None,
            span: TextRange::new(0, 1),
        }
    }
}

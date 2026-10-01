use crate::{Expr, Module, TypeId};
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
        let Some(fields) = module.record_fields(dictionary_type) else {
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
            if !crate::verify::equivalent_types(value.ty, field.ty, module) {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Type;
    use psrs_hir::ModuleId;
    use psrs_span::TextRange;

    #[test]
    fn layout_keeps_declared_indices_and_accepts_compatible_core_field_types() {
        let mut types = vec![
            Type::Constructor(crate::TypeConstructor::Int),
            Type::Constructor(crate::TypeConstructor::Int),
        ];
        let record = record_type(&mut types, &[("method", TypeId(0)), ("super", TypeId(0))]);
        let module = module(types);
        let layout = ClassLayout::from_record_type(&module, record).unwrap();
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
        let mut types = vec![Type::Constructor(crate::TypeConstructor::Int)];
        let record = record_type(&mut types, &[("method", TypeId(0)), ("method", TypeId(0))]);
        let module = module(types);
        assert_eq!(
            ClassLayout::from_record_type(&module, record).unwrap_err(),
            "class dictionary record has duplicate field labels"
        );
    }

    #[test]
    fn layout_rejects_a_dictionary_field_with_the_wrong_type() {
        let (module, record) = method_layout_module();
        let layout = ClassLayout::from_record_type(&module, record).unwrap();
        let values = vec![("method".into(), integer(0)), ("super".into(), integer(0))];
        assert_eq!(
            layout.validate_record_value(&module, &values).unwrap_err(),
            "dictionary value field type differs from its class layout"
        );
    }

    #[test]
    fn layout_rejects_a_dictionary_missing_a_field() {
        let (module, record) = method_layout_module();
        let layout = ClassLayout::from_record_type(&module, record).unwrap();
        let values = vec![("method".into(), integer(0))];
        assert_eq!(
            layout.validate_record_value(&module, &values).unwrap_err(),
            "dictionary value field count differs from its class layout"
        );
    }

    #[test]
    fn layout_rejects_a_non_record_dictionary_type() {
        let (module, _) = method_layout_module();
        assert_eq!(
            ClassLayout::from_record_type(&module, TypeId(0)).unwrap_err(),
            "class dictionary type is not a Core record"
        );
    }

    fn integer(value: i32) -> Expr {
        Expr {
            kind: crate::ExprKind::Integer(value),
            ty: TypeId(0),
            span: TextRange::new(0, 1),
        }
    }

    fn method_layout_module() -> (Module, TypeId) {
        let mut types = vec![
            Type::Constructor(crate::TypeConstructor::Int),
            Type::Constructor(crate::TypeConstructor::Boolean),
        ];
        let record = record_type(&mut types, &[("method", TypeId(1)), ("super", TypeId(1))]);
        (module(types), record)
    }

    /// Appends a closed record type's row nodes and returns its `TypeId`.
    fn record_type(types: &mut Vec<Type>, fields: &[(&str, TypeId)]) -> TypeId {
        let row_empty = TypeId(types.len() as u32);
        types.push(Type::RowEmpty);
        let mut tail = row_empty;
        for (label, ty) in fields.iter().rev() {
            let id = TypeId(types.len() as u32);
            types.push(Type::RowExtend {
                label: (*label).into(),
                ty: *ty,
                tail,
            });
            tail = id;
        }
        let head = TypeId(types.len() as u32);
        types.push(Type::Constructor(crate::TypeConstructor::Record));
        let id = TypeId(types.len() as u32);
        types.push(Type::Application(head, tail));
        id
    }

    fn module(types: Vec<Type>) -> Module {
        Module {
            type_names: Vec::new(),
            id: ModuleId(0),
            name: "DictionaryLayout".into(),
            externals: Vec::new(),
            types,
            newtype_ids: Vec::new(),
            opaque_ids: Vec::new(),
            callable_types: Vec::new(),
            constructors: Vec::new(),
            declarations: Vec::new(),
            entry: None,
            span: TextRange::new(0, 1),
        }
    }
}

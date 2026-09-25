use super::scalar::scalar_type;
use super::{array_element_type, depends_on_type_variable};
use crate::BackendError;
use crate::cc::{RefShape, ReprId, Representation, RepresentationTable, SignatureId, ValueShape};
use psrs_core::{Module, Type, TypeId};
use psrs_hir::TypeId as HirTypeId;
use psrs_span::TextRange;
use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum AggregateKey {
    Array(ValueShape),
    Record(Vec<(String, ValueShape)>),
}

#[derive(Clone)]
pub(super) struct AggregateLayouts {
    pub(super) arrays: HashMap<TypeId, ReprId>,
    pub(super) records: HashMap<TypeId, ReprId>,
}

/// Reserves stable handles so function signatures can refer to aggregates
/// before their canonical layouts have been filled.
pub(super) fn reserve_aggregate_layouts(
    module: &Module,
    representations: &mut RepresentationTable,
) -> AggregateLayouts {
    let mut arrays = HashMap::new();
    let mut records = HashMap::new();
    for (index, ty) in module.types.iter().enumerate() {
        let id = TypeId(index as u32);
        if array_element_type(module, id).is_some() {
            arrays.insert(id, representations.reserve());
        } else if matches!(ty, Type::Record(_)) {
            records.insert(id, representations.reserve());
        }
    }
    AggregateLayouts { arrays, records }
}

/// Fills the reserved handles with canonical array and closed-record layouts.
/// Core IDs select normalization inputs, not runtime layout identities.
#[allow(clippy::too_many_arguments)]
pub(super) fn normalize_aggregate_layouts(
    module: &Module,
    enum_types: &HashSet<HirTypeId>,
    aggregate_types: &HashSet<HirTypeId>,
    newtype_ids: &HashSet<HirTypeId>,
    function_types: &HashMap<TypeId, SignatureId>,
    layouts: AggregateLayouts,
    representations: &mut RepresentationTable,
) -> Result<AggregateLayouts, Vec<BackendError>> {
    let reserved = layouts.clone();
    let mut builder = Builder {
        module,
        enum_types,
        aggregate_types,
        newtype_ids,
        function_types,
        representations,
        arrays: layouts.arrays,
        records: layouts.records,
        canonical: HashMap::new(),
        normalized: HashSet::new(),
        active: HashSet::new(),
    };
    let mut ids = builder
        .arrays
        .keys()
        .chain(builder.records.keys())
        .copied()
        .collect::<Vec<_>>();
    ids.sort_by_key(|id| id.0);
    for id in ids {
        builder.normalize(id, module.span)?;
    }
    let layouts = AggregateLayouts {
        arrays: builder.arrays,
        records: builder.records,
    };
    let mut remapped = HashMap::new();
    for (id, reserved_repr) in reserved.arrays.iter().chain(&reserved.records) {
        let canonical = layouts
            .arrays
            .get(id)
            .or_else(|| layouts.records.get(id))
            .copied()
            .expect("every reserved aggregate has a normalized representation");
        if *reserved_repr != canonical {
            remapped.insert(*reserved_repr, canonical);
        }
    }
    for signature in &mut builder.representations.signatures {
        for parameter in &mut signature.parameters {
            remap_shape(parameter, &remapped);
        }
        remap_shape(&mut signature.result, &remapped);
    }
    Ok(layouts)
}

struct Builder<'a> {
    module: &'a Module,
    enum_types: &'a HashSet<HirTypeId>,
    aggregate_types: &'a HashSet<HirTypeId>,
    newtype_ids: &'a HashSet<HirTypeId>,
    function_types: &'a HashMap<TypeId, SignatureId>,
    representations: &'a mut RepresentationTable,
    arrays: HashMap<TypeId, ReprId>,
    records: HashMap<TypeId, ReprId>,
    canonical: HashMap<AggregateKey, ReprId>,
    normalized: HashSet<TypeId>,
    active: HashSet<TypeId>,
}

impl Builder<'_> {
    fn normalize(&mut self, id: TypeId, span: TextRange) -> Result<ReprId, Vec<BackendError>> {
        if self.normalized.contains(&id) {
            return self.handle(id, span);
        }
        let handle = self.handle(id, span)?;
        if !self.active.insert(id) {
            return Ok(handle);
        }
        let (key, representation, labels) =
            if let Some(element) = array_element_type(self.module, id) {
                let shape = self.value_shape(element, span)?;
                (
                    AggregateKey::Array(shape),
                    Representation::Array { element: shape },
                    None,
                )
            } else {
                let Some(Type::Record(fields)) = self.module.types.get(id.0 as usize) else {
                    self.active.remove(&id);
                    return Err(layout_error(
                        span,
                        "aggregate type has no array or record layout",
                    ));
                };
                let mut fields = fields
                    .iter()
                    .map(|(label, ty)| Ok((label.clone(), self.value_shape(*ty, span)?)))
                    .collect::<Result<Vec<_>, Vec<BackendError>>>()?;
                fields.sort_by(|left, right| left.0.cmp(&right.0));
                let labels = fields.iter().map(|(label, _)| label.clone()).collect();
                let shapes = fields.iter().map(|(_, shape)| *shape).collect();
                (
                    AggregateKey::Record(fields),
                    Representation::Product { fields: shapes },
                    Some(labels),
                )
            };
        let canonical = if let Some(canonical) = self.canonical.get(&key).copied() {
            canonical
        } else {
            self.representations.set(handle, representation);
            if let Some(labels) = labels {
                self.representations.set_product_labels(handle, labels);
            }
            self.canonical.insert(key, handle);
            handle
        };
        if let Some(array) = self.arrays.get_mut(&id) {
            *array = canonical;
        }
        if let Some(record) = self.records.get_mut(&id) {
            *record = canonical;
        }
        self.active.remove(&id);
        self.normalized.insert(id);
        Ok(canonical)
    }

    fn value_shape(
        &mut self,
        id: TypeId,
        span: TextRange,
    ) -> Result<ValueShape, Vec<BackendError>> {
        if self.arrays.contains_key(&id) || self.records.contains_key(&id) {
            let representation = self.normalize(id, span)?;
            return Ok(ValueShape::Reference(crate::cc::Reference {
                nullable: false,
                heap: crate::cc::RefShape::Repr(representation),
            }));
        }
        scalar_type(
            self.module,
            id,
            span,
            self.enum_types,
            self.aggregate_types,
            self.newtype_ids,
            &self.arrays,
            &self.records,
            self.function_types,
        )
    }

    fn handle(&self, id: TypeId, span: TextRange) -> Result<ReprId, Vec<BackendError>> {
        self.arrays
            .get(&id)
            .or_else(|| self.records.get(&id))
            .copied()
            .ok_or_else(|| {
                layout_error(
                    span,
                    if depends_on_type_variable(self.module, id) {
                        "dependent aggregate has no canonical representation handle"
                    } else {
                        "aggregate type has no representation handle"
                    },
                )
            })
    }
}

fn remap_shape(shape: &mut ValueShape, representations: &HashMap<ReprId, ReprId>) {
    let ValueShape::Reference(reference) = shape else {
        return;
    };
    if let RefShape::Repr(id) = reference.heap
        && let Some(canonical) = representations.get(&id)
    {
        reference.heap = RefShape::Repr(*canonical);
    }
}

fn layout_error(span: TextRange, message: &'static str) -> Vec<BackendError> {
    vec![BackendError::new("P8 closure conversion", span, message)]
}

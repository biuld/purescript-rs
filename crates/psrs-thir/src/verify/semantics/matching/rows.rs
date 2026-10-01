use super::Matcher;
use crate::{Type, TypeId, record_row};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

/// A row variable instantiated to a residual that has no single type-table node.
#[derive(Clone, Debug)]
pub(super) struct RowForm {
    fields: Vec<(String, TypeId)>,
    tail: Option<TypeId>,
}

enum TailKind {
    Closed,
    Flex(TypeVariableId),
    Rigid(TypeVariableId),
}

type RowShape = (Vec<(String, TypeId)>, Option<TypeId>);

impl Matcher<'_> {
    /// Closed rows and rigid open rows agree exactly. A flexible tail is a
    /// quantifier being instantiated and may absorb the other side's residual.
    pub(super) fn relate_records(&mut self, actual: TypeId, expected: TypeId) -> bool {
        let (Some(actual_row), Some(expected_row)) = (
            record_row(&self.module.types, actual),
            record_row(&self.module.types, expected),
        ) else {
            return false;
        };
        let (Some((actual_fields, actual_tail)), Some((expected_fields, expected_tail))) =
            (self.flatten_row(actual_row), self.flatten_row(expected_row))
        else {
            return false;
        };
        let Some(mut expected_fields) = index_fields(expected_fields) else {
            return false;
        };
        let mut actual_rest = Vec::new();
        let mut seen = HashSet::new();
        for (label, actual_ty) in actual_fields {
            if !seen.insert(label.clone()) {
                return false;
            }
            if let Some(expected_ty) = expected_fields.remove(&label) {
                if !self.subsumes(actual_ty, expected_ty) {
                    return false;
                }
            } else {
                actual_rest.push((label, actual_ty));
            }
        }
        self.finish_row(
            actual_rest,
            actual_tail,
            expected_fields.into_iter().collect(),
            expected_tail,
        )
    }

    pub(super) fn row_form_matches(&mut self, form: &RowForm, replacement: TypeId) -> bool {
        let Some((fields, tail)) = self.flatten_row(replacement) else {
            return false;
        };
        self.forms_agree(&form.fields, form.tail, &fields, tail)
    }

    fn finish_row(
        &mut self,
        actual_rest: Vec<(String, TypeId)>,
        actual_tail: Option<TypeId>,
        expected_rest: Vec<(String, TypeId)>,
        expected_tail: Option<TypeId>,
    ) -> bool {
        let (Some(actual_kind), Some(expected_kind)) =
            (self.tail_kind(actual_tail), self.tail_kind(expected_tail))
        else {
            return false;
        };
        match (
            actual_rest.is_empty(),
            actual_kind,
            expected_rest.is_empty(),
            expected_kind,
        ) {
            (true, TailKind::Flex(variable), true, TailKind::Flex(other)) if variable == other => {
                true
            }
            (true, TailKind::Flex(variable), _, _) => {
                self.assign_row(variable, expected_rest, expected_tail)
            }
            (_, _, true, TailKind::Flex(variable)) => {
                self.assign_row(variable, actual_rest, actual_tail)
            }
            (true, TailKind::Closed, true, TailKind::Closed) => true,
            (true, TailKind::Rigid(left), true, TailKind::Rigid(right)) => {
                self.alpha_match(left, right)
            }
            _ => false,
        }
    }

    fn assign_row(
        &mut self,
        variable: TypeVariableId,
        mut fields: Vec<(String, TypeId)>,
        tail: Option<TypeId>,
    ) -> bool {
        fields.sort_by(|left, right| left.0.cmp(&right.0));
        if self.row_occurs(variable, &fields, tail) {
            return false;
        }
        if let Some(existing) = self.replacements.get(&variable).copied() {
            return self.row_form_matches(&RowForm { fields, tail }, existing);
        }
        if let Some(existing) = self.row_forms.get(&variable).cloned() {
            return self.forms_agree(&existing.fields, existing.tail, &fields, tail);
        }
        if fields.is_empty()
            && tail.is_some_and(|tail| {
                matches!(
                    self.module.types.get(tail.0 as usize),
                    Some(Type::Variable(found)) if *found == variable
                )
            })
        {
            return true;
        }
        if let Some(existing) = self.lookup_row(&fields, tail) {
            return self.bind(variable, existing);
        }
        self.row_forms.insert(variable, RowForm { fields, tail });
        true
    }

    fn row_occurs(
        &self,
        variable: TypeVariableId,
        fields: &[(String, TypeId)],
        tail: Option<TypeId>,
    ) -> bool {
        let mut free = HashSet::new();
        let mut bound = HashMap::new();
        let mut active = HashSet::new();
        for (_, ty) in fields {
            super::collect_free(*ty, &self.module.types, &mut bound, &mut active, &mut free);
        }
        if let Some(tail) = tail {
            super::collect_free(tail, &self.module.types, &mut bound, &mut active, &mut free);
        }
        free.contains(&variable)
    }

    fn lookup_row(&self, fields: &[(String, TypeId)], tail: Option<TypeId>) -> Option<TypeId> {
        for (index, _) in self.module.types.iter().enumerate() {
            let id = TypeId(index as u32);
            let Some((candidate, candidate_tail)) = crate::row_fields(&self.module.types, id)
            else {
                continue;
            };
            if candidate_tail == tail && same_labels(&candidate, fields) {
                return Some(id);
            }
        }
        None
    }

    fn forms_agree(
        &mut self,
        left: &[(String, TypeId)],
        left_tail: Option<TypeId>,
        right: &[(String, TypeId)],
        right_tail: Option<TypeId>,
    ) -> bool {
        let mut left = left.to_vec();
        let mut right = right.to_vec();
        left.sort_by(|left, right| left.0.cmp(&right.0));
        right.sort_by(|left, right| left.0.cmp(&right.0));
        left.len() == right.len()
            && left
                .iter()
                .zip(&right)
                .all(|(left, right)| left.0 == right.0 && self.types_equal(left.1, right.1))
            && match (left_tail, right_tail) {
                (None, None) => true,
                (Some(left), Some(right)) => self.types_equal(left, right),
                _ => false,
            }
    }

    fn types_equal(&mut self, left: TypeId, right: TypeId) -> bool {
        self.equal(left, right, &mut HashSet::new())
    }

    fn flatten_row(&self, mut row: TypeId) -> Option<RowShape> {
        let mut fields = Vec::new();
        let mut seen_rows = HashSet::new();
        let mut seen_vars = HashSet::new();
        loop {
            if !seen_rows.insert(row) {
                return None;
            }
            match self.module.types.get(row.0 as usize)? {
                Type::RowEmpty => return Some((fields, None)),
                Type::RowExtend { label, ty, tail } => {
                    fields.push((label.clone(), *ty));
                    row = *tail;
                }
                Type::Variable(variable) => {
                    if !seen_vars.insert(*variable) {
                        return None;
                    }
                    if let Some(replacement) = self.replacements.get(variable).copied() {
                        row = replacement;
                        continue;
                    }
                    if let Some(form) = self.row_forms.get(variable) {
                        fields.extend(form.fields.iter().cloned());
                        match form.tail {
                            Some(tail) => {
                                row = tail;
                                continue;
                            }
                            None => return Some((fields, None)),
                        }
                    }
                    return Some((fields, Some(row)));
                }
                _ => return None,
            }
        }
    }

    fn tail_kind(&self, tail: Option<TypeId>) -> Option<TailKind> {
        let Some(id) = tail else {
            return Some(TailKind::Closed);
        };
        let Type::Variable(variable) = self.module.types.get(id.0 as usize)? else {
            return None;
        };
        if self.alpha.contains_key(variable) || self.alpha.values().any(|bound| bound == variable) {
            return Some(TailKind::Rigid(*variable));
        }
        if self.flexible.contains(variable) {
            Some(TailKind::Flex(*variable))
        } else {
            Some(TailKind::Rigid(*variable))
        }
    }
}

fn index_fields(fields: Vec<(String, TypeId)>) -> Option<HashMap<String, TypeId>> {
    let mut indexed = HashMap::new();
    for (label, ty) in fields {
        if indexed.insert(label, ty).is_some() {
            return None;
        }
    }
    Some(indexed)
}

fn same_labels(left: &[(String, TypeId)], right: &[(String, TypeId)]) -> bool {
    let mut left = left.to_vec();
    let mut right = right.to_vec();
    left.sort_by(|left, right| left.0.cmp(&right.0));
    right.sort_by(|left, right| left.0.cmp(&right.0));
    left.len() == right.len()
        && left
            .iter()
            .zip(&right)
            .all(|(left, right)| left.0 == right.0 && left.1 == right.1)
}

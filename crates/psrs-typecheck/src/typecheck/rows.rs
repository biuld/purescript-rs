use super::*;

impl Checker {
    /// Flattens a record by following a solved tail. Common labels are merged
    /// only when unification produced them; a duplicate label is reported.
    pub(super) fn resolve_record(&self, record: InferRecord) -> InferType {
        let mut fields = record
            .fields
            .into_iter()
            .map(|(label, ty)| (label, self.resolve_type(ty)))
            .collect::<Vec<_>>();
        fields.sort_by(|left, right| left.0.cmp(&right.0));
        match record.tail {
            RowTail::Closed => InferType::Record(InferRecord {
                fields,
                tail: RowTail::Closed,
            }),
            RowTail::Open(variable) => match self.substitutions.get(&variable) {
                Some(bound) => self.merge_tail(fields, self.resolve_type(bound.clone())),
                None => InferType::Record(InferRecord {
                    fields,
                    tail: RowTail::Open(variable),
                }),
            },
        }
    }

    fn merge_tail(&self, mut fields: Vec<(String, InferType)>, tail: InferType) -> InferType {
        match tail {
            InferType::Record(rest) => {
                for (label, ty) in rest.fields {
                    if fields.iter().any(|(existing, _)| existing == &label) {
                        // The caller reports the diagnostic. Keep the first
                        // type so later unification still has a row to compare.
                        continue;
                    }
                    fields.push((label, ty));
                }
                fields.sort_by(|left, right| left.0.cmp(&right.0));
                match rest.tail {
                    RowTail::Closed => InferType::Record(InferRecord {
                        fields,
                        tail: RowTail::Closed,
                    }),
                    RowTail::Open(variable) => {
                        if let Some(bound) = self.substitutions.get(&variable) {
                            self.merge_tail(fields, self.resolve_type(bound.clone()))
                        } else {
                            InferType::Record(InferRecord {
                                fields,
                                tail: RowTail::Open(variable),
                            })
                        }
                    }
                }
            }
            InferType::Variable(variable) => InferType::Record(InferRecord {
                fields,
                tail: RowTail::Open(variable),
            }),
            _ => InferType::Record(InferRecord {
                fields,
                tail: RowTail::Closed,
            }),
        }
    }

    pub(super) fn unify_rows(&mut self, left: InferRecord, right: InferRecord, span: TextRange) {
        let mut left_rest = Vec::new();
        let mut right_rest = Vec::new();
        let mut left_fields = left.fields.into_iter().peekable();
        let mut right_fields = right.fields.into_iter().peekable();
        loop {
            match (left_fields.peek(), right_fields.peek()) {
                (Some((left_label, _)), Some((right_label, _))) => {
                    match left_label.cmp(right_label) {
                        std::cmp::Ordering::Equal => {
                            let (_, left_ty) = left_fields.next().unwrap();
                            let (_, right_ty) = right_fields.next().unwrap();
                            self.unify(left_ty, right_ty, span);
                        }
                        std::cmp::Ordering::Less => {
                            left_rest.push(left_fields.next().unwrap());
                        }
                        std::cmp::Ordering::Greater => {
                            right_rest.push(right_fields.next().unwrap());
                        }
                    }
                }
                (Some(_), None) => left_rest.push(left_fields.next().unwrap()),
                (None, Some(_)) => right_rest.push(right_fields.next().unwrap()),
                (None, None) => break,
            }
        }
        self.unify_row_tails(left_rest, left.tail, right_rest, right.tail, span);
    }

    fn unify_row_tails(
        &mut self,
        left_rest: Vec<(String, InferType)>,
        left_tail: RowTail,
        right_rest: Vec<(String, InferType)>,
        right_tail: RowTail,
        span: TextRange,
    ) {
        match (
            left_rest.is_empty(),
            left_tail,
            right_rest.is_empty(),
            right_tail,
        ) {
            (true, RowTail::Open(variable), _, tail) if !self.rigid.contains(&variable) => {
                if right_rest.is_empty()
                    && matches!(tail, RowTail::Open(other) if other == variable)
                {
                    return;
                }
                self.bind_row(variable, right_rest, tail, span);
            }
            (_, tail, true, RowTail::Open(variable)) if !self.rigid.contains(&variable) => {
                self.bind_row(variable, left_rest, tail, span);
            }
            (true, RowTail::Closed, true, RowTail::Closed) => {}
            (true, RowTail::Open(left), true, RowTail::Open(right)) if left == right => {}
            (false, RowTail::Open(left), false, RowTail::Open(right))
                if left != right && !self.rigid.contains(&left) && !self.rigid.contains(&right) =>
            {
                if self.row_occurs(right, &left_rest, span)
                    || self.row_occurs(left, &right_rest, span)
                {
                    return;
                }
                let fresh = self.fresh_row();
                self.bind_row(left, right_rest, RowTail::Open(fresh), span);
                self.bind_row(right, left_rest, RowTail::Open(fresh), span);
            }
            _ => self.row_mismatch(left_rest, left_tail, right_rest, right_tail, span),
        }
    }

    fn bind_row(
        &mut self,
        variable: u32,
        fields: Vec<(String, InferType)>,
        tail: RowTail,
        span: TextRange,
    ) {
        if self.rigid.contains(&variable) {
            self.row_mismatch(Vec::new(), RowTail::Open(variable), fields, tail, span);
            return;
        }
        self.bind_variable(
            variable,
            InferType::Record(InferRecord { fields, tail }),
            span,
        );
    }

    fn fresh_row(&mut self) -> u32 {
        match self.fresh() {
            InferType::Variable(variable) => variable,
            _ => unreachable!("fresh inference types are variables"),
        }
    }

    fn row_occurs(
        &mut self,
        variable: u32,
        fields: &[(String, InferType)],
        span: TextRange,
    ) -> bool {
        if fields.iter().any(|(_, ty)| occurs(variable, ty)) {
            let displayed = fields
                .iter()
                .map(|(label, ty)| format!("{label}: {}", self.display_type(ty)))
                .collect::<Vec<_>>()
                .join(", ");
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::OccursCheck,
                span,
                format!("infinite type: _T{variable} occurs in {{{displayed}}}"),
            ));
            true
        } else {
            false
        }
    }

    fn row_mismatch(
        &mut self,
        left_rest: Vec<(String, InferType)>,
        left_tail: RowTail,
        right_rest: Vec<(String, InferType)>,
        right_tail: RowTail,
        span: TextRange,
    ) {
        let missing = if tail_is_fixed(right_tail, &self.rigid) && !left_rest.is_empty() {
            left_rest.iter().map(|(label, _)| label.clone()).collect()
        } else if tail_is_fixed(left_tail, &self.rigid) && !right_rest.is_empty() {
            right_rest.iter().map(|(label, _)| label.clone()).collect()
        } else {
            Vec::new()
        };
        if missing.is_empty() {
            let expected = self.display_type(&InferType::Record(InferRecord {
                fields: left_rest,
                tail: left_tail,
            }));
            let actual = self.display_type(&InferType::Record(InferRecord {
                fields: right_rest,
                tail: right_tail,
            }));
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                format!("type mismatch: expected {expected}, found {actual}"),
            ));
            return;
        }
        for label in missing {
            self.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                format!("record has no field `{label}`"),
            ));
        }
    }

    pub(super) fn finalize_record(
        &mut self,
        record: InferRecord,
        span: TextRange,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<TypeId> {
        let InferType::Record(record) = self.resolve_record(record) else {
            return None;
        };
        let fields = record
            .fields
            .into_iter()
            .map(|(label, field)| {
                Some((label, self.finalize_type(&field, span, interner, generics)?))
            })
            .collect::<Option<Vec<_>>>()?;
        match record.tail {
            RowTail::Closed => Some(interner.intern(Type::Record(fields))),
            RowTail::Open(variable) if generics.contains(&variable) => {
                let tail = interner.intern(Type::Variable(TypeVariableId(variable)));
                Some(interner.intern(Type::OpenRecord { fields, tail }))
            }
            RowTail::Open(variable) => {
                self.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnconstrainedType,
                    span,
                    format!("cannot infer a monomorphic type for _T{variable}"),
                ));
                None
            }
        }
    }
}

fn tail_is_fixed(tail: RowTail, rigid: &HashSet<u32>) -> bool {
    match tail {
        RowTail::Closed => true,
        RowTail::Open(variable) => rigid.contains(&variable),
    }
}

use super::*;

/// Normalizes a row into its fields and its tail by following solved row
/// variables, or reports the shape it reached that is not a row. `span` is the
/// range an invalid shape is reported at: `InferType` carries no ranges, so it
/// is the range of the operation that reached the shape.
///
/// A shape it does not understand is never converted into a closed row: that
/// loses the entries already collected and leaves the caller unable to describe
/// the mistake. Every consumer — row unification, record literals, field
/// access, record patterns, and finalization — goes through this one function,
/// so two of them cannot disagree about a row's tail.
impl Checker {
    pub(super) fn normalize_row(
        &self,
        row: InferType,
        span: TextRange,
    ) -> Result<FlatRow, RowShapeError> {
        let mut fields = Vec::new();
        let mut current = self.resolve_type(row);
        loop {
            match current {
                InferType::RowEmpty => {
                    return Ok(FlatRow {
                        fields,
                        tail: RowTail::Closed,
                    });
                }
                InferType::RowExtend { label, ty, tail } => {
                    fields.push((label, *ty));
                    current = self.resolve_type(*tail);
                }
                InferType::Variable(variable) => {
                    return Ok(FlatRow {
                        fields,
                        tail: RowTail::Open(variable),
                    });
                }
                // A row that resolved to a non-row value is a kind error, not a
                // closed row.
                found => {
                    return Err(RowShapeError {
                        span,
                        fields,
                        found,
                    });
                }
            }
        }
    }

    /// Normalizes a row, reporting an invalid shape as a diagnostic and using
    /// an empty row when the caller cannot proceed without one.
    pub(super) fn normalize_row_or_report(&mut self, row: InferType, span: TextRange) -> FlatRow {
        match self.normalize_row(row, span) {
            Ok(row) => row,
            Err(error) => {
                self.report_row_shape(&error);
                FlatRow {
                    fields: error.fields,
                    tail: RowTail::Closed,
                }
            }
        }
    }

    /// Reports a value that a row consumer reached but that is not a row. The
    /// diagnostic names the shape and the entries collected before it, so the
    /// caller can still see what the row had.
    pub(super) fn report_row_shape(&mut self, error: &RowShapeError) {
        let found = self.display_type(&error.found);
        let message = if error.fields.is_empty() {
            format!("type mismatch: expected a row type, found {found}")
        } else {
            let fields = self.display_row_fields(&error.fields);
            format!("type mismatch: expected a row type, found {found} after {fields}")
        };
        self.state.errors.push(TypeCheckError::new(
            TypeCheckErrorKind::TypeMismatch,
            error.span,
            message,
        ));
    }

    pub(super) fn unify_rows(&mut self, left: InferType, right: InferType, span: TextRange) {
        let left = match self.normalize_row(left, span) {
            Ok(row) => row,
            Err(error) => {
                self.report_row_shape(&error);
                return;
            }
        };
        let right = match self.normalize_row(right, span) {
            Ok(row) => row,
            Err(error) => {
                self.report_row_shape(&error);
                return;
            }
        };
        self.unify_flat_rows(left, right, span);
    }

    fn unify_flat_rows(&mut self, left: FlatRow, right: FlatRow, span: TextRange) {
        let FlatRow {
            fields: mut left_fields,
            tail: left_tail,
        } = left;
        let FlatRow {
            fields: mut right_fields,
            tail: right_tail,
        } = right;
        left_fields.sort_by(|left, right| left.0.cmp(&right.0));
        right_fields.sort_by(|left, right| left.0.cmp(&right.0));
        let mut left_rest = Vec::new();
        let mut right_rest = Vec::new();
        let mut left_fields = left_fields.into_iter().peekable();
        let mut right_fields = right_fields.into_iter().peekable();
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
        self.unify_row_tails(left_rest, left_tail, right_rest, right_tail, span);
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
            (true, RowTail::Open(variable), _, tail) if !self.state.rigid.contains(&variable) => {
                if right_rest.is_empty() && tail == RowTail::Open(variable) {
                    return;
                }
                self.bind_row(variable, right_rest, tail, span);
            }
            (_, tail, true, RowTail::Open(variable)) if !self.state.rigid.contains(&variable) => {
                self.bind_row(variable, left_rest, tail, span);
            }
            (true, RowTail::Closed, true, RowTail::Closed) => {}
            (true, RowTail::Open(left), true, RowTail::Open(right)) if left == right => {}
            (false, RowTail::Open(left), false, RowTail::Open(right))
                if left != right
                    && !self.state.rigid.contains(&left)
                    && !self.state.rigid.contains(&right) =>
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

    /// Solves one open row tail. The binding goes through
    /// [`Self::bind_type_variable`], so the tail variable keeps the kind its row
    /// admits: a row of what the entries hold, not the `Type` a plain value has.
    /// A tail that would acquire another kind is a kind diagnostic rather than a
    /// silently accepted row.
    fn bind_row(
        &mut self,
        variable: u32,
        fields: Vec<(String, InferType)>,
        tail: RowTail,
        span: TextRange,
    ) {
        if self.state.rigid.contains(&variable) {
            self.row_mismatch(Vec::new(), RowTail::Open(variable), fields, tail, span);
            return;
        }
        self.bind_type_variable(variable, row_from_fields(fields, tail.to_type()), span);
    }

    pub(super) fn fresh_row(&mut self) -> u32 {
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
            let displayed = self.display_row_fields(fields);
            self.state.errors.push(TypeCheckError::new(
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
        let missing = if tail_is_fixed(right_tail, &self.state.rigid) && !left_rest.is_empty() {
            left_rest.iter().map(|(label, _)| label.clone()).collect()
        } else if tail_is_fixed(left_tail, &self.state.rigid) && !right_rest.is_empty() {
            right_rest.iter().map(|(label, _)| label.clone()).collect()
        } else {
            Vec::new()
        };
        if missing.is_empty() {
            let expected = self.display_type(&record_type(left_rest, left_tail.to_type()));
            let actual = self.display_type(&record_type(right_rest, right_tail.to_type()));
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                format!("type mismatch: expected {expected}, found {actual}"),
            ));
            return;
        }
        for label in missing {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::TypeMismatch,
                span,
                format!("record has no field `{label}`"),
            ));
        }
    }

    /// Finalizes a row into THIR row nodes, in canonical field order. An open
    /// row is accepted only when its tail variable is generalized at this
    /// binding site, and a value that is not a row is a diagnostic rather than
    /// a closed row.
    pub(super) fn finalize_row(
        &mut self,
        row: InferType,
        span: TextRange,
        interner: &mut TypeInterner,
        generics: &HashSet<u32>,
    ) -> Option<TypeId> {
        let mut flat = match self.normalize_row(row, span) {
            Ok(flat) => flat,
            Err(error) => {
                self.report_row_shape(&error);
                return None;
            }
        };
        flat.fields.sort_by(|left, right| left.0.cmp(&right.0));
        let FlatRow { fields, tail } = flat;
        let mut current = match tail {
            RowTail::Closed => interner.intern(Type::RowEmpty),
            RowTail::Open(variable) if generics.contains(&variable) => {
                interner.intern(Type::Variable(TypeVariableId(variable)))
            }
            RowTail::Open(variable) => {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnconstrainedType,
                    span,
                    format!("cannot infer a monomorphic type for _T{variable}"),
                ));
                return None;
            }
        };
        for (label, field) in fields.into_iter().rev() {
            let ty = self.finalize_type(&field, span, interner, generics)?;
            current = interner.intern(Type::RowExtend {
                label,
                ty,
                tail: current,
            });
        }
        Some(current)
    }
}

/// A value a row consumer reached that is not a row. `InferType` carries no
/// source ranges, so `span` is the range of the operation that found it.
#[derive(Clone, Debug)]
pub(super) struct RowShapeError {
    pub(super) span: TextRange,
    pub(super) fields: Vec<(String, InferType)>,
    pub(super) found: InferType,
}

fn tail_is_fixed(tail: RowTail, rigid: &HashSet<u32>) -> bool {
    match tail {
        RowTail::Closed => true,
        RowTail::Open(variable) => rigid.contains(&variable),
    }
}

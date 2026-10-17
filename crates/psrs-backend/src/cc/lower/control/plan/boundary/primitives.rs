//! Check ordered CC operations without invoking the machine emitter.
use super::*;
use crate::cc::ControlHelperRole;

impl Checker<'_> {
    pub(super) fn tag(
        &self,
        source: ValueId,
        repr: ReprId,
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let a = self.next(body)?;
        let A::VariantTag {
            representation,
            value,
            ..
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if *representation != repr {
            return Err(MISMATCH);
        }
        self.equal(*value, source)?;
        Ok(a.destination)
    }

    pub(super) fn project(
        &self,
        source: ValueId,
        repr: ReprId,
        tag: u32,
        field: u32,
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let a = self.next(body)?;
        let A::VariantGet {
            representation,
            case,
            field: index,
            value,
            ..
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if (*representation, *case, *index) != (repr, tag, field) {
            return Err(MISMATCH);
        }
        self.equal(*value, source)?;
        Ok(a.destination)
    }

    pub(super) fn node(
        &self,
        repr: ReprId,
        tag: u32,
        args: &[ValueId],
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let a = self.next(body)?;
        let A::VariantNew {
            representation,
            case,
            fields,
            ..
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if (*representation, *case) != (repr, tag) {
            return Err(MISMATCH);
        }
        self.values(fields, args)?;
        Ok(a.destination)
    }

    pub(super) fn pair_conversion(
        &self,
        source: ValueId,
        entering: bool,
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let assignment = self.next(body)?;
        let A::AggregateConvert {
            value, conversion, ..
        } = &assignment.kind
        else {
            return Err(MISMATCH);
        };
        let expected =
            super::super::super::pair::conversion(self.table, self.schema.result_pair, entering);
        if conversion != &expected {
            return Err(MISMATCH);
        }
        self.equal(*value, source)?;
        Ok(assignment.destination)
    }

    pub(super) fn product(
        &self,
        repr: ReprId,
        args: &[ValueId],
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let a = self.next(body)?;
        let A::ProductNew {
            representation,
            arguments,
            ..
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if *representation != repr {
            return Err(MISMATCH);
        }
        self.values(arguments, args)?;
        Ok(a.destination)
    }

    pub(super) fn product_field(
        &self,
        source: ValueId,
        repr: ReprId,
        index: u32,
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let a = self.next(body)?;
        let A::ProductGet {
            representation,
            field,
            value,
            ..
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if (*representation, *field) != (repr, index) {
            return Err(MISMATCH);
        }
        self.equal(*value, source)?;
        Ok(a.destination)
    }

    pub(super) fn call(
        &self,
        role: ControlHelperRole,
        args: &[ValueId],
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let helper = self.table.control_helper(role).ok_or(MISMATCH)?;
        let a = self.next(body)?;
        let A::DirectCall {
            function,
            arguments,
            contract,
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if *function != helper.function()
            || contract.and_then(|id| self.table.control_helpers().get(id.0 as usize))
                != Some(helper)
        {
            return Err(MISMATCH);
        }
        self.values(arguments, args)?;
        Ok(a.destination)
    }

    pub(super) fn constant(&self, expected: i32, body: &mut &[Assignment]) -> Checked<ValueId> {
        let a = self.next(body)?;
        if !matches!(a.kind, A::Constant(n) if n == expected) {
            return Err(MISMATCH);
        }
        Ok(a.destination)
    }

    pub(super) fn new_cell(
        &self,
        repr: ReprId,
        value: ValueId,
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let a = self.next(body)?;
        let A::ArrayNew {
            representation,
            elements,
            ..
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if *representation != repr {
            return Err(MISMATCH);
        }
        self.values(elements, &[value])?;
        Ok(a.destination)
    }

    pub(super) fn read(
        &self,
        repr: ReprId,
        cell: ValueId,
        body: &mut &[Assignment],
    ) -> Checked<ValueId> {
        let zero = self.constant(0, body)?;
        let a = self.next(body)?;
        let A::ArrayGet {
            representation,
            value,
            index,
            ..
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if *representation != repr {
            return Err(MISMATCH);
        }
        self.equal(*value, cell)?;
        self.equal(*index, zero)?;
        Ok(a.destination)
    }

    pub(super) fn write(
        &self,
        repr: ReprId,
        cell: ValueId,
        expected: ValueId,
        body: &mut &[Assignment],
    ) -> Checked<()> {
        let zero = self.constant(0, body)?;
        let a = self.next(body)?;
        let A::ArraySet {
            representation,
            value,
            index,
            new_value,
            ..
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        if *representation != repr {
            return Err(MISMATCH);
        }
        self.equal(*value, cell)?;
        self.equal(*index, zero)?;
        self.equal(*new_value, expected)
    }

    pub(super) fn new_status(&self, body: &mut &[Assignment]) -> Checked<ValueId> {
        let zero = self.constant(0, body)?;
        self.new_cell(self.schema.status, zero, body)
    }

    pub(super) fn guard(&self, cell: ValueId, body: &mut &[Assignment]) -> Checked<()> {
        let status = self.read(self.schema.status, cell, body)?;
        let a = self.next(body)?;
        let A::TagSwitch {
            value,
            cases,
            default_assignments,
            default_value,
        } = &a.kind
        else {
            return Err(MISMATCH);
        };
        self.equal(*value, status)?;
        if cases.len() != 1 || cases[0].tag != 0 {
            return Err(MISMATCH);
        }
        self.end(&mut cases[0].assignments.as_slice())?;
        self.equal(cases[0].value, cell)?;
        self.reject(default_assignments, *default_value)
    }
}

use super::super::layout::scalar_type;
use super::super::lower::FunctionLowerer;
use super::super::{Assignment, AssignmentKind, ValueId, ValueType};
use super::case_error;
use crate::BackendError;
use psrs_core::{CaseBranch, PatternKind, Type};
use psrs_span::TextRange;

impl FunctionLowerer<'_> {
    pub(super) fn lower_record_case(
        &mut self,
        scrutinee_type: psrs_core::TypeId,
        scrutinee: ValueId,
        branches: &[CaseBranch],
        _result_type: ValueType,
        span: TextRange,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let Some(branch) = branches.first() else {
            return Err(case_error(span, "record case has no alternatives"));
        };
        match &branch.pattern.kind {
            PatternKind::Record { .. } => {
                self.lower_record_branch(branch, scrutinee, scrutinee_type, assignments)
            }
            PatternKind::Wildcard | PatternKind::Var { .. } => {
                self.lower_branch(branch, scrutinee, assignments)
            }
            _ => Err(case_error(
                branch.pattern.span,
                "record case requires a record, wildcard, or variable pattern",
            )),
        }
    }

    fn lower_record_branch(
        &mut self,
        branch: &CaseBranch,
        scrutinee: ValueId,
        scrutinee_type: psrs_core::TypeId,
        assignments: &mut Vec<Assignment>,
    ) -> Result<ValueId, Vec<BackendError>> {
        let PatternKind::Record { fields } = &branch.pattern.kind else {
            unreachable!("record branch lowering received another pattern");
        };
        let Some(type_index) = self.record_types.get(&scrutinee_type).copied() else {
            return Err(case_error(
                branch.pattern.span,
                "record pattern has no concrete GC struct layout",
            ));
        };
        let Some(Type::Record(record_fields)) = self.module.types.get(scrutinee_type.0 as usize)
        else {
            return Err(case_error(
                branch.pattern.span,
                "record pattern does not match a record type",
            ));
        };
        let mut bound = Vec::new();
        for (label, pattern) in fields {
            let Some((field_index, (_, field_type))) = record_fields
                .iter()
                .enumerate()
                .find(|(_, (field_label, _))| field_label == label)
            else {
                return Err(case_error(
                    pattern.span,
                    "record pattern field is not present in its type",
                ));
            };
            let PatternKind::Var { id, .. } = &pattern.kind else {
                if matches!(&pattern.kind, PatternKind::Wildcard) {
                    continue;
                }
                return Err(case_error(
                    pattern.span,
                    "nested patterns in record patterns are not supported yet",
                ));
            };
            let value_type = scalar_type(
                self.module,
                *field_type,
                pattern.span,
                self.enum_types,
                self.aggregate_types,
                self.newtype_ids,
                self.array_types,
                self.record_types,
                self.function_types,
            )?;
            let value = self.fresh(value_type);
            assignments.push(Assignment {
                destination: value,
                kind: AssignmentKind::StructGet {
                    destination: value,
                    type_index,
                    field: field_index as u32,
                    value: scrutinee,
                },
                span: pattern.span,
            });
            self.locals.insert(*id, value);
            bound.push(*id);
        }
        let value = self.lower_value(&branch.value, assignments)?;
        for id in bound {
            self.locals.remove(&id);
        }
        Ok(value)
    }
}

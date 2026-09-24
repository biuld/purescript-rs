use crate::mir::{NumericOp, ValueId, ValueType};
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct IntegerRange {
    pub minimum: i64,
    pub maximum: i64,
}

impl IntegerRange {
    pub fn for_type(ty: ValueType) -> Option<Self> {
        match ty {
            ValueType::Boolean => Some(Self {
                minimum: 0,
                maximum: 1,
            }),
            ValueType::I32 => Some(Self {
                minimum: i32::MIN as i64,
                maximum: i32::MAX as i64,
            }),
            _ => None,
        }
    }

    pub fn exact(value: i32) -> Self {
        Self {
            minimum: value as i64,
            maximum: value as i64,
        }
    }

    pub fn exact_value(self) -> Option<i32> {
        (self.minimum == self.maximum)
            .then(|| i32::try_from(self.minimum).ok())
            .flatten()
    }

    fn with_signed_bounds(minimum: i64, maximum: i64) -> Self {
        if minimum < i32::MIN as i64 || maximum > i32::MAX as i64 || minimum > maximum {
            Self::for_type(ValueType::I32).expect("i32 has a range")
        } else {
            Self { minimum, maximum }
        }
    }
}

pub(super) fn arithmetic_range(
    operation: NumericOp,
    left: IntegerRange,
    right: IntegerRange,
) -> Option<IntegerRange> {
    let range = match operation {
        NumericOp::I32Add => IntegerRange::with_signed_bounds(
            left.minimum + right.minimum,
            left.maximum + right.maximum,
        ),
        NumericOp::I32Sub => IntegerRange::with_signed_bounds(
            left.minimum - right.maximum,
            left.maximum - right.minimum,
        ),
        NumericOp::I32Mul => {
            let products = [
                left.minimum * right.minimum,
                left.minimum * right.maximum,
                left.maximum * right.minimum,
                left.maximum * right.maximum,
            ];
            IntegerRange::with_signed_bounds(
                *products.iter().min().expect("four products"),
                *products.iter().max().expect("four products"),
            )
        }
        NumericOp::BoolAnd => bool_range(left, right, true),
        NumericOp::BoolOr => bool_range(left, right, false),
        _ => return None,
    };
    Some(range)
}

pub(super) fn comparison_range(
    operation: NumericOp,
    left: IntegerRange,
    right: IntegerRange,
) -> Option<IntegerRange> {
    let result = match operation {
        NumericOp::I32Eq | NumericOp::BoolEq => {
            if left.exact_value().is_some() && left == right {
                Some(true)
            } else if left.maximum < right.minimum || right.maximum < left.minimum {
                Some(false)
            } else {
                None
            }
        }
        NumericOp::I32Ne | NumericOp::BoolNe => {
            if left.exact_value().is_some() && left == right {
                Some(false)
            } else if left.maximum < right.minimum || right.maximum < left.minimum {
                Some(true)
            } else {
                None
            }
        }
        NumericOp::I32LtS => {
            if left.maximum < right.minimum {
                Some(true)
            } else if left.minimum >= right.maximum {
                Some(false)
            } else {
                None
            }
        }
        NumericOp::I32LeS => {
            if left.maximum <= right.minimum {
                Some(true)
            } else if left.minimum > right.maximum {
                Some(false)
            } else {
                None
            }
        }
        NumericOp::I32GtS => {
            if left.minimum > right.maximum {
                Some(true)
            } else if left.maximum <= right.minimum {
                Some(false)
            } else {
                None
            }
        }
        NumericOp::I32GeS => {
            if left.minimum >= right.maximum {
                Some(true)
            } else if left.maximum < right.minimum {
                Some(false)
            } else {
                None
            }
        }
        _ => return None,
    };
    Some(result.map_or(
        IntegerRange {
            minimum: 0,
            maximum: 1,
        },
        |value| IntegerRange::exact(i32::from(value)),
    ))
}

/// Refines the operands along the path where a comparison used by `TrapIf`
/// evaluated to false.
pub(super) fn refine_comparison_false(
    operation: NumericOp,
    left: ValueId,
    right: ValueId,
    ranges: &mut HashMap<ValueId, IntegerRange>,
) {
    let (Some(mut left_range), Some(mut right_range)) =
        (ranges.get(&left).copied(), ranges.get(&right).copied())
    else {
        return;
    };
    let left_exact = left_range.exact_value();
    let right_exact = right_range.exact_value();
    match operation {
        NumericOp::I32LtS => {
            left_range.minimum = left_range.minimum.max(right_range.minimum);
            right_range.maximum = right_range.maximum.min(left_range.maximum);
        }
        NumericOp::I32LeS => {
            left_range.minimum = left_range.minimum.max(right_range.minimum + 1);
            right_range.maximum = right_range.maximum.min(left_range.maximum - 1);
        }
        NumericOp::I32GtS => {
            left_range.maximum = left_range.maximum.min(right_range.maximum);
            right_range.minimum = right_range.minimum.max(left_range.minimum);
        }
        NumericOp::I32GeS => {
            left_range.maximum = left_range.maximum.min(right_range.maximum - 1);
            right_range.minimum = right_range.minimum.max(left_range.minimum + 1);
        }
        NumericOp::I32Eq | NumericOp::BoolEq => {
            if let Some(value) = left_exact {
                narrow_not_equal(&mut right_range, value);
            }
            if let Some(value) = right_exact {
                narrow_not_equal(&mut left_range, value);
            }
        }
        NumericOp::I32Ne | NumericOp::BoolNe => {
            let minimum = left_range.minimum.max(right_range.minimum);
            let maximum = left_range.maximum.min(right_range.maximum);
            left_range.minimum = minimum;
            left_range.maximum = maximum;
            right_range = left_range;
        }
        _ => return,
    }
    ranges.insert(left, left_range);
    ranges.insert(right, right_range);
}

pub(super) fn refine_comparison_true(
    operation: NumericOp,
    left: ValueId,
    right: ValueId,
    ranges: &mut HashMap<ValueId, IntegerRange>,
) {
    let (Some(mut left_range), Some(mut right_range)) =
        (ranges.get(&left).copied(), ranges.get(&right).copied())
    else {
        return;
    };
    let left_exact = left_range.exact_value();
    let right_exact = right_range.exact_value();
    match operation {
        NumericOp::I32LtS => {
            left_range.maximum = left_range.maximum.min(right_range.maximum - 1);
            right_range.minimum = right_range.minimum.max(left_range.minimum + 1);
        }
        NumericOp::I32LeS => {
            left_range.maximum = left_range.maximum.min(right_range.maximum);
            right_range.minimum = right_range.minimum.max(left_range.minimum);
        }
        NumericOp::I32GtS => {
            left_range.minimum = left_range.minimum.max(right_range.minimum + 1);
            right_range.maximum = right_range.maximum.min(left_range.maximum - 1);
        }
        NumericOp::I32GeS => {
            left_range.minimum = left_range.minimum.max(right_range.minimum);
            right_range.maximum = right_range.maximum.min(left_range.maximum);
        }
        NumericOp::I32Eq | NumericOp::BoolEq => {
            let minimum = left_range.minimum.max(right_range.minimum);
            let maximum = left_range.maximum.min(right_range.maximum);
            left_range.minimum = minimum;
            left_range.maximum = maximum;
            right_range = left_range;
        }
        NumericOp::I32Ne | NumericOp::BoolNe => {
            if let Some(value) = left_exact {
                narrow_not_equal(&mut right_range, value);
            }
            if let Some(value) = right_exact {
                narrow_not_equal(&mut left_range, value);
            }
        }
        _ => return,
    }
    ranges.insert(left, left_range);
    ranges.insert(right, right_range);
}

fn narrow_not_equal(range: &mut IntegerRange, value: i32) {
    let value = value as i64;
    if range.minimum == value {
        range.minimum += 1;
    } else if range.maximum == value {
        range.maximum -= 1;
    }
}

fn bool_range(left: IntegerRange, right: IntegerRange, conjunction: bool) -> IntegerRange {
    let left = left.exact_value();
    let right = right.exact_value();
    let result = if let (Some(left), Some(right)) = (left, right) {
        Some(if conjunction {
            left != 0 && right != 0
        } else {
            left != 0 || right != 0
        })
    } else if conjunction && (left == Some(0) || right == Some(0)) {
        Some(false)
    } else if !conjunction && (left == Some(1) || right == Some(1)) {
        Some(true)
    } else {
        None
    };
    result.map_or(
        IntegerRange {
            minimum: 0,
            maximum: 1,
        },
        |value| IntegerRange::exact(i32::from(value)),
    )
}

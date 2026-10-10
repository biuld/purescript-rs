//! Executable numeric runtime: formatting, parsing, and scalar math.

mod atan2;
mod decimal;
mod format;
mod math;

pub use atan2::number_atan2;
pub use decimal::number_from_decimal;
pub use format::number_to_string;
pub use math::{
    number_acos, number_asin, number_atan, number_cos, number_exp, number_infinity, number_is_nan,
    number_log, number_max, number_min, number_nan, number_pow, number_remainder, number_sign,
    number_sin, number_tan,
};

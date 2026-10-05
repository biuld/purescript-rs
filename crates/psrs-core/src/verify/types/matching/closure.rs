use super::{TypeMatcher, Variance};

impl TypeMatcher<'_> {
    /// Both types are closures with a fixed parameter list. Parameters are
    /// invariant and the result follows the surrounding subsumption.
    pub(super) fn closure_subsumption(
        &mut self,
        actual: crate::TypeId,
        expected: crate::TypeId,
        instantiate: bool,
    ) -> Option<bool> {
        let (actual_parameters, actual_result) = crate::closure_parts(&self.module.types, actual)?;
        let (expected_parameters, expected_result) =
            crate::closure_parts(&self.module.types, expected)?;
        let actual_parameters = actual_parameters.to_vec();
        let expected_parameters = expected_parameters.to_vec();
        if actual_parameters.len() != expected_parameters.len() {
            return Some(false);
        }
        let parameters_match = actual_parameters.into_iter().zip(expected_parameters).all(
            |(actual_parameter, expected_parameter)| {
                self.relate(
                    expected_parameter,
                    actual_parameter,
                    Variance::Subsumption,
                    true,
                ) && self.relate(
                    actual_parameter,
                    expected_parameter,
                    Variance::Subsumption,
                    true,
                )
            },
        );
        Some(
            parameters_match
                && self.relate(
                    actual_result,
                    expected_result,
                    Variance::Subsumption,
                    instantiate,
                ),
        )
    }
}

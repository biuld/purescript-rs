//! Plain pass and artifact records for one backend compile attempt.

/// Version of the backend-owned trace vocabulary.
pub const COMPILE_TRACE_VERSION: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceArtifactId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TraceExecutionId(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceRepresentation {
    Core,
    ExternalBindings,
    ClosureConverted,
    Mir,
    WasmModule,
    WasmCoreBinary,
    ComponentBinary,
    WatText,
    WitWorld,
    WasiRegistry,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceArtifactState {
    Provided,
    Produced,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TracePassStatus {
    Completed,
    Rejected,
    NotApplicable,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceEdgeRole {
    Input,
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceValidationStatus {
    Passed,
    Rejected,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceValidationCoverage {
    Direct,
    Composite,
    NotObserved,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceArtifact {
    pub id: TraceArtifactId,
    pub representation: TraceRepresentation,
    pub state: TraceArtifactState,
    pub producer: Option<TraceExecutionId>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceExecution {
    pub id: TraceExecutionId,
    pub pass_key: &'static str,
    pub contract_version: u32,
    pub inputs: Vec<TraceArtifactId>,
    pub outputs: Vec<TraceArtifactId>,
    pub status: TracePassStatus,
    /// Indexes into the backend error list returned from this same compile.
    pub diagnostic_indices: Vec<usize>,
    /// Nested verifiers and transformations not exposed by this call remain
    /// opaque; the CLI serializes this observed boundary explicitly.
    pub validation_coverage: TraceValidationCoverage,
    pub parameters: Vec<TraceParameter>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceParameter {
    pub key: &'static str,
    pub value: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceEdge {
    pub execution: TraceExecutionId,
    pub artifact: TraceArtifactId,
    pub role: TraceEdgeRole,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceValidation {
    pub execution: TraceExecutionId,
    pub validator_key: &'static str,
    pub artifacts: Vec<TraceArtifactId>,
    pub status: TraceValidationStatus,
    pub coverage: TraceValidationCoverage,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TraceArtifactSelector {
    Input(usize),
    Output(usize),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TraceValidationSpec {
    pub validator_key: &'static str,
    pub artifacts: Vec<TraceArtifactSelector>,
    pub coverage: TraceValidationCoverage,
}

impl TraceValidationSpec {
    pub(crate) fn input(
        validator_key: &'static str,
        index: usize,
        coverage: TraceValidationCoverage,
    ) -> Self {
        Self {
            validator_key,
            artifacts: vec![TraceArtifactSelector::Input(index)],
            coverage,
        }
    }

    pub(crate) fn output(
        validator_key: &'static str,
        index: usize,
        coverage: TraceValidationCoverage,
    ) -> Self {
        Self {
            validator_key,
            artifacts: vec![TraceArtifactSelector::Output(index)],
            coverage,
        }
    }

    pub(crate) fn inputs(
        validator_key: &'static str,
        indices: &[usize],
        coverage: TraceValidationCoverage,
    ) -> Self {
        Self {
            validator_key,
            artifacts: indices
                .iter()
                .copied()
                .map(TraceArtifactSelector::Input)
                .collect(),
            coverage,
        }
    }

    pub(crate) fn outputs(
        validator_key: &'static str,
        indices: &[usize],
        coverage: TraceValidationCoverage,
    ) -> Self {
        Self {
            validator_key,
            artifacts: indices
                .iter()
                .copied()
                .map(TraceArtifactSelector::Output)
                .collect(),
            coverage,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CompileTrace {
    pub version: u32,
    pub initial_core: TraceArtifactId,
    pub artifacts: Vec<TraceArtifact>,
    pub executions: Vec<TraceExecution>,
    pub edges: Vec<TraceEdge>,
    pub validations: Vec<TraceValidation>,
}

pub(crate) struct TraceRecorder {
    trace: CompileTrace,
    next_artifact: u32,
    next_execution: u32,
}

pub(crate) struct TraceCall {
    id: TraceExecutionId,
    inputs: Vec<TraceArtifactId>,
    pass_key: &'static str,
    validation_coverage: TraceValidationCoverage,
    parameters: Vec<TraceParameter>,
}

impl TraceRecorder {
    pub(crate) fn new() -> Self {
        let initial_core = TraceArtifactId(0);
        Self {
            trace: CompileTrace {
                version: COMPILE_TRACE_VERSION,
                initial_core,
                artifacts: vec![TraceArtifact {
                    id: initial_core,
                    representation: TraceRepresentation::Core,
                    state: TraceArtifactState::Provided,
                    producer: None,
                }],
                executions: Vec::new(),
                edges: Vec::new(),
                validations: Vec::new(),
            },
            next_artifact: 1,
            next_execution: 0,
        }
    }

    pub(crate) fn begin(
        &mut self,
        pass_key: &'static str,
        inputs: &[TraceArtifactId],
        validation_coverage: TraceValidationCoverage,
        parameters: Vec<TraceParameter>,
    ) -> TraceCall {
        let id = TraceExecutionId(self.next_execution);
        self.next_execution += 1;
        for artifact in inputs {
            self.trace.edges.push(TraceEdge {
                execution: id,
                artifact: *artifact,
                role: TraceEdgeRole::Input,
            });
        }
        TraceCall {
            id,
            inputs: inputs.to_vec(),
            pass_key,
            validation_coverage,
            parameters,
        }
    }

    pub(crate) fn initial_core(&self) -> TraceArtifactId {
        self.trace.initial_core
    }

    pub(crate) fn complete(
        &mut self,
        call: TraceCall,
        outputs: &[TraceRepresentation],
        validations: &[TraceValidationSpec],
    ) -> Vec<TraceArtifactId> {
        let output_ids = outputs
            .iter()
            .map(|representation| {
                let id = TraceArtifactId(self.next_artifact);
                self.next_artifact += 1;
                self.trace.artifacts.push(TraceArtifact {
                    id,
                    representation: *representation,
                    state: TraceArtifactState::Produced,
                    producer: Some(call.id),
                });
                self.trace.edges.push(TraceEdge {
                    execution: call.id,
                    artifact: id,
                    role: TraceEdgeRole::Output,
                });
                id
            })
            .collect::<Vec<_>>();
        for validation in validations {
            let artifacts = validation
                .artifacts
                .iter()
                .map(|selector| match selector {
                    TraceArtifactSelector::Input(index) => *call
                        .inputs
                        .get(*index)
                        .expect("validation input selector must name an existing artifact"),
                    TraceArtifactSelector::Output(index) => *output_ids
                        .get(*index)
                        .expect("validation output selector must name an existing artifact"),
                })
                .collect();
            self.trace.validations.push(TraceValidation {
                execution: call.id,
                validator_key: validation.validator_key,
                artifacts,
                status: TraceValidationStatus::Passed,
                coverage: validation.coverage,
            });
        }
        self.trace.executions.push(TraceExecution {
            id: call.id,
            pass_key: call.pass_key,
            contract_version: 1,
            inputs: call.inputs,
            outputs: output_ids.clone(),
            status: TracePassStatus::Completed,
            diagnostic_indices: Vec::new(),
            validation_coverage: call.validation_coverage,
            parameters: call.parameters,
        });
        output_ids
    }

    pub(crate) fn reject(&mut self, call: TraceCall, diagnostic_count: usize) {
        self.trace.executions.push(TraceExecution {
            id: call.id,
            pass_key: call.pass_key,
            contract_version: 1,
            inputs: call.inputs,
            outputs: Vec::new(),
            status: TracePassStatus::Rejected,
            diagnostic_indices: (0..diagnostic_count).collect(),
            validation_coverage: call.validation_coverage,
            parameters: call.parameters,
        });
    }

    pub(crate) fn reject_validation(
        &mut self,
        call: TraceCall,
        diagnostic_count: usize,
        validator_key: &'static str,
        artifacts: &[TraceArtifactId],
    ) {
        self.trace.validations.push(TraceValidation {
            execution: call.id,
            validator_key,
            artifacts: artifacts.to_vec(),
            status: TraceValidationStatus::Rejected,
            coverage: TraceValidationCoverage::Direct,
        });
        self.reject(call, diagnostic_count);
    }

    pub(crate) fn not_applicable(&mut self, pass_key: &'static str, reason: &'static str) {
        let call = self.begin(
            pass_key,
            &[],
            TraceValidationCoverage::NotObserved,
            vec![TraceParameter {
                key: "reason",
                value: reason.into(),
            }],
        );
        self.trace.executions.push(TraceExecution {
            id: call.id,
            pass_key: call.pass_key,
            contract_version: 1,
            inputs: Vec::new(),
            outputs: Vec::new(),
            status: TracePassStatus::NotApplicable,
            diagnostic_indices: Vec::new(),
            validation_coverage: call.validation_coverage,
            parameters: call.parameters,
        });
    }

    #[cfg(test)]
    fn validations(&self) -> &[TraceValidation] {
        &self.trace.validations
    }

    pub(crate) fn finish(self) -> CompileTrace {
        self.trace
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[should_panic(expected = "validation output selector must name an existing artifact")]
    fn validation_selectors_cannot_silently_drop_missing_outputs() {
        let mut trace = TraceRecorder::new();
        let core = trace.initial_core();
        let call = trace.begin(
            "test.pass",
            &[core],
            TraceValidationCoverage::Direct,
            Vec::new(),
        );
        trace.complete(
            call,
            &[TraceRepresentation::Core],
            &[TraceValidationSpec::output(
                "test.validator",
                1,
                TraceValidationCoverage::Direct,
            )],
        );
    }

    #[test]
    fn validations_reference_only_the_declared_artifacts() {
        let mut trace = TraceRecorder::new();
        let core = trace.initial_core();
        let call = trace.begin(
            "test.pass",
            &[core],
            TraceValidationCoverage::Composite,
            Vec::new(),
        );
        let output = trace.complete(
            call,
            &[
                TraceRepresentation::Core,
                TraceRepresentation::ExternalBindings,
            ],
            &[TraceValidationSpec::output(
                "test.validator",
                0,
                TraceValidationCoverage::Composite,
            )],
        );
        assert_eq!(trace.validations()[0].artifacts, vec![output[0]]);
    }

    #[test]
    fn rejected_validation_references_its_input_and_stays_rejected() {
        let mut trace = TraceRecorder::new();
        let core = trace.initial_core();
        let call = trace.begin(
            "test.validate",
            &[core],
            TraceValidationCoverage::Direct,
            Vec::new(),
        );
        trace.reject_validation(call, 1, "test.validator", &[core]);
        let trace = trace.finish();
        assert_eq!(trace.validations[0].status, TraceValidationStatus::Rejected);
        assert_eq!(trace.validations[0].artifacts, vec![core]);
        assert_eq!(trace.executions[0].status, TracePassStatus::Rejected);
        assert!(trace.executions[0].outputs.is_empty());
    }
}

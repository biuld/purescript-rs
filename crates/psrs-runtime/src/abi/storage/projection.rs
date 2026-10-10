//! Checked source-to-raw storage call contracts, with no compiler IR dependency.
use super::{StorageAbi, StorageOperation, StorageSourceContract, StorageType, StorageValue};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StorageResultProjection {
    /// The caller recovers a checked payload from the declared raw result.
    Value(StorageType),
    /// The export returns normally without a value. The caller produces Unit.
    Unit,
    /// The export cannot return normally; no successor or payload is produced.
    Never,
}

/// Validated correspondence between source operands and raw ABI operands.
/// This does not authorize erasure of a compiler's dependency graph or provide
/// boxing, reference validation, or instruction correspondence evidence.
pub struct StorageCallProjection {
    source: StorageSourceContract,
    abi: StorageAbi,
    result: StorageResultProjection,
}

impl StorageCallProjection {
    pub fn checked(source: StorageSourceContract, abi: StorageAbi) -> Result<Self, &'static str> {
        if source.parameters.len() != abi.parameters.len() {
            return Err("runtime source and physical argument counts disagree");
        }
        for (source, physical) in source.parameters.iter().zip(abi.parameters) {
            if !matches!(
                (source, physical),
                (StorageValue::Int, StorageType::I32)
                    | (StorageValue::Array, StorageType::Array)
                    | (StorageValue::Element, StorageType::Value)
            ) {
                return Err("runtime source operand has no declared raw storage projection");
            }
        }
        let result = if !abi.returns_normally {
            if abi.result.is_some() || source.payload != StorageValue::Any {
                return Err(
                    "non-returning storage export must have no raw result and an arbitrary source payload",
                );
            }
            StorageResultProjection::Never
        } else {
            match (source.payload, abi.result) {
                (StorageValue::Int, Some(StorageType::I32)) => {
                    StorageResultProjection::Value(StorageType::I32)
                }
                (StorageValue::Array, Some(StorageType::Array)) => {
                    StorageResultProjection::Value(StorageType::Array)
                }
                (StorageValue::Element, Some(StorageType::Value)) => {
                    StorageResultProjection::Value(StorageType::Value)
                }
                (StorageValue::Unit, None) => StorageResultProjection::Unit,
                _ => return Err("runtime source payload has no declared raw result projection"),
            }
        };
        Ok(Self {
            source,
            abi,
            result,
        })
    }

    pub fn source(&self) -> &StorageSourceContract {
        &self.source
    }
    pub fn abi(&self) -> &StorageAbi {
        &self.abi
    }
    pub fn result(&self) -> StorageResultProjection {
        self.result
    }
    pub fn operands(&self) -> impl ExactSizeIterator<Item = (StorageValue, StorageType)> + '_ {
        self.source
            .parameters
            .iter()
            .copied()
            .zip(self.abi.parameters.iter().copied())
    }
}

impl StorageOperation {
    pub fn projection(self) -> Result<StorageCallProjection, &'static str> {
        StorageCallProjection::checked(self.source_contract(), self.abi())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_storage_export_has_an_explicit_result_projection() {
        for (operation, result) in [
            (
                StorageOperation::Fill,
                StorageResultProjection::Value(StorageType::Array),
            ),
            (
                StorageOperation::Read,
                StorageResultProjection::Value(StorageType::Value),
            ),
            (StorageOperation::Write, StorageResultProjection::Unit),
            (StorageOperation::Trap, StorageResultProjection::Never),
        ] {
            let plan = operation.projection().unwrap();
            assert_eq!(plan.result(), result);
            assert_eq!(
                plan.operands().len(),
                operation.source_contract().parameters.len()
            );
            assert_eq!(plan.abi().export, operation.abi().export);
        }
    }

    #[test]
    fn rejects_operand_and_payload_contract_drift() {
        let mut abi = StorageOperation::Fill.abi();
        abi.parameters = &[StorageType::Value, StorageType::I32];
        assert!(
            StorageCallProjection::checked(StorageOperation::Fill.source_contract(), abi).is_err()
        );
        let mut abi = StorageOperation::Write.abi();
        abi.result = Some(StorageType::I32);
        assert!(
            StorageCallProjection::checked(StorageOperation::Write.source_contract(), abi).is_err()
        );
        let mut abi = StorageOperation::Trap.abi();
        abi.returns_normally = true;
        assert!(
            StorageCallProjection::checked(StorageOperation::Trap.source_contract(), abi).is_err()
        );
        let mut abi = StorageOperation::Trap.abi();
        abi.result = Some(StorageType::Value);
        assert!(
            StorageCallProjection::checked(StorageOperation::Trap.source_contract(), abi).is_err()
        );
        let mut abi = StorageOperation::Read.abi();
        abi.parameters = &[StorageType::Array];
        assert!(
            StorageCallProjection::checked(StorageOperation::Read.source_contract(), abi).is_err()
        );
    }
}

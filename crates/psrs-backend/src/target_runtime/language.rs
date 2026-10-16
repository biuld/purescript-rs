use super::*;
use crate::cc::ValueShape;
use psrs_hir::{BuiltinType, TypeKind};
use psrs_runtime::RawCallProtocol;

impl ArtifactImplementation {
    pub(crate) fn language_signature(&self) -> Result<(Vec<ValueShape>, ValueShape), String> {
        let mut ty = (self.intrinsic.descriptor().scheme)();
        let mut parameters = Vec::new();
        while let TypeKind::Function { parameter, result } = ty.kind {
            parameters.push(shape(&parameter.kind)?);
            ty = *result;
        }
        Ok((parameters, shape(&ty.kind)?))
    }

    /// The value protocol must implement the checked language scheme and raw ABI.
    pub(crate) fn validate_protocol(&self) -> Result<(), String> {
        let (parameters, result) = self.language_signature()?;
        let expected = match self.abi.protocol {
            RawCallProtocol::Scalars => CoreSignature {
                parameters: parameters.into_iter().map(raw).collect::<Result<_, _>>()?,
                result: self.raw_result(result)?,
            },
            RawCallProtocol::Utf8Input if parameters == [ValueShape::String] => CoreSignature {
                parameters: vec![CoreType::I32, CoreType::I32],
                result: self.raw_result(result)?,
            },
            RawCallProtocol::Utf8Output { capacity }
                if parameters.len() == 1
                    && result == ValueShape::String
                    && capacity > 0
                    && capacity <= i32::MAX as usize =>
            {
                CoreSignature {
                    parameters: vec![raw(parameters[0])?, CoreType::I32, CoreType::I32],
                    result: Some(CoreType::I32),
                }
            }
            _ => return Err("artifact value protocol cannot implement its language scheme".into()),
        };
        if expected != self.signature() {
            return Err("artifact value protocol does not match its raw ABI".into());
        }
        if !matches!(self.abi.protocol, RawCallProtocol::Scalars)
            && !self.intrinsic.descriptor().effects.may_trap
        {
            return Err("buffer allocation requires a possibly trapping language contract".into());
        }
        Ok(())
    }
}

fn shape(kind: &TypeKind) -> Result<ValueShape, String> {
    match kind {
        TypeKind::Constructor(BuiltinType::Int) => Ok(ValueShape::Integer),
        TypeKind::Constructor(BuiltinType::Number) => Ok(ValueShape::Number),
        TypeKind::Constructor(BuiltinType::Boolean) => Ok(ValueShape::Boolean),
        TypeKind::Constructor(BuiltinType::Char) => Ok(ValueShape::Integer),
        TypeKind::Constructor(BuiltinType::Unit) => Ok(ValueShape::Integer),
        TypeKind::Constructor(BuiltinType::String) => Ok(ValueShape::String),
        _ => Err("artifact calls require a supported closed language scheme".into()),
    }
}

fn raw(shape: ValueShape) -> Result<CoreType, String> {
    match shape {
        ValueShape::Integer | ValueShape::Boolean => Ok(CoreType::I32),
        ValueShape::Number => Ok(CoreType::F64),
        _ => Err("a raw scalar protocol cannot transport a GC value".into()),
    }
}
impl ArtifactImplementation {
    fn raw_result(&self, result: ValueShape) -> Result<Option<CoreType>, String> {
        let mut ty = (self.intrinsic.descriptor().scheme)();
        while let TypeKind::Function { result, .. } = ty.kind {
            ty = *result;
        }
        if matches!(ty.kind, TypeKind::Constructor(BuiltinType::Unit)) {
            Ok(None)
        } else {
            raw(result).map(Some)
        }
    }
}

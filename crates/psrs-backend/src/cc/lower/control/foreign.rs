//! Host-resource values cannot be stored in a software continuation.
use super::{FunctionLowerer, control_error};
use crate::BackendError;
use psrs_core::{Expr, Type, TypeConstructor};
use std::collections::HashSet;

impl FunctionLowerer<'_> {
    pub(super) fn reject_host_resource_captures(
        &self,
        argument: &Expr,
    ) -> Result<(), Vec<BackendError>> {
        let mut bound = HashSet::new();
        let mut captures = Vec::new();
        super::super::lambda::collect_captures(argument, &mut bound, &mut captures);
        for local in captures {
            let Some(&ty) = self.local_types.get(&local) else {
                continue;
            };
            if self.is_host_resource(ty) {
                return Err(control_error(
                    argument.span,
                    "control callback cannot capture a host resource handle",
                ));
            }
        }
        Ok(())
    }

    fn is_host_resource(&self, ty: psrs_core::TypeId) -> bool {
        match self.module.types.get(ty.0 as usize) {
            Some(Type::Constructor(TypeConstructor::User(id))) => {
                self.module.opaque_ids.contains(id)
            }
            Some(Type::Application(head, _)) => self.is_host_resource(*head),
            _ => false,
        }
    }
}

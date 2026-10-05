//! The Core-to-CC boundary side table.
//!
//! Checked instantiation evidence and the representation policies of source
//! constructors travel to P8 through this table, beside CC, the same way
//! [`crate::ExternalBindings`] carries the WIT boundary. P8 consumes it: it
//! does not read the Core type arena for a relation, key a semantic decision on
//! a HIR identity, or derive a producer policy by searching signatures.

use crate::cc::{RefShape, Reference, Signature, SignatureId, ValueShape};
use psrs_core::{Instantiation, Module as CoreModule, TypeConstructor, TypeId};
use psrs_hir::TypeVariableId;
use std::collections::{HashMap, HashSet};

/// A representation owner's policy for the runtime form of a source
/// constructor. It records the constructor's fixed calling-convention
/// parameters; the payload is the value's declared result.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum RepresentationPolicy {
    /// The fixed parameters are the constructor's checked instantiation
    /// arguments (a function arrow's domain).
    InstantiationArguments,
    /// The owner registered explicit fixed parameters (the Effect runtime
    /// token). The payload remains the application's result.
    Fixed(Vec<TypeId>),
}

/// The registry of representation owners, keyed by source constructor.
///
/// `Function` stores its values under the arrow domain from the checked
/// instantiation; a representation owner registers additional constructors,
/// such as the trusted `Effect`. A constructor with no entry has no callable
/// representation contract, so transport reports it rather than guessing one
/// from the declaration arity or a signature search.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct RepresentationRegistry {
    policies: HashMap<TypeConstructor, RepresentationPolicy>,
}

impl RepresentationRegistry {
    /// The built-in owners. `Function` is representation-directed by its own
    /// checked instantiation arguments.
    pub(crate) fn new() -> Self {
        let mut registry = Self::default();
        registry.register(
            TypeConstructor::Function,
            RepresentationPolicy::InstantiationArguments,
        );
        registry
    }

    /// Registers one constructor's representation owner.
    pub(crate) fn register(&mut self, constructor: TypeConstructor, policy: RepresentationPolicy) {
        self.policies.insert(constructor, policy);
    }

    /// The fixed protocol parameters for a constructor application, or `None`
    /// when no owner registered the constructor.
    pub(crate) fn protocol_parameters(
        &self,
        constructor: TypeConstructor,
        arguments: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        Some(match self.policies.get(&constructor)? {
            RepresentationPolicy::InstantiationArguments => arguments.to_vec(),
            RepresentationPolicy::Fixed(parameters) => parameters.clone(),
        })
    }
}

/// Checked evidence and producer policies at the Core-to-CC boundary.
pub(crate) struct BoundaryEvidence<'a> {
    /// The immutable Core program that owns the checked relation.
    source: &'a CoreModule,
    /// The module under lowering. A relation whose type ids still exist in the
    /// source uses the source; a type allocated only by representation
    /// lowering uses the physical module.
    physical: &'a CoreModule,
    registry: RepresentationRegistry,
    /// The payload-erased protocol signature of every callable constructor,
    /// keyed by the concrete callable's signature. Built by P8 layout from the
    /// registered owners and the module's callable signatures.
    protocols: HashMap<SignatureId, SignatureId>,
}

impl<'a> BoundaryEvidence<'a> {
    pub(crate) fn new(
        source: &'a CoreModule,
        physical: &'a CoreModule,
        registry: RepresentationRegistry,
        protocols: HashMap<SignatureId, SignatureId>,
    ) -> Self {
        Self {
            source,
            physical,
            registry,
            protocols,
        }
    }

    /// A boundary with no relation beyond the module itself, for lowering
    /// fixtures that exercise no abstract constructor protocol.
    #[cfg(test)]
    pub(crate) fn empty(physical: &'a CoreModule) -> Self {
        Self::new(
            physical,
            physical,
            RepresentationRegistry::new(),
            HashMap::new(),
        )
    }

    fn relation(&self, scheme: TypeId, instance: TypeId) -> &'a CoreModule {
        if (scheme.0 as usize) < self.source.types.len()
            && (instance.0 as usize) < self.source.types.len()
        {
            self.source
        } else {
            self.physical
        }
    }

    /// Checks a declaration use through the checking-owned relation, retaining
    /// its solved constructor bindings. P8 reads the result; it does not
    /// reimplement the relation.
    pub(crate) fn instantiation_at(
        &self,
        scheme: TypeId,
        instance: TypeId,
    ) -> Option<Instantiation<'a>> {
        let relation = self.relation(scheme, instance);
        let quantified = scheme_quantifiers(relation, scheme);
        relation.checked_instantiation(scheme, &quantified, instance)
    }

    /// Checks a declaration use whose quantifiers are already known.
    pub(crate) fn checked_instantiation(
        &self,
        scheme: TypeId,
        quantified: &[TypeVariableId],
        instance: TypeId,
    ) -> Option<Instantiation<'a>> {
        self.relation(scheme, instance)
            .checked_instantiation(scheme, quantified, instance)
    }

    /// The fixed protocol parameters for a constructor application.
    pub(crate) fn protocol_parameters(
        &self,
        constructor: TypeConstructor,
        arguments: &[TypeId],
    ) -> Option<Vec<TypeId>> {
        self.registry.protocol_parameters(constructor, arguments)
    }

    /// The payload-erased protocol signature a callable constructor stores its
    /// values under, keyed by the concrete callable signature.
    pub(crate) fn protocol_signature(&self, concrete: SignatureId) -> Option<SignatureId> {
        self.protocols.get(&concrete).copied()
    }
}

/// Leading quantifiers of a scheme. Pre-seeding these keeps their constructor
/// bindings when instantiation opens the same quantifiers.
fn scheme_quantifiers(module: &CoreModule, mut ty: TypeId) -> Vec<TypeVariableId> {
    let mut variables = Vec::new();
    let mut seen = HashSet::new();
    while let Some((bound, body)) = psrs_core::forall_parts(&module.types, ty) {
        if !seen.insert(ty) {
            break;
        }
        variables.extend(bound.iter().copied());
        ty = body;
    }
    variables
}

/// Builds the payload-erased protocol signature for every distinct callable
/// signature. The protocol keeps the concrete calling convention but erases the
/// payload, so a producer and its generic consumer can be related by the
/// constructor policy rather than by a cast onto the consumer's signature.
pub(crate) fn payload_erased_protocols(
    representations: &mut crate::cc::RepresentationTable,
    function_types: &HashMap<TypeId, SignatureId>,
) -> HashMap<SignatureId, SignatureId> {
    let mut protocols = HashMap::new();
    let mut interned = HashMap::<Signature, SignatureId>::new();
    let mut ids = function_types.values().copied().collect::<Vec<_>>();
    ids.sort_by_key(|id| id.0);
    ids.dedup();
    for concrete in ids {
        if protocols.contains_key(&concrete) {
            continue;
        }
        let Some(parameters) = representations
            .signature(concrete)
            .map(|signature| signature.parameters.clone())
        else {
            continue;
        };
        let protocol = Signature {
            parameters,
            result: ValueShape::Reference(Reference {
                nullable: false,
                heap: RefShape::Erased,
            }),
        };
        let protocol_id = if let Some(existing) = interned.get(&protocol) {
            *existing
        } else {
            let new = representations.add_signature(protocol.clone());
            interned.insert(protocol, new);
            new
        };
        protocols.insert(concrete, protocol_id);
    }
    protocols
}

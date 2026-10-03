//! Superclass edges as instantiable templates.
//!
//! A superclass edge is written over the subclass's own parameters, so it is
//! stored as a [`TypeTemplate`] rather than as the parameter *names* that
//! supply its arguments. Dictionary construction, superclass search, and
//! instance elaboration all instantiate that one template; nothing reads an
//! argument by matching a parameter name, so two spellings of the same
//! constraint cannot disagree.

use super::super::signature::{flatten_spine, nominal_type_id};
use super::super::unify::substitute;
use super::super::*;
use super::fundeps::collect_infer_variables;
use std::collections::HashMap;

impl Checker {
    /// Elaborates one superclass edge `C τ...`. The edge's arguments are
    /// ordinary types written over the subclass's own parameters, so the class's
    /// parameters become the edge template's binders and any type over them
    /// elaborates; `class C (Array a) <= D a` is one constraint written one
    /// way, not a second spelling to be matched by name. The edge's field name
    /// follows the official compiler's `ClassName<index>` scheme, so the
    /// evidence and the field agree by construction. `local` gates the
    /// diagnostics because an imported class was already checked in its module.
    pub(in crate::typecheck) fn build_superclass(
        &mut self,
        superclass: &hir::Type,
        parameters: &[String],
        index: usize,
        local: bool,
    ) -> Option<SuperclassInfo> {
        let (head, arguments) = flatten_spine(superclass);
        let Some(class_id) = nominal_type_id(head) else {
            if local {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    superclass.span,
                    "a superclass must name a class",
                ));
            }
            return None;
        };
        let Some(arity) = self
            .env
            .classes
            .get(&class_id)
            .map(|class| class.parameters.len())
        else {
            if local {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    superclass.span,
                    "a superclass names an unknown class",
                ));
            }
            return None;
        };
        if arguments.len() != arity {
            if local {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedClass,
                    superclass.span,
                    "a superclass constraint has the wrong number of type arguments",
                ));
            }
            return None;
        }
        let template = self.build_superclass_template(parameters, arguments, local)?;
        let name = self
            .env
            .type_names
            .get(&class_id)
            .cloned()
            .unwrap_or_else(|| format!("Class{}", class_id.index));
        Some(SuperclassInfo {
            class_id,
            template,
            field: format!("{name}{index}"),
            span: superclass.span,
        })
    }

    /// Elaborates a superclass edge's arguments into the template they form.
    /// Every variable the edge mentions must be one of the class's own
    /// parameters: an edge is written over that scope and a free variable is
    /// not a well-formed edge. `None` reports the offending argument and drops
    /// the edge, which leaves the class environment as it was before.
    fn build_superclass_template(
        &mut self,
        parameters: &[String],
        arguments: Vec<&hir::Type>,
        local: bool,
    ) -> Option<TypeTemplate> {
        let binders = self.superclass_binders(parameters);
        let mut scope = HashMap::new();
        for (parameter, binder) in parameters.iter().zip(&binders) {
            scope.insert(parameter.clone(), InferType::Variable(*binder));
        }
        let body = arguments
            .iter()
            .map(|argument| self.elaborate_type(argument, &mut scope))
            .collect::<Vec<_>>();
        for (argument, ty) in arguments.iter().zip(&body) {
            let mut variables = HashSet::new();
            collect_infer_variables(ty, &mut variables);
            if variables.iter().any(|variable| !binders.contains(variable)) {
                if local {
                    self.state.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::UnsupportedClass,
                        argument.span,
                        "a superclass argument must be one of the class's type parameters",
                    ));
                }
                return None;
            }
        }
        Some(TypeTemplate { binders, body })
    }

    /// One rigid binder variable per class parameter, in parameter order. They
    /// are the scope a superclass edge is written in, so they are never solved:
    /// every instantiation substitutes them away before the result is used.
    fn superclass_binders(&mut self, parameters: &[String]) -> Vec<u32> {
        let mut binders = Vec::with_capacity(parameters.len());
        for _ in parameters {
            if let InferType::Variable(id) = self.fresh() {
                self.state.rigid.insert(id);
                binders.push(id);
            }
        }
        binders
    }
}

/// Substitutes a subclass's `arguments` for the template's `binders` and
/// returns the template's body. The substitution is the shared one, so a
/// template body that mentions its binders in any position — as a bare name, as
/// `Array a`, or under a row or a constraint — is instantiated the same way.
pub(in crate::typecheck) fn instantiate_template(
    template: &TypeTemplate,
    arguments: &[InferType],
) -> Vec<InferType> {
    if arguments.len() != template.binders.len() {
        // A template instantiated with the wrong number of arguments has no
        // meaning. `build_superclass` checks the arity once, and every caller
        // passes a class's parameters in order, so this cannot be reached from
        // source.
        return Vec::new();
    }
    let mapping: HashMap<u32, InferType> = template
        .binders
        .iter()
        .copied()
        .zip(arguments.iter().cloned())
        .collect();
    template
        .body
        .iter()
        .map(|argument| substitute(argument, &mapping))
        .collect()
}

impl Checker {
    /// The superclass constraints a dictionary for `class_id arguments` stores,
    /// each with the dictionary field that holds it, in edge order. Each edge is
    /// instantiated over the subclass's arguments through the shared
    /// substitution, so dictionary construction and superclass search agree by
    /// construction.
    pub(in crate::typecheck) fn superclass_constraints(
        &self,
        class_id: hir::TypeId,
        arguments: &[InferType],
    ) -> Vec<(String, ClassConstraint)> {
        let Some(class) = self.env.classes.get(&class_id) else {
            return Vec::new();
        };
        class
            .superclasses
            .iter()
            .map(|superclass| {
                (
                    superclass.field.clone(),
                    ClassConstraint {
                        class_id: superclass.class_id,
                        arguments: instantiate_template(&superclass.template, arguments),
                        span: superclass.span,
                    },
                )
            })
            .collect()
    }
}

use super::*;

/// A declaration's elaborated signature.
pub(super) struct ElaboratedSignature {
    /// The signature's class constraints, in source order.
    pub(super) constraints: Vec<ClassConstraint>,
    /// One dictionary parameter per constraint, in the same order. The parameter
    /// the body discharges a constraint with is the parameter at that
    /// constraint's position.
    pub(super) parameters: Vec<(LocalId, InferType)>,
    /// The type the body is checked at, with the `forall`/`=>` spine flattened.
    pub(super) ty: InferType,
    /// The type variables the signature names, keyed by source name. A typed
    /// pattern or an ascription inside the body reuses these exact variables
    /// instead of elaborating a second rigid variable under the same name.
    pub(super) annotation_variables: HashMap<String, InferType>,
    /// The variables the signature's own `forall` binders introduced, in binding
    /// order. These are the declaration's declared polymorphism, so its scheme
    /// quantifies exactly them however the solver levelled them.
    pub(super) quantified: Vec<u32>,
}

impl Checker {
    /// Elaborates a signature at a use site. Its universally quantified
    /// variables must be fresh and flexible so each imported use can choose a
    /// different concrete type.
    pub(super) fn elaborate_imported_signature(&mut self, ty: &hir::Type) -> InferType {
        let (_, body) = self.elaborate_constrained_signature(ty, false);
        body
    }

    /// Elaborates an imported signature together with its class constraints.
    pub(super) fn elaborate_imported_constraints(
        &mut self,
        ty: &hir::Type,
    ) -> (Vec<ClassConstraint>, InferType) {
        self.elaborate_constrained_signature(ty, false)
    }

    /// Elaborates a declaration's signature, flattening its constraints and
    /// synthesizing one dictionary parameter per constraint in source order.
    pub(super) fn elaborate_declaration_signature(
        &mut self,
        ty: &hir::Type,
    ) -> ElaboratedSignature {
        let mut variables = HashMap::new();
        let mut quantified = Vec::new();
        let (constraints, body) =
            self.elaborate_constraint_spine(ty, &mut variables, true, &mut quantified);
        let mut parameters = Vec::with_capacity(constraints.len());
        for constraint in &constraints {
            let dictionary_type = self.dictionary_type(constraint);
            let id = LocalId(self.state.next_dictionary_local);
            self.state.next_dictionary_local += 1;
            parameters.push((id, dictionary_type));
        }
        ElaboratedSignature {
            constraints,
            parameters,
            ty: body,
            annotation_variables: variables,
            quantified,
        }
    }

    /// Walks a signature's `forall`/`=>` spine, elaborating every constraint
    /// before its body. Constraints appear in source order, and the binders of
    /// each `forall` are appended to `quantified` in binding order.
    pub(super) fn elaborate_constrained_signature(
        &mut self,
        ty: &hir::Type,
        rigid: bool,
    ) -> (Vec<ClassConstraint>, InferType) {
        let mut variables = HashMap::new();
        let mut quantified = Vec::new();
        self.elaborate_constraint_spine(ty, &mut variables, rigid, &mut quantified)
    }

    fn elaborate_constraint_spine(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
        rigid: bool,
        quantified: &mut Vec<u32>,
    ) -> (Vec<ClassConstraint>, InferType) {
        match &ty.kind {
            hir::TypeKind::Forall {
                variables: binders,
                body,
            } => {
                let mut scoped_variables = variables.clone();
                let introduced = self.bind_forall_variables(binders, &mut scoped_variables, rigid);
                quantified.extend(introduced);
                let result =
                    self.elaborate_constraint_spine(body, &mut scoped_variables, rigid, quantified);
                *variables = scoped_variables;
                result
            }
            hir::TypeKind::Constrained { constraint, body } => {
                let class = self.elaborate_constraint(constraint, variables, rigid);
                let (rest, body_ty) =
                    self.elaborate_constraint_spine(body, variables, rigid, quantified);
                let mut constraints = Vec::new();
                if let Some(class) = class {
                    constraints.push(class);
                }
                constraints.extend(rest);
                (constraints, body_ty)
            }
            _ => (Vec::new(), self.elaborate_type_mode(ty, variables, rigid)),
        }
    }

    pub(super) fn elaborate_type(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
    ) -> InferType {
        self.elaborate_type_mode(ty, variables, true)
    }

    pub(super) fn elaborate_type_mode(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
        rigid_variables: bool,
    ) -> InferType {
        match &ty.kind {
            hir::TypeKind::Wildcard => self.fresh(),
            hir::TypeKind::Variable(name) => {
                if let Some(variable) = variables.get(name) {
                    return variable.clone();
                }
                let variable = self.fresh();
                if rigid_variables && let InferType::Variable(id) = variable {
                    self.state.rigid.insert(id);
                    self.scope.type_variable_names.insert(id, name.clone());
                }
                variables.insert(name.clone(), variable.clone());
                variable
            }
            hir::TypeKind::Constructor(builtin) => match builtin {
                hir::BuiltinType::Int => InferType::Constructor(TypeConstructor::Int),
                hir::BuiltinType::Number => InferType::Constructor(TypeConstructor::Number),
                hir::BuiltinType::Boolean => InferType::Constructor(TypeConstructor::Boolean),
                hir::BuiltinType::String => InferType::Constructor(TypeConstructor::String),
                hir::BuiltinType::Char => InferType::Constructor(TypeConstructor::Char),
                hir::BuiltinType::Unit => InferType::Constructor(TypeConstructor::Unit),
                hir::BuiltinType::Array => InferType::Constructor(TypeConstructor::Array),
                hir::BuiltinType::Function => InferType::Constructor(TypeConstructor::Function),
                hir::BuiltinType::Record => InferType::Constructor(TypeConstructor::Record),
                hir::BuiltinType::Row => InferType::Constructor(TypeConstructor::Row),
                // `Type`, `Constraint`, and `Symbol` name kinds, and official
                // PureScript declares each of them with kind `Type`, so a type
                // position that names one is an ordinary nominal type on the
                // same spine as `Record` and `Row`. Their kinds come from the
                // one primitive kind table, not from a reading here.
                hir::BuiltinType::Type => InferType::Constructor(TypeConstructor::Type),
                hir::BuiltinType::Constraint => InferType::Constructor(TypeConstructor::Constraint),
                hir::BuiltinType::Symbol => InferType::Constructor(TypeConstructor::Symbol),
            },
            hir::TypeKind::Named(id) | hir::TypeKind::Opaque(id) => {
                if self.env.synonyms.contains_key(id) {
                    self.expand_synonym(*id, Vec::new(), ty.span)
                } else {
                    // Foreign data stays a nominal user constructor. Opacity is
                    // `Module.opaque_ids`, not a separate type node and not `Int`.
                    InferType::Constructor(TypeConstructor::User(*id))
                }
            }
            hir::TypeKind::Application(function, argument) => {
                let (head, arguments) = flatten_spine(ty);
                if let Some(id) = nominal_type_id(head)
                    && let Some(arity) = self
                        .env
                        .synonyms
                        .get(&id)
                        .map(|synonym| synonym.parameters.len())
                {
                    // Arguments past the synonym's own parameters apply to the
                    // expanded body. `C2 a z` has kind `k -> Type`, so
                    // `C2 a z x` is `(C2 a z) x`, not a third synonym parameter.
                    let elaborated = arguments
                        .iter()
                        .take(arity)
                        .map(|argument| {
                            self.elaborate_type_mode(argument, variables, rigid_variables)
                        })
                        .collect();
                    let mut expanded = self.expand_synonym(id, elaborated, ty.span);
                    for argument in arguments.iter().skip(arity) {
                        expanded = InferType::Application(
                            Box::new(expanded),
                            Box::new(self.elaborate_type_mode(
                                argument,
                                variables,
                                rigid_variables,
                            )),
                        );
                    }
                    return expanded;
                }
                InferType::Application(
                    Box::new(self.elaborate_type_mode(function, variables, rigid_variables)),
                    Box::new(self.elaborate_type_mode(argument, variables, rigid_variables)),
                )
            }
            hir::TypeKind::OperatorChain { .. } => {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::UnsupportedType,
                    ty.span,
                    "type operator chain reached type checking before P4",
                ));
                self.fresh()
            }
            hir::TypeKind::Function { parameter, result } => arrow(
                self.elaborate_type_mode(parameter, variables, rigid_variables),
                self.elaborate_type_mode(result, variables, rigid_variables),
            ),
            hir::TypeKind::Forall {
                variables: binders,
                body,
            } => {
                let mut scoped_variables = variables.clone();
                let quantified =
                    self.bind_forall_variables(binders, &mut scoped_variables, rigid_variables);
                let body = self.elaborate_type_mode(body, &mut scoped_variables, rigid_variables);
                InferType::ForAll {
                    variables: quantified,
                    body: Box::new(body),
                }
            }
            hir::TypeKind::Constrained { constraint, body } => {
                let Some(constraint) =
                    self.elaborate_constraint(constraint, variables, rigid_variables)
                else {
                    return self.fresh();
                };
                InferType::Constrained {
                    constraints: vec![constraint],
                    body: Box::new(self.elaborate_type_mode(body, variables, rigid_variables)),
                }
            }
            hir::TypeKind::Record { fields, tail } => {
                self.elaborate_record(fields, tail.as_deref(), variables, rigid_variables)
            }
            hir::TypeKind::Row { .. } => self.elaborate_row(ty, variables, rigid_variables),
            hir::TypeKind::Integer(text) => match parse_type_level_int(text) {
                Some(value) => InferType::TypeLevelInt(value),
                None => {
                    self.state.errors.push(TypeCheckError::new(
                        TypeCheckErrorKind::UnsupportedType,
                        ty.span,
                        format!("type-level integer literal `{text}` is not a representable Int"),
                    ));
                    self.fresh()
                }
            },
            hir::TypeKind::String(value) => {
                // The lexer decodes a type-level string to a sequence of
                // Unicode scalar values and rejects an unpaired surrogate
                // escape, so the payload here is already a valid `Symbol`.
                InferType::TypeLevelString(value.clone())
            }
        }
    }

    /// Elaborates the general row form `( label :: field | tail )` as a row
    /// value. A record type is the same construction applied to `Record`, so
    /// both spellings reach one row.
    fn elaborate_row(
        &mut self,
        ty: &hir::Type,
        variables: &mut HashMap<String, InferType>,
        rigid_variables: bool,
    ) -> InferType {
        let hir::TypeKind::Row { fields, tail } = &ty.kind else {
            unreachable!("only a row type is elaborated as a row")
        };
        row_from_fields(
            self.elaborate_row_fields(fields, variables, rigid_variables),
            self.elaborate_row_tail(tail.as_deref(), variables, rigid_variables),
        )
    }

    /// Elaborates a record type as `Record row`, the same construction an
    /// explicit `Record` application reaches. There is one record construction
    /// and record syntax does not have a second route into it.
    fn elaborate_record(
        &mut self,
        fields: &[hir::TypeField],
        tail: Option<&hir::Type>,
        variables: &mut HashMap<String, InferType>,
        rigid_variables: bool,
    ) -> InferType {
        record_type(
            self.elaborate_row_fields(fields, variables, rigid_variables),
            self.elaborate_row_tail(tail, variables, rigid_variables),
        )
    }

    /// Elaborates row fields, reporting a duplicate label once at the field
    /// that repeats it. The construction sorts them into canonical label order,
    /// so this keeps source order.
    fn elaborate_row_fields(
        &mut self,
        fields: &[hir::TypeField],
        variables: &mut HashMap<String, InferType>,
        rigid_variables: bool,
    ) -> Vec<(String, InferType)> {
        let mut seen = HashSet::new();
        let mut elaborated = Vec::with_capacity(fields.len());
        for field in fields {
            if !seen.insert(field.label.clone()) {
                self.state.errors.push(TypeCheckError::new(
                    TypeCheckErrorKind::TypeMismatch,
                    field.span,
                    format!("record label `{}` occurs more than once", field.label),
                ));
                continue;
            }
            elaborated.push((
                field.label.clone(),
                self.elaborate_type_mode(&field.ty, variables, rigid_variables),
            ));
        }
        elaborated
    }

    /// Elaborates a row tail. A tail is an ordinary type, so a variable, a
    /// nested row, or a literal all reach the row normalizer as written; a
    /// closed row ends here.
    fn elaborate_row_tail(
        &mut self,
        tail: Option<&hir::Type>,
        variables: &mut HashMap<String, InferType>,
        rigid_variables: bool,
    ) -> InferType {
        match tail {
            None => InferType::RowEmpty,
            Some(tail) => self.elaborate_type_mode(tail, variables, rigid_variables),
        }
    }

    fn bind_forall_variables(
        &mut self,
        binders: &[hir::TypeParameter],
        variables: &mut HashMap<String, InferType>,
        rigid: bool,
    ) -> Vec<u32> {
        let mut kind_scope = HashMap::new();
        let mut quantified = Vec::with_capacity(binders.len());
        for binder in binders {
            let variable = self.fresh();
            let kind = binder
                .kind
                .as_ref()
                .map(|kind| self.kind_from_hir(kind, &kind_scope))
                .unwrap_or_else(|| self.fresh_kind());
            if let InferType::Variable(id) = variable {
                self.record_variable_kind(id, kind.clone());
                if rigid {
                    self.state.rigid.insert(id);
                }
                self.scope
                    .type_variable_names
                    .insert(id, binder.name.clone());
                quantified.push(id);
            }
            // An unannotated forall binder can itself be a kind variable; a
            // later binder annotation may refer to it.
            if binder.kind.is_none() {
                kind_scope.insert(binder.name.clone(), kind);
            }
            variables.insert(binder.name.clone(), variable);
        }
        quantified
    }

    /// Expands a type synonym application by substituting the elaborated
    /// arguments for the synonym's parameters. A recursive synonym, which the
    /// kind pass rejects, is reported here rather than looping.
    fn expand_synonym(
        &mut self,
        id: hir::TypeId,
        arguments: Vec<InferType>,
        span: TextRange,
    ) -> InferType {
        let Some(synonym) = self.env.synonyms.get(&id).cloned() else {
            return InferType::Constructor(TypeConstructor::User(id));
        };
        if arguments.len() != synonym.parameters.len() {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedType,
                span,
                "a type synonym must be fully applied",
            ));
            return self.fresh();
        }
        if !self.state.expanding.insert(id) {
            self.state.errors.push(TypeCheckError::new(
                TypeCheckErrorKind::UnsupportedType,
                span,
                "a type synonym may not be recursive",
            ));
            return self.fresh();
        }
        let mut locals = HashMap::new();
        for (parameter, argument) in synonym.parameters.iter().zip(arguments) {
            locals.insert(parameter.clone(), argument);
        }
        let expanded = self.elaborate_type(&synonym.body, &mut locals);
        self.state.expanding.remove(&id);
        expanded
    }
}

pub(super) fn nominal_type_id(ty: &hir::Type) -> Option<hir::TypeId> {
    match &ty.kind {
        hir::TypeKind::Named(id) | hir::TypeKind::Opaque(id) => Some(*id),
        _ => None,
    }
}

pub(super) fn flatten_spine(ty: &hir::Type) -> (&hir::Type, Vec<&hir::Type>) {
    let mut arguments = Vec::new();
    let mut head = ty;
    while let hir::TypeKind::Application(function, argument) = &head.kind {
        arguments.push(argument.as_ref());
        head = function.as_ref();
    }
    arguments.reverse();
    (head, arguments)
}

/// Parses a type-level integer literal. The lexer spells these in decimal or
/// hexadecimal, and a negative literal reaches here with its sign once the
/// prefix operator has been lowered. A value outside `i64` has no
/// representation as an `Int` and is rejected rather than truncated.
fn parse_type_level_int(text: &str) -> Option<i64> {
    let (sign, digits) = match text.strip_prefix('-') {
        Some(digits) => (-1i64, digits),
        None => (1i64, text.strip_prefix('+').unwrap_or(text)),
    };
    let magnitude = match digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        Some(hexadecimal) => i64::from_str_radix(hexadecimal, 16).ok()?,
        None => digits.parse::<i64>().ok()?,
    };
    magnitude.checked_mul(sign)
}

use psrs_hir::{self as hir, Type, TypeKind};

pub(super) fn desugar_module_types(module: &mut hir::Module) {
    for declaration in &mut module.declarations {
        if let Some(signature) = &mut declaration.signature {
            *signature = desugar_type(signature.clone());
        }
    }
    for external in &mut module.externals {
        if let Some(signature) = &mut external.signature {
            *signature = desugar_type(signature.clone());
        }
    }
    for declaration in &mut module.types {
        for parameter in &mut declaration.parameters {
            if let Some(kind) = &mut parameter.kind {
                *kind = desugar_type(kind.clone());
            }
        }
        for constructor in &mut declaration.constructors {
            constructor.fields = std::mem::take(&mut constructor.fields)
                .into_iter()
                .map(desugar_type)
                .collect();
        }
        for member in &mut declaration.members {
            if let Some(signature) = &mut member.signature {
                *signature = desugar_type(signature.clone());
            }
        }
        if let Some(body) = &mut declaration.body {
            *body = desugar_type(body.clone());
        }
        declaration.superclasses = std::mem::take(&mut declaration.superclasses)
            .into_iter()
            .map(desugar_type)
            .collect();
        if let Some(kind) = &mut declaration.declared_kind {
            *kind = desugar_type(kind.clone());
        }
    }
    for instance in &mut module.instances {
        instance.context = std::mem::take(&mut instance.context)
            .into_iter()
            .map(desugar_type)
            .collect();
        instance.head = desugar_type(instance.head.clone());
    }
}

pub(super) fn desugar_type(ty: Type) -> Type {
    let span = ty.span;
    let kind = match ty.kind {
        TypeKind::Application(function, argument) => TypeKind::Application(
            Box::new(desugar_type(*function)),
            Box::new(desugar_type(*argument)),
        ),
        TypeKind::OperatorChain {
            operands,
            operators,
        } => {
            let operands = operands.into_iter().map(desugar_type).collect();
            return super::fixity::reassociate_type(operands, operators, span);
        }
        TypeKind::Function { parameter, result } => TypeKind::Function {
            parameter: Box::new(desugar_type(*parameter)),
            result: Box::new(desugar_type(*result)),
        },
        TypeKind::Forall { variables, body } => TypeKind::Forall {
            variables: variables
                .into_iter()
                .map(|mut variable| {
                    variable.kind = variable.kind.map(desugar_type);
                    variable
                })
                .collect(),
            body: Box::new(desugar_type(*body)),
        },
        TypeKind::Constrained { constraint, body } => TypeKind::Constrained {
            constraint: Box::new(desugar_type(*constraint)),
            body: Box::new(desugar_type(*body)),
        },
        TypeKind::Row { fields, tail } => TypeKind::Row {
            fields: fields
                .into_iter()
                .map(|mut field| {
                    field.ty = desugar_type(field.ty);
                    field
                })
                .collect(),
            tail: tail.map(|tail| Box::new(desugar_type(*tail))),
        },
        TypeKind::Record { fields, tail } => TypeKind::Record {
            fields: fields
                .into_iter()
                .map(|mut field| {
                    field.ty = desugar_type(field.ty);
                    field
                })
                .collect(),
            tail: tail.map(|tail| Box::new(desugar_type(*tail))),
        },
        leaf => leaf,
    };
    Type { kind, span }
}

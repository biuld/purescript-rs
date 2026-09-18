use psrs_span::{SourceFile, TextRange};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    pub stage: &'static str,
    pub span: TextRange,
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Artifact {
    pub wasm: Vec<u8>,
    pub wat: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct IrDumps {
    pub core: String,
    pub cc: String,
    pub mir: String,
}

impl IrDumps {
    pub fn get(&self, stage: &str) -> Option<&str> {
        match stage {
            "core" => Some(&self.core),
            "cc" => Some(&self.cc),
            "mir" => Some(&self.mir),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Compilation {
    pub artifact: Artifact,
    pub dumps: IrDumps,
}

pub fn compile_source(source_name: &str, source_text: &str) -> Result<Artifact, Vec<Diagnostic>> {
    let core = lower_source_to_core(source_name, source_text)?;
    let output = psrs_backend::compile(core).map_err(backend_diagnostics)?;
    Ok(Artifact {
        wasm: output.wasm,
        wat: output.wat,
    })
}

pub fn compile_source_with_dumps(
    source_name: &str,
    source_text: &str,
) -> Result<Compilation, Vec<Diagnostic>> {
    let core = lower_source_to_core(source_name, source_text)?;
    let core_dump = format!("{core:#?}");
    let stages = psrs_backend::compile_with_stages(core).map_err(backend_diagnostics)?;
    Ok(Compilation {
        artifact: Artifact {
            wasm: stages.artifact.wasm,
            wat: stages.artifact.wat,
        },
        dumps: IrDumps {
            core: core_dump,
            cc: format!("{:#?}", stages.cc),
            mir: format!("{:#?}", stages.mir),
        },
    })
}

fn lower_source_to_core(
    source_name: &str,
    source_text: &str,
) -> Result<psrs_core::Module, Vec<Diagnostic>> {
    let source = SourceFile::new(source_name, source_text);
    let (tokens, lex_errors) = psrs_syntax::lex(source.text());
    if !lex_errors.is_empty() {
        return Err(lex_errors
            .into_iter()
            .map(|error| diagnostic("P0 lex", error.span, error.message))
            .collect());
    }
    let layout_tokens = psrs_syntax::add_layout(&source, &tokens);
    let cst = psrs_syntax::parse_module(&layout_tokens)
        .map_err(|error| vec![diagnostic("P1 parse", error.span, error.message)])?;
    let ast = psrs_ast::lower_module(cst);
    let intrinsics = psrs_resolve::bootstrap_intrinsics();
    let hir = psrs_resolve::resolve_module_with_externals(ast, psrs_hir::ModuleId(0), &intrinsics)
        .map_err(|errors| {
            errors
                .into_iter()
                .map(|error| diagnostic("P3 resolve", error.span, error.message()))
                .collect::<Vec<_>>()
        })?;
    let hir = psrs_desugar::desugar_module(hir).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic("P4 desugar", error.span, error.message))
            .collect::<Vec<_>>()
    })?;
    let thir = psrs_typecheck::typecheck_module(hir).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic("P5 typecheck", error.span, error.message()))
            .collect::<Vec<_>>()
    })?;
    let core = psrs_core::lower_module(thir).map_err(|errors| {
        errors
            .into_iter()
            .map(|error| diagnostic("P6 Core lowering", error.span, error.message))
            .collect::<Vec<_>>()
    })?;
    Ok(core)
}

fn backend_diagnostics(errors: Vec<psrs_backend::BackendError>) -> Vec<Diagnostic> {
    errors
        .into_iter()
        .map(|error| diagnostic(error.pass, error.span, error.message))
        .collect()
}

fn diagnostic(stage: &'static str, span: TextRange, message: impl Into<String>) -> Diagnostic {
    Diagnostic {
        stage,
        span,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compiles_a_direct_call_with_integer_arithmetic_to_valid_wasm_and_wat() {
        let source = "module Main where\nadd x y = x + y\nmain = add 40 2\n";
        let artifact = compile_source("Main.purs", source).unwrap();
        assert_eq!(&artifact.wasm[..8], b"\0asm\x01\0\0\0");
        assert!(artifact.wat.contains("i32.add"));
        assert!(artifact.wat.contains("(export \"main\""));
    }

    #[test]
    fn compiles_if_expression_through_cfg_to_structured_wasm() {
        let source = "module Main where\nmain = if true then 9 else 2\n";
        let artifact = compile_source("Main.purs", source).unwrap();
        assert!(artifact.wat.contains("if (result i32)"));
        assert!(artifact.wasm.len() > 8);
    }

    #[test]
    fn lowers_top_level_scalar_references_to_direct_calls() {
        let source = "module Main where\nanswer = 40\nmain = answer + 2\n";
        let artifact = compile_source("Main.purs", source).unwrap();
        assert!(artifact.wat.contains("call 0"));
        assert!(artifact.wat.contains("i32.add"));
    }

    #[test]
    fn exposes_readable_core_and_backend_ir_dumps() {
        let source = "module Main where\nmain = 42\n";
        let compilation = compile_source_with_dumps("Main.purs", source).unwrap();
        for stage in ["core", "cc", "mir"] {
            assert!(
                compilation
                    .dumps
                    .get(stage)
                    .is_some_and(|dump| !dump.is_empty())
            );
        }
        assert!(compilation.dumps.get("wasm").is_none());
    }

    #[test]
    fn structures_wasm_ir_with_an_explicit_if_region() {
        let source = "module Main where\nmain = if true then 9 else 2\n";
        let core = lower_source_to_core("Main.purs", source).unwrap();
        let stages = psrs_backend::compile_with_stages(core).unwrap();
        assert!(
            stages.wasm.functions.iter().any(|function| function
                .body
                .iter()
                .any(|op| matches!(op, psrs_backend::wasm::Op::If { .. }))),
            "expected a structured if region in the Wasm IR"
        );
    }

    #[test]
    fn reports_type_errors_with_source_ranges() {
        let source = "module Main where\nmain = if 1 then 2 else 3\n";
        let errors = compile_source("Main.purs", source).unwrap_err();
        assert!(errors.iter().any(|error| error.stage == "P5 typecheck"));
        let integer_offset = source.find("1 then").unwrap() as u32;
        assert!(
            errors
                .iter()
                .any(|error| error.span == TextRange::new(integer_offset, integer_offset + 1))
        );
    }
}

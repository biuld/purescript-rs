//! Places the linked runtime's private stack above its static data.
use std::{env, fs};
use wasm_encoder::{ConstExpr, GlobalSection, Module, reencode::Reencode};
use wasmparser::{GlobalSectionReader, Operator, Parser};

struct Reservation;

impl Reencode for Reservation {
    type Error = String;

    fn parse_global_section(
        &mut self,
        globals: &mut GlobalSection,
        section: GlobalSectionReader<'_>,
    ) -> Result<(), wasm_encoder::reencode::Error<Self::Error>> {
        let values = section.into_iter().collect::<Result<Vec<_>, _>>()?;
        if values.len() != 2 || !values[0].ty.mutable || values[1].ty.mutable {
            return Err(wasm_encoder::reencode::Error::UserError(
                "runtime must define exactly the stack pointer and heap boundary".into(),
            ));
        }
        for (index, global) in values.into_iter().enumerate() {
            let mut ops = global.init_expr.get_operators_reader();
            let expected = if index == 0 { 65536 } else { 131072 };
            if global.ty.content_type != wasmparser::ValType::I32
                || !matches!(ops.read()?, Operator::I32Const { value } if value >= 65536 && value <= expected)
                || !matches!(ops.read()?, Operator::End)
            {
                return Err(wasm_encoder::reencode::Error::UserError(
                    "runtime linker layout does not match the reservation recipe".into(),
                ));
            }
            globals.global(
                self.global_type(global.ty)?,
                &ConstExpr::i32_const(psrs_runtime::HEAP_START as i32),
            );
        }
        Ok(())
    }

    fn memory_type(
        &mut self,
        memory: wasmparser::MemoryType,
    ) -> Result<wasm_encoder::MemoryType, wasm_encoder::reencode::Error<Self::Error>> {
        let mut memory = wasm_encoder::reencode::utils::memory_type(self, memory);
        memory.minimum = u64::from(psrs_runtime::HEAP_START / 65536);
        Ok(memory)
    }
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = env::args().collect();
    if args.len() != 3 {
        return Err("usage: package <linked.wasm> <reserved.wasm>".into());
    }
    let bytes = fs::read(&args[1])?;
    wasmparser::Validator::new().validate_all(&bytes)?;
    let mut module = Module::new();
    Reservation
        .parse_core_module(&mut module, Parser::new(0), &bytes)
        .map_err(|error| std::io::Error::other(error.to_string()))?;
    let output = module.finish();
    wasmparser::Validator::new().validate_all(&output)?;
    fs::write(&args[2], output)?;
    Ok(())
}

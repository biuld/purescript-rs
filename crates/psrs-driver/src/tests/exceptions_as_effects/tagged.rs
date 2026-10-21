//! Inject a distinct native tag at the leaf boundary of a real Except program.
//! This observation changes neither the library interpreter nor machine helpers.
use super::*;
use psrs_backend::wasm::{CatchClause, FuncType, Op, TagDef, TagIndex, TypeIndex};
use wasm_encoder::{Encode, Instruction as I, ValType};

#[test]
fn unrelated_native_tag_and_payload_cross_except_to_the_outer_native_observer() {
    if wasmtime_available().is_none() {
        return;
    }
    let source = r#"module Main where
import Prelude
import PSRS.Control as C
import Data.Either (Either(..))
import PSRS.Control.Except as E
import PSRS.Control.Row as R
force :: Int -> Int
force n = if n == 0 then 0 else 1 + force (n - 1)
main :: Int
main = R.runPure (E.catch (\(_ :: String) -> C.pure 0)
  (E.fromResult (\_ -> Right (force 100000))))
"#;
    let mut stages = crate::compile_main_stages(source).expect("real Except source verifies");
    let original = psrs_backend::wasm::encode_module(&stages.wasm).expect("original program core");
    let module = &mut stages.wasm;
    let tag = TagIndex(module.tags.len() as u32);
    let type_index = TypeIndex(module.defined_type_count() + module.types.len() as u32);
    module.types.push(FuncType {
        parameters: vec![ValType::I32],
        results: vec![],
    });
    module.tags.push(TagDef {
        type_index,
        span: module.span,
    });
    let leaf = module
        .functions
        .iter_mut()
        .find(|function| function.name == "force")
        .expect("the recursive fallible leaf survives ordinary optimization");
    leaf.body = vec![Op::Leaf(I::I32Const(42)), Op::Leaf(I::Throw(tag.0))];
    let main = module
        .functions
        .iter_mut()
        .find(|function| Some(function.symbol) == stages.mir.entry)
        .expect("source main");
    let mut protected = std::mem::take(&mut main.body);
    protected.push(Op::Leaf(I::Return));
    main.body = vec![Op::Block {
        body: vec![
            Op::Try {
                clauses: vec![CatchClause::Catch { tag, label: 0 }],
                body: protected,
                span: main.span,
            },
            Op::Leaf(I::Unreachable),
        ],
        result: Some(ValType::I32),
        span: main.span,
    }];
    let core = psrs_backend::wasm::encode_module(module).expect("observed program encodes");
    let bytes = replace_core(&stages.artifact.wasm, &original, &core);
    wasmparser::Validator::new_with_features(wasmparser::WasmFeatures::all())
        .validate_all(&bytes)
        .expect("observed component validates");
    let output = run_artifact_with_wasmtime(&bytes);
    assert_eq!(output.status.code(), Some(42), "{output:?}");
    assert!(
        output.stdout.is_empty() && output.stderr.is_empty(),
        "{output:?}"
    );
}

pub(in crate::tests) fn replace_core(component: &[u8], original: &[u8], core: &[u8]) -> Vec<u8> {
    assert_eq!(&component[..8], b"\0asm\x0d\0\x01\0");
    let mut result = component[..8].to_vec();
    let mut offset = 8;
    let mut replaced = 0;
    while offset < component.len() {
        let start = offset;
        let id = component[offset];
        offset += 1;
        let mut length = 0usize;
        let mut shift = 0;
        loop {
            let byte = component[offset];
            offset += 1;
            length |= usize::from(byte & 127) << shift;
            if byte & 128 == 0 {
                break;
            }
            shift += 7;
        }
        let end = offset + length;
        if id == 1 && &component[offset..end] == original {
            replaced += 1;
            result.push(id);
            (core.len() as u32).encode(&mut result);
            result.extend_from_slice(core);
        } else {
            result.extend_from_slice(&component[start..end]);
        }
        offset = end;
    }
    assert_eq!(replaced, 1);
    result
}

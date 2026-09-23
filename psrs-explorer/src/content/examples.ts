import type { Example } from './types';

export const examples: Example[] = [
  {
    id: 'basic',
    name: 'Arithmetic function',
    source: 'module Main where\n\nadd x y = x + y\n\nmain = add 40 2\n',
    snapshots: [
      { stage: 'tokens', title: 'Tokens after layout', provenance: 'curated', note: 'The generated-token adapter will replace this compact teaching view.', text: 'Identifier(module) ProperName(Main) Keyword(where)\nIdentifier(add) Identifier(x) Identifier(y) Equals\nIdentifier(x) Operator(+) Identifier(y)\nLayoutSep Identifier(main) Equals Identifier(add) Integer(40) Integer(2)' },
      { stage: 'cst', title: 'Concrete syntax tree', provenance: 'curated', note: 'Concrete forms retain syntax and spans.', text: 'Module\n└─ ValueDeclaration add\n   ├─ parameters: x, y\n   └─ InfixExpression\n      ├─ Name x\n      ├─ Operator +\n      └─ Name y' },
      { stage: 'ast', title: 'Abstract syntax tree', provenance: 'curated', note: 'Parser-only infix detail has normalized to application.', text: 'Value add = Lambda x (Lambda y\n  (Apply (Apply (Name "+") (Name x)) (Name y)))' },
      { stage: 'hir', title: 'Resolved HIR', provenance: 'curated', note: 'Names become stable IDs; no types are attached yet.', text: 'Decl SymbolId(0) add\n  Lambda LocalId(0) x\n    Lambda LocalId(1) y\n      Apply Intrinsic(I32Add) [LocalId(0), LocalId(1)]' },
      { stage: 'thir', title: 'Typed high-level IR', provenance: 'curated', note: 'Types are visible but runtime choices are absent.', text: 'add : Int -> Int -> Int\n  Lambda x : Int\n    Lambda y : Int\n      I32Add(x, y) : Int' },
      { stage: 'core', title: 'Typed Core', provenance: 'curated', note: 'Surface syntax has been reduced to a small expression language.', text: 'Global add : Int -> Int -> Int =\n  Lambda x. Lambda y. Prim(I32Add, [x, y])' },
      { stage: 'cc', title: 'Closure-converted IR', provenance: 'curated', note: 'The teaching view makes evaluation order and direct calls explicit.', text: 'function add(x: Int, y: Int) -> Int\n  let v0 = direct I32Add(x, y)\n  return v0' },
      { stage: 'mir', title: 'MIR control-flow graph', provenance: 'curated', note: 'A real dump has detailed IDs, type declarations, and spans.', text: 'block0(x: i32, y: i32):\n  v0:i32 = i32.add x, y\n  return v0' },
      { stage: 'wasm', title: 'Structured Wasm', provenance: 'curated', note: 'This is a target encoding, not a long-lived IR.', text: '(func $add (param i32 i32) (result i32)\n  local.get 0\n  local.get 1\n  i32.add)' },
    ],
  },
  {
    id: 'resolved',
    name: 'Local resolution',
    source: 'module Main where\n\nanswer = let value = 42\n         in value\n',
    snapshots: [
      { stage: 'cst', title: 'Concrete syntax tree', provenance: 'curated', note: 'The layout-sensitive let block remains concrete here.', text: 'LetExpression\n├─ binding value = IntegerLiteral 42\n└─ body Name value' },
      { stage: 'hir', title: 'Resolved HIR', provenance: 'curated', note: 'Both occurrences are connected through LocalId(0).', text: 'Decl SymbolId(0) answer\n  Let LocalId(0) value = Int(42)\n  In Local(LocalId(0))' },
      { stage: 'core', title: 'Typed Core', provenance: 'curated', note: 'The local name is already semantic, not textual.', text: 'Global answer : Int = Let LocalId(0) = Int(42) in LocalId(0)' },
    ],
  },
  {
    id: 'hello',
    name: 'WASI output',
    source: 'module Main where\n\nmain = log "hello world"\n',
    snapshots: [
      { stage: 'hir', title: 'Resolved HIR', provenance: 'curated', note: 'The runtime function is resolved as an external symbol.', text: 'Decl SymbolId(0) main\n  Apply ExternalSymbol(log) [String("hello world")]' },
      { stage: 'core', title: 'Typed Core', provenance: 'curated', note: 'The capability call is explicit at the Core boundary.', text: 'Global main : Unit = ExternalCall(log, String("hello world"))' },
      { stage: 'wasm', title: 'Structured Wasm', provenance: 'curated', note: 'Final linking brings in only the required WASI interfaces.', text: 'import wasi:cli/stdout\nimport wasi:io/streams\n...\ncall $blocking_write_and_flush' },
    ],
  },
];

export const exampleById = Object.fromEntries(examples.map((example) => [example.id, example]));

// Generate source-signature and behavior fixtures from pinned official FFI.
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';

const [prelude, integers, output] = process.argv.slice(2).map(value => resolve(value));
if (!prelude || !integers || !output) throw Error('usage: node stdlib-scalar-oracle.mjs PRELUDE INTEGERS OUTPUT');
const repository = resolve(import.meta.dirname, '../../..');
const inventory = JSON.parse(await readFile(join(repository,
  'docs/implementation/stdlib/vendor-restoration-2026-10-06/inventory.json'), 'utf8'));
const roots = { 'purescript-prelude': prelude, 'purescript-integers': integers };
for (const [packageName, root] of Object.entries(roots)) {
  const pin = inventory.packages.find(value => value.name === packageName);
  if (execFileSync('git', ['-C', root, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() !== pin.commit)
    throw Error(`unexpected upstream revision: ${packageName}`);
  if (execFileSync('git', ['-C', root, 'status', '--porcelain'], { encoding: 'utf8' }).trim())
    throw Error(`dirty upstream: ${packageName}`);
}
const specifications = [
  ['Data/Int.purs', 'toNumber', [[42], [-2147483648], [2147483647]]],
  ['Data/Int/Bits.purs', 'and', [[63, 42], [-1, 42]]],
  ['Data/Int/Bits.purs', 'or', [[32, 10], [-1, 0]]],
  ['Data/Int/Bits.purs', 'xor', [[63, 21], [-1, 0]]],
  ['Data/Int/Bits.purs', 'shl', [[21, 1], [21, 33], [1, -1]]],
  ['Data/Int/Bits.purs', 'shr', [[-84, 1], [-84, 33], [1, -1]]],
  ['Data/Int/Bits.purs', 'zshr', [[-1, 1], [-1, 0], [-1, 32]]],
  ['Data/Int/Bits.purs', 'complement', [[-43], [-2147483648]]],
  ['Data/Eq.purs', 'eqBooleanImpl', [[true, true], [true, false]]],
  ['Data/Eq.purs', 'eqIntImpl', [[42, 42], [-2147483648, 2147483647]]],
  ['Data/Eq.purs', 'eqNumberImpl', [[0, -0], [NaN, NaN], [Infinity, Infinity]]],
  ['Data/Eq.purs', 'eqCharImpl', [['λ', 'λ'], ['😀', '😀'], ['λ', '😀']]],
  ['Data/Ring.purs', 'intSub', [[44, 2], [-2147483648, 1]]],
  ['Data/Ring.purs', 'numSub', [[44, 2], [-0, 0]]],
  ['Data/Semiring.purs', 'intAdd', [[40, 2], [2147483647, 1]]],
  ['Data/Semiring.purs', 'numAdd', [[40, 2], [Infinity, -Infinity]]],
  ['Data/Semiring.purs', 'numMul', [[21, 2], [-0, 2]]],
  ['Data/HeytingAlgebra.purs', 'boolConj', [[true, true], [true, false]]],
  ['Data/HeytingAlgebra.purs', 'boolDisj', [[false, true], [false, false]]],
  ['Data/HeytingAlgebra.purs', 'boolNot', [[true], [false]]],
];
const hashes = new Map(), declarations = [], observations = [], checks = [];
function marker(value) {
  if (typeof value !== 'number') return value;
  if (Number.isNaN(value)) return 'NaN';
  if (Object.is(value, -0)) return '-0';
  if (!Number.isFinite(value)) return String(value);
  return value;
}
function literal(value, type) {
  if (typeof value === 'boolean') return String(value);
  if (typeof value === 'string') return `'${value}'`;
  if (Number.isNaN(value)) return '(numberDiv 0.0 0.0)';
  if (!Number.isFinite(value)) return `(numberDiv ${value < 0 ? '(numberNeg 1.0)' : '1.0'} 0.0)`;
  const negative = value < 0 || Object.is(value, -0);
  const magnitude = String(Math.abs(value));
  if (type === 'Number') {
    const number = magnitude.includes('.') ? magnitude : magnitude + '.0';
    return negative ? `(numberNeg ${number})` : number;
  }
  if (value === -2147483648) return '(intSub (intNeg 2147483647) 1)';
  return negative ? `(intNeg ${magnitude})` : magnitude;
}
for (const [file, name, cases] of specifications) {
  const metadata = inventory.modules.find(value => value.path === file);
  const upstream = join(roots[metadata.package], 'src', file.replace('.purs', '.js'));
  const js = await readFile(upstream);
  hashes.set(upstream, createHash('sha256').update(js).digest('hex'));
  const functions = await import(pathToFileURL(upstream));
  const vendor = await readFile(join(repository, 'stdlib/lib', file), 'utf8');
  const declaration = vendor.split('\n').find(line => line.startsWith('foreign import "psrs:intrinsic#') && line.includes(`" ${name} ::`));
  if (!declaration) throw Error(`missing explicit target binding: ${file}.${name}`);
  declarations.push(declaration);
  const types = declaration.split('::')[1].trim().split(' -> ');
  const resultType = types.at(-1);
  for (const args of cases) {
    let expected = functions[name];
    for (const argument of args) expected = expected(argument);
    // Wasm Int is signed i32; record the raw JS observation separately.
    const target = resultType === 'Int' ? expected | 0 : expected;
    const call = `(Golden.${name} ${args.map((value, i) => literal(value, types[i])).join(' ')})`;
    let condition;
    if (resultType === 'Number' && Number.isNaN(target)) condition = `(booleanNot (numberEq ${call} ${call}))`;
    else if (resultType === 'Number' && Object.is(target, -0))
      condition = `(numberEq (numberDiv 1.0 ${call}) (numberDiv (numberNeg 1.0) 0.0))`;
    else condition = `(${resultType === 'Int' ? 'intEq' : resultType === 'Boolean' ? 'booleanEq' : 'numberEq'} ${call} ${literal(target, resultType)})`;
    observations.push({ module: file, name, arguments: args.map(marker), official_result: marker(expected), target_result: marker(target), representation_difference: !Object.is(expected, target) });
    checks.push(condition);
  }
}
await mkdir(output, { recursive: true });
await writeFile(join(output, 'Golden.purs'), 'module Golden where\n' + declarations.join('\n') + '\n');
function conjunction(values) {
  if (values.length === 1) return values[0];
  const middle = Math.floor(values.length / 2);
  return `(booleanAnd ${conjunction(values.slice(0, middle))} ${conjunction(values.slice(middle))})`;
}
await writeFile(join(output, 'Main.purs'), 'module Main where\nimport Golden as Golden\nmain = if ' + conjunction(checks) + ' then 42 else 1\n');
await writeFile(join(output, 'observations.json'), JSON.stringify({ node: process.version, inputs: [...hashes].map(([path, sha256]) => ({ path, sha256 })), observations }, null, 2) + '\n');
console.log(`${declarations.length} bindings, ${checks.length} cases`);

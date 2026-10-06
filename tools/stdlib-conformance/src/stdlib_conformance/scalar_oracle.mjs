// Generate source-signature and behavior fixtures from pinned official FFI.
import { readFile, writeFile, mkdir } from 'node:fs/promises';
import { execFileSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { resolve, join } from 'node:path';
import { pathToFileURL } from 'node:url';
import { parseArgs } from 'node:util';

const { values } = parseArgs({ options: Object.fromEntries(
  ['vendor', 'inventory', 'cases', 'out', 'upstream'].map(name => [name, { type: 'string', multiple: name === 'upstream' }])) });
for (const name of ['vendor', 'inventory', 'cases', 'out'])
  if (!values[name]) throw Error(`missing --${name}`);
const output = resolve(values.out);
const inventory = JSON.parse(await readFile(values.inventory, 'utf8'));
const caseManifest = JSON.parse(await readFile(values.cases, 'utf8'));
if (caseManifest.schema_version !== 1 || caseManifest.target_int_representation !== 'signed-i32')
  throw Error('unsupported scalar conformance contract');
const roots = Object.fromEntries((values.upstream ?? []).map(value => {
  const split = value.indexOf('=');
  if (split < 1) throw Error('upstream must be PACKAGE=CHECKOUT');
  return [value.slice(0, split), resolve(value.slice(split + 1))];
}));
for (const [packageName, root] of Object.entries(roots)) {
  const pin = inventory.packages.find(value => value.name === packageName);
  if (!pin) throw Error(`unknown upstream package: ${packageName}`);
  if (execFileSync('git', ['-C', root, 'rev-parse', 'HEAD'], { encoding: 'utf8' }).trim() !== pin.commit)
    throw Error(`unexpected upstream revision: ${packageName}`);
  if (execFileSync('git', ['-C', root, 'status', '--porcelain'], { encoding: 'utf8' }).trim())
    throw Error(`dirty upstream: ${packageName}`);
}
function decode(value) {
  if (typeof value !== 'object' || value === null) return value;
  const special = { NaN, Infinity, '-Infinity': -Infinity, '-0': -0 };
  if (Object.keys(value).length !== 1 || !Object.hasOwn(special, value.number))
    throw Error('invalid special numeric input');
  return special[value.number];
}
const specifications = caseManifest.bindings.map(row => [row.module, row.name, row.arguments.map(args => args.map(decode))]);
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
  if (!metadata || !roots[metadata.package]) throw Error(`missing upstream root for ${file}`);
  const upstream = join(roots[metadata.package], 'src', file.replace('.purs', '.js'));
  const js = await readFile(upstream);
  hashes.set(upstream, createHash('sha256').update(js).digest('hex'));
  const functions = await import(pathToFileURL(upstream));
  const vendor = await readFile(join(values.vendor, file), 'utf8');
  const declaration = vendor.split('\n').find(line => line.startsWith('foreign import "psrs:intrinsic#') && line.includes(`" ${name} ::`));
  if (!declaration) throw Error(`missing explicit target binding: ${file}.${name}`);
  declarations.push(declaration);
  const types = declaration.split('::')[1].trim().split(' -> ');
  if (types.some(type => !['Int', 'Number', 'Boolean', 'Char'].includes(type)))
    throw Error(`unsupported scalar signature: ${declaration}`);
  const resultType = types.at(-1);
  for (const args of cases) {
    if (args.length !== types.length - 1) throw Error(`argument arity mismatch: ${file}.${name}`);
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
  if (values.length === 0) throw Error("no scalar observations were generated");
  if (values.length === 1) return values[0];
  const middle = Math.floor(values.length / 2);
  return `(booleanAnd ${conjunction(values.slice(0, middle))} ${conjunction(values.slice(middle))})`;
}
await writeFile(join(output, 'Main.purs'), 'module Main where\nimport Golden as Golden\nmain = if ' + conjunction(checks) + ' then 42 else 1\n');
await writeFile(join(output, 'observations.json'), JSON.stringify({ node: process.version, inputs: [...hashes].map(([path, sha256]) => ({ path, sha256 })), observations }, null, 2) + '\n');
console.log(`${declarations.length} bindings, ${checks.length} cases`);

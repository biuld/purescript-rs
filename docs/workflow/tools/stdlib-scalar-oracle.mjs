// Compatibility entrypoint; the implementation accepts independent package paths.
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
const [prelude, integers, out] = process.argv.slice(2);
if (!prelude || !integers || !out) throw Error('usage: node stdlib-scalar-oracle.mjs PRELUDE INTEGERS OUTPUT');
const compilerRoot = resolve(import.meta.dirname, '../../..');
const lock = JSON.parse(readFileSync(resolve(compilerRoot, 'stdlib.lock.json'), 'utf8'));
const root = process.env.PSRS_STDLIB_ROOT ?? resolve(compilerRoot, lock.path);
const script = resolve(compilerRoot, 'tools/stdlib-conformance/src/stdlib_conformance/scalar_oracle.mjs');
const result = spawnSync(process.execPath, [script, '--vendor', resolve(root, 'lib'),
  '--inventory', resolve(root, 'upstream-lock.json'), '--cases', resolve(root, 'conformance/scalars.json'),
  '--upstream', `purescript-prelude=${resolve(prelude)}`, '--upstream', `purescript-integers=${resolve(integers)}`,
  '--out', resolve(out)], { stdio: 'inherit' });
if (result.error) throw result.error;
process.exit(result.status ?? 1);

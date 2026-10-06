"""Run compiler and mandatory Wasmtime through public executable boundaries."""

import argparse
import base64
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import time

from .package import fingerprint


def digest(data):
    return hashlib.sha256(data).hexdigest()


def output(data):
    return {'base64': base64.b64encode(data).decode('ascii'), 'sha256': digest(data)}


def invoke(argv, timeout, env=None):
    start = time.monotonic()
    try:
        result = subprocess.run(argv, capture_output=True, timeout=timeout, env=env)
        record = {'status': 'completed', 'exit_code': result.returncode,
                  'stdout': output(result.stdout), 'stderr': output(result.stderr)}
    except subprocess.TimeoutExpired as error:
        record = {'status': 'timeout', 'stdout': output(error.stdout or b''),
                  'stderr': output(error.stderr or b'')}
    except OSError as error:
        record = {'status': 'launch_failed', 'error': str(error)}
    return {**record, 'argv': argv, 'elapsed_seconds': time.monotonic() - start}


def executable(value):
    resolved = shutil.which(value)
    if resolved is None:
        raise ValueError(f'executable unavailable: {value}')
    return str(Path(resolved).resolve())


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--compiler', required=True)
    parser.add_argument('--stdlib-root', required=True, type=Path)
    parser.add_argument('--wasmtime', default='wasmtime')
    parser.add_argument('--input', action='append', required=True, type=Path)
    parser.add_argument('--out', required=True, type=Path)
    parser.add_argument('--timeout', default=180, type=float)
    parser.add_argument('--expected-exit', default=42, type=int)
    parser.add_argument('--expected-stdout', type=Path)
    parser.add_argument('--expected-stderr', type=Path)
    args = parser.parse_args(argv)
    args.out.mkdir(parents=True, exist_ok=True)
    report_path = args.out / 'run.json'
    report = {'schema_version': 1, 'accepted': False}
    try:
        if args.timeout <= 0:
            raise ValueError('timeout must be positive')
        compiler = executable(args.compiler)
        runtime = executable(args.wasmtime)
        root = args.stdlib_root.resolve(strict=True)
        package = json.loads((root / 'manifest.json').read_text())
        if package.get('schema_version') != 1 or package.get('name') != 'psrs-stdlib':
            raise ValueError('unsupported stdlib package manifest')
        before = fingerprint(root)
        report['stdlib'] = {'root': str(root), 'source_fingerprint': before}
        report['compiler'] = {'path': compiler, 'sha256': digest(Path(compiler).read_bytes())}
        report['runtime'] = {'path': runtime, 'sha256': digest(Path(runtime).read_bytes()),
                             'version': invoke([runtime, '--version'], args.timeout)}
        version = report['runtime']['version']
        if version['status'] != 'completed' or version['exit_code'] != 0:
            raise ValueError('runtime version query failed')
        inputs = [path.resolve(strict=True) for path in args.input]
        report['inputs'] = [{'path': str(path), 'sha256': digest(path.read_bytes())} for path in inputs]
        expected = {'exit_code': args.expected_exit,
                    'stdout': output(args.expected_stdout.read_bytes() if args.expected_stdout else b''),
                    'stderr': output(args.expected_stderr.read_bytes() if args.expected_stderr else b'')}
        report['expected'] = expected
        wasm = (args.out / 'program.wasm').resolve()
        # Stale output can never serve as evidence for a failed build.
        wasm.unlink(missing_ok=True)
        env = {**os.environ, 'PSRS_STDLIB_ROOT': str(root)}
        build = invoke([compiler, 'build', *map(str, inputs), '-o', str(wasm)], args.timeout, env)
        report['compilation'] = build
        if build['status'] == 'completed' and build['exit_code'] == 0:
            report['wasm'] = {'path': str(wasm), 'sha256': digest(wasm.read_bytes())}
            run = invoke([runtime, str(wasm)], args.timeout)
            report['execution'] = run
            report['accepted'] = (run['status'] == 'completed' and
                                  all(run[key] == value for key, value in expected.items()))
        after = fingerprint(root)
        report['stdlib']['unchanged_during_run'] = before == after
        report['inputs_unchanged_during_run'] = all(
            digest(Path(row['path']).read_bytes()) == row['sha256'] for row in report['inputs'])
        if before != after or not report['inputs_unchanged_during_run']:
            report['accepted'] = False
    except (OSError, ValueError, KeyError) as error:
        report['error'] = str(error)
        report['accepted'] = False
    report_path.write_text(json.dumps(report, indent=2) + '\n')
    print(f'{report_path}: {"accepted" if report["accepted"] else "failed"}')
    return 0 if report['accepted'] else 1

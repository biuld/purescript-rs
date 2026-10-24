#!/usr/bin/env python3
"""Run a frozen release suite with bounded parallelism and per-root evidence."""

import argparse
import collections
import concurrent.futures
import hashlib
import json
import os
from pathlib import Path
import re
import resource
import shutil
import signal
import subprocess
import tempfile
import time


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def save(path, value):
    path.write_text(json.dumps(value, indent=2) + "\n")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("suite", type=Path, help="frozen release suite executable")
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[4],
                        help="checkout used to build the frozen suite")
    parser.add_argument("--out", type=Path, required=True, help="new evidence directory")
    parser.add_argument("--workers", type=int, default=4)
    parser.add_argument("--case", action="append", default=[], help="exact root filename")
    parser.add_argument("--baseline", type=Path, help="previous results.json")
    args = parser.parse_args()
    if args.workers < 1:
        parser.error("--workers must be positive")
    repo = args.repo.resolve(strict=True)
    corpus = repo / "tests/upstream/passing"
    suite = args.suite.resolve(strict=True)
    engine = shutil.which("wasmtime")
    if not engine:
        parser.error("Wasmtime is required")
    roots = sorted(path.name for path in corpus.glob("*.purs"))
    selected = set(args.case) if args.case else set(roots)
    if not selected or not selected <= set(roots):
        parser.error("selection must name existing passing-program roots")
    if args.out.resolve().is_relative_to(repo):
        parser.error("evidence directory must be outside the checkout")
    args.out.mkdir(parents=True, exist_ok=False)
    out = args.out.resolve()
    before_hash = digest(suite)
    runner_hash = digest(Path(__file__))
    shutil.copyfile(Path(__file__), out / "run_runtime.py")
    corpus_snapshot = {str(p.relative_to(corpus)): digest(p) for p in sorted(corpus.rglob("*.purs"))}
    source_files = sorted(repo.glob("crates/**/*.rs"))
    source_files += sorted(repo.glob("**/Cargo.toml"))
    source_files += [repo / "Cargo.lock", repo / "stdlib.lock.json"]
    snapshot = {str(path.relative_to(repo)): digest(path) for path in source_files}
    for name in snapshot:
        target = out / "build-source" / name
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(repo / name, target)
    save(out / "source-files.json", snapshot)
    save(out / "measurement.json", {
        "compiler_head": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=repo, text=True).strip(),
        "compiler_checkout": str(repo),
        "runtime_harness": str(suite), "sha256": before_hash,
        "engine": subprocess.check_output([engine, "--version"], text=True).strip(),
        "workers": args.workers, "guest_seconds": 10, "stack_bytes": 524288,
        "normal_compile_outer_seconds": 110, "big_function_outer_seconds": 90,
        "big_function_address_space_bytes": 8 * 1024**3,
        "roots": roots, "selected": sorted(selected),
        "stdlib_lock": json.loads((repo / "stdlib.lock.json").read_text()),
        "source_snapshot_sha256": digest(out / "source-files.json"),
        "runner_sha256": runner_hash,
        "corpus_sha256": corpus_snapshot,
    })
    # The compiler's address-space bound must not limit the guest engine.
    wrapper_dir = out / "engine-wrapper"
    wrapper_dir.mkdir()
    wrapper = wrapper_dir / "wasmtime"
    wrapper.write_text("#!/usr/bin/env python3\nimport os,resource,sys\n"
                       "resource.setrlimit(resource.RLIMIT_AS,(resource.RLIM_INFINITY,resource.RLIM_INFINITY))\n"
                       f"os.execv({engine!r},[{engine!r}]+sys.argv[1:])\n")
    wrapper.chmod(0o755)

    def bounded():
        resource.setrlimit(resource.RLIMIT_CORE, (0, 0))
        resource.setrlimit(resource.RLIMIT_AS, (8 * 1024**3, resource.RLIM_INFINITY))

    def run(case):
        big = case == "BigFunction.purs"
        seconds = 90 if big else 110
        env = os.environ.copy()
        env.update(PSRS_REQUIRE_WASMTIME="1", PSRS_ORACLE="annotations", PSRS_SUITE_FILTER="passing/" + case)
        env.pop("PURESCRIPT_REPO", None)
        env.pop("PSRS_STDLIB_ROOT", None)
        env.pop("PSRS_SUITE_LIMIT", None)
        if big:
            env["PATH"] = str(wrapper_dir) + os.pathsep + env["PATH"]
        started = time.monotonic()
        with tempfile.TemporaryDirectory(prefix="psrs-runtime-", dir=out) as temp:
            env["TMPDIR"] = temp
            proc = subprocess.Popen([str(suite), "runtime::l6_runtime_scoreboard", "--exact", "--ignored", "--nocapture"],
                                    cwd=repo, env=env, stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                                    start_new_session=True, preexec_fn=bounded if big else None)
            try:
                data, _ = proc.communicate(timeout=seconds)
                status = proc.returncode
            except subprocess.TimeoutExpired:
                os.killpg(proc.pid, signal.SIGKILL)
                data, _ = proc.communicate()
                status = "outer-timeout"
        log = data.decode(errors="replace")
        (out / (case + ".log")).write_text(log)
        row = {"case": case, "status": status, "seconds": round(time.monotonic() - started, 3),
               "requested_outer_seconds": seconds}
        match = re.search(r"^runtime agreement: (\d+)/(\d+), (\d+) excluded", log, re.M)
        if match and status == 0:
            row.update(zip(("agree", "considered", "excluded"), map(int, match.groups())))
        elif big and ("memory allocation" in log or "out of memory" in log.lower()):
            row.update(agree=0, considered=1, excluded=0, incomplete_compilation=True,
                       resource_failure_observed=True, address_space_limit_bytes=8 * 1024**3)
        else:
            raise RuntimeError(f"{case}: operational failure {status}; see retained log")
        if row["agree"]:
            histogram = log.split("exit codes:\n", 1)[1].split("failing cases", 1)[0]
            row["exit_codes"] = {code: int(n) for code, n in re.findall(r"^  (\w+): (\d+)$", histogram, re.M)}
        row["stages"] = {stage: int(n) for n, stage in re.findall(r"^  (\d+) x (P\d+ [^\n]+)$", log, re.M)}
        row["observations"] = re.findall(r"^  passing/[^\n]+$", log, re.M)
        return row

    rows = []
    started = time.monotonic()

    def retain(row):
        rows.append(row)
        save(out / "results.json", sorted(rows, key=lambda r: r["case"]))
        print(f"BOARD {len(rows)}/{len(selected)} {row['case']} {row['agree']}/{row['considered']}", flush=True)

    cases = iter(case for case in sorted(selected) if case != "BigFunction.purs")
    pool = concurrent.futures.ThreadPoolExecutor(max_workers=args.workers)
    pending = set()
    try:
        for _ in range(args.workers):
            case = next(cases, None)
            if case is not None:
                pending.add(pool.submit(run, case))
        while pending:
            done, pending = concurrent.futures.wait(pending, return_when=concurrent.futures.FIRST_COMPLETED)
            # Observe every completed result before scheduling another case.
            completed = [future.result() for future in done]
            for row in completed:
                retain(row)
                case = next(cases, None)
                if case is not None:
                    pending.add(pool.submit(run, case))
    finally:
        for future in pending:
            future.cancel()
        pool.shutdown(wait=True, cancel_futures=True)
    # Run this compiler-only resource experiment after worker threads exit.
    if "BigFunction.purs" in selected:
        retain(run("BigFunction.purs"))
    if digest(suite) != before_hash or any(digest(repo / name) != expected for name, expected in snapshot.items()):
        raise RuntimeError("compiler inputs changed during measurement")
    if digest(Path(__file__)) != runner_hash or any(digest(corpus / name) != expected for name, expected in corpus_snapshot.items()):
        raise RuntimeError("runner or corpus inputs changed during measurement")
    agree = {r["case"] for r in rows if r["agree"]}
    summary = {"roots": len(rows), "full_inventory": selected == set(roots),
               "seconds": round(time.monotonic() - started, 3), "agree": len(agree),
               "considered": sum(r["considered"] for r in rows), "excluded": sum(r["excluded"] for r in rows),
               "failures": sorted(r["case"] for r in rows if r["considered"] and not r["agree"]),
               "all_agreeing_exits_zero": all(r.get("exit_codes") == {"0": 1} for r in rows if r["agree"]),
               "incomplete_compilations": [r for r in rows if r.get("incomplete_compilation")],
               "stages": dict(sum((collections.Counter(r["stages"]) for r in rows), collections.Counter()))}
    if args.baseline:
        previous = {r["case"] for r in json.loads(args.baseline.read_text()) if r["agree"]} & selected
        summary.update(recovered=sorted(agree - previous), lost_agreement=sorted(previous - agree))
    save(out / "summary.json", summary)
    print("BOARD COMPLETE", json.dumps(summary), flush=True)


if __name__ == "__main__":
    main()

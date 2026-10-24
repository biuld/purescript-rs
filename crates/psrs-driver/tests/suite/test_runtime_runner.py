"""Exercise the runtime runner's evidence and operational-failure contracts."""

import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


RUNNER = Path(__file__).with_name("run_runtime.py")


class RuntimeRunnerTests(unittest.TestCase):
    def fixture(self, root, count, fail=False):
        repo = root / "compiler"
        corpus = repo / "tests/upstream/passing"
        corpus.mkdir(parents=True)
        for index in range(count):
            (corpus / f"Case{index:02}.purs").write_text("module Main where\nmain = 0\n")
        (repo / "Cargo.lock").write_text("fixture\n")
        (repo / "stdlib.lock.json").write_text("{}\n")
        subprocess.run(["git", "init", "-q", str(repo)], check=True, capture_output=True)
        subprocess.run(["git", "-C", str(repo), "-c", "user.name=Fixture",
                        "-c", "user.email=fixture@example.invalid", "-c", "commit.gpgsign=false",
                        "commit", "--allow-empty", "--no-verify", "-qm", "fixture"],
                       check=True, capture_output=True)
        binary_dir = root / "bin"
        binary_dir.mkdir()
        engine = binary_dir / "wasmtime"
        engine.write_text("#!/usr/bin/env python3\nprint('wasmtime fixture')\n")
        engine.chmod(0o755)
        suite = root / "suite"
        suite.write_text("#!/usr/bin/env python3\nimport os,sys,time\n"
                         "case=os.environ['PSRS_SUITE_FILTER'].split('/')[-1]\n"
                         "assert 'PURESCRIPT_REPO' not in os.environ\n"
                         "with open(os.environ['INVOCATIONS'],'a') as log: log.write(case+'\\n')\n"
                         + ("if case=='Case00.purs': sys.exit(7)\n" if fail else "")
                         + "time.sleep(0.1)\n"
                         "print('runtime agreement: 1/1, 0 excluded')\n"
                         "print('exit codes:\\n  0: 1\\nfailing cases (0):')\n"
                         "print('  passing/'+case+': stdout: Done')\n")
        suite.chmod(0o755)
        env = os.environ.copy()
        env["PATH"] = str(binary_dir) + os.pathsep + env["PATH"]
        env["INVOCATIONS"] = str(root / "invocations")
        env["PURESCRIPT_REPO"] = str(root / "wrong-corpus")
        return repo, suite, env

    def test_complete_inventory_has_retained_hashes_and_zero_exits(self):
        with tempfile.TemporaryDirectory(prefix="psrs-runner-test-") as temp:
            root = Path(temp)
            repo, suite, env = self.fixture(root, 4)
            out = root / "evidence"
            result = subprocess.run([sys.executable, str(RUNNER), str(suite), "--repo", str(repo),
                                     "--workers", "2", "--out", str(out)], env=env, capture_output=True)
            self.assertEqual(result.returncode, 0, result.stderr)
            summary = json.loads((out / "summary.json").read_text())
            self.assertEqual(summary["roots"], 4)
            self.assertEqual(summary["agree"], 4)
            self.assertTrue(summary["full_inventory"])
            self.assertTrue(summary["all_agreeing_exits_zero"])
            self.assertEqual((out / "run_runtime.py").read_bytes(), RUNNER.read_bytes())
            self.assertTrue((out / "build-source/stdlib.lock.json").exists())

    def test_operational_failure_stops_scheduling_and_never_publishes_a_summary(self):
        with tempfile.TemporaryDirectory(prefix="psrs-runner-test-") as temp:
            root = Path(temp)
            repo, suite, env = self.fixture(root, 32, fail=True)
            out = root / "evidence"
            result = subprocess.run([sys.executable, str(RUNNER), str(suite), "--repo", str(repo),
                                     "--workers", "4", "--out", str(out)], env=env, capture_output=True)
            self.assertNotEqual(result.returncode, 0)
            self.assertLessEqual(len((root / "invocations").read_text().splitlines()), 4)
            self.assertFalse((out / "summary.json").exists())
            self.assertTrue((out / "Case00.purs.log").exists())


if __name__ == "__main__":
    unittest.main()

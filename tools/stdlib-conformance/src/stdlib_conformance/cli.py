"""Standalone command boundary for source and runtime evidence."""

import argparse
from pathlib import Path
import subprocess

from . import audit


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=["audit", "scalar-oracle", "run"])
    parser.add_argument("arguments", nargs=argparse.REMAINDER)
    args = parser.parse_args(argv)
    remaining = args.arguments
    if args.command == "audit":
        audit.main(remaining)
        return 0
    if args.command == "scalar-oracle":
        script = Path(__file__).with_name("scalar_oracle.mjs")
        return subprocess.call(["node", str(script), *remaining])
    from .runner import main as run
    return run(remaining)

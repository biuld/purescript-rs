#!/usr/bin/env python3
"""Compatibility entry for the standalone stdlib conformance component."""
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[3] / "tools/stdlib-conformance/src"))
from stdlib_conformance.audit import main

if __name__ == "__main__":
    main()

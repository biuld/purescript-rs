#!/usr/bin/env python3
"""Compare vendored modules with locally available, tagged upstream checkouts.

This is an inventory, not an allowlist or a PureScript semantic verifier.
It never modifies library sources and never downloads missing dependencies.
"""

import argparse
import collections
import difflib
import hashlib
import json
from pathlib import Path
import re
import subprocess


def git(root, *args):
    return subprocess.check_output(
        ["git", "-C", str(root), *args], text=True, stderr=subprocess.PIPE
    ).strip()


def digest(data):
    return hashlib.sha256(data).hexdigest()


def append_diff(patch, original, vendored, before_path, after_path):
    differences = difflib.unified_diff(
        original.splitlines(keepends=True), vendored.splitlines(keepends=True),
        fromfile=before_path, tofile=after_path,
    )
    for line in differences:
        patch.append(line if line.endswith("\n") else line + "\n\\ No newline at end of file\n")


def foreign_declarations(text):
    lines = text.splitlines()
    declarations = []
    covered = set()
    for index, line in enumerate(lines):
        match = re.match(r"^foreign import (?!data\b)([\w']+)\b", line)
        if not match:
            continue
        end = index + 1
        while end < len(lines) and lines[end].startswith((" ", "\t")):
            end += 1
        covered.update(range(index, end))
        declarations.append({
            "name": match[1],
            "line": index + 1,
            "declaration": " ".join(part.strip() for part in lines[index:end]),
        })
    return declarations, covered


def self_recursions(text):
    result = []
    for number, line in enumerate(text.splitlines(), 1):
        match = re.fullmatch(
            r"([A-Za-z_][\w']*(?:\s+[A-Za-z_][\w']*)*)\s*=\s*(.*?)\s*",
            line,
        )
        if match and match[1].split() == match[2].split():
            result.append({"name": match[1].split()[0], "line": number, "equation": line})
    return result


def ordinary_removals(before, after, foreign_lines):
    """Report changed original code outside value FFI declaration spans.

    This deliberately reports eta expansion and signature/layout changes too.
    A reported removal requires review; it does not automatically prove a bug.
    """
    old, new = before.splitlines(), after.splitlines()
    result = []
    for kind, start, end, _, _ in difflib.SequenceMatcher(
        None, old, new, autojunk=False
    ).get_opcodes():
        if kind not in ("delete", "replace"):
            continue
        for index in range(start, end):
            line = old[index]
            if index not in foreign_lines and line.strip() and not line.lstrip().startswith("--"):
                result.append({"line": index + 1, "text": line})
    return result


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--vendor", type=Path, required=True)
    parser.add_argument("--upstream", type=Path, action="append", required=True,
                        help="A git checkout with src/, or a directory of those checkouts")
    parser.add_argument("--out", type=Path, required=True)
    parser.add_argument("--inventory", type=Path, help="Pinned upstream package inventory")
    args = parser.parse_args(argv)
    pins = json.loads(args.inventory.read_text())["packages"] if args.inventory else None
    args.out.mkdir(parents=True, exist_ok=True)
    roots = []
    for root in args.upstream:
        if (root / "src").is_dir():
            roots.append(root)
        else:
            roots.extend(child for child in sorted(root.iterdir()) if (child / "src").is_dir())

    packages, sources = [], {}
    for root in roots:
        if git(root, "status", "--porcelain"):
            raise SystemExit(f"upstream checkout is dirty: {root}")
        package = {
            "name": root.name,
            "checkout": str(root.resolve()),
            "commit": git(root, "rev-parse", "HEAD"),
            "tag": git(root, "describe", "--tags", "--exact-match"),
            "remote": git(root, "remote", "get-url", "origin"),
        }
        if pins is not None:
            pin = next((pin for pin in pins if pin["name"] == package["name"]), None)
            if pin is None or any(pin[key] != package[key] for key in ("commit", "tag", "remote")):
                raise SystemExit(f"upstream checkout differs from package pin: {root}")
        packages.append(package)
        for path in sorted((root / "src").rglob("*.purs")):
            relative = path.relative_to(root / "src").as_posix()
            if relative in sources:
                raise SystemExit(f"duplicate upstream source: {relative}")
            sources[relative] = (path, package)

    if pins is not None and {pin["name"] for pin in pins} != {package["name"] for package in packages}:
        raise SystemExit("upstream checkout set does not cover every pinned package")

    modules, patch = [], []
    for path in sorted(args.vendor.rglob("*.purs")):
        relative = path.relative_to(args.vendor).as_posix()
        data = path.read_bytes()
        text = data.decode("utf-8")
        row = {
            "path": relative,
            "vendored_sha256": digest(data),
            "vendored_lines": len(text.splitlines()),
            "self_recursions": self_recursions(text),
        }
        if relative not in sources:
            row["status"] = "platform_addition" if relative.startswith("WASI/") or relative == "WASI.purs" else "baseline_unavailable"
            modules.append(row)
            continue
        upstream, package = sources[relative]
        original_data = upstream.read_bytes()
        original = original_data.decode("utf-8")
        row.update({
            "package": package["name"],
            "tag": package["tag"],
            "commit": package["commit"],
            "upstream_sha256": digest(original_data),
            "upstream_url": package["remote"].removesuffix(".git") + "/blob/" + package["commit"] + "/src/" + relative,
            "status": "identical" if data == original_data else "newline_only" if original.splitlines() == text.splitlines() else "modified",
        })
        declarations, covered = foreign_declarations(original)
        names = {declaration["name"] for declaration in declarations}
        recursive_names = {recursion["name"] for recursion in row["self_recursions"]}
        retained_foreign_names = set(re.findall(
            r'^foreign import (?:"[^"\n]*"\s+)?(?!data\b)([\w\x27]+)\b', text, re.M
        ))
        for declaration in declarations:
            name = declaration["name"]
            declaration["vendored_status"] = (
                "foreign_declaration_retained" if name in retained_foreign_names else
                "direct_self_recursion" if name in recursive_names else
                "nonrecursive_replacement" if re.search(r"^" + re.escape(name) + r"\s*::", text, re.M) else
                "declaration_removed"
            )
        row["upstream_value_foreign_declarations"] = declarations
        for recursion in row["self_recursions"]:
            recursion["replaces_upstream_foreign"] = recursion["name"] in names
        row["changed_original_code_outside_value_ffi"] = ordinary_removals(original, text, covered)
        if data != original_data:
            append_diff(patch, original, text,
                        package["name"] + "@" + package["tag"] + "/src/" + relative,
                        "vendor/" + relative)
        modules.append(row)

    absent_modules = []
    absent_paths = sorted(set(sources) - {row["path"] for row in modules})
    for relative in absent_paths:
        path, package = sources[relative]
        data = path.read_bytes()
        absent_modules.append({
            "path": relative,
            "package": package["name"],
            "tag": package["tag"],
            "commit": package["commit"],
            "upstream_sha256": digest(data),
            "upstream_url": package["remote"].removesuffix(".git") + "/blob/" + package["commit"] + "/src/" + relative,
        })
        append_diff(patch, data.decode("utf-8"), "",
                    package["name"] + "@" + package["tag"] + "/src/" + relative,
                    "/dev/null")

    counts = dict(collections.Counter(row["status"] for row in modules))
    counts.update({
        "vendored_modules": len(modules),
        "packages": len(packages),
        "direct_self_recursions": sum(len(row["self_recursions"]) for row in modules),
        "modules_with_direct_self_recursions": sum(bool(row["self_recursions"]) for row in modules),
        "upstream_modules_absent_from_vendor": absent_paths,
        "upstream_value_foreign_declarations": dict(collections.Counter(
            declaration["vendored_status"] for row in modules
            for declaration in row.get("upstream_value_foreign_declarations", [])
        )),
    })
    try:
        vendor_revision = git(args.vendor.resolve(), "rev-parse", "HEAD")
    except subprocess.CalledProcessError:
        vendor_revision = None
    result = {
        "schema_version": 2,
        "vendor_revision": vendor_revision,
        "counts": counts,
        "packages": packages,
        "modules": modules,
        "absent_modules": absent_modules,
        "limits": [
            "Only supplied upstream checkouts are compared; baseline_unavailable is not a pass.",
            "Self-recursion detection covers exact top-level same-argument equations only.",
            "The script records differences without approving target adaptations or proving semantic equivalence.",
            "The official compiler support dependency ranges do not uniquely pin package patch versions.",
        ],
    }
    (args.out / "inventory.json").write_text(json.dumps(result, ensure_ascii=False, indent=2) + "\n")
    (args.out / "official-vs-vendored.diff").write_text("".join(patch))
    table = ["# Vendored module inventory", "", "Generated by `audit-stdlib-vendor.py`. Status is comparison evidence, not approval.", "",
             "| Module path | Official package/tag | Comparison | Direct self-recursions |", "| --- | --- | --- | --- |"]
    for row in modules:
        origin = row.get("package", "unavailable") + (" " + row["tag"] if "tag" in row else "")
        table.append(f"| `{row['path']}` | {origin} | {row['status']} | {len(row['self_recursions'])} |")
    if absent_modules:
        table.extend(["", "## Official modules absent from the vendored library", "",
                      "| Module path | Official package/tag |", "| --- | --- |"])
        for row in absent_modules:
            table.append(f"| `{row['path']}` | {row['package']} {row['tag']} |")
    (args.out / "modules.md").write_text("\n".join(table) + "\n")
    print(json.dumps(counts, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()

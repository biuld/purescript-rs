"""Portable package content identity shared with the compiler loader."""

from pathlib import Path


def fingerprint(root):
    root = Path(root)
    paths = [Path('manifest.json'), Path('upstream-lock.json')]
    for directory in ('lib', 'conformance'):
        if (root / directory).is_symlink():
            raise ValueError(f'package symlinks are unsupported: {directory}')
        if not (root / directory).is_dir():
            raise ValueError(f'missing package directory: {directory}')
        for path in (root / directory).rglob('*'):
            if path.is_symlink():
                raise ValueError(f'package symlinks are unsupported: {path}')
            if path.is_file():
                paths.append(path.relative_to(root))
            elif not path.is_dir():
                raise ValueError(f'expected regular package file: {path}')
    value = 0xcbf29ce484222325

    def feed(data):
        nonlocal value
        for byte in data:
            value = ((value ^ byte) * 0x100000001b3) & ((1 << 64) - 1)

    feed(b'psrs-stdlib-content-v1\0')
    for path in sorted(paths):
        if (root / path).is_symlink():
            raise ValueError(f'package symlinks are unsupported: {path}')
        data = (root / path).read_bytes()
        feed(path.as_posix().encode('utf-8'))
        feed(b'\0')
        feed(len(data).to_bytes(8, 'little'))
        feed(data)
    return f'fnv1a64-v1:{value:016x}'

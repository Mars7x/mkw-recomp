#!/usr/bin/env python3
"""Basic accidental-artifact guard, not a license/provenance certification."""
from pathlib import Path

root = Path(__file__).resolve().parents[1]
excluded = {'base', 'private', 'target', '.git', '.agents', '.aws', '.codex', '.venv'}
binary_suffixes = {'.bin', '.nso', '.nro', '.nca', '.nsp', '.xci', '.o', '.so', '.a', '.spv'}
checked = 0
for path in root.rglob('*'):
    relative = path.relative_to(root)
    if relative.parts[0] in excluded or '__pycache__' in relative.parts or not path.is_file():
        continue
    if path.suffix in binary_suffixes or path.stat().st_size > 5_000_000:
        raise SystemExit(f'Review unexpected public artifact: {relative}')
    with path.open('rb') as stream:
        magic = stream.read(4)
    if magic in {b'NSO0', b'\x7fELF', b'SARC', b'BNTX', b'FRES'}:
        raise SystemExit(f'Private binary signature in public tree: {relative}')
    checked += 1
print(f'PASS: {checked} public source/documentation files checked; private input/output excluded')

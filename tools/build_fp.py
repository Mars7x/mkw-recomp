#!/usr/bin/env python3
"""Build the pinned SoftFloat subset and native FP callback; outputs stay private."""
from pathlib import Path
import argparse
import hashlib
import json
import subprocess

ROOT = Path(__file__).resolve().parents[1]

def build(output, optimization='-O2'):
    vendor = ROOT/'third_party/softfloat'
    provenance = json.loads((vendor/'PROVENANCE.json').read_text())
    for name, expected in provenance['files'].items():
        if hashlib.sha256((vendor/name).read_bytes()).hexdigest() != expected:
            raise SystemExit(f'Vendored SoftFloat source identity changed: {name}')
    output.mkdir(parents=True, exist_ok=True)
    objects = []
    flags = ['-std=c11', optimization, '-fPIC', '-DSOFTFLOAT_FAST_INT64',
             '-DINLINE_LEVEL=5', '-DSOFTFLOAT_FAST_DIV32TO16', '-DSOFTFLOAT_FAST_DIV64TO32',
             '-DTHREAD_LOCAL=_Thread_local', '-Werror=implicit-function-declaration',
             '-fsanitize=undefined', '-fno-sanitize-recover=undefined',
             '-I'+str(ROOT/'native/softfloat'), '-I'+str(vendor/'source/ARM-VFPv2'),
             '-I'+str(vendor/'source/include'), '-I'+str(ROOT/'native')]
    sources = [vendor/name for name in provenance['files'] if name.endswith('.c')]
    sources.extend([ROOT/'native/fp.c', ROOT/'native/convert.c'])
    for source in sources:
        target = output/(source.stem+'.o')
        subprocess.run(['cc', *flags, *(['-Wall', '-Wextra', '-Werror'] if source.name in ['fp.c','convert.c'] else []), '-c', str(source), '-o', str(target)], check=True)
        objects.append(target)
    library = output/'fp.so'
    subprocess.run(['cc', '-shared', '-Wl,-z,defs', '-fsanitize=undefined',
                    '-fno-sanitize-recover=undefined', *map(str, objects), '-o', str(library)], check=True)
    receipt = {'vendor_revision': provenance['revision'], 'specialization': 'ARM-VFPv2',
               'compiler': subprocess.check_output(['cc', '--version'], text=True).splitlines()[0],
               'flags': flags, 'sources': {str(path.relative_to(ROOT)): hashlib.sha256(path.read_bytes()).hexdigest()
                                         for path in [*sources, ROOT/'native/softfloat/platform.h', ROOT/'native/cpu.h', ROOT/'native/fp_environment.h']},
               'library_sha256': hashlib.sha256(library.read_bytes()).hexdigest(),
               'unsupported_profiles': ['flush', 'enabled FP traps', 'alternative/merge profiles'],
               'game_profile_verified': False}
    (output/'manifest.json').write_text(json.dumps(receipt, indent=2) + '\n')
    return library

if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, default=ROOT/'private/builds/fp')
    parser.add_argument('--optimization', choices=['O0','O2'], default='O2')
    args = parser.parse_args()
    print(build(args.output, '-'+args.optimization))

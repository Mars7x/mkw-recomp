#!/usr/bin/env python3
"""Run reproducible local checks; keep observations and game diagnostics private."""
import argparse
import json
import pathlib
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parents[1]
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('--game', type=pathlib.Path)
args = parser.parse_args()
output = ROOT / 'private/testing'
output.mkdir(parents=True, exist_ok=True)
results = []


def run(label, command, *, cwd=ROOT, required=True):
    started = time.monotonic()
    result = subprocess.run(command, cwd=cwd, text=True, capture_output=True)
    record = {'check': label, 'command': list(map(str, command)),
              'return_code': result.returncode, 'stdout': result.stdout,
              'stderr': result.stderr, 'required': required, 'elapsed_seconds': round(time.monotonic() - started, 3)}
    results.append(record)
    (output / 'validation.json').write_text(json.dumps(results, indent=2) + '\n')
    print(f'{label}: ' + ('PASS' if result.returncode == 0 else 'FAILED' if required else 'UNSUPPORTED (see private receipt)'), flush=True)
    if required and result.returncode:
        print(result.stdout + result.stderr, file=sys.stderr)
        raise SystemExit(result.returncode)
    return result


run('format', ['cargo', 'fmt', '--all', '--', '--check'])
run('lint', ['cargo', 'clippy', '--workspace', '--all-targets', '--offline', '--locked', '--', '-D', 'warnings'])
run('unit and native semantic tests', ['cargo', 'test', '--workspace', '--offline', '--locked'])
run('exclusive-memory release semantic tests', ['cargo', 'test', '--release', '--offline', '--locked', '-p', 'mkw-runtime', '--lib', '--test', 'exclusive_memory', '--test', 'exclusive_native'])
run('native synthetic demo', ['cargo', 'run', '--release', '--offline', '--locked', '--', 'aot-demo'])
binary = ROOT / 'target/release/mkw-recomp'
header = run('native ELF header', ['readelf', '-h', str(ROOT / 'private/builds/synthetic/aot-demo')])
if 'Advanced Micro Devices X86-64' not in header.stdout:
    raise SystemExit('This validation script currently expects an x86-64 host.')

if args.game:
    game = args.game.resolve()
    run('exact supplied ExeFS identity', [str(binary), 'verify', str(game)])
    manifest = json.loads((ROOT / 'private/analysis/exefs-manifest.json').read_text())
    # Change only a test copy of manifest metadata, never any game input.
    negative = output / 'negative-version-gate'
    destination = negative / 'private/analysis'
    destination.mkdir(parents=True, exist_ok=True)
    files = [dict(record) for record in manifest['files']]
    files[0]['sha256'] = '0' * 64
    (destination / 'exefs-manifest.json').write_text(json.dumps({'files': files}))
    result = run('changed identity must reject', [str(binary), 'verify', str(game)], cwd=negative, required=False)
    if result.returncode == 0 or 'SHA-256 differs' not in result.stderr:
        raise SystemExit('Version gate did not reject changed identity.')
    results[-1]['expectation_met'] = True
    run('supplied FDE lifting and native object samples', [str(binary), 'cpu-survey', str(game)])
    run('bounded native sample evidence', [sys.executable, str(ROOT / 'tools/check_survey_samples.py')])
    run('all rejected-extent blocker diagnostics', [str(binary), 'cpu-blockers', str(game)])
    if (ROOT/'private/tools/python/capstone').is_dir():
        run('all-blocker coverage prioritization', [sys.executable, str(ROOT/'tools/rank_blockers.py')])
        run('static runtime contract inventory', [sys.executable, str(ROOT/'tools/survey_runtime_contracts.py')])
    link = run('real module link diagnostic', [str(binary), 'link-check', str(game)])
    results[-1]['classification'] = 'implemented and tested' if link.returncode == 0 else 'blocked before guest execution'
    candidate = None
    with (ROOT / 'private/analysis/function-candidates.jsonl').open() as stream:
        for line in stream:
            record = json.loads(line)
            if 'PAC-prologue-candidate' in record['sources']:
                candidate = record
                break
    if candidate:
        result = run('truncated PAC CFG must reject generation', [str(binary), 'generate-slice', str(game), candidate['module'], hex(candidate['module_offset']), '4'], required=False)
        if result.returncode == 0 or 'unresolved CFG edge' not in result.stderr:
            raise SystemExit('Truncated PAC CFG was not rejected.')
        results[-1]['expectation_met'] = True
    (output / 'validation.json').write_text(json.dumps(results, indent=2) + '\n')

if (ROOT/'private/tools/qemu-pauth-reference.c').is_file():
    run('QARMA5 independent reference', [sys.executable, str(ROOT/'tools/check_pac_reference.py')])

if (ROOT/'private/tools/python/capstone').is_dir():
    run('encoding reference cross-check', [sys.executable, str(ROOT/'tools/check_decoder.py')])
run('blocker ranking synthetic checks', [sys.executable, str(ROOT/'tools/rank_blockers.py'), '--self-test'])
run('complete blocker-set candidate enumeration', [sys.executable, str(ROOT/'tools/test_rank_candidates.py')])
run('publication source snapshot guards', [sys.executable, str(ROOT / 'tools/test_publish_snapshot.py')])
run('public artifact guard', [sys.executable, str(ROOT/'tools/check_public_tree.py')])

run('five-fix candidate ranking', [sys.executable, str(ROOT / 'tools/test_rank_five.py')])

run('complete-set bundle ranking', [sys.executable, str(ROOT / 'tools/test_rank_bundles.py')])

run('fixed half-conversion ranking policy', [sys.executable, str(ROOT / 'tools/test_rank_policy.py')])

run('remaining complete-set ranking', [sys.executable, str(ROOT / 'tools/test_rank_remaining.py')])

run('runtime contract inventory synthetic checks', [sys.executable, str(ROOT / 'tools/test_runtime_contracts.py')])

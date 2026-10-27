#!/usr/bin/env python3
"""Check bounded native-sample evidence independently of extent-lifting counts."""
import collections
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
summary = json.loads((ROOT / 'private/analysis/cpu-survey-summary.json').read_text())
modules = {row['module']: row for row in summary['modules']}
compiled = collections.Counter()
pac = collections.Counter()
deferred = collections.Counter()
observed = collections.defaultdict(set)
sampled = collections.defaultdict(set)
for line in (ROOT / 'private/analysis/cpu-survey.jsonl').open():
    row = json.loads(line)
    module = row['module']
    features = set(row.get('instruction_features', []))
    observed[module].update(features)
    policy = modules[module]
    limit = policy['native_sample_instruction_limit']
    assert limit == 4096, 'unexpected representative instruction budget'
    if 'native_sample_deferred' in row:
        record = row['native_sample_deferred']
        assert 'compiled_object' not in row
        assert record['instructions'] == row['subset_instructions'] > limit
        assert record['limit'] == limit
        deferred[module] += 1
    if 'compiled_object' not in row:
        continue
    assert row['subset_instructions'] <= limit
    obj = ROOT / row['compiled_object']
    source = obj.with_name('guest.c')
    metadata = json.loads(obj.with_name('source-map.json').read_text())
    assert metadata['compiled'] and metadata['executed'] is False
    assert len(metadata['ir']) == row['subset_instructions']
    assert metadata['module'] == module and metadata['extent'] == row['extent']
    assert hashlib.sha256(source.read_bytes()).hexdigest() == row['generated_source_sha256']
    with obj.open('rb') as stream:
        header = stream.read(64)
    assert header[:6] == b'\x7fELF\x02\x01', 'native sample is not little-endian ELF64'
    assert int.from_bytes(header[16:18], 'little') == 1, 'sample must remain relocatable'
    assert int.from_bytes(header[18:20], 'little') == 62, 'expected native x86-64 object'
    compiled[module] += 1
    pac[module] += bool(row['requires_pac_backend'])
    sampled[module].update(features)
for module, policy in modules.items():
    assert compiled[module] == policy['native_sample_objects']
    assert pac[module] == policy['native_pac_sample_objects']
    assert deferred[module] == policy['deferred_native_sample_candidates']
    assert sampled[module] == set(policy['compiled_instruction_features'])
    assert observed[module] - sampled[module] == set(policy['uncompiled_instruction_features'])
receipt = {'native_sample_objects': sum(compiled.values()),
           'deferred_candidates': sum(deferred.values()),
           'instruction_limit': 4096, 'source_hashes_and_native_headers_checked': True,
           'uncompiled_features': {m: sorted(observed[m] - sampled[m]) for m in modules},
           'game_code_executed': False}
(ROOT / 'private/testing/survey-samples.json').write_text(json.dumps(receipt, indent=2) + '\n')
print(json.dumps(receipt))

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
run('exclusive-memory release semantic tests', ['cargo', 'test', '--release', '--offline', '--locked', '-p', 'mkw-runtime', '--lib', '--test', 'exclusive_memory', '--test', 'exclusive_native', '--test', 'exclusive_entry', '--test', 'exclusive_lifting', '--test', 'exclusive_calls', '--test', 'romfs_hle', '--test', 'romfs_hooks', '--test', 'thread_runner', '--test', 'services', '--test', 'events', '--test', 'event_services', '--test', 'event_poll', '--test', 'service_permissions', '--test', 'kernel_handles', '--test', 'kernel_handles_native', '--test', 'process_entropy', '--test', 'table_event_services', '--test', 'terminal_proofs', '--test', 'import_thunks', '--test', 'conditional_proofs', '--test', 'guarded', '--test', 'pauth', '--test', 'guarded_entry', '--test', 'thread_queries', '--test', 'stack_query', '--test', 'current_thread', '--test', 'initial_thread_queries', '--test', 'heap_memory', '--test', 'heap_services', '--test', 'arbitration', '--test', 'priority', '--test', 'priority_native', '--test', 'arbitration_svc', '--test', 'actor_entry', '--test', 'condition_native', '--test', 'condition_svc', '--test', 'owned_memory_budget', '--test', 'module_pages', '--test', 'process_memory', '--test', 'process_regions'])
run('kernel capability facts release tests', ['cargo', 'test', '--release', '--offline', '--locked', '-p', 'mkw-formats', '--test', 'capabilities'])
run('immutable terminal proof release tests', ['cargo', 'test', '--release', '--offline', '--locked', '-p', 'mkw-arm64', '--test', 'terminal_proofs', '--test', 'import_thunks', '--test', 'conditional_proofs', '--test', 'guarded_contracts'])
run('native synthetic demo', ['cargo', 'run', '--release', '--offline', '--locked', '--', 'aot-demo'])
binary = ROOT / 'target/release/mkw-recomp'
header = run('native ELF header', ['readelf', '-h', str(ROOT / 'private/builds/synthetic/aot-demo')])
if 'Advanced Micro Devices X86-64' not in header.stdout:
    raise SystemExit('This validation script currently expects an x86-64 host.')

if args.game:
    game = args.game.resolve()
    run('exact supplied ExeFS identity', [str(binary), 'verify', str(game)])
    run('declared supplied runtime metadata consistency', [str(binary), 'runtime-metadata-check', str(game)])
    facts = json.loads((ROOT / 'private/analysis/runtime-metadata-facts.json').read_text())
    if (facts['signature_verified'] or facts['target_configuration_verified']
            or facts['game_code_executed'] or facts['services_activated']):
        raise SystemExit('Runtime metadata inspection crossed authorization/execution boundaries')
    run('synthetic native HLE read of supplied RomFS', [str(binary), 'romfs-hle-check', str(game)])
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
    run('immutable terminal source evidence', [str(binary), 'terminal-proof-check', str(game)])
    source_proofs = json.loads((ROOT / 'private/analysis/terminal-source-proof-summary.json').read_text())
    source_strict = json.loads((ROOT / 'private/analysis/cpu-survey-summary.json').read_text())
    if ({m['module']: (m['indexed_extents'], m['strict_accepted']) for m in source_proofs['modules']}
            != {m['module']: (m['fde_extents'], m['subset_liftable_functions']) for m in source_strict['modules']}
            or any(source_proofs[k] for k in ['compiler_acceptance_changed', 'game_code_generated',
                'game_code_executed', 'runtime_guards_installed', 'target_profile_verified',
                'imports_or_cross_image_facts_supported'])
            or any(m['checked_return_free_candidates'] != m['certified_local_closed_control']
                   for m in source_proofs['modules'])):
        raise SystemExit('Source proof checks changed admission/generation/execution or left candidates unverified')
    run('immutable import source witnesses', [str(binary), 'import-witness-check', str(game)])
    imports = json.loads((ROOT / 'private/analysis/import-witness-summary.json').read_text())
    expected_imports = {'main': (1210, 1209, 1203), 'rtld': (2, 1, 1),
                        'sdk': (9638, 9637, 9637), 'subsdk0': (224, 223, 223)}
    if ({m['module']: (m['canonical_thunks'], m['zero_addend_jump_slot_correlations'],
                      m['diagnostic_selected_providers']) for m in imports['modules']} != expected_imports
            or any(imports[k] for k in ['compiler_acceptance_changed', 'game_code_generated',
            'game_code_executed', 'runtime_guards_installed', 'target_profile_verified',
            'terminal_import_facts_supported'])
            or any(m['diagnostic_selected_providers'] > m['zero_addend_jump_slot_correlations']
                   or m['zero_addend_jump_slot_correlations'] > m['canonical_thunks']
                   for m in imports['modules'])):
        raise SystemExit('Import source witness crossed source/diagnostic boundaries')
    run('conditional source obligations', [str(binary), 'conditional-proof-check', str(game)])
    conditional = json.loads((ROOT / 'private/analysis/conditional-source-summary.json').read_text())
    expected_conditional = {'main': (3701, 9, 0, 0, 0), 'rtld': (8, 0, 0, 0, 0),
                            'sdk': (4334, 7, 38, 213, 4085), 'subsdk0': (1292, 5, 0, 0, 0)}
    if ({m['module']: (m['bounded_strict_rejects'], m['unconditional_seed_facts'],
                      m['conditional_aliases'], m['conditional_control_facts'],
                      m['conditionally_closed_rejected_graphs']) for m in conditional['modules']} != expected_conditional
            or any(m['oversized_unanalyzed'] for m in conditional['modules'])
            or any(conditional[k] for k in ['compiler_acceptance_changed', 'game_code_generated',
                'game_code_executed', 'runtime_guards_installed', 'target_profile_verified',
                'runtime_premises_discharged', 'cross_image_imports_supported'])):
        raise SystemExit('Conditional source evidence changed admission/runtime scope or lost measured evidence')
    conditional_details = json.loads((ROOT / 'private/analysis/conditional-source-details.json').read_text())
    if len(conditional_details) != 4085 or any(not d['undischarged_requirements_private'] for d in conditional_details):
        raise SystemExit('Conditional source premises were lost or presented as unconditional')
    run('explicit exclusive-v10 conditional compiler survey', [str(binary), 'cpu-survey-exclusive-v10', str(game)])
    explicit = json.loads((ROOT / 'private/analysis/cpu-exclusive-v10/summary.json').read_text())
    strict = json.loads((ROOT / 'private/analysis/cpu-survey-summary.json').read_text())
    expected = {m['module']: (m['fde_extents'], m['subset_liftable_functions']) for m in strict['modules']}
    observed = {m['module']: (m['indexed_extents'], m['default_accepted']) for m in explicit['modules']}
    if observed != expected or explicit['game_code_executed'] or explicit['default_acceptance_changed'] or explicit['target_profile_verified']:
        raise SystemExit('Explicit v10 survey changed the strict baseline or execution/profile gates')
    run('explicit exclusive-v2 conditional compiler survey', [str(binary), 'cpu-survey-exclusive-v2', str(game)])
    routed = json.loads((ROOT / 'private/analysis/cpu-exclusive-v2/summary.json').read_text())
    if (routed['modules'] != explicit['modules'] or routed['details_private'] != explicit['details_private']
            or routed['cpu_frame_version'] != 10 or routed['exclusive_table_version'] != 2
            or explicit['exclusive_table_version'] != 1 or routed['game_code_executed']
            or routed['default_acceptance_changed'] or routed['target_profile_verified']):
        raise SystemExit('Exclusive-v2 routing changed compiler coverage, limits, identities or execution gates')
    run('explicit service-table-v3 conditional compiler survey', [str(binary), 'cpu-survey-services-v3', str(game)])
    service = json.loads((ROOT / 'private/analysis/cpu-services-v3/summary.json').read_text())
    service_defaults = {m['module']: (m['indexed_extents'], m['default_accepted']) for m in service['modules']}
    service_v1 = {m['module']: m['v1_conditional_additional'] for m in service['modules']}
    prior_v1 = {m['module']: m['exclusive_v10_conditional_additional'] for m in explicit['modules']}
    if (service_defaults != expected or service_v1 != prior_v1 or service['cpu_frame_version'] != 10
            or service['exclusive_table_version'] != 3 or service['game_code_executed']
            or service['default_acceptance_changed'] or service['target_profile_verified']
            or any(m['services_v3_conditional_additional'] != m['v1_conditional_additional'] + m['new_beyond_v1']
                   or m['additional_compiled_objects'] + m['additional_deferred'] != m['services_v3_conditional_additional']
                   for m in service['modules'])):
        raise SystemExit('Service-v3 survey changed strict baseline, prior profile, counts or execution/configuration gates')
    run('guarded-v4 compiler survey and bounded native objects', [str(binary), 'guarded-compile-check', str(game)])
    guarded = json.loads((ROOT / 'private/analysis/guarded-v4/summary.json').read_text())
    plain_v4 = {m['module']: m['plain_services_eligible'] for m in guarded['modules']}
    prior_v3 = {m['module']: m['default_accepted'] + m['services_v3_conditional_additional'] for m in service['modules']}
    if (plain_v4 != prior_v3
            or {m['module']: m['additional_guarded_eligible'] for m in guarded['modules']}
                != {'main': 0, 'rtld': 0, 'sdk': 4085, 'subsdk0': 0}
            or sum(m['guarded_eligible_total'] for m in guarded['modules']) != 218749
            or guarded['native_sample_objects'] != 9 or guarded['native_sample_source_bytes'] != 588
            or not guarded['game_code_generated']
            or any(guarded[k] for k in ['existing_acceptance_changed', 'original_code_executed',
                 'runtime_routes_registered', 'target_configuration_verified',
                 'cross_image_no_return_supported', 'guarded_veneers_supported'])):
        raise SystemExit('Guarded-v4 survey changed legacy acceptance, lost bounded evidence or crossed original-execution gates')
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

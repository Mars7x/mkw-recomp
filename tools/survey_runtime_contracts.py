#!/usr/bin/env python3
"""Inventory static runtime dependencies; never supplies runtime values or executes guests."""
from pathlib import Path
import collections,itertools,json,sys
ROOT=Path(__file__).resolve().parents[1]
EXCLUSIVE=('ldxr','ldaxr','ldxp','ldaxp','stxr','stlxr','stxp','stlxp')

def contract_key(word,name,operands):
    if name=='svc':
        if word&0xffe0001f!=0xd4000001:raise ValueError('SVC classification/encoding differs')
        return f'service:svc:{(word>>5)&65535:#06x}'
    if name.startswith(EXCLUSIVE)or name=='clrex':return 'memory:exclusive-monitor'
    if name in {'mrs','msr'}:
        pieces=[x.strip()for x in operands.split(',')]
        return f'system-register:{"read"if name=="mrs"else"write"}:{pieces[-1]if name=="mrs"else pieces[0]}'
    if name in {'dc','ic','tlbi','at','sys','sysl','isb','dsb','dmb'}:return f'system-operation:{name}:{operands.split(",")[0].strip()}'
    if name in {'wfe','wfi','sev','sevl'}:return 'scheduler:event-wait'
    return None

def contract_buckets(rows,classify):
    buckets=collections.Counter();barriers=collections.Counter();all_presence=collections.Counter();services=collections.Counter();total=0
    for row in rows:
        total+=1;keys=set();unsupported=False
        for issue in row['cfg_decode_issues']:
            word=issue['word']
            key=classify(word)if word is not None else None
            if key is None:unsupported=True
            else:keys.add(key)
        all_presence.update(keys)
        services.update(k for k in keys if k.startswith('service:svc:'))
        structural=bool(row['cfg_edge_issues'])or row.get('omitted',False)
        if structural:barriers['structural']+=1
        if unsupported:barriers['other_isa_policy_opaque_or_control_flow']+=1
        if not structural and not unsupported and keys:buckets[frozenset(keys)]+=1
    return buckets,barriers,all_presence,services,total

def ranked_contract_sets(buckets):
    # No effort estimate without target ABI, clock, memory and service contracts.
    keys=sorted(set().union(*buckets))if buckets else[]
    groups=set(buckets)
    groups.update(frozenset(x)for n in [1,2,3]for x in itertools.combinations(keys,n))
    return sorted(({'contracts':sorted(group),'conditional_extent_gain':sum(n for required,n in buckets.items()if required<=group),'implementation_effort':'unpriced: target semantics/configuration required','runnable_functions_claimed':False}for group in groups),key=lambda r:(-r['conditional_extent_gain'],len(r['contracts']),r['contracts']))

def main():
    sys.path.insert(0,str(ROOT/'private/tools/python'))
    from capstone import Cs,CS_ARCH_ARM64,CS_MODE_LITTLE_ENDIAN,__version__
    from rank_blockers import fixed_half_conversion_policy
    decoder=Cs(CS_ARCH_ARM64,CS_MODE_LITTLE_ENDIAN);cache={}
    def classify(word):
        if word not in cache:
            instructions=list(decoder.disasm(word.to_bytes(4,'little'),0))
            # Preserve fixed-test policy boundaries instead of repricing them as contracts.
            cache[word]=None if fixed_half_conversion_policy(word)or word&0xfffffc00==0x4ea1d800 or not instructions else contract_key(word,instructions[0].mnemonic,instructions[0].op_str)
        return cache[word]
    folder=ROOT/'private/analysis'
    strict=json.loads((folder/'cpu-survey-summary.json').read_text());diagnostic=json.loads((folder/'cpu-all-blockers-summary.json').read_text())
    expected={m['module']:(m['fde_extents'],m['subset_liftable_functions'])for m in strict['modules']}
    observed={m['module']:(m['extents'],m['accepted'])for m in diagnostic['modules']}
    if expected!=observed:raise SystemExit('Strict and all-blocker baselines differ; rerun before contract inventory')
    rows=(json.loads(x)for x in(folder/'cpu-all-blockers.jsonl').open())
    buckets,barriers,presence,services,total=contract_buckets(rows,classify)
    if total!=sum(m['rejected']for m in diagnostic['modules']):raise SystemExit('Rejected extent count differs')
    accepted_flags=collections.Counter();accepted=0
    for line in(folder/'cpu-survey.jsonl').open():
        row=json.loads(line)
        if 'failure'in row:continue
        accepted+=1;accepted_flags.update(k for k,v in row.items()if k.startswith('requires_')and v is True)
    if accepted!=sum(m['subset_liftable_functions']for m in strict['modules']):raise SystemExit('Strict detail/summary counts differ')
    ranked=ranked_contract_sets(buckets)
    report={'classification_reference':f'Capstone {__version__}','guest_code_executed':False,'target_configuration_verified':False,'acceptance_rules_changed':False,'rejected_extents':total,'nonstructural_contract_only_extents':sum(buckets.values()),'barrier_counts_overlap':dict(barriers),'distinct_svc_immediates':len(services),'svc_extent_presence':dict(sorted(services.items())),'contract_extent_presence':dict(presence.most_common()),'accepted_extent_backend_requirements_overlap':dict(accepted_flags),'accepted_extents':accepted,'ranked_conditional_contract_sets':ranked,'limitations':'Static optimistic-CFG requirements only; unknown decode continuation, service return/control effects, target ABI and hardware configuration remain unverified. Counts are conditional additional strict extents, not runnable functions, verified service mappings or runtime coverage. Other ISA/policy/opaque/control-flow and structural barriers remain excluded. Effort is deliberately unpriced; no successful stubs or fabricated clocks/configuration.'}
    (folder/'runtime-contracts.json').write_text(json.dumps(report,indent=2)+'\n')
    print(json.dumps({k:report[k]for k in ['guest_code_executed','target_configuration_verified','acceptance_rules_changed','rejected_extents','nonstructural_contract_only_extents','distinct_svc_immediates','accepted_extents']}))
    print(json.dumps(ranked[:8],indent=2))
if __name__=='__main__':main()

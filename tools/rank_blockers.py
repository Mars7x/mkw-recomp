#!/usr/bin/env python3
"""Rank conditional static coverage gains; never changes decoder/acceptance rules."""
from pathlib import Path
import argparse,collections,itertools,json,sys
ROOT=Path(__file__).resolve().parents[1]
# Engineering estimates in minutes, excluding the fixed full validation gate.
# Scoped from observed forms and reusable fixtures; half-precision FCVT needs new backend work.
EFFORT={'pacib':(2,5),'sqrshl':(2,5),'srshl':(3,8),'sadalp':(1,3),'rshrn':(2,5),'rshrn2':(1,2),'sqdmlal':(4,10),'sqdmlal2':(1,2),'pmull':(6,15),'pmull2':(6,15),'uabd':(4,10),'uabdl':(4,10),'fcvtl':(3,8),'fcvt':(90,210),'sqshl':(5,15),'sminp':(4,10),'smaxp':(4,10),'saddlp':(6,18),'frecpe.2s':(90,180),'clz':(20,45),'frecps':(2,5),'frsqrts':(90,180),'prfm':(10,25),'prfum':(10,25),'xpaci':(20,45),'shsub':(25,60),'uhsub':(25,60),'shadd':(25,60),'uhadd':(25,60),'srhadd':(25,60),'urhadd':(25,60),'mla':(20,45),'mls':(20,45),'sha1c':(3,7),'sha1p':(1,3),'sha1m':(1,3),'sha1h':(1,2),'sha1su0':(2,4),'sha1su1':(2,4),'sha256su0':(2,4),'sha256su1':(2,4),'sha256h':(3,7),'sha256h2':(1,3),'frecpe':(90,180),'frsqrte':(120,240)}
RUNTIME={'svc','hvc','smc','udf','brk','hlt','eret','drps','wfe','wfi','sev','sevl','clrex'}
EXCLUSIVE=('ldxr','ldaxr','ldxp','ldaxp','stxr','stlxr','stxp','stlxp')

def fixed_half_conversion_policy(word):
    # Preserved floating::tests negatives exclude both complete S/H arrangements.
    return word&0xfffffc00 in {0x1ee24000,0x1e23c000}

def family_cost(names):
    bands=[EFFORT.get(n,(60,180)) for n in names]
    low=sum(v[0]for v in bands);high=sum(v[1]for v in bands)
    # Shared decode/emitter/oracle work, not a discount to correctness gates.
    if set(names)<={'prfm','prfum'} and len(names)==2:return (15,35)
    if set(names)<={'mla','mls'} and len(names)==2:return (25,60)
    if len(names)>1 and all(n in {'shsub','uhsub','shadd','uhadd','srhadd','urhadd'}for n in names):return (30,75)
    if {'sminp','smaxp'}<=set(names):low-=3;high-=8
    if {'pmull','pmull2'}<=set(names):low-=4;high-=10
    if {'uabd','uabdl'}<=set(names):low-=2;high-=5
    if {'sha1c','sha1p','sha1m','sha1h','sha1su0','sha1su1','sha256h','sha256h2','sha256su0','sha256su1'}<=set(names):low-=3;high-=6
    if {'sqrshl','srshl'}<=set(names):low-=1;high-=3
    if {'rshrn','rshrn2'}<=set(names):low-=1;high-=2
    if {'sqdmlal','sqdmlal2'}<=set(names):low-=1;high-=2
    return low,high

def candidates(buckets,validation_minutes):
    singles={next(iter(k)) for k in buckets if len(k)==1}
    candidates={frozenset([n]) for k in buckets for n in k}
    candidates.update(k for k in buckets if len(k)>1)
    pure=[k for k in buckets if not any(n.startswith(('runtime:','configuration:','opaque:','policy:','control_flow:'))for n in k)]
    for count in range(2,min(5,len(pure))+1):candidates.update(frozenset().union(*items)for items in itertools.combinations(pure,count))
    names=sorted({n for k in buckets for n in k if not n.startswith(('runtime:','configuration:','opaque:','policy:','control_flow:'))})
    for size in [2,3,4,5]:candidates.update(map(frozenset,itertools.combinations(names,size)))
    for names in [('prfm','prfum'),('mla','mls'),('shsub','uhsub'),('sha1c','sha1p','sha1m'),('sha256h','sha256h2'),('frecpe','frsqrte')]:
        if any(n in singles or any(n in k for k in buckets) for n in names):candidates.add(frozenset(names))
    rows=[]
    for fixed in candidates:
        if any(n.startswith(('runtime:','configuration:','opaque:','policy:','control_flow:')) for n in fixed):continue
        gain=sum(count for required,count in buckets.items()if required<=fixed)
        low,high=family_cost(fixed);rows.append({'fixes':sorted(fixed),'predicted_cfg_gain':gain,'implementation_minutes':[low,high],'validation_minutes':validation_minutes,'predicted_gain_per_hour_range':[round(gain*60/(high+validation_minutes),2),round(gain*60/(low+validation_minutes),2)],'score':gain*60/((low+high)/2+validation_minutes)})
    return sorted(rows,key=lambda r:(-r['score'],-r['predicted_cfg_gain'],len(r['fixes']),r['fixes']))

def self_test():
    buckets=collections.Counter({frozenset(['prfm']):10,frozenset(['prfum']):2,frozenset(['prfm','prfum']):5,frozenset(['runtime:svc']):100})
    rows=candidates(buckets,15);pair=next(r for r in rows if r['fixes']==['prfm','prfum']);assert pair['predicted_cfg_gain']==17
    assert next(r for r in rows if r['fixes']==['prfm'])['predicted_cfg_gain']==10
    assert not any('runtime:svc'in r['fixes']for r in rows)
    assert candidates(collections.Counter(),15)==[]
    print('blocker ranking synthetic checks: PASS')

def run(validation_minutes):
    sys.path.insert(0,str(ROOT/'private/tools/python'))
    from capstone import Cs,CS_ARCH_ARM64,CS_MODE_LITTLE_ENDIAN,__version__,CS_GRP_JUMP,CS_GRP_CALL,CS_GRP_RET,CS_GRP_INT,CS_GRP_IRET
    decoder=Cs(CS_ARCH_ARM64,CS_MODE_LITTLE_ENDIAN);decoder.detail=True
    cache={}
    def classify(issue):
        word=issue['word'];key=(word,issue['error'].split(' at ')[0])
        if key in cache:return cache[key]
        ins=list(decoder.disasm(word.to_bytes(4,'little'),0))
        if not ins:result=('opaque:undecoded','opaque')
        else:
            ins=ins[0];name=ins.mnemonic
            if fixed_half_conversion_policy(word):result=('policy:fcvt-half.fixed-test','policy')
            elif name=='frecpe' and word&0xfffffc00==0x4ea1d800:result=('policy:frecpe.4s.fixed-test','policy')
            elif name=='frecpe' and word&0xfffffc00==0x0ea1d800:result=('frecpe.2s','isa')
            elif name in {'prfm','prfum'} and ((word>>5)&31)==31 and (word&0xff000000)!=0xd8000000:result=('configuration:prefetch-sp','configuration')
            elif name in RUNTIME or name.startswith(EXCLUSIVE):result=('runtime:'+name,'runtime')
            elif name in {'dc','ic','tlbi','at','sys','sysl','isb','dsb','dmb'}:result=('configuration:'+name,'configuration')
            elif name in {'mrs','msr'}:
                operands=[x.strip()for x in ins.op_str.split(',')];register=operands[-1]if name=='mrs'else operands[0];result=('configuration:'+name+':'+register,'configuration')
            elif any(g in ins.groups for g in [CS_GRP_JUMP,CS_GRP_CALL,CS_GRP_RET,CS_GRP_INT,CS_GRP_IRET]):result=('control_flow:'+name,'control_flow')
            elif 'constrained' in issue['error']:result=('policy:'+name,'policy')
            else:result=(name,'isa')
        cache[key]=result;return result
    # The preserved vector_compare negative test excludes the whole 4S FRECPE arrangement.
    # Valid 2S ISA work remains separately ranked; no register-specific blacklist.
    buckets=collections.Counter();corroborated=collections.Counter();occurrences=collections.Counter();extent_presence=collections.Counter();linear_presence=collections.Counter();classes=collections.Counter();structural=0;runtime_blocked=0;rows_count=0;large_sets=0;baseline_edges=0;pure_isa=0;details=[]
    for line in (ROOT/'private/analysis/cpu-all-blockers.jsonl').open():
        row=json.loads(line);rows_count+=1
        baseline_edges+=row['baseline_failure'].startswith('unresolved CFG edge')
        requirements={classify(i)[0] for i in row['cfg_decode_issues']}
        linear={classify(i)[0]for i in row['linear_decode_issues']}
        occurrences.update(classify(i)[0]for i in row['cfg_decode_issues']);extent_presence.update(requirements);linear_presence.update(linear)
        barriers=bool(row['cfg_edge_issues'])or row.get('omitted',False)
        kinds={classify(i)[1]for i in row['cfg_decode_issues']};classes.update(kinds)
        if barriers:structural+=1
        if kinds&{'runtime','configuration','opaque','policy','control_flow'}:runtime_blocked+=1
        if not barriers and requirements:
            if kinds=={'isa'}:pure_isa+=1
            buckets[frozenset(requirements)]+=1
            corroborated[(frozenset(requirements),frozenset(linear))]+=1
            if len(requirements)>3:large_sets+=1
        details.append({'module':row['module'],'extent':row['extent'],'required_fixes':sorted(requirements),'linear_span_potential_fixes':sorted(linear),'structural_barrier':barriers})
    strict=json.loads((ROOT/'private/analysis/cpu-survey-summary.json').read_text())
    diagnostic=json.loads((ROOT/'private/analysis/cpu-all-blockers-summary.json').read_text())
    expected={m['module']:(m['fde_extents'],m['subset_liftable_functions'])for m in strict['modules']}
    observed={m['module']:(m['extents'],m['accepted'])for m in diagnostic['modules']}
    if expected!=observed:raise SystemExit('Strict acceptance baseline differs; rerun both surveys on the same source before ranking')
    ranked=candidates(buckets,validation_minutes)
    for item in ranked:
        fixed=set(item['fixes']);item['linear_corroborated_cfg_gain']=sum(n for (required,linear),n in corroborated.items()if required<=fixed and linear<=fixed)
        item['score']=round(item['score'],2)
    report={'classification_reference':f'Capstone {__version__}','acceptance_rules_changed':False,'guest_code_executed':False,'rejected_extents':rows_count,'structural_blocked_extents':structural,'baseline_cfg_edge_failures':baseline_edges,'nonstructural_pure_isa_extents':pure_isa,'runtime_configuration_opaque_or_policy_blocked_extents':runtime_blocked,'structural_and_other_barrier_counts_overlap':True,'blocker_sets_larger_than_three':large_sets,'candidate_enumeration':'All singles/pairs/triples/quadruples/quintuples plus complete observed requirement sets and unions of up to five pure complete sets; larger coherent bundles retain explicit full costs.','method':'Every rejected extent: strict baseline, optimistic CFG continuation after failures, plus complete linear-span inventory. Supported control edges/tail policy unchanged. Rankings assume implemented non-control instructions fall through and retain acceptance. Linear inventory can include unreachable data; estimates are conditional, not actual acceptance or runnable functions. Re-survey after every fix.','effort_basis':'Engineering estimates in minutes including implementation/correctness work, plus explicit fixed validation budget; unknown ISA defaults 60..180. Runtime/configuration/opaque/policy/control-flow candidates excluded. Shared-family estimates explicit.','ranked_candidates':ranked,'families':[{'family':name,'cfg_extents_containing':n,'cfg_instruction_occurrences':occurrences[name],'linear_span_extents_containing':linear_presence[name],'sole_nonstructural_blocker_extents':buckets[frozenset([name])]}for name in sorted(extent_presence.keys()|linear_presence.keys(),key=lambda n:(-extent_presence[n],-linear_presence[n],n))for n in [extent_presence[name]]]}
    (ROOT/'private/analysis/cpu-blocker-rankings.json').write_text(json.dumps(report,indent=2)+'\n')
    with (ROOT/'private/analysis/cpu-blocker-requirements.jsonl').open('w')as out:
        for row in details:out.write(json.dumps(row)+'\n')
    print(json.dumps({k:report[k]for k in ['rejected_extents','baseline_cfg_edge_failures','structural_blocked_extents','nonstructural_pure_isa_extents','runtime_configuration_opaque_or_policy_blocked_extents','blocker_sets_larger_than_three']}))
    print(json.dumps(ranked[:12],indent=2))

if __name__=='__main__':
    p=argparse.ArgumentParser(description=__doc__);p.add_argument('--self-test',action='store_true');p.add_argument('--validation-minutes',type=float,default=15);a=p.parse_args()
    if a.self_test:self_test()
    else:run(a.validation_minutes)

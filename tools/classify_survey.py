#!/usr/bin/env python3
"""Classify first unsupported words with an optional private Capstone install.
Counts are first blockers per indexed function, not execution frequencies.
"""
from pathlib import Path
import sys,json,re,collections
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'private/tools/python'))
from capstone import Cs,CS_ARCH_ARM64,CS_MODE_LITTLE_ENDIAN,__version__
decoder=Cs(CS_ARCH_ARM64,CS_MODE_LITTLE_ENDIAN)
counts=collections.Counter();examples={}
for line in (root/'private/analysis/cpu-survey.jsonl').open():
    row=json.loads(line)
    match=re.match(r'unsupported instruction ([0-9a-f]{8})',row.get('failure',''))
    if not match:continue
    word=int(match[1],16)
    instructions=list(decoder.disasm(word.to_bytes(4,'little'),0))
    mnemonic=instructions[0].mnemonic if instructions else 'undecoded'
    counts[mnemonic]+=1
    examples.setdefault(mnemonic,{'module':row['module'],'extent':row['extent'],'failure':row['failure']})
report={'decoder':f'Capstone {__version__}','kind':'first unsupported instruction per indexed function; not dynamic coverage','counts':dict(counts.most_common()),'examples':examples}
(root/'private/analysis/cpu-next-blockers.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps(dict(counts.most_common(12))))

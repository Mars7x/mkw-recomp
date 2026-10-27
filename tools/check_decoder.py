#!/usr/bin/env python3
"""Cross-check independently authored encoding cases with optional private Capstone.
This checks decoding/classification, not instruction execution or hardware behavior.
"""
from pathlib import Path
import sys,json,subprocess,collections
root=Path(__file__).resolve().parents[1]
sys.path.insert(0,str(root/'private/tools/python'))
from capstone import Cs,CS_ARCH_ARM64,CS_MODE_LITTLE_ENDIAN,__version__
words=set()
for vector in range(2):
 for size in range(4):
  for opc in range(4):
   for registers in [(0,1),(31,31),(1,1)]:
    rt,rn=registers
    body=(size<<30)|(vector<<26)|(opc<<22)|(rn<<5)|rt
    words.add(0x39000000|body|(5<<10))
    for mode in range(4):words.add(0x38000000|body|(0x1f0<<12)|(mode<<10))
    for option in range(8):
     for scaled in range(2):words.add(0x38200800|body|(2<<16)|(option<<13)|(scaled<<12))
 for opc in range(4):
  for mode in range(4):
   for load in range(2):words.add(0x28000000|(opc<<30)|(vector<<26)|(mode<<23)|(load<<22)|(0x7e<<15)|(1<<10)|(31<<5))
for sf in range(2):
 for n in range(2):
  for r in range(64):
   for s in range(64):
    words.add(0x12000000|(sf<<31)|(n<<22)|(r<<16)|(s<<10)|(1<<5))
    words.add(0x13000000|(sf<<31)|(n<<22)|(r<<16)|(s<<10)|(1<<5))
for vector in range(2):
 for opc in range(4):words.add(0x18000000|(vector<<26)|(opc<<30)|(4<<5))
for sf in range(2):
 for sub in range(2):
  for immediate in range(2):
   for condition in range(16):
    for nzcv in range(16):
     for rn in [1,31]:
      for rhs in [2,31]:words.add(0x3a400000|(sf<<31)|(sub<<30)|(immediate<<11)|(condition<<12)|(rhs<<16)|(rn<<5)|nzcv)
 for group in range(8):
  for sub in range(2):
   for acc in [0,31]:
    for rn in [1,31]:words.add(0x1b000000|(sf<<31)|(group<<21)|(sub<<15)|(2<<16)|(acc<<10)|(rn<<5))
 for signed in range(2):
  for rn in [1,31]:words.add(0x1ac00800|(sf<<31)|(signed<<10)|(2<<16)|(rn<<5))
for rn in range(32):words.add(0xd61f0000|(rn<<5))
for q in range(2):
 for op in range(2):
  for o2 in range(2):
   for mode in range(16):
    for imm in range(256):words.add(0x0f000400|(q<<30)|(op<<29)|(o2<<11)|(mode<<12)|((imm>>5)<<16)|((imm&31)<<5))
 for op in range(2):
  for imm5 in range(32):
   for imm4 in range(16):words.add(0x0e000400|(q<<30)|(op<<29)|(imm5<<16)|(imm4<<11)|(1<<5))
 for op in range(8):
  for rn,rm in [(1,2),(0,0),(31,31)]:words.add(0x0e201c00|(q<<30)|((op>>2)<<29)|((op&3)<<22)|(rm<<16)|(rn<<5))
 for offset in range(16):words.add(0x2e000000|(q<<30)|(2<<16)|(offset<<11)|(1<<5))
for imm5 in range(32):
 for imm4 in range(16):words.add(0x5e000400|(imm5<<16)|(imm4<<11)|(1<<5))
for ftype in range(4):
 words.add(0x1e204020|(ftype<<22))
 for imm in range(256):words.add(0x1e201000|(ftype<<22)|(imm<<13))
 for sf in range(2):
  for rmode in range(4):
   for direction in range(2):words.add(0x1e260020|(ftype<<22)|(sf<<31)|(rmode<<19)|(direction<<16))
for size in range(4):
 for order in range(4):
  for operation in range(16):
   for rs,rt in [(0,1),(31,31),(1,31),(31,1)]:
    for rn in [0,31]:words.add(0x38200000|(size<<30)|((order&1)<<23)|((order>>1)<<22)|(operation<<12)|(rs<<16)|(rn<<5)|rt)
  for rs,rt in [(0,1),(31,31),(1,31),(31,1)]:
   for rn in [0,31]:words.add(0x08a07c00|(size<<30)|((order&1)<<22)|((order>>1)<<15)|(rs<<16)|(rn<<5)|rt)
for kind in [0xd503309f,0xd50330bf]:
 for option in range(16):words.add(kind|(option<<8))
for register in [0xd53b4400,0xd53b4420,0xd53b4200,0xd51b4400,0xd51b4420,0xd51b4200,0xd53bd040,0xd53bd060,0xd51bd040]:
 for rt in range(32):words.add(register|rt)
for ftype in range(4):
 for operation in range(16):
  for rn in [0,1,31]:
   for rm in [0,2,31]:words.add(0x1e200800|(ftype<<22)|(operation<<12)|(rm<<16)|(rn<<5))
 for signaling in range(2):
  for zero in range(2):
   for rn in [0,1,31]:
    for rm in [0,2,31]:words.add(0x1e202000|(ftype<<22)|(signaling<<4)|(zero<<3)|(rm<<16)|(rn<<5))
for ftype in range(4):
 for rn in [0,1,31]:
  for rm in [0,2,31]:
   for ra in [0,3,31]:
    for neg in range(2):
     for sub in range(2):words.add(0x1f000000|(ftype<<22)|(rm<<16)|(ra<<10)|(rn<<5)|(neg<<21)|(sub<<15))
for ftype in range(4):
 for condition in range(16):
  for rn in [0,1,31]:
   for rm in [0,2,31]:words.add(0x1e200c00|(ftype<<22)|(condition<<12)|(rm<<16)|(rn<<5))
for sf in range(2):
 for ftype in range(4):
  for opcode in range(8):
   for rounding in range(4):
    for rn in [0,1,31]:
     for rd in [0,31]:words.add(0x1e200000|(sf<<31)|(ftype<<22)|(rounding<<19)|(opcode<<16)|(rn<<5)|rd)
for ftype in range(4):
 for base in [0x1e214000,0x1e20c000,0x1e21c000]:
  for rn in [0,1,31]:words.add(base|(ftype<<22)|(rn<<5))
for sf in range(2):
 for operation in range(8):
  for rn in [0,1,31]:words.add(0x5ac00000|(sf<<31)|(operation<<10)|(rn<<5))
 for sub in range(2):
  for flags in range(2):
   for rn in [0,1,31]:
    for rm in [0,2,31]:words.add(0x1a000000|(sf<<31)|(sub<<30)|(flags<<29)|(rm<<16)|(rn<<5))
for ftype in range(4):
 for signaling in range(2):
  for condition in range(16):
   for nzcv in [0,15]:words.add(0x1e220420|(ftype<<22)|(signaling<<4)|(condition<<12)|nzcv)
for q in range(2):
 for size in range(2):
  for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31),(1,2,1)]:
   for base in [0x0e20d400,0x0ea0d400,0x2e20dc00,0x2e20fc00,0x0e20cc00,0x0ea0cc00,0x0e20f400,0x0ea0f400,0x0e20c400,0x0ea0c400]:
    words.add(base|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
   for base in [0x0ea0f800,0x2ea0f800,0x2ea1f800,0x0e21d800,0x2e21d800,0x0ea1b800,0x2ea1b800]:
    words.add(base|(q<<30)|(size<<22)|(rn<<5)|rd)
for size in range(2):
 for rd,rn in [(0,1),(0,0),(31,31)]:
  for base in [0x5e21d800,0x7e21d800,0x5ea1b800,0x7ea1b800]:words.add(base|(size<<22)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(2):
   for low in range(2):
    for high in range(2):
     for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:
      for base in [0x0f801000,0x0f805000,0x0f809000,0x2f809000]:
       words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(low<<21)|(high<<11)|(rm<<16)|(rn<<5)|rd)
for sf in range(2):
 for size in range(4):
  for c in range(2):
   for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31),(1,2,1)]:words.add(0x1ac04000|(sf<<31)|(size<<10)|(c<<12)|(rm<<16)|(rn<<5)|rd)
for source in range(4):
 for dest in range(4):
  for rd,rn in [(0,1),(0,0),(31,31)]:words.add(0x1e224000|(source<<22)|(dest<<15)|(rn<<5)|rd)
for q in range(2):
 for size in range(4):
  for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31),(1,2,1)]:
   for operation in [1,2,3,5,6,7]:words.add(0x0e000800|(q<<30)|(size<<22)|(operation<<12)|(rm<<16)|(rn<<5)|rd)
for q in range(2):
 for size in range(2):
  for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:words.add(0x2e20d400|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for size in range(2):
 for rd,rn in [(0,1),(0,0),(31,31)]:words.add(0x7e30d800|(size<<22)|(rn<<5)|rd)
for q in range(2):
 for post in range(2):
  for load in range(2):
   for r in range(2):
    for opcode in range(8):
     for lane_s in range(2):
      for size in range(4):
       for rm in [0,2,31]:
        for rt,rn in [(0,1),(31,31),(0,2)]:words.add(0x0d000000|(q<<30)|(post<<23)|(load<<22)|(r<<21)|(rm<<16)|(opcode<<13)|(lane_s<<12)|(size<<10)|(rn<<5)|rt)
ordered=sorted(words)
result=subprocess.run([str(root/'target/release/mkw-recomp'),'decode-words'],input=''.join(json.dumps({'word':w,'pc':0x1000})+'\n'for w in ordered),text=True,capture_output=True,check=True)
rows=[json.loads(line)for line in result.stdout.splitlines()]
assert len(rows)==len(ordered)
decoder=Cs(CS_ARCH_ARM64,CS_MODE_LITTLE_ENDIAN);decoder.detail=True
counts=collections.Counter();errors=[]
memory_names={'ldr','str','ldur','stur','ldp','stp','ldnp','stnp','ldpsw','ldrsw','ldursw','ldrsb','ldrsh','ldursb','ldursh','ldrb','ldrh','ldurb','ldurh','strb','strh','sturb','sturh'}
for row in rows:
 if 'error'in row:continue
 word=row['word'];decoded=list(decoder.disasm(word.to_bytes(4,'little'),0x1000));op=row['instruction']['op'];kind=next(iter(op))if isinstance(op,dict)else op;counts[kind]+=1
 if len(decoded)!=1:errors.append({'word':hex(word),'error':'accepted encoding not decoded by Capstone'});continue
 ins=decoded[0]
 if kind=='Structure':
  v=op['Structure'];expected='ld1r' if v['replicate'] else 'ld1' if v['load'] else 'st1'
  if ins.mnemonic!=expected:errors.append({'word':hex(word),'error':'single structure family differs','reference':ins.mnemonic+' '+ins.op_str})
  if v['replicate']:
   prefix='{v'+str(v['vector'])+'.'+str(v['replicate']//v['bytes'])+{1:'b',2:'h',4:'s',8:'d'}[v['bytes']]+'}'
   if not ins.op_str.startswith(prefix):errors.append({'word':hex(word),'error':'replication arrangement differs','reference':ins.op_str})
  else:
   prefix='{v'+str(v['vector'])+'.'+{1:'b',2:'h',4:'s',8:'d'}[v['bytes']]+'}'
   if not ins.op_str.startswith(prefix):errors.append({'word':hex(word),'error':'structure register/element width differs','reference':ins.op_str})
   if ins.operands[0].vector_index!=v['lane']:errors.append({'word':hex(word),'error':'structure lane differs','reference':ins.op_str})
 elif kind=='Crc32':
  expected={1:('crc32b','crc32cb'),2:('crc32h','crc32ch'),4:('crc32w','crc32cw'),8:('crc32x','crc32cx')}
  v=op['Crc32']
  if ins.mnemonic!=expected[v['bytes']][int(v['castagnoli'])]:errors.append({'word':hex(word),'error':'CRC variant differs','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='VectorFp':
  v=op['VectorFp']
  expected={'fadd','fsub','fmul','fdiv','fmla','fmls','faddp','fmax','fmin','fmaxnm','fminnm','fabs','fneg','fsqrt','scvtf','ucvtf','fcvtzs','fcvtzu'}
  if ins.mnemonic not in expected:errors.append({'word':hex(word),'error':'vector numeric family differs','reference':ins.mnemonic+' '+ins.op_str})
  scalar=ins.op_str.startswith(('s','d'))
  size=8 if ((word>>22)&1) else 4
  total=size if scalar else (16 if ((word>>30)&1) else 8)
  if (v['bytes'],v['total'])!=(size,total):errors.append({'word':hex(word),'error':'vector numeric arrangement differs','reference':ins.op_str})
  if v['rhs_lane'] is not None:
   if ins.operands[-1].vector_index!=v['rhs_lane']:errors.append({'word':hex(word),'error':'indexed source lane differs','reference':ins.op_str})
   if (word>>16)&31 != v['rhs']:errors.append({'word':hex(word),'error':'indexed source register differs'})
 elif kind=='ConditionalFloat':
  if ins.mnemonic not in {'fccmp','fccmpe'}:errors.append({'word':hex(word),'error':'conditional FP family differs','reference':ins.mnemonic+' '+ins.op_str})
 elif kind in {'IntegerUnary','CarryArithmetic'}:
  expected={'IntegerUnary':{'rbit','rev16','rev32','rev','clz','cls'},'CarryArithmetic':{'adc','adcs','sbc','sbcs','ngc','ngcs'}}[kind]
  if ins.mnemonic not in expected:errors.append({'word':hex(word),'error':'integer bit/carry family differs','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='Convert':
  if ins.mnemonic not in {'scvtf','ucvtf','fcvtns','fcvtnu','fcvtps','fcvtpu','fcvtms','fcvtmu','fcvtzs','fcvtzu','fcvtas','fcvtau'}:errors.append({'word':hex(word),'error':'conversion family differs','reference':ins.mnemonic+' '+ins.op_str})
 elif kind in {'Floating','FloatSelect','SystemRegister','Barrier'}:
  expected={'FloatSelect':{'fcsel'},'Floating':{'fadd','fsub','fmul','fdiv','fcmp','fcmpe','fmadd','fneg','fabs','fsqrt','fnmul','fmsub','fnmadd','fnmsub','fcvt','fmax','fmin','fmaxnm','fminnm'},'SystemRegister':{'mrs','msr'},'Barrier':{'dmb','dsb'}}[kind]
  if ins.mnemonic not in expected:errors.append({'word':hex(word),'error':'FP/system/barrier family differs','reference':ins.mnemonic+' '+ins.op_str})
  if kind=='Floating' and op['Floating']['operation']==14:
   source='s' if op['Floating']['bytes']==4 else 'd'
   destination='d' if source=='s' else 's'
   if decoder.reg_name(ins.operands[0].reg)[0]!=destination or decoder.reg_name(ins.operands[1].reg)[0]!=source:errors.append({'word':hex(word),'error':'format conversion direction differs','reference':ins.op_str})
 elif kind=='OrderedMemory':
  if ins.mnemonic not in {'ldapr','ldaprb','ldaprh'}:errors.append({'word':hex(word),'error':'ordered-memory family differs','reference':ins.mnemonic})
 elif kind=='Atomic':
  if not ins.mnemonic.startswith(('ldadd','ldclr','ldeor','ldset','ldsmax','ldsmin','ldumax','ldumin','swp','cas','stadd','stclr','steor','stset','stsmax','stsmin','stumax','stumin')):errors.append({'word':hex(word),'error':'atomic family differs','reference':ins.mnemonic+' '+ins.op_str})
  register=decoder.reg_name(ins.operands[0].reg)
  size=1 if ins.mnemonic.endswith('b')else 2 if ins.mnemonic.endswith('h')else 4 if register.startswith('w')else 8
  if size!=op['Atomic']['bytes']:errors.append({'word':hex(word),'error':'atomic width differs','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='Simd':
  expected={'fmov','mov','movi','mvni','orr','bic','dup','umov','smov','and','orn','eor','bsl','bit','bif','ext','zip1','zip2','uzp1','uzp2','trn1','trn2'}
  if ins.mnemonic not in expected:errors.append({'word':hex(word),'error':'SIMD family differs','reference':ins.mnemonic+' '+ins.op_str})
  if isinstance(op['Simd'],dict) and 'Permute' in op['Simd']:
   v=op['Simd']['Permute'];name=[['uzp1','uzp2'],['trn1','trn2'],['zip1','zip2']][v['kind']][v['part']]
   arrangement=str(v['vector_bytes']//v['bytes'])+{1:'b',2:'h',4:'s',8:'d'}[v['bytes']]
   operands=', '.join('v'+str(v[k])+'.'+arrangement for k in ['dst','lhs','rhs'])
   if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'permutation arrangement/registers differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='Logical':
  if ins.mnemonic != 'and':errors.append({'word':hex(word),'error':'logical family differs','reference':ins.mnemonic})
  bits=64 if word>>31 else 32
  immediate=ins.operands[-1].imm & ((1<<bits)-1)
  if immediate!=op['Logical']['rhs']['Immediate']:errors.append({'word':hex(word),'error':'logical immediate differs'})
 elif kind=='Bitfield':
  if ins.mnemonic not in {'sbfm','sbfiz','sbfx','asr','sxtb','sxth','sxtw'}:errors.append({'word':hex(word),'error':'bitfield family differs','reference':ins.mnemonic})
 elif kind in {'ConditionalCompare','IndirectBranch','Multiply','Divide'}:
  expected={'ConditionalCompare':{'ccmp','ccmn'},'IndirectBranch':{'br'},'Multiply':{'mul','mneg','madd','msub','smull','smnegl','smaddl','smsubl','umull','umnegl','umaddl','umsubl','smulh','umulh'},'Divide':{'udiv','sdiv'}}[kind]
  if ins.mnemonic not in expected:errors.append({'word':hex(word),'error':'integer/control family differs','reference':ins.mnemonic})
 elif kind=='Memory':
  transfer=op['Memory'];name=decoder.reg_name(ins.operands[0].reg);vector='Vector'in transfer['first']
  expected_vector=name[0]in'bh sdq'.replace(' ','')
  if ins.mnemonic not in memory_names or vector!=expected_vector:errors.append({'word':hex(word),'error':'transfer family/bank differs','reference':ins.mnemonic+' '+ins.op_str});continue
  size=({'b':1,'h':2,'s':4,'d':8,'q':16}[name[0]]if vector else 1 if ins.mnemonic.endswith('b')else 2 if ins.mnemonic.endswith('h')else 4 if ins.mnemonic.endswith('sw')or name.startswith('w')else 8)
  if size!=transfer['bytes']:errors.append({'word':hex(word),'error':'transfer width differs','reference':ins.mnemonic+' '+ins.op_str,'bytes':transfer['bytes']})
report={'reference':f'Capstone {__version__}','encoding_cases':len(ordered),'accepted':sum(counts.values()),'accepted_families':dict(counts),'errors':errors,'hardware_reference':False}
folder=root/'private/testing/decoder-reference';folder.mkdir(parents=True,exist_ok=True);(folder/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items()if k!='errors'}));assert not errors,errors[:8]

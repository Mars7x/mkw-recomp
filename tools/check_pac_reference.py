#!/usr/bin/env python3
"""Compare the attributed QARMA adapter to a privately supplied QEMU source.

The generated GPL reference harness and combined binary remain under private/.
No QEMU implementation is included in the public project or normal build.
"""
from pathlib import Path
import hashlib
import json
import subprocess

root=Path(__file__).resolve().parents[1]
source=root/'private/tools/qemu-pauth-reference.c'
text=source.read_text()
body=text[text.index('static uint64_t pac_cell_shuffle'):text.index('static uint64_t pauth_computepac_impdef')]
notice=text[:text.index('#include')]
folder=root/'private/testing/qarma-reference'
folder.mkdir(parents=True,exist_ok=True)
wrapper=notice+'''
#include <stdint.h>
#include <stdbool.h>
#include <stdio.h>
typedef struct {uint64_t lo,hi;} ARMPACKey;
static uint64_t extract64(uint64_t v,int start,int count){return (v>>start)&(UINT64_MAX>>(64-count));}
static uint32_t extract32(uint32_t v,int start,int count){return (v>>start)&(UINT32_MAX>>(32-count));}
#define MAKE_64BIT_MASK(start,count) ((UINT64_MAX>>(64-(count)))<<(start))
'''+body+'''
#include "pac.h"
static uint64_t rng(uint64_t *state){uint64_t x=*state;x^=x<<13;x^=x>>7;x^=x<<17;return *state=x;}
int main(void){uint64_t state=UINT64_C(0x123456789abcdef);
 for(unsigned i=0;i<4096;i++){
  uint64_t data=rng(&state),modifier=rng(&state);ARMPACKey key={.lo=rng(&state),.hi=rng(&state)};
  if(mkw_qarma5(data,modifier,key.hi,key.lo)!=pauth_computepac_architected(data,modifier,key,false))return 1;
 }
 puts("4096 QARMA5 comparisons passed against independent QEMU implementation");return 0;}
'''
(folder/'reference.c').write_text(wrapper)
command=['cc','-std=c11','-O2','-fsanitize=undefined','-fno-sanitize-recover=undefined','-I',str(root/'native'),str(folder/'reference.c'),str(root/'native/pac.c'),str(root/'third_party/gem5-qarma/qarma.c'),'-o',str(folder/'compare')]
build=subprocess.run(command,capture_output=True,text=True)
if build.returncode:raise SystemExit(build.stderr)
run=subprocess.run([str(folder/'compare')],capture_output=True,text=True)
receipt={'reference_sha256':hashlib.sha256(source.read_bytes()).hexdigest(),'reference_commit':(root/'private/tools/qemu-pauth-revision.txt').read_text(),'command':command,'return_code':run.returncode,'stdout':run.stdout,'stderr':run.stderr,'reference_hardware':False}
(folder/'receipt.json').write_text(json.dumps(receipt,indent=2)+'\n')
print(run.stdout,end='')
raise SystemExit(run.returncode)

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
  for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:
   for base in [0x2e20d400,0x2e20f400,0x2ea0f400,0x2e20c400,0x2ea0c400]:words.add(base|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for size in range(2):
 for rd,rn in [(0,1),(0,0),(31,31)]:
  for base in [0x7e30d800,0x7e30f800,0x7eb0f800,0x7e30c800,0x7eb0c800]:words.add(base|(size<<22)|(rn<<5)|rd)
for q in range(2):
 for post in range(2):
  for load in range(2):
   for r in range(2):
    for opcode in range(8):
     for lane_s in range(2):
      for size in range(4):
       for rm in [0,2,31]:
        for rt,rn in [(0,1),(31,31),(0,2)]:words.add(0x0d000000|(q<<30)|(post<<23)|(load<<22)|(r<<21)|(rm<<16)|(opcode<<13)|(lane_s<<12)|(size<<10)|(rn<<5)|rt)
for scalar in range(2):
 for q in range(2):
  for size in range(2):
   for base in [0x0e20e400,0x2e20e400,0x2ea0e400,0x2e20ec00,0x2ea0ec00]:
    for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
   for base in [0x0ea0d800,0x2ea0c800,0x0ea0c800,0x2ea0d800,0x0ea0e800]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rn<<5)|rd)
for q in range(2):
 for size in range(4):
  for u in range(2):
   for count in range(1,5):
    for extension in range(2):
     for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31),(0,30,31)]:words.add(0x0e000000|(q<<30)|(size<<22)|(u<<29)|((count-1)<<13)|(extension<<12)|(rm<<16)|(rn<<5)|rd)
   for rd,rn in [(0,1),(0,0),(31,31)]:words.add(0x0e205800|(q<<30)|(size<<22)|(u<<29)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(4):
   for base in [0x0e206400,0x0e206c00,0x2e206400,0x2e206c00,0x0e208400,0x2e208400,0x2e208c00,0x0e208c00,0x0e203c00,0x0e203400,0x2e203c00,0x2e203400]:
    for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31),(1,2,1)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
   for base in [0x0e209800,0x2e208800,0x0e208800,0x2e209800,0x0e20a800,0x2e20b800]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rn<<5)|rd)
for q in range(2):
 for size in range(4):
  for base in [0x0e303800,0x2e303800,0x0e31b800,0x0e30a800,0x0e31a800,0x2e30a800,0x2e31a800]:
   for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(q<<30)|(size<<22)|(rn<<5)|rd)
for ty in range(4):
 for base in [0x1e244000,0x1e24c000,0x1e254000,0x1e25c000,0x1e264000,0x1e274000,0x1e27c000]:
  for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(ty<<22)|(rn<<5)|rd)
for q in range(2):
 for size in range(2):
  for base in [0x0e218800,0x0ea18800,0x0e219800,0x0ea19800,0x2e218800,0x2e219800,0x2ea19800]:
   for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(q<<30)|(size<<22)|(rn<<5)|rd)
for sf in range(2):
 for ty in range(4):
  for op in [0,1,2,3]:
   for rounding in range(4):
    for scale in range(64):words.add(0x1e000020|(sf<<31)|(ty<<22)|(op<<16)|(rounding<<19)|(scale<<10))
for scalar in range(2):
 for q in range(2):
  for imm in range(128):
   for base in [0x0f00e400,0x2f00e400,0x0f00fc00,0x2f00fc00]:words.add(base|(scalar<<28)|(q<<30)|(imm<<16)|(1<<5))
for scalar in range(2):
 for q in range(2):
  for size in range(2):
   for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:words.add(0x2ea0d400|(scalar<<28)|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for q in range(2):
 for size in range(4):
  for base in [0x0e200800,0x2e200800,0x0e201800,0x2e205800,0x2e605800]:
   for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(q<<30)|(size<<22)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for imm in range(128):
   for base in [0x0f005400,0x2f005400,0x0f000400,0x2f000400,0x2f004400,0x0f00a400,0x2f00a400]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(imm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(4):
   for rd,rn in [(0,1),(0,0),(31,31)]:words.add(0x2e213800|(scalar<<28)|(q<<30)|(size<<22)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(4):
   for base in [0x0e212800,0x0e214800,0x2e212800,0x2e214800]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(2):
   for base in [0x2e30f800,0x2eb0f800,0x2e30c800,0x2eb0c800]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rn<<5)|rd)
for q in range(2):
 for post in range(2):
  for load in range(2):
   for reserved in range(2):
    for opcode in range(16):
     for size in range(4):
      for rm in [0,1,2,31]:
       for rt,rn in [(0,1),(31,31),(31,1)]:words.add(0x0c000000|(q<<30)|(post<<23)|(load<<22)|(reserved<<21)|(rm<<16)|(opcode<<12)|(size<<10)|(rn<<5)|rt)
for scalar in range(2):
 for q in range(2):
  for imm in range(128):
   for base in [0x0f008400,0x0f008c00,0x2f008400,0x2f008c00]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(imm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(4):
   for base in [0x0e20b400,0x2e20b400]:
    for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31),(1,2,1)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(4):
   for h in range(2):
    for l in range(2):
     for m in range(2):
      for base in [0x0f00c000,0x0f00d000]:
       for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,15)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(h<<11)|(l<<21)|(m<<20)|(rm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for u in range(2):
   for size in range(4):
    for base in [0x0e20c000,0x0e208000,0x0e20a000]:
     for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:words.add(base|(scalar<<28)|(q<<30)|(u<<29)|(size<<22)|(rm<<16)|(rn<<5)|rd)
    for base in [0x0f00a000,0x0f002000,0x0f006000]:
     for index in range(8):
      for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,15)]:words.add(base|(scalar<<28)|(q<<30)|(u<<29)|(size<<22)|((index>>2)<<11)|(((index>>1)&1)<<21)|((index&1)<<20)|(rm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(4):
   for base in [0x0e200c00,0x2e200c00,0x0e202c00,0x2e202c00]:
    for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31),(1,2,1)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(4):
   for base in [0x0e214800,0x2e214800,0x2e212800]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for size in range(4):
   for base in [0x0e204400,0x2e204400]:
    for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31),(1,2,1)]:words.add(base|(scalar<<28)|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for imm in range(128):
   for base in [0x0f009400,0x0f009c00,0x2f009400,0x2f009c00,0x2f008400,0x2f008c00]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(imm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for u in range(2):
   for size in range(4):
    for base in [0x0e200000,0x0e202000,0x0e201000,0x0e203000]:
     for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:words.add(base|(scalar<<28)|(q<<30)|(u<<29)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for imm in range(128):
   for base in [0x0f001400,0x2f001400,0x0f002400,0x2f002400,0x0f003400,0x2f003400]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(scalar<<28)|(q<<30)|(imm<<16)|(rn<<5)|rd)
for q in range(2):
 for size in range(4):
  for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:words.add(0x0e20bc00|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for size in range(4):
 for rd,rn in [(0,1),(0,0),(31,31)]:words.add(0x5e31b800|(size<<22)|(rn<<5)|rd)
for scalar in range(2):
 for q in range(2):
  for u in range(2):
   for size in range(4):
    for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,31)]:words.add(0x0e209c00|(scalar<<28)|(q<<30)|(u<<29)|(size<<22)|(rm<<16)|(rn<<5)|rd)
   for size in range(4):
    for index in range(8):
     for rd,rn,rm in [(0,1,2),(0,0,0),(31,31,15)]:words.add(0x0f008000|(scalar<<28)|(q<<30)|(u<<29)|(size<<22)|((index>>2)<<11)|(((index>>1)&1)<<21)|((index&1)<<20)|(rm<<16)|(rn<<5)|rd)
words.update([0xd503201f,0xd503203f,0xd503205f,0xd503207f,0xd503209f,0xd50320bf])
for q in range(2):
 for scalar in range(2):
  for size in range(4):
   for base in [0x0e284800,0x0e285800,0x0e286800,0x0e287800]:
    for rd,rn in [(0,1),(0,0),(31,31)]:words.add(base|(q<<30)|(scalar<<28)|(size<<22)|(rn<<5)|rd)
for hint in range(32):
 for rn in [1,31]:
  for imm in [0,1,4095]:words.add(0xf9800000|(imm<<10)|(rn<<5)|hint)
  for imm in [0,1,255,256,511]:words.add(0xf8800000|(imm<<12)|(rn<<5)|hint)
  for option in range(8):
   for shift in range(2):
    for rm in [2,31]:words.add(0xf8a00800|(rm<<16)|(option<<13)|(shift<<12)|(rn<<5)|hint)
 for imm in [0,1,0x3ffff,0x40000,0x7ffff]:words.add(0xd8000000|(imm<<5)|hint)
for q in range(2):
 for size in range(4):
  for base in [0x0e200400,0x0e202400,0x2e200400]:
   for rn in [0,1,31]:
    for rm in [0,2,31]:
     for rd in [0,1,31]:words.add(base|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for rd in range(32):
 for base in [0xdac143e0,0xdac147e0,0xdac143c0]:words.add(base|rd)
for q in range(2):
 for size in range(4):
  for rn in [0,1,31]:
   for rm in [0,2,15,31]:
    for rd in [0,1,31]:
     words.add(0x0e209400|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
     for index in range(8):words.add(0x2f000000|(q<<30)|(size<<22)|((index>>2)<<11)|(((index>>1)&1)<<21)|((index&1)<<20)|(rm<<16)|(rn<<5)|rd)
for q in range(2):
 for size in range(2):
  for rn in [0,1,31]:
   for rd in [0,1,31]:words.add(0x0ea1d800|(q<<30)|(size<<22)|(rn<<5)|rd)
for q in range(2):
 for size in range(4):
  for rn in [0,1,31]:
   for rd in [0,1,31]:words.add(0x2e204800|(q<<30)|(size<<22)|(rn<<5)|rd)
for q in range(2):
 for size in range(2):
  for rn in [0,1,31]:
   for rd in [0,1,31]:
    words.add(0x2ea1d800|(q<<30)|(size<<22)|(rn<<5)|rd)
    for rm in [0,2,31]:words.add(0x0ea0fc00|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
for rn in [0,1,31]:
 for rd in [0,1,31]:
  for q in range(2):
   for rm in [0,7,31]:
    for base in [0x4ea05400,0x4ee05400,0x4ea05c00]:words.add(base|(rm<<16)|(rn<<5)|rd)
    for size in range(4):
     words.add(0x0e209000|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
     for lane in range(8 if size==1 else 4):
      lane_bits=((lane>>2)<<11)|(((lane>>1)&1)<<21)|((lane&1)<<20)if size==1 else((lane>>1)<<11)|((lane&1)<<21)
      words.add(0x0f003000|(q<<30)|(size<<22)|lane_bits|((rm&(15 if size==1 else 31))<<16)|(rn<<5)|rd)
    for imm in range(128):words.add(0x0f008c00|(q<<30)|(imm<<16)|(rn<<5)|rd)
    words.add(0x4e606800|(rn<<5)|rd)
    words.add(0xdac10400|(rn<<5)|rd)
    for base in [0x0e20fc00,0x0e60fc00]:words.add(base|(q<<30)|(rm<<16)|(rn<<5)|rd)
    for base in [0x5e000000,0x5e001000,0x5e002000,0x5e003000,0x5e004000,0x5e005000,0x5e006000]:
     for delta in [0,1<<30,1<<22]:words.add((base|(rm<<16)|(rn<<5)|rd)^delta)
    for base in [0x5e280800,0x5e281800,0x5e282800]:
     for delta in [0,1<<30,1<<22]:words.add((base|(rn<<5)|rd)^delta)
    for size in range(4):
     for base in [0x2e207000,0x2e207400]:words.add(base|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
    for size in range(4):words.add(0x0e20e000|(q<<30)|(size<<22)|(rm<<16)|(rn<<5)|rd)
   for size in range(4):words.add(0x0e217800|(q<<30)|(size<<22)|(rn<<5)|rd)
  for base in [0x0ea0a400,0x0ea0ac00]:
   for rm in [0,2,31]:words.add(base|(rm<<16)|(rn<<5)|rd)
  for q in range(2):
   for size in range(4):words.add(0x0e202800|(q<<30)|(size<<22)|(rn<<5)|rd)
  for imm in range(128):words.add(0x4f007400|(imm<<16)|(rn<<5)|rd)
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
 if kind=='SimdShift':
  v=op['SimdShift'];k=v['kind'];widen=k in ['SignedWiden','UnsignedWiden'];scalar=bool(word&(1<<28));letters={1:'b',2:'h',4:'s',8:'d'}
  if k=='Narrow':
   name='shrn2' if v['upper'] else 'shrn';b=v['bytes']
   dest='v'+str(v['dst'])+'.'+str((16 if v['upper'] else 8)//b)+letters[b]
   source='v'+str(v['src'])+'.'+str(8//b)+letters[b*2]
  elif widen:
   name=('shll' if v['amount']==v['bytes']*8 else 'sshll' if k=='SignedWiden' else 'ushll')+('2' if v['upper'] else '')
   dest='v'+str(v['dst'])+'.'+str(16//(v['bytes']*2))+letters[v['bytes']*2]
   source='v'+str(v['src'])+'.'+str((16 if v['upper'] else 8)//v['bytes'])+letters[v['bytes']]
  else:
   name={'Left':'shl','UnsignedRight':'ushr','SignedRight':'sshr','InsertLeft':'sli','InsertRight':'sri'}[k]
   reg=lambda n: 'd'+str(n) if scalar else 'v'+str(n)+'.'+str(v['total']//v['bytes'])+letters[v['bytes']]
   dest,source=reg(v['dst']),reg(v['src'])
  aliases={name}
  if widen and v['amount']==0:aliases.add(('sxtl' if k=='SignedWiden' else 'uxtl')+('2' if v['upper'] else ''))
  if ins.mnemonic not in aliases or not ins.op_str.startswith(dest+', '+source):errors.append({'word':hex(word),'error':'SIMD shift kind/arrangement/registers differ','reference':ins.mnemonic+' '+ins.op_str})
  elif ins.mnemonic==name and ins.operands[-1].imm!=v['amount']:errors.append({'word':hex(word),'error':'SIMD shift amount differs','reference':ins.op_str})
 elif kind=='YieldHint':
  if word!=0xd503203f or ins.mnemonic!='yield':errors.append({'word':hex(word),'error':'YIELD exact hint differs'})
 elif kind=='PointerAuth':
  v=op['PointerAuth']
  if word&0xffffffe0==0xdac143e0:
   reg='xzr' if v['register']=='Zero' else 'x'+str(v['register']['Gpr'])
   if (ins.mnemonic,ins.op_str)!=('xpaci',reg) or v['operation']!=2 or v['key']!=0 or v['modifier']!='Zero':errors.append({'word':hex(word),'error':'XPACI operands/profile differ','reference':ins.mnemonic+' '+ins.op_str})
  if word&0xfffffc00==0xdac10400:
   reg=lambda r:'sp'if r=='Sp'else'xzr'if r=='Zero'else'x'+str(r['Gpr']);operands=reg(v['register'])+', '+reg(v['modifier'])
   if(ins.mnemonic,ins.op_str)!=('pacib',operands)or v['operation']!=0 or v['key']!=1:errors.append({'word':hex(word),'error':'PACIB operands/profile differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='UnsignedAbsDifference':
  v=op[kind];b=v['bytes'];letters={1:'b',2:'h',4:'s',8:'d'};reg=lambda r,b,t:'v'+str(r)+'.'+str(t//b)+letters[b];name='uabdl2'if v['upper']else'uabdl'if v['long']else'uabd';source_total=16 if v['upper']else 8 if v['long']else v['total'];operands=reg(v['dst'],b*2 if v['long']else b,v['total'])+', '+reg(v['lhs'],b,source_total)+', '+reg(v['rhs'],b,source_total)
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'unsigned absolute operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='RoundedRegisterShift':
  v=op[kind];shape='.4s'if v['bytes']==4 else'.2d';name='sqrshl'if v['saturating']else'srshl';operands=', '.join('v'+str(v[k])+shape for k in ['dst','lhs','rhs'])
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'rounded register shift operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='LongSignedPairAccumulate':
  v=op[kind];operands='v'+str(v['dst'])+'.4s, v'+str(v['src'])+'.8h'
  if (ins.mnemonic,ins.op_str)!=('sadalp',operands):errors.append({'word':hex(word),'error':'long pair accumulate differs','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='RoundedShiftNarrow':
  v=op[kind];b=v['bytes'];letters={1:'b',2:'h',4:'s',8:'d'};name='rshrn2'if v['upper']else'rshrn';total=16 if v['upper']else 8;prefix=f"v{v['dst']}.{total//b}{letters[b]}, v{v['src']}.{8//b}{letters[b*2]}, "
  if ins.mnemonic!=name or not ins.op_str.startswith(prefix)or ins.operands[-1].imm!=v['amount']:errors.append({'word':hex(word),'error':'rounded narrow operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='SaturatingLongAccumulate':
  v=op[kind];b=v['bytes'];letters={2:'h',4:'s',8:'d'};total=16 if v['upper']else 8;name='sqdmlal2'if v['upper']else'sqdmlal';right=f"v{v['rhs']}.{total//b}{letters[b]}"if v['rhs_lane']is None else f"v{v['rhs']}.{letters[b]}[{v['rhs_lane']}]";operands=f"v{v['dst']}.{8//b}{letters[b*2]}, v{v['lhs']}.{total//b}{letters[b]}, {right}"
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'saturating long operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='ReciprocalStep':
  v=op[kind];shape='.4s'if v['total']==16 else'.2s';operands=', '.join('v'+str(v[k])+shape for k in ['dst','lhs','rhs'])
  if (ins.mnemonic,ins.op_str)!=('frecps',operands):errors.append({'word':hex(word),'error':'reciprocal step operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='SecureHash':
  v=op[kind];names={'Sha1Choose':'sha1c','Sha1Parity':'sha1p','Sha1Majority':'sha1m','Sha1Rotate':'sha1h','Sha1Schedule0':'sha1su0','Sha1Schedule1':'sha1su1','Sha256Hash':'sha256h','Sha256Hash2':'sha256h2','Sha256Schedule0':'sha256su0','Sha256Schedule1':'sha256su1'};name=names[v['kind']];d=v['dst'];n=v['lhs'];m=v['rhs']
  if name=='sha1h':operands=f's{d}, s{n}'
  elif name in ['sha1c','sha1m','sha1p']:operands=f'q{d}, s{n}, v{m}.4s'
  elif name in ['sha256h','sha256h2']:operands=f'q{d}, q{n}, v{m}.4s'
  elif name in ['sha1su1','sha256su0']:operands=f'v{d}.4s, v{n}.4s'
  else:operands=f'v{d}.4s, v{n}.4s, v{m}.4s'
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'hash operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='PolynomialMultiply':
  v=op[kind];name='pmull2'if v['upper']else'pmull';shape='.2d'if v['upper']else'.1d';operands='v'+str(v['dst'])+'.1q, v'+str(v['lhs'])+shape+', v'+str(v['rhs'])+shape
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'polynomial operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='WidenSingleVector':
  v=op[kind];name='fcvtl2'if v['upper']else'fcvtl';shape='.4s'if v['upper']else'.2s';operands='v'+str(v['dst'])+'.2d, v'+str(v['src'])+shape
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'single widening operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='VectorLeadingZeros':
  v=op['VectorLeadingZeros'];operands='v'+str(v['dst'])+'.2s, v'+str(v['src'])+'.2s'
  if (ins.mnemonic,ins.op_str)!=('clz',operands)or(v['bytes'],v['total'])!=(4,8):errors.append({'word':hex(word),'error':'2S CLZ kind/operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind in {'PairwiseMinMax','PairwiseLongSum','SaturatingShiftImmediate'}:
  v=op[kind];letters={1:'b',2:'h',4:'s',8:'d'};reg=lambda n,b,t:'v'+str(n)+'.'+str(t//b)+letters[b];b=v['bytes'];total=v['total']
  if kind=='PairwiseMinMax':
   name=('s'if v['signed']else'u')+('minp'if v['minimum']else'maxp');operands=', '.join(reg(v[k],b,total)for k in ['dst','lhs','rhs'])
  elif kind=='PairwiseLongSum':name='saddlp'if v['signed']else'uaddlp';operands=reg(v['dst'],b*2,total)+', '+reg(v['src'],b,total)
  else:name='sqshl'if v['signed']else'uqshl';operands=reg(v['dst'],b,total)+', '+reg(v['src'],b,total)
  if ins.mnemonic!=name or(kind!='SaturatingShiftImmediate'and ins.op_str!=operands)or(kind=='SaturatingShiftImmediate'and(not ins.op_str.startswith(operands+', ')or ins.operands[-1].imm!=v['amount'])):errors.append({'word':hex(word),'error':'scoped integer operation differs','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='MultiplyAccumulate':
  v=op['MultiplyAccumulate'];b=v['bytes'];letters={1:'b',2:'h',4:'s'};reg=lambda n:'v'+str(n)+'.'+str(v['total']//b)+letters[b]
  operands=reg(v['dst'])+', '+reg(v['lhs'])+', '+(reg(v['rhs']) if v['rhs_lane'] is None else 'v'+str(v['rhs'])+'.'+letters[b]+'['+str(v['rhs_lane'])+']')
  if (ins.mnemonic,ins.op_str)!=('mla',operands):errors.append({'word':hex(word),'error':'MLA kind/operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='HalvingInteger':
  v=op['HalvingInteger'];letters={1:'b',2:'h',4:'s'};name=('s' if v['signed'] else 'u')+('hsub' if v['subtract'] else 'hadd');reg=lambda n:'v'+str(n)+'.'+str(v['total']//v['bytes'])+letters[v['bytes']]
  operands=', '.join(reg(v[k])for k in ['dst','lhs','rhs'])
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'halving kind/operands differ','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='VectorInteger':
  variant,v=next(iter(op['VectorInteger'].items()));letters={1:'b',2:'h',4:'s',8:'d'}
  if variant=='MultiplyLow':
   name='mul';b=v['bytes'];reg=lambda n:'v'+str(n)+'.'+str(v['total']//b)+letters[b]
   operands=reg(v['dst'])+', '+reg(v['lhs'])+', '+(reg(v['rhs']) if v['rhs_lane'] is None else 'v'+str(v['rhs'])+'.'+letters[b]+'['+str(v['rhs_lane'])+']')
  elif variant=='PairwiseAdd':
   name='addp';b=v['bytes'];reg=lambda n:'v'+str(n)+'.'+str(v['total']//b)+letters[b]
   operands=reg(v['dst'])+', '+reg(v['lhs'])+', '+reg(v['rhs']) if v['rhs'] is not None else 'd'+str(v['dst'])+', v'+str(v['lhs'])+'.2d'
  elif variant=='RightShift':
   name=('s' if v['signed'] else 'u')+('r' if v['rounding'] else '')+('sra' if v['accumulate'] else 'shr')
   b=v['bytes'];scalar=bool(word&(1<<28));reg=lambda n:letters[b]+str(n) if scalar else 'v'+str(n)+'.'+str(v['total']//b)+letters[b]
   operands=reg(v['dst'])+', '+reg(v['src'])+', #'+(hex(v['amount']) if v['amount']>=10 else str(v['amount']))
  elif variant=='WidenArithmetic':
   name=('s' if v['signed'] else 'u')+('sub' if v['subtract'] else 'add')+('w' if v['wide_lhs'] else 'l')+('2' if v['upper'] else '')
   b=v['bytes'];dest=lambda n:'v'+str(n)+'.'+str(8//b)+letters[b*2];reg=lambda n:'v'+str(n)+'.'+str((16 if v['upper'] else 8)//b)+letters[b]
   operands=dest(v['dst'])+', '+(dest(v['lhs']) if v['wide_lhs'] else reg(v['lhs']))+', '+reg(v['rhs'])
  elif variant=='SaturatingShiftNarrow':
   name={('Signed',False):'sqshrn',('Signed',True):'sqrshrn',('Unsigned',False):'uqshrn',('Unsigned',True):'uqrshrn',('SignedUnsigned',False):'sqshrun',('SignedUnsigned',True):'sqrshrun'}[v['kind'],v['rounding']]+('2' if v['upper'] else '')
   b=v['bytes'];operands=letters[b]+str(v['dst'])+', '+letters[b*2]+str(v['src']) if v['scalar'] else 'v'+str(v['dst'])+'.'+str((16 if v['upper'] else 8)//b)+letters[b]+', v'+str(v['src'])+'.'+str(8//b)+letters[b*2]
   operands+=', #'+(hex(v['amount']) if v['amount']>=10 else str(v['amount']))
  elif variant=='RegisterShift':
   name='sshl' if v['signed'] else 'ushl';scalar=bool(word&(1<<28));reg=lambda n:letters[v['bytes']]+str(n) if scalar else 'v'+str(n)+'.'+str(v['total']//v['bytes'])+letters[v['bytes']]
   operands=', '.join(reg(v[k]) for k in ['dst','lhs','rhs'])
  elif variant=='SaturatingNarrow':
   name={'Signed':'sqxtn','Unsigned':'uqxtn','SignedUnsigned':'sqxtun'}[v['kind']]+('2' if v['upper'] else '')
   b=v['bytes']
   operands=letters[b]+str(v['dst'])+', '+letters[b*2]+str(v['src']) if v['scalar'] else 'v'+str(v['dst'])+'.'+str((16 if v['upper'] else 8)//b)+letters[b]+', v'+str(v['src'])+'.'+str(8//b)+letters[b*2]
  elif variant=='Saturating':
   name=('sq' if v['signed'] else 'uq')+('sub' if v['subtract'] else 'add');scalar=bool(word&(1<<28))
   reg=lambda n:letters[v['bytes']]+str(n) if scalar else 'v'+str(n)+'.'+str(v['total']//v['bytes'])+letters[v['bytes']]
   operands=', '.join(reg(v[k]) for k in ['dst','lhs','rhs'])
  elif variant=='WidenMultiply':
   name=('s' if v['signed'] else 'u')+{'Product':'mull','Add':'mlal','Subtract':'mlsl'}[v['accumulate']]+('2' if v['upper'] else '')
   b=v['bytes'];reg=lambda n:'v'+str(n)+'.'+str((16 if v['upper'] else 8)//b)+letters[b]
   operands='v'+str(v['dst'])+'.'+str(8//b)+letters[b*2]+', '+reg(v['lhs'])+', '
   operands+=reg(v['rhs']) if v['rhs_lane'] is None else 'v'+str(v['rhs'])+'.'+letters[b]+'['+str(v['rhs_lane'])+']'
  elif variant=='MultiplyHigh':
   name='sqrdmulh' if v['rounding'] else 'sqdmulh';scalar=bool(word&(1<<28))
   register=lambda n:letters[v['bytes']]+str(n) if scalar else 'v'+str(n)+'.'+str(v['total']//v['bytes'])+letters[v['bytes']]
   operands=', '.join(register(v[k]) for k in ['dst','lhs'])+', '
   if v['rhs_lane'] is None:operands+=register(v['rhs'])
   else:operands+='v'+str(v['rhs'])+'.'+letters[v['bytes']]+'['+str(v['rhs_lane'])+']'
  elif variant=='Lanes':
   name={'Add':'add','Subtract':'sub','Negate':'neg','SignedMax':'smax','SignedMin':'smin','UnsignedMax':'umax','UnsignedMin':'umin','Equal':'cmeq','Test':'cmtst','SignedGe':'cmge','SignedGt':'cmgt','UnsignedGe':'cmhs','UnsignedGt':'cmhi','SignedLe':'cmle','SignedLt':'cmlt'}[v['kind']]
   scalar=bool(word&(1<<28));register=lambda n: letters[v['bytes']]+str(n) if scalar else 'v'+str(n)+'.'+str(v['total']//v['bytes'])+letters[v['bytes']]
   operands=register(v['dst'])+', '+register(v['lhs'])
   if v['kind']!='Negate':operands+=', '+(register(v['rhs']) if v['rhs'] is not None else '#0')
  else:
   name={'Sum':'addv','SignedLong':'saddlv','UnsignedLong':'uaddlv','SignedMax':'smaxv','SignedMin':'sminv','UnsignedMax':'umaxv','UnsignedMin':'uminv'}[v['kind']];output=v['bytes']*2 if v['kind'] in {'SignedLong','UnsignedLong'} else v['bytes']
   operands=letters[output]+str(v['dst'])+', v'+str(v['src'])+'.'+str(v['total']//v['bytes'])+letters[v['bytes']]
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'integer SIMD kind/arrangement/operands differ','reference':ins.mnemonic+' '+ins.op_str,'expected':name+' '+operands})
 elif kind=='VectorCompare':
  v=op['VectorCompare'];expected={'Equal':'fcmeq','GreaterEqual':'facge' if v['absolute'] else 'fcmge','Greater':'facgt' if v['absolute'] else 'fcmgt','LessEqual':'fcmle','Less':'fcmlt'}[v['relation']]
  if ins.mnemonic!=expected:errors.append({'word':hex(word),'error':'SIMD FP comparison relation differs','reference':ins.mnemonic+' '+ins.op_str})
  scalar=bool(word&(1<<28));size=8 if word&(1<<22) else 4;total=size if scalar else 16 if word&(1<<30) else 8
  if (v['bytes'],v['total'])!=(size,total):errors.append({'word':hex(word),'error':'SIMD FP comparison width differs','reference':ins.op_str})
  if (v['rhs'] is None)!=('#0.0' in ins.op_str):errors.append({'word':hex(word),'error':'SIMD FP compare-zero differs','reference':ins.op_str})
 elif kind=='InterleavedStructure':
  v=op['InterleavedStructure'];name=('ld' if v['load'] else 'st')+str(v['registers'] if v['interleaved'] else 1);arr=str(v['vector_bytes']//v['bytes'])+{1:'b',2:'h',4:'s',8:'d'}[v['bytes']]
  reg=lambda r: 'sp' if r=='Sp' else 'x'+str(r['Gpr'])
  operands='{'+', '.join('v'+str((v['vector']+n)%32)+'.'+arr for n in range(v['registers']))+'}, ['+reg(v['base'])+']'
  wb=v['writeback']
  if isinstance(wb,dict):operands+=', #'+str(wb['Immediate']) if 'Immediate' in wb else ', '+reg(wb['Register'])
  if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'interleaved structure arrangement/register/writeback differs','reference':ins.mnemonic+' '+ins.op_str,'expected':name+' '+operands})
 elif kind=='Structure':
  v=op['Structure'];name=('ld' if v['load'] else 'st')+str(v['registers'])+('r' if v['replicate'] else '')
  letters={1:'b',2:'h',4:'s',8:'d'};arr=(str(v['replicate']//v['bytes']) if v['replicate'] else '')+letters[v['bytes']]
  operands='{'+', '.join('v'+str((v['vector']+n)%32)+'.'+arr for n in range(v['registers']))+'}'
  if not v['replicate']:operands+='['+(hex(v['lane']) if v['lane']>=10 else str(v['lane']))+']'
  reg=lambda r:'sp' if r=='Sp' else 'x'+str(r['Gpr'])
  operands+=', ['+reg(v['base'])+']';wb=v['writeback']
  if isinstance(wb,dict):operands+=', #'+str(wb['Immediate']) if 'Immediate' in wb else ', '+reg(wb['Register'])
  if(ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'lane/replicate list arrangement/register/writeback differs','reference':ins.mnemonic+' '+ins.op_str,'expected':name+' '+operands})
 elif kind=='Crc32':
  expected={1:('crc32b','crc32cb'),2:('crc32h','crc32ch'),4:('crc32w','crc32cw'),8:('crc32x','crc32cx')}
  v=op['Crc32']
  if ins.mnemonic!=expected[v['bytes']][int(v['castagnoli'])]:errors.append({'word':hex(word),'error':'CRC variant differs','reference':ins.mnemonic+' '+ins.op_str})
 elif kind=='VectorFp':
  v=op['VectorFp']
  expected={'frsqrte','frsqrts','frecpe','fadd','fsub','fmul','fdiv','fmla','fmls','faddp','fmaxp','fminp','fmaxnmp','fminnmp','fmaxv','fminv','fmaxnmv','fminnmv','fabd','fmax','fmin','fmaxnm','fminnm','frintn','frintp','frintm','frintz','frinta','frintx','frinti','fabs','fneg','fsqrt','scvtf','ucvtf','fcvtzs','fcvtzu'}
  if ins.mnemonic not in expected:errors.append({'word':hex(word),'error':'vector numeric family differs','reference':ins.mnemonic+' '+ins.op_str})
  if v['kind'] in ['ReciprocalSqrtEstimate','ReciprocalSqrtStep']:
   name='frsqrte' if v['kind']=='ReciprocalSqrtEstimate' else 'frsqrts';reg=lambda n:'v'+str(n)+'.'+str(v['total']//4)+'s';operands=reg(v['dst'])+', '+reg(v['lhs'])+(', '+reg(v['rhs']) if name=='frsqrts' else '')
   if(ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'reciprocal sqrt kind/operands differ','reference':ins.mnemonic+' '+ins.op_str})
  if v['kind']=='ReciprocalEstimate':
   operands='v'+str(v['dst'])+'.2s, v'+str(v['lhs'])+'.2s'
   if (ins.mnemonic,ins.op_str)!=('frecpe',operands):errors.append({'word':hex(word),'error':'2S estimate operands differ','reference':ins.mnemonic+' '+ins.op_str})
  if isinstance(v['kind'],dict) and 'PairwiseMinMax' in v['kind']:
   name={15:'fmaxp',16:'fminp',17:'fmaxnmp',18:'fminnmp'}[v['kind']['PairwiseMinMax']]
   if ins.mnemonic!=name:errors.append({'word':hex(word),'error':'pairwise minmax kind differs','reference':ins.mnemonic})
  if isinstance(v['kind'],dict) and v['kind'].get('Arithmetic')==26 and ins.mnemonic!='fabd':errors.append({'word':hex(word),'error':'absolute difference differs','reference':ins.mnemonic})
  if isinstance(v['kind'],dict) and 'ReduceMinMax' in v['kind']:
   name={15:'fmaxv',16:'fminv',17:'fmaxnmv',18:'fminnmv'}[v['kind']['ReduceMinMax']]
   operands='s'+str(v['dst'])+', v'+str(v['lhs'])+'.4s'
   if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'FP reduction kind/arrangement/registers differ','reference':ins.mnemonic+' '+ins.op_str})
  scalar=ins.op_str.startswith(('s','d'))
  size=8 if ((word>>22)&1) else 4
  total=size if scalar else (16 if ((word>>30)&1) else 8)
  if (v['bytes'],v['total'])!=(size,total):errors.append({'word':hex(word),'error':'vector numeric arrangement differs','reference':ins.op_str})
  if isinstance(v['kind'],dict) and 'ScaledConversion' in v['kind']:
   if ins.operands[-1].imm!=v['kind']['ScaledConversion']['fractional_bits']:errors.append({'word':hex(word),'error':'SIMD fractional bits differ','reference':ins.op_str})
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
  v=op['Convert']
  if v['fractional_bits'] and ins.operands[-1].imm!=v['fractional_bits']:errors.append({'word':hex(word),'error':'GPR fractional bits differ','reference':ins.op_str})
 elif kind in {'Floating','FloatSelect','SystemRegister','Barrier'}:
  expected={'FloatSelect':{'fcsel'},'Floating':{'fadd','fsub','fmul','fdiv','fcmp','fcmpe','fmadd','fneg','fabs','fsqrt','fnmul','fmsub','fnmadd','fnmsub','fcvt','fmax','fmin','fmaxnm','fminnm','frintn','frintp','frintm','frintz','frinta','frintx','frinti'},'SystemRegister':{'mrs','msr'},'Barrier':{'dmb','dsb'}}[kind]
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
  expected={'fmov','mov','movi','mvni','orr','bic','dup','umov','smov','and','orn','eor','bsl','bit','bif','ext','zip1','zip2','uzp1','uzp2','trn1','trn2','tbl','tbx','cnt','xtn','xtn2','mvn','not','rbit','rev16','rev32','rev64','aese','aesd','aesmc','aesimc'}
  if ins.mnemonic not in expected:errors.append({'word':hex(word),'error':'SIMD family differs','reference':ins.mnemonic+' '+ins.op_str})
  if isinstance(op['Simd'],dict) and 'Crypto' in op['Simd']:
   v=op['Simd']['Crypto'];name={'EncryptRound':'aese','DecryptRound':'aesd','MixColumns':'aesmc','InverseMixColumns':'aesimc'}[v['kind']];operands='v'+str(v['dst'])+'.16b, v'+str(v['src'])+'.16b'
   if(ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'AES operation/registers differ','reference':ins.mnemonic+' '+ins.op_str})
  if isinstance(op['Simd'],dict) and 'Narrow' in op['Simd']:
   v=op['Simd']['Narrow'];letters={1:'b',2:'h',4:'s',8:'d'};b=v['bytes'];name='xtn2' if v['upper'] else 'xtn'
   operands='v'+str(v['dst'])+'.'+str((16 if v['upper'] else 8)//b)+letters[b]+', v'+str(v['src'])+'.'+str(8//b)+letters[b*2]
   if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'narrow arrangement/registers differ','reference':ins.mnemonic+' '+ins.op_str})
  if isinstance(op['Simd'],dict) and 'Transform' in op['Simd']:
   v=op['Simd']['Transform'];k=v['kind'];name='mvn' if k=='Not' else 'rbit' if k=='ReverseBits' else 'rev'+str(k['ReverseElements']*8)
   arr=str(v['vector_bytes']//v['bytes'])+{1:'b',2:'h',4:'s'}[v['bytes']];operands='v'+str(v['dst'])+'.'+arr+', v'+str(v['src'])+'.'+arr
   if (ins.mnemonic,ins.op_str)!=(name,operands):errors.append({'word':hex(word),'error':'SIMD transform arrangement/registers differ','reference':ins.mnemonic+' '+ins.op_str})
  if isinstance(op['Simd'],dict) and 'Table' in op['Simd']:
   v=op['Simd']['Table'];name='tbx' if v['extension'] else 'tbl';arr=str(v['bytes'])+'b'
   table=', '.join('v'+str((v['base']+i)%32)+'.16b' for i in range(v['count']))
   expected_operands='v'+str(v['dst'])+'.'+arr+', {'+table+'}, v'+str(v['index'])+'.'+arr
   if (ins.mnemonic,ins.op_str)!=(name,expected_operands):errors.append({'word':hex(word),'error':'table arrangement/list/registers differ','reference':ins.mnemonic+' '+ins.op_str})
  if isinstance(op['Simd'],dict) and 'ByteCount' in op['Simd']:
   v=op['Simd']['ByteCount'];arr=str(v['bytes'])+'b';expected_operands='v'+str(v['dst'])+'.'+arr+', v'+str(v['src'])+'.'+arr
   if (ins.mnemonic,ins.op_str)!=('cnt',expected_operands):errors.append({'word':hex(word),'error':'byte count arrangement/registers differ','reference':ins.op_str})
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
 elif kind=='Prefetch':
  from capstone.arm64_const import ARM64_EXT_UXTW,ARM64_EXT_SXTW,ARM64_EXT_SXTX
  v=op['Prefetch'];address=v['address'];name='prfum' if v['unscaled'] else 'prfm'
  if ins.mnemonic!=name:errors.append({'word':hex(word),'error':'prefetch family differs','reference':ins.mnemonic})
  if v['hint']!=word&31:errors.append({'word':hex(word),'error':'prefetch hint bits differ'})
  operand=ins.operands[-1]
  reg=lambda r: 'sp' if r=='Sp' else 'xzr' if r=='Zero' else 'x'+str(r['Gpr'])
  if 'Literal' in address:
   if operand.imm&((1<<64)-1)!=address['Literal']:errors.append({'word':hex(word),'error':'prefetch literal address differs'})
  else:
   if decoder.reg_name(operand.mem.base)!=reg(v['base']):errors.append({'word':hex(word),'error':'prefetch base differs'})
   if 'Offset' in address:
    if operand.mem.disp!=address['Offset']:errors.append({'word':hex(word),'error':'prefetch offset differs'})
   elif 'Register' in address:
    a=address['Register'];index=reg(a['index']);index=('w'+index[1:]) if a['extend'] in {2,6} else index
    if decoder.reg_name(operand.mem.index)!=index or operand.shift.value!=a['shift'] or operand.ext!={2:ARM64_EXT_UXTW,3:0,6:ARM64_EXT_SXTW,7:ARM64_EXT_SXTX}[a['extend']]:errors.append({'word':hex(word),'error':'prefetch index/extension/shift differs','reference':ins.op_str})
   else:errors.append({'word':hex(word),'error':'prefetch unexpectedly writes back'})
 elif kind=='Memory':
  transfer=op['Memory'];name=decoder.reg_name(ins.operands[0].reg);vector='Vector'in transfer['first']
  expected_vector=name[0]in'bh sdq'.replace(' ','')
  if ins.mnemonic not in memory_names or vector!=expected_vector:errors.append({'word':hex(word),'error':'transfer family/bank differs','reference':ins.mnemonic+' '+ins.op_str});continue
  size=({'b':1,'h':2,'s':4,'d':8,'q':16}[name[0]]if vector else 1 if ins.mnemonic.endswith('b')else 2 if ins.mnemonic.endswith('h')else 4 if ins.mnemonic.endswith('sw')or name.startswith('w')else 8)
  if size!=transfer['bytes']:errors.append({'word':hex(word),'error':'transfer width differs','reference':ins.mnemonic+' '+ins.op_str,'bytes':transfer['bytes']})
report={'reference':f'Capstone {__version__}','encoding_cases':len(ordered),'accepted':sum(counts.values()),'accepted_families':dict(counts),'errors':errors,'hardware_reference':False}
folder=root/'private/testing/decoder-reference';folder.mkdir(parents=True,exist_ok=True);(folder/'receipt.json').write_text(json.dumps(report,indent=2)+'\n')
print(json.dumps({k:v for k,v in report.items()if k!='errors'}));assert not errors,errors[:8]

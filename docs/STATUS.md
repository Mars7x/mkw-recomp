# Status — October 27, 2026

Ordinary SIMD multiplication, exact YIELD and source snapshot publication guards are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added B/H/S register MUL and H/S indexed-element MUL with raw modular products, complete source snapshots and upper clearing. Independent u128 byte-array expectations cover 46,800 native cases per optimization; 31,200 synthetic QEMU cases match O0/O2 with fatal UBSan.
- Added exact typed YIELD with normal instruction-budget consumption and no diagnostic scheduling effect. Four native budget/state cases per optimization and 16 QEMU cases preserve CPU state; adjacent WFE/WFI/SEV/SEVL remain unsupported. This does not implement guest scheduling or original timing.
- Hardened source-only publication: fingerprint file membership/content before validation and recheck after validation, before copying and against the copied checkout before staging/pushing. Four required synthetic Python tests reject source mutation, added/removed files and changed copies. Private prototype scripts also refuse default main-workspace application.
- The survey accepts 22 further extents: 4 accepted extents contain ordinary SIMD multiplication and 18 contain YIELD (overlap). Four further representative objects compile. MUL and YIELD no longer appear among first blockers.
- ABI v9 and complete multi-register lane/replication lists were validated and published as `571c012`. No new upstream sources were added. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **214,182 indexed extents**, including **126,867 with PAC operations**, up from 214,160 under the same tail policy. **38 accepted extents contain contiguous LD1 lists and 61 contain LD3/LD4 interleaving** (categories overlap). **58,455 contain direct extent tails; 14,159 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **One hundred four representative game-derived samples compile into native objects, 68 with PAC; none execute.**

Native samples now have a 4,096-instruction representative limit. One oversized candidate is explicitly deferred; every observed feature still has a compiled representative in its module. Representative counts can decrease when newly accepted extents combine features previously selected separately. These object counts use the new sampling policy and are not directly comparable with prior first-feature selections.

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **195 Rust tests pass**, including extended memory capacity, multiple structures and stale-ABI refusal checks and earlier scaled conversion/rounding/integer SIMD/FP/memory semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **31,200 ordinary SIMD MUL fixtures** match QEMU at O0/O2; independent u128 byte-array expectations check 46,800 native cases per optimization across register/indexed forms and aliases.
- **16 YIELD fixtures** match QEMU at O0/O2; native shared-budget/state tests check exact accounting, rebased PC, zero memory callbacks and unchanged register/control state.
- **4,080 multi-register lane/replication fixtures** match QEMU at O0/O2; independent byte-array checks cover 12,240 native success and 20,400 fault/permission cases per optimization. Exact 3/6/12-byte capacity, partial final words, stale-v8 refusal and canonical ordered-width negatives pass.
- **414,720 accumulating/rounded right-shift fixtures** match QEMU at O0/O2; independent i128 floor-division/accumulation expectations check 456,192 native cases per optimization, including full-width shifts.
- **7,800 integer ADDP fixtures** match QEMU at O0/O2; independent u128 byte-array expectations check 11,960 native cases per optimization, including scalar reduction and vector pair ordering.
- **612,864 saturating narrowing-shift fixtures** match QEMU at O0/O2; independent i128 floor-division/clamp expectations check 1,225,728 native cases per optimization across every legal amount and rounding/saturation boundary.
- **49,920 widening add/subtract fixtures** match QEMU at O0/O2; independent i128 expectations check 74,880 native cases per optimization, including wide-left and upper-source variants.
- **141,504 saturating add/subtract fixtures** and **10,800 saturating narrowing fixtures** match QEMU at O0/O2; independent i128 clamps check 212,256 and 21,600 native cases per optimization.
- **458,752 register-count shift fixtures** match QEMU at O0/O2; independent bit-copy checks cover 688,128 native cases per optimization, all count bytes, unrelated upper bits and large-magnitude shifts.
- **187,200 widening multiply/accumulate fixtures** match QEMU at O0/O2; independent i128 expectations check 280,800 native cases per optimization across signed/unsigned register/indexed inputs, low/upper halves and accumulator wrap.
- **270,144 SQDMULH/SQRDMULH fixtures** match QEMU at O0/O2; independent i128 product/round/clamp expectations check 405,216 native cases per optimization, including H/S scalar/vector and all indexed lanes, minimum-value saturation and sticky QC.
- **448 SHRN/SHRN2 fixtures** match QEMU at O0/O2; independent byte-to-u128 division expectations check 114,688 native cases per optimization, with every legal amount, aliases, retained/cleared halves and QC preservation.
- **24 raw narrowing fixtures** match QEMU at O0/O2; independent byte-array expectations check 6,144 native cases per optimization, including retained/cleared halves and V31 aliases.
- **64 transform and 34,320 immediate/widening shift fixtures** match QEMU at O0/O2. Independent native oracles cover 16,384 and 68,640 cases per optimization respectively, including all legal amounts, aliases, full-width shifts and upper clearing.
- **327,680 scaled-conversion fixtures** match QEMU at O0/O2 and independent native rational expectations. A separate 98,304-case helper matrix checks exact arithmetic and flags per optimization; malformed-operation and AOT refusal/zero-register tests remain mandatory.
- **22,960 integral-rounding fixtures** match QEMU at O0/O2. Independent native rounding/NaN/flag expectations cover 45,920 cases per optimization, including FRINTX inexact behavior and suppressed IXC in other forms.
- **24,864 integer SIMD fixtures** match QEMU at O0/O2. Independent i128/u128 native oracles check 41,280 cases per optimization, including all-lane extrema, aliases, wrap and widened sums.
- **120 table/count fixtures** match QEMU at O0/O2. Independent native byte-sequence/popcount tests cover 34,816 cases per optimization, including every byte/index value, table wrapping, aliases and upper clearing.
- **16,800 SIMD FP comparison fixtures** match QEMU at O0/O2 across S/D, scalar/vector arrangements, aliases, NaNs, signed zeros, controls and compare-zero forms. Mandatory native tests independently compare ordered inputs and NaN/exception expectations.
- **2,120 complete multiple-structure fixtures** match QEMU at O0/O2, with 6,360 independent native success cases and 10,600 fault/permission cases per optimization. Extended bridge widths, short buffers, bad sizes and stale entry/helper versions also reject as required.
- **280 interleaved LD2/ST2 fixtures** match QEMU at O0/O2. Independent native byte-array checks cover 840 successful and 1,400 fault/permission cases per optimization; callback probes additionally verify one complete access and no partial register/base commit.
- **1,088 single-structure memory fixtures** match QEMU at O0/O2. Independent native byte-array cases additionally cover unsigned post-index wrap, aliasing, faults and complete memory/register preservation.
- **210 permutation fixtures** match QEMU at O0/O2; independent native lane-sequence tests cover 8,064 cases per optimization, including aliasing and upper clearing.
- **8,768 S/D format-conversion fixtures** and **40 CRC fixtures** match QEMU at O0/O2. Native CRC tests independently use polynomial division and standard string check values.
- **86,304 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **26,416 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **157,714 constructed encodings**, including **73,227 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
- Native semantic cases run at O0/O2 with fatal undefined-behavior instrumentation. Synthetic programs only are executed; native game sample objects are compilation evidence.
- **4,096 independent QARMA5 comparisons pass** against privately compiled QEMU reference code. The game's PAC profile is still unverified. The legacy/QARMA5/39-bit/no-TBI backend remains diagnostic and selected only by synthetic tests.
- Four mandatory publication snapshot guard tests pass. Source file membership/content and copied checkout must match the snapshot that entered validation before commit/push.
- Formatting, Clippy with warnings denied, exact ExeFS/segment hashes, changed-identity refusal and truncated-CFG refusal pass. Public artifact checks exclude original inputs and all generated/private data.
- Reproduction: `python3 tools/validate.py --game base`. Actual outputs are in `private/testing/validation.json`. Optional `tools/classify_survey.py` uses private Capstone to classify first blockers per indexed function; those counts are not execution frequencies.

## Input investigation retained

205,781 RomFS files / 23,278,867,417 bytes were inventoried, including 121 listings. All 205,787 listed entries match presence/sizes. Deep payload inspection remains sampled/bounded. Retail version and complete retail content remain unverified; SDK markers are not a retail version mapping. Original inputs were not modified. No original-hardware oracle, Vulkan/SDL3, RADV, audio/input or gameplay validation exists.

## Milestones

| Phase | Classification | Remaining work |
|---|---|---|
| 0 investigation | Partially implemented | Retail version/completeness, graphics/resource analysis |
| 1 foundation | Partially implemented | Synthetic native/Rust bridges pass; ISA and runtime semantics incomplete |
| 2 game CPU translation | Partially implemented | 214,182 subset-liftable indexed extents with possible unbound tails/profiles; 104 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Ordinary SIMD MUL and exact YIELD IR/decoder/emitter; independent native/QEMU/budget tests; publication snapshot fingerprint checks and required synthetic Python tests; survey/Capstone metadata; CPU/compatibility/research/status documentation. Twelve source changes were matched byte-for-byte against an isolated tested copy before the supplied-input survey. CPU ABI v9 and upstream sources are unchanged. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), undecoded (122), SVC (79), LDAXR (61), FRECPE (34), MRS (28), FRSQRTE (26), AESE (8), XPACI (7), AESD (5), SHA1C (5) and SHSUB (5). Counts are first blockers, not dynamic frequencies. Next add validated crypto/PAC and remaining arithmetic forms where semantics/profile ownership are established. Reciprocal estimates need exact architectural approximation/exception semantics. Exclusive monitors need a defined concurrency model; existing serialized memory support does not imply hardware transaction equivalence. UDF/undecoded words and unresolved CFG edges require investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, SP/access-trap policies, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

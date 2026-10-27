# Status — October 27, 2026

ABI v9 and complete multi-register SIMD lane/replication transport are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Expanded checked normal memory to exact 3/6/12-byte transfers under ABI v9, retaining CPU layout/signatures and the 64-byte maximum. Buffers still use ceil(bytes/8) words; partial final words zero unused bytes and following sentinel words remain untouched. Stale-v8 native entries/FP backends and missing entry version symbols refuse.
- Ordered accesses now use an explicit 1/2/4/8 whitelist; raw invalid ordered widths reject before buffer construction. Plain 3/6-byte support cannot leak into ordered operations through a maximum-size-only guard. Extended cross-mapping, short-buffer, invalid-size, stale-version and ordered-width negatives pass.
- Added all one-through-four-register LD/ST lane lists and LD1R–LD4R replication lists. One complete callback handles each access; snapshots/deferred commits preserve aliases, wrapped lists, untouched lanes, broadcast upper clearing, base/offset state and failures. Three-way forms access exactly 3/6/12/24 bytes.
- Independent byte-array checks cover 12,240 success and 20,400 fault/permission cases per optimization; 4,080 synthetic QEMU cases match O0/O2 with fatal UBSan. Single-callback failure probes, existing single-register/complete multiple-structure, native bridge and BR/tail regressions pass.
- The survey accepts 12 further extents, with 9 accepted multi-register replication and 4 lane-list extents (overlap). LD3R no longer appears among first blockers. Changed first-feature representative selection compiles 100 objects with every observed feature represented; this count can decrease as feature overlap changes.
- Accumulating/rounded shifts and integer ADDP were validated and published as `77a8b2e`. No new upstream sources were added. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **214,160 indexed extents**, including **126,850 with PAC operations**, up from 214,148 under the same tail policy. **38 accepted extents contain contiguous LD1 lists and 61 contain LD3/LD4 interleaving** (categories overlap). **58,455 contain direct extent tails; 14,159 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **One hundred representative game-derived samples compile into native objects, 64 with PAC; none execute.**

Native samples now have a 4,096-instruction representative limit. One oversized candidate is explicitly deferred; every observed feature still has a compiled representative in its module. Representative counts can decrease when newly accepted extents combine features previously selected separately. These object counts use the new sampling policy and are not directly comparable with prior first-feature selections.

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **189 Rust tests pass**, including extended memory capacity, multiple structures and stale-ABI refusal checks and earlier scaled conversion/rounding/integer SIMD/FP/memory semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
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
- **156,868 constructed encodings**, including **73,068 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
- Native semantic cases run at O0/O2 with fatal undefined-behavior instrumentation. Synthetic programs only are executed; native game sample objects are compilation evidence.
- **4,096 independent QARMA5 comparisons pass** against privately compiled QEMU reference code. The game's PAC profile is still unverified. The legacy/QARMA5/39-bit/no-TBI backend remains diagnostic and selected only by synthetic tests.
- Formatting, Clippy with warnings denied, exact ExeFS/segment hashes, changed-identity refusal and truncated-CFG refusal pass. Public artifact checks exclude original inputs and all generated/private data.
- Reproduction: `python3 tools/validate.py --game base`. Actual outputs are in `private/testing/validation.json`. Optional `tools/classify_survey.py` uses private Capstone to classify first blockers per indexed function; those counts are not execution frequencies.

## Input investigation retained

205,781 RomFS files / 23,278,867,417 bytes were inventoried, including 121 listings. All 205,787 listed entries match presence/sizes. Deep payload inspection remains sampled/bounded. Retail version and complete retail content remain unverified; SDK markers are not a retail version mapping. Original inputs were not modified. No original-hardware oracle, Vulkan/SDL3, RADV, audio/input or gameplay validation exists.

## Milestones

| Phase | Classification | Remaining work |
|---|---|---|
| 0 investigation | Partially implemented | Retail version/completeness, graphics/resource analysis |
| 1 foundation | Partially implemented | Synthetic native/Rust bridges pass; ISA and runtime semantics incomplete |
| 2 game CPU translation | Partially implemented | 214,160 subset-liftable indexed extents with possible unbound tails/profiles; 100 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

CPU ABI marker and memory guards; lane/replication register-count IR/decoder/emitter; stale-version/capacity/ordered negatives and multi-register byte-array/QEMU tests; current ABI safety comments; survey/Capstone metadata; CPU/architecture/compatibility/research/status documentation. Thirteen source changes were matched byte-for-byte against an isolated tested copy before the supplied-input survey. CPU layout/signatures and upstream sources are unchanged. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), undecoded (122), SVC (79), LDAXR (61), FRECPE (34), MRS (28), FRSQRTE (26), YIELD (18), AESE (8), MUL (7), XPACI (7) and AESD (5). Counts are first blockers, not dynamic frequencies. Next add validated remaining integer SIMD multiplication, hint/crypto/PAC forms where semantics/profile ownership are established. Reciprocal estimates need exact architectural approximation/exception semantics. Exclusive monitors need a defined concurrency model; existing serialized memory support does not imply hardware transaction equivalence. UDF/undecoded words and unresolved CFG edges require investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, SP/access-trap policies, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

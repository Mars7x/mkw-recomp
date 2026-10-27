# Status — October 27, 2026

PRFM/PRFUM hints and coverage-driven complete blocker analysis are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added typed PRFM/PRFUM GPR-based and literal forms under an explicit no-cache-effect diagnostic profile. Strict re-survey measures **+228 accepted extents**, exactly the complete-set prediction: **214,426 total**, with no previously accepted extent lost. All 228 newly accepted extents contain prefetch hints. SP forms and reserved neighbors retain their existing rejection gates; CPU ABI v9 is unchanged.
- Whole-blocker reranking now puts FRECPE+FRECPS first: **40 predicted gains / 150–300 engineering minutes**, versus SHADD+SHSUB+UHADD **11 / 30–75 minutes**, each plus the validation gate. Estimates overlap substantially; reciprocal approximation/exception correctness needs investigation before implementation. The measured preceding full gate took 16.91 minutes.
- Implementation start through isolated checks and strict supplied-input measurement took **28.15 minutes**, or **486 accepted extents/hour** for prefetch, excluding the shared initial blocker-analysis work and final publication gate. End-to-end completion time will be recorded privately after publication; earlier stages lack comparable timers.
- Added exact 128-bit AESE/AESD/AESMC/AESIMC with original byte/finite-field transforms and complete snapshots. Independent polynomial/byte expectations check 13,312 native cases per optimization; 6,656 synthetic QEMU cases match O0/O2 with fatal UBSan. Full synthetic AES-128 encryption/decryption also match the standard known answer. No new upstream implementation was copied.
- The pre-existing AES work gains **16 accepted extents**, from 214,182 to 214,198, before changing work priorities. One additional representative object compiles. Accepted AES categories contain 16 mix and 13 round extents (overlap).
- Added a separate all-blocker diagnostic: every rejected extent is examined through optimistic CFG continuation and full linear-span decoding. Its strict baseline exactly matches the original census; no acceptance rule or existing correctness gate changed. Ranking evaluates singles/pairs/triples with explicit effort bands and runtime/configuration exclusions.
- [Coverage analysis](COVERAGE.md) inventories every blocker in all rejected extents and ranks complete requirement sets, including combinations. The baseline prefetch prediction was confirmed by the unchanged strict survey.
- **Runtime/CFG ceiling:** 8,548 of 9,658 strict rejections are CFG-boundary failures. The prefetch fix exhausted 228 of the previous 350 purely ISA/nonstructural opportunities, leaving **122** under the diagnostic model. Further small ISA survey wins have sharply diminishing usefulness until runtime/configuration/boundary dependencies are addressed correctly.
- Ordinary MUL/YIELD and checked publication snapshots were validated and published as `7dc5395`. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **214,426 indexed extents**, including **127,047 with PAC operations**, up from 214,198 under the same tail policy. **38 accepted extents contain contiguous LD1 lists and 61 contain LD3/LD4 interleaving** (categories overlap). **58,469 contain direct extent tails; 14,168 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **109 representative game-derived samples compile into native objects, 70 with PAC; none execute.**

Native samples now have a 4,096-instruction representative limit. One oversized candidate is explicitly deferred; every observed feature still has a compiled representative in its module. Representative counts can decrease when newly accepted extents combine features previously selected separately. These object counts use the new sampling policy and are not directly comparable with prior first-feature selections.

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **928 prefetch fixtures** match QEMU at O0/O2; **8,352 native cases per optimization** check register/control/fault preservation, exact shared budgets, PC rebasing and zero memory callbacks. Existing SP-prefetch and reserved-form refusal tests remain unchanged.
- **207 Rust tests pass**, including extended memory capacity, multiple structures and stale-ABI refusal checks and earlier scaled conversion/rounding/integer SIMD/FP/memory semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **6,656 AES fixtures** match QEMU at O0/O2; independent polynomial/byte expectations check 13,312 native cases per optimization. Two complete AES-128 standard encryption/decryption known answers also pass per optimization.
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
- **160,578 constructed encodings**, including **74,196 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
- Native semantic cases run at O0/O2 with fatal undefined-behavior instrumentation. Synthetic programs only are executed; native game sample objects are compilation evidence.
- **4,096 independent QARMA5 comparisons pass** against privately compiled QEMU reference code. The game's PAC profile is still unverified. The legacy/QARMA5/39-bit/no-TBI backend remains diagnostic and selected only by synthetic tests.
- Complete blocker diagnostics preserve the exact strict baseline across all 9,430 rejects / 4,588,952 bytes with zero omissions. Three diagnostic invariants and synthetic ranking checks pass; detailed effort/ranking artifacts remain private.
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
| 2 game CPU translation | Partially implemented | 214,426 subset-liftable indexed extents with possible unbound tails/profiles; 109 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Typed prefetch decoder/emitter, independent native/QEMU tests, Capstone operand checks and blocker classification; coverage/CPU/compatibility/research/status documentation. All seven implementation files matched the isolated tested copy before the supplied-input survey. CPU ABI v9, existing correctness tests and acceptance rules remain unchanged. Original inputs remain unchanged and all game-derived outputs stay private.

## Measured commit gains

| Commit | Accepted extents | Gain from predecessor |
|---|---:|---:|
| `85d077a` | 213,977 | +70 |
| `2f15125` | 213,980 | +3 |
| `bb397fa` | 214,058 | +78 |
| `69f81a9` | 214,111 | +53 |
| `77a8b2e` | 214,148 | +37 |
| `571c012` | 214,160 | +12 |
| `7dc5395` | 214,182 | +22 |
| `18e9b57` | 214,198 | +16 |
| Current PRFM/PRFUM work | 214,426 | +228 |

Future commits will report measured strict gain against the preceding commit and compare it with the blocker-set prediction. Earlier stages did not record comparable end-to-end effort timings; new validation receipts record gate durations.

## Next concrete dependencies

Select work from the [complete blocker requirements and rankings](COVERAGE.md), not the first unsupported mnemonic. The measured prefetch result confirms +228. Investigate the refreshed highest-scoring FRECPE+FRECPS package (40 conditional gains); approximation/rounding/exception requirements may revise its engineering estimate. SHADD+SHSUB+UHADD (11 gains) has a much shorter but overlapping effort band. Re-run the exact strict survey and report measured gain after each commit. Beyond prefetch, runtime services, exclusive monitor/concurrency contracts, target counter/PAC/FP/access configuration and CFG boundaries dominate; no successful stubs, changed boundary policy or relaxed tests are used to inflate acceptance. Original execution/bootstrap remains gated.

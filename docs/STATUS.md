# Status — October 27, 2026

Reciprocal square-root estimation/refinement and coverage-driven complete blocker analysis are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added 2S/4S FRSQRTE/FRSQRTS with original estimate boundary search and correctly rounded refinement through existing callbacks. **Measured gain: +24**, exactly the complete-set prediction, reaching **214,488** accepted extents without losing prior acceptances. CPU ABI v9, the FPCR whitelist, existing tests and all strict acceptance rules remain unchanged.
- Independent midpoint expectations check **1,040,784 native estimate cases per optimization**; an independent **576-bit exact oracle checks 102,144 refinements**, including a double-rounding adversary. All **102,144 QEMU comparisons** pass O0/O2 with fatal UBSan. Stage-by-stage callback failures do not partially commit vector/FPSR state; exceptional values, modes and aliases pass.
- Estimate/CLZ was validated and pushed as **`3545b9c`: +18**, total 214,464; all 229 Rust tests passed and the full gate took **15.31 minutes**. GitHub's verified main-head push timestamp gives a scoped 52.59-minute interval (**20.54 extents/hour**); adding the separately recorded 7.66-minute earlier reciprocal assessment gives **17.93/hour**. Overlapping intervals are not additive engineer-hours.
- Earlier measured commits: MLA/XPACI **`99c4358`: +9 / 41.88 minutes / 12.89 per hour**; halving **`481eb98`: +11 / 29.14 minutes / 22.65 per hour**; prefetch **`f752c08`: +228 / 43.90 minutes / 311.62 per hour**, excluding shared initial blocker analysis.
- Complete blocker ranking now evaluates singles/pairs/triples/quadruples and complete larger observed requirement sets. Existing ranking self-tests remain unchanged; three additional tests verify four-fix synergy, complete six-fix visibility and runtime exclusions. Operand-form/code inspection refines the next costs: observed 4S immediate SQSHL, 2S signed min/max pairs and long pair sums reuse existing arithmetic/oracles.
- The leading next package is **SQSHL+SMINP+SMAXP+SADDLP: 11 conditional gains / 16–45 estimated engineering minutes**, plus validation; the three-fix SQSHL/min/max package predicts 8 / 10–27 minutes. These overlapping effort bands are forecasts, not measured completion times. Details are in [coverage analysis](COVERAGE.md).
- **Runtime/CFG ceiling:** 8,551 of 9,368 strict rejections are boundary failures; diagnostic edge-blocked extents stay 9,132. Only **35** feasible purely ISA/nonstructural opportunities remain. Services, concurrency, target FP/PAC/VA/TBI/counter/access configuration and unbound targets increasingly limit further static gains. No successful stubs or changed boundaries inflate acceptance.
- Source-only automatic publication remains active; README, inputs/generated code, credentials and private artifacts remain excluded. **No game-derived function has executed; no boot, frame or gameplay is claimed.**

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **214,488 indexed extents**, including **127,091 with PAC operations**, up from 214,464 under the same tail policy. **38 accepted extents contain contiguous LD1 lists and 61 contain LD3/LD4 interleaving** (categories overlap). **58,474 contain direct extent tails; 14,171 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **117 representative game-derived samples compile into native objects, 75 with PAC; none execute.**

Native samples now have a 4,096-instruction representative limit. Three oversized sample candidates are explicitly deferred; every observed feature still has a compiled representative in its module. Representative counts can decrease when newly accepted extents combine features previously selected separately. These object counts use the new sampling policy and are not directly comparable with prior first-feature selections.

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **102,144 reciprocal-square-root QEMU fixtures** match O0/O2; independent midpoint and 576-bit exact expectations check **1,040,784 estimate / 102,144 refinement native cases per optimization**. Stage-by-stage callback faults and unsupported profiles preserve destination/FPSR state. The naive double-rounding adversary is explicitly checked.
- Three additional complete-candidate enumeration tests pass; every original ranking self-test, acceptance gate and pre-existing test file remains unchanged.
- **25,728 2S estimate fixtures** match QEMU at O0/O2; independent midpoint expectations check **1,040,912 native cases per optimization**, with exceptional inputs/controls and unchanged 4S/profile rejection.
- **3,864 2S CLZ fixtures** match QEMU at O0/O2; independent leading-zero expectations check **262,144 native cases per optimization**, including zero, varied lanes, aliases and complete state preservation.
- **31,200 MLA fixtures** match QEMU at O0/O2; independent byte/u128 expectations check **46,800 native cases per optimization** across register/indexed operands, accumulator/source aliases and wrap.
- **1,280 XPACI fixtures** match QEMU on shared canonical pointer cases; **111,264 native cases per optimization** check the explicit no-TBI strip profile, all register/zero destinations, budgets and full state preservation. Additional probes check callback operands, faults and missing backend/context. A separate test asserts the expected QEMU top-tag profile difference. The two earlier PAC native integration tests still pass unchanged.
- **18,720 halving fixtures** match QEMU at O0/O2; independent byte/i128 expectations check **28,080 native cases per optimization**, including odd negative results, extrema, aliases, distinct lanes and unchanged flags. Pre-existing files under crate test directories remain byte-identical to the preceding snapshot.
- **928 prefetch fixtures** match QEMU at O0/O2; **8,352 native cases per optimization** check register/control/fault preservation, exact shared budgets, PC rebasing and zero memory callbacks. Existing SP-prefetch and reserved-form refusal tests remain unchanged.
- **235 Rust tests pass**, including extended memory capacity, multiple structures and stale-ABI refusal checks and earlier scaled conversion/rounding/integer SIMD/FP/memory semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
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
- **163,590 constructed encodings**, including **75,884 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
- Native semantic cases run at O0/O2 with fatal undefined-behavior instrumentation. Synthetic programs only are executed; native game sample objects are compilation evidence.
- **4,096 independent QARMA5 comparisons pass** against privately compiled QEMU reference code. The game's PAC profile is still unverified. The legacy/QARMA5/39-bit/no-TBI backend remains diagnostic and selected only by synthetic tests.
- Complete blocker diagnostics preserve the exact strict baseline across all 9,368 rejects / 4,373,452 bytes with zero omissions. Three diagnostic invariants and synthetic ranking checks pass; detailed effort/ranking artifacts remain private.
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
| 2 game CPU translation | Partially implemented | 214,488 subset-liftable indexed extents with possible unbound tails/profiles; 117 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Reciprocal-square-root IR/decoders/emitter and independent native/QEMU/failure tests; feature sampling and Capstone checks; complete-candidate enumeration, scoped effort calibration and additional ranking tests; CPU/compatibility/research/coverage/status documentation. Five CPU implementation files match the isolated tested snapshot. CPU ABI v9 and all prior correctness/acceptance gates remain unchanged; pre-existing test files stay byte-identical. Original inputs remain unchanged and all binary-derived outputs stay private.

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
| `f752c08` | 214,426 | +228 |
| `481eb98` | 214,437 | +11 |
| `99c4358` | 214,446 | +9 |
| `3545b9c` | 214,464 | +18 |
| Current reciprocal-square-root work | 214,488 | +24 |

Future commits will report measured strict gain against the preceding commit and compare it with the blocker-set prediction. Earlier stages did not record comparable end-to-end effort timings; new validation receipts record gate durations.

## Next concrete dependencies

Use [complete feasible blocker rankings](COVERAGE.md). The measured reciprocal-square-root gain confirms +24. Scoped operand analysis and shared arithmetic now prioritize SQSHL immediate + signed pairwise min/max + SADDLP (11 conditional gains); test saturation/QC, signed comparisons, widening, aliases and upper clearing without changing existing gates. Re-survey and report measured gain after each commit. Runtime/configuration contracts and CFG boundaries dominate; no successful stubs or relaxed policies inflate acceptance, and original bootstrap remains gated.

# Status — October 27, 2026

An explicit v10 exclusive-memory native contract with coherent memory routes and complete blocker analysis are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added an explicit **CPU-v10 frame / exclusive-v1 callback contract** alongside unchanged CPU ABI v9. Plain, ordered, LSE, barrier and exclusive operations share one owned memory authority and mutex. The table attests matching prefix callback pointers/context; foreign routes, pre-attached frames and unversioned call routing refuse. Native v10 entry names are distinct; the existing v9 loader rejects their version before fetching/invoking the entry.
- **Nine new native contract tests pass in debug/release**, with C helpers at O0/O2 and fatal UBSan. Evidence includes **576 native/QEMU sequences per optimization**, **3,200 direct atomic and 2,560 unchanged-v9-AOT LSE cases per optimization**, **40 transfer/interference cases per optimization**, and **four-thread 4,000 atomic / 2,000 exclusive increments per optimization**. The existing v9 64-byte AOT store/load also uses the coherent prefix successfully.
- Tests cover frame/table C↔Rust sizes/offsets, malformed versions/capacities/orders, null/fault output preservation, same-value store invalidation, failed-CAS reservation preservation, ownership/lifetime, stale authorities and poisoned locks. Prefix normal transfers retain 1/2/3/4/6/8/12/16/24/32/48/64 bytes; ordered/LSE retain exactly 1/2/4/8. No existing callback signature or v9 operation contract is repurposed.
- **Strict survey gain: +0; baseline remains 214,521.** LL/SC decoder/emitter acceptance is still disabled; the contract and actual native callbacks are now concrete prerequisites. Target granule/access/trap, alias/MMIO, weak-memory/scheduler settings and cross-module v10 dispatch remain unverified/unbound. No original function executes.
- Exclusive-memory primitive commit **`4978215`: +0**, all 273 Rust tests and the additional release semantic suite passing; full validation took **17.65 minutes**. Seven reservation tests and 576 direct-model QEMU sequences remain unchanged and passing.
- Remaining arithmetic/PAC commit **`93f7bcf`: +6**, total 214,521; all 265 Rust tests passed. The scoped verified push interval is **41.99 minutes / 8.57 extents per hour**, including a **17.13-minute** full gate. Overlapping intervals are not additive engineer-hours.
- SHA/refinement commit **`00e3acb`: +10**, total 214,515; all 256 Rust tests passed. The scoped verified push interval is **39.04 minutes / 15.37 extents per hour**, including a **16.63-minute** full gate. Overlapping intervals are not additive engineer-hours.
- Previous polynomial/widening/difference package: **+6**, exactly predicted, total 214,505. It passed all 246 Rust tests and retained every previous test and acceptance rule. The verified scoped push interval is 32.41 minutes / 11.11 extents per hour, including the 16.47-minute full gate. Overlapping intervals are not additive engineer-hours.
- Previous signed pairwise/widening/saturating package: **+11**, exactly predicted, total 214,499. It passed all 239 Rust tests and retained all pre-existing tests and acceptance rules. The verified push interval is 53.34 minutes / 12.37 extents per hour, including a 15.98-minute full gate. Overlapping scoped intervals are not additive engineer-hours.
- Reciprocal-square-root commit **`1a5195a`: +24**, total 214,488; all 235 Rust tests passed. The verified main-head push interval is **63.86 minutes / 22.55 extents per hour**, including a **15.60-minute full gate**. Overlapping scoped intervals are not additive engineer-hours.
- Estimate/CLZ was validated and pushed as **`3545b9c`: +18**, total 214,464; all 229 Rust tests passed and the full gate took **15.31 minutes**. GitHub's verified main-head push timestamp gives a scoped 52.59-minute interval (**20.54 extents/hour**); adding the separately recorded 7.66-minute earlier reciprocal assessment gives **17.93/hour**. Overlapping intervals are not additive engineer-hours.
- Earlier measured commits: MLA/XPACI **`99c4358`: +9 / 41.88 minutes / 12.89 per hour**; halving **`481eb98`: +11 / 29.14 minutes / 22.65 per hour**; prefetch **`f752c08`: +228 / 43.90 minutes / 311.62 per hour**, excluding shared initial blocker analysis.
- Complete blocker ranking now evaluates singles through five-fix combinations and complete larger observed requirement sets. Existing ranking self-tests remain unchanged; three additional tests verify four-fix synergy, complete six-fix visibility and runtime exclusions. Operand-form/code inspection refines the next costs: observed 4S immediate SQSHL, 2S signed min/max pairs and long pair sums reuse existing arithmetic/oracles.
- Complete-set ranking now considers unions of up to five feasible requirement sets, exposing the final six-gain package. Two additional checks verify full sharing and refuse invented ISA gains when only policy/runtime sets remain. Every previous ranking test stays unchanged.
- **Runtime/CFG ceiling reached for this survey:** all **9,335 rejects / 4,324,436 bytes** are inventoried with zero omissions; **zero feasible pure ISA/nonstructural requirement sets remain**. This does not mean complete ISA support: unsupported operations still coexist with structural/runtime/policy barriers. First-boundary failures stay 8,551, diagnostic structural barriers 9,132 and runtime/configuration/opaque/policy barriers 789 (overlapping counts).
- A new read-only runtime-contract inventory finds **167 nonstructural contract-only extents**, **72 distinct SVC immediates**, and conditional single-contract gains of **64 exclusive-monitor / 14 physical-counter extents** (78 together). Three synthetic checks preserve complete requirements and other barriers. Runtime effort is explicitly unpriced pending real target semantics/configuration; no successful stubs, fake counter values or execution claims inflate these counts.
- Source-only automatic publication remains active; README, inputs/generated code, credentials and private artifacts remain excluded. **No game-derived function has executed; no boot, frame or gameplay is claimed.**

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **214,521 indexed extents**, including **127,110 with PAC operations**, up from 214,515 under the same tail policy. **38 accepted extents contain contiguous LD1 lists and 61 contain LD3/LD4 interleaving** (categories overlap). **58,474 contain direct extent tails; 14,171 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **125 representative game-derived samples compile into native objects, 79 with PAC; none execute.**

Native samples now have a 4,096-instruction representative limit. Three oversized sample candidates are explicitly deferred; every observed feature still has a compiled representative in its module. Representative counts can decrease when newly accepted extents combine features previously selected separately. These object counts use the new sampling policy and are not directly comparable with prior first-feature selections.

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **Nine v10 native contract tests** pass in debug/release, compiling C at O0/O2 with fatal UBSan: C/Rust layout/version checks, malformed inputs/faults, coherence, CAS/RMW invalidation, stale/poisoned contexts, v9 AOT leaf interoperability and four-thread progress. **576 actual-native-callback sequences per optimization** match QEMU; 3,200 direct and 2,560 AOT LSE cases per optimization match independent arithmetic. All 61 pre-existing crate test files remain byte-identical.

- **Seven exclusive-memory unit tests and 576 synthetic QEMU sequences** pass in debug and release. Values/status/memory match the serialized diagnostic model for plain and acquire/release encodings; this does not validate hardware weak-memory ordering, physical aliasing or actual target granule/trap behavior. Existing LL/SC instruction refusals and ABI v9 tests remain unchanged. The release memory/monitor suite is an additional mandatory gate.

- **345,984 remaining-integer QEMU fixtures** and independent **156,672 / 23,296 / 1,024 / 344,064 native cases per optimization** pass O0/O2 with fatal UBSan. Separate product/accumulator clamps, every register count byte/narrowing immediate, QC, aliases and exact budgets are checked.
- **640 PACIB sign/strip round trips** match QEMU on shared canonical addresses. Independent operand/budget/failure probes and the existing independently QARMA-checked backend validate transport. Target signature/profile equivalence remains unverified.
- Three additional runtime-contract inventory checks and two remaining-set ranking checks pass. All 58 pre-existing crate test files remain byte-identical.

- **136,192 FRECPS QEMU fixtures** match O0/O2; independent exact 576-bit expectations check **204,288 native cases per optimization**, including fused-rounding adversary, NaN/exception rules, all supported controls and transactional callback failures.

- **5,032 SHA instruction QEMU fixtures** match O0/O2; independent truth/round/word expectations check **27,864 native cases per optimization**. Six complete standard hash known answers and nine independent additional SHA256 messages pass per optimization; native schedule words and exact shared budgets are checked. All 56 prior crate test files remain byte-identical.

- **9,576 polynomial/widening/unsigned-difference QEMU fixtures** match O0/O2; independent coefficient convolution, exact widening and byte-difference expectations check **352,800 / 99,024 / 271,104 native cases per optimization**. Every prior crate test file remains byte-identical; callback failures and unsupported FPCR preserve output state.

- **25,360 new integer QEMU fixtures** match O0/O2; independent byte/i128 expectations check **25,392 min/max, 2,208 widening and 14,656 saturating immediate cases per optimization**, including every shift amount, sticky QC and full state preservation.

- **102,144 reciprocal-square-root QEMU fixtures** match O0/O2; independent midpoint and 576-bit exact expectations check **1,040,784 estimate / 102,144 refinement native cases per optimization**. Stage-by-stage callback faults and unsupported profiles preserve destination/FPSR state. The naive double-rounding adversary is explicitly checked.
- Three additional complete-candidate enumeration tests pass; every original ranking self-test, acceptance gate and pre-existing test file remains unchanged.
- **25,728 2S estimate fixtures** match QEMU at O0/O2; independent midpoint expectations check **1,040,912 native cases per optimization**, with exceptional inputs/controls and unchanged 4S/profile rejection.
- **3,864 2S CLZ fixtures** match QEMU at O0/O2; independent leading-zero expectations check **262,144 native cases per optimization**, including zero, varied lanes, aliases and complete state preservation.
- **31,200 MLA fixtures** match QEMU at O0/O2; independent byte/u128 expectations check **46,800 native cases per optimization** across register/indexed operands, accumulator/source aliases and wrap.
- **1,280 XPACI fixtures** match QEMU on shared canonical pointer cases; **111,264 native cases per optimization** check the explicit no-TBI strip profile, all register/zero destinations, budgets and full state preservation. Additional probes check callback operands, faults and missing backend/context. A separate test asserts the expected QEMU top-tag profile difference. The two earlier PAC native integration tests still pass unchanged.
- **18,720 halving fixtures** match QEMU at O0/O2; independent byte/i128 expectations check **28,080 native cases per optimization**, including odd negative results, extrema, aliases, distinct lanes and unchanged flags. Pre-existing files under crate test directories remain byte-identical to the preceding snapshot.
- **928 prefetch fixtures** match QEMU at O0/O2; **8,352 native cases per optimization** check register/control/fault preservation, exact shared budgets, PC rebasing and zero memory callbacks. Existing SP-prefetch and reserved-form refusal tests remain unchanged.
- **282 Rust tests pass**, including extended memory capacity, multiple structures and stale-ABI refusal checks and earlier scaled conversion/rounding/integer SIMD/FP/memory semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
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
- **169,228 constructed encodings**, including **78,884 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
- Native semantic cases run at O0/O2 with fatal undefined-behavior instrumentation. Synthetic programs only are executed; native game sample objects are compilation evidence.
- **4,096 independent QARMA5 comparisons pass** against privately compiled QEMU reference code. The game's PAC profile is still unverified. The legacy/QARMA5/39-bit/no-TBI backend remains diagnostic and selected only by synthetic tests.
- Complete blocker diagnostics preserve the exact strict baseline across all 9,335 rejects / 4,324,436 bytes with zero omissions. Three diagnostic invariants and synthetic ranking checks pass; detailed effort/ranking artifacts remain private.
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
| 2 game CPU translation | Partially implemented | 214,521 subset-liftable indexed extents with possible unbound tails/profiles; 125 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Explicit v10 C/Rust frame/table and owned native bridge; coherent plain/ordered/LSE/barrier/exclusive routes; ordinary authority validation/read helpers; nine native C/AOT/QEMU/fault/contention tests; additive release coverage; CPU/compatibility/research/coverage/status documentation. All 61 prior crate test files, native/cpu.h, Rust ABI-v9 definitions and decoder/codegen sources remain byte-identical. Existing correctness/acceptance gates stay unchanged. Inputs remain unchanged; all game-derived outputs stay private.

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
| `1a5195a` | 214,488 | +24 |
| `e3997c1` | 214,499 | +11 |
| `27964cb` | 214,505 | +6 |
| `00e3acb` | 214,515 | +10 |
| `93f7bcf` | 214,521 | +6 |
| `4978215` | 214,521 | +0 |
| Current coherent v10 native contract | 214,521 | +0 |

Future commits will report measured strict gain against the preceding commit and compare it with the blocker-set prediction. Earlier stages did not record comparable end-to-end effort timings; new validation receipts record gate durations.

## Next concrete dependencies

The bounded default diagnostics have no feasible pure ISA sets left. The real reservation primitive and coherent native contract are implemented; next implement a checked v10 native entry/dispatch path and capability-scoped LL/SC IR/emission with the existing CFG boundaries, budgets and refusal rules. Preserve ABI-v9 compatibility/tests and do not silently change the default survey profile. Before counting new monitor coverage, measure the explicitly supported profile against the supplied extents and keep compilation distinct from original execution. Physical counters need real configured time/frequency contracts; service mappings, actual target memory/access configuration and original bootstrap stay gated. No placeholders, fabricated clocks or relaxed boundaries.

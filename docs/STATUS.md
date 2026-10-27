# Status — October 27, 2026

Integer SIMD arithmetic, comparison masks and reductions are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added ordinary vector ADD/SUB, CMEQ/CMTST, signed CMGE/CMGT and unsigned CMHS/CMHI, including valid scalar-D forms. Compare-zero CMEQ/CMGE/CMGT/CMLE/CMLT use literal zero. Arithmetic wraps at the decoded lane width; comparisons produce all-one/zero masks without changing NZCV.
- Added ADDV modulo-width reductions and widening SADDLV/UADDLV for legal B/H/S arrangements. Output scalars clear unused bits. Signed comparisons use unsigned sign-bit ordering; signed widening uses defined unsigned sign extension and sum wrap, avoiding host signed overflow and implementation-defined casts.
- Complete source snapshots preserve destination aliases and V31. Mandatory tests compare against independent i128/u128 math across full-width extrema, mixed/random patterns, carries, signed minima, reductions, upper clearing and general/control/flag preservation. Unsupported scalar/one-D/reduction arrangements reject; existing FCCMP/FCSEL decoding is protected.
- The native matrix checks 28,992 cases per O0/O2 optimization. Separate synthetic QEMU fixtures compare 17,568 cases with fatal UBSan. No extra callbacks, upstream source changes or ABI v7 layout/signature changes were needed.
- Supplied-binary metadata selects integer-lane/reduction examples. Table lookup and byte count are published as `1a18c73`. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **213,058 indexed extents**, including **126,085 with PAC operations**, up from 212,908 under the same tail policy. **150 accepted extents contain the new integer-lane or reduction operations** (categories overlap). **58,391 contain direct extent tails; 14,058 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Sixty-three representative game-derived samples compile into native objects, 45 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **123 Rust tests pass**, including integer SIMD wider-math/reduction/state checks and earlier table/count/comparison/lane-memory/FP/CRC/conversion semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **17,568 integer SIMD fixtures** match QEMU at O0/O2. Independent i128/u128 native oracles check 28,992 cases per optimization, including all-lane extrema, aliases, wrap and widened sums.
- **120 table/count fixtures** match QEMU at O0/O2. Independent native byte-sequence/popcount tests cover 34,816 cases per optimization, including every byte/index value, table wrapping, aliases and upper clearing.
- **16,800 SIMD FP comparison fixtures** match QEMU at O0/O2 across S/D, scalar/vector arrangements, aliases, NaNs, signed zeros, controls and compare-zero forms. Mandatory native tests independently compare ordered inputs and NaN/exception expectations.
- **1,088 single-structure memory fixtures** match QEMU at O0/O2. Independent native byte-array cases additionally cover unsigned post-index wrap, aliasing, faults and complete memory/register preservation.
- **210 permutation fixtures** match QEMU at O0/O2; independent native lane-sequence tests cover 8,064 cases per optimization, including aliasing and upper clearing.
- **8,768 S/D format-conversion fixtures** and **40 CRC fixtures** match QEMU at O0/O2. Native CRC tests independently use polynomial division and standard string check values.
- **43,232 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **26,416 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **97,588 constructed encodings**, including **48,829 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 213,058 subset-liftable indexed extents with possible unbound tails/profiles; 63 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Integer SIMD typed IR/decoder and native emitter; mandatory/differential wider-math tests; scalar-domain boundary protection; survey feature/sample selection; encoding cross-check; STATUS/CPU/architecture/compatibility/research documentation. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), FCVTZS (120), undecoded (120), LD2 (99), FRINTM (82), SVC (79), ST2 (64), MVN (60), LDAXR (60), REV64 (58), FMAXP (53) and SHL (50). Counts are first blockers, not dynamic frequencies. Next add validated scaled conversion/integral rounding, remaining raw SIMD transforms/shifts, pairwise FP min/max and multi-register structure transfers. Exclusive monitors need a defined concurrency model; single-register memory support does not imply larger transactions. UDF/undecoded words and unresolved CFG edges require investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, SP/access-trap policies, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

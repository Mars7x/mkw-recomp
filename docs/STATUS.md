# Status — October 27, 2026

SIMD floating comparison masks are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added S/D scalar/vector FCMEQ/FCMGE/FCMGT, FACGE/FACGT and compare-zero FCMEQ/FCMGE/FCMGT/FCMLE/FCMLT. True elements become all-one bit masks; false/unordered elements become zero. Scalar/64-bit arrangements clear unused upper bits.
- Equality uses the existing quiet comparison helper, while ordered/absolute comparisons signal IOC for any NaN. Signaling NaNs signal IOC for equality too. Absolute forms remove input sign bits before checked comparison; zero forms use literal floating zero, never V0. Original NZCV and general registers are preserved.
- Sources are snapshotted and vector/FPSR changes commit only after every lane succeeds. Mandatory tests exercise operand/destination aliasing, V31, raw NaN classification, independent host comparisons of ordered inputs, sticky flags, unsupported controls, absent helpers and forced second-lane refusal.
- Independent synthetic QEMU fixtures compare 16,800 cases at O0/O2 with fatal UBSan. A decoder-boundary regression test retains ordinary scalar FCCMP/FCSEL encodings whose low fields overlap the new SIMD forms. Half precision and unsupported profiles remain excluded.
- The survey marks floating-backend requirements and selects comparison samples. ABI v7 layout/signatures and upstream sources remain unchanged. Checked SIMD lane transfers/replicated loads are published as `33de44b`. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **212,852 indexed extents**, including **125,916 with PAC operations**, up from 212,753 under the same tail policy. **99 accepted extents contain the new SIMD FP comparison masks** (categories overlap). **58,381 contain direct extent tails; 14,042 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Sixty representative game-derived samples compile into native objects, 43 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **117 Rust tests pass**, including comparison-mask/NaN/alias/refusal behavior and earlier lane-memory/SIMD/FP/CRC/conversion semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **16,800 SIMD FP comparison fixtures** match QEMU at O0/O2 across S/D, scalar/vector arrangements, aliases, NaNs, signed zeros, controls and compare-zero forms. Mandatory native tests independently compare ordered inputs and NaN/exception expectations.
- **1,088 single-structure memory fixtures** match QEMU at O0/O2. Independent native byte-array cases additionally cover unsigned post-index wrap, aliasing, faults and complete memory/register preservation.
- **210 permutation fixtures** match QEMU at O0/O2; independent native lane-sequence tests cover 8,064 cases per optimization, including aliasing and upper clearing.
- **8,768 S/D format-conversion fixtures** and **40 CRC fixtures** match QEMU at O0/O2. Native CRC tests independently use polynomial division and standard string check values.
- **43,232 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **26,416 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **96,268 constructed encodings**, including **48,276 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 212,852 subset-liftable indexed extents with possible unbound tails/profiles; 60 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

SIMD comparison typed IR/decoder and native emitter; mandatory/differential mask and flag tests; scalar-decoder boundary protection; floating-backend metadata/sample selection; encoding cross-check; STATUS/CPU/architecture/compatibility/research documentation. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), TBL (122), FCVTZS (120), undecoded (118), LD2 (96), FRINTM (82), SVC (79), CNT (68), ADD (65), ST2 (64), CMEQ (60) and LDAXR (60). Counts are first blockers, not dynamic frequencies. Next add validated table lookup/popcount and integer vector operations, scaled conversion/rounding and multi-register structure transfers. Exclusive monitors need a defined concurrency model; single-register memory support does not imply larger transactions. UDF/undecoded words require extent/CFG investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, SP/access-trap policies, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

# Status — October 27, 2026

Fused sign variants and indexed-lane SIMD multiplication are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added S/D FMSUB/FNMADD/FNMSUB and vector FMLS. Required input signs change before the existing fused helper applies NaN priority, exception flags and single rounding; this differs from negating a rounded result. Native tests verify finite signs, NaN payload signs, cancellation and refusal behavior.
- Added S/D scalar and vector FMUL/FMLA/FMLS by element, including every legal source lane. The emitter snapshots the complete indexed source before processing destination lanes, preserving overlap semantics. Reserved single-D vectors, reserved D lane bits, half forms and FMULX remain unsupported.
- Expanded the synthetic QEMU matrices to 20,560 scalar FP/selection cases and 36,064 vector numeric cases. Native O0/O2 builds use fatal UBSan. ABI v7 signatures/layout and the restricted FPCR profile remain unchanged.
- Supplied-binary metadata now separately marks fused sign variants and indexed SIMD math and selects representative native object samples. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub. The preceding SIMD milestone is published as `356754c`.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **211,163 indexed extents**, including **124,525 with PAC operations**, up from 210,061 under the same tail policy. **817 accepted extents contain fused sign variants, 328 contain indexed SIMD numeric operations, and 513 contain conditional FP compares.** **58,186 contain direct extent tails; 13,948 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Fifty-two representative game-derived samples compile into native objects, 39 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **99 Rust tests pass**, including fused sign/single-rounding and indexed-lane semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **36,064 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **20,560 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **86,440 constructed encodings**, including **46,965 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 211,163 subset-liftable indexed extents with possible unbound tails/profiles; 52 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Scalar/vector FP decoder and emitter; original fused-sign helper glue; native known-pattern and expanded differential tests; supplied-binary feature selection; decoder cross-check; STATUS/CPU/architecture/compatibility/research documentation. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), FCVT (449), LD1 (270), FADDP (213), ZIP1 (201), FMAXNM (189), LD1R (179), CRC32B (165), ST1 (118), undecoded (117), FCVTZS (106) and LD2 (82). Counts are first blockers, not dynamic frequencies. Next add validated format/scaled conversions, structure/permutation operations and CRC instructions. UDF and undecoded words require extent/CFG investigation rather than guessed execution. Target cache/counter/PAC/FP/address configuration, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

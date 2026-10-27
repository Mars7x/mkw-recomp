# Status — October 27, 2026

SIMD S/D conversions and arithmetic now lower through checked numeric helpers; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added vector 2S/4S/2D FADD/FSUB/FMUL/FDIV/FMLA, FNEG/FABS/FSQRT, SCVTF/UCVTF and FCVTZS/FCVTZU, plus SIMD scalar S/D conversions. Reserved single-D vector forms reject; half, scaled and indexed-lane operations remain separate dependencies.
- Every instruction snapshots sources and the old accumulator, accumulates lane results/FPSR locally, and commits only after every callback succeeds. Register overlap, upper-bit clearing, sticky flags, absent backends and a forced second-lane failure have mandatory native tests. ABI v7 remains unchanged.
- Expanded independent synthetic QEMU comparisons to 13,216 vector cases across arrangements, operand overlap, rounding/default-NaN modes and numeric edge patterns. Native O0/O2 builds use fatal UBSan. No game instruction stream executes in QEMU.
- Survey metadata records vector-FP backend requirements and selects new representative compilation samples. Guest thread registers remain distinct from host TLS; retail configuration/bootstrap remain unverified. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **210,061 indexed extents**, including **123,611 with PAC operations**, up from 209,011 under the same tail policy. **1,050 accepted extents contain the new SIMD numeric operations; 406 contain conditional FP compares.** **58,126 contain direct extent tails; 13,882 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Forty-eight representative game-derived samples compile into native objects, 37 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **96 Rust tests pass**, including SIMD numeric lane/overlap/transactional behavior, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **16,708 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **85,716 constructed encodings**, including **46,647 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 210,061 subset-liftable indexed extents with possible unbound tails/profiles; 48 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Vector numeric IR/decoder and native emitter; mandatory and differential SIMD tests; shared synthetic ELF reference builder; CLI feature/backend requirements; decoder cross-check; STATUS/CPU/architecture/compatibility/research documentation. ABI v7 remains the layout/signature version. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include FMSUB (496 extents), FMUL (475), UDF (457), FCVT (433), FNMSUB (269), LD1 (230), FADDP (182), ZIP1 (166), FMAXNM (165), CRC32B (165), LD1R (163) and FNMADD (151). Counts are first blockers, not dynamic frequencies. Next add the remaining fused forms, indexed SIMD math and floating-format conversions, then structure/permutation operations. Target cache/counter/PAC/FP/address configuration, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

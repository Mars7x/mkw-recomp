# Status — October 27, 2026

S/D format conversion and CRC32/CRC32C instructions are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added scalar S↔D FCVT through unchanged pinned SoftFloat format-conversion sources and Arm NaN conversion helpers. Original adapter glue applies the existing scoped rounding, gradual-underflow, sticky flags and default-NaN profile. Raw NaN payload alignment, signed zeros, overflow/underflow, halfway rounding and overlap have native tests. Half/scaled conversions remain unsupported.
- Added CRC32B/H/W/X and CRC32CB/CH/CW/CX using portable unsigned polynomial recurrence, without host CRC instruction requirements or implicit initial/final complements. Seeds/results are 32 bits, input widths and zero-register semantics are explicit, and NZCV/vector state are preserved.
- Independent polynomial long division checks 7,680 native CRC cases per optimization, plus known CRC-32/CRC-32C string checks. Separate QEMU fixtures check 40 CRC encoding cases and 8,768 S/D format-conversion cases at O0/O2 with fatal UBSan.
- Added six unchanged BSD SoftFloat files from the existing pinned revision; exact Git content and SHA-256 provenance are retained. ABI v7 signatures/layout remain unchanged. The preceding fused/indexed milestone is published as `0535514`. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **211,850 indexed extents**, including **125,128 with PAC operations**, up from 211,163 under the same tail policy. **274 accepted extents contain CRC operations and 414 contain S/D format conversion.** **58,307 contain direct extent tails; 13,979 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Fifty-four representative game-derived samples compile into native objects, 40 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **105 Rust tests pass**, including CRC polynomial division, format conversion and all earlier fused/indexed SIMD semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **8,768 S/D format-conversion fixtures** and **40 CRC fixtures** match QEMU at O0/O2. Native CRC tests independently use polynomial division and standard string check values.
- **36,064 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **20,560 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **86,552 constructed encodings**, including **47,003 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 211,850 subset-liftable indexed extents with possible unbound tails/profiles; 54 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

CRC and format-conversion decoder/emitter/helper paths; mandatory and differential native tests; six pinned BSD sources/provenance; supplied-binary feature selection; decoder cross-check; STATUS/CPU/architecture/compatibility/research documentation. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), LD1 (270), FADDP (218), ZIP1 (203), FMAXNM (193), LD1R (183), ST1 (124), undecoded (117), FCVTZS (107), LD2 (84), SVC (79) and FRINTM (76). Counts are first blockers, not dynamic frequencies. Next add validated SIMD permutations, pairwise FP operations, structure transfers and scaled conversions. UDF/undecoded words require extent/CFG investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

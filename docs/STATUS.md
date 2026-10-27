# Status — October 27, 2026

Negated multiplication and conditional floating-point compares are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added S/D FNMUL with multiplication rounding/exceptions followed by raw result-sign inversion. In particular, NaN/default-NaN sign handling is preserved rather than re-canonicalized after negation. Native known-pattern and QEMU differential fixtures cover this behavior at O0/O2.
- Added S/D FCCMP/FCCMPE. Conditions inspect incoming NZCV. A true path invokes the checked compare backend; a false path writes encoded NZCV without calling the FP helper or changing FPSR. Missing backends refuse only on executed true paths. Unsupported half/reserved forms reject.
- Expanded mandatory condition truth-table tests across both widths, all conditions/NZCV, quiet/signaling NaNs, sticky flags, backend absence and unsupported controls on skipped paths. Optional synthetic QEMU cases additionally force every fallback NZCV value and non-default controls on false conditions.
- No layout/signature change from ABI v7. Existing conversions, thread-register state, bit/carry, FP arithmetic, barriers, SIMD, LSE, PAC and tail regressions remain passing. The preceding commits are `6781ee3`, `79911ed` and `3c2a657`; each was validated before source-only publication.
- Configured guest thread registers remain distinct from host TLS; cache/counter values and retail initialization remain unverified. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub, with required upstream notices retained.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **209,011 indexed extents**, including **122,773 with PAC operations**, up from 208,307 under the same tail policy. **342 accepted extents contain conditional FP compares.** **58,005 contain direct extent tails; 13,819 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Forty-six representative game-derived samples compile into native objects, 36 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **93 Rust tests pass**, including FNMUL NaN/default-NaN sign checks and conditional FP true/false/refusal behavior, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **16,708 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **85,500 constructed encodings**, including **46,479 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 209,011 subset-liftable indexed extents with possible unbound tails/profiles; 46 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Floating IR/decoder and native emitter; original native FP adapter/header operation contract; expanded mandatory/differential FP tests; CLI feature/backend requirements; decoder reference; STATUS/CPU/compatibility/research documentation. ABI v7 remains the layout/signature version. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include remaining SCVTF (609 extents), FMUL (560), FCVTZS (468), UDF (457), FMSUB (401), FCVT (373), FNMSUB (254), UCVTF (243), LD1 (215), CRC32B (165), FSUB (148) and FMAXNM (146). Counts are first blockers, not dynamic frequencies. Next add validated vector/SIMD conversions and arithmetic, remaining fused forms and scaled/format/structure operations. Target cache/counter/PAC/FP/address configuration, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

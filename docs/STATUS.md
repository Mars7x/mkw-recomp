# Status — October 26, 2026

Integer/FP conversions and scalar unary FP operations are implemented and tested; automatic source-only milestone publication continues. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added W/X signed and unsigned integer-to-S/D conversions (SCVTF/UCVTF), plus FP-to-W/X FCVTNS/NU, FCVTPS/PU, FCVTMS/MU, FCVTZS/ZU and FCVTAS/AU. Integer-to-FP follows FPCR rounding; FP-to-integer follows its encoded rounding mode, including ties away. Results saturate on invalid range and return zero for NaN under the inspected Arm specialization, raising cumulative IOC rather than substituting host casts.
- Added S/D FNEG, FABS and FSQRT. Sign operations preserve raw NaN payloads and FPSR even under default-NaN control; square root uses verified software arithmetic and cumulative exception flags. Unsupported half/control/trap/flush profiles still refuse before result mutation.
- ABI v6 appends a dedicated typed conversion callback. The borrowed FP backend installs/restores arithmetic and conversion callbacks together; missing backends and unverified library ABIs refuse. Width truncation, zero registers, V31, preserved flags and callback absence are tested. Integer sign interpretation uses exact-width bit copies rather than out-of-range signed C casts.
- Expanded the unchanged BSD SoftFloat subset from the same pinned revision with required conversion, square-root and primitive sources, retaining source hashes and notices. Original shared environment glue saves/restores thread-local rounding/tininess/exception state for both callback families. Helper identity includes the new source/header inputs.
- Added mandatory native conversion/saturation/refusal fixtures and optional differential QEMU batches. Batched synthetic ELF generation keeps literal addressing within bounds and records general-register outputs as well as vector bits, FPSR, NZCV and FPCR. Existing floating, barrier, LSE, SIMD, PAC and tail regressions pass.
- The validation-gated publisher continues to the explicitly approved public `Mars7x/mkw-recomp` repository. README, original inputs, generated game code, credentials and private artifacts remain excluded; required upstream licences remain included. Periodic continuation is configured in this chat, using the latest measured dependencies from this file.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **205,977 indexed extents**, including **120,453 with PAC operations**, up from 203,734 under the same tail policy. Accepted extents include 1,448 with integer/FP conversions and 5,413 with scalar FP operations. **57,719 contain direct extent tails; 13,689 contain indirect BR** (categories overlap). Runtime helpers/profiles, callees and indirect targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Thirty-eight representative game-derived samples compile into native objects, 29 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **85 Rust tests pass**, including known conversion results, integer saturation, all encoded FP-to-integer rounding variants, masks/sign/zero-register behavior, missing/unsupported callback refusal and shared-backend regression tests.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **13,348 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **85,028 constructed encodings**, including **46,132 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 205,977 subset-liftable indexed extents with possible unbound tails/profiles; 38 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

`native/cpu.h`, fp.c/new convert.c and fp_environment.h; floating IR/decoder/emitter and helper identity; runtime ABI/backend attachment; conversion, FP and synthetic-reference tests/shared support; CLI survey; tools/build_fp.py and check_decoder.py; pinned SoftFloat additions/provenance; STATUS/CPU/architecture/compatibility/research documentation. Original inputs were unchanged; generated outputs stay private. Source-only commits/pushes remain authorized.

## Next concrete dependencies

The latest first-blocker census starts with REV (796 extents), MRS (782), CLZ (760), FNMUL (679), remaining SCVTF variants (588), FMUL (528), UDF (457), remaining FCVTZS variants (450), FCCMP (446), FCVT (351), UCVTF (232) and LD1 (204). Counts are first blockers, not dynamic frequencies. Next add validated scalar bit operations and remaining fused/conditional FP, then fixed-point/format and vector/structure operations. Extend control profiles only with functional/exception fixtures. System registers, ISB, exclusive monitors, device/weak-memory behavior, indirect discovery/binding, target PAC/FP/address configuration and bootstrap/services remain separate gates before original-function execution.

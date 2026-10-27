# Status — October 27, 2026

Scalar bit permutations/counting and carry-chain arithmetic are implemented and tested; automatic source-only milestone publication continues. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added W/X RBIT, REV16, REV/REV32/REV64 as appropriate for width, CLZ and CLS. Zero/all-ones and sign-boundary behavior is defined, including CLZ(0)=width and CLS excluding the sign bit. Reserved W REV64 and neighbouring instruction encodings reject. Results preserve NZCV and W writes zero-extend.
- Added ADC/ADCS and SBC/SBCS, including NGC aliases and zero-register/discarded-result forms. Incoming carry is captured before state updates. Emitted unsigned arithmetic computes carry in two stages and avoids host signed overflow; optional flag updates and width truncation are explicit.
- New native fixtures compare bit permutations/counts with independent Rust builtins/byte operations and carry results with wide signed/unsigned mathematical references across boundary operands and both carry inputs. O0/O2 use fatal UBSan. No new runtime fallback or ABI change was required.
- The preceding published milestone (`6781ee3`) added SCVTF/UCVTF, encoded-rounding FP-to-integer conversions, FNEG/FABS/FSQRT and ABI v6 conversion callbacks. Its 7,232 conversion and 13,348 FP/selection QEMU fixtures remain passing, alongside source provenance and restricted-control refusal checks.
- Periodic continuation is active in this chat and reads this file for the next measured dependency. The publisher retains the explicit public-visibility check and source-only exclusions. No README, original inputs, generated game code, credentials or private artifacts are published; upstream notices are retained.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **207,772 indexed extents**, including **121,685 with PAC operations**, up from 205,977 using the same tail policy. Accepted extents include **1,795 with the new integer bit/carry operations**. **57,840 contain direct extent tails; 13,768 contain indirect BR** (categories overlap). Helpers/profiles, callees and indirect targets can remain unbound or incomplete; these are not runnable-function or complete-module counts. **Forty-two representative game-derived samples compile into native objects, 33 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **88 Rust tests pass**, including independent native bit/carry reference checks, reserved encodings, zero/discarded registers, preserved flags and all prior conversion/FP/runtime regressions.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **13,348 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **85,148 constructed encodings**, including **46,237 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 207,772 subset-liftable indexed extents with possible unbound tails/profiles; 42 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

ARM64 IR/integer decoder; integer emitter; independent native bit/carry tests; CLI feature survey; optional Capstone reference cases; STATUS/CPU/compatibility/research documentation. No ABI change from v6. Original inputs remain unchanged and all game-derived generated outputs remain private. Validated source-only commits/pushes remain authorized.

## Next concrete dependencies

The latest first blockers are MRS (783 extents), FNMUL (682), remaining SCVTF variants (590), FMUL (529), UDF (457), remaining FCVTZS variants (451), FCCMP (447), FCVT (355), UCVTF (237), LD1 (205), FMSUB (203) and FNMSUB (195). Counts are first blockers, not dynamic frequencies. Next investigate remaining system-register requirements without substituting host state, and validate remaining fused/conditional FP before scaled/format conversions and vector/structure operations. Unsupported instructions must not become success/no-op exits just to increase coverage. Target configuration, indirect binding, exception/exclusive/device/weak-memory behavior and bootstrap/services remain gates to original-function execution.

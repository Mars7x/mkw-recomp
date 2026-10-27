# Status — October 26, 2026

Guest FP state, checked scalar floating-point operations and explicit data barriers are implemented and tested. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added ABI v5 FPCR/FPSR state, a floating-point callback and a separate DMB/DSB callback. MRS/MSR support the diagnostic legacy FPCR/FPSR/NZCV registers, including zero-register handling and masked FPSR writes. Unsupported FPCR fields refuse; supported register storage does not imply every stored control profile can execute arithmetic.
- Added S/D FADD, FSUB, FMUL, FDIV, FCMP/FCMPE (register and zero) and scalar FMADD. Arithmetic supports four rounding modes and default-NaN control, with gradual underflow and cumulative IEEE exception flags in FPSR. Enabled traps, flush modes, alternative/merge profiles and half-precision arithmetic currently refuse before committing results. Architectural access/enable traps and exception delivery remain absent.
- Reused a narrow BSD-3-Clause Berkeley SoftFloat subset pinned at `497c81c1f5016dc6ef5b3219ece2f3c261d4d8f1`, retaining all source notices, COPYING and file hashes. Vendored sources are unmodified. Original C glue selects Arm NaN behavior, maps exception flags and saves/restores thread-local SoftFloat state. Differential tests exposed FMADD's addend-first NaN priority; the adapter now handles this plus the NaN/zero/infinity cases explicitly. Fused cancellation tests distinguish one-rounding FMADD from separate multiply/add.
- Added H/S/D FCSEL raw-bit selection, covering all conditions and preserving NZCV/FPSR, without numeric conversion or quieting NaNs. Existing SIMD, LSE, PAC and tail-routing behavior remains tested.
- DMB/DSB preserve their domain/access option tags and require an attached backend. The owned normal-memory backend takes the shared memory mutex and a sequentially consistent fence. All operations are synchronous; the profile deliberately strengthens barrier domains. Devices, cache/TLB maintenance, ISB, asynchronous completion and full target weak-memory outcomes remain unsupported.
- Added a borrowed native FP-library attachment, ABI layout checks, source/build receipts and helper-source inputs in translator identity. `tools/build_fp.py` verifies the vendored source identities, builds only the selected helpers and writes compiler/flags/source/library hashes privately. No host-native FP arithmetic, guest instruction interpreter or JIT fallback was introduced.
- The validation-gated publisher targets `Mars7x/mkw-recomp`. After GitHub reported public visibility and an externally changed description, the user explicitly approved source/documentation publication. The publisher preserves those settings and checks an explicit expected-visibility option. Automatic source-only commits/pushes remain authorized. README, original inputs, generated game code, credentials and private artifacts stay excluded; required upstream notices are retained.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **203,734 indexed extents**, including **118,607 with PAC operations**, up from 198,615 using the same exact-extent tail policy. Accepted extents include 3,515 with scalar FP operations, 863 with FCSEL and 1,554 with data barriers. **57,488 contain direct extent tails; 13,574 contain indirect BR** (categories overlap). FP/control/PAC/memory backends and callees may remain unbound, the game FP profile is unverified, and BR-only targets are not automatically discovered. These counts establish neither runnable functions nor complete-function/module coverage. **Thirty-six representative game-derived samples compile into native objects, 26 containing PAC; none were executed.** Sample selection tracks first feature representatives rather than accumulating objects across revisions.

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **82 Rust tests pass**, including FP state masks/readback, missing/unsupported backend refusal with unchanged results, thread-local rounding isolation, fused cancellation/NaN priority, all FCSEL conditions and widths, barrier options, and threaded barrier publication. Earlier SIMD/LSE/PAC/control-flow regressions remain passing.
- **9,604 FP/selection fixtures match QEMU 8.2.2 at O0/O2 with fatal UBSan**: rounding modes, default-NaN control, mixed quiet/signaling NaNs, infinities, signed zeros, subnormals, exception accumulation, compare-zero, fused behavior and all incoming NZCV/FCSEL conditions. QEMU is a separate software differential path, with related SoftFloat lineage; it is not original-hardware validation or proof of every IEEE/Arm corner case. Only synthetic instructions execute.
- Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software reference comparisons remain passing. QEMU fixtures record explicit unavailable receipts when that optional executable is absent.
- **83,520 constructed encodings**, including **45,776 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 203,734 subset-liftable indexed extents with possible unbound tails/profiles; 36 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

`native/cpu.h`, new fp.c/platform glue; new ARM64/codegen floating.rs and IR/source identity; runtime ABI/bridge/new floating.rs attachment; FP/barrier/reference/selection tests; CLI survey.rs; tools/check_decoder.py/new build_fp.py; pinned third_party/softfloat subset and provenance. CPU, architecture, compatibility, services, roadmap, research and status documentation updated. All game-derived code, binaries and receipts stay private. Original inputs were not modified. Publication contains distributable source/documents and required upstream notices only, without README.

## Next concrete dependencies

The refreshed first-blocker census starts with SCVTF (1,537 extents), FNEG (929), REV (795), MRS (782), CLZ (750), FCVTZS (715), UCVTF (615), FSQRT (599), remaining FMUL variants (471) and UDF (457). Counts are first blockers per extent, not dynamic frequencies. Next add validated conversions and unary/remaining fused/vector FP operations, then scalar bit operations; extend FP controls only with functional/exception fixtures. Remaining system registers, ISB, exclusive monitors, devices and hardware weak-memory validation are separate dependencies. Discover/bind indirect targets, verify PAC/address/execute/FP configuration, then implement startup/RelRo/TLS and services before original-function execution.

# Status — October 27, 2026

Explicitly configured guest thread-register state is implemented and tested, following the published conversion/unary and scalar bit/carry milestones. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Investigated the remaining first-blocker MRS instructions: 416 extents read TPIDR_EL0, 339 read TPIDRRO_EL0, five read CTR_EL0 and 23 read CNTPCT_EL0. These were first blockers per indexed extent, not dynamic instruction frequencies. Cache/counter values are not inferred from the host.
- ABI v7 adds raw guest TPIDR_EL0/TPIDRRO_EL0 fields and an explicit configuration gate. MRS reads both; the EL0 model permits MSR to TPIDR_EL0 and rejects writes to TPIDRRO_EL0. Access refuses until Cpu::configure_thread_registers is called. No host thread pointer is exposed, synthesized or cast into guest memory.
- Native tests verify configured reads/writes, zero/discarded registers, full 64-bit values, unchanged NZCV/SP, refusal without configuration and two CPU instances reading distinct guest blocks concurrently through checked memory. Unmapped guest pointers fault transactionally. This does not allocate TLS, initialize retail TLS templates, create guest threads or implement a scheduler.
- The preceding validated commits are `6781ee3` (conversions/unary FP) and `79911ed` (scalar bit operations/carry chains). Their numeric/control/reference tests remain passing. Source-only automatic commit/push and periodic continuations remain active; README, game inputs/generated code, credentials and private artifacts stay excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **208,307 indexed extents**, including **122,198 with PAC operations**, up from 207,772 under the same tail policy. **535 accepted extents require explicit thread-register configuration.** **57,924 contain direct extent tails; 13,792 contain indirect BR** (categories overlap). Configuration/helper/callee/indirect-target dependencies remain unresolved; these are not runnable-function or full-module counts. **Forty-four representative game-derived samples compile into native objects, 35 containing PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **91 Rust tests pass**, including new thread-register configuration/access/permission tests and concurrent checked guest-block reads, plus all prior native bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **13,348 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **85,244 constructed encodings**, including **46,333 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 208,307 subset-liftable indexed extents with possible unbound tails/profiles; 44 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Shared CPU header/ABI version and thread-register configuration API; system-register IR/decoder/emitter; native thread-register tests; ABI safety comments; CLI configuration requirements/features; optional decoder reference; STATUS/CPU/architecture/services/compatibility/research documentation. No original input was modified; all generated game-derived outputs remain private.

## Next concrete dependencies

The latest first blockers start with FNMUL (682 extents), remaining SCVTF variants (594), FMUL (539), UDF (457), remaining FCVTZS variants (451), FCCMP (447), FCVT (360), UCVTF (238), LD1 (207), FMSUB (203) and FNMSUB (195). Only 28 first-blocker MRS cases remain (cache/counter requirements). Next validate remaining fused/conditional FP and scaled/format/vector/structure operations. Do not turn unsupported instructions into success/no-op exits for coverage. Real TLS templates/allocation, guest threading, verified cache/timer/PAC/FP/address configuration, indirect binding and bootstrap/services remain separate gates before original execution.

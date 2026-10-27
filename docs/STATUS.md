# Status — October 26, 2026

Raw SIMD register moves/immediates and checked scalar LSE atomics are implemented and tested. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added raw H/S/D FMOV register copies, GPR-to/from-FP moves including the high D lane, scalar/vector FP immediate bit construction, MOVI/MVNI and ORR/BIC modified immediates. Constants are expanded ahead of execution without host floating-point arithmetic. Scalar/64-bit results clear unused upper vector bits; lane inserts preserve other bits. XZR and V31 remain distinct.
- Added scalar/element/GPR DUP, element/GPR INS and MOV aliases, UMOV/SMOV, all eight vector logical/bit-select forms (AND/BIC/ORR/ORN/EOR/BSL/BIT/BIF), and EXT byte extraction. Source/destination overlap is tested. Reserved type, width, lane, immediate and opcode combinations reject. FP arithmetic, conversions, FPCR/FPSR/access traps and non-default FP merge behavior remain unsupported.
- Advanced the refreshed dependency census to scalar LSE: LDADD/LDCLR/LDEOR/LDSET, signed/unsigned min/max, SWP and CAS, including byte/halfword/word/doubleword and acquire/release forms. CAS returns the zero-extended old value and updates memory only on comparison success; register/base overlap, zero registers and discarded results are tested. Acquire suppression for discarded RMW results is retained in IR.
- Shared ABI v4 appends an atomic-memory callback. Missing backends refuse execution. The checked runtime validates natural alignment and all read/write spans, then performs the complete operation under the **same mutex used for plain and ordered memory**. Faults preserve guest destinations and memory. Cross-mapping aligned operations work. This is a stronger serialized normal-memory diagnostic model; it does not validate Switch 2 weak-memory outcomes, devices, LSE2 unaligned profiles or exclusive-monitor behavior.
- Added synthetic AArch64 ELF construction and optional QEMU execution as an independent software oracle. QEMU runs independently authored fixtures only, never supplied game code and never as an execution fallback. Native tests retain O0/O2 with fatal UBSan. Existing PAC, tail-routing, arithmetic and memory regressions pass.
- Created private GitHub repository `Mars7x/mkw-recomp` with no description or README. `tools/publish.py` validates before committing/pushing a source-only snapshot through `private/publish`, since the workspace's `.git` mount is read-only. Original inputs, private/generated artifacts, credentials and READMEs are excluded. The tool installs no background watcher; publication is invoked at validated milestones under the user's authorization.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **198,615 indexed extents**, including **114,486 with PAC operations**. Of accepted extents, 2,757 contain the new SIMD register-bit operations and 6,452 contain scalar LSE operations. **56,954 contain direct extent tails; 13,370 contain indirect BR** (categories overlap). The prior count was 189,943; the same exact-extent tail policy is used. Tail destinations and callees may remain unbound, and BR-only targets are not automatically discovered: these counts establish neither runnable functions nor complete-function/module coverage. **Thirty representative game-derived samples compile into native objects, 23 containing PAC; none were executed.** Sample selection follows first feature representatives, so sample totals can change rather than accumulate across revisions.

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **77 Rust tests pass**, including new immediate/reserved encoding checks, mandatory native known-pattern SIMD checks, optional independent SIMD/LSE execution comparisons, all LSE widths/orders/operations, CAS success/failure, permissions/alignment/gaps, callback absence, ABI layout and contention. Four native threads produce each old fetch-add value exactly once across 4,000 operations; another 4,000 contended native CAS increments finish with no lost updates.
- **2,741 SIMD fixtures and 160 LSE fixtures match QEMU 8.2.2 (`-cpu max`)** at native O0/O2 with fatal UBSan. These are software functional comparisons under the fixture's default FP state, not hardware or weak-memory validation. QEMU tests record an explicit unavailable receipt if the optional local executable is absent.
- **81,892 independently constructed encodings** were checked with private Capstone 5.0.6; all **44,954 accepted encodings** pass checked family/width/immediate comparisons. This is a software decoder check, not an ARM hardware execution oracle.
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
| 2 game CPU translation | Partially implemented | 198,615 subset-liftable indexed extents with possible unbound tails; 30 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

`native/cpu.h`; ARM64/codegen new simd.rs and atomics.rs plus IR/source identity; runtime new atomics.rs, ABI/bridge and ABI safety documentation; mandatory SIMD/native LSE tests and optional QEMU fixtures; CLI survey.rs; tools/check_decoder.py/new publish.py; Cargo manifests/lock. CPU, architecture, compatibility, services, roadmap, research and status documentation updated. All game-derived code, binaries and receipts remain under ignored private/build locations. Original inputs were not modified. Repository publication contains distributable source/documents only, with README excluded as requested.

## Next concrete dependencies

The refreshed first-unsupported-instruction census starts with FCMP (2,576 extents), FMUL (2,228), DMB (1,749), FSUB (1,337), SCVTF (1,151), FADD (857), REV (792), MRS (786) and CLZ (746). These are first blockers per extent, not dynamic frequencies; unresolved CFG edges and unbound tail targets are separate limitations. Next define guest FPCR/FPSR and exact FP exception/rounding behavior before arithmetic/conversions; implement explicit barrier semantics for the chosen normal-memory profile, plus remaining scalar bit operations. Exclusive monitors, device memory and target weak-memory validation remain separate work. Bind/discover indirect targets, verify PAC/address/execute configuration, then implement startup/RelRo/TLS and required services before original-function execution.

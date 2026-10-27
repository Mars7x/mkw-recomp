# Source research and architecture decision

Checked October 26, 2026. Public repository metadata, current default-branch trees, branch heads, three recent commits and selected implementation files were fetched directly. Exact revisions, file hashes and branches are in [upstream-audit.json](upstream-audit.json). This is a focused source audit, not a full provenance/security review or reproduction of upstream gameplay results. No upstream build or gameplay test was run here.

## Technical baseline

Switchbrew describes T239's Cortex-A78C CPU, Ampere GPU and pointer-authentication features. These establish hardware requirements, not the exact instructions used by an individual executable. [T239 documentation](https://switchbrew.org/wiki/Switch_2:_Tegra_T239).

NSO documentation specifies segment hashes and LZ4, plus newer XOM and Zbic flags. The supplied modules use LZ4; Zbic is detected and rejected, because ordinary zstd decoding has not been established for that variant. [NSO format](https://switchbrew.org/wiki/NSO).

The NPDM ACID key/signature fields increase from 2048 to 3072 bits on Ounce; the supplied metadata has that larger layout. PAC, executable permissions and software CFI need separate semantics. [NPDM](https://switchbrew.org/wiki/NPDM), [mitigations](https://switchbrew.org/wiki/Security_Mitigations).

Nintendo lists update 1.8.0 dated September 9, 2026 in North America. This does not identify the supplied dump: no trusted retail version mapping, CNMT or control NACP was supplied. Version strings remain unverified. [Nintendo update record](https://en-americas-support.nintendo.com/app/answers/detail/a_id/68580).

## Compared implementations

### mk8-recomp

Audited `7690b2d1cbe8096d6ec826723fa5601c8e7d0d9e`, default `main`, and recorded the native-runtime branch. Recent changes concern guard-refusal reporting and decode coverage. The tree contains an older suyu integration and a newer prepared Belmante-runtime path. `generate.py` verifies segment hashes and generates separate private modules at fixed guest bases. The offline shader path is a Switch 1 reference, not verified Ampere support. CPU dispatch, memory and threading depend on the prepared external runtime; their complete implementation is outside this snapshot. Historical performance numbers are upstream claims, not local results. The repository lacks a root LICENSE; GPL notices, MIT upstream attribution and dependency obligations require a file-level audit before adoption. Useful architectural reference, not a drop-in Switch 2 foundation. [Source](https://github.com/dougchansan/mk8-recomp/tree/7690b2d1cbe8096d6ec826723fa5601c8e7d0d9e), [generator](https://github.com/dougchansan/mk8-recomp/blob/7690b2d1cbe8096d6ec826723fa5601c8e7d0d9e/integrations/belmante-runtime/generate.py).

### WindowSmash

Audited `8025b483b0443adb5668395eca31f9250e8c8efb`, `main`; Linux/macOS and performance branches also exist. Recent history includes credential-log fixes and runtime-package removal. `funcs.py` uses unwind records, symbols, relocations, constructors and BL targets; `lift.py` emits C, indirect dispatch and traps for unsupported instructions. Crucially, its hint mask discards PAC hints, while other PAC operations are unsupported: unsafe to transfer unchanged to this dump. The native kernel uses translated guest addresses and host synchronization/thread APIs. Graphics are D3D-oriented; Linux integration is not our intended Vulkan backend. It targets Switch 1/A57 assumptions, not proven A78C/Ampere behavior. Actual current LICENSE is GPLv3; any adoption needs matching notices and dependency review. Function-discovery strategy is useful; complete title compatibility is not independently reproduced. [Source](https://github.com/OpenSmash-Ultimate/WindowSmash/tree/8025b483b0443adb5668395eca31f9250e8c8efb).

### LASRecomp

Audited `a4b53a3fc5e91bf101367063bafe45017dc23335`, `main`; beta6 and other active branches were recorded. Recent history is release/build work. `gen.py` partitions generated C, retains indirect dispatch and uses emulation-assisted jump-table discovery. `lifter.py` supports portable C or host AArch64 SIMD assembly; the latter cannot serve x86-64. Generic hint lowering silently discards PAC hints. The CPU helpers and kernel include host-pointer memory assumptions and pthread-backed threads/condition variables. Vulkan/Metal graphics and `nvn/maxwell/translate.c` explicitly target Switch 1. Its own code is MIT under the actual LICENSE, with third-party exclusions. This is the leading candidate for a future portable lifting adapter, contingent on semantic tests, checked-memory integration and explicit PAC handling. It is not currently incorporated or validated against this game. [Source](https://github.com/peiikai/LASRecomp/tree/a4b53a3fc5e91bf101367063bafe45017dc23335).

### smm2recomp

Audited `8c0b2d417cd8fc86294bf1d0fc08795ebd3569e3`, `main`; development branches recorded. Recent changes concern a paused release, online integration and a format-string fix. The inspected lifter is byte-identical to LASRecomp's; generated C, indirect-dispatch structure and host kernel therefore share important constraints. There are concrete Vulkan device/execution/shader sources and a Maxwell translator. None establish Ampere shader support. Runtime CPU/kernel code retains native threading and address-model dependencies. The root license is GPLv3, while LAS attribution is separately retained; it must not be treated as wholly MIT. Existing implementation breadth is useful evidence for Switch 1 engineering patterns, but title claims and hardware correctness were not reproduced locally. Its online/mod features are outside this project's initial scope. [Source](https://github.com/smm2recomp/smm2recomp/tree/8c0b2d417cd8fc86294bf1d0fc08795ebd3569e3).

### SwitchRecomp

Audited `3c635ee893dc2f633fead4a60dd1fb19178619ce`, sole branch `main`; history contains an initial research framework and CI fix. The source implements a table decoder, typed IR, C code generation, memory/thread/runtime components and Vulkan tests. The decoder has a limited instruction table; no inspected implementation establishes the supplied PAC/SIMD instruction requirements. The GPU command processor skips unregistered methods, so accepting a buffer cannot certify its effects. Synthetic compute/shader paths are not commercial-game rendering evidence. Full indirect-flow and concurrent-memory equivalence are unverified. Actual LICENSE is GPLv3. This is an informative research skeleton, not a more mature replacement for the inspected Switch 1 lifters or a verified Switch 2 runtime. No code adopted. [Source](https://github.com/kaiyao7/SwitchRecomp/tree/3c635ee893dc2f633fead4a60dd1fb19178619ce).

### OunceCake

Audited `e3bcbd229be2229d44f036455adddda921ade3b2`, sole branch `main`. Recent commits fix binary FTP transfers and crashes. `program.cjs` and `content.cjs` concern installed-content selection, mounting and transfer; this is a dumper, not a CPU/GPU recompiler. There is no lifter, native-game runtime, shader translator or thread model to adopt for recompilation. Its source cannot certify that a particular supplied dump is complete. GPLv2 license and Atmosphere provenance are stated in the actual files. No dumping/exploit components or bundled native payload were executed or adopted. FTP listings in the supplied dump are useful for a limited size/presence comparison. [Source](https://github.com/misshexa66/OunceCake/tree/e3bcbd229be2229d44f036455adddda921ade3b2).

### SuperMarioWonderRecomp and other claims

Direct API/source access to `BelmanteGu/SuperMarioWonderRecomp` returned 404 during this audit, while web search retained a cached README. Consequently its current branch, code, license and claimed platform results could not be verified. mk8-recomp pins an older external revision, which is evidence of a dependency declaration, not access to that implementation. Do not import it based on cached MIT or gameplay claims. [Referenced repository](https://github.com/BelmanteGu/SuperMarioWonderRecomp).

A search also surfaced Mario Kart World native-execution claims without an inspected source/validation path. They supply no reusable verified foundation. No inspected source establishes a tested native Switch 2 Ampere graphics path for this title.

## Decision

Use Rust for the dump identity/format boundary, typed analysis, diagnostics and safe guest-memory model. Use generated C compiled by a host compiler for the initial semantic harness. Existing LZ4/zstd libraries and Rust serialization/hash libraries are reused; mature compression is not reimplemented.

Do not fork a full Switch emulator or copy a Switch 1 graphics runtime. Evaluate a narrowly isolated LASRecomp portable-lifter adapter after an instruction census and conformance suite; its current PAC and memory assumptions prevent direct adoption. The small original emitter implemented here is a testable ABI/semantic baseline, not a claim to have replaced a mature full lifter. Wholesale adoption is deferred for concrete technical reasons, not because a new general-purpose recompiler is assumed easier.

The graphics work remains a separate investigation of actual NVN bootstrap calls, command generation, shader containers and Ampere instructions. Shader container signatures do not establish shader instruction encoding.

## Independent format cross-check

SARC header/node/name layout was checked against oead `5fb74e1128468380b47da23a5690891c8af4ab72`, including its version and header-size checks. Its GPLv2-or-later implementation was inspected, not copied. Our inspector only validates containment; it is not a full asset parser. ELF relocation terminology follows the public Arm ABI reference. [oead parser](https://github.com/zeldamods/oead/blob/5fb74e1128468380b47da23a5690891c8af4ab72/src/sarc.cpp), [Arm ELF ABI](https://github.com/ARM-software/abi-aa/blob/main/aaelf64/aaelf64.rst).

## Feasibility and risk register

Static CPU translation is technically plausible; the current synthetic native result demonstrates only the small implemented subset. There is no evidence yet that the complete title can boot on this foundation. A usable native port remains a substantial research and implementation effort.

| Risk | Evidence and next validation |
|---|---|
| Authenticated control flow | PAC/RETAB observed; need guest PAC conformance and failure-behavior tests |
| Symbol scope/relocation semantics | Real diagnostic linker rejects a duplicate export; establish dependency lookup rules |
| Incomplete control-flow discovery | Metadata has many entries; extents, jump tables, callbacks and loaded modules remain unresolved |
| Ampere graphics/shaders | Containers observed, no verified ISA/API backend; investigate real SDK submission path |
| Floating point, atomics, concurrency | No game-semantic tests or ARM reference execution; require independent differential fixtures |
| Input completeness/version | Listings match sizes, but no trusted complete-content or retail-version mapping |
| Build scale/performance | Many executable words; no complete native build, memory bound or runtime benchmark |
| Upstream reuse/licenses | File-level dependency/provenance audits remain necessary before copying components |

## Follow-up: supplied rtld scopes and PAC implementation

Static disassembly of the hash-verified supplied rtld revealed requester-local lookup for non-default visibility and ordered process-list lookup for default symbols. DT_NEEDED closure is therefore one explicit policy; the supplied-title diagnostic uses a separate rtld-first/address-ordered process scope. Exact offsets and evidence stay in private/analysis/rtld-link-semantics.json. This is static inference, not a reproduced retail startup trace. Generic rtld documentation describes memory-region enumeration but does not prove symbol order. [Switchbrew rtld](https://switchbrew.org/wiki/Rtld).

A QARMA/PAC primitive has now been adopted. gem5 revision fed0e07e930fecf1e73dce94dd44e69a5bf70c46 files src/arch/arm/qarma.cc and qarma.hh were adapted into standalone C; pauth_helpers.cc informed the restricted legacy wrapper. Full Arm/Metempsy BSD notices and provenance are retained. Only a C compiler is required. The explicit diagnostic profile is not confirmed Switch 2 PAC configuration. [gem5 source](https://github.com/gem5/gem5/tree/fed0e07e930fecf1e73dce94dd44e69a5bf70c46/src/arch/arm).

The QARMA core matches 4,096 inputs against QEMU revision d73149283205645625bfa9fe029cfb68e4c4a9ae. That GPL source stays private and builds only the independent comparison harness; it is not part of normal/public implementation. This is software differential evidence without an ARM/Switch 2 hardware oracle. [QEMU reference](https://github.com/qemu/qemu/blob/d73149283205645625bfa9fe029cfb68e4c4a9ae/target/arm/tcg/pauth_helper.c). Arm documents key/SP relationships and authenticated returns; profile-specific behavior still needs verification. [Arm discussion](https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/code-reuse-attacks-the-compiler-story).

## Follow-up: ordered-memory contract

The supplied-binary census exposed LDAPRB among the first unsupported instructions. Arm's compiler discussion distinguishes RCpc acquire from the stronger ordering constraints of other acquire operations. The implementation keeps RCpc acquire, RCsc acquire and release distinct in typed IR and ABI tags. The current Rust bridge serializes all accesses, so its tests establish a stronger diagnostic execution model rather than preservation of every weak-memory outcome. No claim of Switch 2 concurrency equivalence is made. [Arm RCpc compiler discussion](https://developer.arm.com/community/arm-community-blogs/b/tools-software-ides-blog/posts/enabling-rcpc-in-gcc-and-llvm), [Arm ordering discussion](https://developer.arm.com/community/arm-community-blogs/b/architectures-and-processors-blog/posts/when-a-barrier-does-not-block-the-pitfalls-of-partial-order).

## Raw SIMD transport and integer mask validation

The current incremental implementation follows Arm's architectural instruction formats and mask definitions for ordinary scalar/SIMD transfers, logical/bitfield/extract operations and extended arithmetic. New code is original; no implementation copied from these references. Raw vector transport does not establish FP arithmetic, access-trap or FPCR/FPSR semantics. [Arm A64 reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab), [Arm SIMD register-offset LDR](https://developer.arm.com/documentation/111181/2025-09_ASL1/SIMD-FP-Instructions/LDR--register--SIMD-FP---Load-SIMD-FP-register--register-offset--).

Private Capstone 5.0.6 independently classifies generated encoding cases; accepted memory cases are additionally checked for transfer size and register bank. Native x86-64 tests use independently written bit-copy and circular-run oracles, plus fatal UBSan at O0/O2. These checks complement each other; neither is original AArch64 hardware validation. Receipts remain private and the optional validation script skips the Capstone check when its private installation is absent.

## BR/tail, conditional compare and integer arithmetic validation

Arm's instruction reference defines BR without a link-register update, conditional compares with either computed or immediate flags, and separate low/widening/high products and signed/unsigned division. The implementation uses original code for these semantics; no source implementation was copied. The diagnostic runtime routes unresolved tail requests only to registered native functions, retaining missing-target failures rather than inventing destinations. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

Exact CIE/FDE extent surveys now permit explicit unconditional B tails to other entries; this changes the survey acceptance policy and must accompany any comparison with previous counts. Conditional escapes and strict slice truncation still reject. Native arithmetic checks use independent Rust wide-integer calculations and a condition truth table; Capstone separately classifies encoding families. Neither confirms the supplied game's PAC/BTI profile or indirect-target completeness. Original functions remain unexecuted.

## Raw SIMD bits and serialized LSE evidence

Arm documents FMOV as a bit transfer distinct from numeric conversion, and defines MOVI constant replication and vector lane transfers. The new decoder resolves compact FP and modified vector immediates to bits ahead of execution. New implementation and ELF fixture construction are original code; no Arm pseudocode or QEMU implementation was copied. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab), [Arm FMOV general reference](https://developer.arm.com/documentation/111182/2025-09_ASL1/SIMD-FP-Instructions/FMOV--general---Floating-point-move-to-or-from-general-purpose-register-without-conversion-).

The supplied census then exposed scalar LSE operations. They now use a checked serialized normal-memory profile with explicit ordering tags. QEMU's translator was consulted as a separate implementation reference: it also uses stronger atomic primitives rather than establishing all allowed weak outcomes. Its LGPL source was inspected, not incorporated. Independent, locally constructed AArch64 ELFs compare functional SIMD/LSE results against installed QEMU 8.2.2; native contention tests separately exercise the Rust callback bridge. Neither check proves original-hardware weak-memory behavior, FPCR/FPSR semantics or the game's PAC/BTI configuration. [QEMU A64 translator](https://github.com/qemu/qemu/blob/master/target/arm/tcg/translate-a64.c).

## Pinned SoftFloat adoption and data barriers

Berkeley SoftFloat was inspected at `497c81c1f5016dc6ef5b3219ece2f3c261d4d8f1`. Its BSD-3-Clause COPYING text and per-file notices permit the selected redistribution. Only the required S/D arithmetic/fused helpers, supporting primitives/headers and ARM-VFPv2 specialization were incorporated, unchanged, with per-file hashes. Project platform/build/ABI glue is separate. This reuses a mature numeric implementation rather than host FP or a new general-purpose arithmetic engine. [Pinned source](https://github.com/ucb-bar/berkeley-softfloat-3/tree/497c81c1f5016dc6ef5b3219ece2f3c261d4d8f1), [licence](https://github.com/ucb-bar/berkeley-softfloat-3/blob/497c81c1f5016dc6ef5b3219ece2f3c261d4d8f1/COPYING.txt), [thread-local/specialization documentation](https://www.jhauser.us/arithmetic/SoftFloat-3/doc/SoftFloat-source.html).

Native O0/O2 fixtures compare result bits, cumulative FPSR flags and NZCV with QEMU 8.2.2 under selected rounding/default-NaN profiles. Fused tests exposed addend-first NaN selection that the binary SoftFloat specialization alone did not express; original adapter logic corrects this and tests zero/infinity/NaN combinations. QEMU is a distinct software code path with related SoftFloat lineage, not a hardware oracle. Flush modes, enabled traps and broader FP profiles remain unsupported rather than inferred to work.

Arm distinguishes data ordering from completion and instruction synchronization. The synchronous owned-memory backend explicitly strengthens DMB/DSB domain/access tags through the shared mutex/fence; it cannot establish device/cache/TLB completion or ISB behavior. Synthetic publication tests validate that chosen diagnostic model only. [Arm memory-ordering/barrier discussion](https://developer.arm.com/community/arm-community-blogs/b/architectures-and-processors-blog/posts/memory-access-ordering-part-3---memory-access-ordering-in-the-arm-architecture).

## Conversion and unary extension

The selected unchanged SoftFloat sources now include signed/unsigned integer conversions and square root from the same pinned revision. The inspected ARM-VFPv2 specialization defines saturation and zero-for-NaN results; native tests compare exact invalid and inexact flags with separate QEMU execution. Instruction rounding is kept distinct from FPCR rounding. No native floating casts or host fenv state stand in for guest semantics. The shared project environment helper scopes the vendored thread-local state. [SoftFloat operation/exception documentation](https://www.jhauser.us/arithmetic/SoftFloat-3/doc/SoftFloat.html), [pinned Arm specialization](https://github.com/ucb-bar/berkeley-softfloat-3/blob/497c81c1f5016dc6ef5b3219ece2f3c261d4d8f1/source/ARM-VFPv2/specialize.h).

## Scalar bit and carry validation

The next supplied-binary blockers included scalar REV and CLZ. Their fixed-width permutations/counts, plus RBIT/CLS and carry-chain operations, now lower through original unsigned C. Reserved width/opcode fields remain errors. Independent native tests compare bit operations with Rust builtins/byte swaps and carry/overflow flags with wide mathematical results, rather than copying the emitter's flag formula into the oracle. [Arm A64 bit-operation overview](https://developer.arm.com/-/media/Files/pdf/graphics-and-multimedia/ARMv8_InstructionSetOverview.pdf).

## Thread-register requirements

A private first-MRS census identified mostly software thread-register reads, with a smaller cache/counter remainder. LLVM's system operand definitions corroborate the architectural field encodings. QEMU's register table separately records EL0 read/write access for TPIDR_EL0 and EL0 read-only access for TPIDRRO_EL0; those GPL source records were inspected, not copied. The project uses original per-Cpu storage and an explicit configuration gate, not emulator register infrastructure or host TLS. [LLVM system operands](https://github.com/llvm/llvm-project/blob/main/llvm/lib/Target/AArch64/AArch64SystemOperands.td), [QEMU architectural register access table](https://github.com/qemu/qemu/blob/master/target/arm/helper.c).

Tests use independently authored system-register field encodings and synthetic guest blocks. Thread-register values and layout are not inferred from Linux/QEMU defaults or asserted to be the retail application's TLS ABI. Cache geometry and counter frequencies remain unverified and are not supplied as guessed constants.

## Negated multiplication and conditional FP

A private synthetic probe compared FNMUL with multiply followed by raw result-sign inversion, including default-NaN mode; the expanded native/QEMU matrix now tests that behavior directly. The adapter delegates numeric multiply to the established scoped helper and inverts the processed result, avoiding a second default-NaN normalization. Conditional FP compares were validated against independent condition truth tables and QEMU for every incoming condition flag combination, including skipped signaling-NaN operands and forced fallback flags. No original executable was run and no reference implementation source was copied.

## SIMD numeric helper reuse

The new vector path uses the already attributed scalar SoftFloat adapters per lane, with original source snapshots, packing and commit logic. Independently authored AArch64 fixtures now initialize full Q0–Q3 values and compare vector results/FPSR/NZCV across arrangements, source/destination aliases and controls. The 13,216 comparisons exercise synthetic instructions only. Capstone separately checks constructed encoding families and arrangements; neither software reference establishes original-hardware behavior or retail FP configuration.

## Fused sign and indexed-lane validation

Arm distinguishes fused subtract/negated fused operations from multiplying then subtracting with two roundings. Original glue now transforms the required raw input signs before calling the established fused adapter. Mandatory cancellation and NaN-sign fixtures and expanded independent QEMU tests check this ordering. Indexed S/D FMUL/FMLA/FMLS reuse source snapshots with a statically decoded element index; all legal source indices and overlap cases are compared. No Arm/QEMU implementation source was copied. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

## Format conversion and CRC evidence

The pinned SoftFloat subset now includes unchanged f32_to_f64/f64_to_f32 and four ARM-VFPv2 common-NaN helpers. Copies were compared with exact Git blobs at the recorded revision and hashed in provenance. Original glue retains guest rounding/exception state and canonicalizes default NaNs in the output format. The independent synthetic QEMU matrix covers 8,768 cases; no host casts/fenv are used. [SoftFloat source](https://github.com/ucb-bar/berkeley-softfloat-3/tree/497c81c1f5016dc6ef5b3219ece2f3c261d4d8f1/source).

Original CRC lowering uses the architectural CRC-32/CRC-32C polynomials with explicit raw seed semantics and input bit widths. A separate polynomial-division oracle and known string checks validate the recurrence; QEMU additionally checks 40 synthetic encoding fixtures. No Arm pseudocode or reference implementation was copied. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

## Permutation and pairwise evidence

Original ZIP/UZP/TRN lowering computes lane bank/index selection over snapshots. Mandatory tests independently construct lane lists using interleave/select operations; 210 synthetic QEMU comparisons separately check arrangement and alias encodings. Scalar/vector FADDP now reuses numeric add with explicit adjacent-pair ordering. The expanded 37,856-case vector matrix checks software-reference results/FPSR/NZCV; it is not a hardware or retail-profile oracle. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

## Min/max validation and artifact containment

A small synthetic probe confirmed signaling-NaN payload priority in installed QEMU before native implementation. Original min/max glue now explicitly handles quiet/signaling NaNs, numeric selection and signed-zero ties. Mandatory known-pattern tests and expanded native/QEMU matrices cover 26,416 scalar FP/selection cases and 43,232 vector numeric cases at O0/O2. These remain software-reference evidence under the restricted FP profile, not original-hardware proof.

The artifact guard rejected a stale QEMU crash dump from an earlier unallocated synthetic fixture during publication. The dump and failed validation receipt stay private; the fixture generator was corrected, and fixture process working directories now point into private testing. No dump or game input was published. Publication retains its full validation and artifact gates.

## Single-register structure transfer evidence

Original single-structure decoding and native lowering distinguish lane preservation from full-vector upper clearing on replication. Independent byte-array expectations cover every element lane, guest SP/vector register 31, post-index aliasing/wrap, unaligned normal-memory accesses and fault preservation. Separate, independently authored AArch64 ELF fixtures compare 1,088 cases with QEMU, including vector results, memory bytes, writeback deltas and NZCV. Constructed Capstone cases also cover rejected multi-register/reserved encodings. These are diagnostic software results; no original function executes and no SP/access-trap/device/hardware equivalence is established. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

## SIMD comparison evidence

Independent synthetic probes first distinguished equality's quiet-NaN behavior from ordered/absolute comparison IOC behavior. Original mask lowering now reuses the established checked compare callbacks with explicit unordered exclusion. Mandatory tests classify NaNs from bits and use independent host comparisons only for ordered test inputs; production uses no host FP operation or host fenv. The separate 16,800-case QEMU matrix covers scalar/vector widths, aliases, zero/absolute forms and rounding/default-NaN controls. Decoder tests protect existing scalar FCCMP/FCSEL forms with overlapping low fields. No reference implementation source was copied and no original code was executed. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

## Raw table/count evidence

Original table lookup lowering is checked by independently concatenating byte arrays and indexing a Vec, including fallback behavior, full-Q table sources for 64-bit results and modulo-32 register lists. Byte population count is checked with Rust u8::count_ones. The native matrix covers 34,816 cases per O0/O2 optimization; a separate 120-case synthetic QEMU run checks encodings/arrangements. Constructed Capstone cases also preserve neighboring EXT and reject unsupported variants. No reference implementation code was copied, and no game/hardware execution occurred. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

## Integer SIMD evidence

Original ordinary-lane/reduction lowering is checked against separate i128/u128 mathematical references, including signed interpretation, modulo-width truncation, all-lane extrema and widened sums. The native matrix covers 28,992 cases per optimization; independently authored AArch64 fixtures compare 17,568 cases with QEMU at O0/O2. Constructed Capstone cases check exact instruction names, scalar/vector arrangements and operands, while boundary tests retain existing FCCMP/FCSEL forms. No reference implementation source was copied; original functions remain unexecuted. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).

## Integral-rounding evidence

Independently authored synthetic probes first checked FRINT results and invalid/inexact flags. Original glue now wraps unchanged f32_roundToInt/f64_roundToInt at the existing pinned SoftFloat revision, verified against exact Git blobs and hashed in provenance. Native tests independently use host rounding for non-NaN test inputs and bit classification for NaNs; production does not use host floating operations. The 45,920-case native and 22,960-case QEMU matrices cover rounding directions, ties, controls, aliases and transactional refusal at O0/O2. These remain software observations under the restricted profile, with no original execution or hardware equivalence claim. [Pinned SoftFloat source](https://github.com/ucb-bar/berkeley-softfloat-3/tree/497c81c1f5016dc6ef5b3219ece2f3c261d4d8f1/source).

## Scaled-conversion evidence

Private first-blocker classification identified scaled FCVTZS forms, mainly W-from-S. Independently authored encoding probes established GPR/SIMD scale layouts before implementation. Original numeric glue now uses exact S-to-D widening and raw binary scaling for FP→integer, or rounded integer→FP followed by exact exponent rescaling. This avoids spurious intermediate overflow and preserves the checked conversion rules. Separate integer/rational oracles cover 98,304 helper cases; AOT/QEMU comparisons cover 327,680 cases across all legal scales and controls at O0/O2. Additional tests protect scalar/fused/MOVI neighbors and transactional refusal. No upstream implementation source was copied or modified, and no original function or hardware oracle was executed.

## Raw transforms and immediate-shift evidence

Independent authored Capstone probes established the legal reversal and immediate/widening shift arrangements. Original native lowering is checked against separate byte chunk reversal, u8::reverse_bits/NOT, wide signed mathematical shifts and per-bit insertion references. The mandatory native matrices cover 16,384 transform and 68,352 shift cases per optimization; separate synthetic QEMU matrices cover 64 and 34,176 cases at O0/O2. Tests include full-width shift boundaries, upper-half widening, aliases/V31, upper clearing and state preservation. No reference implementation code was copied and no original code/hardware execution occurred. [Arm instruction reference](https://documentation-service.arm.com/static/6245c734b059dc5ff9a8bdab).


Pairwise min/max reuses the independently authored adjacent-pair routing and existing min/max NaN/zero rules. An author-created 288-pair synthetic QEMU experiment compared FABD with FSUB followed by FABS across S/D scalar/vector controls and special values before implementation; the expanded 86,304-case SIMD numeric matrix also passes. The adapter performs checked subtraction then raw sign clearing, preserving operation order. No external implementation or additional upstream source was copied. Software-oracle lineage and target-profile limitations remain unchanged.


Independently constructed min/max and SHLL encoding probes were compared with the private Capstone installation before implementation. Six ordinary-scalar-domain min/max probe neighbors decode as FCCMP/FCSEL and retain their existing decoder ownership. Typed min/max selection/reduction and full-width widening reuse original snapshot/packing code, with independent i128/u128/byte-array expectations and synthetic QEMU fixtures. No additional upstream source or external implementation was copied.


XTN/XTN2 encoding probes, independent byte-selection expectations and synthetic QEMU fixtures validate raw truncating narrowing separately from saturating forms. Before implementing four-S min/max reductions, an original synthetic experiment compared full reductions with vector-plus-scalar pairwise instructions across all combinations of eight raw finite/zero/quiet/signaling-NaN inputs and two DN settings: 32,768 pairs match, including sticky FPSR/NZCV. This supports pair-tree routing; implementation additionally uses mandatory known results/refusals and the expanded 86,304-case numeric matrix. No external implementation or additional upstream source was copied. Related software-oracle ancestry and unverified target/hardware behavior remain limitations.


Original LD2/ST2 encoding probes and synthetic ELF construction cover two-register de/interleaving, V31-to-V0 wrapping and post-index modes. Private Capstone checks exact arrangements, register lists and writeback fields across constructed opcode/register/reserved neighborhoods. Independent native byte-array expectations and the 280-case QEMU matrix exercise the existing 32-byte bridge without increasing ABI capacity. A failing-read callback that fills its temporary buffer verifies refusal without partial register/base commit. All new implementation/fixtures are original code; no SDK/game code or external implementation was copied. Software-oracle and actual target-profile limitations remain.


Constructed NEG/SHRN boundary probes preceded implementation. Independent i128 signed-negation and byte-to-u128 division oracles differ from production unsigned packing/arithmetic. The SHRN native matrix covers 114,688 cases per optimization and 448 QEMU cases, including every legal amount and overlapping/V31 sources; integer arithmetic matrices additionally cover NEG minimum-value wrap and state preservation. Scalar FP and rounding/saturating encoding neighborhoods remain separately checked with private Capstone. All new implementation and ELF fixtures are original code, without new upstream files or copied external implementations. Software-oracle and target-configuration limitations remain unchanged.


A private 4,056-case synthetic SQDMULH/SQRDMULH experiment validated independent i128 doubling/high-half/round/clamp expectations and sticky QC before implementation. The supplied first-blocker census distinguished register operands from indexed-element forms; both are implemented, with independent Capstone probes for shared lane/register fields. Expanded production tests cover 405,216 mandatory native cases per optimization and 270,144 QEMU cases. The production int64 range proof removes minimum-times-minimum saturation before doubling and avoids signed right shifts; independent test arithmetic uses i128 shifts/clamping. All new code and ELF fixtures are original; no external implementation or upstream files were copied. Software-only and target-profile limitations remain explicit.


The multiple-structure encoding table was independently constructed and cross-checked with private Capstone, including distinct contiguous LD1 lists and LD2/LD3/LD4 interleaving, Q0 single-D constraints, list wrapping and post-index fields. Expanding beyond the old 32-byte contract deliberately increments CPU ABI to v8, updates both guards/buffers and adds per-native-entry version checks. Independent byte-array and QEMU fixtures compare up to four destination vectors and 64 memory bytes; stale ABI and buffer/fault negatives are required. No upstream or game code was copied. Software-only and unmodeled hardware/profile limitations remain explicit.

## Widening multiply provenance

The widening register/indexed masks and shared H/L/M lane fields were constructed independently and checked using private Capstone. Native code uses unsigned modulo arithmetic; the independent oracle uses i128 products and accumulator arithmetic followed by byte truncation. Tests include signed/unsigned boundaries, both source halves, destination aliases, V31, indexed lanes and preserved status fields. All implementation and synthetic ELF code is original; no new upstream sources or game code were copied. QEMU remains a software oracle and original execution stays gated.

# CPU implementation and limits

The lifter supports immediate/shifted-register integer arithmetic and NZCV, move-wide/register moves, ADR/ADRP, direct branches and calls, BR/tail routing, BLR, B.cond, CBZ/CBNZ, TBZ/TBNZ, integer CSEL/CSINC/CSINV/CSNEG and aliases, CCMP/CCMN, multiply/divide, integer logical/bitfield/extract operations, variable shifts, extended-register arithmetic, ADC/ADCS/SBC/SBCS, scalar bit reversals/counts, scalar/SIMD loads/stores and register pairs, raw SIMD moves/immediates/lanes/logical operations, scalar LSE atomics, DMB/DSB, FPCR/FPSR/NZCV transfers, S/D basic arithmetic/compare/FMADD/FNMUL/FCCMP/FCCMPE and H/S/D FCSEL, NOP, RET, PACIASP/PACIBSP, AUTIASP/AUTIBSP, XPACLRI and RETAA/RETAB. Unknown instructions and constrained pair/writeback forms reject. PAC is never discarded as a hint.

Generated C uses fixed native labels. Typed callbacks handle guest memory, PAC and translated calls; a missing backend returns unsupported. No interpreter/JIT fallback exists. LDAPR/LDAR/STLR byte/half/word/doubleword operations use a separate ordered-memory callback with distinct RCpc-acquire, RCsc-acquire and release tags. Calls share one remaining-instruction budget, verify the return PC and require restoration of the caller module bias. PC diagnostics, address construction, call targets and LR use the configured guest module base. ADRP requires a page-aligned bias. Plain memory callbacks support 1/2/4/8/16/32 bytes and receive buffers of ceil(bytes/8) little-endian words; the complete access must validate before mutation. ABI v7 is shared through native/cpu.h and mirrored by a layout-tested Rust Cpu. A pinned checked-memory bridge and registered-native-entry dispatcher now serve synthetic integration tests. They are not connected to a game bootstrap.

## PAC execution

`native/pac.c` supplies an explicit diagnostic profile: QARMA5, legacy PAuth, 39-bit addresses and no TBI. It adapts the attributed BSD gem5 primitive and legacy field behavior. This profile is selected by synthetic tests only. The actual game CPU feature, address/tagging and enhanced-authentication configuration are not established. QARMA3, implementation-defined algorithms, EPAC/PAuth2, FPAC, PACM, data-pointer authentication and additional authenticated branch/register forms remain unsupported.

Signing changes guest LR using key A/B and SP. AUT restores a valid pointer or produces the legacy poisoned pointer. RETAA/RETAB authenticate into a branch target without overwriting LR and report authentication/branch failure before returning to the wrapper. Architectural exception classes/ESR and a complete guest dispatcher are absent. XPACLRI removes PAC according to the selected address profile; it is not authentication.

Native synthetic tests cover signed LR saved/restored through a guest stack, nested generated calls using both keys, shared budget exhaustion, modified return pointers, wrong modifiers, missing backends, memory faults, arithmetic flags and rebased addresses. They run at O0/O2 with undefined-behavior checks. QARMA matches QEMU's independent pure-crypto implementation on 4,096 deterministic inputs; this is software comparison, not Switch 2 validation.

## Supplied-binary survey

`cpu-survey` validates all 223,856 indexed CIE/FDE ranges for the observed encoding, including zRB/zPLRB key-B augmentations. It does not execute CFI programs, unwind exceptions or discover every function.

209,011 indexed extents lower within the current subset; 122,773 contain PAC. The survey now explicitly permits unconditional out-of-extent B tail transfers, unlike strict slice lifting. These counts allow unbound tail/call/memory/PAC exits and do not establish runnable functions, full BR target discovery or translated callees. Forty-six selected functions compile into private native objects, 36 containing PAC. No game-derived function has executed. Full-module translation and linking of generated code remain incomplete.

Private JSONL records preserve surveyed failures. `tools/classify_survey.py` optionally uses privately installed Capstone to classify the first unsupported word in each function. Counts are neither dynamic instruction frequencies nor full instruction coverage. Full indirect-target discovery/binding, Additional fixed-point/format conversions and arithmetic/control profiles, remaining SIMD computation and structure transfers, ISB, exclusive monitors, TLS and exceptions remain gaps.

## Ordered memory and native dispatch

MemoryBridge serializes all accesses through one mutex, including plain accesses. This deliberately stronger diagnostic model is not equivalence with every weak-memory outcome allowed on Switch 2. It enforces natural alignment for ordered transfers and rejects unsupported access profiles. Every span validates before reads modify output or writes change memory. Adjacent mappings work; gaps and permissions fault without partial transfers.

NativeTarget borrows the native Library for code lifetime. Dispatcher and memory attachments pin context, borrow CPU exclusively, and restore prior callbacks on drop. Only registered native functions can be called; unknown targets refuse. Callback shared budgets and per-module rebasing are tested. Targets remain trusted ABI-v7 libraries; actual executable permissions, game identities and PAC configuration require bootstrap validation.

Native tests check all conditional-select variants across 32/64-bit width and every NZCV combination, shifted arithmetic boundaries, test-bit branches, ordered load/store widths, transactional faults, threaded publication through the serialized bridge, and native indirect calls through Rust. CPU layout offsets/size/version are checked against the C compiler. Native semantic cases run at O0/O2 with fatal undefined-behavior instrumentation.

## SIMD transport and addressing

ABI v7 retains 32 raw 128-bit vector registers and includes atomic, data-barrier and floating-point callbacks plus FPCR/FPSR state. Ordinary B/H/S/D/Q loads/stores and S/D/Q pairs copy bits without FP computation; scalar loads zero the unused vector bits. W/X pairs, LDPSW, signed scalar loads, signed unscaled/pre/post offsets, extended/scaled register offsets and scalar/SIMD literals share the checked transfer path. Loads and writeback commit only after the complete callback succeeds; the diagnostic bridge makes pair faults transactional. This is not a claim about hardware pair atomicity. Non-temporal pair intent is retained but introduces no separate behavior in this serialized backend. FP enable/access traps and SP alignment policy remain unmodeled, so this path is gated from actual game execution.

Logical masks are decoded ahead of execution. Native semantic tests compare bitfields against bit-by-bit copies and immediate logical operations against independently constructed circular runs. Optional `tools/check_decoder.py` submits JSONL to `decode-words` and checks accepted families/transfer widths against private Capstone; it does not establish runtime semantics or require redistributing that dependency.

## Tail control flow, conditional compares and integer products

BR switches only among fixed native labels already lifted in the current slice. Other targets set PC and return internal `MKW_EXIT_TAIL` (8). The public generated wrapper forwards an initial tail request to the call callback; registered native budget entries return tail requests to NativeDispatcher's iterative loop. LR stays unchanged and nested calls retain the shared budget. Native code that reports a tail without reducing that budget refuses with status 6. Missing/unknown targets refuse; this mechanism does not infer jump tables, validate BTI or reproduce architectural branch exception classes.

`lift_function` accepts direct B transfers outside exact validated extents as tails; ordinary `lift` remains strict. Conditional escapes and fallthrough truncation reject in both policies. Unbound tails must not be counted as fully translated callees. A local BR target absent from the lifted label set also requires an independently registered entry.

CCMP/CCMN conditionally compute subtraction/addition flags or replace NZCV with their immediate. Native tests exercise all conditions, incoming/fallback flags and widths using independent condition truth tables and wide mathematical references. MADD/MSUB, signed/unsigned long products, SMULH/UMULH and SDIV/UDIV preserve NZCV. Native product/division results are compared with Rust i128/u128 calculations; generated C avoids signed-overflow UB, including minimum/-1 and division by zero. This remains software conformance evidence, not original-hardware validation.

## Raw SIMD bits and scalar LSE

Raw FMOV (including high D lane), scalar/vector compact FP constants, MOVI/MVNI, modified-immediate ORR/BIC, DUP/INS/UMOV/SMOV, vector logic/bit selects and EXT are supported. Scalar results clear the unused vector bits; lane inserts preserve them. Bit transport never converts values through host FP types. A legacy FPCR/FPSR profile is modeled; access/enable trap delivery and non-default merge profiles remain unsupported; actual game execution remains gated.

LSE byte/halfword/word/doubleword RMW and CAS use an explicit ABI-v7 atomic callback. Atomic order tags distinguish relaxed, acquire, release and acquire/release; ordinary LSE RMW with Rt=31 suppresses acquire, while CAS retains its encoded order. The diagnostic bridge validates naturally aligned normal-memory access and full read/write permissions even for a failed comparison, serializing the complete operation against plain/ordered operations. No weaker fallback to separate load/store occurs. Exclusive/pair atomics, ISB, devices and hardware weak-memory equivalence are absent; DMB/DSB are explicit under the serialized normal-memory profile. Unsupported alignment profiles refuse.

Mandatory native tests verify arithmetic, register/flag preservation, transactional faults and contended updates. Optional independent fixtures compare generated x86-64 code against QEMU 8.2.2 for 2,741 SIMD and 160 LSE cases. The reference executes synthetic instruction words only; its existence does not introduce a game interpreter/JIT path. Receipts explicitly distinguish unavailable software references and hardware/weak-memory checks not performed.

## Scalar FP state and data barriers

ABI v7 carries FPCR/FPSR and separate borrowed floating and data-barrier callbacks. System-register transfers support legacy FPCR fields, masked FPSR and NZCV. S/D FADD/FSUB/FMUL/FDIV/FMADD and FCMP/FCMPE require a configured backend; H/S/D FCSEL copies raw bits and preserves FPSR/NZCV. The numeric helper supports the four architectural rounding modes, default-NaN control and gradual underflow with sticky IEEE flags. Flush/alternative/merge profiles and enabled FP traps refuse before committing destinations/FPSR. Half arithmetic, fixed-point/FP-format conversions and exception delivery remain gaps.

The selected BSD Berkeley SoftFloat sources remain unmodified, with pinned hashes and ARM-VFPv2 specialization. Original native glue uses thread-local operation state, preserves previous SoftFloat settings, handles A64 addend-first fused NaN priority and maps exception flags into FPSR. `python3 tools/build_fp.py` builds the verified subset privately and records source/compiler/library identities. FloatingBackend borrows its Library; its CPU attachment preserves code lifetime and restores the previous callback. Numeric helpers are attached only to synthetic tests, not a game bootstrap.

DMB/DSB domain/access options remain in IR and callback tags. For synchronous owned normal memory, the bridge holds the shared mutex and executes a sequentially consistent fence; there are no pending device/cache/TLB operations to complete. This deliberately stronger profile does not claim all weak-memory outcomes or device completion. ISB and unsupported barrier variants refuse.

Mandatory native tests cover state/refusal behavior, fused single rounding, thread isolation, FCSEL truth tables and threaded barrier publication. An additional 9,604 fixtures compare native O0/O2 results, FPSR and NZCV with QEMU 8.2.2, including IEEE edge cases. The comparison shares broad SoftFloat ancestry and remains software evidence, not original-hardware proof.

## Integer conversions and unary operations

ABI v7 adds a dedicated conversion callback with explicit integer width, FP width, direction and instruction rounding. SCVTF/UCVTF use FPCR rounding; the FCVT[N/P/M/Z/A][S/U] register forms use their encoded mode. The Arm-specialized SoftFloat result/exception rules include saturation and zero for NaN conversions, with sticky IOC/IXC. Signed integer input is transferred through exact-width bit copies; W outputs zero-extend. Fixed-point scaled and floating-format conversions remain unsupported.

S/D FNEG/FABS are raw sign operations and do not quiet NaNs or alter FPSR, including when DN is set. FSQRT uses the software numeric backend. The shared scoped environment restores thread-local helper state after either arithmetic or conversion calls. The same restricted FPCR profile applies; enabling unsupported flush/trap/alternative/merge behavior refuses rather than changing host FP mode.

## Scalar bit permutations and carry chains

RBIT, REV16, width-valid REV/REV32/REV64, CLZ and CLS use unsigned fixed-width lowering. Count-zero and count-sign edge cases are explicit and do not call undefined host builtins on zero. ADC/SBC capture incoming C, complement the subtrahend for subtract-with-carry, and compute the two-stage unsigned sum; ADCS/SBCS commit NZCV after computing the result. No signed host arithmetic determines guest overflow. Native O0/O2 tests use Rust bit/byte builtins and separate u128/i128 mathematical references, including W truncation, zero registers, discarded results and both carry inputs.

## Software thread registers

ABI v7 carries raw guest TPIDR_EL0/TPIDRRO_EL0 values and a configuration flag. The explicit EL0 diagnostic model supports reads of both and writes of TPIDR_EL0; read-only-register writes and unconfigured access refuse. Cpu::configure_thread_registers establishes these values without looking up host TLS. Register values are not adjusted by module bias. Using one as an address still goes through checked guest mappings, so an invalid pointer faults rather than becoming a host dereference.

Synthetic concurrent CPU instances with different configured values read separate author-created guest blocks. This verifies state separation and address transport, not retail TLS initialization or guest thread scheduling. Actual TLS templates/layout, process/thread setup and target configuration remain bootstrap work. Other system registers, including cache geometry and physical counters, still reject rather than return guessed host values.

FNMUL applies its result-sign inversion after the ordinary multiply's rounding and NaN processing, preserving the resulting negative default-NaN sign when DN is enabled. FCCMP/FCCMPE conditionally execute the compare helper or replace NZCV from the instruction; skipped compares preserve FPSR and do not require a floating backend. All conditions, quiet/signaling inputs, missing-backend paths and forced fallback values have native/differential tests. These additions retain the same restricted execution profile and do not establish enabled trap delivery or original-hardware equivalence.

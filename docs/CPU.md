# CPU implementation and limits

The lifter supports immediate/shifted-register integer arithmetic and NZCV, move-wide/register moves, ADR/ADRP, direct branches and calls, BR/tail routing, BLR, B.cond, CBZ/CBNZ, TBZ/TBNZ, integer CSEL/CSINC/CSINV/CSNEG and aliases, CCMP/CCMN, multiply/divide, integer logical/bitfield/extract operations, variable shifts, extended-register arithmetic, ADC/ADCS/SBC/SBCS, scalar bit reversals/counts, scalar/SIMD loads/stores and register pairs, raw SIMD moves/immediates/lanes/logical operations, scalar LSE atomics, DMB/DSB, FPCR/FPSR/NZCV transfers, S/D basic arithmetic/compare/FMADD/FNMUL/FCCMP/FCCMPE and H/S/D FCSEL, NOP, RET, PACIASP/PACIBSP, AUTIASP/AUTIBSP, XPACLRI and RETAA/RETAB. Unknown instructions and constrained pair/writeback forms reject. PAC is never discarded as a hint.

Generated C uses fixed native labels. Typed callbacks handle guest memory, PAC and translated calls; a missing backend returns unsupported. No interpreter/JIT fallback exists. LDAPR/LDAR/STLR byte/half/word/doubleword operations use a separate ordered-memory callback with distinct RCpc-acquire, RCsc-acquire and release tags. Calls share one remaining-instruction budget, verify the return PC and require restoration of the caller module bias. PC diagnostics, address construction, call targets and LR use the configured guest module base. ADRP requires a page-aligned bias. Plain memory callbacks support 1/2/3/4/6/8/12/16/24/32/48/64 bytes and receive buffers of ceil(bytes/8) little-endian words; the complete access must validate before mutation. ABI v9 is shared through native/cpu.h and mirrored by a layout-tested Rust Cpu. A pinned checked-memory bridge and registered-native-entry dispatcher now serve synthetic integration tests. They are not connected to a game bootstrap.

## PAC execution

`native/pac.c` supplies an explicit diagnostic profile: QARMA5, legacy PAuth, 39-bit addresses and no TBI. It adapts the attributed BSD gem5 primitive and legacy field behavior. This profile is selected by synthetic tests only. The actual game CPU feature, address/tagging and enhanced-authentication configuration are not established. QARMA3, implementation-defined algorithms, EPAC/PAuth2, FPAC, PACM, data-pointer authentication and additional authenticated branch/register forms remain unsupported.

Signing changes guest LR using key A/B and SP. AUT restores a valid pointer or produces the legacy poisoned pointer. RETAA/RETAB authenticate into a branch target without overwriting LR and report authentication/branch failure before returning to the wrapper. Architectural exception classes/ESR and a complete guest dispatcher are absent. XPACLRI removes PAC according to the selected address profile; it is not authentication.

Native synthetic tests cover signed LR saved/restored through a guest stack, nested generated calls using both keys, shared budget exhaustion, modified return pointers, wrong modifiers, missing backends, memory faults, arithmetic flags and rebased addresses. They run at O0/O2 with undefined-behavior checks. QARMA matches QEMU's independent pure-crypto implementation on 4,096 deterministic inputs; this is software comparison, not Switch 2 validation.

## Supplied-binary survey

`cpu-survey` validates all 223,856 indexed CIE/FDE ranges for the observed encoding, including zRB/zPLRB key-B augmentations. It does not execute CFI programs, unwind exceptions or discover every function.

213,503 indexed extents lower within the current subset; 126,400 contain PAC. The survey now explicitly permits unconditional out-of-extent B tail transfers, unlike strict slice lifting. These counts allow unbound tail/call/memory/PAC exits and do not establish runnable functions, full BR target discovery or translated callees. Seventy-two selected functions compile into private native objects, 51 containing PAC. No game-derived function has executed. Full-module translation and linking of generated code remain incomplete.

Private JSONL records preserve surveyed failures. `tools/classify_survey.py` optionally uses privately installed Capstone to classify the first unsupported word in each function. Counts are neither dynamic instruction frequencies nor full instruction coverage. Full indirect-target discovery/binding, remaining half-format conversions and arithmetic/control profiles, remaining SIMD computation and structure transfers, ISB, exclusive monitors, TLS and exceptions remain gaps.

## Ordered memory and native dispatch

MemoryBridge serializes all accesses through one mutex, including plain accesses. This deliberately stronger diagnostic model is not equivalence with every weak-memory outcome allowed on Switch 2. It enforces natural alignment for ordered transfers and rejects unsupported access profiles. Every span validates before reads modify output or writes change memory. Adjacent mappings work; gaps and permissions fault without partial transfers.

NativeTarget borrows the native Library for code lifetime. Dispatcher and memory attachments pin context, borrow CPU exclusively, and restore prior callbacks on drop. Only registered native functions can be called; unknown targets refuse. Callback shared budgets and per-module rebasing are tested. Targets remain trusted ABI-v8 libraries; actual executable permissions, game identities and PAC configuration require bootstrap validation.

Native tests check all conditional-select variants across 32/64-bit width and every NZCV combination, shifted arithmetic boundaries, test-bit branches, ordered load/store widths, transactional faults, threaded publication through the serialized bridge, and native indirect calls through Rust. CPU layout offsets/size/version are checked against the C compiler. Native semantic cases run at O0/O2 with fatal undefined-behavior instrumentation.

## SIMD transport and addressing

ABI v9 retains 32 raw 128-bit vector registers and includes atomic, data-barrier and floating-point callbacks plus FPCR/FPSR state. Ordinary B/H/S/D/Q loads/stores and S/D/Q pairs copy bits without FP computation; scalar loads zero the unused vector bits. W/X pairs, LDPSW, signed scalar loads, signed unscaled/pre/post offsets, extended/scaled register offsets and scalar/SIMD literals share the checked transfer path. Loads and writeback commit only after the complete callback succeeds; the diagnostic bridge makes pair faults transactional. This is not a claim about hardware pair atomicity. Non-temporal pair intent is retained but introduces no separate behavior in this serialized backend. FP enable/access traps and SP alignment policy remain unmodeled, so this path is gated from actual game execution.

Logical masks are decoded ahead of execution. Native semantic tests compare bitfields against bit-by-bit copies and immediate logical operations against independently constructed circular runs. Optional `tools/check_decoder.py` submits JSONL to `decode-words` and checks accepted families/transfer widths against private Capstone; it does not establish runtime semantics or require redistributing that dependency.

## Tail control flow, conditional compares and integer products

BR switches only among fixed native labels already lifted in the current slice. Other targets set PC and return internal `MKW_EXIT_TAIL` (8). The public generated wrapper forwards an initial tail request to the call callback; registered native budget entries return tail requests to NativeDispatcher's iterative loop. LR stays unchanged and nested calls retain the shared budget. Native code that reports a tail without reducing that budget refuses with status 6. Missing/unknown targets refuse; this mechanism does not infer jump tables, validate BTI or reproduce architectural branch exception classes.

`lift_function` accepts direct B transfers outside exact validated extents as tails; ordinary `lift` remains strict. Conditional escapes and fallthrough truncation reject in both policies. Unbound tails must not be counted as fully translated callees. A local BR target absent from the lifted label set also requires an independently registered entry.

CCMP/CCMN conditionally compute subtraction/addition flags or replace NZCV with their immediate. Native tests exercise all conditions, incoming/fallback flags and widths using independent condition truth tables and wide mathematical references. MADD/MSUB, signed/unsigned long products, SMULH/UMULH and SDIV/UDIV preserve NZCV. Native product/division results are compared with Rust i128/u128 calculations; generated C avoids signed-overflow UB, including minimum/-1 and division by zero. This remains software conformance evidence, not original-hardware validation.

## Raw SIMD bits and scalar LSE

Raw FMOV (including high D lane), scalar/vector compact FP constants, MOVI/MVNI, modified-immediate ORR/BIC, DUP/INS/UMOV/SMOV, vector logic/bit selects and EXT are supported. Scalar results clear the unused vector bits; lane inserts preserve them. Bit transport never converts values through host FP types. A legacy FPCR/FPSR profile is modeled; access/enable trap delivery and non-default merge profiles remain unsupported; actual game execution remains gated.

LSE byte/halfword/word/doubleword RMW and CAS use an explicit ABI-v8 atomic callback. Atomic order tags distinguish relaxed, acquire, release and acquire/release; ordinary LSE RMW with Rt=31 suppresses acquire, while CAS retains its encoded order. The diagnostic bridge validates naturally aligned normal-memory access and full read/write permissions even for a failed comparison, serializing the complete operation against plain/ordered operations. No weaker fallback to separate load/store occurs. Exclusive/pair atomics, ISB, devices and hardware weak-memory equivalence are absent; DMB/DSB are explicit under the serialized normal-memory profile. Unsupported alignment profiles refuse.

Mandatory native tests verify arithmetic, register/flag preservation, transactional faults and contended updates. Optional independent fixtures compare generated x86-64 code against QEMU 8.2.2 for 2,741 SIMD and 160 LSE cases. The reference executes synthetic instruction words only; its existence does not introduce a game interpreter/JIT path. Receipts explicitly distinguish unavailable software references and hardware/weak-memory checks not performed.

## Scalar FP state and data barriers

ABI v7 carries FPCR/FPSR and separate borrowed floating and data-barrier callbacks. System-register transfers support legacy FPCR fields, masked FPSR and NZCV. S/D FADD/FSUB/FMUL/FDIV/FMADD/FMSUB/FNMADD/FNMSUB and FCMP/FCMPE require a configured backend; H/S/D FCSEL copies raw bits and preserves FPSR/NZCV. The numeric helper supports the four architectural rounding modes, default-NaN control and gradual underflow with sticky IEEE flags. Flush/alternative/merge profiles and enabled FP traps refuse before committing destinations/FPSR. Half arithmetic/conversions and exception delivery remain gaps.

The selected BSD Berkeley SoftFloat sources remain unmodified, with pinned hashes and ARM-VFPv2 specialization. Original native glue uses thread-local operation state, preserves previous SoftFloat settings, handles A64 addend-first fused NaN priority and maps exception flags into FPSR. `python3 tools/build_fp.py` builds the verified subset privately and records source/compiler/library identities. FloatingBackend borrows its Library; its CPU attachment preserves code lifetime and restores the previous callback. Numeric helpers are attached only to synthetic tests, not a game bootstrap.

DMB/DSB domain/access options remain in IR and callback tags. For synchronous owned normal memory, the bridge holds the shared mutex and executes a sequentially consistent fence; there are no pending device/cache/TLB operations to complete. This deliberately stronger profile does not claim all weak-memory outcomes or device completion. ISB and unsupported barrier variants refuse.

Mandatory native tests cover state/refusal behavior, fused single rounding, thread isolation, FCSEL truth tables and threaded barrier publication. An additional 26,416 fixtures compare native O0/O2 results, FPSR and NZCV with QEMU 8.2.2, including IEEE edge cases. The comparison shares broad SoftFloat ancestry and remains software evidence, not original-hardware proof.

## Integer conversions and unary operations

ABI v7 adds a dedicated conversion callback with explicit integer width, FP width, direction and instruction rounding. SCVTF/UCVTF use FPCR rounding; the FCVT[N/P/M/Z/A][S/U] register forms use their encoded mode. The Arm-specialized SoftFloat result/exception rules include saturation and zero for NaN conversions, with sticky IOC/IXC. Signed integer input is transferred through exact-width bit copies; W outputs zero-extend. Half-format conversions remain unsupported. Scaled fixed-point forms are described below. S/D format conversion is described below.

S/D FNEG/FABS are raw sign operations and do not quiet NaNs or alter FPSR, including when DN is set. FSQRT uses the software numeric backend. The shared scoped environment restores thread-local helper state after either arithmetic or conversion calls. The same restricted FPCR profile applies; enabling unsupported flush/trap/alternative/merge behavior refuses rather than changing host FP mode.

## Scalar bit permutations and carry chains

RBIT, REV16, width-valid REV/REV32/REV64, CLZ and CLS use unsigned fixed-width lowering. Count-zero and count-sign edge cases are explicit and do not call undefined host builtins on zero. ADC/SBC capture incoming C, complement the subtrahend for subtract-with-carry, and compute the two-stage unsigned sum; ADCS/SBCS commit NZCV after computing the result. No signed host arithmetic determines guest overflow. Native O0/O2 tests use Rust bit/byte builtins and separate u128/i128 mathematical references, including W truncation, zero registers, discarded results and both carry inputs.

## Software thread registers

ABI v7 carries raw guest TPIDR_EL0/TPIDRRO_EL0 values and a configuration flag. The explicit EL0 diagnostic model supports reads of both and writes of TPIDR_EL0; read-only-register writes and unconfigured access refuse. Cpu::configure_thread_registers establishes these values without looking up host TLS. Register values are not adjusted by module bias. Using one as an address still goes through checked guest mappings, so an invalid pointer faults rather than becoming a host dereference.

Synthetic concurrent CPU instances with different configured values read separate author-created guest blocks. This verifies state separation and address transport, not retail TLS initialization or guest thread scheduling. Actual TLS templates/layout, process/thread setup and target configuration remain bootstrap work. Other system registers, including cache geometry and physical counters, still reject rather than return guessed host values.

FNMUL applies its result-sign inversion after the ordinary multiply's rounding and NaN processing, preserving the resulting negative default-NaN sign when DN is enabled. FCCMP/FCCMPE conditionally execute the compare helper or replace NZCV from the instruction; skipped compares preserve FPSR and do not require a floating backend. All conditions, quiet/signaling inputs, missing-backend paths and forced fallback values have native/differential tests. These additions retain the same restricted execution profile and do not establish enabled trap delivery or original-hardware equivalence.

## SIMD numeric lanes

S/D 2S/4S/2D arithmetic, FMLA/FMLS and unary operations, vector integer conversions and SIMD scalar S/D conversions lower to a fixed number of checked scalar helper calls. All sources and the accumulator are snapshotted before lane evaluation. Destination bits and sticky FPSR commit only after all lanes succeed; upper unused bits clear. This transactional refusal model handles unsupported profiles without partial vector state and does not implement enabled exception delivery. Half and remaining arithmetic/structure instructions still reject. Native known-pattern and overlap tests plus 86,304 QEMU comparisons at O0/O2 provide software evidence under the restricted FP profile.

S/D FMUL/FMLA/FMLS by element capture all 128 bits of the indexed source and broadcast the decoded lane to the destination lanes. Scalar forms clear the upper result bits. Lane fields and reserved arrangements are checked ahead of code generation. FMSUB/FNMADD/FNMSUB and FMLS negate the required input bit patterns before the existing fused helper runs, preserving single rounding, input-NaN priority and sign behavior under the restricted legacy profile. They do not use a post-rounded negate.

## Format conversion and CRC

S↔D FCVT uses the existing floating callback with operation 14; its byte width describes the input, and the output uses the other supported format. The adapter scopes SoftFloat state, converts NaN payloads with the pinned Arm specialization, applies default-NaN in the output format and accumulates FPSR. Unsupported control profiles still refuse before changing destination/FPSR. Scalar outputs clear unused bits. Native known-pattern/rounding tests and 8,768 synthetic QEMU cases at O0/O2 cover this path; half precision remains unimplemented.

CRC32/CRC32C instructions consume the specified low 8/16/32/64 data bits and a 32-bit seed, then write a zero-extended 32-bit result without changing NZCV. Original portable C uses a reflected polynomial recurrence. Tests independently use polynomial long division, verify known string check values and register overlap/discarded results, and compare 40 synthetic encoding fixtures with QEMU. No host CRC extension or runtime opcode decoder is required.

## Permutations and pairwise addition

ZIP/UZP/TRN select lanes from complete raw source snapshots, then commit the packed result with unused upper bits cleared. B/H/S/D arrangements follow their decoded element widths; a single-D vector is reserved. They preserve all FP/control/flag state and do not require numeric helpers. Independent lane-list construction and QEMU fixtures check arrangement and overlapping destinations, including V31.

S/D scalar/vector FADDP selects adjacent source pairs before calling checked FP add. Vector results concatenate left-source sums then right-source sums; scalar results use the one source pair. Complete snapshots and deferred destination/FPSR commits preserve overlap and later-lane refusal behavior. Gradual-underflow/default-NaN/rounding restrictions remain the existing profile; no enabled trap behavior is implied.

## Floating min/max

S/D scalar/vector FMAX/FMIN propagate NaNs with signaling priority and quiet their selected payload, setting IOC for signaling inputs. FMAXNM/FMINNM select the numeric operand when exactly one input is a quiet NaN; signaling inputs still propagate/raise IOC. Default-NaN applies to NaN results. Numeric selection uses unsigned raw-encoding comparisons with explicit signed-zero ties: maximum selects positive zero unless both are negative; minimum selects negative zero if either is negative. FPCR profile checks and vector deferred commits remain in force. No host floating operation determines these results.

Synthetic QEMU fixture executions now run with private working directories so even crash dumps from rejected author-created instructions stay out of distributable source directories. The public artifact guard continues to reject unexpected binary files rather than excluding them from review silently.

## Single-register structure memory

B/H/S/D LD1/ST1 lane transfers preserve all non-selected vector bits. LD1R reads one checked element and repeats its raw bits across the decoded 64/128-bit arrangement, zeroing unused upper bits. Base-only, immediate post-index and register post-index variants preserve guest SP/vector-register-31 distinctions and capture an aliased index before access. Callback failures leave destination/base and mapped data unchanged under the owned normal-memory bridge; fault address/status remain explicit.

The decoder rejects multi-register structure variants and reserved size/S/R/register combinations. Unsupported diagnostics retain the original word for private blocker classification. These single-element transfers use the existing ABI v7 callback without enlarging its transaction limit. Native byte-array/fault cases and 1,088 synthetic QEMU comparisons at O0/O2 cover the implementation. SP alignment/access traps, device memory and actual guest configuration remain unmodeled gates, as with earlier raw SIMD memory transport.

## SIMD floating comparison masks

S/D scalar/vector FCMEQ/FCMGE/FCMGT and FACGE/FACGT produce an all-one or zero element without changing NZCV. The compare-zero forms additionally support FCMLE/FCMLT and use a literal zero operand. Absolute forms clear operand signs before comparison. Equality uses quiet comparison (IOC on signaling NaNs); ordered/absolute comparisons raise IOC for quiet or signaling NaNs. Every NaN produces a false mask, with sticky FPSR preserved. Half precision and unsupported control profiles reject.

The emitter reuses existing checked comparison helpers and tests their exact equal/less/greater nibble results, excluding unordered explicitly. Complete source snapshots and deferred vector/FPSR commits preserve aliases and later-lane refusal; unused upper bits clear. A scalar-domain boundary check leaves ordinary FCCMP/FCSEL to their existing decoder. Mandatory native ordered-input/NaN expectations and 16,800 QEMU fixtures at O0/O2 provide software evidence under the same restricted FP profile.

## Table lookup and byte population count

TBL/TBX output 8/16 bytes from unsigned byte indices into one through four complete Q table registers, with register-list wrapping. All table/index/old-destination bits are snapshotted before evaluation. TBL returns zero outside the table; TBX preserves the old destination byte there. Both 64-bit output forms clear the upper half. CNT counts the eight bits of each source byte independently. These original unsigned native paths preserve general registers/NZCV/FPCR/FPSR and require no extra callback.

Independent native byte-array and u8::count_ones tests cover all input-byte values, all table lengths, wrapping lists and overlapping source/destination/index roles. A separate 120-case QEMU matrix checks constructed encodings at O0/O2. Access traps and the retail configuration remain gates before original execution; these raw operations do not establish full SIMD support or hardware equivalence.

## Integer SIMD lanes and reductions

Ordinary ADD/SUB wrap independently at B/H/S/D lane width. CMEQ/CMTST, signed CMGE/CMGT, unsigned CMHS/CMHI and valid scalar-D forms produce masks; signed compare-zero forms also support CMLE/CMLT. Unsigned sign-bit ordering implements signed comparisons without host signed casts. ADDV truncates its scalar sum to the original width; SADDLV/UADDLV widen each input and return a scalar twice the input element width. Reserved .2S/.1D and scalar-size combinations reject where required by each format.

Complete snapshots preserve aliases; scalar/64-bit results clear unused upper bits. General registers, NZCV, FPCR and FPSR remain unchanged. Original native lowering uses defined unsigned operations only; independent i128/u128 tests and 17,568 synthetic QEMU cases cover overflow boundaries, signed minima, reductions and state preservation at O0/O2 with fatal UBSan. This remains diagnostic raw-register behavior, not a claim about access traps or verified retail configuration.

## Integral floating rounding

Scalar/vector S/D FRINTN/P/M/Z/A/X/I use the pinned SoftFloat roundToInt operations. N/P/M/Z/A select nearest-even/+infinity/-infinity/zero/ties-away independently of FPCR rounding; X/I follow FPCR. Only X requests a new IXC flag for inexact finite input. Signaling NaNs still raise IOC, NaN payload/default-NaN handling uses the existing restricted profile, and signed zero is retained. Half precision, enabled trap delivery, flush/alternative profiles and newer constrained-integer forms remain unsupported.

All sources are captured and destinations/FPSR commit only on successful helpers; unused upper bits clear. The helper scopes and restores its thread-local state. Mandatory independent host-rounding/NaN-bit expectations cover 45,920 cases per O0/O2 optimization, and separate QEMU fixtures cover 22,960 cases. No host FP operation or fenv implements production semantics. Two unchanged upstream source files and their exact hashes are retained in provenance; ABI layout/signatures remain v7.

## Scaled fixed-point conversion

Scaled SCVTF/UCVTF and FCVTZS/FCVTZU retain fractional_bits explicitly in scalar/vector IR. The helper operation encodes the documented FIXED_SCALE flag, fractional_bits shifted by eight and the existing base operation; all other encodings refuse. Scales are bounded by integer width, and the callback signature/layout remains ABI v7. Guest register-zero and vector-31 semantics remain distinct.

For FP→integer, S is widened exactly to D before raw exponent/significand scaling, then the existing RTZ integer conversion supplies saturation, NaN-zero and IOC/IXC behavior. Binary64 scale overflow only denotes an already out-of-range integer result; no intermediate floating multiply contributes OFC/IXC. Subnormals normalize with bounded unsigned shifts. For integer→FP, existing guest-mode rounding precedes an exact exponent decrease; the minimum nonzero supported magnitude is 2^-64, still normal in S/D, so no second rounding occurs. Helper state and sticky flags remain scoped.

Every legal scale is covered by separate integer/rational expectations and synthetic QEMU comparisons: 98,304 helper cases and 327,680 AOT cases per O0/O2 optimization. Edge tests verify malformed-operation rejection, missing helpers, unsupported controls, zero/discarded registers, aliases and later-lane refusal. Half precision, enabled traps and unverified target profiles remain gated; no original code executes.

## Raw reversals and non-saturating immediate shifts

MVN complements selected raw vector bits; RBIT reverses each byte independently. REV16/REV32/REV64 reverse element order within 2/4/8-byte blocks while retaining each element's byte order. Legal element sizes and register arrangements are checked before emission. Sources are snapshotted and unused upper bits clear. CNT and ordinary scalar/fused/MOVI neighboring encodings retain their decoders.

SHL/SSHR/USHR and SLI/SRI support every legal immediate and valid scalar-D/vector arrangement. Signed right shifts use unsigned sign-fill; full-width shifts have explicit results rather than undefined C shifts. Insert forms select old destination bits explicitly. SSHLL/USHLL and their upper-half variants select a source half and widen before shifting; outputs occupy 128 bits. All computations use defined unsigned native operations and preserve GPR/NZCV/FPCR/FPSR without helpers. The separately encoded SHLL/SHLL2 forms now select the source half and widen with a shift equal to the original element width. Saturating/rounding and register-controlled shifts remain unimplemented.

Independent byte-array/builtin and i128/bit-selection tests verify 16,384 transform and 68,640 shift cases per O0/O2 optimization, with separate QEMU matrices of 64 and 34,320 cases. These raw-register diagnostic results do not establish actual access-trap/configuration behavior or original-hardware equivalence. Original functions remain unexecuted.


## Pairwise min/max and absolute difference

Scalar/vector S/D FMAXP/FMINP/FMAXNMP/FMINNMP capture adjacent source pairs and reuse checked min/max operations 15..18, preserving signaling-NaN priority, number semantics and signed-zero ties. Vector outputs concatenate pairs from the left then right register; scalar outputs use one pair and clear unused bits. Scalar fixed opcode fields are never treated as a second source register in alias fixtures.

FABD uses checked helper operation 26: guest-mode subtraction followed by raw result-sign clearing. Rounding and NaN processing occur before the absolute bit operation; cumulative subtraction flags are retained. No host floating arithmetic implements this operation. Unsupported FPCR profiles refuse, and every lane completes before destination/FPSR commit. ABI v7 signatures/layout and upstream source provenance remain unchanged.

The expanded SIMD numeric matrix compares 86,304 synthetic cases with QEMU at O0/O2 with fatal UBSan. Mandatory native expectations additionally cover adjacent-pair ordering, numeric/NaN selection, signed zero, overflow/inexact flags and later-lane refusal. This is software-oracle evidence under the existing restricted FP profile; original functions remain unexecuted.


## Integer min/max and full-width widening

SMAX/SMIN/UMAX/UMIN select raw source elements across legal B/H/S vectors. Signed ordering compares unsigned values after sign-bit bias; selected values retain their original bits. SMAXV/SMINV/UMAXV/UMINV start from the first element, reduce all remaining elements and clear all destination bits above the scalar result. Two-S reduction, D and scalar min/max forms reject. Source snapshots preserve aliases and V31; complete register/control-state tests run without helpers.

SHLL/SHLL2 use their separate encoding, selecting the low/high source half and shifting widened B/H/S elements by 8/16/32 respectively. The resulting value fits the doubled unsigned width, including the highest input bit; no shift by 64 or host signed overflow is evaluated. Existing SSHLL/USHLL ranges remain distinct.

Independent i128/u128 expectations now cover 41,280 integer SIMD cases per optimization, with a separate 24,864-case QEMU matrix. The expanded shift/widening matrices cover 68,640 native expectations and 34,320 QEMU cases. All run at O0/O2 with fatal UBSan; these remain diagnostic software tests, with no original-hardware or original-function execution evidence.


## Truncating narrowing and floating min/max reductions

XTN takes the low B/H/S bits of each doubled-width element from a full 128-bit source and packs a 64-bit result. XTN clears the upper destination half; XTN2 replaces the upper half while retaining the lower half. Complete source capture preserves overlap and V31. The raw operation does not saturate or change FPSR.QC. Saturating narrow forms remain unsupported. Independent byte selection checks 6,144 native cases per optimization; 24 constructed source/alias/arrangement cases compare with QEMU.

FMAXV/FMINV/FMAXNMV/FMINNMV reduce four S elements with a pair tree: adjacent low pair, adjacent high pair, then their results. This preserves the order-dependent signaling/quiet NaN rules of checked min/max operations 15..18. The helper accumulates sticky flags locally and commits only after all three calls succeed. Mandatory tests include signed-zero/NaN/numeric selection, V31 overlap and refusal at the second and final stages. The existing restricted FP profile remains required; scalar pairwise neighbors retain their separate ownership. The expanded SIMD numeric matrix has 86,304 QEMU fixtures at native O0/O2 with fatal UBSan. A separate 32,768-case software composition probe supports pair-tree routing, without establishing hardware or retail-profile equivalence.


The supplied-binary survey retains its full bounded extent-lifting census but selects representative native object samples only from extents with at most 4,096 lifted instructions. Oversized sample candidates are explicitly deferred; later smaller feature examples remain eligible. Summary fields list deferred candidates and any observed features without a compiled representative. An independent validation step checks source hashes, source-map instruction counts, native relocatable ELF headers, module counts and feature accounting. This is a representative compilation limit, not a new supported-instruction or runnable-function claim.


## Interleaved two-register structures

LD2/ST2 support legal B/H/S/D arrangements in two vectors, including V31-to-V0 list wrapping, SP or general-register bases, immediate/register post-indexing and a post-index register that aliases the base. Each instruction captures its sources and old address/offset before one checked 16/32-byte callback; byte interleaving is explicit. Loads clear unused upper bits in 64-bit arrangements and commit both vectors only after successful memory access. Stores and writeback rely on the owned bridge's complete-range validation before mutation. Unsigned address addition wraps explicitly. Single-D Q0 arrangements and other multi-structure opcodes reject.

Independent byte-array expectations cover 840 successful and 1,400 fault/permission cases per O0/O2 optimization; another callback probe verifies exactly one complete transfer and refusal even if a failing read callback fills its temporary buffer. The 280-case synthetic QEMU matrix compares both destination vectors, wrapped register lists, all 32 memory bytes, base deltas, NZCV/FPCR/FPSR. Tests include unaligned ordinary-memory diagnostics, missing helpers, short ranges, permission failures, address-range overflow and offset wrap. Native code runs with fatal UBSan. The existing serialized normal-memory model, unverified SP/access traps and target gates remain; this adds no device/weak-memory or retail-execution equivalence. LD2/ST2 retain their 16/32-byte transfers within the ABI v8 64-byte callback limit.


## Integer negation and shift narrowing

Non-saturating NEG supports legal B/H/S/D vector and scalar-D arrangements. Defined unsigned zero-minus-input arithmetic wraps at the element width, including the signed minimum, without changing FPSR.QC or NZCV. Mandatory tests retain QC with both clear and set initial states. Source snapshots preserve aliases and V31; unused upper bits clear. Ordinary scalar FP neighbors keep their decoder ownership.

SHRN/SHRN2 support every shift amount from one through the output element width for B/H/S outputs. Each full 128-bit source is captured, doubled-width elements shift right without rounding, and low output bits pack into a 64-bit result. SHRN clears the destination upper half; SHRN2 preserves its lower half. No shift by 64 or host signed overflow is evaluated. Independent byte-to-u128 division expectations cover 114,688 native cases per O0/O2 optimization; 448 synthetic QEMU cases cover all amounts, arrangements and source aliases. Saturating/rounding narrowing and constrained D/scalar arrangements remain unsupported. Both operations require no helpers or ABI v7 changes; actual access-trap/profile gates remain unverified before original execution.


## Saturating signed high products

SQDMULH/SQRDMULH support H/S scalar and vector register operands and indexed element sources. H indices combine the shared H/L/M fields with a four-bit source register; S indices use H/L and retain M as the fifth register bit. Every source is captured before evaluation, including upper indexed lanes and V31 aliases. Unused destination bits clear. Ordinary scalar FP, B/D and unrelated indexed forms retain their separate ownership or reject.

Production uses exact int64 operands derived from bounded raw H/S values. Their products fit int64. The minimum-times-minimum case saturates before doubling and sets sticky FPSR.QC; every remaining doubled product plus the optional positive rounding increment remains representable. Converting to uint64 before shifting and masking avoids implementation-defined negative signed shifts. All non-QC FPSR bits, NZCV and FPCR remain unchanged; QC accumulates across every lane. No floating helper or ABI v7 change is needed.

Independent i128 product/round/clamp expectations check 405,216 native cases per O0/O2 optimization; 270,144 synthetic QEMU cases compare actual results and FPSR/NZCV. Boundaries include minima/maxima, negative/positive halfway rounding, mixed saturating and non-saturating lanes, deterministic random inputs, reversed lanes, every indexed lane, overlapping sources/destinations and clear/set initial QC. Fatal UBSan covers the signed intermediate range. Software-oracle and actual SIMD/FP access/configuration limitations remain; no original code executes. Accumulating and other saturating multiply families remain separate.


## ABI v8 and complete multiple structures

The current CPU ABI is v8. Layout/signatures are unchanged, but normal memory now accepts 24/48/64-byte transfers in addition to existing widths. Both width guards and both temporary buffers expand together; dispatch rejects invalid widths before creating a callback word slice. Ordered/LSE widths remain limited to their prior scalar forms. Registered native entries must export a matching per-entry budget ABI version, and FP backend version checks reject older helpers. Trusted unsafe callers still must honor signatures, lifetimes and buffer capacity; version claims alone are not a sandbox.

LD1/ST1 support contiguous lists of one through four registers. LD2/ST2, LD3/ST3 and LD4/ST4 support explicit element interleaving across their lists. Complete source/base snapshots, modulo-32 register wrapping, upper clearing in Q0 loads, one checked transfer and deferred destination/writeback preserve the diagnostic transaction model for all 8–64-byte totals. Q0 single-D is legal for contiguous LD1 lists but reserved for interleaved forms. Post-index uses the complete transfer size or captured old X register.

The expanded native byte-array suite checks 6,360 successful and 10,600 fault/permission cases per optimization; 2,120 synthetic QEMU cases compare all touched vectors, 64 memory bytes, base deltas and status/control fields at O0/O2 with fatal UBSan. Extended bridge tests cover adjacent mappings, short buffers, invalid sizes, ordered-width refusal and unchanged failed outputs. ABI tests reject stale version-7 native targets/FP helpers and missing entry version symbols. Actual hardware atomicity, SP/access traps, device/weak-memory semantics and game execution remain unverified. Multi-register lane/replication forms were added subsequently under ABI v9.

## Widening integer products and accumulation

SMULL/UMULL, SMLAL/UMLAL and SMLSL/UMLSL support B/H/S vector inputs, low/upper source halves and H/S indexed-element forms. All forms produce a complete 128-bit widened destination. Upper forms select the upper half of ordinary sources; indexed right operands retain their independent full-vector lane selection. Complete snapshots preserve source/destination aliases and V31. Unsigned modulo arithmetic implements products and accumulator wrap without signed overflow; no callbacks or ABI changes are needed and NZCV/FPCR/FPSR remain unchanged.

Independent i128 product/add/subtract expectations check 280,800 native cases per optimization, including extrema, upper halves, aliases, accumulator wrap and indexed lanes. A separate 187,200-case synthetic QEMU matrix matches O0/O2 with fatal UBSan. Scalar and reserved width encodings retain their prior ownership or reject. Software comparisons do not establish original hardware behavior or remove target/access/bootstrap gates.

## Saturating arithmetic and narrowing

SQADD/UQADD and SQSUB/UQSUB support B/H/S/D scalar forms and legal vector arrangements. Inputs are snapshotted before destination updates; unsigned carry/borrow and sign-bit overflow tests avoid host signed overflow, including 64-bit lanes. Saturation sets sticky FPSR.QC and other state is preserved. Independent i128 clamping checks 212,256 native cases per optimization; 141,504 synthetic QEMU cases match O0/O2 with fatal UBSan.

SQXTN/UQXTN/SQXTUN support H/S/D inputs narrowed to B/H/S, scalar results and vector low/upper destination halves. Upper vector forms retain the low half; low/scalar forms clear unused bits. Signed 64-bit inputs are compared with unsigned encoded thresholds, avoiding implementation-defined signed conversions. Independent i128 clamp/truncate checks cover 21,600 native cases per optimization; 10,800 synthetic QEMU cases match O0/O2 with fatal UBSan. Reserved widths and neighboring scalar FP encodings remain separately checked. These operations require no callbacks or ABI change; access/configuration gates and software-oracle limitations still apply.

## Register-count SIMD shifts

SSHL/USHL support legal B/H/S/D vectors and D scalar forms. Each lane uses only the signed low byte of its count: positive counts shift left, negative counts shift right, and counts at/beyond element width produce zero or arithmetic sign fill. Complete snapshots preserve aliases/V31; unused destination bits clear. Explicit bounds prevent C shifts by the element width or larger; unsigned sign fill avoids host signed-right-shift dependencies. FPSR.QC and all other flags/controls are unchanged.

An independent bit-copy oracle checks 688,128 native cases per optimization; 458,752 synthetic QEMU cases match O0/O2 with fatal UBSan. Matrices cover every count byte, irrelevant upper count bits, all arrangements, extrema, aliases and preserved state. No callbacks or ABI changes are required; software-reference and actual target/access/bootstrap limitations remain.

## Saturating immediate narrowing shifts

SQSHRN/SQRSHRN, UQSHRN/UQRSHRN and SQSHRUN/SQRSHRUN support every legal H/S/D-to-B/H/S amount, scalar results and vector low/upper destination halves. Source snapshots and shared narrowing saturation preserve aliases, retained halves and sticky QC. Bounded unsigned arithmetic computes the signed/logical shifted value and optional rounding bit before clamping; this avoids overflowing a 64-bit pre-shift rounding addition.

Independent i128 division/clamp expectations check 1,225,728 native cases per optimization. A 612,864-case synthetic QEMU matrix matches O0/O2 with fatal UBSan. The matrix covers all amounts, rounding ties, saturation thresholds, extrema, scalar/vector forms and V31 aliases. Plain SQXTN/UQXTN/SQXTUN regressions also pass through the shared lowering. Software comparisons do not establish original hardware or remove target/access/bootstrap gates.

## Widening integer add/subtract

SADDL/UADDL/SSUBL/USUBL and SADDW/UADDW/SSUBW/USUBW support B/H/S inputs and upper-source variants. Wide-left forms retain the full-width left operand; upper selection applies only to narrow sources. Complete snapshots preserve aliases and unsigned arithmetic defines widened wrap while sign extension applies to signed narrow sources. Independent i128 expectations cover 74,880 native cases per optimization; 49,920 synthetic QEMU cases match O0/O2 with fatal UBSan. No callbacks, flags or ABI changes are introduced.

## Right-shift accumulation and rounding

SSRA/USRA, SRSHR/URSHR and SRSRA/URSRA support every legal immediate amount, vector arrangement and D scalar form. Explicit full-width cases avoid shifting C values by 64; extracted rounding bits avoid overflowing a pre-shift addition. Accumulation wraps per element, source/destination aliases are snapshotted and unused destination bits clear. FPSR.QC and unrelated CPU state remain unchanged.

Independent i128 floor-division/accumulation expectations check 456,192 native cases per optimization; 414,720 synthetic QEMU cases match O0/O2 with fatal UBSan. Counts at element width, rounding ties, extremes, accumulator wrap and V31 aliases are included. No callbacks or ABI change are required.

## Integer pairwise addition

ADDP supports legal B/H/S/D vectors and D scalar reduction. Vector results concatenate pair sums from the left and right snapshots; scalar results sum the two D lanes and clear the upper half. Unsigned arithmetic defines element wrap and no flags change. Independent u128 byte-array expectations cover 11,960 native cases per optimization; 7,800 synthetic QEMU cases match O0/O2 with fatal UBSan, including aliasing, V31, vector ordering and upper clearing. Original target/access/bootstrap gates and software-reference limitations remain.

## ABI v9 and complete lane/replication lists

Normal-memory transfers now additionally accept exactly 3/6/12 bytes, using the same ceil(bytes/8) buffer contract and complete-range validation. This contract expansion increments ABI to v9 without changing CPU layout or callback signatures. Ordered accesses retain an explicit 1/2/4/8 whitelist, and raw invalid ordered sizes reject before buffer construction. Native entry/FP loading rejects stale v8 helpers; partial final words zero unused bytes while words beyond the transfer remain untouched.

LD1–LD4/ST1–ST4 lane lists and LD1R–LD4R replication lists now decode register count from R/opcode fields. One checked callback transfers the complete list, with source/base/offset snapshots and deferred destination/writeback commits. Lists wrap at V31; lane loads preserve untouched bits and broadcasts clear unused upper halves. Three-way forms access exactly 3/6/12/24 bytes rather than touching adjacent bytes or issuing partial element callbacks.

Independent byte-array tests cover 12,240 success and 20,400 fault/permission cases per optimization. Another 4,080 synthetic QEMU cases compare all touched vectors, memory and status/control/base fields at O0/O2 with fatal UBSan. Callback probes check one complete access and no CPU commit after a failing read filled its temporary buffer. Extended capacity, stale-version and ordered-width negatives pass; existing single-register, complete multiple-structure, bridge and BR/tail regressions pass. Hardware transaction/weak-memory/device/access-trap and original target/bootstrap behavior remain unverified.

## Ordinary SIMD multiplication and YIELD

MUL supports B/H/S register arrangements and H/S indexed elements. Raw unsigned products are truncated to the destination width; full snapshots preserve aliases, Q0 upper clearing and independent upper indexed-lane selection. Independent u128 byte-array expectations cover 46,800 native cases per optimization; 31,200 synthetic QEMU cases match O0/O2 with fatal UBSan. FPSR/NZCV/FPCR and GPR state remain unchanged.

YIELD retains a distinct typed operation and consumes its normal instruction budget. The diagnostic profile chooses no scheduling effect, consistent with [Arm's explanation of the hardware scheduling hint](https://developer.arm.com/community/arm-community-blogs/b/architectures-and-processors-blog/posts/multi-threaded-applications-arm). Four native budget/state cases per optimization and 16 synthetic QEMU flag/vector cases pass at O0/O2 with fatal UBSan. WFE/WFI/SEV/SEVL remain unsupported; this supplies no guest scheduler or timing/performance equivalence. PAC instructions retain their separate mandatory semantics.

## AES round and column transforms

AESE/AESD/AESMC/AESIMC support exact 128-bit forms with complete source snapshots and unchanged GPR/NZCV/FPCR/FPSR state. Native byte transforms use mathematically derived substitution tables and cached finite-field doublings; independent tests instead use polynomial long multiplication/reduction, brute-force inverse discovery and bit-level affine equations. No new callbacks or ABI changes are required.

The independent matrix checks 13,312 native cases per optimization; 6,656 synthetic QEMU cases match O0/O2 with fatal UBSan. Complete synthetic AES-128 encryption and equivalent-inverse decryption also match the public known-answer example from [original FIPS 197, Appendix C.1](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197.pdf). Algorithm definitions are unchanged in [FIPS 197-upd1](https://nvlpubs.nist.gov/nistpubs/FIPS/NIST.FIPS.197-upd1.pdf). All implementation/test code is original; no upstream implementation was copied. Actual target AES availability/access configuration and hardware timing remain unverified; original execution is still gated.


## Prefetch hint diagnostic profile

PRFM supports GPR-based unsigned-scaled and register-offset forms, plus PC-relative literals; PRFUM supports GPR-based signed-unscaled offsets. All 32 hint fields are retained in typed IR. Register extensions are exactly UXTW/LSL/SXTW/SXTX, shifted by zero or three. Neighboring reserved, writeback and unprivileged encodings remain rejected. SP-based forms remain unsupported, preserving the existing negative test and leaving stack/access trap policy gated.

The diagnostic profile chooses no cache effect: hints consume their normal instruction budget and advance PC without probing guest memory or calling a memory backend. Native tests cover 928 constructed instructions and 8,352 state/budget/fault-preservation cases per optimization, including invalid/unmapped address values. All 928 synthetic QEMU cases match at O0/O2 with fatal UBSan. This supplies no cache-performance, hardware access/trap or target-configuration equivalence and does not authorize original execution. CPU ABI v9 is unchanged.


## Halving integer arithmetic

SHADD/SHSUB and UHADD support 8/16/32-bit lanes in 64/128-bit vectors. The decoder preserves scalar, 64-bit-lane and adjacent unsupported-family rejection. Original C lowering snapshots all sources, widens before arithmetic, explicitly floors signed halves and clears unused upper bits; it changes no QC/NZCV/FP state and requires no callback or ABI change.

Independent byte/i128 expectations check 28,080 native cases per optimization across extrema, odd negative results, full-width differences, aliases and distinct lanes. All 18,720 synthetic QEMU cases match O0/O2 with fatal UBSan. Existing correctness tests and CPU ABI v9 remain unchanged; this supplies no target access-configuration or original execution evidence.


## Integer multiply-accumulate and register pointer stripping

MLA supports B/H/S vector registers and H/S indexed elements with modulo lane arithmetic and complete source/accumulator snapshots. Its decoder is separate from the legacy integer-family entry point, preserving that entry point's existing exclusion tests. Independent byte/u128 tests cover 46,800 native cases per optimization; 31,200 synthetic QEMU fixtures match at O0/O2 with fatal UBSan. Unused upper vector bits are cleared and flags are preserved.

XPACI retains an explicit GPR/zero-register destination and zero modifier in PAC IR. Existing PAC hints retain their LR/SP operands. It calls the existing checked strip operation and refuses a missing backend/context; failures do not commit a register result. CPU ABI v9 and the attributed legacy 39-bit/no-TBI PAC helper are unchanged. Independent native state/budget/profile tests check 111,264 cases per optimization, plus callback/failure probes.

The 1,280 QEMU XPACI comparisons use addresses shared by the diagnostic and QEMU profiles, including canonical top bytes. A separate synthetic test demonstrates QEMU retaining a noncanonical top tag byte while the no-TBI diagnostic helper strips it. This expected profile difference is asserted, not hidden by changing the backend. Actual game PAC/VA/TBI/access configuration remains unverified, and no original function executes.


## 2S reciprocal estimates and leading-zero counts

2S FRECPE uses original integer midpoint quantization, explicit normalization and IEEE bit assembly. It handles quiet/signaling NaNs, signed zero/infinity, tiny-input overflow under each supported rounding mode and sticky FPSR. The diagnostic profile requires an attached FP backend and preserves the existing FPCR whitelist; 4S, scalar and other precisions remain outside this implementation. The existing global 4S rejection test is unchanged. CPU ABI v9 and callback operations are unchanged.

Independent real-valued midpoint expectations cover 1,040,912 native cases per optimization, including every normal exponent/bucket edge, subnormal boundaries, signs and exceptional controls. All 25,728 synthetic QEMU cases match O0/O2 with fatal UBSan. Missing-backend and unsupported-profile tests preserve destination/FPSR state.

2S SIMD CLZ snapshots sources, counts each lane and clears the unused upper half while preserving GPR/control/FP/fault state. Independent leading-zero expectations check 262,144 native cases per optimization; 3,864 synthetic QEMU cases match O0/O2 with fatal UBSan. No helper/ABI extension is required. Target access/FP configuration remains unverified and original execution stays gated.


## Reciprocal square-root estimates and refinements

FRSQRTE/FRSQRTS support exact 2S/4S encodings. Original integer midpoint/boundary search computes estimates, including NaNs, negative inputs, signed zeros and infinities. Refinement preserves one final S rounding through existing callbacks: exact S-to-D conversion, D fused subtraction toward zero, exact halving, odd-bit jamming when inexact, then D-to-S conversion in the original mode. Nonzero finite refinement magnitudes lie between 2^-47 and 2^255, keeping the intermediate D-normal. Exact-zero signs and exceptional inputs have explicit handling.

CPU ABI v9 and all callback operations remain unchanged. The original FPCR whitelist still refuses traps/flush/alternative profiles; output vectors/FPSR commit only after every lane/callback succeeds. An independent 576-bit fixed-point oracle checks refinement, including an adversary where naive D-to-S double rounding fails. Independent midpoint expectations check 1,040,784 native estimate cases per optimization; exact refinement checks 102,144. All 102,144 synthetic QEMU cases match O0/O2 with fatal UBSan. Stage-by-stage failures, aliases and missing/unsupported profiles also pass. These are software/synthetic results; target FP configuration and hardware behavior remain unverified.


## Scoped signed integer gaps

Separate typed operations add 2S signed SMINP/SMAXP, SADDLP from B/H/S pairs in legal 64/128-bit vectors, and 4S SQSHL immediate amounts 0–31. Original emitters snapshot sources before packing results. Pairwise min/max compares signed lanes; SADDLP widens each signed pair before summing; SQSHL uses i128 intermediates, clamps and sets sticky FPSR.QC only on saturation. Unused upper bits clear where required. No backend callback or ABI v9 change is needed. Legacy family-local exclusions and all existing tests remain unchanged.

Independent byte/i128 tests check 25,392 min/max, 2,208 long-pair and 14,656 saturating cases per optimization; 25,360 synthetic QEMU cases match O0/O2 with fatal UBSan. Every legal immediate, threshold neighborhoods, aliases, V31 and preserved control/register state are checked. Actual SIMD access/trap configuration remains unverified; no original function executes.


## Polynomial products, unsigned differences and vector widening

64-bit PMULL/PMULL2 snapshot the selected source halves and compute the 128-bit carryless product through defined unsigned shifts/XOR. Independent coefficient convolution checks all basis-pair degrees, dense patterns and aliases. B/H/S UABD and UABDL/UABDL2 snapshot full sources, form unsigned differences, widen when required, and clear unused output bits. No integer operation changes FPSR or needs a callback.

S-to-D FCVTL/FCVTL2 snapshot the selected two S lanes, invoke existing format-conversion operation 14 for each, and commit vector/FPSR only after both succeed. The existing backend validates FPCR and ABI v9. Missing backend, unsupported profiles and stage-one/two callback faults refuse without partial destination/FPSR state. Other precisions stay outside this operation. Tests independently check finite exact widening and NaN payload/IOC/default-NaN rules.

Independent native checks cover 352,800 polynomial, 99,024 widening and 271,104 unsigned-difference cases per optimization; 9,576 synthetic QEMU cases match O0/O2 with fatal UBSan. Every prior test and acceptance gate remains unchanged. These implementations establish no target SIMD access/feature configuration and permit no original execution.


## Complete SHA1/SHA256 operations

Separate typed records implement SHA1C/H/M/P/SU0/SU1 and SHA256H/H2/SU0/SU1 without altering legacy crypto enum contracts. Emitters snapshot complete sources and accumulators before original unsigned lane arithmetic. Four-round update ordering, scalar-E selection, high-state transport and cross-word schedule dependencies are explicit. SHA1H clears unused bits. All operations preserve GPRs, NZCV, FPCR and FPSR, consume the normal shared instruction budget, and require no callback or ABI change.

Independent bit-truth functions, widened sums and word/round expectations check 27,864 native cases per optimization; 5,032 synthetic QEMU cases match O0/O2 with fatal UBSan. Complete native-generated schedules and compression groups produce six standard SHA1/SHA256 hash known answers per optimization, including two-block padding, plus nine further SHA256 messages matching independent RustCrypto. Aliases, V31, exact budgets and full failure-state preservation pass. Actual target crypto/SIMD access/feature configuration remains unverified; no original function executes.


## Fused reciprocal refinement

2S/4S FRECPS snapshots both vectors, uses existing checked fused-subtract operation 11 for one final S rounding of 2−a×b, and handles infinity-times-zero as positive 2 without invalid. The established NaN/sign/priority rules come from the same attributed backend used by other fused instructions. Backend presence and the original FPCR whitelist are checked even on constant-result paths. Output vectors/FPSR commit only after every lane succeeds; unused upper bits clear for 2S. No callback contract or ABI v9 change is required.

An independent nine-limb fixed-point oracle at scale 2^-298 checks 204,288 native cases per optimization, including a naive double-rounding adversary. All 136,192 synthetic QEMU comparisons pass O0/O2 with fatal UBSan. Every supported mode, fresh/sticky flags, exceptional values, aliases and per-lane callback failure are covered. Target FP/access configuration and hardware equivalence remain unverified; no original execution occurs.


## Remaining scoped integer arithmetic and PACIB

SQRSHL covers 4S and SRSHL covers 4S/2D. Both use only the signed low byte of each count. Original unsigned bit packing implements arithmetic rounded-right shifts; large right shifts return zero, ordinary large left shifts wrap to zero, and saturating left shifts clamp with sticky QC. RSHRN/RSHRN2 cover legal B/H/S destinations and every immediate, with u128 bias-before-shift to preserve carry beyond a 64-bit input. Upper forms retain the destination low half; lower forms clear the unused upper half. Scoped SADALP accumulates adjacent signed H pairs modulo S lanes.

Register/indexed SQDMLAL/SQDMLAL2 cover H/S sources and doubled-width accumulators. Original i128 arithmetic clamps the doubled product first, then the accumulated result; either saturation sets sticky QC. Complete snapshots preserve aliases and upper source halves. Independent native and QEMU checks explicitly cover the minimum-times-minimum case where a single final clamp would be incorrect. All shared budgets and existing flags/refusals are unchanged.

PACIB retains explicit GPR/zero destinations and GPR/SP modifiers, selecting the existing instruction-key-B sign operation. Operand/failure/budget probes cover every destination and modifier class. The existing diagnostic backend remains legacy QARMA5/39-bit/no-TBI, independently checked by the mandatory reference test. Synthetic sign/strip round trips compare canonical reconstructed addresses with QEMU; they do not compare signed bits across different VA/TBI/key profiles. A missing context is refused by the existing backend; the callback contract is unchanged. Actual target keys/configuration remain unverified.

Independent native cases per optimization: 156,672 rounded register, 23,296 rounded narrow, 1,024 pair accumulation and 344,064 saturating long. All 345,984 integer QEMU fixtures pass O0/O2 with fatal UBSan. PACIB adds 564 transport and 564 checked-backend cases per optimization, failure/missing-backend probes and 640 QEMU round trips. No original function executes.

## Static runtime dependency inventory

`tools/survey_runtime_contracts.py` consumes matching strict/all-blocker receipts, groups actual SVC immediates, exclusive-monitor operations and system-register/operation dependencies, and excludes structural or other ISA/policy/opaque/control-flow barriers from contract-only estimates. It also counts overlapping backend requirements in accepted extents. Three synthetic checks preserve complete sets and barriers. Output remains private; no service names, counter values, target profile or runnable-function claim is fabricated. Runtime contracts need explicit semantics and versioned native integration while preserving the ABI v9 compatibility path and existing tests.


## Owned exclusive-memory backend primitive

`exclusive::ExclusiveMemory` owns sealed checked normal memory and all reservation state. Configuration explicitly supplies a power-of-two granule of at least 16 bytes and a nonzero owner quota. Opaque owners carry per-authority identity and monotonically allocated IDs; foreign/released handles refuse, quotas are enforced, and ID exhaustion refuses instead of wrapping. Loads and conditional stores use typed 1/2/4/8/16-byte aligned accesses and zero-extended little-endian u128 values. Unaligned ordinary writes still use the existing complete-transfer checks.

Successful ordinary or conditional writes invalidate every reservation whose configured granule overlaps the written bytes, including writes of unchanged data and transfers spanning regions. Failed ordinary writes leave memory/reservations unchanged. A failed load leaves prior reservations intact. Each registered-owner conditional-store attempt consumes that owner's reservation, including mismatch, alignment or write faults; other owners change only after a successful overlapping write. These are explicit backend primitive rules; architectural exception/trap effects belong to the future native binding. Clear/release operations are per owner, and no mutable memory escape bypasses invalidation.

Seven unit tests cover widths, owner isolation/release, every tested granule byte, same-value interference, peer invalidation, alignment, permission/overflow faults, cross-region atomicity and quota/identity exhaustion. A synthetic ELF reference compares 576 plain/acquire-release load/store sequences at 1/2/4/8-byte widths, including success, CLREX and changed-value interference. Rust debug and release results match QEMU. Serialized 16-byte primitive behavior is unit-tested separately; no hardware pair-atomicity or weak-memory equivalence is claimed.

This module does not add LL/SC IR, alter default instruction acceptance or attach to CPU ABI v9. Actual native integration needs an explicitly versioned interface covering every write path, owner lifetime and status/fault transport. Physical aliasing, devices, target granule/access/trap settings and scheduler/ordering effects remain outside the primitive. Existing game identity/configuration and execution gates remain unchanged.


## Coherent native exclusive contract

`native/exclusive_v10.h` defines a distinct MkwCpuV10 frame with the complete unchanged MkwCpu-v9 prefix plus an exclusive-v1 table/context. Entry names use `mkw_guest_v10_budget` and a version-10 marker, avoiding conflicting v9 declarations. The existing v9 native loader refuses version 10 before entry lookup/invocation. The v1 table has explicit version/size, load/store/clear operations and the attested prefix callback pointers. Exclusive values use exactly two aligned u64 words; unsupported width/order/capacity and null/malformed buffers refuse without output commit.

`exclusive_native::Bridge` registers an opaque owner in a shared authority and keeps its boxed context alive through an attachment. Every plain, ordered, atomic, barrier and exclusive callback locks the same authority. Prefix operations retain their existing v9 signatures, width sets and return conventions. No ordinary write bypasses reservation invalidation; matched CAS and every actual RMW store invalidate even if bytes stay equal, while a mismatched CAS validates permissions but performs no write/invalidation. Domain/access barrier options remain explicit and are strengthened to serialized synchronous normal memory with a real fence. This is not a device or weak-memory implementation.

The C frame gate checks matching prefix/table pointers and context. Pre-existing foreign memory/call routes refuse; attachments restore the empty prior memory route on drop and release the owner when the bridge is destroyed. Every callback rejects a stale/replaced authority; poisoned locks refuse, preserving output buffers. Context pointers must remain owned by the attachment for the entire native call, and host code must not retain them or hold the authority lock while invoking callbacks/destroying the bridge. Unversioned cross-module calls remain refused pending a coherent v10 dispatcher.

Nine native tests compile C at O0/O2 with fatal UBSan and run in Rust debug/release. C/Rust sizes/offsets and distinct marker rejection pass. Per optimization, 3,200 direct LSE cases and 2,560 unchanged-v9-AOT LSE cases match independent arithmetic; the v9 64-byte AOT store/load uses the coherent prefix. Four threads complete 4,000 fetch-adds without lost/duplicate old values and 2,000 real load/store-exclusive increments. Another 576 actual native-callback sequences match QEMU value/status/memory for success, explicit clear and changed-value interference. Profile/argument/fault, CAS/matched-store invalidation, peer ownership and stale/poisoned-authority cases pass.

No LL/SC IR or default decoder/emitter capability is enabled by this contract. Default ABI-v9 survey acceptance remains unchanged; actual target configuration, physical aliases, devices, architectural trap effects, weak ordering and scheduler behavior remain gated. Synthetic v9 leaf compatibility is a deliberate prefix use, not permission to invoke v10 entries through v9 function types.

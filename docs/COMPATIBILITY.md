# Compatibility

| Area | Classification | Evidence/limitation |
|---|---|---|
| Supplied ExeFS identification/decompression | Implemented and tested | Four modules, 12 checked segment hashes |
| RomFS header inventory | Implemented and tested | Every supplied file inventoried |
| Nested asset inspection | Partially implemented | Bounded zstd/SARC sampling; full asset semantics absent |
| Retail version identification | Blocked | Full identities recorded; trusted version mapping/control metadata unavailable |
| Function discovery | Partially implemented | 223,856 validated CIE/FDE extents plus candidates; indirect resolution incomplete |
| Synthetic native translation | Implemented and tested | Host x86-64 executable ran; limited instruction subset |
| Guest memory/relocation foundation | Partially implemented | Synthetic Rust/native memory and dispatcher tests pass; 309,844 relocations apply/read back; serialized diagnostic ordering |
| Actual game CPU translation | Partially implemented | 213,503 subset-liftable indexed extents with possible unbound tails; 72 object samples; no complete module |
| Actual runtime bootstrap | Not started | No guest instruction execution |
| Vulkan/shaders/game frame | Not started | Container observations only |
| SDL3 input/audio, VFS/saves | Not started | No runtime implementations |
| Title screen through all gameplay modes | Not started | No game execution or rendering |
| ARM64 host, Fedora, RADV, Wayland/X11, Flatpak | Not started | Current test environment only; packaging deferred |

No claim of booting, a frame, playability, framerate or commercial-game compatibility is made. No reference Switch 2 execution was available for differential verification.


Scalar/vector S/D FMAXP/FMINP/FMAXNMP/FMINNMP and FABD are implemented under the restricted gradual-underflow/rounding/default-NaN profile. The expanded 86,304-case SIMD numeric matrix passes software QEMU comparisons at native O0/O2 with fatal UBSan. Enabled traps, flush/alternative profiles and actual SIMD/FP access traps remain unmodeled; these results establish no original execution or hardware equivalence.


Integer SIMD SMAX/SMIN/UMAX/UMIN support B/H/S vector arrangements; SMAXV/SMINV/UMAXV/UMINV reduce legal vectors to one zero-extended scalar element. Source snapshots preserve overlap and V31; unsigned comparisons with sign-bit bias implement signed ordering without host signed overflow. Reserved D/scalar arrangements reject and ordinary FCCMP/FCSEL neighbors retain their decoder ownership. SHLL/SHLL2 additionally widen B/H/S elements and shift by their original width, using the existing defined unsigned widening emitter. These operations need no callbacks or ABI v7 changes and preserve GPR/NZCV/FPCR/FPSR. Actual access-trap/configuration gates remain unverified.


Raw XTN/XTN2 narrowing now selects low bytes from doubled-width elements, preserving the destination low half for XTN2 and clearing its upper half for XTN. Saturating narrowing/QC semantics remain separate. Four-S floating min/max reductions now use three checked helper calls in adjacent-pair tree order, with deferred destination/FPSR commit. Both preserve source aliases and V31, require no ABI layout/signature changes, and retain actual SIMD/FP access and target-profile gates before original execution.


The supplied-binary survey retains its full bounded extent-lifting census but selects representative native object samples only from extents with at most 4,096 lifted instructions. Oversized sample candidates are explicitly deferred; later smaller feature examples remain eligible. Summary fields list deferred candidates and any observed features without a compiled representative. An independent validation step checks source hashes, source-map instruction counts, native relocatable ELF headers, module counts and feature accounting. This is a representative compilation limit, not a new supported-instruction or runnable-function claim.


Interleaved LD2/ST2 now use a distinct typed structure operation and one checked 16/32-byte transfer. Complete source/base snapshots preserve vector-list wrapping, aliases and offset wrap; complete-range validation and deferred destination/writeback preserve failure state. Legal Q0/Q1 B/H/S/D arrangements are checked separately; Q0 single-D forms reject. Metadata marks the required memory backend and selects representative objects. LD2/ST2 retain their 16/32-byte sizes within the current ABI v9 contract; multi-register lane/replication forms now use the same complete-access policy. Actual SP/access traps, device/weak-memory behavior and target/bootstrap gates remain unverified.


Non-saturating integer NEG and SHRN/SHRN2 now use defined unsigned native arithmetic with complete source snapshots. NEG wraps per element; SHRN drops bits without rounding and clears the unused upper half, while SHRN2 replaces only the upper destination half. All legal narrowing amounts and B/H/S output widths are bounded at decode time; scalar/reserved neighbors retain their previous ownership or reject. No callbacks, ABI v7 changes or saturation/QC semantics are introduced. Original execution and actual SIMD/FP access-trap behavior remain gated.


SQDMULH/SQRDMULH now lower H/S scalar/vector register and indexed-element operands to native integer arithmetic. Source snapshots and exact lane decoding preserve aliases; bounded int64 products handle minimum-times-minimum saturation before doubling, followed by unsigned high-half extraction. FPSR.QC is sticky and all other status/control fields remain unchanged. Independent i128 expectations and QEMU matrices run at O0/O2 with fatal UBSan. No numerical helper, upstream source or ABI v7 change is introduced. Accumulating variants, actual access traps and target/bootstrap gates remain unverified or unsupported.


Current CPU contract: ABI v9, with unchanged layout/signatures and checked normal-memory sizes 1/2/3/4/6/8/12/16/24/32/48/64 bytes. Invalid widths reject before raw buffer construction; ordered/LSE sizes remain exactly 1/2/4/8. Generated budget entries and FP helpers must report v9; missing/stale versions refuse. Earlier per-step v7/v8 notes describe historical implementation, not current loader compatibility.

All multiple-structure LD1/ST1 lists of one through four registers and interleaved LD2/ST2, LD3/ST3, LD4/ST4 forms now use one bounded callback with full range validation. Snapshots and deferred vector/base commits preserve aliases, list wrapping and failures. The independent native matrix covers 6,360 success and 10,600 fault/permission cases per optimization; QEMU compares 2,120 synthetic cases at O0/O2 with fatal UBSan. Actual hardware transactions, access traps, device memory and target/bootstrap configuration remain gated.

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

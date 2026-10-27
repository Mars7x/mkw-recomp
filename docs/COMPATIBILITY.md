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


Interleaved LD2/ST2 now use a distinct typed structure operation and one checked 16/32-byte transfer. Complete source/base snapshots preserve vector-list wrapping, aliases and offset wrap; complete-range validation and deferred destination/writeback preserve failure state. Legal Q0/Q1 B/H/S/D arrangements are checked separately; Q0 single-D forms reject. Metadata marks the required memory backend and selects representative objects. ABI v7/callback capacity remains unchanged. Other multiple LD1/LD3/LD4 and lane/replicate multi-register forms still require their own semantics and capacity checks. Actual SP/access traps, device/weak-memory behavior and target/bootstrap gates remain unverified.


Non-saturating integer NEG and SHRN/SHRN2 now use defined unsigned native arithmetic with complete source snapshots. NEG wraps per element; SHRN drops bits without rounding and clears the unused upper half, while SHRN2 replaces only the upper destination half. All legal narrowing amounts and B/H/S output widths are bounded at decode time; scalar/reserved neighbors retain their previous ownership or reject. No callbacks, ABI v7 changes or saturation/QC semantics are introduced. Original execution and actual SIMD/FP access-trap behavior remain gated.


SQDMULH/SQRDMULH now lower H/S scalar/vector register and indexed-element operands to native integer arithmetic. Source snapshots and exact lane decoding preserve aliases; bounded int64 products handle minimum-times-minimum saturation before doubling, followed by unsigned high-half extraction. FPSR.QC is sticky and all other status/control fields remain unchanged. Independent i128 expectations and QEMU matrices run at O0/O2 with fatal UBSan. No numerical helper, upstream source or ABI v7 change is introduced. Accumulating variants, actual access traps and target/bootstrap gates remain unverified or unsupported.

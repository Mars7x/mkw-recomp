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


Scalar/vector S/D FMAXP/FMINP/FMAXNMP/FMINNMP and FABD are implemented under the restricted gradual-underflow/rounding/default-NaN profile. The expanded 52,640-case SIMD numeric matrix passes software QEMU comparisons at native O0/O2 with fatal UBSan. Enabled traps, flush/alternative profiles and actual SIMD/FP access traps remain unmodeled; these results establish no original execution or hardware equivalence.


Integer SIMD SMAX/SMIN/UMAX/UMIN support B/H/S vector arrangements; SMAXV/SMINV/UMAXV/UMINV reduce legal vectors to one zero-extended scalar element. Source snapshots preserve overlap and V31; unsigned comparisons with sign-bit bias implement signed ordering without host signed overflow. Reserved D/scalar arrangements reject and ordinary FCCMP/FCSEL neighbors retain their decoder ownership. SHLL/SHLL2 additionally widen B/H/S elements and shift by their original width, using the existing defined unsigned widening emitter. These operations need no callbacks or ABI v7 changes and preserve GPR/NZCV/FPCR/FPSR. Actual access-trap/configuration gates remain unverified.

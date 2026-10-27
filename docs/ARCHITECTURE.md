# Architecture

## Implemented data path

```text
owned ExeFS -> checked NSO/LZ4/SHA-256 -> module-relative segments
                                      -> MOD0 / ELF dynamic symbols / RELA / RELR
                                      -> diagnostics / unwind-index candidates
synthetic AArch64 -> typed instruction IR -> reachable CFG -> C labels -> host cc -> native ELF
owned RomFS -> complete header inventory -> bounded zstd/SARC inspection -> private containment graph
```

No interpreter or JIT fallback exists. Unknown reachable instructions and unresolved CFG edges stop generation. The current IR is an explicit small instruction enum, not a complete optimizing compiler IR. Returns exit a generated-function wrapper. Direct calls use an explicit native dispatcher callback with shared budget and return-PC checks; no real-game dispatcher is connected.

## Identity and privacy

The analyzed ExeFS manifest stores complete-file SHA-256 and all 32 module-ID bytes. IDs are never trimmed based on zero padding. `verify` checks the exact file set and identities before slice generation or diagnostic linking. No patch system exists; no version-specific patches are applied. `software_version=null` is intentional. Metadata verification is not gameplay support.

Generated slices are cached under `private/generated/<source hash>/<translator hash>/slice-<entry>-<length>/`. The translator hash includes the decoder/emitter source and version. Output contains source-PC labels, raw word, module build ID and identity. Native slice compilation, multi-function dispatch, incremental compilation scheduling and crash translation remain future work.

## Address model

`GuestAddr(u64)` is distinct from a host pointer. The runtime owns separate segment buffers, validates mapping overlap and access bounds, enforces declared permissions and zero-initializes BSS. Borrowed read/write slices stay within one region. Transactional read_into/write_bytes operations and the bridge span adjacent mappings after validating every region. Remapping and shared mappings are not implemented. No code executes from these mappings.

Load-time relocation uses a plan: all symbol resolutions and destinations must validate before any write is applied. Loader initialization may modify mapped read-only segments before sealing. Final RelRo page transitions, TLS, initializers, unwind execution, exclusive/RMW atomics and a full guest weak-memory model are not implemented. The new bridge supplies serialized diagnostic plain/ordered access only. LinkScopes binds DT_NEEDED to actual SONAMEs and computes breadth-first dependency closures. A separate explicit process-order policy models default-symbol interposition observed in the supplied rtld; first visible global/weak definition wins. Local/non-default-visible references self-bind. Missing/duplicate dependency identities and invalid scopes reject. Relocation support is ABS64/GLOB_DAT/JUMP_SLOT/RELATIVE and RELR. TLS/IFUNC and other relocations are rejected.

`link-check` uses diagnostic module bases only; alphabetical layout is not asserted to match Horizon. All 309,844 planned relocations now apply and read back under the diagnostic mappings and statically inferred search rules. There is no original-hardware startup trace.

## Intended execution boundary

A future adapted portable lifter must use a reviewed C ABI for CPU state, guest-memory operations, guest-PC dispatch and runtime exits. Native tests cover calls, guest-stack callbacks and an explicit diagnostic PAC profile. Exceptions, threading, indirect dispatch and the actual PAC configuration still require work before game execution. No convenience HLE result may silently stand in for an unsupported service.

Vulkan and SDL3 are selected targets, but neither backend has been implemented. Game CPU/SDK execution should remain separate from host service handling and observed graphics API translation. Maxwell shader translation is not an Ampere solution.

## Reproducibility limits

`Cargo.lock` pins Rust dependencies. The source emitter is deterministic for identical IR; host binaries depend on the compiler/toolchain and build paths. There is not yet a reproducible game build, bounded parallel native build system, or verified guest reference execution. Only the current x86-64 Linux tool environment has been tested.

## Version records

Analysis also creates `private/versions/<aggregate ExeFS identity>/manifest.json`. It includes exact module hashes/IDs, SDK-marker evidence, an inventory fingerprint and empty patch/function-map lists. `runtime_supported=false` and a null retail version distinguish a recognized input from a supported build. Asset expectations are counts/headers, not full asset hashes. Only the supplied layer is recorded. Reanalyzing another input is not validation of its runtime behavior or permission to reuse patches.

## CIE/FDE survey

The reader validates bounded DWARF32 records, CIE versions 1/3, observed zR/zRB/zPLRB augmentations, LEB fields, personality field lengths, pcrel/sdata4 locations and indexed ranges. It retains key-B metadata without executing CFI. The survey regenerates PAC samples and first representatives of supported instruction features per module, with exact identities and source maps. Explicit out-of-extent unconditional B tails and unbound BR requests count as subset lowering, not resolved control flow. Strict slice generation retains hard escaping-edge errors.

## Runtime ABI v7

native/cpu.h is the C ABI authority. ABI v7 includes 32 raw 128-bit vector registers and scalar atomic/data-barrier/floating/conversion callbacks and FPCR/FPSR plus configured guest thread-register state; plain transfers use up to four little-endian words for 32-byte Q pairs. Rust abi::Cpu has C representation and tests check size/offsets/version against compiled C before invocation. MemoryBridge holds owned mappings behind one mutex; plain, ordered and atomic accesses share this serialized diagnostic model. Atomic RMW/CAS validate before mutation and hold that lock for their entire operation. Ordered kinds stay explicit, although this backend does not reproduce the full weak-memory outcome set. Pinned attachments manage context lifetimes and callback restoration.

NativeDispatcher resolves guest targets exclusively through registered borrowed native functions. It switches/restores module bias and shares budgets across nested calls. Internal tail exits route iteratively without changing LR or growing the tail chain on the host stack; targets must be registered and tail callbacks must consume budget. No interpreter or JIT fallback is provided. NativeTarget::load is unsafe because callers must establish the trusted function ABI; the borrowed Library prevents code unloading while targets exist. Guest module identity/execute permissions are not yet tied to registration.

The slice generation identity now includes the decoder, codegen helpers and shared CPU header, preventing ABI/helper changes from retaining the prior translator identity. The surveyed object samples are regenerated rather than treated as a complete cached game build.

## Floating helper boundary

Generated native labels call a typed numeric helper with statically decoded operation/width and raw IEEE operands; there is no guest instruction fetch/decode at runtime. The original adapter uses a pinned, attributed BSD SoftFloat subset, while explicit CPU state carries guest FP controls/status. Unsupported control profiles exit before mutation. Source provenance, adapter/platform/header inputs and library build receipts prevent treating changed helper semantics as a verified cached backend. Original-game registration remains gated on target configuration and identities.

Software thread-register storage is per Cpu and explicitly configured. It is independent of host thread-local storage used internally by SoftFloat. The EL0 register access gate prevents unspecified bootstrap state from silently appearing as a valid thread pointer; it does not replace a TLS allocator, constructor path or scheduler.

## SIMD numeric lanes

S/D 2S/4S/2D arithmetic, FMLA/FMLS and unary operations, vector integer conversions and SIMD scalar S/D conversions lower to a fixed number of checked scalar helper calls. All sources and the accumulator are snapshotted before lane evaluation. Destination bits and sticky FPSR commit only after all lanes succeed; upper unused bits clear. This transactional refusal model handles unsupported profiles without partial vector state and does not implement enabled exception delivery. Half/scaled and remaining arithmetic/structure instructions still reject. Native known-pattern and overlap tests plus 43,232 QEMU comparisons at O0/O2 provide software evidence under the restricted FP profile.

Indexed S/D multiplication and fused accumulate/subtract use a fixed decoded source lane from the source snapshot. Scalar fused sign variants pass the original addend with explicit input-sign transformations to the same scoped numeric backend. Helper signatures/layout stay ABI v7; unsupported controls still refuse transactionally.

S/D format conversion uses floating operation 14 with the input format as the byte parameter; output width is the other S/D format. Six unchanged pinned SoftFloat sources implement format conversion and Arm NaN transport, with all hashes in provenance. CRC operations lower directly to fixed-width unsigned native integer computation and require no callback or host CRC feature. ABI v7 layout/signatures are retained.

SIMD permutations use fixed decoded lane-list transforms over raw source snapshots. Pairwise FP addition statically selects adjacent pairs and reuses the existing add helper; complete result/FPSR commits happen only after all pairs succeed. Neither addition changes the ABI or introduces a runtime guest decoder.

FP min/max operations retain the same borrowed helper ABI. Original helper logic handles NaN priority/number selection and signed-zero ties over raw bits inside the scoped exception environment; vector forms reuse the existing transactional per-lane path. Numeric/default-NaN behavior is tested under the restricted legacy profile. QEMU synthetic reference processes use private working directories, and the publisher still gates source snapshots on a strict artifact scan.

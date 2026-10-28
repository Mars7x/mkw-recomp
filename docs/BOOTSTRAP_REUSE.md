# Bounded bootstrap comparison

The pinned upstream audit in [UPSTREAM_REUSE.md](UPSTREAM_REUSE.md) now has a bounded static comparison with the supplied initialization exports. No original function was invoked, patched or generated for this comparison; input-derived addresses, disassembly and detailed receipts remain private.

Pinned LASRecomp requests `_ZN2nn4init5StartEmmPFvvES2_`, with two callback parameters. The supplied SDK exports `_ZN2nn4init5StartEmmPFvvES2_S2_`, with three. Nine direct calls in that supplied Start body match exact four-instruction import stubs and zero-addend JUMP_SLOT records, then existing diagnostic provider bindings. The supplied rtld has one matching Start import/callsite and three callback pointer getters. The SDK preserves incoming X2, X3 and X4 and invokes them after nnosInitialize, before nnMain and after nnMain respectively. Callback roles and complete side effects still need validation; neither the four-argument LAS launch nor nxnative's manually omitted callback can be copied unchanged.

Seven of eight selected bootstrap exports are strict-accepted. Expanding the first initialization phase's named imports gives 20 selected exports, 18 strict-accepted. These are not runnable closures. Both remaining roots have complete blocker diagnostics with no missing ISA: Start reaches an unresolved successor after nnosQuickExit, and the random-manager constructor does so after its abort call. Strict acceptance and truncated-CFG refusal remain unchanged. No fake return or guessed non-returning-call exemption is added.

The initial OS phase directly calls that random-manager constructor. Its two GetInfo wrapper calls request RandomEntropy, handle zero and subindices 2 and 3; the exact wrapper performs SVC 0x29 and stores X1 to the caller's output. The supplied declarations permit that SVC. [PROCESS_ENTROPY.md](PROCESS_ENTROPY.md) implements the bounded reference service with real immutable host process entropy. This is a necessary dependency, not a solution to the constructor's CFG barrier or proof of startup progress.

Whole-module or 100% survey coverage is not required before runtime work. The useful next measurements are these actual phase closures, callback/constructor/TLS effects and target/main-thread/PAC/FP/time configuration. Additional unobserved SDK service wrappers have lower value. Owned table event handlers support genuine transferred state, while actual IPC copy/move rights, lifecycle and wait cancellation remain separate requirements.

A host phase supervisor could potentially retain all validated callbacks and constructor order while using accepted phase entries, shortening the path around the rejected Start body. That would change the bootstrap architecture and must be explained before implementation. It cannot silently omit rtld state, constructors, finalizers or TLS registration. No such supervisor or wholesale runtime replacement is implemented here. The checked AOT/memory/PAC core, existing correctness tests and original-execution gates remain intact.

The inspected upstream source is [LAS main.c](https://github.com/peiikai/LASRecomp/blob/a4b53a3fc5e91bf101367063bafe45017dc23335/runtime/main.c) and [nxnative main.c](https://github.com/seedds/nxnative/blob/c8ab8166f68296dc712a4ec13b892535d9d790c6/dm/host/main.c), both at the already audited revisions. Their title-specific startup assumptions are source evidence, not demonstrated Mario Kart World compatibility.

## Local source evidence foundation

[TERMINAL_PROOFS.md](TERMINAL_PROOFS.md) adds immutable local control evidence and certifies 21 existing closed bodies without changing lifting or adding runtime guards. The measured startup roots still reject under current admission. Import/cross-image assumptions are explicitly unsupported by this first verifier; the much larger private terminal-call candidate count remains counterfactual, not accepted coverage. This proof foundation precedes any compiler or bootstrap change.

## Same-image conditional source follow-up

[CONDITIONAL_PROOFS.md](CONDITIONAL_PROOFS.md) establishes explicit source theorems for the actual Start and random-manager roots under ten/four self-SDK import requirements respectively. The constructor still may return normally on success. Every requirement remains undischarged; neither root becomes accepted or runnable under existing compiler/runtime profiles. The smaller four-obligation constructor dependency set now defines the next bounded guarded-contract work, preserving the retail phase/callback/TLS sequence.

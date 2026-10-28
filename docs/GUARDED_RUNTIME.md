# Explicit guarded native execution, table v4

The v4 capability connects immutable source contracts, guarded C generation and actual routed-target checks. It preserves CPU frame v10 and adds a 112-byte table with the complete v3 prefix, a preflight callback and its context. Old headers, default/v1/v2/v3 admission and their negative tests remain unchanged. This extends the checked runtime; it does not replace bootstrap, rtld phases, callbacks, constructors or TLS.

## Source and build contracts

`terminal::contract::Contract` is opaque and owns immutable source and its proof dependencies. A conditional control contract requires complete graph closure. A theorem-backed provider must reproduce its exact effect; a plain source artifact with the same bytes cannot impersonate that provider. Initial unconditional seed support requires zero local supporting facts. Import contracts currently support the canonical single four-word thunk; guarded veneers and cross-image no-return evidence explicitly refuse. A separate plain-services factory verifies actual source through the existing services-v3 lift, without widening default conditional analysis or asserting no-normal-return.

`mkw_codegen::guarded::Artifact` accepts the opaque contract, validates the graph and emits v4 C, origin queries and preflight. It removes a call continuation only at an exact source-proven terminal site. If the callback unexpectedly reports normal return, generated code diagnoses contradiction instead of entering a pruned continuation. Every indirect branch in a guarded import thunk returns through the router, including targets inside that thunk; an internal C jump must not bypass the target check.

The origin commitment binds the source/proof contract, analysis model and codegen/header producer fingerprint. The unsafe native loader checks CPU/table versions and the exact advertised origin before registration, and retains library borrows and source contracts. These queries detect mismatches; **self-reported metadata does not authenticate arbitrary machine code**. The caller must retain trusted reproducible builds/helpers and attest the native ABI, source correspondence and reentrant behavior. Native artifact hashes are recorded for supplied object samples. Actual game identity/configuration/execute gates are independent and remain unfulfilled.

## Runtime checks

`mkw_runtime::guarded` uses a separate router and attachment. Registration checks every terminal proof dependency against the exact same-image source/effect at the configured module base. A plain or missing provider refuses. Preflight checks the active registered entry, origin, source coordinate, live table/context identity and coherent memory authority before guest instruction work. Source/code/slot address arithmetic must not wrap.

The router retains active origins across nested calls and checks the actual requested thunk/provider chain before invoking each provider. It does not trust an initial GOT snapshot. A slot changed after preflight, or changed to a returning/unregistered/internal-thunk target, refuses before the unproved provider executes. Exhausted instruction budgets can stop a transfer first. A terminal provider reporting normal return diagnoses contradiction; fault/budget statuses propagate. This is a bounded contract, not rollback of side effects from dishonest injected native code, guest exception unwinding, process lifecycle or a retail scheduler.

The v3 typed services and owned memory/monitor engine are reused explicitly. No service is registered by default. Ordinary service bodies can return normally; handled SVC completion still clears the local monitor. Host-call target adapters, worker integration, guarded veneer fragments and cross-image terminal proofs are not added by this first v4 implementation.

## Measured evidence

Eight new Rust tests (three source-contract and five native integration tests) pass debug/release. Native C compiles at O0/O2 with fatal UBSan. Evidence includes:

- 588 exact budget/register/PC traces across direct calls and tails, valid providers, repeatedly changed slots and all four internal thunk targets.
- A four-import transitive chain: 98 budget traces, eight independent changed-slot refusals, and ten returning-branch traces. The caller's normal success return is preserved.
- Slot mutation through a nested service after preflight; active-origin recovery after refusal; ordinary SVC return; LL/SC monitor clearing compared with two real Linux/QEMU close(-1) cases. This is transport/monitor evidence, not a Horizon service oracle.
- C/Rust table sizes/offsets, malformed prefixes, producer/source/profile/dependency mismatches, source/library lifetime, foreign frames, absent active origin, contradictory-return injections and generated-code refusal of a falsely successful terminal callback.

`guarded-compile-check GAME_ROOT` verifies the supplied identities, reconstructs opaque same-image source proofs and runs every counted entry through source-contract and v4 emitter checks. It finds **218,749 capability-scoped translations**, including **4,085 additional SDK translations** beyond v3. Default remains **214,521**, v1/v2 **214,585**, v3 **214,664**. These existing profiles gain zero; the new v4 profile adds 4,085 eligible translations. This is compiler eligibility under explicit contracts, not runnable-function coverage.

Nine source fragments from the actual random-manager constructor proof closure (588 bytes) compile into private native objects: the returning constructor, four no-return bodies and four import thunks. All original-derived C, object code, addresses, hashes and binding records stay private; no original entry executes or route is activated by the survey. Native compilation is sampled, rather than compiling every eligible body. The supplied source/compiler result does not establish first game execution, boot, a frame or playability.

Further broad survey work now has lower first-execution value than the selected ordinary-callee/service closure and actual target, TLS/main-thread, PAC/FP and lifecycle configuration. Do not make full survey coverage a prerequisite, replace retail phases silently or treat diagnostic refusals as successful game behavior.

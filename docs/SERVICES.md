# Runtime services

No guest service calls are currently executed or answered. There are no success-returning service stubs.

The private NPDM manifest records declared service permissions and kernel capability words. The import map records unresolved symbols and binding/type information. These are requirements evidence, not a call trace. Static SVC opcode locations are diagnostic observations; linear scanning can include data.

The runtime implements owned mappings/BSS, permissions, scoped relocations, transactional transfers, a serialized plain/ordered/atomic memory bridge and a registered native-call dispatcher with iterative tail routing, explicit diagnostic data barriers and a bounded FP control/arithmetic profile for synthetic tests. Explicit guest software thread-register storage now exists, but actual TLS templates/allocation and guest thread scheduling do not. No real startup ABI or service emulation is connected. It does not implement process entry, rtld startup, constructors, destructors, exception delivery, TLS, scheduling, events, IPC, clocks, SDK services, filesystem mounts, saves, audio, input, graphics or networking.

Load-time scopes and relocation readback now pass. Before bootstrap, implement the startup ABI and verify the actual PAC feature profile. Explicit authenticated-return lowering exists with a diagnostic backend only. Keep the original SDK execution path available where practical; introduce native implementations only after identifying actual requirements. Offline gameplay and same-device multiplayer retain priority over online services.

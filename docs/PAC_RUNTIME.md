# Owned diagnostic PAC attachment

The selected random-manager constructor uses PACIBSP and RETAB. A fresh static
survey of its normal-return-side closure resolves five bodies (1,196 bytes) and
three ordinary import stubs (48 bytes), with GetInfo SVC 0x29 and no observed
TPIDR/FPCR/FPSR instructions. Seven additional normal-path fragments compile into private native objects;
together with the prior proof objects, the bounded closure is 16 fragments /
1,696 source bytes. No original library is loaded or route activated. Error
paths retain their separate conditional proof obligations. This is bounded source evidence, not a complete process startup or
permission to execute original code.

`pauth::Backend` owns immutable IA/IB keys and borrows a trusted native library.
Construction explicitly selects `DiagnosticProfile::Legacy39NoTbiQarma5` and
checks callback ABI, profile version, context size and field offset queries from
`native/pac_profile.c`. The existing attributed PAC/QARMA implementation and
all previous native headers are unchanged. Version 1 means legacy PAuth,
39-bit virtual addresses, no TBI and QARMA5; it is **not verified for T239**.
Queries detect accidental mismatches; the unsafe loader still requires trusted
code with correct, reentrant, read-only context semantics.

The RAII attachment borrows the entire CPU-v10 frame, so coherent memory and the
v4 router can be attached inside it. Both callback and context are restored on
normal exit and Rust unwind. Nested attachments restore the previous process
context. Library/key lifetimes outlast every attachment, and independent frames
can share the immutable process keys. No default keys, serialization or key
logging is provided; erasure of key material from host memory is not guaranteed.
No process entropy is silently repurposed as a hardware PAC key.

Four additive tests cover O0/O2 fatal-UBSan native builds: exact C/Rust key order,
both instruction keys, lower/upper pointers, modifiers, sign/authenticate/strip,
authenticated-return failure, unsupported operations, nested unwind restoration,
512 concurrent round trips across both optimizations, and missing/mismatched
metadata refusal. Guarded PACIBSP/RETAB programs exercise 24 budget/fault traces.
A synthetic Linux/QEMU program confirms RETAB branches to the authenticated
address while leaving signed X30 unchanged. QEMU chooses its own keys/profile;
that check establishes instruction transport, not matching PAC bits or retail
compatibility. Existing independent QARMA reference checks remain required.

Compiler coverage does not change: strict 214,521; v1/v2 214,585; v3 214,664;
guarded v4 218,749. Actual target geometry/TBI/PAC policy, authenticated metadata,
startup/TLS/main-thread state and original-function authorization remain open.
The next useful work is composing the complete selected normal/error source
closure and establishing those runtime prerequisites, rather than extending
survey coverage alone. No original game/SDK function has executed.

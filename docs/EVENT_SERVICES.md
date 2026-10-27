# Explicit event SVC reference profile

`EventServices::new(Abi::Horizon64EventReference, quota).registry()` creates an explicit event-only SVC registry on the coherent table-v3 boundary. It supplies real owned event state for five operations. Constructing/selecting this reference profile does not establish Switch 2 compatibility or authorize original code. Default service registries stay empty.

| SVC | Operation | State/result behavior |
|---|---|---|
| 0x45 | CreateEvent | Atomically creates both endpoints; W1 write, W2 read, W0 result |
| 0x11 | SignalEvent | Requires live write end; sets persistent signal state |
| 0x12 | ClearEvent | Accepts live read/write end; clears signal state |
| 0x17 | ResetSignal | Requires signaled read end; distinct already-clear error |
| 0x16 | CloseHandle | Removes one live event endpoint; final end releases object/quota |

The selected reference uses module 1 results: InvalidHandle description 114 (0xe401), InvalidState 125 (0xfa01), and LimitReached 132 (0x10801). Failed operation replies update W0 and retain unspecified output registers; this does not claim retail failure-output values. Host identity exhaustion, poison or unsupported model state refuses internally rather than inventing kernel success. Input handles use W0's low 32 bits. All handled replies use the existing exception-return monitor and budget contract.

All handlers in a registry share one process event namespace. Cloned registry views retain that namespace; separate instances reject foreign handles. This profile must not be combined with another handle-producing kernel backend before handle types/ownership are unified. It currently cannot close thread/process/device objects. ResetSignal of the current-process pseudo-handle explicitly refuses because process state is absent.

WaitSynchronization (0x18), clocks, blocking waits, cancellation, threads/process objects, interrupts and SDK initialization are not registered. In particular, an unimplemented wait returns the transport's unsupported exit; it does not pretend to have waited or timed out. There is no automatic binding to supplied game code.

## Evidence and provenance

Three added native tests pass debug/release at O0/O2 with fatal UBSan. They verify the actual SVC instruction/callback/register path, kernel-reference result encodings, endpoint permissions, reset state, both-end lifetime/quota reuse, W-register input truncation, stale and foreign rejection, cloned/shared namespaces and unmodeled-operation refusals. Budgets distinguish no instruction/no allocation from a completed CreateEvent followed by an exhausted RET: the latter correctly retains its object. Four concurrent native clients perform 128 create/signal/reset/close cycles with 256 distinct endpoints per optimization. Empty registries remain unsupported; no implicit service appears.

Existing QEMU transport/monitor comparisons remain required; Linux QEMU is not an oracle for Horizon event results. No original game/SDK function executes. Strict and all conditional compiler survey counts stay unchanged. Previous tests, ABI/native headers and correctness gates remain required.

This is an original Rust binding around the attributed MIT event primitive described in EVENTS.md. Behavior/result facts were checked against pinned [event](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/svc/kern_svc_event.cpp), [synchronization](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/svc/kern_svc_synchronization.cpp) and [result definitions](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libvapours/include/vapours/results/svc_results.hpp) at `6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8`. `event-services-reference.json` records inspected file identities. Those GPL implementation files remain private reference material; no implementation was copied or linked. These references describe an inspected open kernel, not a verified retail Switch 2 trace.

Next: unify handle kinds and implement genuine wait/cancellation and dynamic thread lifecycle under verified target/TLS/time contracts. Keep original execution behind exact identity/configuration gates.

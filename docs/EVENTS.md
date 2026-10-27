# Owned manual-reset events

The runtime has an owned event primitive with paired read/write endpoints, manual-reset signal state, capability checks, bounded immediate polling and object lifetime management. It is a reusable kernel foundation, not a Horizon SVC binding or a thread scheduler. No host clock, blocking wait, cancellation or successful generic service stub is supplied.

`Events::new` accepts an explicit quota of 1–1,024 live event objects. Creation atomically allocates both endpoints. A shared process-wide identity source assigns non-reused u32 handles below the reserved waiter-bit range; exhaustion refuses without partially inserting an object or endpoint. Each table accepts only its own live handles. Foreign, stale and duplicate closes refuse.

Signal requires the write end. Clear accepts either end and is idempotent. Reset requires a signaled read end; resetting a clear event reports a distinct error. Poll validates every read handle, returns the first signaled index and leaves the signal set. The primitive requires 1–64 poll handles. Closing one end retains the object and its quota charge until the other end closes. These are typed Rust operations/errors, not assertions about target numeric handles or SVC results.

## Evidence and provenance

Five added tests pass debug/release. Three Rust tests cover rights, persistent signal/reset state, lowest-index selection, last-endpoint lifetime, quotas, global uniqueness, failed pair allocation, exhaustion, invalid/foreign/stale handles and 4,000 concurrent object cycles. Two native tests compile author-created v3 service fixtures at O0/O2 with fatal UBSan and check the same state transitions across the actual CPU/call boundary, including four concurrent callers performing 400 cycles / 800 distinct endpoint identities per optimization. Fixture numbers 0x700–0x705 and fixture result codes are deliberately artificial, not Horizon service mappings.

Existing QEMU instruction/exception-return tests remain required. QEMU Linux syscalls are not an oracle for Horizon event behavior. Default and all opt-in compiler survey counts remain unchanged; no original game/SDK function executes.

The signal/lifetime semantics adapt selected MIT nxnative `dm/host/hle_thread.c` logic at `c8ab8166f68296dc712a4ec13b892535d9d790c6`. `third_party/nxnative/EVENTS_PROVENANCE.json` records source/adaptation hashes, notices, changes and exclusions. Existing seedds and credited d2r-mobile MIT notices are retained. The Rust storage, locking, quota and identity implementation is independent of upstream's native pointer/handle table and Mach/guest-stack machinery.

Two upstream behaviors are deliberately excluded: invalid CloseHandle returning unconditional success, and stopping input validation as soon as a ready object is found. Our primitive rejects invalid closes and validates all poll inputs. A pinned [Mesosphere synchronization reference](https://github.com/Atmosphere-NX/Atmosphere/blob/6b0c077b4c608a3a2ee712b8dc35fee87ceb62e8/libraries/libmesosphere/source/svc/kern_svc_synchronization.cpp) corroborates checked handle removal and complete object acquisition. That GPL source was inspected privately as semantic reference; no implementation was copied. It does not prove Switch 2 behavior.

## Next binding requirements

Real kernel binding remains separate: validate target result/register conventions and permissions, unify handles with thread/process/resource objects, establish pending-wait reference ownership, cancellation and actual timeout/scheduler contracts. A future WaitSynchronization binding must handle its zero-handle case separately: the pinned kernel reference accepts it, while this nonempty poll helper intentionally rejects it. Never translate that primitive limitation into an invented kernel error.

No production SVC handler is registered by this module. Initialization of actual SDK thread/TLS objects and original execution remain behind identity/configuration gates. The bounded primitive advances runtime work without requiring 100% survey coverage.

## Explicit event SVC reference bindings (October 27)

[EVENT_SERVICES.md](EVENT_SERVICES.md) describes five opt-in reference-shaped event handlers with real owned state, typed W0/W1/W2 results and no default activation. Three O0/O2 fatal-UBSan native tests add budget, namespace, lifetime, error and concurrent-client evidence. These bindings are event-only; waits, clocks, cancellation, threads/process objects and actual retail target verification remain open. Compiler counts, prior tests/headers and original-execution gates remain unchanged.

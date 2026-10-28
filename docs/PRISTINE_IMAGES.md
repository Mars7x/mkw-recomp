# Initial module-image checkpoint

`module_pages::Plan::verify_pristine(&Memory)` verifies the current bytes and
mapping shape against the immutable plan's borrowed module payloads. Every
planned allocation must retain its exact base, extent and permissions, original
payload, and zero BSS/page padding. Heap-backed replacements reject. Additional
mappings, such as stack storage, are allowed. The check reads owned host backing
without granting guest read permission to execute-only pages. It changes no
bytes, permissions or seal state.

This is a point-in-time content check, not a load-history token or an execution
gate. Independently allocated equal images pass, and a change restored to the
original bytes passes. It does not authenticate input identities, determine
retail startup configuration or authorize original code. The caller must retain
ownership or the same memory lock between checking and using the image. An empty
plan checks no module mappings. Existing mapping and relocation APIs remain
unchanged; the diagnostic host linker still applies its explicit plans.

## Why startup needs a separate initial-state check

Fresh supplied rtld inspection shows self-relative relocation before process
module discovery. Other modules are discovered by ascending address-space
queries for executable Code mappings; candidate validation and BSS clearing
precede their initial relative relocations and module-list insertion. RELA uses
an explicit addend, while RELR adds the module base to the current destination
word. Therefore entering that loader over an already host-relocated image can
apply the base twice. Replaying the same immutable host relocation plan differs
from building a new plan against modified memory.

The supplied four-module check passes before host relocation and rejects after
it. It retains original payload/zero-fill checks and 309,844 verified relocation
writes. Replanning changes all 283,734 RELR destinations by the corresponding
base; replaying the original plan restores its verified outputs. All 309,844
destinations also pass checked guest-write permission checks with unchanged
values before relocation. No original
function or generated library is executed or loaded by this check.

Four additive tests cover sealed/execute-only images, unrelated mappings,
independent equal images, changed/restored payloads, BSS and padding, missing,
split, shifted and wrong-permission allocations, plus the RELR/RELA distinction.
Twelve private authored arithmetic cases agree between native C at O0/O2 with
fatal UBSan and AArch64 Linux/QEMU, including wrapping arithmetic and one/two
passes. This is an arithmetic oracle, not a Horizon loader-policy oracle.

## Module order and remaining configuration

The observed rtld prepends each discovered module to its list. Initialization
traverses that list forward; finalization and the observed global symbol search
traverse it backward. Under the current diagnostic addresses, discovery/global
search is rtld, main, SDK, additional module; initialization reverses that order.
A separate authored list model checks 384 insertion/traversal/null-callback
configurations. This is not proof of the retail address layout, later dynamic
loading or all symbol-resolution policies.

The direct rtld startup call set contains 15 FDE bodies totaling 5,820 source
bytes; all compile to private native objects. QueryMemory, Break and debug-string
services and the SDK Start import occur in that set. Indirect calls, actual entry state,
software/target configuration, TLS and SDK initialization remain unresolved.
All target/identity gates, source proof obligations, native/QEMU checks and
acceptance rules remain intact. No bootstrap supervisor is introduced, no
constructor is skipped and no boot/frame result is claimed. Runtime startup
state and closure remain more useful than additional survey coverage.

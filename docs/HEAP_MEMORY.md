# Explicit bounded heap memory

`ExclusiveMemory::with_heap(memory, config, HeapSpec)` reserves one fixed guest
address interval and returns the coherent memory object plus an opaque `Heap`
capability. Base and capacity must be page aligned, nonempty, nonoverflowing and
within the existing `MAX_IMAGE` allocation bound (512 MiB). The entire capacity
must avoid every existing mapping, even though the initial accessible size is
zero. There is no inferred heap base, process address space or capacity.

Backing storage is owned and zeroed at construction. `resize_heap(&heap, size)`
changes only the accessible prefix, in 4 KiB steps within that fixed capacity.
Retained bytes survive; changed bytes are zeroed on shrink/growth, so shrinking
and regrowing cannot expose old bytes. `heap_size` reports the current accessible
size. Invalid sizes/capabilities fail before mutation. Heap capabilities remain
scoped to the memory generation; a replacement memory instance cannot inherit
one. The capacity is fixed and this is not sparse host-memory allocation.

Ordinary mappings stay sealed. The default `ExclusiveMemory::new` behavior is
unchanged. All existing plain, ordered, atomic and exclusive transfers continue
through the same checked memory object. A host resize requires the same mutable
borrow/mutex used by those transfers. Mapping bounds are checked before writes;
accesses that cross the accessible heap end fault without partial writes.

Resizing invalidates reservations whose granules overlap the changed range.
Unaffected reservations survive; no-op and rejected resizes preserve them. This
also prevents an old reservation from surviving shrink/regrowth at the same
address. Granules larger than a page are handled conservatively. Fixed stack
query bindings remain valid, while mutable heap regions cannot be declared as
immutable stack mappings through `StackQuery`.

Five new tests cover full-capacity overlap and configuration refusal, zero initial
visibility, retained/zeroed bytes, capability generations, invalid size/no-op
behavior, reservation isolation and shrink/regrowth, and stack classification.
The native test includes 576 load/store budget traces at O0/O2 with fatal UBSan,
plus native exclusive-reservation and boundary-fault cases. Debug/release suites
and all prior native/QEMU checks remain required. This host-memory primitive is
not a Horizon kernel oracle or a weak-memory/MMIO model.

The measured startup path calls SDK heap sizing/allocation before installing title
TLS allocator callbacks. Static probes trace the SDK heap manager to its kernel
wrapper and preserve the critical-section and terminal-error dependencies.
No original function is executed. This foundation does not yet register SVC 0x01,
produce Horizon results, commit a resize and SVC outputs as one transaction, or
provide heap QueryMemory, permissions, aliasing, physical memory or a full virtual
memory manager. Those require separate explicit contracts and tests. Actual
initial thread/TLS/phase, target/PAC and signature gates remain unresolved.

Architectural choice: retain checked Rust memory and add a narrowly scoped heap
capability instead of replacing the backend. The pinned MIT
[LAS runtime heap handler](https://github.com/peiikai/LASRecomp/blob/a4b53a3fc5e91bf101367063bafe45017dc23335/runtime/kernel.c)
provides a useful service/mapping model, but its direct guest-address memory path
would not preserve this runtime's checked transfers and shared reservations.
No upstream implementation was copied or linked. Adapting further kernel service
semantics is still possible without replacing the CPU or bootstrap phases.

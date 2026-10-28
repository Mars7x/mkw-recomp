# Explicit process region reservations

`process_regions::ProcessRegions` validates an explicitly configured `Layout` against sealed owned page mappings. It supplies immutable reservation snapshots for later service integration. It neither discovers a kernel layout nor registers GetInfo or QueryMemory handlers.

The address-space span contains five nonempty, 4 KiB-aligned, disjoint reservations: code, heap, stack, alias and remote. Arithmetic is checked. Heap bounds must exactly match the full capacity of the supplied heap capability in this memory generation. Every fixed mapping must fit wholly inside code or stack; stack mappings must be RW. Alias and remote ranges therefore remain uncommitted. Empty code/stack reservations are permitted; declaring a reservation does not allocate its pages.

Construction requires page-exact sealed mappings. Snapshots lock coherent memory and reject replacement or poisoned memory. Heap growth/shrink changes commitment, while reservation bounds remain unchanged. Reading a snapshot consumes no owner slots and changes neither memory nor exclusive monitors.

This deliberately disjoint host contract is not a claim about all Horizon or Switch 2 layouts. In particular, an initial stack mapping does not establish the whole stack reservation, and visible heap size does not establish heap reservation bounds. No fabricated region answers are exposed to original code. Real target configuration, region service semantics and bootstrap identity gates remain unresolved.

Six additive tests cover invalid bounds, overlap, overflow, mapping permissions, foreign heap capabilities, stale generations, poisoning and concurrency. Native synthetic heap-service cases run at O0/O2 with fatal UBSan across 18 size/budget combinations. Concurrent testing performs 2,048 snapshots during 1,024 heap resizes. Existing correctness checks remain unchanged.

A private supplied-input probe places four modules in explicitly synthetic reservations, preserves payload and zero padding/BSS, checks the existing diagnostic relocation selections, reads back all relocation writes, and confirms bounds remain stable through heap growth. It does not execute original code or verify the configured ranges against a target kernel. Survey coverage is expected to remain unchanged; this work advances the runtime configuration foundation.

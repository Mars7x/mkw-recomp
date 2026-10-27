# Concrete dependency order

1. Verify the actual PAC/address regime and bind native registration to exact module identity/executable permissions before executing original functions. Scoped relocation readback and synthetic checked-memory/native dispatch now pass; rtld startup, final RelRo and TLS remain incomplete.
2. Extend lowering from the refreshed census: FP conversions, remaining arithmetic/unary/vector operations and control profiles, plus scalar bit operations. Basic S/D arithmetic/compare/FMADD, legacy FP state, H/S/D FCSEL and normal-memory DMB/DSB now pass diagnostic tests. Raw SIMD register bits and scalar LSE RMW/CAS now pass native diagnostic tests. BR/tail routing, CCMP/CCMN and integer multiply/divide are implemented; complete indirect-target discovery/binding remains open. Common scalar/SIMD memory forms and integer logical/bitfield operations are implemented. RCpc/RCsc acquire/release tags, serialized diagnostic memory, TBZ/TBNZ, BLR, shifted arithmetic and conditional selects are implemented. Actual weak-memory equivalence is not established.
3. Evaluate/adapt the MIT LAS portable lifter behind the checked memory/CPU ABI. Retain native semantic tests and reject unsupported PAC/FP/SIMD/atomic behavior. Add authenticated-return and guest-memory instruction tests before executing game functions.
4. Generate multiple private native translation units tied to exact module identities. Compile and measure; complete translation coverage is a separate gate from startup.
5. Implement the observed startup, module initialization, TLS/thread and required service path, with explicit unsupported diagnostics. Observe repeatable real initialization milestones.
6. Trace real graphics bootstrap/submission and shader payloads. Establish Ampere semantics, texture layouts and synchronization before Vulkan draws. Validate a genuine game-generated frame.
7. Integrate SDL3 input/audio, VFS and saves, then verify title/menu progression and a playable race. Expand modes only from observed behavior.
8. Profile correct gameplay before optimization, enhancements or Flatpak packaging.

The long-term acceptance criteria remain the user's original phases. The foundation does not relax them or replace the original game with a recreation.

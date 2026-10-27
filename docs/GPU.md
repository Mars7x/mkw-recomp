# Graphics investigation

No Vulkan backend, translated shader, real draw or presented game frame exists.

Observed evidence consists of executable bootstrap imports, resource-reference strings using `Nin_Ounce_NVN2`, and sampled shader containers with FSHA/BNSH signatures. This supports investigating an NVN-related path. It does not establish the interface ABI, command packet grammar, shader ISA, resource bindings or synchronization model.

Next work must connect the bootstrap import to actual function-table calls and original SDK code, identify command-buffer construction and shader payload boundaries, then identify and decode the actual shader instruction representation; Ampere compatibility is not yet demonstrated. Switch 1 Maxwell translators inspected during research cannot be reused without demonstrating compatibility. No shader payload has been published or replaced with meaningless SPIR-V.

Texture samples have BNTX signatures. Tiling, formats, compression, swizzles and lifetimes are unknown until parsed and validated. Vulkan 1.3/RADV and SDL3 windowing remain targets; no hardware or Wayland/X11 compatibility has been tested. Audio/input backends remain unimplemented; the read-only filesystem HLE described below is separate from graphics.

## Pinned upstream reuse and bounded runtime integration (October 27)

See [UPSTREAM_REUSE.md](UPSTREAM_REUSE.md) and upstream-reuse-audit.json for pinned nxnative/LASRecomp/nx2android source findings, licenses, portability costs and ranked execution/frame opportunities. The selected nxnative-derived five-API read-only filesystem profile now uses checked host targets on the coherent v10/table-v2 router. Five exact supplied provider descriptors cover ten references; a synthetic native wrapper reads 1,342 supplied RomFS bytes matching an independent stream. This is opt-in service transport, not proof of the retail SDK object layout or original game startup. Six call, four filesystem and two hook tests add to existing checks; no original code executes. Default survey gain stays zero (214,521); conditional v1/v2 totals stay 214,585. Prior acceptance and correctness checks remain required. Runtime configuration, thread/TLS ownership and Switch 2 graphics/shader identification now take priority over further survey-only work.

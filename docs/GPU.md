# Graphics investigation

No Vulkan backend, translated shader, real draw or presented game frame exists.

Observed evidence consists of executable bootstrap imports, resource-reference strings using `Nin_Ounce_NVN2`, and sampled shader containers with FSHA/BNSH signatures. This supports investigating an NVN-related path. It does not establish the interface ABI, command packet grammar, shader ISA, resource bindings or synchronization model.

Next work must connect the bootstrap import to actual function-table calls and original SDK code, identify command-buffer construction and shader payload boundaries, then decode the observed Ampere instruction representation. Switch 1 Maxwell translators inspected during research cannot be reused without demonstrating compatibility. No shader payload has been published or replaced with meaningless SPIR-V.

Texture samples have BNTX signatures. Tiling, formats, compression, swizzles and lifetimes are unknown until parsed and validated. Vulkan 1.3/RADV and SDL3 windowing remain targets; no hardware or Wayland/X11 compatibility has been tested. Audio/input/native services likewise have no implementations.

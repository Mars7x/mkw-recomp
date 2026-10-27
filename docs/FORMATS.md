# Formats and input analysis

The input is the workspace's `base/` tree. Original files are opened read-only. Detailed names, symbols, addresses, headers and asset metadata remain in `private/analysis/`.

## Executables

Four NSO0 modules and one META/ACI0/ACID metadata file are detected by signature; `.listing` is recorded as transfer metadata. All three segments of every supplied NSO decompress using the established LZ4 library and pass embedded SHA-256 checks. Header compression flags, memory permissions, BSS, module names, full module IDs, complete-file hashes and MOD0 dynamic data are recorded.

RELA and packed RELR tables are parsed separately. Dynamic symbols/imports/exports and dependency names are retained. Unwind header indexes are validated for the observed encoding; The separate cpu-survey validates observed CIE augmentations and FDE extents; CFI execution and actual exception unwinding are not implemented. NPDM includes the Ounce ACID layout, service-access names and raw kernel capabilities. Signatures are not authenticated. NPDM's version field is not treated as the retail game version.

Unknown NSO versions/flags, malformed ranges, hashes, overlaps, decompression failures and unsupported Zbic compression are errors. Ordinary zstd is supported only for detected ordinary frames. NRO/other dynamically loaded executable formats are not loaders yet.

## RomFS

205,781 files, 23,278,867,417 bytes, including 121 `.listing` files. The inventory opens every file for a header and records path, size, extension and signature. 140,650 outer zstd frames, 20,957 BWAV signatures, 352 BYML signatures, 49 EBML containers and 34 SARC signatures were counted. Signature labels ending in `candidate` do not claim full structural parsing.

Bounded inspection covers every filename-declared SARC plus eight files per directory/extension-chain/signature class. Inputs/decoded payloads are limited to 64 MiB and nesting to four inspection levels. Inspection failures are retained. It is not a complete deep scan or full-content integrity audit. Nested archive entries record containment edges; they are not a resolved runtime asset-dependency graph. Executable string-reference candidates are separate and labeled as unexecuted observations.

Decompressed samples contain FRES, BNTX, BYML, FSHA/BNSH and other signatures. Their filenames and directory names suggest models, textures, configuration, UI, sound, world/terrain and physics resources; only future full parsers can establish detailed resource semantics. Shader headers do not identify their ISA. Full asset parsers, texture untile/decode tools, sound decoding, Phive structures and the observed TARB terrain containers need investigation.

Every supplied directory listing parsed successfully and its 205,787 file/directory entries match presence and declared sizes. This is evidence of consistency with those listings. It cannot establish that the source extraction included every required retail file, update layer, dynamically loaded module or DLC. No trusted complete-content manifest is available.

`base` in the folder name and `Product_100` in filenames are hints only. There is no verified version 1.0.0 or 1.8.0 assignment. Update and mod overlay precedence is designed as mod > update > base but is not implemented or enabled.

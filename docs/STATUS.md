# Status — October 27, 2026

SIMD permutations, pairwise addition and FP min/max are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added ZIP1/ZIP2, UZP1/UZP2 and TRN1/TRN2 for legal 64/128-bit B/H/S/D arrangements. Sources are snapshotted before lane packing; unused upper bits clear, overlap/register 31 work, and general registers/NZCV/FP state remain unchanged. Reserved single-D forms reject, while neighboring EXT retains its own semantics.
- Added S/D scalar FADDP and vector 2S/4S/2D FADDP through the existing checked FP helper. Scalar forms add the source pair; vector forms concatenate pairwise sums from each source. Lane results/FPSR commit only after all helper calls succeed. Mandatory tests cover ordering, signaling NaNs, sticky flags and forced later-lane refusal.
- Added S/D scalar/vector FMAX/FMIN/FMAXNM/FMINNM with explicit signaling/quiet-NaN priority, numeric selection, signed-zero ties, sticky IOC and output default-NaN handling. Original code compares raw finite encodings without host FP. Mandatory scalar/vector patterns and expanded QEMU matrices verify the restricted profile.
- A stale crash dump from an earlier invalid synthetic fixture was correctly rejected by the publication guard. The dump/failed receipt were quarantined privately; QEMU fixture runs now use private working directories. No dump was published and no game code was involved. The corrected fixtures pass.
- Independent lane-sequence tests cover 8,064 permutation cases per native optimization; 210 separate QEMU cases check encoding/arrangement behavior. The expanded vector numeric matrix contains 43,232 QEMU cases. Native O0/O2 builds use fatal UBSan.
- Survey metadata/sample selection now separately records SIMD permutation and pairwise FP operations. ABI v7 and the restricted FP profile remain unchanged. CRC and S/D format conversion are published as `7e6033b`. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **212,303 indexed extents**, including **125,474 with PAC operations**, up from 211,850 under the same tail policy. **116 accepted extents contain permutations, 94 contain pairwise FP operations and 309 contain FP min/max** (categories overlap). **58,330 contain direct extent tails; 14,012 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Fifty-eight representative game-derived samples compile into native objects, 42 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **111 Rust tests pass**, including permutation sequences, pairwise FP ordering/refusal, min/max NaN/signed-zero semantics and earlier CRC/conversion/fused/indexed cases, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **210 permutation fixtures** match QEMU at O0/O2; independent native lane-sequence tests cover 8,064 cases per optimization, including aliasing and upper clearing.
- **8,768 S/D format-conversion fixtures** and **40 CRC fixtures** match QEMU at O0/O2. Native CRC tests independently use polynomial division and standard string check values.
- **43,232 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **26,416 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **86,826 constructed encodings**, including **47,306 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
- Native semantic cases run at O0/O2 with fatal undefined-behavior instrumentation. Synthetic programs only are executed; native game sample objects are compilation evidence.
- **4,096 independent QARMA5 comparisons pass** against privately compiled QEMU reference code. The game's PAC profile is still unverified. The legacy/QARMA5/39-bit/no-TBI backend remains diagnostic and selected only by synthetic tests.
- Formatting, Clippy with warnings denied, exact ExeFS/segment hashes, changed-identity refusal and truncated-CFG refusal pass. Public artifact checks exclude original inputs and all generated/private data.
- Reproduction: `python3 tools/validate.py --game base`. Actual outputs are in `private/testing/validation.json`. Optional `tools/classify_survey.py` uses private Capstone to classify first blockers per indexed function; those counts are not execution frequencies.

## Input investigation retained

205,781 RomFS files / 23,278,867,417 bytes were inventoried, including 121 listings. All 205,787 listed entries match presence/sizes. Deep payload inspection remains sampled/bounded. Retail version and complete retail content remain unverified; SDK markers are not a retail version mapping. Original inputs were not modified. No original-hardware oracle, Vulkan/SDL3, RADV, audio/input or gameplay validation exists.

## Milestones

| Phase | Classification | Remaining work |
|---|---|---|
| 0 investigation | Partially implemented | Retail version/completeness, graphics/resource analysis |
| 1 foundation | Partially implemented | Synthetic native/Rust bridges pass; ISA and runtime semantics incomplete |
| 2 game CPU translation | Partially implemented | 212,303 subset-liftable indexed extents with possible unbound tails/profiles; 58 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

SIMD permutation, pairwise FP and min/max decoder/emitter/helper paths; mandatory and differential native tests; shared SIMD-reference classification and private QEMU working directories; supplied-binary feature selection; decoder cross-check; STATUS/CPU/architecture/compatibility/research documentation. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), LD1 (290), LD1R (216), ST1 (169), FCVTZS (118), undecoded (118), FCMGE (110), LD2 (92), TBL (92), FRINTM (79), SVC (79) and CNT (68). Counts are first blockers, not dynamic frequencies. Private classification finds many single-lane S LD1/ST1 operations and single-register replicated loads, making checked lane-preserving loads/stores and broadcasts the next concrete memory dependencies. Remaining compare masks, table lookup/popcount, scaled conversion and rounding also need validated semantics. UDF/undecoded words require extent/CFG investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

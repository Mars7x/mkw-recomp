# Status — October 27, 2026

Checked single-register SIMD lane loads/stores and replicated loads are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added single-register B/H/S/D LD1/ST1 lane transfers and 64/128-bit LD1R replication. Lane loads preserve every unselected bit, including the upper half; replicated loads clear unused upper bits. Register 31 is a vector register, while base register 31 selects guest SP.
- Added base-only, immediate post-index and register post-index forms. Address/source/index values are captured before checked callbacks; destination/base writeback commit only on success. Tests cover a post-index register aliasing the base, unsigned writeback wrap, unaligned normal-memory access, missing callbacks, short mappings and read-only store faults without partial register/memory effects.
- Explicit typed IR retains byte size, lane, replication width and writeback source. Multi-register structure forms, reserved combinations and invalid nonpost register fields reject with the original word retained in diagnostics. No ABI v7 layout/signature change or host memory pointer exposure was needed.
- Native independent byte-array expectations check 3,468 successful cases per optimization plus fault/refusal cases. Separate synthetic QEMU fixtures compare 1,088 cases across all legal element lanes, replication arrangements, SP/vector-31 and supported post-index forms at O0/O2 with fatal UBSan. No game instruction stream executes in QEMU.
- Supplied-binary metadata marks the required plain memory callback and selects lane-memory and replicated-memory representative objects. The preceding SIMD/minmax milestone is published as `09131f7`. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **212,753 indexed extents**, including **125,831 with PAC operations**, up from 212,303 under the same tail policy. **371 accepted extents contain lane-memory operations and 93 contain replicated loads** (categories overlap). **58,377 contain direct extent tails; 14,037 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Fifty-nine representative game-derived samples compile into native objects, 42 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **114 Rust tests pass**, including lane-memory preservation, replication, writeback, fault handling and earlier SIMD/FP/CRC/conversion semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **1,088 single-structure memory fixtures** match QEMU at O0/O2. Independent native byte-array cases additionally cover unsigned post-index wrap, aliasing, faults and complete memory/register preservation.
- **210 permutation fixtures** match QEMU at O0/O2; independent native lane-sequence tests cover 8,064 cases per optimization, including aliasing and upper clearing.
- **8,768 S/D format-conversion fixtures** and **40 CRC fixtures** match QEMU at O0/O2. Native CRC tests independently use polynomial division and standard string check values.
- **43,232 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **26,416 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **96,042 constructed encodings**, including **48,122 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 212,753 subset-liftable indexed extents with possible unbound tails/profiles; 59 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Single-structure typed IR/decoder and native emitter; mandatory byte-array/fault tests and synthetic AArch64 memory-reference ELF builder; supplied-binary feature/sample selection; encoding cross-check; STATUS/CPU/architecture/compatibility/research documentation. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), FCVTZS (120), FCMGE (120), undecoded (118), TBL (115), LD2 (95), FCMGT (90), FRINTM (80), SVC (79), FCMEQ (78), CNT (68) and ST2 (63). Counts are first blockers, not dynamic frequencies. Next add validated comparison-mask operations, table lookup/popcount, scaled conversion/rounding and multi-register structure transfers. Single-register structure support does not imply LD2/ST2 or larger transaction support. UDF/undecoded words require extent/CFG investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, SP/access-trap policies, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

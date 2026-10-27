# Status — October 27, 2026

S/D integral floating rounding is implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added scalar and vector FRINTN/P/M/Z/A/X/I for legal S/D arrangements. N/P/M/Z/A use encoded rounding directions; X/I use FPCR rounding. Only X raises new inexact flags; other forms suppress IXC while retaining signaling-NaN IOC behavior and sticky FPSR.
- Added two unchanged f32/f64 roundToInt sources from the existing pinned BSD SoftFloat revision, verified against exact Git blobs and recorded SHA-256 provenance. Original adapter glue scopes helper state and applies output default-NaN processing. ABI v7 layout/signatures remain unchanged.
- Mandatory host-rounding/NaN-bit oracles cover 45,920 cases per native optimization, including signed zeros, fractions/ties, subnormals, infinities, quiet/signaling NaNs, all rounding/default-NaN controls, V31, aliases, upper clearing, state preservation and transactional refusal. A forced later-lane callback failure verifies no partial vector/FPSR commit.
- Separate independently authored QEMU fixtures compare 22,960 scalar/vector cases at O0/O2 with fatal UBSan. Enabled traps, flush/alternative/merge profiles, half precision and newer constrained-integer FRINT forms remain unsupported.
- The survey marks floating-helper requirements and rounding examples. Integer SIMD arithmetic/reductions are published as `42ea29f`. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **213,188 indexed extents**, including **126,171 with PAC operations**, up from 213,058 under the same tail policy. **130 accepted extents contain integral FP rounding** (categories overlap). **58,406 contain direct extent tails; 14,067 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Sixty-six representative game-derived samples compile into native objects, 48 with PAC; none execute.**

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **126 Rust tests pass**, including integral-rounding direction/NaN/flag/refusal cases and earlier integer SIMD/table/count/comparison/lane-memory/FP/CRC/conversion semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **22,960 integral-rounding fixtures** match QEMU at O0/O2. Independent native rounding/NaN/flag expectations cover 45,920 cases per optimization, including FRINTX inexact behavior and suppressed IXC in other forms.
- **17,568 integer SIMD fixtures** match QEMU at O0/O2. Independent i128/u128 native oracles check 28,992 cases per optimization, including all-lane extrema, aliases, wrap and widened sums.
- **120 table/count fixtures** match QEMU at O0/O2. Independent native byte-sequence/popcount tests cover 34,816 cases per optimization, including every byte/index value, table wrapping, aliases and upper clearing.
- **16,800 SIMD FP comparison fixtures** match QEMU at O0/O2 across S/D, scalar/vector arrangements, aliases, NaNs, signed zeros, controls and compare-zero forms. Mandatory native tests independently compare ordered inputs and NaN/exception expectations.
- **1,088 single-structure memory fixtures** match QEMU at O0/O2. Independent native byte-array cases additionally cover unsigned post-index wrap, aliasing, faults and complete memory/register preservation.
- **210 permutation fixtures** match QEMU at O0/O2; independent native lane-sequence tests cover 8,064 cases per optimization, including aliasing and upper clearing.
- **8,768 S/D format-conversion fixtures** and **40 CRC fixtures** match QEMU at O0/O2. Native CRC tests independently use polynomial division and standard string check values.
- **43,232 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **26,416 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **97,756 constructed encodings**, including **48,934 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 213,188 subset-liftable indexed extents with possible unbound tails/profiles; 66 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Scalar/vector rounding decode and original helper glue; two pinned BSD sources/provenance; mandatory/differential rounding tests; helper-requirement/sample metadata; encoding cross-check; STATUS/CPU/architecture/compatibility/research documentation. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), FCVTZS (120), undecoded (120), LD2 (99), SVC (79), ST2 (64), MVN (60), LDAXR (60), REV64 (58), FMAXP (53), SHL (50) and LD4 (49). Counts are first blockers, not dynamic frequencies. Private classification confirms the FCVTZS blockers are scaled fixed-point forms (mostly W-from-S). Next add validated scaled conversion, remaining raw SIMD transforms/shifts, pairwise FP min/max and multi-register structure transfers. Exclusive monitors need a defined concurrency model; single-register memory support does not imply larger transactions. UDF/undecoded words and unresolved CFG edges require investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, SP/access-trap policies, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

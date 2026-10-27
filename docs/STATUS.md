# Status — October 27, 2026

Raw XTN/XTN2 narrowing and four-S floating min/max reductions are implemented and tested; automatic source-only publication and periodic continuations remain active. **No game-derived function has executed. Mario Kart World has not booted, rendered or become playable.**

## Latest implementation

- Added XTN/XTN2 raw B/H/S narrowing from doubled-width elements. XTN clears the destination upper half; XTN2 retains its lower half. Complete snapshots preserve aliases/V31; saturating/QC forms remain separate.
- Added FMAXV/FMINV/FMAXNMV/FMINNMV reductions over four S elements, using adjacent-pair tree order and existing checked min/max helpers. Destination/FPSR commit only after all three calls succeed; mandatory tests cover NaN/signed-zero selection, V31 and second/final-stage refusal.
- Independent byte selection checks 6,144 native narrowing cases per optimization; 24 QEMU fixtures check arrangements/source aliases. The expanded SIMD numeric matrix directly compares 86,304 QEMU cases at native O0/O2 with fatal UBSan. A separate 32,768-pair composition probe supports reduction order. ABI v7 and upstream sources are unchanged.
- A newly accepted 51,516-line generated sample spent over eight minutes in the compiler. Representative object selection now caps candidates at 4,096 lifted instructions, records oversized deferrals and seeks later smaller examples, without changing the full lifting census. An independent gate checks sample source hashes, metadata counts, native relocatable ELF headers and feature accounting; synthetic negatives reject four false evidence claims.
- Supplied-binary metadata selects narrowing/reduction samples. Previous integer min/max/SHLL work is published as `ead9810`. README, game inputs/generated code, credentials and private artifacts remain excluded from GitHub.

## Supplied-binary evidence

All **223,856 indexed CIE/FDE extents** validate. The survey lowers **213,755 indexed extents**, including **126,609 with PAC operations**, up from 213,673 under the same tail policy. **55 accepted extents contain raw narrowing and 39 contain floating min/max reductions** (categories overlap). **58,433 contain direct extent tails; 14,120 contain indirect BR** (categories overlap). Configuration/helpers/callees/targets remain incomplete or unbound; these are not runnable-function or full-module counts. **Eighty representative game-derived samples compile into native objects, 54 with PAC; none execute.**

Native samples now have a 4,096-instruction representative limit. One oversized candidate is explicitly deferred; every observed feature still has a compiled representative in its module. These object counts use the new sampling policy and are not directly comparable with prior first-feature selections.

The existing exact-version diagnostic still applies and reads back **309,844 relocations** across four modules / 79,721,040 mapped bytes, with 26,110 named symbol bindings. Symbol search uses validated SONAME dependencies and an explicit process policy inferred statically from the supplied rtld. Actual retail address layout/startup has not been reproduced.

## Validation

- **143 Rust tests pass**, including raw narrowing and floating reduction byte/numeric/refusal checks and earlier scaled conversion/rounding/integer SIMD/FP/memory semantics, plus all prior native thread-register, bit/carry, conversion, FP, SIMD, LSE, PAC and control-flow regressions.
- **24 raw narrowing fixtures** match QEMU at O0/O2; independent byte-array expectations check 6,144 native cases per optimization, including retained/cleared halves and V31 aliases.
- **64 transform and 34,320 immediate/widening shift fixtures** match QEMU at O0/O2. Independent native oracles cover 16,384 and 68,640 cases per optimization respectively, including all legal amounts, aliases, full-width shifts and upper clearing.
- **327,680 scaled-conversion fixtures** match QEMU at O0/O2 and independent native rational expectations. A separate 98,304-case helper matrix checks exact arithmetic and flags per optimization; malformed-operation and AOT refusal/zero-register tests remain mandatory.
- **22,960 integral-rounding fixtures** match QEMU at O0/O2. Independent native rounding/NaN/flag expectations cover 45,920 cases per optimization, including FRINTX inexact behavior and suppressed IXC in other forms.
- **24,096 integer SIMD fixtures** match QEMU at O0/O2. Independent i128/u128 native oracles check 39,744 cases per optimization, including all-lane extrema, aliases, wrap and widened sums.
- **120 table/count fixtures** match QEMU at O0/O2. Independent native byte-sequence/popcount tests cover 34,816 cases per optimization, including every byte/index value, table wrapping, aliases and upper clearing.
- **16,800 SIMD FP comparison fixtures** match QEMU at O0/O2 across S/D, scalar/vector arrangements, aliases, NaNs, signed zeros, controls and compare-zero forms. Mandatory native tests independently compare ordered inputs and NaN/exception expectations.
- **1,088 single-structure memory fixtures** match QEMU at O0/O2. Independent native byte-array cases additionally cover unsigned post-index wrap, aliasing, faults and complete memory/register preservation.
- **210 permutation fixtures** match QEMU at O0/O2; independent native lane-sequence tests cover 8,064 cases per optimization, including aliasing and upper clearing.
- **8,768 S/D format-conversion fixtures** and **40 CRC fixtures** match QEMU at O0/O2. Native CRC tests independently use polynomial division and standard string check values.
- **86,304 SIMD numeric fixtures** match QEMU at O0/O2, covering lane selection/arrangements, overlapping sources/destinations, rounding/default-NaN controls and sticky FPSR.
- **7,232 conversion fixtures** match QEMU 8.2.2 at native O0/O2 with fatal UBSan, checking vector/general-register results, FPSR, NZCV and FPCR across W/X and S/D, controls, integer boundaries, fractions, signed zeros, infinities and quiet/signaling NaNs.
- The expanded **26,416 FP/selection fixtures** also match QEMU at O0/O2. Earlier **2,741 SIMD**, **160 LSE** and **4,096 QARMA5** software comparisons remain passing. QEMU shares broad SoftFloat lineage and is not a hardware oracle; optional unavailability is explicitly recorded.
- **119,268 constructed encodings**, including **56,729 accepted encodings**, pass private Capstone family/width/immediate checks. This checks decoding, not runtime semantics or hardware behavior.
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
| 2 game CPU translation | Partially implemented | 213,755 subset-liftable indexed extents with possible unbound tails/profiles; 80 objects; no complete module |
| 3 runtime bootstrap | Not started | Load-time linking passes; no actual game execution/startup |
| 4–8 graphics through packaging | Not started | No genuine game frame, native input/audio or gameplay |

## Files changed this session

Raw narrowing IR/decoder/emitter and byte/QEMU tests; floating min/max reduction IR/decoder/tree emitter and expanded semantic/refusal tests; survey feature/sample selection and explicit representative instruction budget; independent native-sample evidence gate integrated into validation; encoding cross-check; STATUS/CPU/architecture/compatibility/research documentation. Upstream sources and ABI layout/signatures are unchanged. Original inputs remain unchanged and all game-derived outputs stay private.

## Next concrete dependencies

The leading first blockers now include UDF (457 extents), undecoded (122), LD2 (103), SVC (79), ST2 (65), LDAXR (60), LD4 (56), FRECPE (29), SMULL (29), MRS (28), FRSQRTE (26) and LD1 (25). Counts are first blockers, not dynamic frequencies. Next add validated widening multiplication/accumulation and multi-register structure transfers. Reciprocal estimates need exact architectural approximation/exception semantics. Multi-register transfers can exceed the current 32-byte memory callback limit and need a deliberate transactional design. Exclusive monitors need a defined concurrency model; single-register memory support does not imply larger transactions. UDF/undecoded words and unresolved CFG edges require investigation; SVC requires actual service ABI work. Target cache/counter/PAC/FP/address configuration, SP/access-trap policies, TLS templates/allocation, actual guest threading, indirect binding and bootstrap/services remain gates before original execution.

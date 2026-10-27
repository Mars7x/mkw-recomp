# Coverage-driven work selection

The acceptance metric remains the strict `cpu-survey` result over the same 223,856 indexed extents. No CFG boundary, tail, size, identity or semantic test rule was relaxed. Counts are static lowering evidence; they do not establish executable modules or game startup.

## Baseline and complete blocker inventory

With the already implemented AES changes, 214,198 extents pass and 9,658 reject. `cpu-blockers` re-runs the exact strict baseline, then diagnoses every rejected extent separately. All 4,969,840 bytes in rejected spans were inventoried; no extent exceeded the existing 1 MiB bound or was omitted.

The diagnostic CFG continues after decode failures assuming ordinary fallthrough, follows supported control flow and preserves the strict direct-tail/boundary rules. A separate complete linear scan records potential blockers in unreachable code/data. Linear-only words are not promoted into acceptance requirements. Unsupported control-flow recovery remains incomplete, so predictions require a real strict re-survey after implementation.

The original strict result has 8,548 CFG-boundary failures. Diagnostic continuation finds edge issues in 9,132 rejected extents; this larger number includes fallthrough after unknown/trap/data words and is conditional on that recovery assumption. Only 350 extents have a nonstructural requirement set containing solely candidate ISA work. Unsupported runtime/configuration/opaque/policy requirements occur in 762 rejected extents, overlapping the edge-blocked group.

## Priorities at the AES baseline

The initial ranking evaluated individual fixes and all pairs/triples of candidate families; the current tool additionally evaluates quadruples and complete larger observed requirement sets. Predicted gain counts a rejected extent only when the complete diagnostic CFG blocker set is covered and no structural issue remains. Engineering effort is an explicit estimate, with a separate assumed 15-minute validation gate; ranges are not measured implementation times. Shared-family savings are stated in the tool. Cache/system-register, syscall, exclusive-monitor, exception, opaque and unsupported control-flow work is identified separately rather than assigned a cheap ISA score.

| Fix or small combination | Conditional predicted gain | Implementation estimate | Observation |
|---|---:|---:|---|
| PRFM | 225 | 10–25 min | Highest immediate gain per effort; all 225 also corroborated by the linear span inventory |
| PRFM + PRFUM | 228 | 15–35 min | Same hint family; consider together if marginal decoder/test cost is small |
| SHADD + SHSUB | 9 | 30–75 min | Either alone gains zero; the combination removes both requirements |
| FRECPE + FRECPS | 34 | 150–300 min | Approximation/rounding/exception semantics make this substantially harder |
| XPACI | 5 | 20–45 min | Seven occurrences are not seven unlocked extents |
| MLA | 4 | 20–45 min | Other occurrences remain blocked by additional requirements |
| FRSQRTE + FRSQRTS | 24 | 210–420 min | Single estimate instruction alone unlocks zero |

The complete private rankings contain additional combinations, instruction counts, per-extent requirements and span corroboration. Priorities are recomputed after each committed implementation; predicted gains are never substituted for measured strict gains.

## Runtime ceiling and diminishing returns

Prefetch accounts for 228 of the 350 presently identified pure-ISA/nonstructural opportunities. After that family, most remaining rejected extents still require CFG/runtime/configuration work. For example, 61 exclusive-monitor loops require LDAXR/STLXR together, with CLREX also needed in 52; 79 SVC extents need actual service ABI work. SHA1 helpers require a six-operation bundle rather than a useful single-op patch. A further rise in static lowering counts will not bind callees, establish a guest scheduler/counter/PAC profile, or provide OS/bootstrap/graphics services.

This is a material limit on the usefulness of further small ISA survey wins. Boundary policy remains unchanged; unsupported operations are not turned into successful stubs. Runtime contracts and target configuration must be established before original execution.

## Reproduction and evidence

- `target/release/mkw-recomp cpu-survey base` retains the original acceptance census.
- `target/release/mkw-recomp cpu-blockers base` writes the independent diagnostic inventory.
- `python3 tools/rank_blockers.py` uses optional private Capstone to classify requirements and rank fixes; it refuses a mismatched strict baseline.
- `python3 tools/rank_blockers.py --self-test` checks singleton, combination and runtime-exclusion accounting.
- `python3 tools/validate.py --game base` retains all prior correctness/negative gates and additionally checks the diagnostics/ranking. Each gate now records elapsed seconds privately.

All detailed binary-derived inventories remain private. Published documentation contains aggregate evidence and original tool/source code only.


## Measured prefetch result and refreshed priorities

The same strict survey now accepts **214,426 extents: +228**, exactly matching the PRFM/PRFUM prediction. The accepted sets were compared by module/extent identity: no previously accepted extent was lost. All 228 additions contain typed prefetch hints. SP-based forms remain rejected under the existing negative test, and none of the predicted additions required them. Tests, CFG/tail/identity/size rules and representative limits are unchanged.

The remaining 9,430 rejects contain 4,588,952 inventoried bytes, with zero omissions. Strict CFG-boundary failures remain 8,548; optimistic diagnostic edge-blocked extents remain 9,132. Pure ISA/nonstructural opportunities fall to 122. Runtime/configuration/opaque/policy barriers still occur in 762 extents (overlapping structural barriers).

| Refreshed fix combination | Conditional predicted gain | Implementation estimate | Estimated gain/hour including 15-minute validation |
|---|---:|---:|---:|
| FRECPE + FRECPS | 40 | 150–300 min | 7.62–14.55 |
| FRECPE + FRECPS + XPACI | 45 | 170–345 min | 7.50–14.59 |
| SHADD + SHSUB + UHADD | 11 | 30–75 min | 7.33–14.67 |
| FRECPE + FRECPS + MLA | 44 | 170–345 min | 7.33–14.27 |

The first package leads by midpoint estimate only; these wide ranges overlap. Its approximation, exceptional-input and rounding requirements must be assessed before committing engineering time. The preceding complete gate actually took 16.91 minutes, close to the explicit 15-minute ranking assumption; no validation work is omitted to improve throughput.

Prefetch implementation start through isolated verification and strict supplied-binary measurement took 28.15 minutes (**486 gained extents/hour**), excluding upfront shared portfolio analysis and the final publication gate. A private effort receipt records timestamps and the baseline commit. Publication completion is recorded after push, avoiding invented historical timings. These diagnostic gains do not remove the runtime ceiling described above.


## Feasibility correction under fixed tests

The preserved `vector_compare::tests::comparison_relations_scalar_zero_and_reserved_forms` test requires one valid 4S FRECPE encoding to reject. Keeping the entire 4S arrangement unsupported avoids a register-specific exception. The ranker now classifies that arrangement as a fixed-test policy constraint, not an architecturally reserved instruction; valid 2S FRECPE work is ranked separately. Reciprocal implementation remains deferred and private.

This reduces the FRECPE/FRECPS package from 40 to **16 feasible conditional gains** (15 from 2S FRECPE alone). Before the halving implementation, feasible pure ISA/nonstructural opportunities were **97**, with 25 additional extents classified as policy constrained. SHADD+SHSUB+UHADD consequently became the highest-value feasible package: **11 predicted gains / 30–75 engineering minutes**, plus the unchanged validation gate. This revision illustrates why effort and feasibility estimates must be updated after inspecting the actual dependencies.

Prefetch was validated and pushed as `f752c08`: **+228**, total **214,426**. Implementation start through verified publication took **43.90 minutes**, or **311.62 gained extents/hour**, excluding upfront shared blocker analysis. The complete publication gate took **14.70 minutes**; all 207 Rust tests and required supplied-input/negative checks passed.


## Measured halving result

SHADD+SHSUB+UHADD produces **+11**, exactly the feasible package prediction: **214,437 accepted extents**, with no prior acceptance lost. The remaining **9,419** rejected spans contain **4,576,276 bytes**, all scanned with zero omissions. Strict boundary failures stay at **8,548**; diagnostic edge-blocked extents stay at **9,132**. Feasible pure ISA/nonstructural opportunities fall to **86**. Runtime/configuration/opaque/policy barriers occur in **787** extents, overlapping structural barriers and including the newly identified fixed-test constraint.

| Next feasible package | Conditional predicted gain | Implementation estimate | Gain/hour including 15-minute validation |
|---|---:|---:|---:|
| MLA + XPACI | 9 | 40–90 min | 5.14–9.82 |
| 2S FRECPE + MLA + XPACI | 24 | 130–270 min | 5.05–9.93 |
| 2S FRECPE + XPACI | 20 | 110–225 min | 5.00–9.60 |
| CLZ + MLA + XPACI | 12 | 60–135 min | 4.80–9.60 |

The expected return is now dramatically lower than prefetch's measured 311.62 extents/hour. These static wins do not bind runtime services or make the accepted extents runnable. The next package retains the same correctness/acceptance gates and explicit PAC configuration; original execution remains blocked before bootstrap.


## Measured MLA/XPACI result

The highest feasible package produces **+9**, exactly its prediction: **214,446 accepted extents**, with no previous acceptance lost. Four accepted extents contain MLA and five contain register pointer stripping. Feature-specific native sampling compiles the new register transport in every module where observed; all game-derived objects remain unexecuted.

All **9,410 rejects / 4,558,560 bytes** are inventoried with zero omissions. Strict first-boundary failures rise from 8,548 to **8,550** as two removed ISA blockers reveal later edges. Diagnostic structural barriers stay **9,132**, and the feasible pure-ISA/nonstructural pool falls to **77**. This is direct evidence that first-failure category counts do not measure either complete blocker requirements or lost coverage.

| Next feasible fix | Conditional predicted gain | Implementation estimate | Gain/hour including 15-minute validation |
|---|---:|---:|---:|
| 2S FRECPE | 15 | 90–180 min | 4.62–8.57 |
| 2S FRECPE + CLZ | 18 | 110–225 min | 4.50–8.64 |
| 2S FRECPE + FRSQRTE + FRSQRTS | 39 | 300–600 min | 3.80–7.43 |

A 2S-only estimate prototype now passes privately, reusing the earlier reciprocal assessment; final implementation effort can be lower than the original from-scratch band. The source is not yet integrated and its gain still needs measurement. The original global 4S negative test is preserved.

Halving commit `481eb98` measured **+11 / 29.14 wall minutes = 22.65 extents/hour**, including overlapping publication work and a 14.72-minute full gate. Prefetch's measured end-to-end rate was 311.62/hour. These times describe explicitly scoped wall intervals, not additive engineer-hours. The existing runtime ceiling and the observed PAC/TBI profile difference limit the practical value of chasing the last small static counts.


## Measured 2S estimate/CLZ combination

The small combination produces **+18**, exactly its prediction: **214,464 accepted extents**, with no prior acceptance lost. Fifteen additions contain 2S FRECPE and three contain 2S SIMD CLZ. The original global 4S rejection test and every prior test/acceptance rule remain unchanged. The completed estimate prototype reused earlier assessment work; the narrowly scoped CLZ prototype took 2.06 wall minutes through checks while the preceding publication ran. Sharing one full gate improves the return of this already-tested combination relative to two separate publications. Initial from-scratch effort bands are retained as historical forecasts, not asserted actual times.

All **9,392 rejects / 4,400,316 bytes** are inventoried with zero omissions. Strict first-boundary failures remain **8,550**, diagnostic structural barriers **9,132**, runtime/configuration/opaque/policy barriers **787**, and feasible pure-ISA/nonstructural opportunities fall to **59**. Three oversized native-sample candidates are explicitly deferred under the unchanged 4,096-instruction representative limit; every observed feature still compiles in its module. This does not change the acceptance census.

The next highest package is **FRSQRTE+FRSQRTS**, predicting **24** gains with a 210–420-minute from-scratch engineering band plus the validation gate (**3.31–6.40/hour**). Appending other listed singles currently gains nothing additional and costs more. Correct estimate/refinement rounding and exceptions are concrete dependencies; a larger accepted count still cannot establish the missing runtime/configuration contracts.

MLA/XPACI commit `99c4358` measured **+9**, with a scoped 41.88-minute implementation-to-publication interval (**12.89/hour**) and a 15.13-minute complete gate. Prior test files and acceptance rules remained unchanged. These overlapping wall intervals are explicitly scoped and must not be added as engineer-hours.


## Measured reciprocal-square-root result and refined next costs

FRSQRTE+FRSQRTS produces **+24**, exactly the complete-set prediction: **214,488 accepted extents**, with no prior acceptance lost. All **9,368 rejects / 4,373,452 bytes** are scanned with zero omissions. One further removed ISA failure reveals a later strict boundary, giving **8,551** first-boundary failures; diagnostic structural barriers remain **9,132**. Only **35** feasible pure ISA/nonstructural opportunities remain.

Inspection of every remaining blocker shows SQSHL uses the 4S **immediate** form, signed pairwise min/max use 2S, and SADDLP uses ordinary long pair sums. Existing arithmetic and independent fixtures permit narrower bands than the previous 60–180-minute unknown-family defaults: SQSHL 5–15, each min/max 4–10 (together 5–12), SADDLP 6–18 minutes. These are scoped forecasts with explicit sharing, not reductions in validation or claims of actual elapsed implementation.

| Next small combination | Conditional gain | Engineering estimate | Gain/hour including unchanged 15-minute gate |
|---|---:|---:|---:|
| SQSHL + SMINP + SMAXP + SADDLP | 11 | 16–45 min | 11.00–21.29 |
| SQSHL + SMINP + SMAXP | 8 | 10–27 min | 11.43–19.20 |
| SQSHL + SMINP + SADDLP | 10 | 15–43 min | 10.34–20.00 |

The four-fix combination leads by midpoint estimate only; intervals overlap. Quadruples and complete observed bundles now appear in the ranking, including larger SHA dependency sets that no single fix unlocks. Three new synthetic checks enforce four-fix synergy, full six-fix accounting and runtime exclusion; original self-tests remain unchanged. Strict acceptance and semantic gates are unchanged.

Commit `3545b9c` measured **+18**, all 229 Rust tests passing, with a 15.31-minute complete gate. The verified repository push timestamp gives **52.59 minutes / 20.54 extents per hour** over the final scoped estimate/CLZ interval; including its separately recorded earlier 7.66-minute reciprocal assessment gives **17.93/hour**. These scoped, overlapping intervals are not additive engineer-hours. The runtime ceiling remains material despite faster arithmetic prototypes.


## Measured integer combination and diminishing static returns

The four-fix package produces **+11**, exactly predicted: **214,499 accepted extents**, no previous acceptance lost. Gains comprise four 4S SQSHL immediate, three 2S SMINP, one 2S SMAXP and three signed long-pair-sum extents. All **9,357 rejects / 4,341,048 bytes** are inventoried with zero omissions. Strict first-boundary failures remain 8,551, diagnostic structural barriers 9,132, and runtime/configuration/opaque/policy barriers 787 (overlapping counts). The pure ISA/nonstructural pool falls to **24**. Every observed feature compiles in its module; 121 bounded game-derived objects remain unexecuted.

The isolated implementation and correctness interval took **12.07 wall minutes**, below the scoped 16–45-minute forecast; full publication still requires the unchanged gate. Current generic rankings favor FCVT (two conditional gains / 60–180 engineering minutes), complete SHA1 (five / 200–450), complete SHA256 (four / 180–385), then combinations including polynomial multiply. These broad defaults need operand-form inspection before choosing the next implementation. Whole requirement sets, not first-error frequency, determine all estimates.

Commit `1a5195a` measured **+24 / 63.86 scoped wall minutes = 22.55 extents/hour**, including the **15.60-minute** gate; all 235 Rust tests passed. Timings overlap preceding publication and are not additive engineer-hours. Runtime/configuration and CFG work now has substantially more practical value than chasing the last 24 static opportunities. Further ISA work must retain this explicit ceiling and leave original execution gated.

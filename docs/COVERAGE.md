# Coverage-driven work selection

The acceptance metric remains the strict `cpu-survey` result over the same 223,856 indexed extents. No CFG boundary, tail, size, identity or semantic test rule was relaxed. Counts are static lowering evidence; they do not establish executable modules or game startup.

## Baseline and complete blocker inventory

With the already implemented AES changes, 214,198 extents pass and 9,658 reject. `cpu-blockers` re-runs the exact strict baseline, then diagnoses every rejected extent separately. All 4,969,840 bytes in rejected spans were inventoried; no extent exceeded the existing 1 MiB bound or was omitted.

The diagnostic CFG continues after decode failures assuming ordinary fallthrough, follows supported control flow and preserves the strict direct-tail/boundary rules. A separate complete linear scan records potential blockers in unreachable code/data. Linear-only words are not promoted into acceptance requirements. Unsupported control-flow recovery remains incomplete, so predictions require a real strict re-survey after implementation.

The original strict result has 8,548 CFG-boundary failures. Diagnostic continuation finds edge issues in 9,132 rejected extents; this larger number includes fallthrough after unknown/trap/data words and is conditional on that recovery assumption. Only 350 extents have a nonstructural requirement set containing solely candidate ISA work. Unsupported runtime/configuration/opaque/policy requirements occur in 762 rejected extents, overlapping the edge-blocked group.

## Priorities at the AES baseline

The ranker evaluates individual fixes and all pairs/triples of candidate families. Predicted gain counts a rejected extent only when the complete diagnostic CFG blocker set is covered and no structural issue remains. Engineering effort is an explicit estimate, with a separate assumed 15-minute validation gate; ranges are not measured implementation times. Shared-family savings are stated in the tool. Cache/system-register, syscall, exclusive-monitor, exception, opaque and unsupported control-flow work is identified separately rather than assigned a cheap ISA score.

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

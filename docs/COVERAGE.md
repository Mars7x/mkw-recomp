# Coverage-driven work selection

The acceptance metric remains the strict `cpu-survey` result over the same 223,856 indexed extents. No CFG boundary, tail, size, identity or semantic test rule was relaxed. Counts are static lowering evidence; they do not establish executable modules or game startup.

## Baseline and complete blocker inventory

With the already implemented AES changes, 214,198 extents pass and 9,658 reject. `cpu-blockers` re-runs the exact strict baseline, then diagnoses every rejected extent separately. All 4,969,840 bytes in rejected spans were inventoried; no extent exceeded the existing 1 MiB bound or was omitted.

The diagnostic CFG continues after decode failures assuming ordinary fallthrough, follows supported control flow and preserves the strict direct-tail/boundary rules. A separate complete linear scan records potential blockers in unreachable code/data. Linear-only words are not promoted into acceptance requirements. Unsupported control-flow recovery remains incomplete, so predictions require a real strict re-survey after implementation.

The original strict result has 8,548 CFG-boundary failures. Diagnostic continuation finds edge issues in 9,132 rejected extents; this larger number includes fallthrough after unknown/trap/data words and is conditional on that recovery assumption. Only 350 extents have a nonstructural requirement set containing solely candidate ISA work. Unsupported runtime/configuration/opaque/policy requirements occur in 762 rejected extents, overlapping the edge-blocked group.

## Priorities

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

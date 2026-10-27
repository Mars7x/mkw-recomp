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


## Measured polynomial/widening/difference package

Precise operand inspection finds 64-bit PMULL/PMULL2, S-to-D FCVTL and B/H/S unsigned differences. Half-precision FCVT instead needs new backend support and retains a higher forecast. Original loops and existing conversion callbacks reduce scoped engineering estimates: PMULL/PMULL2 together 8–20 minutes, UABD/UABDL 6–15, FCVTL 3–8. The five-fix combination predicts **six gains / 17–43 engineering minutes plus the unchanged 15-minute gate (6.21–11.25/hour)**; polynomial/difference alone predicts five / 14–35 plus the gate (6.00–10.34/hour). These are forecasts based on scoped work, not reduced testing.

The isolated implementation/check interval takes **4.82 wall minutes**. Strict measurement confirms **+6**, total **214,505**, with no earlier acceptance lost: three polynomial extents, two requiring both unsigned differences, and one S-to-D widening extent. The survey compiles 122 bounded objects, all unexecuted. Complete diagnostics scan **9,351 rejects / 4,338,280 bytes**, zero omissions; strict first-boundary failures stay 8,551 and diagnostic structural barriers 9,132. The pure ISA pool falls to **18**, with 787 runtime/configuration/opaque/policy barriers (overlapping structural counts).

Rankings now enumerate all five-fix combinations in addition to complete larger observed sets. Two additional synthetic tests check shared validation and monotone family costs; previous tests remain unchanged. Complete SHA1 (five conditional gains) and SHA256 (four) lead the remaining generic forecasts, ahead of half-precision conversions. Their actual scoped costs still need investigation. Runtime/CFG dependencies increasingly limit the practical value of further survey work; these counts do not make original functions runnable.

Commit `e3997c1` measured **+11**, all 239 Rust tests passing. Its scoped implementation-to-verified-push interval is **53.34 minutes / 12.37 extents per hour**, including a **15.98-minute** full gate. This interval overlaps prior publication and is not additive engineer-hours.


## Measured complete SHA bundle and corrected policy ceiling

Original lane packing, independent bit-truth arithmetic and shared complete-hash fixtures reduce the scoped SHA1 forecast to **10–23 engineering minutes**, SHA256 to **8–18**, and the combined package to **15–35**, plus the unchanged 15-minute gate. The combined nine-gain forecast gives **10.80–18.00 extents/hour**, ahead of either family alone. Ranking pairwise unions of complete feasible requirement sets exposes this combination without limiting large coherent sets to a fixed instruction count. Two new tests enforce complete accounting, shared validation and monotone costs; old tests are unchanged.

Strict measurement confirms **+9**, reaching **214,514**, with no previous acceptance lost. All **9,342 rejects / 4,328,836 bytes** are inventoried with zero omissions. First-boundary failures stay 8,551 and diagnostic structural barriers 9,132. All ten observed hash operations have compiled representatives in their modules; 123 bounded objects remain unexecuted. Sample selection combines features, so PAC-object counts need not increase monotonically.

An additional inspection finds the existing `floating::format_tests::format_conversion_directions_and_reserved_or_half_types` negatives exclude S↔H scalar conversion arrangements. The earlier two FCVT estimates were therefore infeasible under unchanged tests, not new arithmetic opportunities. Classification now excludes both full arrangements for every register pair; decoding/acceptance and all original tests remain unchanged. Together with the nine measured SHA gains, this corrects the remaining feasible ISA pool from **18 to 7**. Runtime/configuration/opaque/policy barriers rise to **789** through corrected classification, with no change to supported instructions.

The seven opportunities are two SQDMLAL/SQDMLAL2 extents; one PACIB, one FRECPS, one SQRSHL; one SADALP+SRSHL combination and one RSHRN/RSHRN2+SRSHL combination. Generic cost defaults are conservative and need scoped review. Runtime/CFG/target contracts now make further survey-only gains substantially less useful. Accepted extents still have unverified configurations and potentially unbound callees/targets; they are not executable-game progress.


## Shared SHA/refinement publication

The SHA-only survey above is an intermediate measurement, not a separate published commit. The remaining FRECPS opportunity reuses the checked fused-subtract callback and an independently adapted exact 576-bit oracle. Its scoped 2–5-minute forecast combines with SHA's 15–35 minutes: **ten predicted gains / 17–40 engineering minutes plus one unchanged 15-minute gate (10.91–18.75/hour)**. Unions of up to three complete pure requirement sets now expose this combination; a third bundle test checks it. Existing tests and all acceptance rules are unchanged.

Final strict measurement confirms **+10**, total **214,515**, with no prior acceptance lost. Nine additions contain SHA operations and one contains reciprocal refinement. Complete diagnostics scan **9,341 rejects / 4,327,996 bytes**, zero omissions. First-boundary failures remain 8,551, diagnostic structural barriers 9,132 and runtime/configuration/opaque/policy barriers 789. The feasible ISA pool falls to **six**: the preceding seven minus FRECPS. Every observed feature compiles in its module; representative selection now combines more features in **122 objects**, all unexecuted. The practical runtime/configuration/CFG ceiling remains unchanged.

Commit `27964cb` measured **+6**, all 246 Rust tests passing, in a scoped **32.41-minute implementation-to-verified-push interval / 11.11 extents per hour**, including a **16.47-minute** complete gate. This interval overlaps earlier publication and is not additive engineer-hours.


## Measured final feasible ISA sets and runtime transition

Operand inspection narrows the remaining work to register PACIB, 4S SQRSHL, 4S/2D SRSHL, 4S SADALP, RSHRN/RSHRN2 and register/indexed SQDMLAL/SQDMLAL2. Shared packing/oracles and one unchanged gate give a scoped forecast of **six gains / 13–33 engineering minutes plus 15-minute validation (7.50–12.86/hour)**. Ranking unions of up to five complete feasible sets exposes the package; two additional tests cover full sharing and zero invented gains when only runtime/policy sets remain. All previous tests and acceptance rules stay unchanged.

Strict measurement confirms **+6**, total **214,521**, no prior acceptance lost. Complete diagnostics inventory **9,335 rejects / 4,324,436 bytes**, zero omissions. Strict first-boundary failures stay 8,551; diagnostic structural barriers 9,132 and runtime/configuration/opaque/policy barriers 789 overlap. Every observed feature has a compiled representative; 125 objects remain unexecuted. **No feasible pure ISA/nonstructural requirement sets remain under the current diagnostic.** This is not complete instruction support: unsupported operations still occur with CFG, runtime, opaque and policy barriers, and unknown-control recovery remains conditional.

The new static runtime-contract inventory retains all other barriers and exposes **167 nonstructural contract-only extents**, **72 distinct SVC immediates**, and conditional gains of **64 exclusive-monitor extents / 14 physical-counter extents / 78 together**. No target service mapping, clock value, memory model or configuration is inferred from these counts. Effort is explicitly unpriced until those real contracts are specified; the tool never supplies values, executes original instructions or changes acceptance. Three synthetic checks enforce service identity, complete requirements, barrier exclusion and empty-set behavior.

Accepted extents still overlap large requirements for checked memory (165,966), call routing (161,644), PAC (127,110), ordered memory (22,543), floating helpers (11,014), LSE atomics (8,314), barriers (1,749) and thread-register configuration (674). These are static extent requirements, not execution frequencies or configured/runnable-function counts. Further survey-only ISA work now has no measured independent whole-extent return. The next concrete dependencies are a real versioned exclusive-monitor contract and configured counter access, followed by target service contracts and CFG/target resolution.

Commit `00e3acb` measured **+10**, all 256 Rust tests passing, with a **39.04-minute scoped implementation-to-verified-push interval / 15.37 extents per hour**, including a **16.63-minute** complete gate. Overlapping intervals are not additive engineer-hours.


## Exclusive-memory foundation: runtime work without inflated survey gains

The next measured dependency now has a standalone Rust implementation: owned, checked normal memory with per-context exclusive reservations, configured granule invalidation and real success/failure conditional stores. Seven unit tests and 576 synthetic QEMU sequences pass in debug and release. Plain/acquire-release cases compare values, status and memory; hardware weak-memory ordering, real target granules/traps and physical aliasing remain unverified.

**Strict gain is zero: 214,521 accepted extents remain the baseline.** Decoder/codegen/native-header sources are unchanged, and no LL/SC instruction is newly accepted. The 64 conditional monitor opportunities therefore remain predictions pending an actual versioned native binding and target profile. This distinction prevents a backend primitive from being reported as executable guest support. Existing correctness/acceptance gates remain intact; a release semantic check is additive.

Commit `93f7bcf` measured **+6**, all 265 Rust tests passing, with a **41.99-minute scoped implementation-to-verified-push interval / 8.57 extents per hour**, including a **17.13-minute** full gate. Those overlapping intervals are not additive engineer-hours. Static ISA-only return has reached zero under the current complete diagnostic; concrete runtime contracts now drive useful progress.


## Coherent native contract, unchanged default census

The next native dependency is now implemented: an explicit v10 frame/v1 table with one owned authority for plain, ordered, LSE, barrier and exclusive callbacks. Nine native C/AOT/QEMU/fault/contention tests pass in debug/release, including same-value writes and CAS-mismatch distinctions. Existing v9 layout/signatures, all old tests and default decoding remain unchanged. A separate native marker prevents the legacy loader from calling a v10 entry with the v9 function type.

**Default strict gain: +0, total 214,521.** The 64 exclusive-monitor opportunities remain conditional until capability-scoped LL/SC IR/emission and an appropriate native entry path are integrated and measured. New runtime code is not itself additional instruction acceptance. The current bounded diagnostics still have zero feasible pure ISA sets; runtime/CFG/target contracts dominate. No original function executes.

Commit `4978215` published the reservation primitive with **+0** default strict gain, all **273 Rust tests** plus the release semantic suite passing. Its complete gate took **17.65 minutes**. That phase deliberately addressed the measured runtime dependency instead of inventing new survey gains.


### Checked v10 entry dependency

The single-entry v10 loader now validates library markers, address metadata and a live owned coherent attachment before native invocation. Four new O0/O2 C/native entry/tail semantic and refusal tests pass in debug/release. **Default strict gain remains +0, 214,521 accepted extents**; all prior tests and acceptance rules are unchanged. Runtime-bound survey work still needs explicit capability-scoped LL/SC lowering and target configuration, and cross-module calls need a coherent v10 dispatcher. No runtime contract is counted as new accepted coverage.


A follow-up inventory of every existing diagnostic exclusive site finds only single-register forms: 8 LDXR, 6 STXR, 62 LDAXR, 61 STLXR and 52 CLREX sites across rejected extents. The 64 monitor-only nonstructural candidates split into 52 requiring LDAXR+STLXR+CLREX, nine requiring LDAXR+STLXR, and three requiring LDXR+STXR. Thus adding CLREX to the acquire/release pair conditionally raises its count from nine to 61, whereas any individual family gains zero. No pair-exclusive form appears in this bounded inventory. Prioritize the coherent single-register package; pair lowering can wait. These remain conditional static requirements, with unchanged strict acceptance and unverified target configuration.


### Measured explicit v10 compiler profile

The single-register package realizes the 64 conditional opportunities in an opt-in compiler profile: three extents in main and 61 in SDK. All 64 additional v10 objects compile at O2 with warnings denied, within the existing 4,096-instruction representative-object limit. The explicit profile therefore has 214,585 subset-liftable extents; default strict coverage remains 214,521, **+0 default gain**. Neither figure counts runnable functions or establishes the target memory/PAC/service/dispatch profile.

Reproduce with `target/release/mkw-recomp cpu-survey-exclusive-v10 base`. The command verifies input identity first and writes only private artifacts under `private/analysis/cpu-exclusive-v10/`. The additive validation gate compares every module's exact extent/default-accepted counts with the ordinary strict census. Existing CFG bounds, unknown-instruction refusal, old tests and object limits are unchanged. Further pure ISA inventory offers little return until target configuration, coherent cross-module routing and the remaining CFG boundaries are resolved.


The checked loader also has a host-side registered tail router. Its synthetic two-library tests establish budget progress, rebasing and reservation continuity; they add zero default survey coverage and do not supply nested call routing or target configuration. Entry/tail/scoped-lowering work shares one full validation gate to avoid paying that gate for each prerequisite separately.


A private audit of the 64 additional generated objects finds nested call sites in **60 extents (385 sites)** and tail requests in **five extents (five sites)**, with overlap. The host tail router is tested, but nested BL/BLR routing remains unbound. This measured dependency takes priority over further ISA surveys; compilation gains do not establish runnable code. Target identity/configuration and other callees remain required.


### Coherent call-routing dependency

The explicit table-v2 router binds the next measured native dependency, retaining the owned memory authority across registered direct/indirect calls and tails. Six new native tests pass in debug/release; 96 nested-call sequences per optimization match QEMU and independent value/status expectations. The v2 compiler survey compiles exactly the same 64 additional supplied extents as v1, so **new default gain is zero and new conditional compiler gain beyond v1 is zero**. The totals remain 214,521 default / 214,585 conditional. All old tests and acceptance limits remain unchanged.

Reproduce with `target/release/mkw-recomp cpu-survey-exclusive-v2 base`. Separate private artifacts live under `private/analysis/cpu-exclusive-v2/`; the mandatory gate compares module hashes, exact extents, baseline counts, object limits and deferrals with the v1 report. Game-derived code is only compiled into objects, never loaded/executed.

The complete static call inventory finds 273 direct / 112 indirect sites in the additional extents. 218 direct sites reach 62 unindexed stubs; exact X16/X17 dataflow, zero-addend JUMP_SLOT metadata and diagnostic link bindings agree for every one. Of their bound provider starts, 59 are strict-accepted and three have structural CFG failures with no remaining decode issues. These are static sites, not execution frequencies. Runtime import configuration and structural/call-target dependencies now dominate; widening FDE boundaries or more pure ISA fixes would not resolve this measured ceiling. The synthetic import-thunk fixture proves transport semantics, not recognition or acceptance of those game extents.

## Pinned upstream reuse and bounded runtime integration (October 27)

See [UPSTREAM_REUSE.md](UPSTREAM_REUSE.md) and upstream-reuse-audit.json for pinned nxnative/LASRecomp/nx2android source findings, licenses, portability costs and ranked execution/frame opportunities. The selected nxnative-derived five-API read-only filesystem profile now uses checked host targets on the coherent v10/table-v2 router. Five exact supplied provider descriptors cover ten references; a synthetic native wrapper reads 1,342 supplied RomFS bytes matching an independent stream. This is opt-in service transport, not proof of the retail SDK object layout or original game startup. Six call, four filesystem and two hook tests add to existing checks; no original code executes. Default survey gain stays zero (214,521); conditional v1/v2 totals stay 214,585. Prior acceptance and correctness checks remain required. Runtime configuration, thread/TLS ownership and Switch 2 graphics/shader identification now take priority over further survey-only work.

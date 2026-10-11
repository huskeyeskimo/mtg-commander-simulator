# Milestone 2E trigger placement contract

Rules reference: user-supplied **MagicCompRules 20260925.pdf**, titled *Magic:
The Gathering Comprehensive Rules*, effective **September 25, 2026**. Its credits
identify **Wizards of the Coast LLC** as publisher. Its introduction points to
<https://Magic.Wizards.com/Rules>. The supplied document is the authoritative
reference for this work; the URL's hosted bytes were not independently retrieved.

CR 117.5 and 704.3 require simultaneous SBA passes until stable, then trigger
placement, then repetition of that settlement cycle before priority. CR 704.4
precludes ordinary SBAs during resolution. CR 603.3 places waiting triggers on
the real stack at the next priority boundary. CR 603.3a preserves the controller
at triggering time. CR 603.3b orders placement APNAP, each controller selecting
their own order, and explicitly has two stages: triggers whose condition is not
another ability triggering, followed by the remaining triggers in APNAP order.
The engine currently represents only the first class. Supporting the second
class requires a separate placement stage; it must not be silently mixed into
this operation. No currently supported trigger requires that architectural change.

Each selection consumes one uniquely identified occurrence in the frozen window
and immediately appends its captured context to the actual stack. Later selections
are above earlier selections and resolve first. Identical-looking occurrences
remain distinct. An event group is historical context, not a placement boundary.
Newly collected occurrences cannot join an already frozen window. Leading APNAP
singleton groups need no choice and may be placed synchronously. Once a window
is suspended, even its final singleton is an explicit selection.

The operation owns settlement, controller choice and continuation. No priority,
ordinary SBA pass, resolution or phase advance occurs between its selections.
After the last selection, settlement repeats and the saved continuation resumes
once. Runtime window/occurrence counters prevent stale concrete actions; semantic
keys use fresh public graph occurrence coordinates, never those counters.

CR 603.3c–d requires modes, targets and divisions at placement when applicable;
CR 603.5 generally handles optional effects at resolution. This change preserves
represented captured effects and targets. Generic triggered modes/target selection,
trigger-of-trigger abilities and unsupported mechanics are not introduced.

Incomplete operations persist in clone, snapshot, JSON and bincode. Validation
precedes continuation and observation. Historical bincode is not compatible with
the changed state layout; serde defaults are not a compatibility promise. Solver
policy keys use an explicitly new semantic format.

Incremental action lists have n, n−1, …, 1 choices for a controller with n waiting
occurrences. This represents all n! orders without constructing a permutation list.
It does not remove factorial solver search trees, change budgets or guarantee
runtime speedups. Greedy/Goldfish/BFS retain their first-choice ordering heuristic.

## Compatibility and review evidence

The supplied PDF's SHA-256 is
`674241213075cd581dd3255567ebeec7b40ffae3a27c6a3bd99218af6b2b6659`.

`GameState` now has the leading serialized format tag `0x32454754`; untagged
historical JSON/bincode states are rejected rather than migrated implicitly.
The operation has its own version and frozen occurrence records. Its validator
checks handles, APNAP progress, saved continuation, remaining membership and the
actual placed stack prefix before observation or continuation. A terminal loss
freezes the operation without resuming it. The solver format is
`mtg-joint-public-2e-v1`; earlier policy tables must not be reused.

Canonical decisions retain a universal public occurrence description and a fresh
joint graph coordinate. Window counters and runtime occurrence handles do not
enter semantic keys. Pending captured targets and window membership are encoded;
future arrivals remain separate from frozen members. One normalization witness
serves the information set and all actions at a decision, then is recomputed for
the next prefix.

Legacy complete-order enum variants remain available for historical diagnostic
mapping, but are neither generated nor accepted by gameplay. Migrated interaction
tests translate their fixed historical orders into real one-occurrence actions;
they do not restore the old product API. New regressions exercise every prefix
at 2, 6, 7, 10 and 20 occurrences, plus controller boundaries, historical context,
restoration, hidden information, stale/invalid actions and failure disposal.

Independent Astra High review found no unresolved production or coverage findings
after corrections. Final qualification uses the unchanged AGENTS.md commands;
only `benchmark_mccfr_training_throughput` is excluded. Selection inventories
were checked for both configurations, including unchanged ignored inventories.

## Final qualification

Baseline: `56ad3ca46fff971ac0dc6e7a6b11f0bbb47fc9d3`. Toolchain:
`rustc 1.99.0 (b940084d7 2026-09-28)` and
`cargo 1.99.0 (5f94df478 2026-08-27)`.

The following final gates completed sequentially, each with exit status 0 and
zero warnings:

| Command | Result |
| --- | --- |
| `cargo test --locked -- --exact --skip benchmark_mccfr_training_throughput` | 1,171 passed; 0 failed; 7 ignored; 1 filtered |
| `cargo test --locked --all-features -- --exact --skip benchmark_mccfr_training_throughput` | 1,222 passed; 0 failed; 12 ignored; 1 filtered |
| `cargo build --locked` | Passed |
| `cargo build --locked --features tui --bin tui` | Passed |
| `cargo build --locked --all-features` | Passed |
| `cargo check --locked --all-targets` | Passed |
| `cargo check --locked --all-features --all-targets` | Passed |

Discovery compared 1,179 versus 1,178 selected tests for default features and
1,235 versus 1,234 for all features. The sole difference in each configuration
was `tests/benchmark_test.rs::benchmark_mccfr_training_throughput`. Ignored
inventories were identical before and after filtering. The other three benchmarks
and all MCCFR correctness tests remained selected and passed. Existing ignored
diagnostics, live-network tests and documentation examples remain ignored.

An initial default qualification attempt found one remaining legacy whole-order
assertion in `queued_trigger_survives_source_leaving_and_ordering`. Its correction
preserves source departure, captured history, restoration, explicit ability order
and life-gain assertions, and verifies canonical reconstruction and one actual
stack entry per selection. Astra reviewed the correction; focused trigger-context
and 2E tests passed in both configurations (27 each), then the complete default
gate was repeated successfully before the all-features gate. No production source
changed during this correction or the final gates.

Raw Cargo logs, exit statuses and target-preserving selection/result summaries are
retained in `/tmp/2e-*.log`, `/tmp/2e-*.status`,
`/tmp/2e-selection-evidence.json` and `/tmp/2e-gate-evidence.json` in this workspace.
`AGENTS.md`, benchmark source and thresholds remain unchanged. No commit or push
has been performed.

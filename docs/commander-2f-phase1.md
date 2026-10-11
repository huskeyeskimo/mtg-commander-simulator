# Commander rules foundation: Phase 1

Baseline: `18f788a63e16fd720e1d4bc23e2f6a82477bf546`.
Rules reference: supplied Wizards of the Coast Comprehensive Rules, effective
September 25, 2026; <https://Magic.Wizards.com/Rules>.
Relevant provisions: 903.3, 903.8, 903.10a, 702.124d, 601.2f/h/i,
120.3b, 120.4, 614.7a, 704.6c.

## Identity and histories

`GameState.commanders` is the sole authoritative registry. Each designation is
owner plus primary/partner role, bound to the existing physical ObjectId. Runtime
bindings never appear in public history or canonical action keys. Incarnations
remain governed by the existing zone-change generation. Designation does not
follow copied definitions or control changes. Historical transition facts retain
the designation without reconnecting to a private current object.

Each record contains the initial public card definition, casts indexed by caster,
and combat damage indexed by recipient. Partner records never share histories.
Registry storage order is not semantic. Validation rejects duplicate/missing/token
bindings, wrong ownership/residence, invalid roles and malformed ledger dimensions
or unrepresentable tax. Physical residence includes ordinary/Aura stack spells and
an existing suspended copy operation's retained physical resolving spell.
Historical source and subject designations must agree with the registry in
pending, stacked, suspended and frozen trigger records, including absent values.

## Casting

Legal enumeration and execution share Commander admission and cost quotation.
The represented path supports printed costs, additional Commander tax, represented
static generic increases/reductions, Affinity, and permitted Flash timing including
cleanup priority. Admission validates designation, owner, zone, mandatory windows,
timing and capacities before payment. Existing payment planning is reused.
Rejected attempts preserve state; successful command-zone casts increment their
own designation/caster history once. Countering does not reverse the count.
Other-zone casts and spell copies do not increment it.

Unrestricted alternative/free-casting effects are not introduced. The generic
Convoke/Delve helper's approximate payment reductions are not imported into this
Commander path; it represents ordinary payment without those optional mechanisms.

## Damage

Commander damage records actual combat damage, independently of life loss,
including Infect. Combat uses the existing automatic self-replacement subset:
Prevent and ModifyAmount. Zero damage cannot be revived by a later increase.
The complete damage batch is capacity-checked before life/counter/ledger results
are applied; unsupported player-choice/custom damage replacements fail explicitly.

This does not implement the existing `PreventCombatDamage` placeholder, a general
replacement-order system, or loss prevention. Existing Toxic modeling is unchanged.
The prior terminal-only loss contract remains: continuing multiplayer elimination
stops as unsupported. Commander loss provenance names each designation and has
stable ordering.

## Visibility and compatibility

Public designation and cast/damage histories remain visible when the current card
is hidden. Current-object bindings are included only where that object is visible;
no hidden runtime identity, generation, library position or private characteristics
are added. Canonical command-zone casts name designation, then reconstruct and
validate the current legal object and targets.

Clone, snapshot, JSON and bincode retain the registry. Deserialization uses the
explicit GameState wire layout plus cross-field Commander validation. Restored
invalid state is rejected; runtime consumers also reject malformed direct states.

Incompatible versions:

- GameState format: `0x3246_3154` (previous `0x3245_4754`).
- Public projection schema: `3` (previous `2`).
- Solver semantic format: `mtg-joint-public-2f1-v1`.

Older saves/tables are rejected, without migration. PlayerState's obsolete separate
Commander fields are removed; callers and fixtures use the registry instead.

## Scope and validation

Phase 2 automatic graveyard/exile-to-command movement remains unchanged. No owner
SBA choices, hand/library replacement choices or new movement continuations are
implemented. General copying/static effects, multiplayer attack selection and
MCCFR algorithms are unchanged.

Deterministic Phase 1 regressions live in `tests/commander_2f1_test.rs`. Existing
identity fixtures now register physical cards. Impossible token designation is
replaced with explicit rejection and ordinary token-lifecycle assertions. The
Aura command-origin rejection fixture checks unchanged state before unsupported
Aura handling. Historical-event, attachment, copy continuation, persistence and
loss-provenance assertions are retained.

Qualification follows AGENTS.md unchanged: focused development tests, related
regressions, independent Astra High review, then sequential default/all-features
qualification and the existing build/check gates. Only the exact
`benchmark_mccfr_training_throughput` test is excluded. Execution results are
reported separately; this document is not evidence that a command passed.

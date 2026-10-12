# Milestone 2F Phase 2: graveyard and exile Commander choices

Baseline: `a4d1e1892ad0346038f800e9b4770aa7fa3976b3`.
Rules reference: Wizards of the Coast, *Magic: The Gathering Comprehensive Rules*, effective September 25, 2026, supplied PDF; the PDF lists `https://magic.wizards.com/rules` as the rules download location.

## Implemented semantics

Successful represented movements first put a Commander into its actual graveyard or exile destination. The zone-change history and trigger matching use that destination. A successful arrival records the exact object generation and designation; another move before the next state-based-action check removes or replaces that opportunity. Failed, prevented, or no-op movement creates no offer. A persistent consumed-opportunity record prevents a declined offer from being replayed by restoration. Linked-exile returns and represented Surveil milling use the same arrival contract.

The next SBA pass freezes eligible arrivals. Owners answer in APNAP order, including separate partner offers. No object moves between answers. Accepted command-zone movements join the prepared ordinary SBA movement set and commit as one transition group; declined offers are consumed. Settlement and priority resume only after choices and SBAs finish, before trigger placement and ordinary actions. A bounded cleanup continuation distinguishes a declined offer with no other activity from a move or trigger that requires another cleanup step. Commander self-sacrifice and Annihilator departures capture death history through the existing transition kernel; a Commander-containing Annihilator selection commits as one event. Lost-player continuing multiplayer elimination remains unsupported.

The persistent window is versioned and validated on restoration. Legal and canonical actions use the public owner/role designation rather than an object ID. The player view, public graph, information set, strategies, goldfish helper, interactive UI and TUI represent the choice without exact generations or runtime IDs. GameState's binary/JSON format tag, projection schema, and solver semantic format advance; prior formats reject without migration.

## Qualification and boundaries

Focused regressions cover actual destinations, accept/decline lifecycle, replacement prevention and exile, stale generations, two to four player APNAP, partner order, simultaneous deaths and historical controller, resolved Undying/Persist, canonical/public invariance, four restoration routes, malformed windows, consumed-offer replay rejection, cleanup and attack continuations, and failed-action immutability. Existing tests were updated where they asserted the old automatic redirection.

Phase 3 hand/library replacement choices are not implemented. The existing bounded replacement engine, generic copy/static effects, general suspended effect continuations, and continuing multiplayer elimination remain outside this milestone. No MCCFR redesign or throughput qualification is included.

# Goblin Storm V1 benchmark contract

Status: **approved contract, execution blocked**. Contract ID: `goblin-storm-v1-draft-runtime-blocked`. This document describes the desired first trustworthy engine-correctness goldfish. Milestone 1 only loads and audits the deck and refuses benchmark execution while required behavior is uncertified. It produces no kill-time or win-rate estimate.

## Purpose and permitted claims

V1 is a two-player **solitaire engine-correctness fixture**: exact cards, rules transitions, pilot decisions and a passive opponent in a pinned environment. Once implemented and certified, it may establish reproducible outcomes for that fixture and pilot. It is neither a representative four-player Commander goldfish nor actual multiplayer performance. The latter needs realistic boards, interaction, hidden information and competent opponents. Passive speed cannot establish deck strength, multiplayer win rate or optimal piloting. Future controlled-interaction and developed-board environments will have distinct contract IDs.

## Cards and setup

- Use precisely the official playable 100 in `decks/goblin_storm.txt`, pinned by the local catalog's expected multiset. Zada, Hedron Grinder is one command-zone card. The remaining 99 are the pilot's library; 22 of those are Mountains. No sideboard, extra display commander, substitutions or chosen opening hand.
- The pilot and passive opponent each begin at 40 life. Each shuffles a real 99-card library and draws seven. All other zones, battlefield, stack and mana pools begin empty. Normal commander tax, damage and zone-change rules apply when relevant.
- The opponent's 100th structural card is the repository's existing **Miku, Lost but Singing** (Azusa reskin), in its command zone only; its library is exactly 99 Forests. It has no battlefield object or resource. No Miku behavior is certified by this fixture. The passive policy never casts it. A forced effect that makes it or its uncertified behavior relevant invalidates the run until that interaction is implemented and reviewed.
- The opponent takes actual turns, draws, performs mandatory actions and choices, processes triggers and cleanup, and can lose by normal rules. It voluntarily plays no lands, casts no spells or commander, activates nothing, never attacks or blocks, and otherwise passes priority. Optional benefits are declined; mandatory choices use a pinned canonical order. The passive policy supplies no removal, taxes, blockers, tapped lands or other resources.

## Mulligan, seats and draws

**V1 benchmark mulligan rule: London mulligan with no multiplayer free mulligan. This is an intentional benchmark simplification and differs from normal multiplayer Commander.** Automated pilot and opponent keep their first seven. Manual pilots may use the legal V1 mulligan range; their decisions and results are recorded separately. No fixed four-mulligan cap.

Each game seed is used for both seats: pilot starts once and opponent starts once. The starting player skips its first-turn draw; the other draws normally. Report both seat strata. Future multiplayer models require separate draw and mulligan contracts.

## Rules-sensitive opponent state

Mana Geyser counts **actual** tapped opposing lands, normally zero. Grenzo sees the actual opponent library and must follow normal exile/play permissions. Gifts create actual objects under the chosen opponent's control in the correct state. Each-opponent effects count one opponent. Own-board targets and actual forced changes to the opponent are processed normally. No beneficial opponent state is assumed or added to help Goblin Storm.

## Outcome, coordinates and limits

Victory is an actual terminal Magic result (life, commander combat damage, failed draw or another supported cause), including simultaneous draws. Potential damage is not a win. Store the global turn serial, active seat, pilot-turn ordinal, phase/step and accepted action index at termination; label kills on an opponent turn explicitly. “Kill turn” means the pilot-turn ordinal of an actual terminal win, with its exact coordinate and cause attached, never the current global turn number alone.

Stop immediately before pilot turn 21, after the preceding opponent turn; extra turns count as turns actually taken. Stop at 100,000 accepted actions including passes and choices. These limits **censor** attempts, not wins, losses or draws. Invalid actions, unsupported reached behavior and nonprogress are invalid runs; infrastructure timeouts are reported separately. Do not prune attacks or priority opportunities for speed.

## Information, randomness and pilots

The acting pilot may see its hand plus public and legally revealed information, never hidden library order or opponent hand. Searches and look effects reveal only what rules allow. Debug omniscience is not a benchmark pilot. Human traces need a decision transcript in addition to the seed.

The intended implementation pins ChaCha8 algorithm/version and seed derivation, with distinct game and policy streams and canonical tie breaking. All shuffles, random effects, mulligans, choices and replay/undo state must use the pinned streams. Preserve random draws by named purpose where practical so future matched deck variants can use corresponding streams, not merely identical initial seed numbers. Serial and parallel traces for a seed/seat must match. Record engine, deck, catalog, behavior, contract and pilot-policy versions. These RNG and view guarantees are **not yet implemented** and block V1 execution.

First review seeded manually piloted traces. Only then evaluate a pinned deterministic automated pilot (initially Greedy). Its results describe that policy, not optimal deck performance. Default aggregate plan: seeds 0–49, both seats, 100 attempts, with a seed-0 two-seat smoke run. Keep every attempt; do not search seeds or retry failures. Training seeds must be separate.

## Reporting and readiness

Record attempted, preflight-blocked, invalid, completed, turn-censored and action-censored counts with reasons; seed, seat, pilot and versions; result/cause/coordinates; mulligans; draws and discards; cast and copied spells; mana and token sources; damage; and action counts. Future comparative reports should also explain opening hand quality, mulligan reasons, land/color sources, screw/flood, commander cast turn, stranded cards and mana/color reasons, spent and unused mana, per-turn damage, setup completion and card participation. These extra measures are **not Milestone 1 implementation work**.

Unknown or unsupported behavior is an invalid attempt, not ordinary right censoring. Do not omit it from denominators or publish performance for only surviving runs. The full fixed-deck benchmark currently fails preflight even though the deck loads for inspection. Removing a capability blocker merely identifies cards for review and retesting; it cannot certify them automatically.

## Catalog, cache and revision boundary

The local catalog pins the 79 names and the exact 99-card mainboard, records printed colors separately from Commander color identity, and keeps support status/capability reasons outside executable `CardDef`. Its stored audit SHA-256 identifies the historical source snapshot. The audit did not retain the original Scryfall retrieval date or external card IDs; the catalog states that provenance limit and records the reviewed printed corrections. Normal loading and tests use no network. The catalog's current `fnv1a64` fingerprint is a stable revision marker, not an authenticity guarantee.

The Kindred enum variant was appended; the existing CardType bincode discriminants remain unchanged, while older readers cannot understand a new Kindred value. `CardDef` layout was not expanded for support metadata. Catalog certification requires reviewed evidence and an executable-definition fingerprint; a changed definition makes certification stale. Old replay/solver caches have no catalog/version binding today, so they must not be treated as V1 benchmark evidence. A future runner must include all pinned versions and RNG state in results/replay before the execution gate may be removed.

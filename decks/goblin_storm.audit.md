# Goblin Storm implementation audit

This audit examines the actual CardDef objects produced by the existing importer for all 79 unique names in `decks/goblin_storm.txt`, using the cached Scryfall responses and current sample database. It is a structural and source-code audit, not an exhaustive gameplay or Comprehensive Rules certification. No engine or card implementations were changed for this audit.

**The deck imports, but its current simulation results cannot measure the intended Zada/Goblin/Storm game plan faithfully.** Missing effects can suppress wins, while dropped costs and restrictions can inflate performance. A successful import or absence of `Unimplemented` is not proof of correct behavior.

## Findings that change the previous coverage interpretation

- The original report is a provenance/heuristic report: six sample definitions and 73 auto-parsed definitions.
- Of the 73 auto-parsed definitions, 37 contain explicit `Unimplemented` effects, six have no executable abilities, and 30 contain parsed fields without an explicit `Unimplemented` marker. These groups are disjoint; the last group is **not** a fully-working count.
- The six ability-empty auto-parsed cards are Conspicuous Snoop, Skirk Prospector, Goblin Bombardment, Goblin Trashmaster, Searslicer Goblin, and Boggart Shenanigans. Creature bodies still exist where applicable.
- At least three of the six sample cards need correction: Roaming Throne has no executable abilities; Skullclamp's death trigger is broader than its attachment; Siege-Gang Commander lacks its activated ability. The original “0 partial/stubbed” count does not describe semantic completeness.

## Implementation order

1. **Make reporting honest.** Keep import provenance separate from implementation confidence. Surface explicit no-ops and known semantic defects. Do not upgrade a parsed creature merely because it has power/toughness. Preserve the original report as a record of the analyzer's output.
2. **Repair the smallest mana/token slice using existing effects.** Start with hand-authored Seething Song, Battle Hymn, Dragon Fodder, and Krenko's Command. Existing `AddMana`, `AddDynamicMana`, `DynamicValue::CreaturesControlled`, and token effects are sufficient. Tests should verify red mana, dynamic counts, exact token properties, and sample-definition precedence. Then add Krenko using subtype counting; inspect its tap/summoning-sickness behavior before declaring it complete.
3. **Implement the defining Zada interaction as one coherent slice.** Preserve targeting on Expedite/Crimson Wisps before adding Zada. Extend the existing stack/trigger path only as needed for spell copies; copies must retain choices, target a different eligible creature, and not count as casts. Test one target, multiple targets, noneligible creatures, a spell targeting Zada plus another object, and copy-resolution behavior. Do not substitute direct repeated effect resolution for stack copies.
4. **Add Storm and cast/copy payoffs on that foundation.** The keyword enum and spell counter exist, but the importer does not map Storm and the counter has no copying consumer. Cover Grapeshot and Empty the Warrens, then Storm-Kiln Artist's cast/copy distinction. Test spell-count timing and ensure copies do not recursively trigger cast events.
5. **Repair sacrifice/death and tribal filters.** Prioritize Skirk Prospector, Goblin Bombardment, Pashalik Mons, Skullclamp, Impact Tremors, and Siege-Gang Commander. Test the actual sacrificed/entering/dying object and its controller/subtype, rather than using self-only or broad triggers.
6. **Complete secondary mechanics and lands.** Graveyard casting, discover, conditional land entry, cycling, activation restrictions, and color/subtype-specific cost reduction follow. Keep unsupported mechanics explicit while this work proceeds.

The next implementation slice should be step 2: a small, useful change with existing effects and deterministic tests. It improves the deck without redesigning the engine; it does not by itself make Zada functional.

## Source evidence

- `src/scryfall.rs`: `scryfall_to_card_def`, `parse_keywords`, `parse_spell_effect`, and the triggered/activated/static parsers. Spell parsing returns after the first recognized effect; unrecognized keywords are silently omitted.
- `src/action/mod.rs`: effect-derived targeting treats DrawCards as untargeted, explaining why parsed Zada cantrips cannot target Zada.
- `src/rules/effects.rs`: `Unimplemented` does nothing; AddMana with `color: None` adds colorless mana.
- `src/rules/triggers.rs`: the cast path increments `spells_cast_this_turn`; controlled-creature death matching does not check Skullclamp attachment.
- `src/card/sample.rs`: the current Roaming Throne, Skullclamp, and Siege-Gang Commander definitions substantiate the sample-card gaps above.
- `src/card/catalog.rs`: the analyzer considers a creature or land sufficient evidence of implementation when it finds no explicit Unimplemented effect.

## All-card inventory

Flags below describe emitted definitions, not completeness. Notes identify verified structural problems; a missing detailed note means further semantic review is required. All 79 unique names are included, with Mountain represented once for its 22 copies.

| Card | Source | Structural flags | Audit note |
|---|---|---|---|
| Zada, Hedron Grinder | auto | Explicit Unimplemented | Copy effect is Unimplemented; trigger is generic YouCastSpell, without the target-only-Zada predicate. |
| Krenko, Mob Boss | auto | Explicit Unimplemented | Tap ability exists, but variable Goblin token creation is Unimplemented. |
| Pashalik Mons | auto | Explicit Unimplemented | Death watcher becomes self Dies; sacrifice-a-Goblin cost becomes SelfSacrifice; token effect is Unimplemented. |
| Brightstone Ritual | auto | Parsed fields present; not certified | Parses as one colorless mana; loses both red color and board-dependent amount. |
| Broadside Bombardiers | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Conspicuous Snoop | auto | No executable abilities | Oracle abilities are absent from executable fields; requires a hand-authored implementation or parser support. |
| Empty the Warrens | auto | Explicit Unimplemented | Token effect is Unimplemented and Storm is not imported. |
| Grapeshot | auto | Parsed fields present; not certified | Single damage effect exists; Storm is not imported. |
| Skirk Prospector | auto | No executable abilities | Oracle abilities are absent from executable fields; requires a hand-authored implementation or parser support. |
| Roaming Throne | sample | No executable abilities | Sample definition has no executable abilities; fixed Golem/Rat subtypes. Analyzer fully-implemented label is a false positive. |
| Skullclamp | sample | Parsed fields present; not certified | Sample trigger watches a controlled creature dying, without checking attachment to that creature; draw trigger is too broad. |
| Sol Ring | sample | Parsed fields present; not certified | Expected core mana/equipment definitions present. No additional defect identified in this structural review; not a complete interaction certification. |
| Blasphemous Act | auto | Parsed fields present; not certified | Parsed fields require semantic review; not certified. |
| Chaos Warp | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Frontline Heroism | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Goblin Bombardment | auto | No executable abilities | Oracle abilities are absent from executable fields; requires a hand-authored implementation or parser support. |
| Goblin Chieftain | auto | Parsed fields present; not certified | Anthem/haste affect creatures of hard-coded player 0, without Goblin or other-creature restrictions. |
| Goblin Dark-Dwellers | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Goblin Lackey | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Goblin Trashmaster | auto | No executable abilities | Oracle abilities are absent from executable fields; requires a hand-authored implementation or parser support. |
| Great Train Heist | auto | Parsed fields present; not certified | Parsed fields require semantic review; not certified. |
| Grenzo, Havoc Raiser | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Howlsquad Heavy | auto | Explicit Unimplemented | Max-speed mana gate is lost, amount becomes fixed; token creation is Unimplemented. |
| Past in Flames | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Redcap Gutter-Dweller | auto | Explicit Unimplemented | Token creation is Unimplemented; upkeep is reduced to Buff, losing sacrifice, exile, and permission. |
| Rundvelt Hordemaster | auto | Explicit Unimplemented | Goblin anthem absent; death watcher becomes self Dies and exile/casting effect is Unimplemented. |
| Searslicer Goblin | auto | No executable abilities | Oracle abilities are absent from executable fields; requires a hand-authored implementation or parser support. |
| Siege-Gang Commander | sample | Parsed fields present; not certified | Sample ETB creates three Goblins; activated sacrifice-for-damage ability is absent. |
| Siege-Gang Lieutenant | auto | Explicit Unimplemented | Sacrifice-a-Goblin cost becomes SelfSacrifice; conditional token/haste effect is Unimplemented. |
| Idol of Oblivion | auto | Explicit Unimplemented | Draw ability lacks the token-created-this-turn restriction; Eldrazi creation is Unimplemented. |
| Ruby Medallion | auto | Parsed fields present; not certified | Reduction applies to all spells, without red restriction. |
| Throne of Eldraine | auto | Explicit Unimplemented | Mana effect is Unimplemented; color-choice/spending constraints are missing. Parser invents an ETB draw-two trigger. |
| Arena of Glory | auto | Parsed fields present; not certified | Conditional tapped entry becomes unconditional; exert/haste restrictions lost from the special mana ability. |
| Castle Embereth | auto | Parsed fields present; not certified | Conditional tapped entry becomes unconditional; team pump becomes a targeted Buff. |
| Den of the Bugbear | auto | Explicit Unimplemented | Conditional tapped entry becomes unconditional; creature animation is Unimplemented. |
| Fountainport | auto | Explicit Unimplemented | Sacrifice-a-token becomes SelfSacrifice; Fish creation is Unimplemented. Treasure creation exists. |
| Kher Keep | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Spinerock Knoll | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| War Room | auto | Parsed fields present; not certified | Draw ability loses the color-identity-dependent life cost. |
| Shinka, the Bloodsoaked Keep | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Ancestors' Aid | auto | Explicit Unimplemented | Buff is parsed; first strike and Treasure creation are missing. Treasure reminder text creates spurious abilities. |
| Battle Hymn | auto | Parsed fields present; not certified | Parses as one colorless mana; loses both red color and board-dependent amount. |
| Boggart Shenanigans | auto | No executable abilities | Oracle abilities are absent from executable fields; requires a hand-authored implementation or parser support. |
| Crimson Wisps | auto | Parsed fields present; not certified | Parses as DrawCards(1) only; creature targeting and other effects disappear. Cannot supply the intended Zada target. |
| Daring Discovery | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Dragon Fodder | auto | Explicit Unimplemented | Fixed Goblin token creation is Unimplemented despite existing engine token effects. |
| Expedite | auto | Parsed fields present; not certified | Parses as DrawCards(1) only; creature targeting and other effects disappear. Cannot supply the intended Zada target. |
| Faithless Looting | auto | Parsed fields present; not certified | Only draws two; discard and flashback are absent. |
| Fists of Flame | auto | Parsed fields present; not certified | Parses as DrawCards(1) only; creature targeting and other effects disappear. Cannot supply the intended Zada target. |
| Gempalm Incinerator | auto | Parsed fields present; not certified | Cycling becomes an ordinary activated draw with doubled printed/reminder mana cost and no discard cost; special cycling behavior absent. |
| General Kreat, the Boltbringer | auto | Parsed fields present; not certified | Creature-entry watcher becomes self EntersBattlefield; damage uses AnyPlayer instead of each opponent. |
| Glimpse the Impossible | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Goblin Bushwhacker | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Goblin Matron | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Goblin Negotiation | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Goblin Warchief | auto | Parsed fields present; not certified | Reduction applies to all spells; Goblin restriction and Goblin haste are absent. |
| Haze of Rage | auto | Parsed fields present; not certified | Parses a single-target buff and static anthem; Storm, buyback, and the intended team-wide spell effect are absent. |
| Impact Tremors | auto | Parsed fields present; not certified | Creature-entry watcher becomes self EntersBattlefield; damage uses AnyPlayer instead of each opponent. |
| Impulsive Pilferer | auto | Explicit Unimplemented | Dies-to-Treasure effect exists, but reminder text grants a spurious TapForAny ability. Encore is malformed/Unimplemented. |
| Krenko's Command | auto | Explicit Unimplemented | Fixed Goblin token creation is Unimplemented despite existing engine token effects. |
| Mana Geyser | auto | Parsed fields present; not certified | Parses as one colorless mana; loses both red color and board-dependent amount. |
| Mogg War Marshal | auto | Explicit Unimplemented | Echo and token creation are Unimplemented; combined enters-or-dies text only produces a Dies trigger. |
| Quest for the Goblin Lord | auto | Explicit Unimplemented | Counter effect is Unimplemented; team buff lacks the five-counter condition and uses hard-coded player 0. |
| Renegade Tactics | auto | Parsed fields present; not certified | Parses as DrawCards(1) only; creature targeting and other effects disappear. Cannot supply the intended Zada target. |
| Sazacap's Brew | auto | Parsed fields present; not certified | Only draws one; draw quantity, costs/conditions, targeting and additional effects are lost. |
| Seething Song | auto | Parsed fields present; not certified | Parses as five colorless mana, not red. |
| Spreading Insurrection | auto | Explicit Unimplemented | Control-changing spell effect is Unimplemented; Storm is not imported. |
| Storm-Kiln Artist | auto | Explicit Unimplemented | Generic cast trigger produces Treasure; copy trigger and artifact-scaled power are absent. Treasure reminder text also creates a spurious mana ability. |
| Vandalblast | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Wild Ride | auto | Parsed fields present; not certified | Only +3/+0 buff is parsed; haste and harmonize are absent. |
| Witch's Mark | auto | Parsed fields present; not certified | Only draws one; draw quantity, costs/conditions, targeting and additional effects are lost. |
| Swiftfoot Boots | sample | Parsed fields present; not certified | Expected core mana/equipment definitions present. No additional defect identified in this structural review; not a complete interaction certification. |
| Dwarven Mine | auto | Parsed fields present; not certified | Conditional tapped entry becomes unconditional; Dwarf token trigger absent. |
| Forgotten Cave | auto | Parsed fields present; not certified | Cycling becomes an ordinary activated draw with doubled printed/reminder mana cost and no discard cost; special cycling behavior absent. |
| Goblin Burrows | auto | Parsed fields present; not certified | Pump loses the Goblin-only target restriction. |
| Hidden Volcano | auto | Explicit Unimplemented | At least one ability is explicitly a no-op; remaining costs, triggers and restrictions require semantic review. |
| Reliquary Tower | auto | Parsed fields present; not certified | Colorless mana works; no-maximum-hand-size ability absent. |
| Smoldering Crater | auto | Parsed fields present; not certified | Cycling becomes an ordinary activated draw with doubled printed/reminder mana cost and no discard cost; special cycling behavior absent. |
| Mountain | sample | Parsed fields present; not certified | Expected core mana/equipment definitions present. No additional defect identified in this structural review; not a complete interaction certification. |

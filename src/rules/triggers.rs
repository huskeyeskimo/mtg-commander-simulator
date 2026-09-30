use crate::card::{ObjectId, TriggerCondition};
use crate::events::GameEvent;
use crate::game::{GameState, PendingTrigger, PlayerIndex, StackEntry, StackSource};

/// Check all permanents on the battlefield for triggered abilities matching
/// the given condition, and queue any that trigger.
pub(super) fn check_triggers(state: &mut GameState, condition: TriggerCondition, source_hint: Option<ObjectId>) {
    let triggers: Vec<PendingTrigger> = {
        let db = state.card_db();
        let mut found = Vec::new();

        // Determine which objects to check
        let objects_to_check: Vec<(ObjectId, usize)> = match source_hint {
            // If a specific source is given (e.g., ETB on a specific permanent), only check it
            Some(id) => {
                if let Some(inst) = state.objects.get(&id) {
                    vec![(id, inst.controller)]
                } else {
                    vec![]
                }
            }
            // Otherwise check all permanents on the battlefield
            None => state
                .battlefield
                .iter()
                .map(|&id| (id, state.objects[&id].controller))
                .collect(),
        };

        for (obj_id, controller) in objects_to_check {
            let inst = &state.objects[&obj_id];
            let def = match db.get(inst.card_def_id) {
                Some(d) => d,
                None => continue,
            };

            for (i, trigger) in def.triggered_abilities.iter().enumerate() {
                if trigger.trigger == condition {
                    found.push(PendingTrigger {
                        source_id: obj_id,
                        ability_index: i,
                        controller,
                        targets: vec![], // targets chosen when put on stack (simplified: auto-target)
                    });
                }
            }
        }
        found
    };

    state.pending_triggers.extend(triggers);
}

/// Check `ACreatureYouControlDies` triggers — only fires for permanents whose
/// controller matches the dying creature's controller.
pub(super) fn check_your_creature_dies_triggers(
    state: &mut GameState,
    dying_controllers: &[PlayerIndex],
) {
    let triggers: Vec<PendingTrigger> = {
        let db = state.card_db();
        let mut found = Vec::new();

        for &obj_id in &state.battlefield {
            let inst = &state.objects[&obj_id];
            let controller = inst.controller;
            let def = match db.get(inst.card_def_id) {
                Some(d) => d,
                None => continue,
            };

            for (i, trigger) in def.triggered_abilities.iter().enumerate() {
                if trigger.trigger == TriggerCondition::ACreatureYouControlDies
                    && dying_controllers.contains(&controller)
                {
                    found.push(PendingTrigger {
                        source_id: obj_id,
                        ability_index: i,
                        controller,
                        targets: vec![],
                    });
                }
            }
        }
        found
    };

    state.pending_triggers.extend(triggers);
}

/// Flush pending triggers onto the stack in APNAP order
/// (Active Player first, then clockwise through all non-active players).
/// When a player controls multiple simultaneous triggers, they must choose
/// the ordering — this is surfaced as an `Action::OrderTriggers` decision
/// point for MCCFR to observe.
///
/// If a player has >1 trigger, this function pauses (leaves triggers in
/// `pending_triggers` and sets `priority_player`) so the game loop can
/// present the ordering choice. Returns `true` if all triggers were flushed,
/// `false` if paused waiting for a player's ordering decision.
#[must_use]
pub(super) fn flush_triggers(state: &mut GameState) -> bool {
    if state.pending_triggers.is_empty() {
        return true;
    }

    let active = state.active_player;
    let num_players = state.players.len();

    // Process players in APNAP order: active player first, then clockwise
    for offset in 0..num_players {
        let player = (active + offset) % num_players;
        if state.players[player].has_lost {
            continue;
        }

        let player_count = state
            .pending_triggers
            .iter()
            .filter(|t| t.controller == player)
            .count();

        if player_count == 0 {
            continue;
        }

        if player_count > 1 {
            // Player has multiple triggers — pause for ordering decision
            state.priority_player = player;
            return false;
        }

        // Player has exactly 1 trigger: auto-push it
        let triggers: Vec<PendingTrigger> = state
            .pending_triggers
            .iter()
            .filter(|t| t.controller == player)
            .cloned()
            .collect();
        for trigger in triggers {
            push_trigger_to_stack(state, &trigger);
        }
        state.pending_triggers.retain(|t| t.controller != player);
    }

    true
}

/// Push a single trigger onto the stack as a TriggeredAbility entry.
pub(super) fn push_trigger_to_stack(state: &mut GameState, trigger: &PendingTrigger) {
    let stack_id = state.new_stack_id();
    state.stack.push(StackEntry {
        id: stack_id,
        source: StackSource::TriggeredAbility {
            source_id: trigger.source_id,
            ability_index: trigger.ability_index,
        },
        controller: trigger.controller,
        targets: trigger.targets.clone(),
        target_generations: crate::targeting::target_generations(state, &trigger.targets),
    });
    state.emit_event(GameEvent::AbilityTriggered {
        source: trigger.source_id,
        ability_index: trigger.ability_index,
    });
}

/// Check triggers for a specific game event and flush them to the stack.
/// Returns `true` if all triggers were flushed, `false` if paused waiting
/// for a player's ordering decision (i.e. pending_triggers is non-empty).
#[must_use]
pub fn fire_triggers(state: &mut GameState, condition: TriggerCondition, source_hint: Option<ObjectId>) -> bool {
    check_triggers(state, condition, source_hint);
    flush_triggers(state)
}

/// Count how many permanents a player controls that have a specific
/// `StaticAbility` variant. Used for mana bonus abilities like
/// `ManaFromNonlandBonus` (Kinnan) and `ManaFromSwampBonus` (Nirkana Revenant).
pub(super) fn count_static_ability(
    state: &GameState,
    player: PlayerIndex,
    target: &crate::layers::StaticAbility,
) -> u32 {
    let db = state.card_db();
    let target_disc = std::mem::discriminant(target);
    let mut count = 0u32;
    for &obj_id in &state.battlefield {
        let inst = match state.objects.get(&obj_id) {
            Some(i) => i,
            None => continue,
        };
        if inst.controller != player {
            continue;
        }
        let def = match db.get(inst.card_def_id) {
            Some(d) => d,
            None => continue,
        };
        for sa in &def.static_abilities {
            if std::mem::discriminant(sa) == target_disc {
                count += 1;
            }
        }
    }
    count
}

/// Convenience wrapper: count `ManaFromNonlandBonus` sources (e.g., Kinnan).
pub(super) fn mana_from_nonland_bonus_count(state: &GameState, player: PlayerIndex) -> u32 {
    count_static_ability(state, player, &crate::layers::StaticAbility::ManaFromNonlandBonus)
}

/// Convenience wrapper: count `ManaFromSwampBonus` sources (e.g., Nirkana Revenant, Crypt Ghast).
pub(super) fn mana_from_swamp_bonus_count(state: &GameState, player: PlayerIndex) -> u32 {
    count_static_ability(state, player, &crate::layers::StaticAbility::ManaFromSwampBonus)
}

/// Fire spell-cast triggers for a spell that was just cast.
/// `caster` is the player who cast the spell. `is_creature` indicates whether
/// the spell is a creature spell (relevant for OpponentCastsNoncreatureSpell).
pub(super) fn fire_spell_cast_triggers(state: &mut GameState, caster: PlayerIndex, is_creature: bool) {
    let triggers: Vec<PendingTrigger> = {
        let db = state.card_db();
        let mut found = Vec::new();

        for &obj_id in &state.battlefield {
            let inst = &state.objects[&obj_id];
            let controller = inst.controller;
            let def = match db.get(inst.card_def_id) {
                Some(d) => d,
                None => continue,
            };

            for (i, trigger) in def.triggered_abilities.iter().enumerate() {
                let matches = match trigger.trigger {
                    TriggerCondition::YouCastSpell => controller == caster,
                    TriggerCondition::YouCastCreatureSpell => {
                        controller == caster && is_creature
                    }
                    TriggerCondition::OpponentCastsSpell => controller != caster,
                    TriggerCondition::OpponentCastsNoncreatureSpell => {
                        controller != caster && !is_creature
                    }
                    _ => false,
                };
                if matches {
                    found.push(PendingTrigger {
                        source_id: obj_id,
                        ability_index: i,
                        controller,
                        targets: vec![],
                    });
                }
            }
        }
        found
    };

    state.pending_triggers.extend(triggers);

    // Prowess: noncreature spells give +1/+1 until EOT to creatures with Prowess
    if !is_creature {
        apply_prowess(state, caster);
    }

    // Extort: each permanent with Extort drains 1 life from each opponent
    apply_extort(state, caster);

    // Track spell count for Storm
    state.spells_cast_this_turn += 1;

    let _ = flush_triggers(state);
}

/// Apply prowess: each creature the caster controls with Prowess gets +1/+1 until EOT.
fn apply_prowess(state: &mut GameState, caster: PlayerIndex) {
    use crate::card::KeywordAbility;
    use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};

    let prowess_creatures: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            state
                .objects
                .get(&id)
                .map_or(false, |inst| inst.controller == caster)
                && state.has_keyword(id, KeywordAbility::Prowess)
        })
        .collect();

    for &id in &prowess_creatures {
        let ts = state.new_timestamp();
        state.continuous_effects.push(ContinuousEffect {
            source_id: id,
            controller: caster,
            timestamp: ts,
            duration: Duration::UntilEndOfTurn,
            affected: AffectedObjects::Specific(id),
            modification: LayerModification::ModifyPT(1, 1),
        });
    }
    if !prowess_creatures.is_empty() {
        state.invalidate_characteristics_cache();
    }
}

/// Check for Undying and Persist keyword abilities on creatures that just died.
/// - Undying: if the creature had no +1/+1 counters, return it from graveyard
///   with a +1/+1 counter.
/// - Persist: if the creature had no -1/-1 counters, return it from graveyard
///   with a -1/-1 counter.
pub(super) fn check_undying_persist(state: &mut GameState, died: &[ObjectId]) {
    use crate::card::KeywordAbility;

    let mut to_return: Vec<(ObjectId, bool)> = Vec::new(); // (id, is_undying)

    for &obj_id in died {
        let (has_undying, has_persist, plus_counters, minus_counters) = {
            let inst = match state.objects.get(&obj_id) {
                Some(i) => i,
                None => continue,
            };
            let db = state.card_db();
            let def = match db.get(inst.card_def_id) {
                Some(d) => d,
                None => continue,
            };
            let has_undying = def.keywords.contains(&KeywordAbility::Undying);
            let has_persist = def.keywords.contains(&KeywordAbility::Persist);
            (has_undying, has_persist, inst.plus_counters, inst.minus_counters)
        };

        if has_undying && plus_counters == 0 {
            to_return.push((obj_id, true));
        } else if has_persist && minus_counters == 0 {
            to_return.push((obj_id, false));
        }
    }

    for (obj_id, is_undying) in to_return {
        // Check if still in graveyard
        let in_graveyard = state.players.iter().any(|p| p.graveyard.contains(&obj_id));
        if !in_graveyard {
            continue;
        }

        let owner = state.objects.get(&obj_id).map(|i| i.owner).unwrap_or(0);
        state.move_object(obj_id, crate::card::ZoneType::Graveyard, crate::card::ZoneType::Battlefield);

        // Reset the instance and add the appropriate counter
        if let Some(inst) = state.objects.get_mut(&obj_id) {
            inst.controller = owner;
            inst.damage_marked = 0;
            inst.tapped = false;
            inst.summoning_sick = true;
            if is_undying {
                inst.plus_counters += 1;
            } else {
                inst.minus_counters += 1;
            }
        }

        // Fire ETB triggers for the returned creature
        let _ = fire_triggers(state, crate::card::TriggerCondition::EntersBattlefield, Some(obj_id));
    }
}

/// Fire card-draw triggers when a player draws a card.
/// `drawing_player` is the player who drew. This fires `OpponentDrawsCard`
/// on permanents controlled by each opponent of the drawing player.
pub(super) fn fire_card_draw_triggers(state: &mut GameState, drawing_player: PlayerIndex) {
    let triggers: Vec<PendingTrigger> = {
        let db = state.card_db();
        let mut found = Vec::new();

        for &obj_id in &state.battlefield {
            let inst = &state.objects[&obj_id];
            let controller = inst.controller;
            let def = match db.get(inst.card_def_id) {
                Some(d) => d,
                None => continue,
            };

            for (i, trigger) in def.triggered_abilities.iter().enumerate() {
                if trigger.trigger == TriggerCondition::OpponentDrawsCard
                    && controller != drawing_player
                {
                    found.push(PendingTrigger {
                        source_id: obj_id,
                        ability_index: i,
                        controller,
                        targets: vec![],
                    });
                }
            }
        }
        found
    };

    state.pending_triggers.extend(triggers);
    let _ = flush_triggers(state);
}

/// Apply Exalted: when exactly one creature attacks, each permanent you control
/// with Exalted gives the attacker +1/+1 until end of turn.
pub(super) fn apply_exalted(state: &mut GameState, attacker_id: ObjectId) {
    use crate::card::KeywordAbility;
    use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};

    let controller = match state.objects.get(&attacker_id) {
        Some(inst) => inst.controller,
        None => return,
    };

    let exalted_count = state
        .battlefield
        .iter()
        .filter(|&&id| {
            state
                .objects
                .get(&id)
                .map_or(false, |inst| inst.controller == controller)
                && state.has_keyword(id, KeywordAbility::Exalted)
        })
        .count() as i32;

    if exalted_count > 0 {
        let ts = state.new_timestamp();
        state.continuous_effects.push(ContinuousEffect {
            source_id: attacker_id,
            controller,
            timestamp: ts,
            duration: Duration::UntilEndOfTurn,
            affected: AffectedObjects::Specific(attacker_id),
            modification: LayerModification::ModifyPT(exalted_count, exalted_count),
        });
        state.invalidate_characteristics_cache();
    }
}

/// Apply Annihilator: when a creature with Annihilator N attacks, the defending
/// player sacrifices N permanents (simplified: random permanents).
pub(super) fn apply_annihilator(state: &mut GameState, attackers: &[ObjectId]) {
    use crate::card::ZoneType;

    let defending_player = state.next_player(state.active_player);

    // Collect annihilator counts from attacking creatures
    let mut total_annihilator = 0u32;
    {
        let db = state.card_db();
        for &attacker_id in attackers {
            if let Some(inst) = state.objects.get(&attacker_id) {
                if let Some(def) = db.get(inst.card_def_id) {
                    if let Some(n) = def.annihilator_count {
                        total_annihilator += n;
                    }
                }
            }
        }
    }

    if total_annihilator == 0 {
        return;
    }

    // Defending player sacrifices N permanents (simplified: sacrifice the cheapest permanents)
    let mut sacrificed = 0u32;
    let their_perms: Vec<ObjectId> = state
        .battlefield
        .iter()
        .copied()
        .filter(|&id| {
            state
                .objects
                .get(&id)
                .map_or(false, |inst| inst.controller == defending_player)
        })
        .collect();

    for &perm_id in &their_perms {
        if sacrificed >= total_annihilator {
            break;
        }
        state.move_object(perm_id, ZoneType::Battlefield, ZoneType::Graveyard);
        sacrificed += 1;
    }

    if sacrificed > 0 {
        state.refresh_continuous_effects();
        state.refresh_replacement_effects();
    }
}

/// Apply Extort: when a spell is cast, each permanent with Extort drains 1 life
/// from each opponent (simplified: auto-extort without optional payment).
pub(super) fn apply_extort(state: &mut GameState, caster: PlayerIndex) {
    use crate::card::KeywordAbility;

    let extort_count = {
        state
            .battlefield
            .iter()
            .filter(|&&id| {
                state
                    .objects
                    .get(&id)
                    .map_or(false, |inst| inst.controller == caster)
                    && state.has_keyword(id, KeywordAbility::Extort)
            })
            .count() as i32
    };

    if extort_count > 0 {
        let opponents = state.opponents(caster);
        for &opp in &opponents {
            state.players[opp].life -= extort_count;
        }
        state.players[caster].life += extort_count * opponents.len() as i32;
    }
}

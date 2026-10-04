use crate::card::{CardDef, CardType, ObjectId, TriggerCondition, ZoneType};
use crate::game::{GameState, PlayerIndex};

/// Find duplicate permanents to remove based on a predicate.
/// Used by both the legendary rule (CR 704.5j) and planeswalker uniqueness
/// rule (CR 704.5i). For each (controller, name) group with >1 match, keeps
/// the newest (highest ObjectId) and returns the rest.
fn find_duplicates_to_remove(
    state: &GameState,
    predicate: impl Fn(&CardDef) -> bool,
) -> Vec<ObjectId> {
    let db = state.card_db();
    let mut name_map: std::collections::HashMap<(usize, String), Vec<ObjectId>> =
        std::collections::HashMap::new();
    for &id in &state.battlefield {
        let inst = &state.objects[&id];
        if let Some(def) = db.get(inst.card_def_id) {
            if predicate(def) {
                name_map
                    .entry((inst.controller, def.name.clone()))
                    .or_default()
                    .push(id);
            }
        }
    }
    let mut to_remove = Vec::new();
    for (_, ids) in name_map {
        if ids.len() > 1 {
            let keep = *ids.iter().max().unwrap();
            for &id in &ids {
                if id != keep {
                    to_remove.push(id);
                }
            }
        }
    }
    to_remove
}

/// Check and apply state-based actions, implementing the CR 704.3 loop.
///
/// Settle all SBA passes and collect their triggers before placement:
///
/// ```text
/// loop {
///     perform_all_SBAs()         // inner loop until no more SBAs apply
///     check_and_queue_triggers() // queue triggers for events that happened
///     if no_SBAs_performed && no_triggers_queued { break }
/// }
/// put_triggers_on_stack()        // may pause for OrderTriggers
/// // only now grant priority
/// ```
pub fn check_state_based_actions(state: &mut GameState) -> bool {
    if state.pending_copy_order.is_some() {
        return false;
    }
    // Every SBA pass belongs to one settlement window, including events from
    // returns and other work performed by an earlier pass. Keep a caller's
    // enclosing-resolution deferral intact when this function returns.
    let caller_deferred = std::mem::replace(&mut state.trigger_placement_deferred, true);
    let mut performed_sba = false;
    // Outer CR 704.3 loop: interleave SBA checks with trigger checks
    loop {
        // --- Inner SBA loop: perform all SBAs until stable ---
        let mut died_this_round: Vec<ObjectId> = Vec::new();
        let mut captured_dies_triggers = Vec::new();
        let mut any_sba = false;

        loop {
            let mut any_action = false;

            // CR 704.5a: Player with 0 or less life loses
            for i in 0..state.players.len() {
                if state.players[i].life <= 0 && !state.players[i].has_lost {
                    state.players[i].has_lost = true;
                    any_action = true;
                }
            }

            // CR 704.5c: Player with 10+ poison counters loses
            for i in 0..state.players.len() {
                if state.players[i].poison_counters >= 10 && !state.players[i].has_lost {
                    state.players[i].has_lost = true;
                    any_action = true;
                }
            }

            // CR 903.10a (Commander): Player with 21+ commander damage from
            // a single commander loses the game.
            if state.is_commander_format() {
                for i in 0..state.players.len() {
                    if state.players[i].has_lost {
                        continue;
                    }
                    for dmg in &state.players[i].commander_damage_received {
                        if *dmg >= 21 {
                            state.players[i].has_lost = true;
                            any_action = true;
                            break;
                        }
                    }
                }
            }

            // Player elimination: remove eliminated players' permanents and stack entries
            {
                let eliminated: Vec<usize> = (0..state.players.len())
                    .filter(|&i| state.players[i].has_lost)
                    .collect();
                let mut removed_any = false;
                for &elim in &eliminated {
                    // Remove their permanents from the battlefield
                    let their_perms: Vec<ObjectId> = state
                        .battlefield
                        .iter()
                        .copied()
                        .filter(|&id| {
                            state
                                .objects
                                .get(&id)
                                .map_or(false, |inst| inst.controller == elim)
                        })
                        .collect();
                    for &id in &their_perms {
                        if state.battlefield.contains(&id) {
                            state.move_object(id, ZoneType::Battlefield, ZoneType::Exile);
                            any_action = true;
                            removed_any = true;
                        }
                    }
                    // Remove their spells/abilities from the stack
                    state.stack.retain(|entry| entry.controller != elim);
                    // Remove their pending triggers
                    state.pending_triggers.retain(|t| t.controller != elim);
                }
                if removed_any {
                    state.refresh_continuous_effects();
                    state.refresh_replacement_effects();
                }
            }

            // CR 704.5d: +1/+1 and -1/-1 counter cancellation
            for &obj_id in &state.battlefield.clone() {
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    if inst.plus_counters > 0 && inst.minus_counters > 0 {
                        let cancel = inst.plus_counters.min(inst.minus_counters);
                        inst.plus_counters -= cancel;
                        inst.minus_counters -= cancel;
                        any_action = true;
                    }
                }
            }
            if any_action {
                state.invalidate_characteristics_cache();
            }

            // CR 704.5j: Legendary rule — if a player controls two or more
            // legendary permanents with the same name, keep the newest.
            let legendary_dupes: Vec<ObjectId> =
                find_duplicates_to_remove(state, |def| {
                    def.supertypes.contains(&crate::card::Supertype::Legendary)
                });
            for &id in &legendary_dupes {
                captured_dies_triggers.extend(super::triggers::capture_source_triggers(state, TriggerCondition::Dies, id));
                state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                any_action = true;
            }
            if !legendary_dupes.is_empty() {
                state.refresh_continuous_effects();
                state.refresh_replacement_effects();
                died_this_round.extend(legendary_dupes);
            }

            // CR 704.5i: Planeswalker uniqueness rule — keep newest per name.
            let pw_dupes: Vec<ObjectId> =
                find_duplicates_to_remove(state, |def| {
                    def.card_types.contains(&CardType::Planeswalker)
                });
            for &id in &pw_dupes {
                captured_dies_triggers.extend(super::triggers::capture_source_triggers(state, TriggerCondition::Dies, id));
                state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                any_action = true;
            }
            if !pw_dupes.is_empty() {
                state.refresh_continuous_effects();
                state.refresh_replacement_effects();
                died_this_round.extend(pw_dupes);
            }

            // CR 704.5i: Planeswalker with 0 or fewer loyalty counters is put
            // into its owner's graveyard.
            {
                let db = state.card_db();
                let pw_zero_loyalty: Vec<ObjectId> = state
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let inst = match state.objects.get(&id) {
                            Some(i) => i,
                            None => return false,
                        };
                        let def = match db.get(inst.card_def_id) {
                            Some(d) => d,
                            None => return false,
                        };
                        def.card_types.contains(&CardType::Planeswalker)
                            && inst.loyalty_counters == 0
                    })
                    .collect();
                for &id in &pw_zero_loyalty {
                    captured_dies_triggers.extend(super::triggers::capture_source_triggers(state, TriggerCondition::Dies, id));
                    state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                    any_action = true;
                }
                if !pw_zero_loyalty.is_empty() {
                    state.refresh_continuous_effects();
                    state.refresh_replacement_effects();
                    died_this_round.extend(pw_zero_loyalty);
                }
            }

            // CR 704.5n: Aura not attached to a legal permanent goes to graveyard
            {
                let db = state.card_db();
                let aura_to_remove: Vec<ObjectId> = state
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let inst = match state.objects.get(&id) {
                            Some(i) => i,
                            None => return false,
                        };
                        let def = match db.get(inst.card_def_id) {
                            Some(d) => d,
                            None => return false,
                        };
                        if !def.is_aura() {
                            return false;
                        }
                        // Aura must be attached to something on the battlefield
                        match inst.attached_to {
                            None => true, // not attached → remove
                            Some(target_id) => !state.battlefield.contains(&target_id), // target left → remove
                        }
                    })
                    .collect();
                for &id in &aura_to_remove {
                    state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                    any_action = true;
                }
                if !aura_to_remove.is_empty() {
                    state.refresh_continuous_effects();
                }
            }

            // Equipment that loses its equipped creature stays on the battlefield (unattached)
            {
                let db = state.card_db();
                let orphaned_equipment: Vec<ObjectId> = state
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&id| {
                        let inst = match state.objects.get(&id) {
                            Some(i) => i,
                            None => return false,
                        };
                        let def = match db.get(inst.card_def_id) {
                            Some(d) => d,
                            None => return false,
                        };
                        if !def.is_equipment() {
                            return false;
                        }
                        match inst.attached_to {
                            None => false,
                            Some(target_id) => !state.battlefield.contains(&target_id),
                        }
                    })
                    .collect();
                for &id in &orphaned_equipment {
                    if let Some(inst) = state.objects.get_mut(&id) {
                        inst.attached_to = None;
                        any_action = true;
                    }
                }
            }

            // CR 704.5f/g: Creature with toughness <= 0 or lethal damage
            let to_die: Vec<ObjectId> = {
                state
                    .battlefield
                    .iter()
                    .copied()
                    .filter(|&id| {
                        if state.is_creature(id) {
                            let toughness = state.effective_toughness(id);
                            let damage = state.objects[&id].damage_marked as i32;
                            if toughness <= 0 {
                                return true;
                            }
                            if damage >= toughness {
                                return true;
                            }
                        }
                        false
                    })
                    .collect()
            };

            for &obj_id in &to_die {
                // Check death replacement effects (CR 614)
                let dest_zone = state.death_replacement_zone(obj_id);
                if dest_zone == ZoneType::Battlefield {
                    // Replacement prevented the death — creature stays
                    continue;
                }
                captured_dies_triggers.extend(super::triggers::capture_source_triggers(state, TriggerCondition::Dies, obj_id));
                state.move_object(obj_id, ZoneType::Battlefield, dest_zone);
                any_action = true;
            }
            if !to_die.is_empty() {
                // Refresh continuous effects after permanents leave the battlefield
                state.refresh_continuous_effects();
                state.refresh_replacement_effects();
            }
            died_this_round.extend(to_die);

            // Check for game end
            let losers: Vec<usize> = (0..state.players.len())
                .filter(|&i| state.players[i].has_lost)
                .collect();

            if losers.len() >= state.players.len() - 1 {
                state.game_over = true;
                state.winner = (0..state.players.len())
                    .find(|&i| !state.players[i].has_lost);
            }

            if !any_action {
                break;
            }
            any_sba = true;
            performed_sba = true;
        }

        // --- Queue triggers for SBA events ---
        let triggers_before = state.pending_triggers.len();
        state.pending_triggers.extend(captured_dies_triggers);
        // Check "whenever a creature dies" watcher triggers on surviving permanents.
        if !died_this_round.is_empty() {
            super::triggers::check_triggers(state, TriggerCondition::ACreatureDies, None);
            // Check controller-filtered "whenever a creature you control dies" triggers.
            let dying_controllers: Vec<PlayerIndex> = died_this_round
                .iter()
                .filter_map(|&id| state.objects.get(&id).map(|inst| inst.controller))
                .collect();
            super::triggers::check_your_creature_dies_triggers(state, &dying_controllers);
        }
        // Check Undying/Persist for creatures that died this round
        super::triggers::check_undying_persist(state, &died_this_round);

        let triggers_queued = state.pending_triggers.len() > triggers_before;

        // --- CR 704.3 exit condition ---
        if !any_sba && !triggers_queued {
            break;
        }
        }
    state.trigger_placement_deferred = caller_deferred;
    // A resolving spell/ability owns the later placement boundary. A direct
    // SBA caller places only after all passes have stabilized.
    if !caller_deferred && !state.cleanup_discard_in_progress && !state.pending_triggers.is_empty() {
        let _ = super::triggers::flush_triggers(state);
    }
    performed_sba
}

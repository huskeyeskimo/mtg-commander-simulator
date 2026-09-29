use crate::card::{CardType, ObjectId, TriggerCondition, ZoneType};
use crate::game::{GameState, PlayerIndex, StackSource, Target};

/// Resolve the top entry on the stack.
pub(super) fn resolve_top_of_stack(state: &mut GameState) {
    let entry = match state.stack.pop() {
        Some(e) => e,
        None => return,
    };

    match entry.source {
        StackSource::Spell(obj_id) => {
            resolve_spell(state, obj_id, &entry.targets, entry.controller);
        }
        StackSource::ActivatedAbility {
            source_id,
            ability_index,
        } => {
            resolve_activated_ability(state, source_id, ability_index, &entry.targets, entry.controller);
        }
        StackSource::TriggeredAbility {
            source_id,
            ability_index,
        } => {
            resolve_triggered_ability(state, source_id, ability_index, &entry.targets);
        }
    }

    // Check state-based actions after resolution
    super::sba::check_state_based_actions(state);
}

/// Resolve a spell.
fn resolve_spell(
    state: &mut GameState,
    obj_id: ObjectId,
    targets: &[Target],
    controller: PlayerIndex,
) {
    // Clone the card definition to avoid borrow conflict
    let def = {
        let db = state.card_db();
        let inst = &state.objects[&obj_id];
        db.get(inst.card_def_id).unwrap().clone()
    };

    if def.is_creature() || def.card_types.contains(&CardType::Artifact)
        || def.card_types.contains(&CardType::Enchantment)
        || def.card_types.contains(&CardType::Planeswalker)
    {
        state.move_object(obj_id, ZoneType::Stack, ZoneType::Battlefield);
        if let Some(inst) = state.objects.get_mut(&obj_id) {
            inst.controller = controller;
        }

        // Planeswalker: enter with starting loyalty counters
        if def.card_types.contains(&CardType::Planeswalker) {
            if let Some(loyalty) = def.starting_loyalty {
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    inst.loyalty_counters = loyalty;
                }
            }
        }

        // Aura attachment: when an aura spell resolves, attach it to its target
        if def.is_aura() {
            if let Some(Target::Object(target_id)) = targets.first() {
                // Set attachment relationship
                if let Some(aura_inst) = state.objects.get_mut(&obj_id) {
                    aura_inst.attached_to = Some(*target_id);
                }
                if let Some(target_inst) = state.objects.get_mut(target_id) {
                    if !target_inst.attachments.contains(&obj_id) {
                        target_inst.attachments.push(obj_id);
                    }
                }
            }
        }

        // Apply ETB replacement effects (CR 614): enters tapped, enters with counters, etc.
        state.apply_etb_replacements(obj_id);

        // Refresh continuous effects when a permanent enters the battlefield.
        state.refresh_continuous_effects();

        // Queue ETB triggered abilities
        let _ = super::triggers::fire_triggers(state, TriggerCondition::EntersBattlefield, Some(obj_id));
        // Fire "whenever a creature enters" watcher triggers on other permanents
        if def.is_creature() {
            super::triggers::check_triggers(state, TriggerCondition::ACreatureEnters, None);
            let _ = super::triggers::flush_triggers(state);
        }
    } else {
        if let Some(ref effect) = def.spell_effect {
            super::effects::resolve_effect(state, effect, controller, targets, Some(obj_id));
        }
        // Commander redirect: non-permanent commander spells go to command zone
        if state.is_commander(obj_id) {
            state.move_object(obj_id, ZoneType::Stack, ZoneType::Command);
        } else if def.flashback_cost.is_some() || def.escape_exile_count.is_some() {
            // Flashback/Escape: exile after resolution instead of graveyard
            state.move_object(obj_id, ZoneType::Stack, ZoneType::Exile);
        } else {
            state.move_object(obj_id, ZoneType::Stack, ZoneType::Graveyard);
        }
    }
}

/// Resolve an activated ability.
fn resolve_activated_ability(
    state: &mut GameState,
    source_id: ObjectId,
    ability_index: usize,
    targets: &[Target],
    controller: PlayerIndex,
) {
    let effect = {
        let db = state.card_db();
        let inst = &state.objects[&source_id];
        let def = db.get(inst.card_def_id).unwrap();
        // Check activated abilities first, then loyalty abilities
        def.activated_abilities.get(ability_index)
            .map(|a| a.effect.clone())
            .or_else(|| def.loyalty_abilities.get(ability_index).map(|a| a.effect.clone()))
    };

    if let Some(effect) = effect {
        super::effects::resolve_effect(state, &effect, controller, targets, Some(source_id));
    }
}

/// Resolve a triggered ability.
fn resolve_triggered_ability(
    state: &mut GameState,
    source_id: ObjectId,
    ability_index: usize,
    targets: &[Target],
) {
    let info = {
        let db = state.card_db();
        state.objects.get(&source_id).and_then(|inst| {
            let def = db.get(inst.card_def_id)?;
            let effect = def.triggered_abilities.get(ability_index).map(|a| a.effect.clone())?;
            Some((effect, inst.controller))
        })
    };

    if let Some((effect, controller)) = info {
        super::effects::resolve_effect(state, &effect, controller, targets, Some(source_id));
    }
}

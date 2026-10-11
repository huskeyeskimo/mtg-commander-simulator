//! Narrow synchronous physical Aura resolution; no persisted preparation state.
use super::{sba::PreparedPassFailure, transitions::TransitionError};
use crate::{
    card::{
        CardType, ComputedEnchant, EnchantRestriction, PreparedAuraAttachment, TriggerCondition,
        ZoneType,
    },
    events::{GameEvent, Zone},
    game::{GameState, StackEntry, Target},
};

/// Aura checks precede both top-level accounting and nested action dispatch.
pub(super) fn preflight_action(
    state: &GameState,
    action: &crate::action::Action,
) -> Result<(), PreparedPassFailure> {
    use crate::{action::Action, game::StackSource};
    let invalid = PreparedPassFailure::StateEncoding;
    crate::targeting::validate_pending_auras(state).map_err(|_| invalid.clone())?;
    if matches!(action, crate::action::Action::ChooseTutorTarget { .. })
        && state
            .pending_tutor
            .as_ref()
            .is_some_and(|pending| pending.controller >= state.players.len())
    {
        return Err(invalid);
    }
    if let Action::CastSpell { object_id, targets }
    | Action::CastCommander { object_id, targets }
    | Action::CastFromGraveyard { object_id, targets } = action
    {
        if let Some(inst) = state.objects.get(object_id) {
            if let Some(def) = state.card_db().get(inst.card_def_id).filter(|def| {
                !def.is_aura()
                    && state.has_prospective_aura_effect()
                    && def.card_types.iter().any(|kind| {
                        matches!(
                            kind,
                            CardType::Creature
                                | CardType::Artifact
                                | CardType::Enchantment
                                | CardType::Planeswalker
                        )
                    })
            }) {
                let mut future = inst.clone();
                future.controller = state.priority_player;
                future.zone_change_count = future
                    .zone_change_count
                    .checked_add(2)
                    .ok_or(invalid.clone())?;
                if state.prospective_aura(future, def).map_err(entry_failure)? {
                    return Err(PreparedPassFailure::Transition(
                        TransitionError::UnsupportedPath,
                    ));
                }
            }
        }
        if state
            .objects
            .get(object_id)
            .and_then(|inst| state.card_db().get(inst.card_def_id))
            .is_some_and(|def| def.is_aura())
        {
            crate::targeting::preflight_aura_cast(state, *object_id, targets)
                .map_err(|_| invalid.clone())?;
        }
    }
    if (matches!(action, Action::PassPriority)
        && state.pending_tutor.is_none()
        && state.pending_copy_order.is_none()
        && !super::cleanup_discard_required(state)
        && super::mandatory_trigger_order_chooser(state).is_none()
        && state.consecutive_passes.saturating_add(1) >= state.players.len() as u32)
        || matches!(action, Action::EndTurn)
    {
        if let Some(entry) = state
            .stack
            .last()
            .filter(|entry| matches!(entry.source, StackSource::AuraSpell { .. }))
        {
            prepare_resolution(state, entry)?;
        }
        if let Some(entry) = state.stack.last() {
            if let StackSource::Spell(id) = entry.source {
                if state
                    .objects
                    .get(&id)
                    .and_then(|inst| state.card_db().get(inst.card_def_id))
                    .is_some_and(|def| {
                        def.card_types.iter().any(|kind| {
                            matches!(
                                kind,
                                CardType::Creature
                                    | CardType::Artifact
                                    | CardType::Enchantment
                                    | CardType::Planeswalker
                            )
                        })
                    })
                {
                    state
                        .preflight_noncast_entry_for_controller(
                            id,
                            ZoneType::Battlefield,
                            entry.controller,
                        )
                        .map_err(entry_failure)?;
                }
            }
        }
    }
    if let Action::ChooseTutorTarget { card_id } = action {
        if let Some(pending) = state.pending_tutor.as_ref().filter(|pending| {
            pending.controller == state.priority_player && state.pending_copy_order.is_none()
        }) {
            if let Some(&id) = state.players[pending.controller]
                .library
                .iter()
                .find(|&&id| state.is_card(id) && state.objects[&id].card_def_id == *card_id)
            {
                state
                    .preflight_noncast_entry(id, pending.destination)
                    .map_err(entry_failure)?;
            }
        }
    }
    Ok(())
}

pub(super) struct PreparedAuraResolution {
    source: u64,
    controller: usize,
    attached: Option<PreparedAuraAttachment>,
    plus_counters: i32,
    tapped: bool,
    destination: ZoneType,
    refresh: Option<crate::game::PreparedContinuousRefresh>,
}

pub(super) fn prepare_resolution(
    state: &GameState,
    entry: &StackEntry,
) -> Result<PreparedAuraResolution, PreparedPassFailure> {
    let invalid = PreparedPassFailure::StateEncoding;
    crate::targeting::validate_pending_auras(state).map_err(|_| invalid.clone())?;
    let Some((source, target)) =
        crate::targeting::aura_entry_endpoints(entry, state.card_db(), state.players.len(), |id| {
            state.objects.get(&id)
        })
        .map_err(|_| invalid.clone())?
    else {
        return Err(invalid);
    };
    state
        .validate_attachment_structure()
        .map_err(|_| invalid.clone())?;
    let inst = &state.objects[&source.id];
    if state.exact_object(target.id) == Some(target) && target.generation == u32::MAX {
        return Err(invalid);
    }
    source
        .generation
        .checked_add(1)
        .ok_or_else(|| invalid.clone())?;
    state
        .pending_events
        .len()
        .checked_add(1)
        .ok_or_else(|| invalid.clone())?;
    // Reserve the represented event-group counter too, without allocating a group.
    state
        .next_zone_event_group_id
        .checked_add(1)
        .ok_or_else(|| invalid.clone())?;
    let def = state
        .card_db()
        .get(inst.card_def_id)
        .ok_or_else(|| invalid.clone())?;
    let chars = state
        .get_characteristics(source.id)
        .ok_or_else(|| invalid.clone())?;
    let legal = state.exact_object(target.id) == Some(target)
        && crate::targeting::target_is_legal(
            state,
            entry.controller,
            &crate::card::TargetSpec::AnyCreature,
            &Target::Object(target.id),
        )
        && chars.card_types.contains(&CardType::Enchantment)
        && chars.subtypes.iter().any(|s| s.0 == "Aura")
        && !chars.card_types.contains(&CardType::Creature)
        && chars.enchant == ComputedEnchant::Restriction(EnchantRestriction::Creature);
    if !legal {
        let destination = if state.is_commander_format() && state.is_commander(source.id) {
            ZoneType::Command
        } else {
            ZoneType::Graveyard
        };
        let zone = if destination == ZoneType::Command {
            &state.players[inst.owner].command_zone
        } else {
            &state.players[inst.owner].graveyard
        };
        zone.len().checked_add(1).ok_or_else(|| invalid.clone())?;
        return Ok(PreparedAuraResolution {
            source: source.id,
            controller: entry.controller,
            attached: None,
            plus_counters: 0,
            tapped: false,
            destination,
            refresh: None,
        });
    }
    state
        .battlefield
        .len()
        .checked_add(1)
        .ok_or_else(|| invalid.clone())?;
    // No represented definition static changes control. Existing control-layer
    // records may become active on entry, so validate their player endpoints.
    if state.continuous_effects.iter().any(|effect| {
        matches!(effect.modification,
        crate::layers::LayerModification::ChangeController(player) if player >= state.players.len())
    }) {
        return Err(invalid);
    }
    // Represented ETB self/watch occurrences may allocate stack entries at
    // settlement. Reserve their upper bound before consuming the physical spell.
    let mut trigger_capacity = state.pending_triggers.len();
    for &id in state.battlefield.iter().chain(std::iter::once(&source.id)) {
        let definition = state
            .objects
            .get(&id)
            .and_then(|inst| state.card_db().get(inst.card_def_id))
            .ok_or_else(|| invalid.clone())?;
        let instance = &state.objects[&id];
        if instance.owner >= state.players.len()
            || instance.controller >= state.players.len()
            || state
                .get_characteristics(id)
                .is_none_or(|chars| chars.controller >= state.players.len())
        {
            return Err(invalid);
        }
        let count = definition
            .triggered_abilities
            .iter()
            .filter(|ability| {
                (id == source.id && ability.trigger == TriggerCondition::EntersBattlefield)
                    || ability.trigger == TriggerCondition::ACreatureEnters
            })
            .count();
        trigger_capacity = trigger_capacity
            .checked_add(count)
            .ok_or_else(|| invalid.clone())?;
    }
    state
        .next_stack_id
        .checked_add(u64::try_from(trigger_capacity).map_err(|_| invalid.clone())?)
        .ok_or_else(|| invalid.clone())?;
    let attached = state
        .prepare_aura_attachment(source, target)
        .map_err(|_| invalid.clone())?;
    let mut plus_counters = 0i32;
    let mut tapped = def.enters_tapped;
    let (replacements, _) = crate::replacement::find_applicable_replacements(
        &state.replacement_effects,
        &crate::replacement::ReplacementEventKind::EntersBattlefield,
        entry.controller,
    );
    for index in replacements {
        if let crate::replacement::ReplacementAction::EntersModified {
            enters_tapped,
            extra_counters,
        } = state.replacement_effects[index].action
        {
            tapped |= enters_tapped;
            if extra_counters > 0 {
                plus_counters = plus_counters
                    .checked_add(extra_counters)
                    .ok_or_else(|| invalid.clone())?;
            }
        }
    }
    let mut projection = crate::game::RefreshProjection::new(state);
    let mut future = inst.clone();
    future.zone_change_count += 1;
    future.controller = entry.controller;
    future.plus_counters = plus_counters;
    future.minus_counters = 0;
    future.loyalty_counters = 0;
    future.loyalty_activated_this_turn = false;
    future.exiled_by = None;
    future.tapped = tapped;
    future.summoning_sick = true;
    future.damage_marked = 0;
    future.temp_power_mod = 0;
    future.temp_toughness_mod = 0;
    future.temp_keywords.clear();
    projection.put(future);
    projection.battlefield.push(source.id);
    state
        .project_aura_attachment(&attached, &mut projection)
        .map_err(|_| invalid.clone())?;
    let refresh = projection.refresh(state)?;
    Ok(PreparedAuraResolution {
        source: source.id,
        controller: entry.controller,
        attached: Some(attached),
        plus_counters,
        tapped,
        destination: ZoneType::Battlefield,
        refresh: Some(refresh),
    })
}

pub(super) fn commit_resolution(state: &mut GameState, prepared: PreparedAuraResolution) {
    let id = prepared.source;
    let to = prepared.destination;
    state.move_prepared_aura(id, to);
    if let Some(attachment) = prepared.attached {
        let inst = state.objects.get_mut(&id).unwrap();
        inst.controller = prepared.controller;
        inst.plus_counters = prepared.plus_counters;
        inst.minus_counters = 0;
        inst.loyalty_counters = 0;
        inst.loyalty_activated_this_turn = false;
        inst.exiled_by = None;
        inst.tapped = prepared.tapped;
        state.commit_aura_attachment(attachment);
        prepared.refresh.unwrap().commit(state);
        state.refresh_replacement_effects();
        state.invalidate_characteristics_cache();
        state.emit_event(GameEvent::ZoneChange {
            object: id,
            from: Zone::Stack,
            to: Zone::Battlefield,
        });
        super::triggers::check_aura_entry_triggers(state, id);
    } else {
        state.emit_event(GameEvent::ZoneChange {
            object: id,
            from: Zone::Stack,
            to: Zone::from(to),
        });
    }
}

fn entry_failure(error: TransitionError) -> PreparedPassFailure {
    if error == TransitionError::UnsupportedPath {
        PreparedPassFailure::Transition(error)
    } else {
        PreparedPassFailure::StateEncoding
    }
}

pub(crate) fn record_entry_failure(state: &mut GameState, error: TransitionError) {
    state.sba_failure = Some(entry_failure(error));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        action::{legal_actions, Action},
        card::{CardDef, Subtype},
        game::{CardDatabase, Phase},
        mana::ManaCost,
    };
    use std::sync::Arc;
    #[test]
    fn nested_counted_boundary_keeps_aura_preparation_failure_atomic() {
        let mut db = CardDatabase::new();
        db.insert(CardDef {
            id: 979010,
            card_types: vec![CardType::Enchantment],
            subtypes: vec![Subtype("Aura".into())],
            enchant: EnchantRestriction::Creature,
            mana_cost: Some(ManaCost::zero()),
            ..Default::default()
        });
        db.insert(CardDef {
            id: 979011,
            card_types: vec![CardType::Creature],
            power: Some(2),
            toughness: Some(2),
            ..Default::default()
        });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::PreCombatMain;
        let a = state
            .create_card_in_zone(979010, 0, ZoneType::Hand)
            .unwrap();
        let target = state
            .create_card_in_zone(979011, 0, ZoneType::Battlefield)
            .unwrap();
        crate::rules::apply_action(
            &mut state,
            &Action::CastSpell {
                object_id: a,
                targets: vec![Target::Object(target)],
            },
        );
        assert_eq!(state.stack.len(), 1);
        crate::rules::apply_action(&mut state, &Action::PassPriority);
        state.next_timestamp = u32::MAX;
        state.loss_action_in_progress = true;
        let before = bincode::serialize(&state).unwrap();
        let actions = legal_actions(&state);
        assert_eq!(
            crate::simulation::apply_counted_action(&mut state, &Action::PassPriority, &actions),
            Err(crate::simulation::TerminationReason::StateEncoding)
        );
        assert!(state.loss_action_in_progress);
        state.sba_failure = None;
        assert_eq!(bincode::serialize(&state).unwrap(), before);
    }
}

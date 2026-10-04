//! Direct creation of independent instant and sorcery spell copies.
use crate::card::effects::{Condition, DynamicValue};
use crate::card::{CardType, Effect};
use crate::game::{CastSpellSnapshot, CopyTargetDescription, GameState, ObservablePermanentKey, PendingCopyOrder, PlayerIndex, PreparedSpellCopy, StackEntry, StackId, StackSource, Target};
use crate::targeting::{self, SpellTargeting};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyError {
    InvalidController,
    MissingSource,
    SourceIsAbility,
    MissingDefinition,
    UnsupportedSpell,
    UnsupportedEffect,
    InvalidTargets,
    StackIdExhausted,
    PendingOrder,
    MixedControllers,
    NonTerminalBatch,
    InvalidOrder,
    ConflictingPendingChoice,
    InvalidPendingState,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyTargetPolicy {
    Preserve,
    Replace(Vec<Target>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CopyBatchOutcome {
    Empty,
    Committed(Vec<StackId>),
    Pending,
}

/// Put a copy directly onto the stack. Every fallible check precedes mutation.
pub fn copy_stack_spell(
    state: &mut GameState,
    source_stack_id: StackId,
    copy_controller: PlayerIndex,
    target_policy: CopyTargetPolicy,
) -> Result<StackId, CopyError> {
    if copy_controller >= state.players.len() || state.players[copy_controller].has_lost {
        return Err(CopyError::InvalidController);
    }
    let snapshot = snapshot_stack_spell(state, source_stack_id)?;
    copy_spell_snapshot(state, &snapshot, copy_controller, target_policy)
}

/// Own the spell information while its stack entry still exists. The result
/// remains usable after the source card and stack entry leave their zones.
pub fn snapshot_stack_spell(state: &GameState, source_stack_id: StackId) -> Result<CastSpellSnapshot, CopyError> {
    let source = state.stack.iter().find(|entry| entry.id == source_stack_id)
        .ok_or(CopyError::MissingSource)?;
    let definition = match &source.source {
        StackSource::Spell(object_id) => {
            let instance = state.objects.get(object_id).ok_or(CopyError::MissingDefinition)?;
            state.card_db.as_ref().and_then(|db| db.get(instance.card_def_id))
                .ok_or(CopyError::MissingDefinition)?.clone()
        }
        StackSource::SpellCopy { definition } => (**definition).clone(),
        StackSource::ActivatedAbility { .. } | StackSource::TriggeredAbility { .. } => {
            return Err(CopyError::SourceIsAbility);
        }
    };
    Ok(CastSpellSnapshot {
        stack_id: source.id,
        definition: Box::new(definition),
        controller: source.controller,
        targets: source.targets.clone(),
        target_generations: source.target_generations.clone(),
        historical_object_targets: source.targets.iter().map(|target| match target {
            Target::Object(id) => state.objects.get(id).map(|inst| (inst.card_def_id, inst.controller)),
            Target::Player(_) | Target::StackEntry(_) => None,
        }).collect(),
    })
}

/// Put an independent copy on the stack from retained spell information.
/// Validation and construction are shared with live-source copying.
pub fn copy_spell_snapshot(
    state: &mut GameState,
    source: &CastSpellSnapshot,
    copy_controller: PlayerIndex,
    target_policy: CopyTargetPolicy,
) -> Result<StackId, CopyError> {
    if state.pending_copy_order.is_some() { return Err(CopyError::PendingOrder); }
    let prepared = prepare_spell_copy(state, source, copy_controller, target_policy)?;
    let ids = materialize_batch(state, &[prepared], &[0])?;
    Ok(ids[0])
}

/// Read-only validation and capture. A caller supplying a batch must give its
/// items in a deterministic semantic order: vector indices are the choices.
pub fn prepare_spell_copy(
    state: &GameState,
    source: &CastSpellSnapshot,
    copy_controller: PlayerIndex,
    target_policy: CopyTargetPolicy,
) -> Result<PreparedSpellCopy, CopyError> {
    if state.pending_copy_order.is_some() { return Err(CopyError::PendingOrder); }
    if copy_controller >= state.players.len() || state.players[copy_controller].has_lost {
        return Err(CopyError::InvalidController);
    }
    let definition = &*source.definition;
    if !definition
        .card_types
        .iter()
        .any(|kind| matches!(kind, CardType::Instant | CardType::Sorcery))
        || definition.card_types.iter().any(|kind| {
            matches!(
                kind,
                CardType::Creature
                    | CardType::Artifact
                    | CardType::Enchantment
                    | CardType::Planeswalker
                    | CardType::Land
            )
        })
    {
        return Err(CopyError::UnsupportedSpell);
    }
    if definition
        .spell_effect
        .as_ref()
        .is_some_and(|effect| !copy_safe(effect))
    {
        return Err(CopyError::UnsupportedEffect);
    }
    let targeting = targeting::spell_targeting(&definition);
    if matches!(targeting, SpellTargeting::Unsupported) {
        return Err(CopyError::UnsupportedSpell);
    }
    let (targets, target_generations) = match target_policy {
        CopyTargetPolicy::Preserve => {
            let shape_ok = match targeting {
                SpellTargeting::Untargeted => source.targets.is_empty(),
                SpellTargeting::Single(_) => source.targets.len() == 1,
                SpellTargeting::Unsupported => false,
            };
            if !shape_ok
                || source.target_generations.len() != source.targets.len()
                || source.targets.iter().zip(&source.target_generations).any(
                    |(target, generation)| {
                        let generation_shape_invalid = match target {
                            Target::Object(_) => generation.is_none(),
                            Target::Player(_) | Target::StackEntry(_) => generation.is_some(),
                        };
                        let target_kind_invalid = match &targeting {
                            SpellTargeting::Single(spec) => !target_kind_matches(spec, target),
                            SpellTargeting::Untargeted | SpellTargeting::Unsupported => true,
                        };
                        generation_shape_invalid || target_kind_invalid
                    },
                )
            {
                return Err(CopyError::InvalidTargets);
            }
            (source.targets.clone(), source.target_generations.clone())
        }
        CopyTargetPolicy::Replace(targets) => {
            if !targeting::valid_spell_targets(state, copy_controller, definition, &targets) {
                return Err(CopyError::InvalidTargets);
            }
            let generations = targeting::target_generations(state, &targets);
            (targets, generations)
        }
    };
    let target_descriptions = targets.iter().enumerate().map(|(index, target)| match target {
        Target::Player(player) => CopyTargetDescription::Player(*player),
        Target::StackEntry(id) => CopyTargetDescription::StackPosition(
            state.stack.iter().position(|entry| entry.id == *id)),
        Target::Object(id) => {
            let current = state.battlefield.contains(id)
                && state.objects.get(id).is_some_and(|inst| Some(inst.zone_change_count) == target_generations[index]);
            let historical = source.historical_object_targets.get(index).copied().flatten();
            let live_identity = state.objects.get(id).map(|inst| (inst.card_def_id, inst.controller));
            let (card_id, controller) = (if current { live_identity.or(historical) }
                else { historical.or(live_identity) })
                .map(|(card, player)| (Some(card), Some(player)))
                .unwrap_or((None, None));
            let observable = if current { observable_permanent_key(state, *id) } else { None };
            CopyTargetDescription::Permanent {
                card_id, controller,
                observable,
                // The batch assigns occurrence numbers by first target use,
                // preserving same-vs-different identity without raw ID order.
                occurrence: None,
                same_incarnation: current,
            }
        }
    }).collect();
    Ok(PreparedSpellCopy {
        definition: source.definition.clone(), controller: copy_controller,
        targets, target_generations, target_descriptions,
        definition_description: serde_json::to_string(&source.definition).expect("CardDef serializes"),
    })
}

fn observable_permanent_key(state: &GameState, id: crate::card::ObjectId) -> Option<ObservablePermanentKey> {
    let inst = state.objects.get(&id)?;
    let characteristics = format!("{:?}", state.get_characteristics(id)?);
    Some(ObservablePermanentKey {
        card_id: inst.card_def_id, owner: inst.owner, controller: inst.controller,
        characteristics, tapped: inst.tapped, damage_marked: inst.damage_marked,
        summoning_sick: inst.summoning_sick, plus_counters: inst.plus_counters,
        minus_counters: inst.minus_counters, loyalty_counters: inst.loyalty_counters,
        loyalty_activated_this_turn: inst.loyalty_activated_this_turn,
        is_token: inst.is_token, attached: inst.attached_to.is_some(),
        attachment_count: inst.attachments.len(),
    })
}

fn canonicalize_pending_target_occurrences(items: &mut [PreparedSpellCopy]) {
    let mut seen: std::collections::HashMap<ObservablePermanentKey, Vec<crate::card::ObjectId>> =
        std::collections::HashMap::new();
    for item in items {
        for (target, description) in item.targets.iter().zip(&mut item.target_descriptions) {
            if let (Target::Object(id), CopyTargetDescription::Permanent {
                observable: Some(key), occurrence, same_incarnation: true, ..
            }) = (target, description) {
                let objects = seen.entry(key.clone()).or_default();
                let index = objects.iter().position(|other| other == id).unwrap_or_else(|| {
                    objects.push(*id);
                    objects.len() - 1
                });
                *occurrence = Some(index);
            }
        }
    }
}

/// Begin a terminal operation. No copy is materialized until every choice is
/// known. The caller must prove the operation is the final resolving effect.
pub fn begin_terminal_copy_batch(
    state: &mut GameState,
    mut items: Vec<PreparedSpellCopy>,
    terminal: bool,
) -> Result<CopyBatchOutcome, CopyError> {
    if !terminal || state.effect_terminal_position == Some(false) {
        return Err(CopyError::NonTerminalBatch);
    }
    if state.pending_copy_order.is_some() { return Err(CopyError::PendingOrder); }
    if state.pending_tutor.is_some() { return Err(CopyError::ConflictingPendingChoice); }
    if items.is_empty() { return Ok(CopyBatchOutcome::Empty); }
    let controller = items[0].controller;
    if controller >= state.players.len() || state.players[controller].has_lost {
        return Err(CopyError::InvalidController);
    }
    if items.iter().any(|item| item.controller != controller) {
        return Err(CopyError::MixedControllers);
    }
    if items.iter().any(|item| item.target_descriptions.len() != item.targets.len()) {
        return Err(CopyError::InvalidTargets);
    }
    let count = u64::try_from(items.len()).map_err(|_| CopyError::StackIdExhausted)?;
    state.next_stack_id.checked_add(count)
        .ok_or(CopyError::StackIdExhausted)?;
    canonicalize_pending_target_occurrences(&mut items);
    if items.len() == 1 {
        check_completion_capacity(state, 1)?;
        return materialize_batch(state, &items, &[0]).map(CopyBatchOutcome::Committed);
    }
    state.pending_copy_order = Some(PendingCopyOrder {
        controller, items, selected_order: Vec::new(),
        expected_stack_len: state.stack.len(),
        expected_next_stack_id: state.next_stack_id,
        resolving_entry: None,
        resolving_source_generation: None,
    });
    state.priority_player = controller;
    Ok(CopyBatchOutcome::Pending)
}

fn materialize_batch(state: &mut GameState, items: &[PreparedSpellCopy], order: &[usize]) -> Result<Vec<StackId>, CopyError> {
    if order.len() != items.len() || {
        let mut sorted = order.to_vec(); sorted.sort_unstable();
        sorted != (0..items.len()).collect::<Vec<_>>()
    } { return Err(CopyError::InvalidOrder); }
    let count = u64::try_from(items.len()).map_err(|_| CopyError::StackIdExhausted)?;
    let next = state.next_stack_id.checked_add(count).ok_or(CopyError::StackIdExhausted)?;
    let ids: Vec<_> = (state.next_stack_id..next).collect();
    let entries: Vec<_> = order.iter().enumerate().map(|(position, index)| {
        let item = &items[*index];
        StackEntry { id: ids[position], source: StackSource::SpellCopy { definition: item.definition.clone() },
            controller: item.controller, targets: item.targets.clone(),
            target_generations: item.target_generations.clone() }
    }).collect();
    state.stack.extend(entries);
    state.next_stack_id = next;
    Ok(ids)
}

/// Reserve room for copies and the trigger work that immediate completion can
/// put onto the stack, before either path changes the stack or allocator.
fn check_completion_capacity(state: &GameState, copies: usize) -> Result<(), CopyError> {
    let battlefield_count = state.battlefield.len() as u128;
    let abilities: u128 = state.battlefield.iter().filter_map(|id| state.objects.get(id))
        .filter_map(|inst| state.card_db.as_ref().and_then(|db| db.get(inst.card_def_id)))
        .map(|def| def.triggered_abilities.len() as u128).sum();
    let required = copies as u128 + state.pending_triggers.len() as u128
        + battlefield_count.saturating_mul(abilities.saturating_add(1));
    if (state.next_stack_id as u128).saturating_add(required) > u64::MAX as u128 {
        return Err(CopyError::StackIdExhausted);
    }
    Ok(())
}

/// Append one copy to the bottom-to-top prefix. The final remaining item is
/// automatic; commitment inserts the whole ordered batch in one operation.
pub(crate) fn choose_next_copy(state: &mut GameState, item_index: usize) -> Result<(), CopyError> {
    let pending = state.pending_copy_order.as_ref().ok_or(CopyError::PendingOrder)?;
    if pending.controller >= state.players.len() || state.players[pending.controller].has_lost
        || pending.items.iter().any(|item| item.controller != pending.controller)
        || state.pending_tutor.is_some() {
        return Err(CopyError::InvalidPendingState);
    }
    let mut seen = vec![false; pending.items.len()];
    if pending.items.len() < 2 || pending.selected_order.len() >= pending.items.len()
        || pending.selected_order.iter().any(|&index| {
            if index >= seen.len() || seen[index] { true } else { seen[index] = true; false }
        }) { return Err(CopyError::InvalidOrder); }
    if state.priority_player != pending.controller || item_index >= pending.items.len()
        || pending.selected_order.contains(&item_index) { return Err(CopyError::InvalidOrder); }
    let mut order = pending.selected_order.clone();
    order.push(item_index);
    if order.len() + 1 == pending.items.len() {
        order.push((0..pending.items.len()).find(|index| !order.contains(index)).unwrap());
    }
    if order.len() == pending.items.len() {
        if state.stack.len() != pending.expected_stack_len || state.next_stack_id != pending.expected_next_stack_id {
            return Err(CopyError::InvalidOrder);
        }
        if let Some(entry) = pending.resolving_entry.as_ref() {
            if state.stack.iter().any(|other| other.id == entry.id) {
                return Err(CopyError::InvalidPendingState);
            }
            if let StackSource::Spell(id) = entry.source {
                let Some(inst) = state.objects.get(&id) else { return Err(CopyError::InvalidPendingState); };
                if state.card_db.as_ref().and_then(|db| db.get(inst.card_def_id)).is_none()
                    || pending.resolving_source_generation != Some(inst.zone_change_count)
                    || state.battlefield.contains(&id)
                    || state.players.iter().any(|player| player.library.contains(&id)
                        || player.hand.contains(&id) || player.graveyard.contains(&id)
                        || player.exile.contains(&id) || player.command_zone.contains(&id)) {
                    return Err(CopyError::InvalidPendingState);
                }
            }
        }
        // Copy insertion can be followed immediately by queued triggers and
        // SBA-generated dies triggers. Reserve a conservative upper bound before
        // changing either the stack or the pending decision.
        check_completion_capacity(state, pending.items.len())?;
        let items = pending.items.clone();
        let resolving_entry = pending.resolving_entry.clone();
        let _ = materialize_batch(state, &items, &order)?;
        state.pending_copy_order = None;
        // A restored copy-order state has no active Rust resolution frame.
        // Re-enter its settlement window before deferred cleanup and SBA work;
        // complete_stack_resolution releases this gate after stabilization.
        state.trigger_placement_deferred = true;
        if let Some(entry) = resolving_entry {
            super::resolution::finish_deferred_resolution(state, &entry);
        }
        state.consecutive_passes = 0;
        super::complete_stack_resolution(state);
    } else {
        state.pending_copy_order.as_mut().unwrap().selected_order = order;
    }
    Ok(())
}

/// Validate only the representation permitted by a target specification.
/// Preserve deliberately does not check current existence or targeting legality.
fn target_kind_matches(spec: &crate::card::TargetSpec, target: &Target) -> bool {
    use crate::card::TargetSpec;
    match (spec, target) {
        (TargetSpec::AnySpell, Target::StackEntry(_)) => true,
        (TargetSpec::AnyPlayer | TargetSpec::Opponent, Target::Player(_)) => true,
        (TargetSpec::CreatureOrPlayer, Target::Object(_) | Target::Player(_)) => true,
        (
            TargetSpec::AnyCreature
            | TargetSpec::CreatureOrPlaneswalker
            | TargetSpec::AnyNonlandPermanent
            | TargetSpec::AnyPermanent,
            Target::Object(_),
        ) => true,
        _ => false,
    }
}

#[cfg(test)]
mod terminal_position_tests {
    use super::*;
    use std::sync::Arc;
    use crate::action::Action;
    use crate::card::{CardDef, ZoneType};
    use crate::game::{CardDatabase, Phase};

    #[test]
    fn recursive_nonterminal_position_overrides_caller_claim() {
        let mut state = GameState::new(2);
        state.effect_terminal_position = Some(false);
        let before = serde_json::to_value(&state).unwrap();
        assert_eq!(begin_terminal_copy_batch(&mut state, vec![], true),
            Err(CopyError::NonTerminalBatch));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }

    #[test]
    fn physical_spell_finishes_only_after_atomic_batch_commit() {
        let definition = CardDef { id: 982002, name: "Original".into(),
            card_types: vec![CardType::Instant],
            spell_effect: Some(Effect::DrawCards { count: 1 }), ..Default::default() };
        let mut db = CardDatabase::new();
        db.insert(definition);
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        let object = state.create_card_in_zone(982002, 0, ZoneType::Hand);
        super::super::apply_action(&mut state, &Action::CastSpell { object_id: object, targets: vec![] });
        let snapshot = snapshot_stack_spell(&state, state.stack[0].id).unwrap();
        let entry = state.stack.pop().unwrap();
        let item = prepare_spell_copy(&state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
        assert_eq!(begin_terminal_copy_batch(&mut state, vec![item.clone(), item], true),
            Ok(CopyBatchOutcome::Pending));
        state.pending_copy_order.as_mut().unwrap().resolving_entry = Some(Box::new(entry));
        state.pending_copy_order.as_mut().unwrap().resolving_source_generation =
            state.objects.get(&object).map(|inst| inst.zone_change_count);
        assert!(!state.players[0].graveyard.contains(&object));
        let next = state.next_stack_id;
        let saved = state.snapshot();
        let json = serde_json::to_vec(&state).unwrap();
        let binary = bincode::serialize(&state).unwrap();
        let mut restored = vec![state.clone(),
            serde_json::from_slice::<GameState>(&json).unwrap(),
            bincode::deserialize::<GameState>(&binary).unwrap()];
        state.restore(saved);
        restored.push(state);
        for mut state in restored {
            if state.card_db.is_none() {
                let mut db = CardDatabase::new();
                db.insert((*snapshot.definition).clone());
                state.card_db = Some(Arc::new(db));
            }
            assert!(state.pending_copy_order.as_ref().unwrap().resolving_entry().is_some());
            super::super::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
            assert_eq!(state.players[0].graveyard.iter().filter(|&&id| id == object).count(), 1);
            assert_eq!(state.stack.iter().map(|entry| entry.id).collect::<Vec<_>>(), vec![next, next + 1]);
        }
    }

    #[test]
    fn malformed_restored_order_fails_without_partial_commit_or_fast_forward_panic() {
        let definition = CardDef { id: 982003, name: "Original".into(),
            card_types: vec![CardType::Instant],
            spell_effect: Some(Effect::DrawCards { count: 1 }), ..Default::default() };
        let mut db = CardDatabase::new();
        db.insert(definition);
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::PreCombatMain;
        let object = state.create_card_in_zone(982003, 0, ZoneType::Hand);
        super::super::apply_action(&mut state, &Action::CastSpell { object_id: object, targets: vec![] });
        let snapshot = snapshot_stack_spell(&state, state.stack[0].id).unwrap();
        let entry = state.stack.pop().unwrap();
        let item = prepare_spell_copy(&state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
        begin_terminal_copy_batch(&mut state, vec![item.clone(), item], true).unwrap();
        state.pending_copy_order.as_mut().unwrap().resolving_entry = Some(Box::new(entry));
        state.pending_copy_order.as_mut().unwrap().resolving_source_generation =
            state.objects.get(&object).map(|inst| inst.zone_change_count);

        let mut mismatched = state.clone();
        mismatched.pending_copy_order.as_mut().unwrap().items[1].controller = 1;
        let mut mismatched: GameState = serde_json::from_value(serde_json::to_value(&mismatched).unwrap()).unwrap();
        mismatched.card_db = state.card_db.clone();
        let before = serde_json::to_value(&mismatched).unwrap();
        assert_eq!(choose_next_copy(&mut mismatched, 0), Err(CopyError::InvalidPendingState));
        assert_eq!(serde_json::to_value(&mismatched).unwrap(), before);

        let mut complete_prefix = state.clone();
        complete_prefix.pending_copy_order.as_mut().unwrap().selected_order = vec![0, 1];
        let mut complete_prefix: GameState = serde_json::from_value(serde_json::to_value(&complete_prefix).unwrap()).unwrap();
        complete_prefix.card_db = state.card_db.clone();
        let before = serde_json::to_value(&complete_prefix).unwrap();
        assert_eq!(choose_next_copy(&mut complete_prefix, 0), Err(CopyError::InvalidOrder));
        assert_eq!(super::super::fast_forward_goldfish_turn(&mut complete_prefix), 0);
        assert_eq!(serde_json::to_value(&complete_prefix).unwrap(), before);

        let mut missing_source = state;
        missing_source.objects.remove(&object);
        let db = missing_source.card_db.clone();
        let mut missing_source: GameState = serde_json::from_value(serde_json::to_value(&missing_source).unwrap()).unwrap();
        missing_source.card_db = db;
        let before = serde_json::to_value(&missing_source).unwrap();
        assert_eq!(choose_next_copy(&mut missing_source, 0), Err(CopyError::InvalidPendingState));
        assert_eq!(serde_json::to_value(&missing_source).unwrap(), before);
    }

    #[test]
    fn queued_trigger_overflow_returns_error_before_any_copy_is_inserted() {
        let definition = CardDef { id: 982004, name: "Source".into(),
            card_types: vec![CardType::Instant],
            spell_effect: Some(Effect::GainLife { amount: 1 }), ..Default::default() };
        let mut state = GameState::new(2);
        state.phase = Phase::PreCombatMain;
        state.stack.push(StackEntry { id: 1,
            source: StackSource::SpellCopy { definition: Box::new(definition.clone()) },
            controller: 0, targets: vec![], target_generations: vec![] });
        let snapshot = CastSpellSnapshot { stack_id: 1, definition: Box::new(definition),
            controller: 0, targets: vec![], target_generations: vec![], historical_object_targets: vec![] };
        let item = prepare_spell_copy(&state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
        state.next_stack_id = u64::MAX - 2;
        begin_terminal_copy_batch(&mut state, vec![item.clone(), item], true).unwrap();
        state.pending_triggers.push(crate::game::PendingTrigger { source_id: 99, ability_index: 0,
            controller: 0, targets: vec![], context: crate::game::TriggerContext {
                source_card_id: 982004, source_generation: 0,
                effect: Effect::GainLife { amount: 1 }, cast_spell: None } });
        let before = serde_json::to_value(&state).unwrap();
        assert_eq!(choose_next_copy(&mut state, 0), Err(CopyError::StackIdExhausted));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
}

fn dynamic_safe(value: &DynamicValue) -> bool {
    match value {
        DynamicValue::ChargeCountersOnSource => false,
        DynamicValue::CardsInHand
        | DynamicValue::CreaturesControlled
        | DynamicValue::TotalPowerControlled
        | DynamicValue::TappedCreaturesControlled
        | DynamicValue::LandsControlled
        | DynamicValue::CreaturesInGraveyard
        | DynamicValue::AllPermanentsWithSubtype(_)
        | DynamicValue::PermanentsWithSubtype(_)
        | DynamicValue::CreaturesWithSubtype(_)
        | DynamicValue::DevotionTo(_)
        | DynamicValue::SwampsControlled
        | DynamicValue::Fixed(_)
        | DynamicValue::CardTypesInGraveyards => true,
    }
}

fn condition_safe(condition: &Condition) -> bool {
    match condition {
        Condition::SourceHasCounters => false,
        Condition::ControlCreatures
        | Condition::LifeAtOrAbove(_)
        | Condition::LifeAtOrBelow(_)
        | Condition::IsYourTurn
        | Condition::ControlNOrMore {
            count: _,
            card_type: _,
        }
        | Condition::Always
        | Condition::HandIsEmpty
        | Condition::ControlNOrMorePermanents { count: _ } => true,
    }
}

// Deliberately exhaustive: a new effect must be classified before copies can use it.
fn copy_safe(effect: &Effect) -> bool {
    use Effect::*;
    match effect {
        #[cfg(test)]
        TestCopyBatch { copies: _ } => false,
        ExileFromHandLinked | ReturnLinkedExileToHand | CreateTokenCopyOfSource
        | CopyCastSpellForOtherCreatures => false,
        DoublePowerUntilEOT { target: _ }
        | BuffOtherSubtype {
            subtype: _,
            amount: _,
            until_eot: _,
        } => false,
        Multiple(children) => children.iter().all(copy_safe),
        Conditional {
            condition,
            if_true,
            if_false,
        } => {
            condition_safe(condition)
                && matches!(
                    targeting::effect_targeting(if_true),
                    SpellTargeting::Untargeted
                )
                && if_false.as_ref().is_none_or(|effect| {
                    matches!(
                        targeting::effect_targeting(effect),
                        SpellTargeting::Untargeted
                    )
                })
                && copy_safe(if_true)
                && if_false.as_ref().is_none_or(|effect| copy_safe(effect))
        }
        ForEach { count, effect } => {
            dynamic_safe(count)
                && matches!(
                    targeting::effect_targeting(effect),
                    SpellTargeting::Untargeted
                )
                && copy_safe(effect)
        }
        // The existing engine has no recorded mode choice to carry onto a copy.
        Modal {
            choices: _,
            choose_count: _,
        } => false,
        CreateTokens { token: _, count } | AddDynamicMana { color: _, count } => {
            dynamic_safe(count)
        }
        LoseDynamicLife { amount, target: _ }
        | DealDynamicDamage { amount, target: _ }
        | GainDynamicLife { amount } => dynamic_safe(amount),
        DealDamage {
            amount: _,
            target: _,
        }
        | GainLife { amount: _ }
        | LoseLife {
            amount: _,
            target: _,
        }
        | DrawCards { count: _ }
        | DestroyTarget { target: _ }
        | ExileTarget { target: _ }
        | DestroyAll
        | BounceTo { zone: _, target: _ }
        | Buff {
            power: _,
            toughness: _,
            until_eot: _,
        }
        | Debuff {
            power: _,
            toughness: _,
            until_eot: _,
        }
        | DiscardCards {
            count: _,
            target: _,
        }
        | CreateToken(_)
        | Counter { target: _ }
        | PutCounters {
            count: _,
            target: _,
        }
        | MillCards {
            count: _,
            target: _,
        }
        | SacrificeCreatures {
            count: _,
            target: _,
        }
        | PreventCombatDamage
        | AddMana {
            color: _,
            amount: _,
        }
        | ExtraTurn
        | SkipPhase(_)
        | SearchLibrary {
            destination: _,
            subtype_filter: _,
        }
        | BounceAllNonlandOpponents
        | ReturnToTopOfLibrary { target: _ }
        | UntapTarget { target: _ }
        | ReturnFromGraveyardToBattlefield { target: _ }
        | ReturnFromGraveyardToHand { target: _ }
        | ExileFromGraveyard { target: _ }
        | ShuffleIntoLibrary { target: _ }
        | PutOnBottomOfLibrary { target: _ }
        | GainKeywordUntilEOT {
            keyword: _,
            target: _,
        }
        | SetPowerToughness {
            power: _,
            toughness: _,
            until_eot: _,
            target: _,
        }
        | GainControlUntilEOT { target: _ }
        | Fight { target: _ }
        | TapTarget { target: _ }
        | EachOpponentLosesLife { amount: _ }
        | EachOpponentDiscards { count: _ }
        | EachOpponentSacrifices { count: _ }
        | DrawThenDiscard {
            draw: _,
            discard: _,
            target: _,
        }
        | CreatePredefinedToken {
            token_type: _,
            count: _,
        }
        | Scry { count: _ }
        | Proliferate
        | ExtraLandDrop
        | Surveil { count: _ }
        | AddManaOfAnyColor { amount: _ }
        | CreateTokenFromDef { card_def_id: _ }
        | Unimplemented(_) => true,
    }
}

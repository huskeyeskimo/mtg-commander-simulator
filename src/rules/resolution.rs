use crate::card::{CardType, ObjectId, TriggerCondition, ZoneType};
use crate::game::{GameState, PlayerIndex, StackSource, Target};

/// Resolve the top entry on the stack.
pub(super) fn resolve_top_of_stack(state: &mut GameState) {
    if state.pending_copy_order.is_some() { return; }
    let entry = match state.stack.pop() {
        Some(e) => e,
        None => return,
    };
    state.trigger_order_resume = Some(crate::game::TriggerOrderResume::AfterResolution);
    state.trigger_placement_deferred = true;

    match &entry.source {
        StackSource::Spell(obj_id) => {
            resolve_spell(state, *obj_id, &entry.targets, &entry.target_generations, entry.controller);
        }
        StackSource::SpellCopy { definition } => {
            resolve_spell_effect(state, &definition, &entry.targets, &entry.target_generations,
                entry.controller, None);
        }
        StackSource::ActivatedAbility {
            source_id,
            ability_index,
        } => {
            resolve_activated_ability(state, *source_id, *ability_index, &entry.targets,
                &entry.target_generations, entry.controller);
        }
        StackSource::TriggeredAbility {
            source_id,
            ability_index: _,
            context,
        } => {
            resolve_triggered_ability(state, *source_id, context, &entry.targets,
                &entry.target_generations, entry.controller);
        }
    }

    // Check state-based actions after resolution
    if let Some(pending) = state.pending_copy_order.as_mut() {
        pending.resolving_source_generation = match entry.source {
            StackSource::Spell(id) => state.objects.get(&id).map(|inst| inst.zone_change_count),
            _ => None,
        };
        pending.resolving_entry = Some(Box::new(entry));
    } else {
        super::complete_stack_resolution(state);
    }
}

/// Resolve a spell.
fn resolve_spell(
    state: &mut GameState,
    obj_id: ObjectId,
    targets: &[Target],
    generations: &[Option<u32>],
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
        // CR 608.2b: an illegal sole target stops the whole spell, including
        // untargeted instructions such as drawing. Check only before execution.
        resolve_spell_effect(state, &def, targets, generations, controller, Some(obj_id));
        if state.pending_copy_order.is_some() { return; }
        finish_physical_spell(state, obj_id, &def);
    }
}

/// Complete movement of a physical spell after any terminal ordering choice.
pub(super) fn finish_deferred_resolution(state: &mut GameState, entry: &crate::game::StackEntry) {
    if let StackSource::Spell(obj_id) = entry.source {
        let def = {
            let inst = &state.objects[&obj_id];
            state.card_db().get(inst.card_def_id).unwrap().clone()
        };
        finish_physical_spell(state, obj_id, &def);
    }
}

fn finish_physical_spell(state: &mut GameState, obj_id: ObjectId, def: &crate::card::CardDef) {
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

/// Execute the shared instant/sorcery portion; card movement belongs to the
/// physical-spell caller. A copy has no physical source or destination card.
fn resolve_spell_effect(
    state: &mut GameState,
    definition: &crate::card::CardDef,
    targets: &[Target],
    generations: &[Option<u32>],
    controller: PlayerIndex,
    physical_source: Option<ObjectId>,
) {
    let legal = crate::targeting::valid_spell_targets(state, controller, definition, targets)
        && crate::targeting::target_generations(state, targets) == generations;
    if legal {
        if let Some(ref effect) = definition.spell_effect {
            super::effects::resolve_effect(state, effect, controller, targets, generations, physical_source);
        }
    }
}

/// Resolve an activated ability.
fn resolve_activated_ability(
    state: &mut GameState,
    source_id: ObjectId,
    ability_index: usize,
    targets: &[Target],
    generations: &[Option<u32>],
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
        if !crate::targeting::valid_destroy_ability_targets(
            state, controller, &effect, targets, generations,
        ) { return; }
        super::effects::resolve_effect(state, &effect, controller, targets, generations, Some(source_id));
    }
}

/// Resolve a triggered ability.
fn resolve_triggered_ability(
    state: &mut GameState,
    source_id: ObjectId,
    context: &crate::game::TriggerContext,
    targets: &[Target],
    generations: &[Option<u32>],
    controller: PlayerIndex,
) {
    if !crate::targeting::valid_destroy_ability_targets(
        state, controller, &context.effect, targets, generations,
    ) { return; }
    // The source is useful only while it is the same incarnation. Never let
    // a returned permanent supply state for the older ability on the stack.
    let live_source = state.objects.get(&source_id)
        .filter(|inst| state.battlefield.contains(&source_id)
            && inst.zone_change_count == context.source_generation)
        .map(|_| source_id);
    super::effects::resolve_trigger_effect(
        state, context, controller, targets, generations, live_source, source_id,
    );
}

#[cfg(test)]
mod destruction_ability_tests {
    use std::sync::Arc;
    use super::*;
    use crate::card::{ActivatedAbility, CardDef, Effect, KeywordAbility, TargetSpec};
    use crate::game::CardDatabase;
    use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    use crate::mana::ManaCost;

    fn fixture(effect: Effect) -> (GameState, ObjectId, ObjectId) {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 996_001, name: "Destroy ability".into(),
            card_types: vec![CardType::Artifact],
            activated_abilities: vec![ActivatedAbility { cost: ManaCost::zero(),
                requires_tap: false, sacrifice_cost: None, life_cost: 0,
                effect, description: "test".into() }], ..Default::default() });
        db.insert(CardDef { id: 996_002, name: "Target".into(),
            card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
            ..Default::default() });
        db.insert(CardDef { id: 996_003, name: "Library".into(),
            card_types: vec![CardType::Land], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        let source = state.create_card_in_zone(996_001, 0, ZoneType::Battlefield);
        let subject = state.create_card_in_zone(996_002, 1, ZoneType::Battlefield);
        (state, source, subject)
    }

    fn resolve(state: &mut GameState, source: ObjectId, target: ObjectId, generation: u32) {
        resolve_activated_ability(state, source, 0, &[Target::Object(target)],
            &[Some(generation)], 0);
    }

    fn make_noncreature(state: &mut GameState, target: ObjectId) {
        state.continuous_effects.push(ContinuousEffect {
            source_id: target, controller: 1, timestamp: 1,
            duration: Duration::Permanent,
            affected: AffectedObjects::Specific(target),
            modification: LayerModification::SetTypes(vec![CardType::Artifact]),
        });
        state.invalidate_characteristics_cache();
    }

    #[test]
    fn targeted_destroy_ability_checks_current_legality_and_incarnation() {
        let effect = Effect::DestroyTarget { target: TargetSpec::AnyCreature };
        let (mut legal, source, target) = fixture(effect.clone());
        let generation = legal.objects[&target].zone_change_count;
        resolve(&mut legal, source, target, generation);
        assert!(legal.players[1].graveyard.contains(&target));

        let (mut protected, source, target) = fixture(effect.clone());
        let generation = protected.objects[&target].zone_change_count;
        protected.objects.get_mut(&target).unwrap().temp_keywords.push(KeywordAbility::Shroud);
        resolve(&mut protected, source, target, generation);
        assert!(protected.battlefield.contains(&target));

        let (mut departed, source, target) = fixture(effect.clone());
        let generation = departed.objects[&target].zone_change_count;
        departed.move_object(target, ZoneType::Battlefield, ZoneType::Graveyard);
        resolve(&mut departed, source, target, generation);
        assert!(!departed.battlefield.contains(&target));
        assert_eq!(departed.players[1].graveyard.iter().filter(|&&id| id == target).count(), 1);

        let (mut returned, source, target) = fixture(effect);
        let old = returned.objects[&target].zone_change_count;
        returned.move_object(target, ZoneType::Battlefield, ZoneType::Graveyard);
        returned.move_object(target, ZoneType::Graveyard, ZoneType::Battlefield);
        resolve(&mut returned, source, target, old);
        assert!(returned.battlefield.contains(&target));
    }

    #[test]
    fn destroy_ability_multiple_does_not_recheck_later_child() {
        let effect = Effect::Multiple(vec![
            Effect::DestroyTarget { target: TargetSpec::AnyCreature },
            Effect::DrawCards { count: 1 },
        ]);
        let (mut state, source, target) = fixture(effect);
        let card = state.create_card_in_zone(996_003, 0, ZoneType::Library);
        let generation = state.objects[&target].zone_change_count;
        resolve(&mut state, source, target, generation);
        assert!(state.players[1].graveyard.contains(&target));
        assert!(state.players[0].hand.contains(&card));
    }

    #[test]
    fn type_change_uses_entry_legality_and_pretransition_creature_lki() {
        let (mut creature_only, source, target) = fixture(
            Effect::DestroyTarget { target: TargetSpec::AnyCreature });
        let generation = creature_only.objects[&target].zone_change_count;
        make_noncreature(&mut creature_only, target);
        resolve(&mut creature_only, source, target, generation);
        assert!(creature_only.battlefield.contains(&target));

        let (mut any_permanent, source, target) = fixture(
            Effect::DestroyTarget { target: TargetSpec::AnyPermanent });
        let generation = any_permanent.objects[&target].zone_change_count;
        make_noncreature(&mut any_permanent, target);
        resolve(&mut any_permanent, source, target, generation);
        assert!(any_permanent.players[1].graveyard.contains(&target));
        assert!(!any_permanent.pending_triggers.iter().any(|trigger|
            trigger.context.zone_transition.as_ref().is_some_and(|zone| zone.subject.creature_died())));
    }
}

#[cfg(test)]
mod settlement_copy_tests {
    use super::*;
    use crate::action::Action;
    use crate::card::{CardDef, Effect, TriggeredAbility};
    use crate::events::{GameEvent, Zone};
    use crate::game::{CardDatabase, Phase};
    use std::sync::Arc;

    #[test]
    fn copy_order_suspends_deferred_trigger_until_physical_cleanup() {
        let mut db = CardDatabase::new();
        db.insert(CardDef {
            id: 990101,
            name: "Ordered spell".into(),
            card_types: vec![CardType::Instant],
            spell_effect: Some(Effect::Multiple(vec![
                Effect::DrawCards { count: 1 },
                Effect::TestCopyBatch { copies: 2 },
            ])),
            ..Default::default()
        });
        db.insert(CardDef {
            id: 990102,
            name: "Draw observer".into(),
            card_types: vec![CardType::Enchantment],
            triggered_abilities: vec![TriggeredAbility {
                trigger: TriggerCondition::OpponentDrawsCard,
                effect: Effect::GainLife { amount: 1 },
                description: "Observed draw".into(),
            }],
            ..Default::default()
        });
        db.insert(CardDef {
            id: 990103,
            name: "Library card".into(),
            card_types: vec![CardType::Land],
            ..Default::default()
        });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::PreCombatMain;
        state.create_card_in_zone(990103, 0, ZoneType::Library);
        let observer = state.create_card_in_zone(990102, 1, ZoneType::Battlefield);
        let spell = state.create_card_in_zone(990101, 0, ZoneType::Hand);
        super::super::apply_action(
            &mut state,
            &Action::CastSpell {
                object_id: spell,
                targets: vec![],
            },
        );
        state.drain_events();
        super::super::apply_action(&mut state, &Action::PassPriority);
        super::super::apply_action(&mut state, &Action::PassPriority);
        assert!(state.pending_copy_order.is_some());
        assert_eq!(state.pending_triggers.len(), 1);
        assert!(state.stack.is_empty());
        assert!(!state.players[0].graveyard.contains(&spell));
        assert!(!state.drain_events().iter().any(|event|
            matches!(event, GameEvent::AbilityTriggered { source, .. } if *source == observer)));

        super::super::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
        assert!(state.pending_copy_order.is_none());
        assert_eq!(
            state.players[0]
                .graveyard
                .iter()
                .filter(|&&id| id == spell)
                .count(),
            1
        );
        assert_eq!(state.stack.len(), 3);
        assert!(matches!(state.stack.last().unwrap().source,
            StackSource::TriggeredAbility { source_id, .. } if source_id == observer));
        let events = state.drain_events();
        let cleanup = events.iter().position(|event| matches!(event,
            GameEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Graveyard } if *object == spell)).unwrap();
        let placement = events
            .iter()
            .position(|event| {
                matches!(event,
            GameEvent::AbilityTriggered { source, .. } if *source == observer)
            })
            .unwrap();
        assert!(cleanup < placement);
        let before = serde_json::to_value(&state).unwrap();
        super::super::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
}

#[cfg(test)]
mod terminal_copy_lock_tests {
    use super::*;
    use crate::card::{CardDef, Effect};
    use crate::game::{CastSpellSnapshot, Phase, StackEntry};
    use crate::rules::{begin_terminal_copy_batch, prepare_spell_copy, CopyTargetPolicy};

    #[test]
    fn direct_resolution_phase_sba_and_trigger_flush_defer_to_order_choice() {
        let mut state = GameState::new(2);
        state.phase = Phase::PreCombatMain;
        let definition = CardDef { id: 982001, name: "Synthetic".into(),
            card_types: vec![CardType::Instant],
            spell_effect: Some(Effect::GainLife { amount: 5 }), ..Default::default() };
        state.stack.push(StackEntry { id: 1,
            source: StackSource::SpellCopy { definition: Box::new(definition.clone()) },
            controller: 0, targets: vec![], target_generations: vec![] });
        state.next_stack_id = 2;
        let snapshot = CastSpellSnapshot { stack_id: 1, definition: Box::new(definition),
            controller: 0, targets: vec![], target_generations: vec![],
            historical_object_targets: vec![] };
        let item = prepare_spell_copy(&state, &snapshot, 0, CopyTargetPolicy::Preserve).unwrap();
        begin_terminal_copy_batch(&mut state, vec![item.clone(), item], true).unwrap();
        state.players[0].life = 0;
        let before = serde_json::to_value(&state).unwrap();
        resolve_top_of_stack(&mut state);
        crate::rules::phases::handle_priority_pass(&mut state);
        crate::rules::phases::advance_phase(&mut state);
        crate::rules::sba::check_state_based_actions(&mut state);
        assert!(!crate::rules::triggers::flush_triggers(&mut state));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
    }
}

#[cfg(test)]
mod terminal_copy_integration_tests {
    use super::*;
    use std::sync::Arc;
    use crate::action::Action;
    use crate::card::{CardDef, Effect, TriggerCondition, TriggeredAbility};
    use crate::card::effects::{Condition, DynamicValue};
    use crate::game::{CardDatabase, Phase};

    const SPELL: u64 = 982100;

    fn resolve_fixture(effect: Effect) -> (GameState, ObjectId) {
        resolve_fixture_with(effect, |_| {})
    }

    fn resolve_fixture_with(effect: Effect, setup: impl FnOnce(&mut GameState)) -> (GameState, ObjectId) {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: SPELL, name: "Test batch spell".into(),
            card_types: vec![CardType::Instant], spell_effect: Some(effect),
            ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state.phase = Phase::PreCombatMain;
        let object = state.create_card_in_zone(SPELL, 0, ZoneType::Hand);
        super::super::apply_action(&mut state, &Action::CastSpell { object_id: object, targets: vec![] });
        setup(&mut state);
        super::super::apply_action(&mut state, &Action::PassPriority);
        super::super::apply_action(&mut state, &Action::PassPriority);
        (state, object)
    }

    #[test]
    fn nested_terminal_position_controls_real_batch_creation() {
        let batch = || Effect::TestCopyBatch { copies: 2 };
        let life = || Effect::GainLife { amount: 7 };
        let conditional = |child| Effect::Conditional { condition: Condition::Always,
            if_true: Box::new(child), if_false: None };
        let cases = [
            (Effect::Multiple(vec![batch(), life()]), false),
            (Effect::Multiple(vec![life(), batch(), life()]), false),
            (Effect::Multiple(vec![life(), batch()]), true),
            (Effect::Multiple(vec![conditional(batch()), life()]), false),
            (Effect::Multiple(vec![life(), conditional(Effect::Multiple(vec![life(), batch()]))]), true),
            (Effect::Modal { choices: vec![batch(), life()], choose_count: 2 }, false),
            (Effect::Modal { choices: vec![life(), batch()], choose_count: 2 }, true),
            (Effect::Multiple(vec![Effect::ForEach { count: DynamicValue::Fixed(2),
                effect: Box::new(batch()) }, life()]), false),
            (Effect::ForEach { count: DynamicValue::Fixed(2), effect: Box::new(batch()) }, true),
        ];
        for (effect, should_pause) in cases {
            let (state, object) = resolve_fixture(effect);
            assert_eq!(state.pending_copy_order.is_some(), should_pause);
            assert_eq!(state.players[0].graveyard.contains(&object), !should_pause);
            if should_pause {
                assert!(state.pending_copy_order.as_ref().unwrap().resolving_entry().is_some());
            }
        }
    }

    #[test]
    fn real_stack_resolution_suspends_and_cleans_up_original_exactly_once() {
        let (mut state, object) = resolve_fixture(Effect::TestCopyBatch { copies: 2 });
        let pending = state.pending_copy_order.as_ref().expect("batch pauses resolution");
        assert_eq!(pending.items().len(), 2);
        assert!(pending.resolving_entry().is_some());
        assert!(!state.players[0].graveyard.contains(&object));
        assert!(state.stack.is_empty());
        super::super::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 1 });
        assert!(state.pending_copy_order.is_none());
        assert_eq!(state.players[0].graveyard.iter().filter(|&&id| id == object).count(), 1);
        assert_eq!(state.stack.len(), 2);
        assert_eq!(state.priority_player, state.active_player);
        super::super::apply_action(&mut state, &Action::PassPriority);
        super::super::apply_action(&mut state, &Action::PassPriority);
        assert_eq!(state.players[0].life, 21);
        super::super::apply_action(&mut state, &Action::PassPriority);
        super::super::apply_action(&mut state, &Action::PassPriority);
        assert_eq!(state.players[0].life, 22);
        assert_eq!(state.players[0].graveyard.iter().filter(|&&id| id == object).count(), 1);
    }

    #[test]
    fn real_suspended_resolution_runs_sba_and_nonactive_trigger_order_after_commit() {
        const WATCHER: u64 = 982101;
        const DOOMED: u64 = 982102;
        let (mut state, object) = resolve_fixture_with(Effect::TestCopyBatch { copies: 2 }, |state| {
            let watcher = CardDef { id: WATCHER, name: "Watcher".into(),
                card_types: vec![CardType::Enchantment],
                triggered_abilities: (0..2).map(|i| TriggeredAbility {
                    trigger: TriggerCondition::ACreatureDies,
                    effect: Effect::GainLife { amount: i + 1 }, description: format!("Death {i}")
                }).collect(), ..Default::default() };
            let doomed = CardDef { id: DOOMED, name: "Doomed".into(),
                card_types: vec![CardType::Creature], power: Some(1), toughness: Some(1),
                ..Default::default() };
            let db = Arc::make_mut(state.card_db.as_mut().unwrap());
            db.insert(watcher);
            db.insert(doomed);
            state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
            let dying = state.create_card_in_zone(DOOMED, 0, ZoneType::Battlefield);
            state.objects.get_mut(&dying).unwrap().damage_marked = 1;
        });
        assert_eq!(state.pending_triggers.len(), 0);
        assert!(state.pending_copy_order.is_some());
        super::super::apply_action(&mut state, &Action::ChooseNextCopy { item_index: 0 });
        assert_eq!(state.players[0].graveyard.iter().filter(|&&id| id == object).count(), 1);
        assert_eq!(state.pending_triggers.len(), 2);
        assert_eq!(state.priority_player, 1);
        let order = crate::action::legal_actions(&state).into_iter()
            .find(|action| matches!(action, Action::OrderTriggers { .. })).unwrap();
        super::super::apply_action(&mut state, &order);
        assert_eq!(state.priority_player, 0);
        assert!(state.pending_triggers.is_empty());
        assert_eq!(state.stack.len(), 4);
    }
}

use crate::card::{CardId, Effect, KeywordAbility, ObjectId, TriggerCondition, ZoneType};
use crate::events::GameEvent;
use crate::game::{CastSpellSnapshot, GameState, PlayerIndex, StackSource, Target, TriggerContext};

#[cfg(test)]
mod destruction_migration_tests {
    use std::sync::Arc;
    use super::*;
    use crate::card::{CardDef, CardType, TargetSpec, TriggeredAbility};
    use crate::events::Zone;
    use crate::game::CardDatabase;
    use crate::replacement::{ReplacementAction, ReplacementEffect, ReplacementEventKind};
    use crate::rules::transitions::{destroy_batch, ExactObjectRef, TransitionError};

    fn fixture() -> GameState {
        let mut db = CardDatabase::new();
        db.insert(CardDef { id: 995_001, name: "Subject".into(),
            card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
            ..Default::default() });
        db.insert(CardDef { id: 995_002, name: "Observer".into(),
            card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
            triggered_abilities: vec![TriggeredAbility {
                trigger: TriggerCondition::ACreatureDies,
                effect: Effect::GainLife { amount: 1 }, description: "death".into(),
            }], ..Default::default() });
        db.insert(CardDef { id: 995_003, name: "Self".into(),
            card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
            triggered_abilities: vec![TriggeredAbility {
                trigger: TriggerCondition::Dies,
                effect: Effect::GainLife { amount: 1 }, description: "self death".into(),
            }], ..Default::default() });
        db.insert(CardDef { id: 995_004, name: "Noncreature".into(),
            card_types: vec![CardType::Artifact],
            triggered_abilities: vec![TriggeredAbility {
                trigger: TriggerCondition::LeavesBattlefield,
                effect: Effect::GainLife { amount: 1 }, description: "leave".into(),
            }], ..Default::default() });
        let mut state = GameState::new(2);
        state.card_db = Some(Arc::new(db));
        state
    }

    #[test]
    fn destroy_all_captures_simultaneous_departed_watcher() {
        let mut state = fixture();
        let watcher = state.create_card_in_zone(995_002, 0, ZoneType::Battlefield);
        let a = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        let b = state.create_card_in_zone(995_001, 1, ZoneType::Battlefield);
        resolve_effect(&mut state, &Effect::DestroyAll, 0, &[], &[], None);
        let seen: Vec<_> = state.pending_triggers.iter().filter(|p| p.source_id == watcher)
            .filter_map(|p| p.context.zone_transition.as_ref()).collect();
        assert_eq!(seen.len(), 3);
        assert!(seen.iter().all(|p| p.group_id == seen[0].group_id));
        assert!(seen.iter().any(|p| p.subject.before.object.id == a));
        assert!(seen.iter().any(|p| p.subject.before.object.id == b));
    }

    #[test]
    fn indestructible_single_destroy_does_not_collect_self_death() {
        let mut state = fixture();
        let id = state.create_card_in_zone(995_003, 0, ZoneType::Battlefield);
        state.objects.get_mut(&id).unwrap().temp_keywords.push(KeywordAbility::Indestructible);
        let generation = state.objects[&id].zone_change_count;
        resolve_effect(&mut state, &Effect::DestroyTarget { target: TargetSpec::AnyCreature },
            1, &[Target::Object(id)], &[Some(generation)], None);
        assert!(state.battlefield.contains(&id));
        assert!(state.pending_triggers.is_empty());
    }

    fn exact(state: &GameState, id: ObjectId) -> ExactObjectRef {
        ExactObjectRef { id, generation: state.objects[&id].zone_change_count }
    }

    fn replacement(state: &mut GameState, action: ReplacementAction) {
        state.replacement_effects.push(ReplacementEffect {
            source_id: 123, controller: 0, applies_to: ReplacementEventKind::WouldDie,
            action, is_self_replacement: true, description: "represented test".into(),
        });
    }

    #[test]
    fn single_destroy_emits_once_with_owned_death_and_noncreature_is_not_death() {
        let mut state = fixture();
        let watcher = state.create_card_in_zone(995_002, 0, ZoneType::Battlefield);
        let creature = state.create_card_in_zone(995_003, 1, ZoneType::Battlefield);
        let artifact = state.create_card_in_zone(995_004, 1, ZoneType::Battlefield);
        let creature_ref = exact(&state, creature);
        let first = destroy_batch(&mut state, &[creature_ref]).unwrap().unwrap();
        assert_eq!(first.transitions.len(), 1);
        assert!(first.transitions[0].creature_died());
        assert_eq!(state.pending_triggers.iter().filter(|p| p.source_id == creature).count(), 1);
        assert_eq!(state.pending_triggers.iter().filter(|p| p.source_id == watcher).count(), 1);
        let artifact_ref = exact(&state, artifact);
        let second = destroy_batch(&mut state, &[artifact_ref]).unwrap().unwrap();
        assert_ne!(first.group_id, second.group_id);
        assert!(!second.transitions[0].creature_died());
        assert_eq!(state.pending_triggers.iter().filter(|p| p.source_id == artifact).count(), 1);
        assert_eq!(state.pending_triggers.iter().filter(|p| p.source_id == watcher).count(), 1);
        let events = state.drain_events();
        for id in [creature, artifact] {
            assert_eq!(events.iter().filter(|e| matches!(e,
                GameEvent::ZoneChange { object, from: Zone::Battlefield,
                    to: Zone::Graveyard } if *object == id)).count(), 1);
        }
    }

    #[test]
    fn destruction_validates_all_intents_before_filtering_protected_members() {
        let mut state = fixture();
        let valid = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        let protected = state.create_card_in_zone(995_001, 1, ZoneType::Battlefield);
        state.objects.get_mut(&protected).unwrap().temp_keywords.push(KeywordAbility::Indestructible);
        let before = serde_json::to_value(&state).unwrap();
        let stale = ExactObjectRef { id: protected, generation: exact(&state, protected).generation + 1 };
        let valid_ref = exact(&state, valid);
        let protected_ref = exact(&state, protected);
        assert_eq!(destroy_batch(&mut state, &[valid_ref, stale]).unwrap_err(),
            TransitionError::StaleIncarnation(protected));
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        let batch = destroy_batch(&mut state, &[valid_ref, protected_ref])
            .unwrap().unwrap();
        assert_eq!(batch.transitions.len(), 1);
        assert_eq!(batch.transitions[0].before.object.id, valid);
        assert!(state.battlefield.contains(&protected));
    }

    #[test]
    fn represented_prevention_and_redirect_control_actual_death() {
        let mut prevented = fixture();
        let id = prevented.create_card_in_zone(995_003, 0, ZoneType::Battlefield);
        replacement(&mut prevented, ReplacementAction::Prevent);
        let id_ref = exact(&prevented, id);
        assert!(destroy_batch(&mut prevented, &[id_ref]).unwrap().is_none());
        assert!(prevented.battlefield.contains(&id));
        assert!(prevented.pending_triggers.is_empty());
        assert!(!prevented.drain_events().iter().any(|e| matches!(e,
            GameEvent::ZoneChange { object, from: Zone::Battlefield, .. } if *object == id)));

        let mut redirected = fixture();
        let id = redirected.create_card_in_zone(995_003, 0, ZoneType::Battlefield);
        replacement(&mut redirected, ReplacementAction::RedirectToZone(ZoneType::Exile));
        let id_ref = exact(&redirected, id);
        let batch = destroy_batch(&mut redirected, &[id_ref]).unwrap().unwrap();
        assert_eq!(batch.transitions[0].destination.zone, ZoneType::Exile);
        assert!(!batch.transitions[0].creature_died());
        assert!(redirected.pending_triggers.is_empty());
    }

    #[test]
    fn token_death_survives_purge_and_commander_redirect_is_not_death() {
        let mut state = fixture();
        let watcher = state.create_card_in_zone(995_002, 0, ZoneType::Battlefield);
        let token = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        state.objects.get_mut(&token).unwrap().is_token = true;
        let token_ref = exact(&state, token);
        let batch = destroy_batch(&mut state, &[token_ref]).unwrap().unwrap();
        assert!(batch.transitions[0].creature_died());
        assert!(!state.objects.contains_key(&token));
        let occurrence = state.pending_triggers.iter().find(|p| p.source_id == watcher)
            .unwrap().context.zone_transition.as_ref().unwrap();
        assert_eq!(occurrence.subject.before.object.id, token);
        assert!(occurrence.subject.before.is_token);
        assert_eq!(state.drain_events().iter().filter(|e| matches!(e,
            GameEvent::ZoneChange { object, from: Zone::Battlefield, .. } if *object == token)).count(), 1);

        let mut commander = GameState::new_commander(2);
        commander.card_db = state.card_db.clone();
        let id = commander.create_card_in_zone(995_003, 0, ZoneType::Battlefield);
        commander.players[0].commander_object_id = Some(id);
        let id_ref = exact(&commander, id);
        let batch = destroy_batch(&mut commander, &[id_ref]).unwrap().unwrap();
        assert_eq!(batch.transitions[0].destination.zone, ZoneType::Command);
        assert!(!batch.transitions[0].creature_died());
        assert!(commander.pending_triggers.is_empty());
    }

    #[test]
    fn mass_destroy_only_commits_actual_movers_from_one_view() {
        let mut state = fixture();
        let watcher = state.create_card_in_zone(995_002, 0, ZoneType::Battlefield);
        let protected = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        let second_protected = state.create_card_in_zone(995_001, 1, ZoneType::Battlefield);
        for id in [protected, second_protected] {
            state.objects.get_mut(&id).unwrap().temp_keywords.push(KeywordAbility::Indestructible);
        }
        let a = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        let b = state.create_card_in_zone(995_001, 1, ZoneType::Battlefield);
        let token = state.create_card_in_zone(995_001, 1, ZoneType::Battlefield);
        state.objects.get_mut(&token).unwrap().is_token = true;
        resolve_effect(&mut state, &Effect::DestroyAll, 0, &[], &[], None);
        for id in [protected, second_protected] { assert!(state.battlefield.contains(&id)); }
        for id in [watcher, a, b, token] { assert!(!state.battlefield.contains(&id)); }
        let contexts: Vec<_> = state.pending_triggers.iter().filter(|p| p.source_id == watcher)
            .filter_map(|p| p.context.zone_transition.as_ref()).collect();
        assert_eq!(contexts.len(), 4);
        assert!(contexts.iter().all(|zone| zone.group_id == contexts[0].group_id));
        assert!(contexts.iter().all(|zone| zone.subject.creature_died()));
        assert_eq!(state.pending_events.iter().filter(|e| matches!(e,
            GameEvent::ZoneChange { from: Zone::Battlefield, to: Zone::Graveyard, .. })).count(), 4);
    }

    #[test]
    fn different_destroy_instructions_have_distinct_groups() {
        let mut state = fixture();
        let watcher = state.create_card_in_zone(995_002, 0, ZoneType::Battlefield);
        state.objects.get_mut(&watcher).unwrap().temp_keywords.push(KeywordAbility::Indestructible);
        let a = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        let b = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        for id in [a, b] {
            let generation = state.objects[&id].zone_change_count;
            resolve_effect(&mut state, &Effect::DestroyTarget { target: TargetSpec::AnyCreature },
                0, &[Target::Object(id)], &[Some(generation)], None);
        }
        let groups: Vec<_> = state.pending_triggers.iter().filter(|p| p.source_id == watcher)
            .map(|p| p.context.zone_transition.as_ref().unwrap().group_id).collect();
        assert_eq!(groups.len(), 2);
        assert_ne!(groups[0], groups[1]);
    }

    #[test]
    fn mixed_type_destruction_batch_has_only_creature_death() {
        let mut state = fixture();
        let creature = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        let artifact = state.create_card_in_zone(995_004, 0, ZoneType::Battlefield);
        let subjects = [exact(&state, creature), exact(&state, artifact)];
        let batch = destroy_batch(&mut state, &subjects).unwrap().unwrap();
        assert_eq!(batch.transitions.len(), 2);
        assert_eq!(batch.transitions.iter().filter(|t| t.creature_died()).count(), 1);
        assert_eq!(batch.transitions.iter().filter(|t| t.left_battlefield()).count(), 2);
    }

    #[test]
    fn departed_watcher_cannot_observe_later_destroy() {
        let mut state = fixture();
        let watcher = state.create_card_in_zone(995_002, 0, ZoneType::Battlefield);
        let later = state.create_card_in_zone(995_001, 0, ZoneType::Battlefield);
        let first = exact(&state, watcher);
        let second = exact(&state, later);
        destroy_batch(&mut state, &[first]).unwrap();
        destroy_batch(&mut state, &[second]).unwrap();
        let seen: Vec<_> = state.pending_triggers.iter().filter(|p| p.source_id == watcher)
            .filter_map(|p| p.context.zone_transition.as_ref()).collect();
        assert_eq!(seen.len(), 1);
        assert_eq!(seen[0].subject.before.object.id, watcher);
    }

    #[test]
    fn represented_replacement_source_departing_in_batch_is_checked_before_commit() {
        let mut state = fixture();
        let source = state.create_card_in_zone(995_003, 0, ZoneType::Battlefield);
        let other = state.create_card_in_zone(995_001, 1, ZoneType::Battlefield);
        state.replacement_effects.push(ReplacementEffect {
            source_id: source, controller: 0, applies_to: ReplacementEventKind::WouldDie,
            action: ReplacementAction::RedirectToZone(ZoneType::Exile),
            is_self_replacement: true, description: "pre-event replacement".into(),
        });
        let subjects = [exact(&state, source), exact(&state, other)];
        let batch = destroy_batch(&mut state, &subjects).unwrap().unwrap();
        assert_eq!(batch.transitions.len(), 2);
        assert!(batch.transitions.iter().all(|t| t.destination.zone == ZoneType::Exile));
        assert!(batch.transitions.iter().all(|t| !t.creature_died()));
    }
}

#[derive(Clone, Copy)]
struct CastEffectContext<'a> {
    spell: &'a CastSpellSnapshot,
    source_id: ObjectId,
    source_generation: u32,
}

// A copy-order choice suspends resolution. Reject a malformed trigger tree
// before any earlier or later child can change the game state.
fn copy_instruction_is_nonterminal(effect: &Effect, terminal: bool) -> bool {
    match effect {
        Effect::CopyCastSpellForOtherCreatures => !terminal,
        Effect::Multiple(children) => children.iter().enumerate().any(|(index, child)|
            copy_instruction_is_nonterminal(child, terminal && index + 1 == children.len())),
        Effect::Modal { choices, choose_count } => {
            let count = choices.len().min(*choose_count as usize);
            choices.iter().take(count).enumerate().any(|(index, child)|
                copy_instruction_is_nonterminal(child, terminal && index + 1 == count))
        }
        Effect::Conditional { if_true, if_false, .. } =>
            copy_instruction_is_nonterminal(if_true, terminal)
                || if_false.as_deref().is_some_and(|child|
                    copy_instruction_is_nonterminal(child, terminal)),
        // Dynamic repetition cannot promise a sole final invocation.
        Effect::ForEach { effect, .. } => contains_copy_instruction(effect),
        _ => false,
    }
}

fn contains_copy_instruction(effect: &Effect) -> bool {
    match effect {
        Effect::CopyCastSpellForOtherCreatures => true,
        Effect::Multiple(children) => children.iter().any(contains_copy_instruction),
        Effect::Modal { choices, .. } => choices.iter().any(contains_copy_instruction),
        Effect::Conditional { if_true, if_false, .. } =>
            contains_copy_instruction(if_true)
                || if_false.as_deref().is_some_and(contains_copy_instruction),
        Effect::ForEach { effect, .. } => contains_copy_instruction(effect),
        _ => false,
    }
}

/// Resolve an effect.
/// `source_id` is the ObjectId of the permanent that generated this effect
/// (e.g., the creature whose triggered ability fired). Used by effects like
/// `BuffOtherSubtype` that need to exclude "self" from the buff.
pub(super) fn resolve_effect(
    state: &mut GameState,
    effect: &Effect,
    controller: PlayerIndex,
    targets: &[Target],
    target_generations: &[Option<u32>],
    source_id: Option<ObjectId>,
) {
    if state.pending_copy_order.is_some() { return; }
    resolve_effect_inner(state, effect, controller, targets, target_generations,
        source_id, None, None, true);
}

/// Resolve an owned trigger instruction with its historical cast context.
pub(super) fn resolve_trigger_effect(
    state: &mut GameState,
    context: &TriggerContext,
    controller: PlayerIndex,
    targets: &[Target],
    target_generations: &[Option<u32>],
    live_source_id: Option<ObjectId>,
    original_source_id: ObjectId,
) {
    if state.pending_copy_order.is_some() { return; }
    if copy_instruction_is_nonterminal(&context.effect, true) { return; }
    let cast_context = context.cast_spell.as_ref().map(|spell| CastEffectContext {
        spell, source_id: original_source_id, source_generation: context.source_generation,
    });
    resolve_effect_inner(state, &context.effect, controller, targets, target_generations, live_source_id,
        Some(context.source_card_id), cast_context, true);
}

fn resolve_effect_inner(
    state: &mut GameState,
    effect: &Effect,
    controller: PlayerIndex,
    targets: &[Target],
    target_generations: &[Option<u32>],
    source_id: Option<ObjectId>,
    last_known_source_card_id: Option<CardId>,
    cast_context: Option<CastEffectContext<'_>>,
    terminal: bool,
) {
    let previous_terminal = state.effect_terminal_position.replace(terminal);
    // Spell targets and untargeted recipients are different. Interpret each
    // leaf's declaration, including leaves nested inside Multiple, before its
    // handler runs. NoTarget retains the handler's existing untargeted behavior.
    use crate::targeting::EffectRecipients;
    let recipients = match crate::targeting::effect_recipients(effect) {
        EffectRecipients::Declared(crate::card::TargetSpec::Controller) => Some(vec![Target::Player(controller)]),
        EffectRecipients::Declared(crate::card::TargetSpec::EachCreature) => Some(state.battlefield.iter().copied()
            .filter(|&id| state.is_creature(id)).map(Target::Object).collect()),
        EffectRecipients::Declared(crate::card::TargetSpec::NoTarget)
        | EffectRecipients::Independent => Some(Vec::new()),
        EffectRecipients::Declared(_)
        | EffectRecipients::SelectedObjects
        | EffectRecipients::Children => None,
    };
    let targets = recipients.as_deref().unwrap_or(targets);
    let recipient_generations = recipients.as_ref()
        .map(|selected| crate::targeting::target_generations(state, selected));
    let target_generations = recipient_generations.as_deref().unwrap_or(target_generations);

    match effect {
        Effect::CopyCastSpellForOtherCreatures => {
            if terminal {
                if let Some(cast) = cast_context {
                    let candidates: Vec<_> = state.battlefield.iter().copied()
                        .filter(|&id| {
                            let Some(chars) = state.get_characteristics(id) else { return false; };
                            chars.controller == controller
                                && chars.card_types.contains(&crate::card::CardType::Creature)
                                && !(id == cast.source_id && state.objects.get(&id).is_some_and(
                                    |inst| inst.zone_change_count == cast.source_generation))
                                && crate::targeting::valid_spell_targets(state, controller,
                                    &cast.spell.definition, &[Target::Object(id)])
                        })
                        .collect();
                    let prepared: Result<Vec<_>, _> = candidates.into_iter().map(|id| {
                        super::spell_copy::prepare_spell_copy(state, cast.spell, controller,
                            super::spell_copy::CopyTargetPolicy::Replace(vec![Target::Object(id)]))
                    }).collect();
                    if let Ok(mut items) = prepared {
                        // Copy-order indices follow public target characteristics;
                        // identical creatures are equivalent ties, not ID ranks.
                        items.sort_by_cached_key(|item| serde_json::to_string(&item.target_descriptions)
                            .expect("copy target descriptions serialize"));
                        let _ = super::spell_copy::begin_terminal_copy_batch(state, items, terminal);
                    }
                }
            }
        }
        #[cfg(test)]
        Effect::TestCopyBatch { copies } => {
            let id = source_id.expect("test spell has a physical source");
            let mut definition = state.card_db().get(state.objects[&id].card_def_id).unwrap().clone();
            definition.spell_effect = Some(Effect::GainLife { amount: 1 });
            let snapshot = crate::game::CastSpellSnapshot {
                stack_id: 0, definition: Box::new(definition), controller,
                targets: vec![], target_generations: vec![], historical_object_targets: vec![],
            };
            let items = (0..*copies).map(|_| super::spell_copy::prepare_spell_copy(
                state, &snapshot, controller, super::spell_copy::CopyTargetPolicy::Preserve).unwrap()
            ).collect();
            let _ = super::spell_copy::begin_terminal_copy_batch(state, items, terminal);
        }
        Effect::DealDamage { amount, target: target_spec } => {
            // For untargeted effects, auto-generate targets from the spec.
            let effective_targets: Vec<Target> = if targets.is_empty() {
                match target_spec {
                    crate::card::TargetSpec::NoTarget => {
                        // "Each player" — deal damage to all players
                        (0..state.players.len())
                            .map(|i| Target::Player(i))
                            .collect()
                    }
                    crate::card::TargetSpec::EachCreature => {
                        // "Each creature" — deal damage to all creatures on the battlefield
                        state.battlefield.iter().copied()
                            .filter(|&id| state.is_creature(id))
                            .map(Target::Object)
                            .collect()
                    }
                    crate::card::TargetSpec::Opponent => {
                        // "Target opponent" — deal damage to the opponent
                        let opp = state.opponent(controller);
                        vec![Target::Player(opp)]
                    }
                    crate::card::TargetSpec::CreatureOrPlayer
                    | crate::card::TargetSpec::AnyPlayer => {
                        // Default to targeting opponent when no explicit target
                        let opp = state.opponent(controller);
                        vec![Target::Player(opp)]
                    }
                    _ => vec![],
                }
            } else {
                targets.to_vec()
            };

            for target in &effective_targets {
                // Apply replacement effects to damage (CR 614)
                let actual_damage = state.deal_damage_with_replacement(*amount, target);
                if actual_damage == 0 {
                    continue;
                }

                match target {
                    Target::StackEntry(_) => (),
                    Target::Player(p) => {
                        let old_life = state.players[*p].life;
                        state.players[*p].life -= actual_damage as i32;
                        state.emit_event(GameEvent::LifeChanged {
                            player: *p,
                            old: old_life,
                            new: state.players[*p].life,
                        });
                        state.emit_event(GameEvent::DamageDealt {
                            source: 0,
                            target: target.clone(),
                            amount: actual_damage,
                            is_combat: false,
                        });
                    }
                    Target::Object(id) => {
                        if let Some(inst) = state.objects.get_mut(id) {
                            inst.damage_marked += actual_damage;
                        }
                        state.emit_event(GameEvent::DamageDealt {
                            source: 0,
                            target: target.clone(),
                            amount: actual_damage,
                            is_combat: false,
                        });
                    }
                }
            }
        }

        Effect::GainLife { amount } => {
            let old_life = state.players[controller].life;
            state.players[controller].life += *amount as i32;
            state.emit_event(GameEvent::LifeChanged {
                player: controller,
                old: old_life,
                new: state.players[controller].life,
            });
        }

        Effect::LoseLife { amount, .. } => {
            for target in targets {
                if let Target::Player(p) = target {
                    let old_life = state.players[*p].life;
                    state.players[*p].life -= *amount as i32;
                    state.emit_event(GameEvent::LifeChanged {
                        player: *p,
                        old: old_life,
                        new: state.players[*p].life,
                    });
                }
            }
        }

        Effect::DrawCards { count } => {
            super::draw_cards(state, controller, *count as usize);
        }

        Effect::DestroyTarget { .. } => {
            // Resolution-entry target legality is checked for physical spells
            // and targeted abilities. Retain their cast/activation generations;
            // a newly arrived incarnation must never inherit an old target.
            let subjects: Option<Vec<_>> = targets.iter().enumerate().map(|(index, target)| {
                let Target::Object(id) = target else { return None; };
                Some(super::transitions::ExactObjectRef {
                    id: *id, generation: target_generations.get(index).copied().flatten()?,
                })
            }).collect();
            if let Some(subjects) = subjects.filter(|subjects| !subjects.is_empty()) {
                let _ = super::transitions::destroy_batch(state, &subjects);
            }
        }

        Effect::BounceTo { zone, .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    state.move_object(*id, ZoneType::Battlefield, *zone);
                }
            }
        }

        Effect::Buff {
            power,
            toughness,
            until_eot,
        } => {
            use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
            for target in targets {
                if let Target::Object(id) = target {
                    if *until_eot {
                        // Create a continuous effect that lasts until end of turn
                        let ts = state.new_timestamp();
                        state.continuous_effects.push(ContinuousEffect {
                            source_id: *id,
                            controller,
                            timestamp: ts,
                            duration: Duration::UntilEndOfTurn,
                            affected: AffectedObjects::Specific(*id),
                            modification: LayerModification::ModifyPT(*power, *toughness),
                        });
                    } else {
                        // Permanent buff via +1/+1 counters
                        if let Some(inst) = state.objects.get_mut(id) {
                            let counters = (*power).min(*toughness);
                            inst.plus_counters += counters;
                            // Any asymmetric remainder as a permanent continuous effect
                            if *power != *toughness {
                                let ts = state.new_timestamp();
                                state.continuous_effects.push(ContinuousEffect {
                                    source_id: *id,
                                    controller,
                                    timestamp: ts,
                                    duration: Duration::Permanent,
                                    affected: AffectedObjects::Specific(*id),
                                    modification: LayerModification::ModifyPT(
                                        power - counters,
                                        toughness - counters,
                                    ),
                                });
                            }
                        }
                    }
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::DiscardCards { count, .. } => {
            for target in targets {
                if let Target::Player(p) = target {
                    super::discard_random(state, *p, *count as usize);
                }
            }
        }

        Effect::CreateToken(token_def) => {
            super::tokens::create_token(state, token_def, controller);
        }

        Effect::CreateTokens { token, count } => {
            let n = state.evaluate_dynamic_value(count, controller, None);
            for _ in 0..n.max(0) {
                super::tokens::create_token(state, token, controller);
            }
        }

        Effect::Counter { .. } => {
            // No fallback to another spell: only the selected stack occurrence
            // can be countered, even if its ID has since become stale.
            if let Some(Target::StackEntry(target_id)) = targets.first() {
                if let Some(idx) = state.stack.iter().position(|entry| {
                    entry.id == *target_id && entry.source.is_spell()
                }) {
                    let countered = state.stack.remove(idx);
                    if let StackSource::Spell(obj_id) = countered.source {
                        state.move_object(obj_id, ZoneType::Stack, ZoneType::Graveyard);
                    }
                }
            }
        }

        Effect::ExileTarget { .. } => {
            // One effect instruction is one event, even when it has several
            // selected objects. Spell target legality/generations were checked
            // before resolution; the kernel independently validates the full
            // exact-incarnation request before any mutation.
            let requests: Option<Vec<_>> = targets.iter().enumerate().map(|(index, target)| {
                let Target::Object(id) = target else { return None; };
                let generation = target_generations.get(index).copied().flatten()?;
                Some(super::transitions::TransitionRequest {
                    object: super::transitions::ExactObjectRef {
                        id: *id, generation,
                    },
                    from: ZoneType::Battlefield,
                    to: ZoneType::Exile,
                    kind: super::transitions::MovementKind::Put,
                })
            }).collect();
            if let Some(requests) = requests.filter(|requests| !requests.is_empty()) {
                let _ = super::transitions::transition_batch(state, &requests);
            }
        }

        Effect::DestroyAll => {
            // One instruction is one event. Capture the complete current set
            // before any prevention/replacement decision or storage movement.
            let subjects: Vec<_> = state
                .battlefield
                .iter()
                .copied()
                .filter(|&id| state.is_creature(id))
                .map(|id| super::transitions::ExactObjectRef {
                    id, generation: state.objects[&id].zone_change_count,
                })
                .collect();
            if !subjects.is_empty() {
                let _ = super::transitions::destroy_batch(state, &subjects);
            }
        }

        Effect::Debuff {
            power,
            toughness,
            until_eot,
        } => {
            use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
            for target in targets {
                if let Target::Object(id) = target {
                    let duration = if *until_eot {
                        Duration::UntilEndOfTurn
                    } else {
                        Duration::Permanent
                    };
                    let ts = state.new_timestamp();
                    state.continuous_effects.push(ContinuousEffect {
                        source_id: *id,
                        controller,
                        timestamp: ts,
                        duration,
                        affected: AffectedObjects::Specific(*id),
                        modification: LayerModification::ModifyPT(-power, -toughness),
                    });
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::PutCounters { count, .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if let Some(inst) = state.objects.get_mut(id) {
                        if *count > 0 {
                            inst.plus_counters += count;
                        } else {
                            inst.minus_counters += count.abs();
                        }
                    }
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::MillCards { count, .. } => {
            for target in targets {
                if let Target::Player(p) = target {
                    for _ in 0..*count {
                        if !state.players[*p].library.is_empty() {
                            let card_id = state.players[*p].library.remove(0);
                            state.move_object(card_id, ZoneType::Library, ZoneType::Graveyard);
                        }
                    }
                }
            }
        }

        Effect::SacrificeCreatures { count, .. } => {
            for target in targets {
                if let Target::Player(p) = target {
                    let mut creatures = state.creatures_controlled_by(*p);
                    // Sort by effective power ascending so the weakest are
                    // sacrificed first — a reasonable heuristic standing in
                    // for actual player choice until we surface a UI action.
                    creatures.sort_by_key(|&id| state.effective_power(id));
                    for &id in creatures.iter().take(*count as usize) {
                        state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                    }
                }
            }
            state.refresh_continuous_effects();
        }

        Effect::PreventCombatDamage => {
            // Not yet implemented — no cards in the current pool use this effect.
        }

        Effect::AddMana { color, amount } => {
            for _ in 0..*amount {
                match color {
                    Some(c) => state.players[controller].mana_pool.add_color(*c, 1),
                    None => state.players[controller].mana_pool.colorless += 1,
                }
            }
        }

        Effect::AddDynamicMana { color, count } => {
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let n = state.evaluate_dynamic_value(count, controller, Some(&ctx));
            for _ in 0..n.max(0) {
                state.players[controller].mana_pool.add_color(*color, 1);
            }
        }

        Effect::LoseDynamicLife { amount, .. } => {
            let db_ref = state.card_db.clone();
            let db = db_ref.as_ref().expect("card_db required");
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let n = amount.evaluate(
                controller,
                &state.objects,
                &state.battlefield,
                &|id| db.get(id),
                Some(&ctx),
            );
            if n > 0 {
                for target in targets {
                    if let Target::Player(p) = target {
                        let old_life = state.players[*p].life;
                        state.players[*p].life -= n;
                        state.emit_event(GameEvent::LifeChanged {
                            player: *p,
                            old: old_life,
                            new: state.players[*p].life,
                        });
                    }
                }
                // If no explicit targets, apply to controller
                if targets.is_empty() {
                    let old_life = state.players[controller].life;
                    state.players[controller].life -= n;
                    state.emit_event(GameEvent::LifeChanged {
                        player: controller,
                        old: old_life,
                        new: state.players[controller].life,
                    });
                }
            }
        }

        Effect::ExtraTurn => {
            state.extra_turns.push_back(controller);
        }

        Effect::SkipPhase(phase) => {
            state.skip_phases.insert(*phase);
        }

        Effect::Multiple(effects) => {
            for (index, e) in effects.iter().enumerate() {
                resolve_effect_inner(state, e, controller, targets, target_generations,
                    source_id, last_known_source_card_id, cast_context,
                    terminal && index + 1 == effects.len());
            }
        }

        Effect::SearchLibrary { destination, subtype_filter } => {
            if !subtype_filter.is_empty() {
                // Fetch-land style: filter by subtype. Always present a choice
                // (or fail to find if no matching cards in library).
                state.pending_tutor = Some(crate::game::PendingTutor {
                    controller,
                    destination: *destination,
                    subtype_filter: subtype_filter.clone(),
                });
            } else if state.players[controller].tutor_targets.is_empty() {
                // Legacy behavior: no tutor targets configured, take top card.
                if !state.players[controller].library.is_empty() {
                    let card_obj = state.players[controller].library.remove(0);
                    state.move_object(card_obj, ZoneType::Library, *destination);
                }
            } else {
                // Set pending tutor — the player will choose via ChooseTutorTarget.
                state.pending_tutor = Some(crate::game::PendingTutor {
                    controller,
                    destination: *destination,
                    subtype_filter: vec![],
                });
            }
        }

        Effect::BounceAllNonlandOpponents => {
            // Bounce all nonland permanents opponents control to their owners' hands.
            let db = state.card_db();
            let to_bounce: Vec<ObjectId> = state
                .battlefield
                .iter()
                .copied()
                .filter(|&id| {
                    if let Some(inst) = state.objects.get(&id) {
                        if inst.controller == controller {
                            return false; // skip own permanents
                        }
                        if let Some(def) = db.get(inst.card_def_id) {
                            return !def.is_land();
                        }
                    }
                    false
                })
                .collect();
            for id in to_bounce {
                state.move_object(id, ZoneType::Battlefield, ZoneType::Hand);
            }
        }

        Effect::ReturnToTopOfLibrary { .. } => {
            // Put target card from graveyard on top of owner's library.
            for target in targets {
                match target {
                    Target::Object(obj_id) => {
                        if let Some(inst) = state.objects.get(obj_id) {
                            let owner = inst.owner;
                            state.move_object(*obj_id, ZoneType::Graveyard, ZoneType::Library);
                            // Move to front (top) of library
                            if let Some(pos) = state.players[owner].library.iter().position(|&id| id == *obj_id) {
                                let id = state.players[owner].library.remove(pos);
                                state.players[owner].library.insert(0, id);
                            }
                        }
                    }
                    _ => {}
                }
            }
        }

        Effect::UntapTarget { .. } => {
            for target in targets {
                match target {
                    Target::Object(obj_id) => {
                        if let Some(inst) = state.objects.get_mut(obj_id) {
                            inst.tapped = false;
                        }
                    }
                    _ => {}
                }
            }
        }

        // --- Zone manipulation effects ---

        Effect::ReturnFromGraveyardToBattlefield { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.players.iter().any(|p| p.graveyard.contains(id)) {
                        state.move_object(*id, ZoneType::Graveyard, ZoneType::Battlefield);
                        if let Some(inst) = state.objects.get_mut(id) {
                            inst.controller = controller;
                        }
                        state.refresh_continuous_effects();
                        let _ = super::triggers::fire_triggers(state, TriggerCondition::EntersBattlefield, Some(*id));
                    }
                }
            }
        }

        Effect::ReturnFromGraveyardToHand { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.players.iter().any(|p| p.graveyard.contains(id)) {
                        state.move_object(*id, ZoneType::Graveyard, ZoneType::Hand);
                    }
                }
            }
        }

        Effect::ExileFromGraveyard { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.players.iter().any(|p| p.graveyard.contains(id)) {
                        state.move_object(*id, ZoneType::Graveyard, ZoneType::Exile);
                    }
                }
            }
        }

        Effect::ExileFromHandLinked => {
            // Exile a card from controller's hand, linked to the source permanent.
            if let Some(source) = source_id {
                for target in targets {
                    if let Target::Object(id) = target {
                        let controller = state.objects.get(&source)
                            .map(|i| i.controller)
                            .unwrap_or(0);
                        if state.players[controller].hand.contains(id) {
                            state.move_object(*id, ZoneType::Hand, ZoneType::Exile);
                            if let Some(inst) = state.objects.get_mut(id) {
                                inst.exiled_by = Some(source);
                            }
                        }
                    }
                }
            }
        }

        Effect::ReturnLinkedExileToHand => {
            // Return a card exiled by the source permanent to its owner's hand.
            if let Some(source) = source_id {
                for target in targets {
                    if let Target::Object(id) = target {
                        let owner = state.objects.get(id).map(|i| i.owner).unwrap_or(0);
                        if state.players[owner].exile.contains(id)
                            && state.objects.get(id).and_then(|i| i.exiled_by) == Some(source)
                        {
                            if let Some(inst) = state.objects.get_mut(id) {
                                inst.exiled_by = None;
                            }
                            state.move_object(*id, ZoneType::Exile, ZoneType::Hand);
                        }
                    }
                }
            }
        }

        Effect::ShuffleIntoLibrary { .. } => {
            use rand::seq::SliceRandom;
            for target in targets {
                if let Target::Object(id) = target {
                    if state.battlefield.contains(id) {
                        let owner = state.objects.get(id).map(|i| i.owner).unwrap_or(0);
                        state.move_object(*id, ZoneType::Battlefield, ZoneType::Library);
                        let mut rng = rand::thread_rng();
                        state.players[owner].library.shuffle(&mut rng);
                    }
                }
            }
        }

        Effect::PutOnBottomOfLibrary { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if state.battlefield.contains(id) {
                        let owner = state.objects.get(id).map(|i| i.owner).unwrap_or(0);
                        state.move_object(*id, ZoneType::Battlefield, ZoneType::Library);
                        // move_object puts it at the end (bottom) by default via push, which is correct
                        // but we need to ensure it's at the end, not the front
                        if let Some(pos) = state.players[owner].library.iter().position(|&x| x == *id) {
                            let removed = state.players[owner].library.remove(pos);
                            state.players[owner].library.push(removed);
                        }
                    }
                }
            }
        }

        // --- Creature/permanent manipulation ---

        Effect::GainKeywordUntilEOT { keyword, .. } => {
            use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
            for target in targets {
                if let Target::Object(id) = target {
                    let Some(zone_change_count) = state.objects.get(id)
                        .filter(|_| state.battlefield.contains(id))
                        .map(|inst| inst.zone_change_count) else { continue };
                    let timestamp = state.new_timestamp();
                    state.continuous_effects.push(ContinuousEffect {
                        source_id: source_id.unwrap_or(*id),
                        controller,
                        timestamp,
                        duration: Duration::UntilEndOfTurn,
                        affected: AffectedObjects::SpecificIncarnation {
                            object_id: *id,
                            zone_change_count,
                        },
                        modification: LayerModification::AddKeyword(*keyword),
                    });
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::SetPowerToughness { power, toughness, until_eot, .. } => {
            use crate::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
            for target in targets {
                if let Target::Object(id) = target {
                    let duration = if *until_eot {
                        Duration::UntilEndOfTurn
                    } else {
                        Duration::Permanent
                    };
                    let ts = state.new_timestamp();
                    state.continuous_effects.push(ContinuousEffect {
                        source_id: *id,
                        controller,
                        timestamp: ts,
                        duration,
                        affected: AffectedObjects::Specific(*id),
                        modification: LayerModification::SetPT(*power, *toughness),
                    });
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::GainControlUntilEOT { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if let Some(inst) = state.objects.get_mut(id) {
                        inst.controller = controller;
                        // Grant haste so the creature can attack/tap this turn
                        if !inst.temp_keywords.contains(&KeywordAbility::Haste) {
                            inst.temp_keywords.push(KeywordAbility::Haste);
                        }
                    }
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::Fight { .. } => {
            // Both targets deal damage equal to their power to each other
            if targets.len() >= 2 {
                if let (Target::Object(a), Target::Object(b)) = (&targets[0], &targets[1]) {
                    let power_a = state.effective_power(*a).max(0) as u32;
                    let power_b = state.effective_power(*b).max(0) as u32;
                    if let Some(inst) = state.objects.get_mut(b) {
                        inst.damage_marked += power_a;
                    }
                    if let Some(inst) = state.objects.get_mut(a) {
                        inst.damage_marked += power_b;
                    }
                }
            }
        }

        Effect::TapTarget { .. } => {
            for target in targets {
                if let Target::Object(id) = target {
                    if let Some(inst) = state.objects.get_mut(id) {
                        inst.tapped = true;
                    }
                }
            }
        }

        // --- Player-targeted effects ---

        Effect::EachOpponentLosesLife { amount } => {
            let opponents: Vec<usize> = (0..state.players.len())
                .filter(|&i| i != controller)
                .collect();
            for opp in opponents {
                let old_life = state.players[opp].life;
                state.players[opp].life -= *amount as i32;
                state.emit_event(GameEvent::LifeChanged {
                    player: opp,
                    old: old_life,
                    new: state.players[opp].life,
                });
            }
        }

        Effect::EachOpponentDiscards { count } => {
            let opponents: Vec<usize> = (0..state.players.len())
                .filter(|&i| i != controller)
                .collect();
            for opp in opponents {
                super::discard_random(state, opp, *count as usize);
            }
        }

        Effect::EachOpponentSacrifices { count } => {
            let opponents: Vec<usize> = (0..state.players.len())
                .filter(|&i| i != controller)
                .collect();
            for opp in opponents {
                let mut creatures = state.creatures_controlled_by(opp);
                creatures.sort_by_key(|&id| state.effective_power(id));
                for &id in creatures.iter().take(*count as usize) {
                    state.move_object(id, ZoneType::Battlefield, ZoneType::Graveyard);
                }
            }
            state.refresh_continuous_effects();
        }

        Effect::DrawThenDiscard { draw, discard, .. } => {
            // Apply to the targeted player, or controller if no target
            let player = targets.iter().find_map(|t| {
                if let Target::Player(p) = t { Some(*p) } else { None }
            }).unwrap_or(controller);
            super::draw_cards(state, player, *draw as usize);
            super::discard_random(state, player, *discard as usize);
        }

        Effect::GainDynamicLife { amount } => {
            let db_ref = state.card_db.clone();
            let db = db_ref.as_ref().expect("card_db required");
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let n = amount.evaluate(
                controller,
                &state.objects,
                &state.battlefield,
                &|id| db.get(id),
                Some(&ctx),
            );
            if n > 0 {
                let old_life = state.players[controller].life;
                state.players[controller].life += n;
                state.emit_event(GameEvent::LifeChanged {
                    player: controller,
                    old: old_life,
                    new: state.players[controller].life,
                });
            }
        }

        // --- Conditional/modal effects ---

        Effect::Modal { choices, choose_count } => {
            // Simplified: for goldfish/AI, always choose the first N choices
            let selected_count = choices.len().min(*choose_count as usize);
            for (index, effect) in choices.iter().take(selected_count).enumerate() {
                resolve_effect_inner(state, effect, controller, targets, target_generations,
                    source_id, last_known_source_card_id, cast_context,
                    terminal && index + 1 == selected_count);
            }
        }

        Effect::Conditional { condition, if_true, if_false } => {
            let met = evaluate_condition(state, condition, controller);
            if met {
                resolve_effect_inner(state, if_true, controller, targets, target_generations,
                    source_id, last_known_source_card_id, cast_context, terminal);
            } else if let Some(else_effect) = if_false {
                resolve_effect_inner(state, else_effect, controller, targets, target_generations,
                    source_id, last_known_source_card_id, cast_context, terminal);
            }
        }

        Effect::ForEach { count, effect } => {
            let db_ref = state.card_db.clone();
            let db = db_ref.as_ref().expect("card_db required");
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let n = count.evaluate(
                controller,
                &state.objects,
                &state.battlefield,
                &|id| db.get(id),
                Some(&ctx),
            );
            for iteration in 0..n.max(0) {
                resolve_effect_inner(state, effect, controller, targets, target_generations,
                    source_id, last_known_source_card_id, cast_context,
                    terminal && iteration + 1 == n.max(0));
            }
        }

        // --- Predefined tokens ---

        Effect::CreatePredefinedToken { token_type, count } => {
            let token_def = token_type.to_token_def();
            for _ in 0..*count {
                super::tokens::create_token(state, &token_def, controller);
            }
        }

        // --- Library manipulation ---

        Effect::Scry { count } => {
            // Simplified scry: for now, leave top cards in place (proper implementation
            // would need player choices about which to put on bottom).
            // In goldfish mode this is a no-op since the player can't make informed
            // choices without seeing the cards.
            let _ = count;
        }

        Effect::Proliferate => {
            // Add one counter of each type already present on each permanent/player
            // that the controller chooses. Simplified: proliferate all permanents
            // with +1/+1 counters (add another) and all players with poison counters.
            let bf = state.battlefield.clone();
            for &obj_id in &bf {
                if let Some(inst) = state.objects.get_mut(&obj_id) {
                    if inst.plus_counters > 0 {
                        inst.plus_counters += 1;
                    }
                    if inst.minus_counters > 0 {
                        inst.minus_counters += 1;
                    }
                    if inst.loyalty_counters > 0 {
                        inst.loyalty_counters += 1;
                    }
                }
            }
            for p in &mut state.players {
                if p.poison_counters > 0 {
                    p.poison_counters += 1;
                }
            }
            state.invalidate_characteristics_cache();
        }

        Effect::BuffOtherSubtype {
            subtype,
            amount,
            until_eot,
        } => {
            use crate::layers::{
                AffectedObjects, ContinuousEffect, Duration, LayerModification,
            };
            let ctx = super::tokens::build_dynamic_context(state, controller);
            let card_db_arc = state.card_db.as_ref().expect("card_db required").clone();
            let val = amount.evaluate(
                controller,
                &state.objects,
                &state.battlefield,
                &|id| card_db_arc.get(id),
                Some(&ctx),
            );
            if val > 0 {
                let ts = state.new_timestamp();
                let duration = if *until_eot {
                    Duration::UntilEndOfTurn
                } else {
                    Duration::Permanent
                };
                state.continuous_effects.push(ContinuousEffect {
                    source_id: source_id.unwrap_or(0),
                    controller,
                    timestamp: ts,
                    duration,
                    affected: AffectedObjects::OtherCreaturesWithSubtypeControlledBy(
                        subtype.clone(),
                        controller,
                    ),
                    modification: LayerModification::ModifyPT(val, val),
                });
                state.invalidate_characteristics_cache();
            }
        }

        Effect::Unimplemented(_) => {
            // Can't resolve unimplemented effects
        }

        Effect::ExtraLandDrop => {
            state.players[controller].land_plays_remaining += 1;
        }

        Effect::Surveil { count } => {
            // Simplified surveil: mill N cards (put top N into graveyard).
            // Full surveil would let you choose which go to GY vs stay on top.
            let n = (*count).min(state.players[controller].library.len() as u32);
            for _ in 0..n {
                if !state.players[controller].library.is_empty() {
                    let card_id = state.players[controller].library.remove(0);
                    state.players[controller].graveyard.push(card_id);
                }
            }
        }

        Effect::AddManaOfAnyColor { amount } => {
            // In goldfish/solver context, add green mana as default for "any color"
            state.players[controller].mana_pool.green += *amount;
        }

        Effect::DoublePowerUntilEOT { target: _ } => {
            // Double the source creature's power until EOT
            if let Some(sid) = source_id {
                let card_def_id = state.objects.get(&sid).map(|i| i.card_def_id);
                let base_power = card_def_id.and_then(|cid| {
                    let db = state.card_db();
                    db.get(cid).and_then(|d| d.power)
                }).unwrap_or(0);
                if let Some(inst) = state.objects.get_mut(&sid) {
                    let current_power = base_power + inst.temp_power_mod;
                    inst.temp_power_mod += current_power;
                }
            }
        }

        Effect::DealDynamicDamage { amount: _, target: _ } => {
            // Evaluated with full context in the card-specific handlers
            // For now, this is a no-op placeholder
        }

        Effect::CreateTokenCopyOfSource => {
            // Create a token that is a copy of the source permanent (same card_def_id).
            // The token inherits all abilities (e.g., Scute Swarm copies get landfall).
            if let Some(cid) = source_id.and_then(|sid| state.objects.get(&sid).map(|i| i.card_def_id))
                .or(last_known_source_card_id) {
                    let obj_id = state.create_card_in_zone(cid, controller, ZoneType::Battlefield);
                    if let Some(inst) = state.objects.get_mut(&obj_id) {
                        inst.controller = controller;
                        inst.is_token = true;
                        inst.summoning_sick = true;
                    }
                    state.refresh_continuous_effects();
                    let _ = super::triggers::fire_triggers(
                        state,
                        TriggerCondition::EntersBattlefield,
                        Some(obj_id),
                    );
                    super::triggers::check_triggers(
                        state,
                        TriggerCondition::ACreatureEnters,
                        None,
                    );
                    let _ = super::triggers::flush_triggers(state);
            }
        }

        Effect::CreateTokenFromDef { card_def_id } => {
            // Create a token from a pre-registered CardDef in the database.
            let obj_id = state.create_card_in_zone(*card_def_id, controller, ZoneType::Battlefield);
            if let Some(inst) = state.objects.get_mut(&obj_id) {
                inst.controller = controller;
                inst.is_token = true;
                inst.summoning_sick = true;
            }
            state.refresh_continuous_effects();
            let _ = super::triggers::fire_triggers(
                state,
                TriggerCondition::EntersBattlefield,
                Some(obj_id),
            );
            super::triggers::check_triggers(state, TriggerCondition::ACreatureEnters, None);
            let _ = super::triggers::flush_triggers(state);
        }
    }
    state.effect_terminal_position = previous_terminal;
}

/// Evaluate a condition in the current game state.
fn evaluate_condition(
    state: &GameState,
    condition: &crate::card::effects::Condition,
    controller: PlayerIndex,
) -> bool {
    use crate::card::effects::Condition;
    match condition {
        Condition::ControlCreatures => {
            !state.creatures_controlled_by(controller).is_empty()
        }
        Condition::LifeAtOrAbove(n) => {
            state.players[controller].life >= *n
        }
        Condition::LifeAtOrBelow(n) => {
            state.players[controller].life <= *n
        }
        Condition::IsYourTurn => {
            state.active_player == controller
        }
        Condition::SourceHasCounters => {
            // Check if any target has +1/+1 counters (simplified)
            false
        }
        Condition::ControlNOrMore { count, card_type } => {
            let db = state.card_db();
            let matching = state.battlefield.iter().filter(|&&id| {
                if let Some(inst) = state.objects.get(&id) {
                    if inst.controller != controller {
                        return false;
                    }
                    if let Some(def) = db.get(inst.card_def_id) {
                        return def.card_types.contains(card_type);
                    }
                }
                false
            }).count();
            matching >= *count as usize
        }
        Condition::Always => true,
        Condition::HandIsEmpty => {
            state.players[controller].hand.is_empty()
        }
        Condition::ControlNOrMorePermanents { count } => {
            let matching = state.battlefield.iter().filter(|&&id| {
                state.objects.get(&id).map_or(false, |inst| inst.controller == controller)
            }).count();
            matching >= *count as usize
        }
    }
}

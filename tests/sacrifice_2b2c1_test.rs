use mtg_gto::action::Action;
use mtg_gto::card::ActivatedAbility;
use mtg_gto::card::{
    CardDef, CardType, Effect, TargetSpec, TriggerCondition, TriggeredAbility, ZoneType,
};
use mtg_gto::game::{CardDatabase, GameState, Target};
use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
use mtg_gto::mana::ManaCost;
use mtg_gto::rules::apply_action;
use std::sync::Arc;

const CREATURE: u64 = 994_100;
const WATCHER: u64 = 994_101;
const ARTIFACT: u64 = 994_102;
fn game(players: usize) -> GameState {
    let mut db = CardDatabase::new();
    for (id, types, triggers) in [
        (
            CREATURE,
            vec![CardType::Creature],
            vec![TriggerCondition::Dies, TriggerCondition::LeavesBattlefield],
        ),
        (
            WATCHER,
            vec![CardType::Creature],
            vec![
                TriggerCondition::ACreatureDies,
                TriggerCondition::ACreatureYouControlDies,
                TriggerCondition::APermanentLeaves,
            ],
        ),
        (
            ARTIFACT,
            vec![CardType::Artifact],
            vec![TriggerCondition::LeavesBattlefield],
        ),
    ] {
        db.insert(CardDef {
            id,
            name: format!("Sacrifice fixture {id}"),
            card_types: types,
            power: Some(2),
            toughness: Some(2),
            triggered_abilities: triggers
                .into_iter()
                .map(|trigger| TriggeredAbility {
                    trigger,
                    effect: Effect::GainLife { amount: 1 },
                    description: "observer".into(),
                })
                .collect(),
            ..Default::default()
        });
    }
    let mut state = GameState::new(players);
    state.card_db = Some(Arc::new(db));
    state
}
fn sacrifice(state: &mut GameState, player: usize, count: u32) {
    run_effect(
        state,
        player,
        Effect::SacrificeCreatures {
            count,
            target: TargetSpec::AnyPlayer,
        },
    );
}
fn run_effect(state: &mut GameState, player: usize, effect: Effect) {
    let source_id = 994_103;
    let mut db = state.card_db().clone();
    db.insert(CardDef {
        id: source_id,
        name: "Resolved sacrifice".into(),
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(),
            requires_tap: false,
            sacrifice_cost: None,
            life_cost: 0,
            effect,
            description: "test".into(),
        }],
        ..Default::default()
    });
    state.card_db = Some(Arc::new(db));
    state.phase = mtg_gto::game::Phase::PreCombatMain;
    let source = state.create_card_in_zone(source_id, 0, ZoneType::Battlefield);
    apply_action(
        state,
        &Action::ActivateAbility {
            object_id: source,
            ability_index: 0,
            targets: vec![Target::Player(player)],
        },
    );
    for _ in 0..state.players.len() {
        apply_action(state, &Action::PassPriority);
    }
}
fn layer(state: &mut GameState, id: u64, modification: LayerModification) {
    let timestamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect {
        source_id: id,
        controller: 0,
        timestamp,
        duration: Duration::UntilEndOfTurn,
        affected: AffectedObjects::SpecificIncarnation {
            object_id: id,
            zone_change_count: state.objects[&id].zone_change_count,
        },
        modification,
    });
    state.invalidate_characteristics_cache();
}
#[test]
fn sacrifice_captures_one_common_event_and_departing_watcher() {
    let mut state = game(2);
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    sacrifice(&mut state, 0, 3);
    assert_eq!(state.pending_triggers.len(), 13);
    let contexts: Vec<_> = state
        .pending_triggers
        .iter()
        .map(|p| p.context.zone_transition.as_ref().unwrap())
        .collect();
    assert!(contexts.iter().all(|c| c.group_id == contexts[0].group_id));
    assert_eq!(
        contexts
            .iter()
            .filter(|c| c.source_before.object.id == watcher)
            .count(),
        9
    );
    assert!(contexts.iter().any(|c| c.subject.before.object.id == a));
    assert!(contexts.iter().any(|c| c.subject.before.object.id == b));
}
#[test]
fn sacrifice_uses_effective_controller_and_type() {
    let mut state = game(2);
    let stolen = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    layer(&mut state, stolen, LayerModification::ChangeController(0));
    let given = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    layer(&mut state, given, LayerModification::ChangeController(1));
    let animated = state.create_card_in_zone(ARTIFACT, 0, ZoneType::Battlefield);
    layer(
        &mut state,
        animated,
        LayerModification::AddType(CardType::Creature),
    );
    let deanimated = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    layer(
        &mut state,
        deanimated,
        LayerModification::RemoveType(CardType::Creature),
    );
    sacrifice(&mut state, 0, 10);
    assert!(state.players[1].graveyard.contains(&stolen));
    assert!(state.players[0].graveyard.contains(&animated));
    assert!(state.battlefield.contains(&given));
    assert!(state.battlefield.contains(&deanimated));
    let lki = state
        .pending_triggers
        .iter()
        .find_map(|p| {
            p.context
                .zone_transition
                .as_ref()
                .filter(|c| c.subject.before.object.id == stolen)
        })
        .unwrap();
    assert_eq!(
        (lki.subject.before.owner, lki.subject.before.controller),
        (1, 0)
    );
}

use mtg_gto::action::{
    canonical::{canonicalize, resolve},
    legal_actions,
};
use mtg_gto::card::{KeywordAbility, SacrificeCost, Subtype};
use mtg_gto::events::{GameEvent, Zone};
use mtg_gto::info_set::InformationSet;
use mtg_gto::replacement::{ReplacementAction, ReplacementEffect, ReplacementEventKind};
use mtg_gto::rules::transitions::{
    destroy_batch, resolved_sacrifice_batch, ExactObjectRef, MovementKind,
    ResolvedSacrificeSubject, TransitionError,
};
fn selected(state: &GameState, id: u64, player: usize) -> ResolvedSacrificeSubject {
    ResolvedSacrificeSubject {
        player,
        object: ExactObjectRef {
            id,
            generation: state.objects[&id].zone_change_count,
        },
    }
}
fn replacement(state: &mut GameState, action: ReplacementAction) {
    state.replacement_effects.push(ReplacementEffect {
        source_id: 0,
        controller: 0,
        applies_to: ReplacementEventKind::WouldDie,
        action,
        is_self_replacement: true,
        description: "represented death replacement".into(),
    });
}
#[test]
fn single_indestructible_sacrifice_is_not_destruction() {
    let mut state = game(2);
    let id = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state
        .objects
        .get_mut(&id)
        .unwrap()
        .temp_keywords
        .push(KeywordAbility::Indestructible);
    let object = selected(&state, id, 0);
    assert!(destroy_batch(&mut state, &[object.object])
        .unwrap()
        .is_none());
    sacrifice(&mut state, 0, 1);
    assert!(state.players[0].graveyard.contains(&id));
    assert_eq!(
        state.objects[&id].zone_change_count,
        object.object.generation + 1
    );
    assert_eq!(state.pending_triggers.len(), 2);
    assert!(state.pending_triggers.iter().all(|p| {
        let t = &p.context.zone_transition.as_ref().unwrap().subject;
        t.creature_died() && t.kind == MovementKind::Sacrifice { player: 0 }
    }));
}
#[test]
fn count_controls_and_weakest_first_stable_ties() {
    for (count, available) in [(0, 3), (1, 0), (1, 3), (2, 3), (3, 3), (5, 3)] {
        let mut state = game(2);
        let ids: Vec<_> = (0..available)
            .map(|_| state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield))
            .collect();
        if ids.len() == 3 {
            layer(&mut state, ids[2], LayerModification::SetPT(1, 2));
        }
        sacrifice(&mut state, 0, count);
        let n = (count as usize).min(ids.len());
        assert_eq!(state.players[0].graveyard.len(), n);
        assert_eq!(state.next_zone_event_group_id, u64::from(n != 0));
        assert_eq!(state.pending_triggers.len(), n * 2);
        if count == 1 && ids.len() == 3 {
            assert_eq!(state.players[0].graveyard, vec![ids[2]]);
        }
        if count == 2 {
            assert_eq!(state.players[0].graveyard, vec![ids[2], ids[0]]);
        }
        if count == 3 {
            assert_eq!(state.players[0].graveyard, vec![ids[2], ids[0], ids[1]]);
        }
    }
}
#[test]
fn represented_replacement_endpoints_and_prevention() {
    for zone in [
        ZoneType::Graveyard,
        ZoneType::Exile,
        ZoneType::Hand,
        ZoneType::Library,
        ZoneType::Command,
        ZoneType::Battlefield,
    ] {
        let mut state = game(2);
        let id = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
        layer(&mut state, id, LayerModification::ChangeController(0));
        replacement(
            &mut state,
            if zone == ZoneType::Battlefield {
                ReplacementAction::Prevent
            } else {
                ReplacementAction::RedirectToZone(zone)
            },
        );
        let intent = selected(&state, id, 0);
        let result = resolved_sacrifice_batch(&mut state, &[intent]).unwrap();
        if zone == ZoneType::Battlefield {
            assert!(result.is_none());
            assert!(state.battlefield.contains(&id));
            assert!(state.pending_triggers.is_empty());
            assert_eq!(state.next_zone_event_group_id, 0);
        } else {
            let batch = result.unwrap();
            assert_eq!(batch.transitions.len(), 1);
            assert_eq!(batch.transitions[0].destination.player, 1);
            assert_eq!(batch.transitions[0].destination.zone, zone);
            assert_eq!(
                batch.transitions[0].creature_died(),
                zone == ZoneType::Graveyard
            );
            assert_eq!(
                state.pending_triggers.len(),
                if zone == ZoneType::Graveyard { 2 } else { 1 }
            );
            let view = state.visible_state(0);
            assert_eq!(
                view.objects.contains_key(&id),
                !matches!(zone, ZoneType::Hand | ZoneType::Library)
            );
        }
    }
}
#[test]
fn token_and_commander_actual_evidence() {
    let mut token_game = game(2);
    let id = token_game.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    token_game.objects.get_mut(&id).unwrap().is_token = true;
    layer(&mut token_game, id, LayerModification::ChangeController(0));
    sacrifice(&mut token_game, 0, 1);
    assert!(!token_game.objects.contains_key(&id));
    assert_eq!(token_game.pending_triggers.len(), 2);
    assert!(token_game.pending_triggers.iter().all(|p| p
        .context
        .zone_transition
        .as_ref()
        .unwrap()
        .subject
        .creature_died()));
    assert_eq!(token_game.pending_events.iter().filter(|e| matches!(e, GameEvent::ZoneChange { object, from: Zone::Battlefield, .. } if *object == id)).count(), 1);
    let mut commander = GameState::new_commander(2);
    commander.card_db = game(2).card_db;
    let id = commander.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    commander.players[0].commander_object_id = Some(id);
    sacrifice(&mut commander, 0, 1);
    assert!(commander.players[0].command_zone.contains(&id));
    assert_eq!(commander.stack.len(), 1);
    let mtg_gto::game::StackSource::TriggeredAbility { context, .. } = &commander.stack[0].source
    else {
        panic!("owned LTB trigger");
    };
    let context = context.zone_transition.as_ref().unwrap();
    assert_eq!(context.subject.destination.zone, ZoneType::Command);
    assert!(!context.subject.creature_died()); // Existing 2F compatibility debt.
}
#[test]
fn all_malformed_selected_sets_reject_atomically_including_prevented_members() {
    for case in 0..11 {
        let mut state = game(2);
        let a = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let b = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let mut intents = vec![selected(&state, a, 0), selected(&state, b, 0)];
        match case {
            0 => intents[1].object.generation += 1,
            1 => intents[1] = intents[0],
            2 => {
                state.move_object(b, ZoneType::Battlefield, ZoneType::Hand);
                intents[1] = selected(&state, b, 0);
            }
            3 => layer(&mut state, b, LayerModification::ChangeController(1)),
            4 => layer(
                &mut state,
                b,
                LayerModification::RemoveType(CardType::Creature),
            ),
            5 => state.objects.get_mut(&b).unwrap().owner = 99,
            6 => {
                state.objects.get_mut(&b).unwrap().zone_change_count = u32::MAX;
                intents[1].object.generation = u32::MAX;
            }
            7 => state.next_zone_event_group_id = u64::MAX,
            8 => replacement(
                &mut state,
                ReplacementAction::RedirectToZone(ZoneType::Stack),
            ),
            9 => {
                replacement(&mut state, ReplacementAction::Prevent);
                intents[1].object.generation += 1;
            }
            10 => intents[1].player = 99,
            _ => unreachable!(),
        }
        // Restore malformed exact intents too: no mutable selection handles survive.
        let binary = bincode::serialize(&state).unwrap();
        let mut restored: GameState = bincode::deserialize(&binary).unwrap();
        restored.card_db = state.card_db.clone();
        let before = serde_json::to_value(&restored).unwrap();
        assert!(
            resolved_sacrifice_batch(&mut restored, &intents).is_err(),
            "case {case}"
        );
        assert_eq!(
            serde_json::to_value(&restored).unwrap(),
            before,
            "case {case}"
        );
    }
}
#[test]
fn noncreature_is_not_a_resolved_creature_sacrifice() {
    let mut state = game(2);
    let id = state.create_card_in_zone(ARTIFACT, 0, ZoneType::Battlefield);
    let intent = selected(&state, id, 0);
    assert_eq!(
        resolved_sacrifice_batch(&mut state, &[intent]).unwrap_err(),
        TransitionError::InvalidSacrificeSubject(id)
    );
    assert!(state.battlefield.contains(&id));
}
#[test]
fn successive_batches_do_not_reuse_departed_watchers() {
    let mut state = game(2);
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let a = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let b = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let first = vec![selected(&state, watcher, 0), selected(&state, a, 0)];
    let mut simultaneous = state.clone();
    let all = vec![
        selected(&state, watcher, 0),
        selected(&state, a, 0),
        selected(&state, b, 0),
    ];
    resolved_sacrifice_batch(&mut simultaneous, &all).unwrap();
    resolved_sacrifice_batch(&mut state, &first).unwrap();
    let second = selected(&state, b, 0);
    resolved_sacrifice_batch(&mut state, &[second]).unwrap();
    assert_eq!(state.next_zone_event_group_id, 2);
    assert_eq!(simultaneous.next_zone_event_group_id, 1);
    assert_eq!(
        state
            .pending_triggers
            .iter()
            .filter(|p| p.source_id == watcher)
            .count(),
        6
    );
    assert_eq!(
        simultaneous
            .pending_triggers
            .iter()
            .filter(|p| p.source_id == watcher)
            .count(),
        9
    );
    assert_ne!(
        InformationSet::normalize_retained_view(&state.visible_state(0)).encoding,
        InformationSet::normalize_retained_view(&simultaneous.visible_state(0)).encoding
    );
}
#[test]
fn cost_paths_keep_their_legacy_or_unpaid_behavior() {
    for cost in [
        SacrificeCost::SelfSacrifice,
        SacrificeCost::AnyCreature,
        SacrificeCost::CreatureWithSubtype(Subtype("Rat".into())),
    ] {
        let mut state = game(2);
        let mut db = state.card_db().clone();
        db.insert(CardDef {
            id: 994_104,
            name: "Legacy cost control".into(),
            card_types: vec![CardType::Creature],
            power: Some(2),
            toughness: Some(2),
            activated_abilities: vec![ActivatedAbility {
                cost: ManaCost::zero(),
                requires_tap: false,
                sacrifice_cost: Some(cost.clone()),
                life_cost: 0,
                effect: Effect::GainLife { amount: 1 },
                description: "cost".into(),
            }],
            ..Default::default()
        });
        state.card_db = Some(Arc::new(db));
        let outlet = state.create_card_in_zone(994_104, 0, ZoneType::Battlefield);
        let victim = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        state.phase = mtg_gto::game::Phase::PreCombatMain;
        apply_action(
            &mut state,
            &Action::ActivateAbility {
                object_id: outlet,
                ability_index: 0,
                targets: vec![],
            },
        );
        assert_eq!(state.stack.len(), 1);
        assert_eq!(
            state.battlefield.contains(&outlet),
            cost != SacrificeCost::SelfSacrifice
        );
        assert!(state.battlefield.contains(&victim));
        assert_eq!(state.next_zone_event_group_id, 0);
        assert!(state.pending_triggers.is_empty());
    }
}

#[test]
fn surviving_and_stolen_controller_relative_watchers() {
    let mut state = game(3);
    let watcher = state.create_card_in_zone(WATCHER, 2, ZoneType::Battlefield);
    layer(&mut state, watcher, LayerModification::ChangeController(0));
    layer(&mut state, watcher, LayerModification::SetPT(10, 10));
    let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    layer(&mut state, subject, LayerModification::ChangeController(0));
    sacrifice(&mut state, 0, 1);
    assert!(state.battlefield.contains(&watcher));
    let observed: Vec<_> = state
        .pending_triggers
        .iter()
        .filter(|p| p.source_id == watcher)
        .collect();
    assert_eq!(observed.len(), 3);
    assert!(observed.iter().all(|p| p.controller == 0));
    for occurrence in observed {
        let c = occurrence.context.zone_transition.as_ref().unwrap();
        assert_eq!((c.source_before.owner, c.source_before.controller), (2, 0));
        assert_eq!(
            (c.subject.before.owner, c.subject.before.controller),
            (1, 0)
        );
    }
}

fn finish(state: &mut GameState) {
    for _ in 0..100 {
        if !state.pending_triggers.is_empty() {
            let choice = legal_actions(state)
                .into_iter()
                .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }));
            if let Some(choice) = choice {
                let key = canonicalize(&choice, state);
                let reconstructed = resolve(&key, state, state.priority_player).unwrap();
                assert_eq!(canonicalize(&reconstructed, state), key);
                apply_action(state, &reconstructed);
                continue;
            }
        }
        if state.stack.is_empty() && state.pending_triggers.is_empty() {
            return;
        }
        apply_action(state, &Action::PassPriority);
    }
    panic!("sacrifice continuation did not finish");
}
fn restores(state: &GameState) -> Vec<GameState> {
    let mut restored = state.clone();
    restored.restore(state.snapshot());
    let mut variants = vec![
        state.clone(),
        restored,
        serde_json::from_slice(&serde_json::to_vec(state).unwrap()).unwrap(),
        bincode::deserialize(&bincode::serialize(state).unwrap()).unwrap(),
    ];
    for game in &mut variants {
        game.card_db = state.card_db.clone();
    }
    variants
}
#[test]
fn pending_and_stacked_final_continuations_preserve_purged_stolen_history() {
    let mut state = game(3);
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let token = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    state.objects.get_mut(&token).unwrap().is_token = true;
    layer(&mut state, token, LayerModification::ChangeController(0));
    sacrifice(&mut state, 0, 2);
    assert!(!state.objects.contains_key(&token));
    assert!(!state.battlefield.contains(&watcher));
    assert_eq!(state.pending_triggers.len(), 8);
    let expected_info =
        InformationSet::from_view(&state.visible_state(0), state.card_db()).hash_value();
    let initial_life = state.players[0].life;
    let mut expected = state.clone();
    finish(&mut expected);
    assert_eq!(expected.players[0].life, initial_life + 8);
    assert_eq!(expected.priority_player, expected.active_player);
    for mut game in restores(&state) {
        assert_eq!(
            InformationSet::from_view(&game.visible_state(0), game.card_db()).hash_value(),
            expected_info
        );
        finish(&mut game);
        assert_eq!(
            serde_json::to_value(game).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
    // Also restore after mandatory placement, through the actual final effects.
    let order = legal_actions(&state)
        .into_iter()
        .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    apply_action(&mut state, &order);
    assert!(state.pending_triggers.is_empty());
    assert_eq!(state.stack.len(), 8);
    for mut game in restores(&state) {
        finish(&mut game);
        assert_eq!(
            serde_json::to_value(game).unwrap(),
            serde_json::to_value(&expected).unwrap()
        );
    }
}
#[test]
fn thirteen_occurrence_state_invariance_and_own_fifo_roundtrips() {
    let mut expected = None;
    for offset in [0, 7] {
        for reverse in [false, true] {
            for reverse_pending in [false, true] {
                let mut state = game(2);
                // Actual irrelevant allocation history, then a separate raw-ID shift.
                if offset != 0 {
                    let unused = state.create_card_in_zone(ARTIFACT, 1, ZoneType::Exile);
                    state.objects.remove(&unused);
                    state.players[1].exile.clear();
                }
                state.next_object_id += offset;
                state.next_zone_event_group_id += offset;
                let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
                let a = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
                let b = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
                let ids = [watcher, a, b];
                let mut intents: Vec<_> = ids.iter().map(|&id| selected(&state, id, 0)).collect();
                if reverse {
                    intents.reverse();
                }
                state.drain_events();
                let batch = resolved_sacrifice_batch(&mut state, &intents)
                    .unwrap()
                    .unwrap();
                assert_eq!(batch.transitions.len(), 3);
                assert_eq!(state.pending_triggers.len(), 13);
                assert_eq!(
                    state
                        .pending_events
                        .iter()
                        .filter(|e| matches!(
                            e,
                            GameEvent::ZoneChange {
                                from: Zone::Battlefield,
                                to: Zone::Graveyard,
                                ..
                            }
                        ))
                        .count(),
                    3
                );
                if reverse_pending {
                    state.pending_triggers.reverse();
                }
                let mut relations = Vec::new();
                for occurrence in &state.pending_triggers {
                    let c = occurrence.context.zone_transition.as_ref().unwrap();
                    let source = ids
                        .iter()
                        .position(|&id| id == c.source_before.object.id)
                        .unwrap();
                    let subject = ids
                        .iter()
                        .position(|&id| id == c.subject.before.object.id)
                        .unwrap();
                    assert_eq!(c.group_id, batch.group_id);
                    assert_eq!(occurrence.controller, 0);
                    assert_eq!((c.source_before.owner, c.source_before.controller), (0, 0));
                    assert_eq!(
                        (c.subject.before.owner, c.subject.before.controller),
                        (0, 0)
                    );
                    assert_eq!(c.subject.kind, MovementKind::Sacrifice { player: 0 });
                    assert_eq!(c.subject.destination.zone, ZoneType::Graveyard);
                    assert!(c.subject.creature_died());
                    assert_eq!(c.source_was_subject, source == subject);
                    assert!(batch
                        .transitions
                        .iter()
                        .any(|t| t.before.object == c.subject.before.object));
                    relations.push((source, subject, occurrence.ability_index));
                }
                relations.sort();
                assert_eq!(relations.iter().filter(|r| r.0 == 0).count(), 9);
                assert_eq!(relations.iter().filter(|r| r.0 == 1).count(), 2);
                assert_eq!(relations.iter().filter(|r| r.0 == 2).count(), 2);
                let information =
                    InformationSet::from_view(&state.visible_state(0), state.card_db())
                        .hash_value();
                let encoding =
                    InformationSet::normalize_retained_view(&state.visible_state(0)).encoding;
                let observed = (information, encoding, relations);
                if let Some(ref expected) = expected {
                    assert_eq!(&observed, expected);
                } else {
                    expected = Some(observed);
                }
                // Above six occurrences, the inherited sole FIFO action depends
                // on occurrence storage order. Cross-state ACTION-SET equality
                // is outside this accepted contract and remains 2E debt, not
                // rules-complete trigger ordering. Each own-state choice must
                // still be legal and reconstruct exactly; state/event invariants
                // above remain mandatory for the full 13-occurrence fixture.
                let actions = legal_actions(&state);
                assert_eq!(
                    actions
                        .iter()
                        .filter(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
                        .count(),
                    1
                );
                for action in &actions {
                    let key = canonicalize(action, &state);
                    let reconstructed = resolve(&key, &state, state.priority_player).unwrap();
                    assert!(actions.contains(&reconstructed));
                    assert_eq!(canonicalize(&reconstructed, &state), key);
                }
            }
        }
    }
}

#[test]
fn six_occurrence_cross_state_actions_reconstruct_and_finish_equivalently() {
    let mut baseline: Option<GameState> = None;
    for offset in [0, 7] {
        for reverse_members in [false, true] {
            for reverse_pending in [false, true] {
                let mut state = game(2);
                if offset != 0 {
                    let unused = state.create_card_in_zone(ARTIFACT, 1, ZoneType::Exile);
                    state.objects.remove(&unused);
                    state.players[1].exile.clear();
                }
                state.next_object_id += offset;
                state.next_zone_event_group_id += offset;
                for _ in 0..3 {
                    state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
                }
                let mut intents: Vec<_> = state
                    .battlefield
                    .iter()
                    .map(|&id| selected(&state, id, 0))
                    .collect();
                if reverse_members {
                    intents.reverse();
                }
                let batch = resolved_sacrifice_batch(&mut state, &intents)
                    .unwrap()
                    .unwrap();
                assert_eq!(batch.transitions.len(), 3);
                assert_eq!(state.pending_triggers.len(), 6);
                if reverse_pending {
                    state.pending_triggers.reverse();
                }
                if baseline.is_none() {
                    baseline = Some(state.clone());
                }
                let reference = baseline.as_ref().unwrap();
                assert_eq!(
                    InformationSet::from_view(&state.visible_state(0), state.card_db())
                        .hash_value(),
                    InformationSet::from_view(&reference.visible_state(0), reference.card_db())
                        .hash_value()
                );
                assert_eq!(
                    InformationSet::normalize_retained_view(&state.visible_state(0)).encoding,
                    InformationSet::normalize_retained_view(&reference.visible_state(0)).encoding
                );
                let actions = legal_actions(&state);
                let mut actual_keys: Vec<_> =
                    actions.iter().map(|a| canonicalize(a, &state)).collect();
                actual_keys.sort_by_key(|a| format!("{a:?}"));
                let reference_actions = legal_actions(reference);
                let mut expected_keys: Vec<_> = reference_actions
                    .iter()
                    .map(|a| canonicalize(a, reference))
                    .collect();
                expected_keys.sort_by_key(|a| format!("{a:?}"));
                assert_eq!(actual_keys, expected_keys);
                // Every supported ordering key from A must reconstruct a legal
                // action in B with the same key, not merely round-trip in A.
                for action in reference_actions
                    .iter()
                    .filter(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
                {
                    let key = canonicalize(action, reference);
                    let reconstructed = resolve(&key, &state, state.priority_player).unwrap();
                    assert!(actions.contains(&reconstructed));
                    assert_eq!(canonicalize(&reconstructed, &state), key);
                }
                // A common key also produces equivalent final represented play.
                let ordering = reference_actions
                    .iter()
                    .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
                    .unwrap();
                let key = canonicalize(ordering, reference);
                let mut a = reference.clone();
                let mut b = state.clone();
                let choice = resolve(&key, &b, b.priority_player).unwrap();
                apply_action(&mut a, ordering);
                apply_action(&mut b, &choice);
                finish(&mut a);
                finish(&mut b);
                assert_eq!(
                    a.players.iter().map(|p| p.life).collect::<Vec<_>>(),
                    b.players.iter().map(|p| p.life).collect::<Vec<_>>()
                );
                assert_eq!(a.priority_player, b.priority_player);
                assert_eq!(a.phase, b.phase);
                assert!(a.pending_triggers.is_empty() && b.pending_triggers.is_empty());
                assert!(a.stack.is_empty() && b.stack.is_empty());
                assert_eq!(
                    InformationSet::from_view(&a.visible_state(0), a.card_db()).hash_value(),
                    InformationSet::from_view(&b.visible_state(0), b.card_db()).hash_value()
                );
            }
        }
    }
}
#[test]
fn realistic_sacrifice_normalizer_diagnostic() {
    use mtg_gto::card::sample::{build_sample_db, ids};
    use mtg_gto::rules::transitions::normalize_retained;
    for players in [2, 3, 4] {
        let mut state = game(players);
        let mut db = state.card_db().clone();
        let authored = build_sample_db();
        // Existing authored generic watchers, not new Goblin implementations.
        for card in [
            ids::BLOOD_ARTIST,
            ids::ZULAPORT_CUTTHROAT,
            ids::GRAVE_PACT,
            ids::DICTATE_OF_EREBOS,
        ] {
            db.insert(authored.get(card).unwrap().clone());
        }
        state.card_db = Some(Arc::new(db));
        for (i, card) in [
            ids::BLOOD_ARTIST,
            ids::ZULAPORT_CUTTHROAT,
            ids::GRAVE_PACT,
            ids::DICTATE_OF_EREBOS,
        ]
        .into_iter()
        .enumerate()
        {
            state.create_card_in_zone(card, i % players, ZoneType::Battlefield);
        }
        for _ in 0..3 {
            state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
        }
        // One SacrificeCreatures instruction, never EachOpponentSacrifices.
        sacrifice(&mut state, 0, 5);
        let view = state.visible_state(0);
        let normalized = normalize_retained(
            &state.pending_triggers,
            &state.stack,
            |context| {
                let zone = context.zone_transition.as_ref().unwrap();
                let exact = zone.source_before.object;
                let live = state
                    .objects
                    .get(&exact.id)
                    .filter(|i| i.zone_change_count == exact.generation)
                    .and_then(|_| view.zone_live_sources.get(&exact.id));
                zone.source_public_info(live)
            },
            |id| state.objects.get(&id).map(|i| i.zone_change_count),
        );
        println!("sacrifice players={players} sources={} occurrences={} components={:?} ties={:?} nodes={} candidates={} ns={}",
            normalized.source_ranks.len(), state.pending_triggers.len(), normalized.stats.component_sizes,
            normalized.stats.tied_cell_sizes, normalized.stats.search_nodes, normalized.stats.encoded_candidates, normalized.stats.elapsed_nanos);
        assert!(!normalized.encoding.is_empty());
    }
}

#[test]
fn two_resolved_instructions_are_distinct_from_one_two_victim_instruction() {
    let mut one = game(2);
    for _ in 0..2 {
        one.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    }
    let mut two = one.clone();
    sacrifice(&mut one, 0, 2);
    run_effect(
        &mut two,
        0,
        Effect::Multiple(vec![
            Effect::SacrificeCreatures {
                count: 1,
                target: TargetSpec::AnyPlayer,
            },
            Effect::SacrificeCreatures {
                count: 1,
                target: TargetSpec::AnyPlayer,
            },
        ]),
    );
    assert_eq!(one.next_zone_event_group_id, 1);
    assert_eq!(two.next_zone_event_group_id, 2);
    assert_eq!(one.pending_triggers.len(), 4);
    assert_eq!(two.pending_triggers.len(), 4);
    assert_ne!(
        InformationSet::normalize_retained_view(&one.visible_state(0)).encoding,
        InformationSet::normalize_retained_view(&two.visible_state(0)).encoding
    );
}
#[test]
fn explicit_sacrifice_then_common_sba_has_separate_context() {
    use mtg_gto::layers::StaticAbility;
    let mut state = game(2);
    let mut db = state.card_db().clone();
    db.insert(CardDef {
        id: 994_105,
        name: "Sacrificed anthem".into(),
        card_types: vec![CardType::Creature],
        power: Some(0),
        toughness: Some(2),
        static_abilities: vec![StaticAbility::Anthem {
            power: 0,
            toughness: 2,
            affected: AffectedObjects::AllCreatures,
        }],
        ..Default::default()
    });
    state.card_db = Some(Arc::new(db));
    let anthem = state.create_card_in_zone(994_105, 0, ZoneType::Battlefield);
    let subject = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let watcher = state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
    state.objects.get_mut(&subject).unwrap().damage_marked = 2;
    state.refresh_continuous_effects();
    assert_eq!(state.effective_toughness(subject), 4);
    sacrifice(&mut state, 0, 1);
    assert!(state.players[0].graveyard.contains(&anthem));
    assert!(state.players[0].graveyard.contains(&subject));
    let observed: Vec<_> = state
        .pending_triggers
        .iter()
        .filter(|p| p.source_id == watcher)
        .collect();
    assert_eq!(observed.len(), 4); // two printed watcher abilities for each event
    assert!(observed.iter().all(|p| p.context.zone_transition.is_some()));
    let contexts: Vec<_> = observed.iter().map(|p| p.context.zone_transition.as_ref().unwrap()).collect();
    let first: Vec<_> = contexts.iter().filter(|c| c.subject.before.object.id == anthem).collect();
    let later: Vec<_> = contexts.iter().filter(|c| c.subject.before.object.id == subject).collect();
    assert_eq!(first.len(), 2);
    assert_eq!(later.len(), 2);
    assert_ne!(first[0].group_id, later[0].group_id);
    assert!(first.iter().all(|c| c.group_id == first[0].group_id));
    assert!(later.iter().all(|c| c.group_id == later[0].group_id));
    assert!(contexts.iter().all(|c| c.subject.creature_died()));
    assert_eq!(state.next_zone_event_group_id, 2);
}

#[test]
fn noncreature_departure_negative_control_and_raw_sacrifice_rejection() {
    use mtg_gto::rules::transitions::{transition_batch, TransitionRequest};
    let mut state = game(2);
    let id = state.create_card_in_zone(ARTIFACT, 0, ZoneType::Battlefield);
    let object = selected(&state, id, 0).object;
    let mut request = TransitionRequest {
        object,
        from: ZoneType::Battlefield,
        to: ZoneType::Graveyard,
        kind: MovementKind::Sacrifice { player: 0 },
    };
    assert_eq!(
        transition_batch(&mut state, &[request]).unwrap_err(),
        TransitionError::UnsupportedPath
    );
    request.kind = MovementKind::Put;
    let batch = transition_batch(&mut state, &[request]).unwrap();
    assert!(batch.transitions[0].left_battlefield());
    assert!(!batch.transitions[0].creature_died());
    assert_eq!(state.pending_triggers.len(), 1);
}
#[test]
fn linked_followup_validation_precedes_sacrifice_and_followup_is_separate() {
    for malformed in [true, false] {
        let mut state = game(2);
        let a = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let b = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let linked = state.create_card_in_zone(ARTIFACT, 1, ZoneType::Exile);
        state.objects.get_mut(&linked).unwrap().exiled_by = Some(b);
        if malformed {
            state.objects.get_mut(&linked).unwrap().zone_change_count = u32::MAX;
        }
        let intents = [selected(&state, a, 0), selected(&state, b, 0)];
        let before = serde_json::to_value(&state).unwrap();
        let result = resolved_sacrifice_batch(&mut state, &intents);
        if malformed {
            assert!(result.is_err());
            assert_eq!(serde_json::to_value(&state).unwrap(), before);
        } else {
            assert_eq!(result.unwrap().unwrap().transitions.len(), 2);
            assert!(state.players[1].graveyard.contains(&linked));
            assert_eq!(state.pending_triggers.len(), 4);
        }
    }
}

#[test]
fn production_successive_instructions_exclude_earlier_departed_watcher() {
    let mut state = game(2);
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    layer(&mut state, watcher, LayerModification::SetPT(0, 2));
    let victim = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    run_effect(
        &mut state,
        0,
        Effect::Multiple(vec![
            Effect::SacrificeCreatures {
                count: 1,
                target: TargetSpec::AnyPlayer,
            },
            Effect::SacrificeCreatures {
                count: 1,
                target: TargetSpec::AnyPlayer,
            },
        ]),
    );
    assert!(state.players[0].graveyard.contains(&victim));
    let observations: Vec<_> = state
        .pending_triggers
        .iter()
        .filter(|p| p.source_id == watcher)
        .map(|p| p.context.zone_transition.as_ref().unwrap())
        .collect();
    assert_eq!(observations.len(), 3);
    assert!(observations
        .iter()
        .all(|c| c.subject.before.object.id == watcher));
    assert_eq!(state.next_zone_event_group_id, 2);
}

#[test]
fn prevented_selected_intent_still_rejects_invalid_linked_generation() {
    let mut state = game(2);
    let victim = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.objects.get_mut(&victim).unwrap().is_token = true;
    let linked = state.create_card_in_zone(ARTIFACT, 1, ZoneType::Exile);
    state.objects.get_mut(&linked).unwrap().exiled_by = Some(victim);
    state.objects.get_mut(&linked).unwrap().zone_change_count = u32::MAX;
    replacement(&mut state, ReplacementAction::Prevent);
    let intent = selected(&state, victim, 0);
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(
        resolved_sacrifice_batch(&mut state, &[intent]).unwrap_err(),
        TransitionError::InvalidLinkedState(linked)
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
    assert!(state.objects.contains_key(&victim));
    assert!(state.players[1].exile.contains(&linked));
}

#[test]
fn selected_intent_linked_preflight_crosses_validity_and_prevention() {
    for invalid in [false, true] {
        for prevented in [false, true] {
            let mut state = game(2);
            let victim = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
            let linked = state.create_card_in_zone(ARTIFACT, 1, ZoneType::Exile);
            state.objects.get_mut(&linked).unwrap().exiled_by = Some(victim);
            if invalid {
                state.objects.get_mut(&linked).unwrap().zone_change_count = u32::MAX;
            }
            if prevented {
                replacement(&mut state, ReplacementAction::Prevent);
            }
            let intent = selected(&state, victim, 0);
            let before = serde_json::to_value(&state).unwrap();
            let result = resolved_sacrifice_batch(&mut state, &[intent]);
            if invalid {
                assert_eq!(
                    result.unwrap_err(),
                    TransitionError::InvalidLinkedState(linked)
                );
                assert_eq!(serde_json::to_value(&state).unwrap(), before);
            } else if prevented {
                assert!(result.unwrap().is_none());
                assert_eq!(serde_json::to_value(&state).unwrap(), before);
            } else {
                assert_eq!(result.unwrap().unwrap().transitions.len(), 1);
                assert!(state.players[0].graveyard.contains(&victim));
                assert!(state.players[1].graveyard.contains(&linked));
                assert_eq!(state.pending_triggers.len(), 2);
            }
        }
    }
}

#[test]
fn selected_intent_preflight_rejects_entire_prevented_multivictim_set() {
    for reverse in [false, true] {
        let mut state = game(2);
        let a = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        let b = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        state.objects.get_mut(&b).unwrap().is_token = true;
        let linked = state.create_card_in_zone(ARTIFACT, 1, ZoneType::Exile);
        state.objects.get_mut(&linked).unwrap().exiled_by = Some(b);
        state.objects.get_mut(&linked).unwrap().zone_change_count = u32::MAX;
        // The existing represented WouldDie subset applies this prevention to
        // both selected subjects; no new per-object replacement matching.
        replacement(&mut state, ReplacementAction::Prevent);
        let mut intents = [selected(&state, a, 0), selected(&state, b, 0)];
        if reverse {
            intents.reverse();
        }
        let before = serde_json::to_value(&state).unwrap();
        assert_eq!(
            resolved_sacrifice_batch(&mut state, &intents).unwrap_err(),
            TransitionError::InvalidLinkedState(linked)
        );
        assert_eq!(serde_json::to_value(&state).unwrap(), before);
        assert!(state.battlefield.contains(&a) && state.battlefield.contains(&b));
    }
}

#[test]
fn prevented_selected_intent_still_rejects_exhausted_event_group_counter() {
    let mut state = game(2);
    let victim = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.next_zone_event_group_id = u64::MAX;
    replacement(&mut state, ReplacementAction::Prevent);
    let intent = selected(&state, victim, 0);
    let before = serde_json::to_value(&state).unwrap();
    assert_eq!(
        resolved_sacrifice_batch(&mut state, &[intent]).unwrap_err(),
        TransitionError::GroupIdExhausted
    );
    assert_eq!(serde_json::to_value(&state).unwrap(), before);
}

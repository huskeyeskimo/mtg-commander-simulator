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
fn each(state: &mut GameState, controller: usize, count: u32) {
    run_effect(state, controller, Effect::EachOpponentSacrifices { count });
}
fn run_effect(state: &mut GameState, player: usize, effect: Effect) {
    run_targeted_effect(state, player, player, effect);
}
fn run_targeted_effect(state: &mut GameState, player: usize, target: usize, effect: Effect) {
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
    state.priority_player = player;
    let source = state.create_card_in_zone(source_id, player, ZoneType::Battlefield);
    apply_action(
        state,
        &Action::ActivateAbility {
            object_id: source,
            ability_index: 0,
            targets: vec![Target::Player(target)],
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
use mtg_gto::action::{
    canonical::{canonicalize, resolve},
    legal_actions,
};
use mtg_gto::card::KeywordAbility;
use mtg_gto::events::{GameEvent, Zone};
use mtg_gto::info_set::InformationSet;
use mtg_gto::replacement::{ReplacementAction, ReplacementEffect, ReplacementEventKind};
use mtg_gto::rules::transitions::{
    resolved_sacrifice_batch, ExactObjectRef, MovementKind, ResolvedSacrificeSubject,
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
fn finish(state: &mut GameState) {
    for _ in 0..400 {
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
fn each_opponent_seat_rotations_counts_and_lost_players() {
    for players in [3, 4] {
        for active in 0..players {
            for controller in 0..players {
                for count in [0, 1, 2, 5] {
                    let mut state = game(players);
                    state.active_player = active;
                    let ids: Vec<Vec<_>> = (0..players)
                        .map(|p| {
                            (0..p)
                                .map(|_| {
                                    state.create_card_in_zone(CREATURE, p, ZoneType::Battlefield)
                                })
                                .collect()
                        })
                        .collect();
                    let lost = (controller + 2) % players;
                    state.players[lost].has_lost = true;
                    each(&mut state, controller, count);
                    let mut expected = Vec::new();
                    for offset in 0..players {
                        let p = (active + offset) % players;
                        let n = if p == controller || p == lost {
                            0
                        } else {
                            (count as usize).min(ids[p].len())
                        };
                        assert_eq!(state.players[p].graveyard, ids[p][..n]);
                        expected.extend(ids[p][..n].iter().copied());
                    }
                    let actual: Vec<_> = state
                        .pending_events
                        .iter()
                        .filter_map(|e| match e {
                            GameEvent::ZoneChange {
                                object,
                                from: Zone::Battlefield,
                                to: Zone::Graveyard,
                            } => Some(*object),
                            _ => None,
                        })
                        .collect();
                    assert_eq!(
                        actual, expected,
                        "players={players} active={active} controller={controller} count={count}"
                    );
                    assert_eq!(
                        state.next_zone_event_group_id,
                        u64::from(!expected.is_empty())
                    );
                    for occurrence in state.pending_triggers.iter().chain(std::iter::empty()) {
                        let c = occurrence.context.zone_transition.as_ref().unwrap();
                        assert_eq!(
                            c.subject.kind,
                            MovementKind::Sacrifice {
                                player: c.subject.before.controller
                            }
                        );
                        assert_eq!(c.group_id, 0);
                    }
                }
            }
        }
    }
}
#[test]
fn each_effective_selection_owner_destinations_and_stable_ties() {
    let mut state = game(4);
    state.active_player = 3;
    let own = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    let stolen = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    layer(&mut state, stolen, LayerModification::ChangeController(1));
    layer(&mut state, stolen, LayerModification::SetPT(0, 2));
    state
        .objects
        .get_mut(&stolen)
        .unwrap()
        .temp_keywords
        .push(KeywordAbility::Indestructible);
    let given = state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
    layer(&mut state, given, LayerModification::ChangeController(0));
    let animated = state.create_card_in_zone(ARTIFACT, 2, ZoneType::Battlefield);
    layer(
        &mut state,
        animated,
        LayerModification::AddType(CardType::Creature),
    );
    let deanimated = state.create_card_in_zone(CREATURE, 3, ZoneType::Battlefield);
    layer(
        &mut state,
        deanimated,
        LayerModification::RemoveType(CardType::Creature),
    );
    let tie_a = state.create_card_in_zone(CREATURE, 3, ZoneType::Battlefield);
    let tie_b = state.create_card_in_zone(CREATURE, 3, ZoneType::Battlefield);
    each(&mut state, 0, 1);
    assert_eq!(state.players[0].graveyard, vec![stolen]);
    assert_eq!(state.players[2].graveyard, vec![animated]);
    assert_eq!(state.players[3].graveyard, vec![tie_a]);
    for id in [own, given, deanimated, tie_b] {
        assert!(state.battlefield.contains(&id));
    }
    for p in &state.pending_triggers {
        let c = p.context.zone_transition.as_ref().unwrap();
        assert_eq!(c.group_id, 0);
        assert_eq!(c.subject.destination.player, c.subject.before.owner);
        assert_eq!(
            c.subject.kind,
            MovementKind::Sacrifice {
                player: c.subject.before.controller
            }
        );
    }
}

#[test]
fn each_common_prestate_lord_selects_before_any_movement() {
    use mtg_gto::layers::StaticAbility;
    let mut state = game(3);
    let mut db = state.card_db().clone();
    db.insert(CardDef {
        id: 994_105,
        name: "Opponent anthem".into(),
        card_types: vec![CardType::Creature],
        power: Some(0),
        toughness: Some(2),
        static_abilities: vec![StaticAbility::Anthem {
            power: 5,
            toughness: 0,
            affected: AffectedObjects::CreaturesControlledBy(1),
        }],
        ..Default::default()
    });
    state.card_db = Some(Arc::new(db));
    let lord = state.create_card_in_zone(994_105, 1, ZoneType::Battlefield);
    // The lord's control layer disappears when its source departs. Both the
    // controller and power of this candidate must be read before that happens.
    let a = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    let b = state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
    let timestamp = state.new_timestamp();
    state.continuous_effects.push(ContinuousEffect {
        source_id: lord,
        controller: 1,
        timestamp,
        duration: Duration::WhileSourceOnBattlefield,
        affected: AffectedObjects::SpecificIncarnation {
            object_id: a,
            zone_change_count: state.objects[&a].zone_change_count,
        },
        modification: LayerModification::ChangeController(2),
    });
    layer(&mut state, b, LayerModification::SetPT(3, 2));
    state.refresh_continuous_effects();
    each(&mut state, 0, 1);
    assert!(state.players[1].graveyard.contains(&lord));
    assert!(state.players[1].graveyard.contains(&a));
    assert!(state.battlefield.contains(&b));
    let c = state
        .pending_triggers
        .iter()
        .find_map(|p| {
            p.context
                .zone_transition
                .as_ref()
                .filter(|c| c.subject.before.object.id == a)
        })
        .unwrap();
    assert_eq!(c.subject.before.controller, 2);
    assert_eq!(c.subject.kind, MovementKind::Sacrifice { player: 2 });
}

#[test]
fn each_replacements_prevention_tokens_and_commanders() {
    for zone in [
        ZoneType::Graveyard,
        ZoneType::Exile,
        ZoneType::Hand,
        ZoneType::Library,
        ZoneType::Command,
        ZoneType::Battlefield,
    ] {
        let mut state = game(3);
        let a = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
        let b = state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
        replacement(
            &mut state,
            if zone == ZoneType::Battlefield {
                ReplacementAction::Prevent
            } else {
                ReplacementAction::RedirectToZone(zone)
            },
        );
        each(&mut state, 0, 1);
        assert_eq!(
            state.next_zone_event_group_id,
            u64::from(zone != ZoneType::Battlefield)
        );
        assert_eq!(
            state.pending_triggers.len() + state.stack.len(),
            if zone == ZoneType::Battlefield {
                0
            } else if zone == ZoneType::Graveyard {
                4
            } else {
                2
            }
        );
        for (player, id) in [(1, a), (2, b)] {
            let destination = match zone {
                ZoneType::Graveyard => &state.players[player].graveyard,
                ZoneType::Exile => &state.players[player].exile,
                ZoneType::Hand => &state.players[player].hand,
                ZoneType::Library => &state.players[player].library,
                ZoneType::Command => &state.players[player].command_zone,
                ZoneType::Battlefield => &state.battlefield,
                ZoneType::Stack => unreachable!(),
            };
            assert!(destination.contains(&id));
        }
        for p in &state.pending_triggers {
            let c = p.context.zone_transition.as_ref().unwrap();
            assert_eq!(c.subject.destination.zone, zone);
            assert_eq!(c.subject.creature_died(), zone == ZoneType::Graveyard);
        }
    }
    let mut state = game(4);
    let tokens: Vec<_> = (1..4)
        .map(|p| {
            let id = state.create_card_in_zone(CREATURE, p, ZoneType::Battlefield);
            state.objects.get_mut(&id).unwrap().is_token = true;
            id
        })
        .collect();
    layer(
        &mut state,
        tokens[0],
        LayerModification::ChangeController(2),
    );
    layer(
        &mut state,
        tokens[1],
        LayerModification::ChangeController(1),
    );
    state.drain_events();
    each(&mut state, 0, 1);
    assert_eq!(state.pending_triggers.len(), 6);
    for id in tokens {
        assert!(!state.objects.contains_key(&id));
        assert_eq!(state.pending_events.iter().filter(|e| matches!(e, GameEvent::ZoneChange {object, from: Zone::Battlefield, to: Zone::Graveyard} if *object == id)).count(), 1);
    }
    let mut state = GameState::new_commander(3);
    state.card_db = game(3).card_db;
    let ids: Vec<_> = (1..3)
        .map(|p| {
            let id = state.create_card_in_zone(CREATURE, p, ZoneType::Battlefield);
            state.players[p].commander_object_id = Some(id);
            id
        })
        .collect();
    each(&mut state, 0, 1);
    assert_eq!(state.next_zone_event_group_id, 1);
    assert!(state.pending_triggers.is_empty());
    assert_eq!(state.stack.len(), 2);
    for (p, id) in (1..3).zip(ids) {
        assert!(state.players[p].command_zone.contains(&id));
    }
    for item in &state.stack {
        let mtg_gto::game::StackSource::TriggeredAbility { context, .. } = &item.source else {
            panic!("expected trigger")
        };
        let c = context.zone_transition.as_ref().unwrap();
        assert_eq!(c.subject.destination.zone, ZoneType::Command);
        assert!(!c.subject.creature_died());
    }
}

#[test]
fn each_departing_cross_player_and_stolen_observers_common_group() {
    let mut state = game(4);
    state.active_player = 3;
    let watcher = state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
    let survivor = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let stolen_watcher = state.create_card_in_zone(WATCHER, 3, ZoneType::Battlefield);
    layer(
        &mut state,
        stolen_watcher,
        LayerModification::ChangeController(2),
    );
    layer(&mut state, stolen_watcher, LayerModification::SetPT(10, 10));
    let a = state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
    let b = state.create_card_in_zone(CREATURE, 3, ZoneType::Battlefield);
    each(&mut state, 0, 1);
    assert_eq!(state.pending_triggers.len(), 24);
    for (id, n, controller) in [
        (watcher, 7, 1),
        (survivor, 6, 0),
        (stolen_watcher, 7, 2),
        (a, 2, 2),
        (b, 2, 3),
    ] {
        let occurrences: Vec<_> = state
            .pending_triggers
            .iter()
            .filter(|p| p.source_id == id)
            .collect();
        assert_eq!(occurrences.len(), n);
        assert!(occurrences.iter().all(|p| p.controller == controller));
    }
    let own: Vec<_> = state
        .pending_triggers
        .iter()
        .filter(|p| p.source_id == stolen_watcher && p.ability_index == 1)
        .collect();
    assert_eq!(own.len(), 1);
    assert_eq!(
        own[0]
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .subject
            .before
            .object
            .id,
        a
    );
    let contexts: Vec<_> = state
        .pending_triggers
        .iter()
        .map(|p| p.context.zone_transition.as_ref().unwrap())
        .collect();
    assert!(contexts.iter().all(|c| c.group_id == 0));
    assert_eq!(state.priority_player, 3);
    let order = legal_actions(&state)
        .into_iter()
        .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
        .unwrap();
    apply_action(&mut state, &order);
    assert!(state.stack.iter().all(|s| s.controller == 3));
    assert_eq!(state.priority_player, 0);
    finish(&mut state);
    assert_eq!(state.priority_player, state.active_player);
    assert_eq!(
        state
            .pending_events
            .iter()
            .filter(|e| matches!(e, GameEvent::AbilityTriggered { .. }))
            .count(),
        24
    );
}

#[test]
fn successive_each_and_sacrifice_then_each_keep_separate_groups_and_observers() {
    for first_is_each in [false, true] {
        let mut state = game(3);
        let watcher = state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
        layer(&mut state, watcher, LayerModification::SetPT(0, 2));
        let a = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
        for _ in 0..2 {
            state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
        }
        let first = if first_is_each {
            Effect::EachOpponentSacrifices { count: 1 }
        } else {
            Effect::SacrificeCreatures {
                count: 1,
                target: TargetSpec::AnyPlayer,
            }
        };
        // The targeted first instruction sacrifices player 1, while the effect's
        // controller is 0. The ability target is independent of controller.
        run_targeted_effect(
            &mut state,
            0,
            1,
            Effect::Multiple(vec![first, Effect::EachOpponentSacrifices { count: 1 }]),
        );
        assert!(state.players[1].graveyard.contains(&a));
        assert_eq!(state.next_zone_event_group_id, 2);
        let seen: Vec<_> = state
            .pending_triggers
            .iter()
            .filter(|p| p.source_id == watcher)
            .map(|p| p.context.zone_transition.as_ref().unwrap())
            .collect();
        assert_eq!(seen.len(), if first_is_each { 5 } else { 3 });
        assert!(seen.iter().all(|c| c.group_id == 0));
        assert!(state.pending_triggers.iter().any(|p| p
            .context
            .zone_transition
            .as_ref()
            .unwrap()
            .group_id
            == 1));
    }
}

#[test]
fn each_multiplayer_persistence_every_continuation_checkpoint() {
    let mut state = game(4);
    state.active_player = 3;
    let watcher = state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
    let token = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.objects.get_mut(&token).unwrap().is_token = true;
    layer(&mut state, token, LayerModification::ChangeController(2));
    let other = state.create_card_in_zone(CREATURE, 3, ZoneType::Battlefield);
    // Selection prestate itself also survives every persistence path.
    for mut restored in restores(&state) {
        each(&mut restored, 0, 1);
        assert!(!restored.objects.contains_key(&token));
        assert!(!restored.battlefield.contains(&watcher));
        assert!(restored.players[3].graveyard.contains(&other));
    }
    each(&mut state, 0, 1);
    assert!(!state.objects.contains_key(&token));
    let mut expected = state.clone();
    finish(&mut expected);
    assert_eq!(expected.priority_player, 3);
    // Inspect and restore at event/pending, each mandatory placement stage,
    // stacked, each trigger resolution, and final settled priority.
    for _ in 0..160 {
        let keys: Vec<_> = legal_actions(&state)
            .iter()
            .map(|a| canonicalize(a, &state))
            .collect();
        for mut restored in restores(&state) {
            for p in 0..4 {
                assert_eq!(
                    InformationSet::from_view(&restored.visible_state(p), restored.card_db())
                        .hash_value(),
                    InformationSet::from_view(&state.visible_state(p), state.card_db())
                        .hash_value()
                );
                assert_eq!(
                    InformationSet::normalize_retained_view(&restored.visible_state(p)).encoding,
                    InformationSet::normalize_retained_view(&state.visible_state(p)).encoding
                );
            }
            assert_eq!(
                legal_actions(&restored)
                    .iter()
                    .map(|a| canonicalize(a, &restored))
                    .collect::<Vec<_>>(),
                keys
            );
            finish(&mut restored);
            assert_eq!(
                serde_json::to_value(restored).unwrap(),
                serde_json::to_value(&expected).unwrap()
            );
        }
        if state.stack.is_empty() && state.pending_triggers.is_empty() {
            assert_eq!(
                serde_json::to_value(&state).unwrap(),
                serde_json::to_value(&expected).unwrap()
            );
            return;
        }
        let order = legal_actions(&state)
            .into_iter()
            .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }));
        if let Some(order) = order {
            apply_action(&mut state, &order);
        } else {
            apply_action(&mut state, &Action::PassPriority);
        }
    }
    panic!("continuation checkpoint bound");
}

#[test]
fn each_raw_allocation_pending_and_member_order_canonical_invariance() {
    for rich in [false, true] {
        let mut baseline: Option<GameState> = None;
        for offset in [0, 9] {
            for reverse_members in [false, true] {
                for reverse_pending in [false, true] {
                    let mut state = game(4);
                    state.active_player = 3;
                    if offset != 0 {
                        let unused = state.create_card_in_zone(ARTIFACT, 0, ZoneType::Exile);
                        state.objects.remove(&unused);
                        state.players[0].exile.clear();
                    }
                    state.next_object_id += offset;
                    state.next_zone_event_group_id += offset;
                    let mut subjects = Vec::new();
                    for player in [3, 1, 2] {
                        let id = state.create_card_in_zone(
                            if rich && player == 1 {
                                WATCHER
                            } else {
                                CREATURE
                            },
                            player,
                            ZoneType::Battlefield,
                        );
                        subjects.push(selected(&state, id, player));
                    }
                    if reverse_members {
                        subjects.reverse();
                    }
                    let batch = resolved_sacrifice_batch(&mut state, &subjects)
                        .unwrap()
                        .unwrap();
                    assert_eq!(batch.transitions.len(), 3);
                    assert_eq!(state.pending_triggers.len(), if rich { 11 } else { 6 });
                    if reverse_pending {
                        state.pending_triggers.reverse();
                    }
                    if baseline.is_none() {
                        baseline = Some(state.clone());
                    }
                    let reference = baseline.as_ref().unwrap();
                    for p in 0..4 {
                        assert_eq!(
                            InformationSet::from_view(&state.visible_state(p), state.card_db())
                                .hash_value(),
                            InformationSet::from_view(
                                &reference.visible_state(p),
                                reference.card_db()
                            )
                            .hash_value()
                        );
                        assert_eq!(
                            InformationSet::normalize_retained_view(&state.visible_state(p))
                                .encoding,
                            InformationSet::normalize_retained_view(&reference.visible_state(p))
                                .encoding
                        );
                    }
                    let actual = legal_actions(&state);
                    let expected = legal_actions(reference);
                    let mut actual_keys: Vec<_> =
                        actual.iter().map(|a| canonicalize(a, &state)).collect();
                    let mut expected_keys: Vec<_> = expected
                        .iter()
                        .map(|a| canonicalize(a, reference))
                        .collect();
                    actual_keys.sort_by_key(|k| format!("{k:?}"));
                    expected_keys.sort_by_key(|k| format!("{k:?}"));
                    // The inherited >6 FIFO action is only guaranteed within its
                    // own state. Full cross-state action invariance <=6 remains.
                    if !rich {
                        assert_eq!(actual_keys, expected_keys);
                        for a in expected {
                            let key = canonicalize(&a, reference);
                            let rebuilt = resolve(&key, &state, state.priority_player).unwrap();
                            assert!(actual.contains(&rebuilt));
                            assert_eq!(canonicalize(&rebuilt, &state), key);
                        }
                    } else {
                        for a in &actual {
                            let key = canonicalize(a, &state);
                            let rebuilt = resolve(&key, &state, state.priority_player).unwrap();
                            assert!(actual.contains(&rebuilt));
                            assert_eq!(canonicalize(&rebuilt, &state), key);
                        }
                    }
                    let mut finished = state.clone();
                    finish(&mut finished);
                    let mut reference_finished = reference.clone();
                    finish(&mut reference_finished);
                    assert_eq!(
                        finished.players.iter().map(|p| p.life).collect::<Vec<_>>(),
                        reference_finished
                            .players
                            .iter()
                            .map(|p| p.life)
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }
}

#[test]
fn each_production_raw_id_visibility_and_large_fifo_own_state_invariance() {
    let mut baseline: Option<GameState> = None;
    for offset in [0, 11] {
        for reverse_pending in [false, true] {
            let mut state = game(3);
            state.next_object_id += offset;
            state.next_zone_event_group_id += offset;
            for _ in 0..3 {
                state.create_card_in_zone(WATCHER, 1, ZoneType::Battlefield);
            }
            let token = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
            state.objects.get_mut(&token).unwrap().is_token = true;
            layer(&mut state, token, LayerModification::ChangeController(2));
            each(&mut state, 0, 3);
            if reverse_pending {
                state.pending_triggers.reverse();
            }
            assert_eq!(state.pending_triggers.len(), 35);
            if baseline.is_none() {
                baseline = Some(state.clone());
            }
            let reference = baseline.as_ref().unwrap();
            for p in 0..3 {
                assert_eq!(
                    InformationSet::from_view(&state.visible_state(p), state.card_db())
                        .hash_value(),
                    InformationSet::from_view(&reference.visible_state(p), reference.card_db())
                        .hash_value()
                );
                assert_eq!(
                    InformationSet::normalize_retained_view(&state.visible_state(p)).encoding,
                    InformationSet::normalize_retained_view(&reference.visible_state(p)).encoding
                );
            }
            for a in legal_actions(&state) {
                let key = canonicalize(&a, &state);
                let restored = resolve(&key, &state, state.priority_player).unwrap();
                assert_eq!(a, restored);
                assert_eq!(canonicalize(&restored, &state), key);
            }
        }
    }
}
#[test]
fn realistic_each_opponent_normalizer_diagnostic() {
    use mtg_gto::card::sample::{build_sample_db, ids};
    use mtg_gto::rules::transitions::normalize_retained;
    for players in [3, 4] {
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
        for player in 1..players {
            state.create_card_in_zone(CREATURE, player, ZoneType::Battlefield);
        }
        let token = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
        state.objects.get_mut(&token).unwrap().is_token = true;
        layer(&mut state, token, LayerModification::ChangeController(1));
        state.active_player = players - 1;
        each(&mut state, 0, 2);
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
        println!("each players={players} sources={} occurrences={} components={:?} ties={:?} nodes={} candidates={} ns={}",
            normalized.source_ranks.len(), state.pending_triggers.len(), normalized.stats.component_sizes,
            normalized.stats.tied_cell_sizes, normalized.stats.search_nodes, normalized.stats.encoded_candidates, normalized.stats.elapsed_nanos);
        assert!(!normalized.encoding.is_empty());
    }
}

#[test]
fn each_explicit_sacrifice_then_legacy_sba_has_separate_context() {
    use mtg_gto::layers::StaticAbility;
    let mut state = game(3);
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
    let anthem = state.create_card_in_zone(994_105, 1, ZoneType::Battlefield);
    let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    state.objects.get_mut(&subject).unwrap().damage_marked = 2;
    state.refresh_continuous_effects();
    assert_eq!(state.effective_toughness(subject), 4);
    each(&mut state, 0, 1);
    assert!(state.players[1].graveyard.contains(&anthem));
    assert!(state.players[1].graveyard.contains(&subject));
    let observed: Vec<_> = state
        .pending_triggers
        .iter()
        .filter(|p| p.source_id == watcher)
        .collect();
    assert_eq!(
        observed
            .iter()
            .filter(|p| p.context.zone_transition.is_some())
            .count(),
        2
    );
    assert_eq!(
        observed
            .iter()
            .filter(|p| p.context.zone_transition.is_none())
            .count(),
        1
    );
    assert_eq!(state.next_zone_event_group_id, 1); // SBA stays legacy, not certified simultaneous.
}

#[test]
fn each_six_occurrences_production_cross_state_every_ordering_window() {
    let mut baseline: Option<GameState> = None;
    for offset in [0, 17] {
        for reverse_pending in [false, true] {
            let mut state = game(4);
            state.active_player = 3;
            state.next_object_id += offset;
            state.next_zone_event_group_id += offset;
            state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
            state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
            state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
            each(&mut state, 0, 2);
            assert_eq!(state.pending_triggers.len(), 6);
            if reverse_pending {
                state.pending_triggers.reverse();
            }
            if baseline.is_none() {
                baseline = Some(state.clone());
            }
            let mut reference = baseline.as_ref().unwrap().clone();
            while !state.pending_triggers.is_empty() {
                assert_eq!(state.priority_player, reference.priority_player);
                let actual = legal_actions(&state);
                let expected = legal_actions(&reference);
                let mut akeys: Vec<_> = actual.iter().map(|a| canonicalize(a, &state)).collect();
                let mut ekeys: Vec<_> = expected
                    .iter()
                    .map(|a| canonicalize(a, &reference))
                    .collect();
                akeys.sort_by_key(|k| format!("{k:?}"));
                ekeys.sort_by_key(|k| format!("{k:?}"));
                assert_eq!(akeys, ekeys);
                for a in &expected {
                    let key = canonicalize(a, &reference);
                    let rebuilt = resolve(&key, &state, state.priority_player).unwrap();
                    assert!(actual.contains(&rebuilt));
                    assert_eq!(canonicalize(&rebuilt, &state), key);
                }
                let order = expected
                    .iter()
                    .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. }))
                    .unwrap();
                let key = canonicalize(order, &reference);
                let reconstructed = resolve(&key, &state, state.priority_player).unwrap();
                apply_action(&mut reference, order);
                apply_action(&mut state, &reconstructed);
            }
            finish(&mut state);
            finish(&mut reference);
            for p in 0..4 {
                assert_eq!(state.players[p].life, reference.players[p].life);
                assert_eq!(
                    InformationSet::from_view(&state.visible_state(p), state.card_db())
                        .hash_value(),
                    InformationSet::from_view(&reference.visible_state(p), reference.card_db())
                        .hash_value()
                );
            }
            assert_eq!(state.priority_player, 3);
        }
    }
}

use std::sync::Arc;

use mtg_gto::action::{canonical::{canonicalize, resolve}, legal_actions, Action};
use mtg_gto::card::{ActivatedAbility, CardDef, CardType, Effect, KeywordAbility,
    TargetSpec, TriggerCondition, TriggeredAbility, ZoneType};
use mtg_gto::events::{GameEvent, Zone};
use mtg_gto::game::{CardDatabase, GameState, Phase, Target};
use mtg_gto::info_set::InformationSet;
use mtg_gto::mana::ManaCost;
use mtg_gto::rules::apply_action;

const SOURCE: u64 = 997_100;
const CREATURE: u64 = 997_101;
const ARTIFACT: u64 = 997_102;
const WATCHER: u64 = 997_103;
const DEATH: u64 = 997_104;
const LAND: u64 = 997_105;

fn trigger(trigger: TriggerCondition) -> TriggeredAbility {
    TriggeredAbility { trigger, effect: Effect::GainLife { amount: 1 },
        description: "synthetic observer".into() }
}

fn fixture(effect: Effect, players: usize) -> (GameState, u64) {
    let mut db = CardDatabase::new();
    db.insert(CardDef { id: SOURCE, name: "Departure ability".into(),
        card_types: vec![CardType::Artifact],
        activated_abilities: vec![ActivatedAbility {
            cost: ManaCost::zero(), requires_tap: false, sacrifice_cost: None,
            life_cost: 0, effect, description: "test".into(),
        }], ..Default::default() });
    db.insert(CardDef { id: CREATURE, name: "Subject".into(),
        card_types: vec![CardType::Creature], power: Some(2), toughness: Some(2),
        triggered_abilities: vec![trigger(TriggerCondition::LeavesBattlefield),
            trigger(TriggerCondition::Dies)], ..Default::default() });
    db.insert(CardDef { id: ARTIFACT, name: "Artifact subject".into(),
        card_types: vec![CardType::Artifact],
        triggered_abilities: vec![trigger(TriggerCondition::LeavesBattlefield)],
        ..Default::default() });
    db.insert(CardDef { id: WATCHER, name: "Departure watcher".into(),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![trigger(TriggerCondition::APermanentLeaves)],
        ..Default::default() });
    db.insert(CardDef { id: DEATH, name: "Death watcher".into(),
        card_types: vec![CardType::Enchantment],
        triggered_abilities: vec![trigger(TriggerCondition::ACreatureDies)],
        ..Default::default() });
    db.insert(CardDef { id: LAND, name: "Land".into(), card_types: vec![CardType::Land],
        ..Default::default() });
    let mut state = GameState::new(players);
    state.card_db = Some(Arc::new(db));
    state.phase = Phase::PreCombatMain;
    let source = state.create_card_in_zone(SOURCE, 0, ZoneType::Battlefield);
    (state, source)
}

fn effects() -> [Effect; 3] {
    [Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyPermanent },
        Effect::ShuffleIntoLibrary { target: TargetSpec::AnyPermanent },
        Effect::PutOnBottomOfLibrary { target: TargetSpec::AnyPermanent }]
}

fn activate(state: &mut GameState, source: u64, targets: Vec<Target>) {
    apply_action(state, &Action::ActivateAbility { object_id: source,
        ability_index: 0, targets });
    assert_eq!(state.stack.len(), 1);
}

fn pass_round(state: &mut GameState) {
    for _ in 0..state.players.len() { apply_action(state, &Action::PassPriority); }
}

fn move_subject(state: &mut GameState, source: u64, subject: u64) {
    activate(state, source, vec![Target::Object(subject)]);
    state.drain_events();
    pass_round(state);
}

#[test]
fn direct_departure_ability_rechecks_shroud_at_resolution() {
    for effect in effects() {
        let (mut state, source) = fixture(effect, 2);
        let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
        activate(&mut state, source, vec![Target::Object(subject)]);
        state.objects.get_mut(&subject).unwrap().temp_keywords.push(KeywordAbility::Shroud);
        state.invalidate_characteristics_cache();
        let group = state.next_zone_event_group_id;
        state.drain_events();
        pass_round(&mut state);
        assert!(state.battlefield.contains(&subject));
        assert_eq!(state.next_zone_event_group_id, group);
        assert!(state.pending_triggers.is_empty());
        assert!(state.pending_events.iter().all(|e| !matches!(e, GameEvent::ZoneChange { .. })));
    }
}

#[test]
fn creature_and_noncreature_departures_emit_once_without_death() {
    for (index, effect) in effects().into_iter().enumerate() {
        for card in [CREATURE, ARTIFACT] {
            let (mut state, source) = fixture(effect.clone(), 2);
            let watcher = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
            let death = state.create_card_in_zone(DEATH, 0, ZoneType::Battlefield);
            let subject = state.create_card_in_zone(card, 1, ZoneType::Battlefield);
            state.objects.get_mut(&subject).unwrap().controller = 0;
            let generation = state.objects[&subject].zone_change_count;
            move_subject(&mut state, source, subject);
            let destination = if index == 0 { ZoneType::Hand } else { ZoneType::Library };
            let zone = if index == 0 { &state.players[1].hand } else { &state.players[1].library };
            assert_eq!(zone.iter().filter(|&&id| id == subject).count(), 1);
            assert!(!state.battlefield.contains(&subject));
            let occurrences: Vec<_> = state.pending_triggers.iter().collect();
            assert_eq!(occurrences.len(), 2);
            assert!(occurrences.iter().any(|p| p.source_id == subject));
            assert!(occurrences.iter().any(|p| p.source_id == watcher));
            assert!(occurrences.iter().all(|p| p.source_id != death));
            for occurrence in occurrences {
                let context = occurrence.context.zone_transition.as_ref().unwrap();
                assert_eq!(context.subject.before.object.generation, generation);
                assert_eq!(context.subject.after.generation, generation + 1);
                assert_eq!(context.subject.destination.zone, destination);
                assert!(context.subject.left_battlefield());
                assert!(!context.subject.creature_died());
            }
            assert_eq!(state.pending_events.iter().filter(|event| matches!(event,
                GameEvent::ZoneChange { object, from: Zone::Battlefield, .. }
                    if *object == subject)).count(), 1);
        }
    }
}

#[test]
fn stale_and_blinked_targets_leave_returned_incarnation_and_event_counter_untouched() {
    for effect in effects() {
        for blink in [false, true] {
            let (mut state, source) = fixture(effect.clone(), 2);
            let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
            activate(&mut state, source, vec![Target::Object(subject)]);
            state.move_object(subject, ZoneType::Battlefield, ZoneType::Hand);
            if blink { state.move_object(subject, ZoneType::Hand, ZoneType::Battlefield); }
            let generation = state.objects[&subject].zone_change_count;
            let group = state.next_zone_event_group_id;
            let before = (state.battlefield.clone(), state.players[1].hand.clone(),
                state.players[1].library.clone());
            state.drain_events();
            pass_round(&mut state);
            assert_eq!(state.objects[&subject].zone_change_count, generation);
            assert_eq!(state.next_zone_event_group_id, group);
            assert_eq!((state.battlefield.clone(), state.players[1].hand.clone(),
                state.players[1].library.clone()), before);
            assert!(state.pending_triggers.is_empty());
            assert!(state.pending_events.iter().all(|event| !matches!(event, GameEvent::ZoneChange { .. })));
        }
    }
}

#[test]
fn library_bottom_and_shuffle_preserve_contents_and_placement() {
    let mut randomized = [false; 2];
    for index in [1, 2] {
        for token in [false, true] {
          for _ in 0..4 {
            let (mut state, source) = fixture(effects()[index].clone(), 2);
            let mut before: Vec<_> = (0..24)
                .map(|_| state.create_card_in_zone(LAND, 1, ZoneType::Library)).collect();
            let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
            state.objects.get_mut(&subject).unwrap().is_token = token;
            if !token { before.push(subject); }
            move_subject(&mut state, source, subject);
            let library = &state.players[1].library;
            if index == 2 { assert_eq!(library, &before); }
            else { randomized[usize::from(token)] |= library != &before; }
            let mut actual = library.clone(); actual.sort_unstable();
            before.sort_unstable();
            assert_eq!(actual, before);
            assert_eq!(library.iter().filter(|&&id| id == subject).count(), usize::from(!token));
          }
        }
    }
    // Legacy thread_rng remains in use. Four large-library trials make accidental
    // identity permutations negligible without imposing a new RNG contract.
    assert!(randomized.into_iter().all(|shuffled| shuffled));
}

#[test]
fn rejected_library_target_preserves_order_without_shuffle_or_bottom_postprocessing() {
    for index in [1, 2] {
        let (mut state, source) = fixture(effects()[index].clone(), 2);
        for _ in 0..12 { state.create_card_in_zone(LAND, 1, ZoneType::Library); }
        let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
        activate(&mut state, source, vec![Target::Object(subject)]);
        state.move_object(subject, ZoneType::Battlefield, ZoneType::Hand);
        let order = state.players[1].library.clone();
        let group = state.next_zone_event_group_id;
        pass_round(&mut state);
        assert_eq!(state.players[1].library, order);
        assert_eq!(state.next_zone_event_group_id, group);
    }
}

fn mass_state(players: usize, id_start: u64, reverse: bool) -> (GameState, Vec<u64>, Vec<u64>) {
    let (mut state, source) = fixture(Effect::BounceAllNonlandOpponents, players);
    state.next_object_id = id_start;
    if reverse {
        // Different allocation history, with no surviving extra information.
        for _ in 0..3 {
            let transient = state.create_card_in_zone(LAND, players - 1, ZoneType::Library);
            state.players[players - 1].library.retain(|&id| id != transient);
            state.objects.remove(&transient);
        }
    }
    let watchers: Vec<_> = (0..players)
        .map(|p| state.create_card_in_zone(WATCHER, p, ZoneType::Battlefield)).collect();
    let mut movers = watchers[1..].to_vec();
    for p in 1..players { movers.push(state.create_card_in_zone(CREATURE, p, ZoneType::Battlefield)); }
    movers.push(state.create_card_in_zone(ARTIFACT, 1, ZoneType::Battlefield));
    let stolen = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.objects.get_mut(&stolen).unwrap().controller = 1;
    movers.push(stolen);
    let token = state.create_card_in_zone(CREATURE, players - 1, ZoneType::Battlefield);
    state.objects.get_mut(&token).unwrap().is_token = true;
    movers.push(token);
    for p in 0..players { state.create_card_in_zone(LAND, p, ZoneType::Battlefield); }
    state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    if reverse { state.battlefield.reverse(); }
    activate(&mut state, source, vec![]);
    state.drain_events();
    pass_round(&mut state);
    if reverse { state.pending_triggers.reverse(); }
    (state, watchers, movers)
}

#[test]
fn mass_bounce_uses_common_view_owner_hands_one_group_and_all_controllers() {
    for players in [2, 3, 4] {
        let (state, watchers, movers) = mass_state(players, 1_000_000, false);
        assert!(state.pending_triggers.iter().all(|pending| {
            let context = pending.context.zone_transition.as_ref().unwrap();
            !context.subject.creature_died() && context.subject.destination.zone == ZoneType::Hand
        }));
        let group = state.pending_triggers[0].context.zone_transition.as_ref().unwrap().group_id;
        assert!(state.pending_triggers.iter().all(|p|
            p.context.zone_transition.as_ref().unwrap().group_id == group));
        for watcher in watchers {
            assert_eq!(state.pending_triggers.iter().filter(|p| p.source_id == watcher).count(), movers.len());
        }
        for &id in &movers {
            let context = &state.pending_triggers.iter().find_map(|p| {
                let c = p.context.zone_transition.as_ref().unwrap();
                (c.subject.before.object.id == id).then_some(c)
            }).unwrap().subject;
            if context.before.is_token { assert!(!state.objects.contains_key(&id)); }
            else { assert_eq!(state.players[context.before.owner].hand.iter().filter(|&&x| x == id).count(), 1); }
            assert_eq!(state.pending_events.iter().filter(|e| matches!(e,
                GameEvent::ZoneChange { object, from: Zone::Battlefield, to: Zone::Hand }
                    if *object == id)).count(), 1);
        }
        assert!(state.battlefield.iter().all(|id| !movers.contains(id)));
        assert_eq!(state.battlefield.iter().filter(|&&id|
            state.objects[&id].card_def_id == LAND).count(), players);
    }
}

fn coordinates(state: &GameState, viewer: usize) -> (Vec<u8>, u64, Vec<Vec<u8>>) {
    let view = state.visible_state(viewer);
    let normalized = InformationSet::normalize_retained_view(&view);
    let information = InformationSet::from_view_with_normalization(&view, state.card_db(), &normalized);
    let mut keys: Vec<_> = legal_actions(state).iter()
        .map(|action| bincode::serialize(&canonicalize(action, state)).unwrap()).collect();
    keys.sort();
    (normalized.encoding, information.hash_value(), keys)
}

#[test]
fn mass_member_pending_and_allocation_permutations_preserve_information_and_actions() {
    for players in [2, 3, 4] {
        let (base, _, _) = mass_state(players, 1_000_010, false);
        let (renamed, _, _) = mass_state(players, 2_000_020, true);
        for viewer in 0..players {
            assert_eq!(coordinates(&base, viewer), coordinates(&renamed, viewer));
        }
        for action in legal_actions(&base) {
            let key = canonicalize(&action, &base);
            let reconstructed = resolve(&key, &renamed, renamed.priority_player).unwrap();
            assert_eq!(canonicalize(&reconstructed, &renamed), key);
        }
    }
}

#[test]
fn hidden_hand_and_library_current_identity_is_player_relative() {
    for players in [2, 3, 4] {
        for (index, effect) in effects().into_iter().enumerate() {
            let (mut state, source) = fixture(effect, players);
            state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
            let owner = players - 1;
            let subject = state.create_card_in_zone(CREATURE, owner, ZoneType::Battlefield);
            // Controller before movement must not replace destination ownership.
            state.objects.get_mut(&subject).unwrap().controller = 0;
            move_subject(&mut state, source, subject);
            for viewer in 0..players {
                let view = state.visible_state(viewer);
                let normalized = InformationSet::normalize_retained_view(&view);
                for occurrence in normalized.pending_occurrences.iter().flatten() {
                    let current = occurrence.subject.as_ref().unwrap();
                    assert_eq!(current.same_incarnation_now, index == 0 && viewer == owner);
                    assert_eq!(current.owner, owner);
                    assert_eq!(current.controller_before, 0);
                }
                if index == 0 && viewer == owner { continue; }
                let before = coordinates(&state, viewer);
                let mut changed = state.clone();
                let instance = changed.objects.get_mut(&subject).unwrap();
                instance.card_def_id = LAND;
                instance.zone_change_count += 8;
                instance.damage_marked += 3;
                changed.players[owner].library.reverse();
                changed.players[owner].hand.reverse();
                changed.next_object_id += 9000;
                assert_eq!(coordinates(&changed, viewer), before);
            }
        }
    }
}

fn assert_continuations(state: GameState) {
    let db = state.card_db.clone();
    let mut expected_life: Vec<_> = state.players.iter().map(|p| p.life).collect();
    for pending in &state.pending_triggers { expected_life[pending.controller] += 1; }
    for entry in &state.stack { expected_life[entry.controller] += 1; }
    let snapshot = state.snapshot();
    let json = serde_json::to_vec(&state).unwrap();
    let binary = bincode::serialize(&state).unwrap();
    let cloned = state.clone();
    let mut restored = state.clone();
    restored.pending_triggers.clear();
    restored.restore(snapshot);
    let expected_views: Vec<_> = (0..state.players.len()).map(|viewer| coordinates(&state, viewer)).collect();
    let mut variants = vec![state, cloned, restored,
        serde_json::from_slice::<GameState>(&json).unwrap(),
        bincode::deserialize::<GameState>(&binary).unwrap()];
    let mut expected_final = None;
    for game in &mut variants {
        game.card_db = db.clone();
        for viewer in 0..game.players.len() {
            assert_eq!(coordinates(game, viewer), expected_views[viewer]);
        }
        while let Some(choice) = legal_actions(game).into_iter()
            .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. })) {
            let key = canonicalize(&choice, game);
            let action = resolve(&key, game, game.priority_player).unwrap();
            assert_eq!(canonicalize(&action, game), key);
            apply_action(game, &action);
        }
        assert!(game.pending_triggers.is_empty());
        let controllers: Vec<_> = game.stack.iter().map(|e| e.controller).collect();
        assert!(controllers.windows(2).all(|pair| pair[0] <= pair[1]));
        for _ in 0..100 {
            if game.stack.is_empty() { break; }
            pass_round(game);
        }
        mtg_gto::rules::check_state_based_actions(game);
        assert!(game.stack.is_empty());
        assert!(game.pending_triggers.is_empty());
        assert!(game.trigger_order_resume.is_none());
        assert!(game.pending_copy_order.is_none());
        assert_eq!(game.priority_player, game.active_player);
        assert_eq!(game.players.iter().map(|p| p.life).collect::<Vec<_>>(), expected_life);
        assert!(legal_actions(game).iter().any(|a| matches!(a, Action::PassPriority)));
        let final_state = serde_json::to_value(&*game).unwrap();
        if let Some(ref expected) = expected_final { assert_eq!(&final_state, expected); }
        else { expected_final = Some(final_state); }
    }
}

#[test]
fn private_destinations_and_purged_tokens_finish_identically_after_every_restore() {
    for effect in effects() {
        for token in [false, true] {
            let (mut state, source) = fixture(effect.clone(), 3);
            state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
            let subject = state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
            state.objects.get_mut(&subject).unwrap().is_token = token;
            state.objects.get_mut(&subject).unwrap().controller = 0;
            move_subject(&mut state, source, subject);
            assert_eq!(state.objects.contains_key(&subject), !token);
            assert_eq!(state.pending_triggers.len(), 2);
            assert_eq!(state.pending_events.iter().filter(|e| matches!(e,
                GameEvent::ZoneChange { object, from: Zone::Battlefield, .. }
                    if *object == subject)).count(), 1);
            assert!(state.pending_triggers.iter().all(|p| {
                let c = p.context.zone_transition.as_ref().unwrap();
                c.subject.before.is_token == token && !c.subject.creature_died()
            }));
            assert_continuations(state);
        }
    }
}

#[test]
fn simultaneous_mass_bounce_finishes_apnap_after_every_restore_with_token_absent() {
    let (state, _, movers) = mass_state(3, 3_000_030, false);
    assert!(!state.objects.contains_key(movers.last().unwrap()));
    assert_continuations(state);
}

#[test]
fn compatible_multiple_rechecks_only_at_entry_and_preserves_later_draw() {
    for effect in effects() {
        for protected in [false, true] {
            let (mut state, source) = fixture(Effect::Multiple(vec![effect.clone(),
                Effect::DrawCards { count: 1 }]), 2);
            let drawn = state.create_card_in_zone(LAND, 0, ZoneType::Library);
            let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
            activate(&mut state, source, vec![Target::Object(subject)]);
            if protected {
                state.objects.get_mut(&subject).unwrap().temp_keywords.push(KeywordAbility::Hexproof);
                state.invalidate_characteristics_cache();
            }
            pass_round(&mut state);
            assert_eq!(state.battlefield.contains(&subject), protected);
            assert_eq!(state.players[0].hand.contains(&drawn), !protected);
        }
    }
}

#[test]
fn conditional_modal_and_foreach_departures_preserve_existing_envelope_behavior() {
    use mtg_gto::card::{effects::Condition, DynamicValue};
    for effect in effects() {
        let child = || Effect::Multiple(vec![effect.clone(), Effect::DrawCards { count: 1 }]);
        for envelope in [
            Effect::Conditional { condition: Condition::Always,
                if_true: Box::new(child()), if_false: None },
            Effect::Modal { choices: vec![child()], choose_count: 1 },
            Effect::ForEach { count: DynamicValue::Fixed(1), effect: Box::new(child()) },
        ] {
            let (mut state, source) = fixture(envelope, 2);
            let drawn = state.create_card_in_zone(LAND, 0, ZoneType::Library);
            let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
            move_subject(&mut state, source, subject);
            assert!(!state.battlefield.contains(&subject));
            assert!(state.players[0].hand.contains(&drawn));
        }
    }
}

#[test]
fn creature_contract_rejects_current_noncreature_at_resolution() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    for effect in [
        Effect::BounceTo { zone: ZoneType::Hand, target: TargetSpec::AnyCreature },
        Effect::ShuffleIntoLibrary { target: TargetSpec::AnyCreature },
        Effect::PutOnBottomOfLibrary { target: TargetSpec::AnyCreature },
    ] {
        let (mut state, source) = fixture(effect, 2);
        let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
        activate(&mut state, source, vec![Target::Object(subject)]);
        state.continuous_effects.push(ContinuousEffect {
            source_id: subject, controller: 1, timestamp: 1, duration: Duration::Permanent,
            affected: AffectedObjects::Specific(subject),
            modification: LayerModification::SetTypes(vec![CardType::Artifact]),
        });
        state.invalidate_characteristics_cache();
        pass_round(&mut state);
        assert!(state.battlefield.contains(&subject));
        assert_eq!(state.next_zone_event_group_id, 0);
    }
}

#[test]
fn commander_hand_and_library_compatibility_records_actual_destination() {
    for (index, effect) in effects().into_iter().enumerate() {
        let (mut state, source) = fixture(effect, 2);
        state.format = mtg_gto::game::GameFormat::Commander;
        state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
        let commander = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
        state.players[1].commander_card_id = Some(CREATURE);
        state.players[1].commander_object_id = Some(commander);
        state.objects.get_mut(&commander).unwrap().controller = 0;
        move_subject(&mut state, source, commander);
        assert!(state.is_commander(commander));
        assert!(state.players[1].command_zone.is_empty());
        let to = if index == 0 { ZoneType::Hand } else { ZoneType::Library };
        assert!(state.pending_triggers.iter().all(|p|
            p.context.zone_transition.as_ref().unwrap().subject.destination.zone == to));
        assert_eq!(state.players[1].commander_tax, 0);
        assert_continuations(state);
    }
    let (mut state, source) = fixture(Effect::BounceAllNonlandOpponents, 3);
    state.format = mtg_gto::game::GameFormat::Commander;
    let commander = state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
    state.players[2].commander_object_id = Some(commander);
    activate(&mut state, source, vec![]);
    pass_round(&mut state);
    assert!(state.players[2].hand.contains(&commander));
    assert!(state.players[2].command_zone.is_empty());
}

#[test]
fn migrated_departure_then_common_sba_death_has_separate_contexts_and_endpoints() {
    use mtg_gto::layers::{AffectedObjects, ContinuousEffect, Duration, LayerModification};
    let (mut state, source) = fixture(effects()[0].clone(), 2);
    let departure = state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let death = state.create_card_in_zone(DEATH, 0, ZoneType::Battlefield);
    let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
    state.objects.get_mut(&subject).unwrap().controller = 0;
    let cascade = state.create_card_in_zone(CREATURE, 0, ZoneType::Battlefield);
    state.objects.get_mut(&cascade).unwrap().damage_marked = 2;
    state.continuous_effects.push(ContinuousEffect {
        source_id: subject, controller: 0, timestamp: 1,
        duration: Duration::WhileSourceOnBattlefield,
        affected: AffectedObjects::Specific(cascade),
        modification: LayerModification::ModifyPT(0, 1),
    });
    state.invalidate_characteristics_cache();
    move_subject(&mut state, source, subject);
    assert!(state.players[1].hand.contains(&subject));
    assert!(state.players[0].graveyard.contains(&cascade));
    let migrated: Vec<_> = state.pending_triggers.iter().filter_map(|p|
        p.context.zone_transition.as_ref()).collect();
    assert_eq!(migrated.len(), 6);
    let first: Vec<_> = migrated.iter().filter(|c| c.subject.before.object.id == subject).collect();
    let later: Vec<_> = migrated.iter().filter(|c| c.subject.before.object.id == cascade).collect();
    assert_eq!(first.len(), 2);
    assert_eq!(later.len(), 4); // own leave/dies, departure watcher, death watcher
    assert!(first.iter().all(|c| !c.subject.creature_died() && c.subject.destination.zone == ZoneType::Hand));
    assert!(later.iter().all(|c| c.subject.creature_died()));
    assert_ne!(first[0].group_id, later[0].group_id);
    assert!(later.iter().all(|c| c.group_id == later[0].group_id));
    assert_eq!(state.pending_triggers.iter().filter(|p| p.source_id == death).count(), 1);
    assert_eq!(state.pending_triggers.iter().filter(|p| p.source_id == departure).count(), 2);
    assert!(state.pending_triggers.iter().all(|p| p.context.zone_transition.is_some()));
    assert_eq!(state.next_zone_event_group_id, 2);
}

#[test]
fn mass_bounce_relationship_graph_diagnostics_remain_exact() {
    let (state, _, _) = mass_state(4, 4_000_040, false);
    let view = state.visible_state(0);
    let normalized = InformationSet::normalize_retained_view(&view);
    println!("2B.2b four-player mass bounce: occurrences={}, components={:?}, ties={:?}, search_nodes={}, candidates={}, elapsed_ns={}",
        state.pending_triggers.len(), normalized.stats.component_sizes,
        normalized.stats.tied_cell_sizes, normalized.stats.search_nodes,
        normalized.stats.encoded_candidates, normalized.stats.elapsed_nanos);
    assert_eq!(normalized.pending_occurrences.len(), state.pending_triggers.len());
}

#[test]
fn later_opponent_public_exile_and_death_controls_confirm_current_incarnation() {
    for effect in [Effect::ExileTarget { target: TargetSpec::AnyPermanent },
        Effect::DestroyTarget { target: TargetSpec::AnyPermanent }] {
        let dies = matches!(effect, Effect::DestroyTarget { .. });
        let (mut state, source) = fixture(effect, 3);
        state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
        let subject = state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
        state.objects.get_mut(&subject).unwrap().controller = 0;
        move_subject(&mut state, source, subject);
        for viewer in 0..3 {
            let view = state.visible_state(viewer);
            let normalized = InformationSet::normalize_retained_view(&view);
            assert!(!normalized.pending_occurrences.is_empty());
            for occurrence in normalized.pending_occurrences.iter().flatten() {
                assert!(occurrence.subject.as_ref().unwrap().same_incarnation_now);
            }
        }
        assert!(state.pending_triggers.iter().all(|p|
            p.context.zone_transition.as_ref().unwrap().subject.creature_died() == dies));
    }
}

#[test]
fn owned_targeted_trigger_rechecks_departure_legality_without_rebinding_target() {
    use mtg_gto::game::{StackEntry, StackSource, TriggerContext};
    for effect in effects() {
        for case in 0..3 {
            let (mut state, source) = fixture(effect.clone(), 2);
            let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
            let generation = state.objects[&subject].zone_change_count;
            let id = state.new_stack_id();
            state.stack.push(StackEntry { id, controller: 0,
                source: StackSource::TriggeredAbility { source_id: source, ability_index: 0,
                    context: Box::new(TriggerContext { source_card_id: SOURCE,
                        source_generation: state.objects[&source].zone_change_count,
                        effect: effect.clone(), cast_spell: None, zone_transition: None }) },
                targets: vec![Target::Object(subject)], target_generations: vec![Some(generation)] });
            match case {
                0 => {},
                1 => {
                    state.objects.get_mut(&subject).unwrap().temp_keywords.push(KeywordAbility::Shroud);
                    state.invalidate_characteristics_cache();
                },
                2 => {
                    state.move_object(subject, ZoneType::Battlefield, ZoneType::Hand);
                    state.move_object(subject, ZoneType::Hand, ZoneType::Battlefield);
                },
                _ => unreachable!(),
            }
            let group = state.next_zone_event_group_id;
            pass_round(&mut state);
            assert_eq!(state.battlefield.contains(&subject), case != 0);
            assert_eq!(state.next_zone_event_group_id, group + u64::from(case == 0));
        }
    }
}

#[test]
fn physical_spell_cleanup_precedes_trigger_placement_for_all_four_families() {
    const SPELL: u64 = 997_106;
    let mut all = effects().to_vec();
    all.push(Effect::BounceAllNonlandOpponents);
    for effect in all {
        let mass = matches!(effect, Effect::BounceAllNonlandOpponents);
        let (mut state, _) = fixture(effect.clone(), 2);
        let mut db = state.card_db().clone();
        db.insert(CardDef { id: SPELL, name: "Departure spell".into(),
            card_types: vec![CardType::Instant], mana_cost: Some(ManaCost::zero()),
            spell_effect: Some(effect), ..Default::default() });
        state.card_db = Some(Arc::new(db));
        state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
        let subject = state.create_card_in_zone(CREATURE, 1, ZoneType::Battlefield);
        let spell = state.create_card_in_zone(SPELL, 0, ZoneType::Hand);
        apply_action(&mut state, &Action::CastSpell { object_id: spell,
            targets: if mass { vec![] } else { vec![Target::Object(subject)] } });
        state.drain_events();
        pass_round(&mut state);
        assert!(!state.battlefield.contains(&subject));
        assert_eq!(state.players[0].graveyard.iter().filter(|&&id| id == spell).count(), 1);
        assert_eq!(state.stack.len(), 2);
        let events = &state.pending_events;
        let primary = events.iter().position(|e| matches!(e,
            GameEvent::ZoneChange { object, from: Zone::Battlefield, .. } if *object == subject)).unwrap();
        let cleanup = events.iter().position(|e| matches!(e,
            GameEvent::ZoneChange { object, from: Zone::Stack, to: Zone::Graveyard } if *object == spell)).unwrap();
        let placement = events.iter().position(|e| matches!(e, GameEvent::AbilityTriggered { .. })).unwrap();
        assert!(primary < cleanup && cleanup < placement);
        assert_continuations(state);
    }
}

#[test]
fn every_private_departure_family_preserves_coordinates_after_id_and_history_changes() {
    for players in [2, 3] {
        for effect in effects() {
            let setup = |renamed: bool| {
                let (mut state, source) = fixture(effect.clone(), players);
                state.next_object_id = if renamed { 6_000_060 } else { 5_000_050 };
                if renamed {
                    for _ in 0..3 {
                        let transient = state.create_card_in_zone(LAND, players - 1, ZoneType::Library);
                        state.players[players - 1].library.retain(|&id| id != transient);
                        state.objects.remove(&transient);
                    }
                    state.next_zone_event_group_id = 77;
                }
                state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
                let subject = state.create_card_in_zone(CREATURE, players - 1, ZoneType::Battlefield);
                state.objects.get_mut(&subject).unwrap().controller = 0;
                move_subject(&mut state, source, subject);
                if renamed { state.pending_triggers.reverse(); }
                state
            };
            let base = setup(false);
            let renamed = setup(true);
            for viewer in 0..players { assert_eq!(coordinates(&base, viewer), coordinates(&renamed, viewer)); }
            for action in legal_actions(&base) {
                let key = canonicalize(&action, &base);
                assert_eq!(canonicalize(&resolve(&key, &renamed, 0).unwrap(), &renamed), key);
            }
        }
    }
}

fn departure_source_state(effect: Effect, pending: bool) -> (GameState, u64) {
    let mass = matches!(effect, Effect::BounceAllNonlandOpponents);
    let (mut state, ability) = fixture(effect, 3);
    if pending { state.create_card_in_zone(WATCHER, 2, ZoneType::Battlefield); }
    let source = state.create_card_in_zone(CREATURE, 2, ZoneType::Battlefield);
    state.objects.get_mut(&source).unwrap().plus_counters = 2;
    state.objects.get_mut(&source).unwrap().tapped = true;
    if mass {
        activate(&mut state, ability, vec![]);
        pass_round(&mut state);
    } else { move_subject(&mut state, ability, source); }
    assert_eq!(!state.pending_triggers.is_empty(), pending);
    assert_eq!(!state.stack.is_empty(), !pending);
    (state, source)
}

#[test]
fn pending_opponent_hand_source_exposes_only_owned_history() {
    let (state, source) = departure_source_state(effects()[0].clone(), true);
    let view = state.visible_state(0);
    assert!(!view.objects.contains_key(&source));
    let historical = view.pending_triggers.iter().find(|p| p.source_id == source).unwrap();
    let context = historical.context.zone_transition.as_ref().unwrap();
    assert_eq!(context.source_before.card_id, CREATURE);
    assert_eq!(context.source_before.controller, 2);
    assert_eq!(context.source_before.power, 4);
    assert_eq!(context.source_before.plus_counters, 2);
    assert!(context.source_before.tapped);
    assert_eq!(context.source_before.object.generation, 0);
}

#[test]
fn departed_source_combat_reference_cannot_bypass_current_zone_visibility() {
    let (mut state, source) = source_zone_state(ZoneType::Hand, 3, 13_000_130);
    // A historical participant reference is not current battlefield residence.
    state.phase = Phase::DeclareBlockers;
    state.combat.attackers.push(source);
    assert_source_projection(&state, source, 0, false);
    place_owned_sources(&mut state);
    assert_source_projection(&state, source, 0, false);
    assert_source_projection(&state, source, 2, true);
}

fn source_contexts(state: &GameState, source: u64) -> Vec<&mtg_gto::game::TriggerContext> {
    use mtg_gto::game::StackSource;
    state.pending_triggers.iter().filter(|p| p.source_id == source).map(|p| &p.context)
        .chain(state.stack.iter().filter_map(|entry| match &entry.source {
            StackSource::TriggeredAbility { source_id, context, .. } if *source_id == source => Some(context.as_ref()),
            _ => None,
        })).collect()
}

fn assert_source_projection(state: &GameState, source: u64, viewer: usize, visible: bool) {
    let view = state.visible_state(viewer);
    assert_eq!(view.objects.contains_key(&source), visible, "viewer {viewer}, source {source}");
    if visible {
        assert_eq!(view.objects[&source].zone_change_count, state.objects[&source].zone_change_count);
    }
    let contexts = source_contexts(state, source);
    assert!(!contexts.is_empty());
    for context in contexts {
        let zone = context.zone_transition.as_ref().unwrap();
        assert_eq!(context.source_card_id, CREATURE);
        assert_eq!(context.source_generation, zone.source_before.object.generation);
        assert_eq!(zone.source_before.card_id, CREATURE);
        assert_eq!(zone.source_before.power, 4);
        assert_eq!(zone.source_before.plus_counters, 2);
        assert!(zone.source_before.tapped);
        assert!(zone.source_was_subject);
    }
    let normalized = InformationSet::normalize_retained_view(&view);
    let occurrences: Vec<_> = normalized.pending_occurrences.iter()
        .chain(&normalized.stack_occurrences).flatten()
        .filter(|occurrence| occurrence.source_card_id == CREATURE).collect();
    assert!(!occurrences.is_empty());
    for occurrence in occurrences {
        let subject = occurrence.subject.as_ref().unwrap();
        let after = source_contexts(state, source)[0].zone_transition.as_ref().unwrap().subject.after.generation;
        assert_eq!(subject.same_incarnation_now, visible && state.objects[&source].zone_change_count == after);
        assert!(occurrence.source.as_ref().unwrap().live.is_none(), "old source must not bind to a later incarnation");
    }
}

fn place_owned_sources(state: &mut GameState) {
    let _ = mtg_gto::rules::fire_triggers(state, TriggerCondition::BeginningOfUpkeep, None);
    while let Some(choice) = legal_actions(state).into_iter()
        .find(|a| matches!(a, Action::OrderTriggerOccurrences { .. })) {
        let key = canonicalize(&choice, state);
        let action = resolve(&key, state, state.priority_player).unwrap();
        apply_action(state, &action);
    }
    assert!(state.pending_triggers.is_empty());
    assert!(!state.stack.is_empty());
}

#[test]
fn all_four_departure_families_project_pending_and_stacked_sources_by_current_zone() {
    let mut families = effects().to_vec();
    families.push(Effect::BounceAllNonlandOpponents);
    for (index, effect) in families.into_iter().enumerate() {
        let hand = index == 0 || index == 3;
        for pending in [true, false] {
            let (mut state, source) = departure_source_state(effect.clone(), pending);
            for viewer in 0..3 { assert_source_projection(&state, source, viewer, hand && viewer == 2); }
            if pending {
                place_owned_sources(&mut state);
                for viewer in 0..3 { assert_source_projection(&state, source, viewer, hand && viewer == 2); }
            }
        }
    }
}

fn source_zone_state(zone: ZoneType, players: usize, id_start: u64) -> (GameState, u64) {
    use mtg_gto::rules::transitions::{transition_batch, ExactObjectRef, MovementKind, TransitionRequest};
    let (mut state, _) = fixture(effects()[0].clone(), players);
    state.next_object_id = id_start;
    state.create_card_in_zone(WATCHER, 0, ZoneType::Battlefield);
    let source = state.create_card_in_zone(CREATURE, players - 1, ZoneType::Battlefield);
    let instance = state.objects.get_mut(&source).unwrap();
    instance.controller = 0;
    instance.plus_counters = 2;
    instance.tapped = true;
    transition_batch(&mut state, &[TransitionRequest {
        object: ExactObjectRef { id: source, generation: 0 }, from: ZoneType::Battlefield,
        to: zone, kind: MovementKind::Put,
    }]).unwrap();
    (state, source)
}

#[test]
fn source_visibility_matrix_retains_history_but_requires_current_zone_membership() {
    for zone in [ZoneType::Hand, ZoneType::Library, ZoneType::Graveyard, ZoneType::Exile, ZoneType::Command] {
        let (mut state, source) = source_zone_state(zone, 3, 7_000_070);
        let public = matches!(zone, ZoneType::Graveyard | ZoneType::Exile | ZoneType::Command);
        for stacked in [false, true] {
            if stacked { place_owned_sources(&mut state); }
            for viewer in 0..3 {
                assert_source_projection(&state, source, viewer, public || (zone == ZoneType::Hand && viewer == 2));
            }
        }
    }
    for zone in [ZoneType::Graveyard, ZoneType::Exile, ZoneType::Command] {
        let (mut state, source) = source_zone_state(zone, 4, 8_000_080);
        assert_source_projection(&state, source, 0, true);
        assert_source_projection(&state, source, 3, true);
        place_owned_sources(&mut state);
        assert_source_projection(&state, source, 0, true);
        assert_source_projection(&state, source, 3, true);
    }
}

#[test]
fn stacked_activated_source_reference_does_not_grant_hidden_current_access() {
    use mtg_gto::game::{StackEntry, StackSource};
    for zone in [ZoneType::Hand, ZoneType::Library, ZoneType::Battlefield,
        ZoneType::Graveyard, ZoneType::Exile, ZoneType::Command] {
        let (mut state, _) = fixture(effects()[0].clone(), 3);
        let source = state.create_card_in_zone(SOURCE, 2, zone);
        let id = state.new_stack_id();
        state.stack.push(StackEntry { id, source: StackSource::ActivatedAbility { source_id: source, ability_index: 0 },
            controller: 2, targets: vec![], target_generations: vec![] });
        for viewer in 0..3 {
            let visible = !matches!(zone, ZoneType::Hand | ZoneType::Library) || (zone == ZoneType::Hand && viewer == 2);
            assert_eq!(state.visible_state(viewer).objects.contains_key(&source), visible);
        }
    }
}

#[test]
fn hidden_current_source_mutations_ids_history_and_storage_preserve_visible_semantics() {
    let semantics = |state: &GameState, viewer| {
        let mut values: Vec<_> = state.visible_state(viewer).objects.values().map(|i|
            (i.card_def_id, i.owner, i.controller, i.zone_change_count, i.plus_counters,
                i.damage_marked, i.tapped)).collect();
        values.sort_unstable(); values
    };
    for zone in [ZoneType::Hand, ZoneType::Library] {
        let (base, source) = source_zone_state(zone, 3, 9_000_090);
        let (mut renamed, renamed_source) = source_zone_state(zone, 3, 10_000_100);
        renamed.next_object_id += 70;
        renamed.pending_triggers.reverse();
        let current = renamed.objects.get_mut(&renamed_source).unwrap();
        current.zone_change_count += 17;
        current.card_def_id = LAND;
        current.controller = 1;
        current.plus_counters = 99;
        current.damage_marked = 77;
        current.tapped = false;
        for stacked in [false, true] {
            let mut a = base.clone(); let mut b = renamed.clone();
            if stacked {
                let _ = mtg_gto::rules::fire_triggers(&mut a, TriggerCondition::BeginningOfUpkeep, None);
                let _ = mtg_gto::rules::fire_triggers(&mut b, TriggerCondition::BeginningOfUpkeep, None);
                // Select the same semantic order in both storage layouts.
                // Independently taking raw permutation [0, 1] would choose
                // opposite stack orders after reversing the pending storage.
                let choice = legal_actions(&a).into_iter()
                    .find(|action| matches!(action, Action::OrderTriggerOccurrences { .. })).unwrap();
                let key = canonicalize(&choice, &a);
                for game in [&mut a, &mut b] {
                    let action = resolve(&key, game, game.priority_player).unwrap();
                    apply_action(game, &action);
                    assert!(game.pending_triggers.is_empty());
                }
            }
            for viewer in [0, 1] {
                assert!(!a.visible_state(viewer).objects.contains_key(&source));
                assert!(!b.visible_state(viewer).objects.contains_key(&renamed_source));
                assert_eq!(semantics(&a, viewer), semantics(&b, viewer));
                assert_eq!(coordinates(&a, viewer), coordinates(&b, viewer));
            }
            for action in legal_actions(&a) {
                let key = canonicalize(&action, &a);
                assert_eq!(canonicalize(&resolve(&key, &b, b.priority_player).unwrap(), &b), key);
            }
        }
    }
}

#[test]
fn returned_public_source_never_rebinds_the_old_owned_trigger_incarnation() {
    let (mut state, source) = source_zone_state(ZoneType::Hand, 3, 11_000_110);
    state.move_object(source, ZoneType::Hand, ZoneType::Battlefield);
    assert_eq!(state.objects[&source].zone_change_count, 2);
    assert_source_projection(&state, source, 0, true);
    place_owned_sources(&mut state);
    assert_source_projection(&state, source, 0, true);
}

#[test]
fn restored_hidden_source_stays_hidden_and_owned_card_identity_resolves_to_final_settlement() {
    for zone in [ZoneType::Hand, ZoneType::Library] {
        let (mut base, source) = source_zone_state(zone, 3, 12_000_120);
        // Existing effect uses the owned source definition when that source's
        // original incarnation is gone, even if a hidden current object exists.
        let effect = Effect::Multiple(vec![Effect::GainLife { amount: 1 }, Effect::CreateTokenCopyOfSource]);
        for pending in base.pending_triggers.iter_mut().filter(|p| p.source_id == source) {
            pending.context.effect = effect.clone();
        }
        base.objects.get_mut(&source).unwrap().card_def_id = LAND;
        base.objects.get_mut(&source).unwrap().zone_change_count += 17;
        let db = base.card_db.clone();
        let snapshot = base.snapshot();
        let json = serde_json::to_vec(&base).unwrap();
        let binary = bincode::serialize(&base).unwrap();
        let cloned = base.clone();
        let mut restored = base.clone(); restored.pending_triggers.clear(); restored.restore(snapshot);
        let mut variants = vec![base, cloned, restored, serde_json::from_slice::<GameState>(&json).unwrap(),
            bincode::deserialize::<GameState>(&binary).unwrap()];
        let finish = |game: &mut GameState| {
            for _ in 0..20 { if game.stack.is_empty() { break; } pass_round(game); }
            mtg_gto::rules::check_state_based_actions(game);
            assert!(game.pending_triggers.is_empty() && game.stack.is_empty());
            assert!(game.trigger_order_resume.is_none() && game.pending_copy_order.is_none());
            assert_eq!(game.priority_player, game.active_player);
            assert!(!game.visible_state(0).objects.contains_key(&source));
            assert_eq!(game.players[0].life, 22);
            let copies: Vec<_> = game.battlefield.iter().filter(|&&id|
                game.objects[&id].is_token && game.objects[&id].card_def_id == CREATURE).collect();
            assert_eq!(copies.len(), 1, "owned historical source definition must create the copy");
            serde_json::to_value(&*game).unwrap()
        };
        let mut final_value = None;
        for game in &mut variants {
            game.card_db = db.clone();
            assert_source_projection(game, source, 0, false);
            place_owned_sources(game);
            assert_source_projection(game, source, 0, false);
            if zone == ZoneType::Library { assert_source_projection(game, source, 2, false); }
            let stacked_json = serde_json::to_vec(&*game).unwrap();
            let stacked_binary = bincode::serialize(&*game).unwrap();
            let stacked_snapshot = game.snapshot();
            let mut snapshot_restore = game.clone(); snapshot_restore.stack.clear(); snapshot_restore.restore(stacked_snapshot);
            let mut expected_stack_final = None;
            for mut stacked_restore in [game.clone(), snapshot_restore, serde_json::from_slice::<GameState>(&stacked_json).unwrap(),
                bincode::deserialize::<GameState>(&stacked_binary).unwrap()] {
                stacked_restore.card_db = db.clone();
                assert_source_projection(&stacked_restore, source, 0, false);
                let value = finish(&mut stacked_restore);
                if let Some(ref expected) = expected_stack_final { assert_eq!(&value, expected); }
                else { expected_stack_final = Some(value); }
            }
            let value = finish(game);
            assert_eq!(Some(&value), expected_stack_final.as_ref());
            if let Some(ref expected) = final_value { assert_eq!(&value, expected); }
            else { final_value = Some(value); }
        }
    }
}
